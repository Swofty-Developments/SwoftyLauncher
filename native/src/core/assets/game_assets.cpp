#include "core/assets/game_files.hpp"
#include "core/runtime/files.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <windows.h>

#include <atomic>
#include <chrono>
#include <cstring>

// The game assets library (swofty/game_assets.hpp): the SwoftyCoreGameAssets export over game_files::Files, reading the
// game folder's archives with the overlay archives of the launcher's resource_erps on top.

namespace {

namespace gf = swofty_core::assets::game_files;
namespace erp = swofty_core::apply::content::erpfile;

constexpr unsigned MAX_LOGS = 32;

std::atomic<unsigned> logs{0};

struct FileSource {
    HANDLE h = INVALID_HANDLE_VALUE;
    erp::Source src;
    ~FileSource()
    {
        if (h != INVALID_HANDLE_VALUE)
            CloseHandle(h);
    }
};

std::wstring game_file(const std::string &rel)
{
    std::string p = rel;
    std::replace(p.begin(), p.end(), '/', '\\');
    return runtime::game_root() + L"\\" + runtime::wide(p);
}

std::shared_ptr<erp::Source> open_file(const std::string &rel)
{
    auto fs = std::make_shared<FileSource>();
    fs->h = CreateFileW(game_file(rel).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                        OPEN_EXISTING, FILE_FLAG_RANDOM_ACCESS, nullptr);
    LARGE_INTEGER size{};
    if (fs->h == INVALID_HANDLE_VALUE || !GetFileSizeEx(fs->h, &size) || size.QuadPart < 0)
        return nullptr;
    fs->src.size = uint64_t(size.QuadPart);
    HANDLE h = fs->h;
    uint64_t total = fs->src.size;
    fs->src.read = [h, total](uint64_t off, size_t n, char *out) {
        if (off > total || n > total - off || n > MAXDWORD)
            return false;
        OVERLAPPED at{};
        at.Offset = DWORD(off);
        at.OffsetHigh = DWORD(off >> 32);
        DWORD got = 0;
        return ReadFile(h, out, DWORD(n), &got, &at) && got == n;
    };
    return std::shared_ptr<erp::Source>(fs, &fs->src);
}

void list_erps(const std::wstring &dir, const std::string &rel, std::vector<std::string> &out, unsigned depth)
{
    if (depth > 4 || out.size() > gf::MAX_ARCHIVES)
        return;
    WIN32_FIND_DATAW fd;
    HANDLE h =
        FindFirstFileExW((dir + L"*").c_str(), FindExInfoBasic, &fd, FindExSearchNameMatch, nullptr, FIND_FIRST_EX_LARGE_FETCH);
    if (h == INVALID_HANDLE_VALUE)
        return;
    do {
        std::wstring n = fd.cFileName;
        if (n == L"." || n == L"..")
            continue;
        std::string r = rel + runtime::narrow(n);
        if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)
            list_erps(dir + n + L"\\", r + "/", out, depth + 1);
        else if (r.size() > 4 && runtime::lower(r.substr(r.size() - 4)) == ".erp")
            out.push_back(r);
    } while (FindNextFileW(h, &fd));
    FindClose(h);
}

std::string swofty_base()
{
    std::string root = runtime::narrow(runtime::game_root()), sw = runtime::narrow(runtime::swofty_root());
    if (root.empty() || runtime::lower(sw).rfind(runtime::lower(root), 0) != 0)
        return {};
    std::string base = sw.substr(root.size());
    std::replace(base.begin(), base.end(), '\\', '/');
    while (!base.empty() && base.front() == '/')
        base.erase(base.begin());
    if (!base.empty() && base.back() != '/')
        base += '/';
    return base;
}

std::map<std::string, std::vector<std::string>> overlay_archives()
{
    std::map<std::string, std::vector<std::string>> out;
    std::string base = swofty_base();
    if (base.empty())
        return out;
    for (auto &j : runtime::config()["resource_erps"].a) {
        std::string file = j["file"].str();
        if (file.empty())
            continue;
        std::replace(file.begin(), file.end(), '\\', '/');
        for (auto &x : j["erps"].a)
            if (x.type == json::Value::Str) {
                auto &list = out[runtime::lower(x.s)];
                if (std::find(list.begin(), list.end(), base + file) == list.end())
                    list.push_back(base + file);
            }
    }
    return out;
}

gf::Files &files()
{
    static gf::Files *f = [] {
        gf::Env env;
        auto overlays = std::make_shared<std::map<std::string, std::vector<std::string>>>(overlay_archives());
        auto started = std::make_shared<std::chrono::steady_clock::time_point>();
        env.list = [started](const std::string &folder) {
            std::vector<std::string> out;
            *started = std::chrono::steady_clock::now();
            list_erps(game_file(folder) + L"\\", folder, out, 0);
            std::sort(out.begin(), out.end());
            return out;
        };
        env.indexed = [started](const std::string &folder, size_t archives, size_t uris) {
            double ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - *started).count();
            swofty::log("[game-assets] %s: %zu archive(s), %zu resource(s) indexed in %.0f ms", folder.c_str(), archives, uris,
                        ms);
        };
        env.open = open_file;
        env.overlays = [overlays](const std::string &rel) {
            auto it = overlays->find(runtime::lower(rel));
            return it == overlays->end() ? std::vector<std::string>{} : it->second;
        };
        return new gf::Files(std::move(env));
    }();
    return *f;
}

int refuse(const char *call, const std::string &what, const std::string &why)
{
    if (logs.fetch_add(1) < MAX_LOGS)
        swofty::log("[game-assets] %s %s -> invalid: %s", call, what.c_str(), why.c_str());
    return SWOFTY_ASSET_INVALID;
}

int ga_available() { return runtime::game_root().empty() ? 0 : 1; }

int ga_read(const char *uri, uint32_t fragment, void *buf, size_t cap, size_t *len)
{
    if (!uri || !len)
        return SWOFTY_ASSET_INVALID;
    if (!ga_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    struct Last {
        std::string uri, bytes;
        uint32_t fragment = 0;
    };
    thread_local Last last;
    std::string why;
    if (last.uri != uri || last.fragment != fragment) {
        last = {};
        if (!files().read(uri, fragment, last.bytes, why))
            return refuse("read", uri, why);
        last.uri = uri;
        last.fragment = fragment;
    }
    *len = last.bytes.size();
    if (!buf || cap < last.bytes.size())
        return SWOFTY_TOO_SMALL;
    if (!last.bytes.empty())
        std::memcpy(buf, last.bytes.data(), last.bytes.size());
    last = {};
    return SWOFTY_ASSET_OK;
}

int ga_texture(const char *uri, SwoftyGameTexture *out)
{
    if (!uri || !out)
        return SWOFTY_ASSET_INVALID;
    if (!ga_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    gf::Texture t;
    std::string why;
    if (!files().texture(uri, t, why))
        return refuse("texture", uri, why);
    *out = {t.format, t.width, t.height, t.levels, t.layers};
    return SWOFTY_ASSET_OK;
}

int ga_texture_rgba(const char *uri, uint32_t layer, uint32_t level, void *rgba, size_t cap, uint32_t *width, uint32_t *height)
{
    if (!uri || !rgba || !width || !height)
        return SWOFTY_ASSET_INVALID;
    if (!ga_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    gf::Texture t;
    std::vector<uint8_t> px;
    std::string why;
    if (!files().rgba(uri, layer, level, t, px, why))
        return refuse("texture_rgba", std::string(uri) + " layer " + std::to_string(layer), why);
    *width = std::max(1u, t.width >> level);
    *height = std::max(1u, t.height >> level);
    if (cap < px.size())
        return SWOFTY_TOO_SMALL;
    std::memcpy(rgba, px.data(), px.size());
    return SWOFTY_ASSET_OK;
}

int copy_out(const std::string &s, char *out, size_t cap, size_t *len)
{
    if (len)
        *len = s.size();
    if (!out || cap <= s.size())
        return SWOFTY_TOO_SMALL;
    std::memcpy(out, s.data(), s.size());
    out[s.size()] = 0;
    return SWOFTY_ASSET_OK;
}

int ga_catalogue_find(const char *catalogue, const char *name, char *texture_uri, size_t cap, size_t *len, uint32_t *layer)
{
    if (!catalogue || !name || !layer)
        return SWOFTY_ASSET_INVALID;
    if (!ga_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    gf::Catalogue c;
    std::string why;
    if (!files().catalogue(catalogue, c, why))
        return refuse("catalogue_find", catalogue, why);
    auto it = std::find(c.names.begin(), c.names.end(), std::string(name));
    if (!*name || it == c.names.end())
        return refuse("catalogue_find", std::string(catalogue) + " " + name, "the catalogue has no entry of that name");
    *layer = uint32_t(it - c.names.begin());
    return copy_out(c.texture, texture_uri, cap, len);
}

int ga_catalogue_name(const char *catalogue, uint32_t index, char *name, size_t cap, size_t *len)
{
    if (!catalogue)
        return SWOFTY_ASSET_INVALID;
    if (!ga_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    gf::Catalogue c;
    std::string why;
    if (!files().catalogue(catalogue, c, why))
        return refuse("catalogue_name", catalogue, why);
    if (index >= c.names.size())
        return SWOFTY_ASSET_LATE;
    return copy_out(c.names[index], name, cap, len);
}

const SwoftyGameAssets table = {SWOFTY_GAME_ASSETS_VERSION, ga_available,     ga_read, ga_texture, ga_texture_rgba,
                                ga_catalogue_find,          ga_catalogue_name};

}

extern "C" __declspec(dllexport) const SwoftyGameAssets *SwoftyCoreGameAssets(void) { return &table; }
