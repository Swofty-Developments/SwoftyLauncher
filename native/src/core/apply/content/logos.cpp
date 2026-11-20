#include "core/apply/content/logos.hpp"

#include "core/apply/files_api.hpp"
#include "core/runtime/runtime.hpp"
#include "core/render/badges/badge_images.hpp"
#include "core/apply/content/logo_build.hpp"
#include "core/apply/content/logo_cache.hpp"
#include "core/apply/content/logo_deliver.hpp"
#include "core/apply/content/png_straight.hpp"
#include "core/diagnostics/steps.hpp"
#include "core/runtime/cache_dir.hpp"
#include "core/runtime/files.hpp"

#include <swofty/mixin.hpp>

#include <zstd.h>

#include <algorithm>
#include <atomic>
#include <map>
#include <memory>
#include <set>

namespace swofty_core::apply::content {

namespace {

constexpr const char *CACHE_ADDON = "swofty.logos";
constexpr const char *ROUTE_ID = "logos";
constexpr const wchar_t *SLICES = L"logo-slices\\";
constexpr const wchar_t *PLAN = L"plan.tsv";
constexpr ULONGLONG SLICE_KEEP_100NS = 14ull * 24 * 3600 * 10000000;

/* The slice cache's files are zstd frames of the sealed entries, written to a temporary name and moved into place,
   so a reader never sees half a file; one that does not decompress or unseal is a miss. */
logo_build::SliceCache disk_slices(const std::wstring &dir)
{
    if (dir.empty())
        return {};
    CreateDirectoryW(dir.c_str(), nullptr);
    logo_build::SliceCache c;
    c.get = [dir](const std::string &name, std::string &bytes) {
        std::string z = runtime::read_file(dir + runtime::wide(name), size_t(64) << 20);
        unsigned long long n = z.empty() ? 0 : ZSTD_getFrameContentSize(z.data(), z.size());
        if (!n || n == ZSTD_CONTENTSIZE_UNKNOWN || n == ZSTD_CONTENTSIZE_ERROR || n > (size_t(64) << 20))
            return false;
        bytes.resize(size_t(n));
        size_t got = ZSTD_decompress(bytes.data(), bytes.size(), z.data(), z.size());
        return !ZSTD_isError(got) && got == n;
    };
    c.put = [dir](const std::string &name, const std::string &bytes) {
        std::string z(ZSTD_compressBound(bytes.size()), '\0');
        size_t n = ZSTD_compress(z.data(), z.size(), bytes.data(), bytes.size(), 3);
        if (ZSTD_isError(n))
            return;
        z.resize(n);
        std::wstring path = dir + runtime::wide(name), tmp = path + L"." + std::to_wstring(GetCurrentThreadId()) + L".tmp";
        if (!runtime::write_file(tmp, z) || !MoveFileExW(tmp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING))
            DeleteFileW(tmp.c_str());
    };
    return c;
}

bool ahead_wanted()
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_LOGO_AHEAD", v, sizeof v);
    return !(n && n < sizeof v && v[0] == '0');
}

struct Ahead {
    HANDLE thread = nullptr;
    std::wstring dir;
    std::atomic<bool> hurry{false};
} ahead;

void set_priority()
{
    SetThreadPriority(GetCurrentThread(), ahead.hurry.load() ? THREAD_PRIORITY_NORMAL : THREAD_PRIORITY_BELOW_NORMAL);
}

/* Encodes, at the slice sizes of the last build, the logos whose PNG bytes the slice cache does not hold yet, while the
   game boots. It reads the addons' PNGs and writes only the slice cache, which deliver_logos reads after waiting for it,
   and slice_from_rgba gives the same bytes whichever thread runs it. */
DWORD WINAPI encode_ahead(LPVOID)
try {
    set_priority();
    perf::Stopwatch took;
    logo_build::SliceCache cache = disk_slices(ahead.dir);
    std::string plan = runtime::read_file(ahead.dir + PLAN, size_t(4) << 20);
    struct Job {
        std::string png, source;
        std::vector<std::pair<uint32_t, uint32_t>> sizes;
    };
    std::vector<Job> jobs;
    size_t lines = 0, cached = 0;
    for (size_t at = 0; at < plan.size();) {
        size_t end = plan.find('\n', at);
        if (end == std::string::npos)
            end = plan.size();
        std::string line = plan.substr(at, end - at);
        at = end + 1;
        size_t tab = line.find('\t');
        if (tab == std::string::npos)
            continue;
        lines++;
        Job j;
        j.png = runtime::read_file(runtime::under_swofty(line.substr(0, tab)), size_t(64) << 20);
        if (j.png.empty())
            continue;
        j.source = logo_build::source_of(j.png);
        for (size_t p = tab + 1; p < line.size();) {
            size_t sp = line.find(' ', p);
            if (sp == std::string::npos)
                sp = line.size();
            unsigned w = 0, h = 0;
            std::string stored, bytes;
            if (std::sscanf(line.substr(p, sp - p).c_str(), "%ux%u", &w, &h) == 2 && w && h) {
                if (cache.get(logo_build::slice_name(j.source, w, h), stored) && logo_build::unseal(stored, bytes))
                    cached++;
                else
                    j.sizes.push_back({w, h});
            }
            p = sp + 1;
        }
        if (!j.sizes.empty())
            jobs.push_back(std::move(j));
    }
    if (!lines)
        return 0;
    std::atomic<size_t> encoded{0};
    unsigned cores = std::thread::hardware_concurrency();
    size_t workers = std::clamp<unsigned>(cores / 2, 1, 4);
    std::atomic<size_t> next{0};
    auto drain = [&] {
        for (size_t i; (i = next.fetch_add(1)) < jobs.size();)
            try {
                set_priority();
                Job &j = jobs[i];
                std::string rgba;
                logo_build::PngInfo info = logo_build::look_at_png(cache, j.png, j.source, rgba);
                if (!info.ok)
                    continue;
                if (rgba.empty() && !png::decode(j.png, rgba, info.width, info.height, info.why))
                    continue;
                for (auto [w, h] : j.sizes) {
                    logo_array::Shape shape;
                    shape.width = w, shape.height = h;
                    std::string bytes, why;
                    if (logo_array::slice_from_rgba(shape, reinterpret_cast<const uint8_t *>(rgba.data()), info.width,
                                                    info.height, bytes, why)) {
                        cache.put(logo_build::slice_name(j.source, w, h), logo_build::sealed(bytes));
                        encoded++;
                    }
                }
            } catch (...) {
            }
    };
    std::vector<std::thread> pool;
    for (size_t k = 1; k < workers && k < jobs.size(); k++) {
        try {
            pool.emplace_back(drain);
        } catch (...) {
            break;
        }
    }
    drain();
    for (auto &t : pool)
        t.join();
    swofty::log("[apply] team.logo: encoded %zu slice(s) of %zu logo(s) ahead of the database load in %.0f ms; %zu of the last "
                "build's slice(s) were cached already",
                encoded.load(), jobs.size(), took.total_ms(), cached);
    return 0;
} catch (...) {
    return 0;
}

void wait_for_ahead()
{
    if (!ahead.thread)
        return;
    ahead.hurry.store(true);
    perf::Stopwatch waited;
    WaitForSingleObject(ahead.thread, INFINITE);
    CloseHandle(ahead.thread);
    ahead.thread = nullptr;
    swofty::log(perf::STEP_LINE, waited.total_ms(), "logo wait for the slices encoded ahead");
}

void write_plan(const std::wstring &dir, const logo_build::Result &res, const std::map<std::string, std::string> &path_of)
{
    if (dir.empty())
        return;
    std::map<std::string, std::string> lines;
    for (auto &[want, planned] : res.planned) {
        auto it = path_of.find(planned.source);
        if (it == path_of.end() || planned.sizes.empty())
            continue;
        std::string sizes;
        for (auto [w, h] : planned.sizes)
            sizes += (sizes.empty() ? "" : " ") + std::to_string(w) + "x" + std::to_string(h);
        lines[it->second] = sizes;
    }
    std::string text;
    for (auto &[path, sizes] : lines)
        text += path + "\t" + sizes + "\n";
    if (runtime::read_file(dir + PLAN, size_t(4) << 20) != text)
        runtime::write_file(dir + PLAN, text);
}

/* Keeps the slices this build used and any written in the last two weeks, so going back to an earlier set of addons
   soon after is still cheap. */
void prune_slices(const std::wstring &dir, const logo_build::Result &res)
{
    if (dir.empty())
        return;
    std::set<std::string> used;
    for (auto &[want, planned] : res.planned) {
        used.insert(logo_build::info_name(planned.source));
        for (auto [w, h] : planned.sizes)
            used.insert(logo_build::slice_name(planned.source, w, h));
    }
    FILETIME now_ft;
    GetSystemTimeAsFileTime(&now_ft);
    ULONGLONG now = (ULONGLONG(now_ft.dwHighDateTime) << 32) | now_ft.dwLowDateTime;
    WIN32_FIND_DATAW fd;
    HANDLE h = FindFirstFileW((dir + L"*").c_str(), &fd);
    if (h == INVALID_HANDLE_VALUE)
        return;
    do {
        if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)
            continue;
        std::string n = runtime::narrow(fd.cFileName);
        if (n == runtime::narrow(PLAN) || used.count(n))
            continue;
        ULONGLONG written = (ULONGLONG(fd.ftLastWriteTime.dwHighDateTime) << 32) | fd.ftLastWriteTime.dwLowDateTime;
        if (now > written && now - written > SLICE_KEEP_100NS)
            DeleteFileW((dir + fd.cFileName).c_str());
    } while (FindNextFileW(h, &fd));
    FindClose(h);
}

struct FileSource {
    HANDLE h = INVALID_HANDLE_VALUE;
    erpfile::Source src;
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

void list_erps(const std::wstring &dir, const std::string &rel, std::vector<std::string> &out, unsigned depth)
{
    if (depth > 4)
        return;
    WIN32_FIND_DATAW fd;
    HANDLE h = FindFirstFileW((dir + L"*").c_str(), &fd);
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
    std::sort(out.begin(), out.end());
}

bool size_file(const std::string &rel, uint64_t &size)
{
    WIN32_FILE_ATTRIBUTE_DATA a{};
    if (!GetFileAttributesExW(game_file(rel).c_str(), GetFileExInfoStandard, &a))
        return false;
    size = (uint64_t(a.nFileSizeHigh) << 32) | a.nFileSizeLow;
    return true;
}

bool stat_file(const std::string &rel, logo_cache::Seen &now)
{
    HANDLE h = CreateFileW(game_file(rel).c_str(), FILE_READ_ATTRIBUTES, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                           nullptr, OPEN_EXISTING, 0, nullptr);
    if (h == INVALID_HANDLE_VALUE)
        return false;
    BY_HANDLE_FILE_INFORMATION info{};
    FILE_BASIC_INFO basic{};
    bool ok = GetFileInformationByHandle(h, &info) && GetFileInformationByHandleEx(h, FileBasicInfo, &basic, sizeof basic) &&
              !(info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY);
    CloseHandle(h);
    if (!ok)
        return false;
    now.size = (uint64_t(info.nFileSizeHigh) << 32) | info.nFileSizeLow;
    now.time = uint64_t(basic.LastWriteTime.QuadPart);
    now.changed = uint64_t(basic.ChangeTime.QuadPart);
    now.id = (uint64_t(info.nFileIndexHigh) << 32) | info.nFileIndexLow;
    now.links = info.nNumberOfLinks;
    return true;
}

uint64_t file_clock()
{
    FILETIME ft;
    GetSystemTimeAsFileTime(&ft);
    return (uint64_t(ft.dwHighDateTime) << 32) | ft.dwLowDateTime;
}

bool hash_file(const std::string &rel, uint64_t &hash)
{
    HANDLE h = CreateFileW(game_file(rel).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                           OPEN_EXISTING, FILE_FLAG_SEQUENTIAL_SCAN, nullptr);
    if (h == INVALID_HANDLE_VALUE)
        return false;
    std::vector<char> buf(size_t(1) << 20);
    logo_cache::Bytes bytes;
    DWORD got = 0;
    bool ok = true;
    while ((ok = ReadFile(h, buf.data(), DWORD(buf.size()), &got, nullptr)) && got)
        bytes.add(buf.data(), got);
    CloseHandle(h);
    if (ok)
        hash = bytes.done();
    return ok;
}

bool write_copy(const std::wstring &path, const std::function<bool(const erpfile::Sink &, std::string &)> &emit, std::string &why,
                uint32_t *error = nullptr)
{
    std::wstring tmp = path + L".tmp";
    uint32_t last = 0;
    HANDLE out = CreateFileW(tmp.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (out == INVALID_HANDLE_VALUE)
        last = GetLastError();
    bool ok = out != INVALID_HANDLE_VALUE && emit(
                                                 [&](const char *p, size_t n) {
                                                     DWORD put = 0;
                                                     while (n) {
                                                         DWORD want = DWORD(std::min<size_t>(n, size_t(1) << 24));
                                                         if (!WriteFile(out, p, want, &put, nullptr) || !put) {
                                                             last = GetLastError();
                                                             return false;
                                                         }
                                                         p += put;
                                                         n -= put;
                                                     }
                                                     return true;
                                                 },
                                                 why);
    if (out != INVALID_HANDLE_VALUE)
        CloseHandle(out);
    if (ok && !MoveFileExW(tmp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING)) {
        last = GetLastError();
        ok = false;
    }
    if (ok)
        return true;
    DeleteFileW(tmp.c_str());
    if (error)
        *error = last;
    return false;
}

std::string flat(const std::string &rel)
{
    std::string folder = logo_build::UI_FOLDER;
    std::string s = rel.compare(0, folder.size(), folder) == 0 ? rel.substr(folder.size()) : rel;
    std::replace(s.begin(), s.end(), '/', '_');
    return s;
}

std::string swofty_base()
{
    std::string root = runtime::narrow(runtime::game_root()), sw = runtime::narrow(runtime::swofty_root());
    if (runtime::lower(sw).rfind(runtime::lower(root), 0) != 0)
        return {};
    std::string base = sw.substr(root.size());
    std::replace(base.begin(), base.end(), '\\', '/');
    while (!base.empty() && base.front() == '/')
        base.erase(base.begin());
    if (!base.empty() && base.back() != '/')
        base += '/';
    return base;
}

std::map<std::string, std::vector<std::string>> overlay_archives(const std::string &base)
{
    std::map<std::string, std::vector<std::string>> out;
    if (base.empty())
        return out;
    for (auto &j : runtime::config()["resource_erps"].a) {
        std::string file = j["file"].str();
        if (file.empty())
            continue;
        std::replace(file.begin(), file.end(), '\\', '/');
        std::string rel = base + file;
        for (auto &x : j["erps"].a)
            if (x.type == json::Value::Str) {
                auto &list = out[runtime::lower(x.s)];
                if (std::find(list.begin(), list.end(), rel) == list.end())
                    list.push_back(rel);
            }
    }
    return out;
}

}

void encode_logos_ahead()
{
    if (!ahead_wanted() || runtime::cache_dir().empty())
        return;
    ahead.dir = runtime::cache_dir() + SLICES;
    ahead.thread = CreateThread(nullptr, 0, encode_ahead, nullptr, 0, nullptr);
}

void deliver_logos(const std::vector<LogoImage> &logos, LogoReport &report)
{
    wait_for_ahead();
    std::wstring slices_dir = runtime::cache_dir().empty() ? std::wstring() : runtime::cache_dir() + SLICES;
    logo_build::SliceCache slices = disk_slices(slices_dir);
    std::map<std::string, std::string> path_of;
    std::vector<logo_build::Want> wants;
    std::set<std::string> seen;
    logo_build::Refusals refusals;
    perf::Stopwatch steps;
    auto step = [&](double ms, const std::string &name) { swofty::log(perf::STEP_LINE, ms, ("logo " + name).c_str()); };
    uint64_t key = runtime::fnv64_field(runtime::FNV64_BASIS, logo_cache::CACHE_FORMAT);
    for (auto &l : logos) {
        if (!seen.insert(l.array + "|" + l.key).second)
            continue;
        logo_build::Want w;
        w.array = l.array;
        w.key = l.key;
        key = runtime::fnv64_field(key, l.array + "|" + l.key + "|" + l.stock + (l.fallback ? "|f" : ""));
        if (l.stock.empty()) {
            std::string rel = "addons/" + l.addon + "/" + l.file;
            std::string bytes = runtime::read_file(runtime::under_swofty(rel), size_t(64) << 20), why;
            logo_build::PngInfo info;
            if (!bytes.empty()) {
                w.source = logo_build::source_of(bytes);
                info = logo_build::look_at_png(slices, bytes, w.source, w.rgba);
                why = info.why;
            }
            if (bytes.empty() || !info.ok) {
                refusals.add(l.context + " image does not decode: " + (why.empty() ? "it is empty" : why), l.id + " " + l.file);
                report.refused++;
                continue;
            }
            w.width = info.width, w.height = info.height;
            key = runtime::fnv64_field(key, bytes);
            path_of.emplace(w.source, rel);
            if (w.rgba.empty())
                w.png = std::move(bytes);
        } else {
            w.donor = l.stock;
            w.fallback = l.fallback;
        }
        wants.push_back(std::move(w));
    }
    refusals.flush(report.lines);
    step(steps.lap_ms(), "decode (" + std::to_string(wants.size()) + " logo(s))");
    if (wants.empty())
        return;
    std::vector<std::string> files;
    list_erps(game_file(logo_build::UI_FOLDER), logo_build::UI_FOLDER, files, 0);
    std::string base = swofty_base();
    auto overlays = overlay_archives(base);
    std::set<std::string> overlay_rels;
    for (auto &[target, list] : overlays)
        overlay_rels.insert(list.begin(), list.end());
    logo_build::Env env;
    env.slices = slices;
    env.list = [&] { return files; };
    env.overlays = [&](const std::string &rel) {
        auto it = overlays.find(runtime::lower(rel));
        return it == overlays.end() ? std::vector<std::string>{} : it->second;
    };
    env.open = [&](const std::string &rel) -> std::shared_ptr<erpfile::Source> {
        auto fs = std::make_shared<FileSource>();
        fs->h = CreateFileW(game_file(rel).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0,
                            nullptr);
        LARGE_INTEGER size;
        if (fs->h == INVALID_HANDLE_VALUE || !GetFileSizeEx(fs->h, &size))
            return nullptr;
        fs->src.size = uint64_t(size.QuadPart);
        HANDLE h = fs->h;
        fs->src.read = [h](uint64_t off, size_t n, char *out) {
            LARGE_INTEGER at;
            at.QuadPart = LONGLONG(off);
            DWORD got = 0;
            return SetFilePointerEx(h, at, nullptr, FILE_BEGIN) && ReadFile(h, out, DWORD(n), &got, nullptr) && got == n;
        };
        return std::shared_ptr<erpfile::Source>(fs, &fs->src);
    };
    std::vector<std::string> boot = logo_build::array_layers(env, logo_build::BOOT_ARCHIVE);
    if (!boot.empty()) {
        files.push_back(logo_build::BOOT_ARCHIVE);
        report.lines.push_back("team.logo: the boot archive " + std::string(logo_build::BOOT_ARCHIVE) +
                               " carries logo arrays in " + boot[0] +
                               (boot.size() > 1 ? " and " + std::to_string(boot.size() - 1) + " more layer(s)" : "") +
                               "; its copies get the same slices as the ui_package ones");
    }
    std::vector<std::string> keyed_files = files;
    for (auto &rel : files) {
        auto it = overlays.find(runtime::lower(rel));
        if (it != overlays.end())
            keyed_files.insert(keyed_files.end(), it->second.begin(), it->second.end());
    }
    std::string dir = "addons/" + std::string(CACHE_ADDON) + "/";
    uint32_t unwritable = runtime::write_refusal(runtime::under_swofty(dir));
    std::wstring stamps_path = runtime::under_swofty(dir + logo_cache::STAMPS);
    DeleteFileW(runtime::under_swofty(dir + logo_cache::OLD_STAMPS).c_str());
    std::string stamps_before = runtime::read_file(stamps_path, size_t(16) << 20);
    logo_cache::Env stamp_env{
        size_file, stat_file,
        [&](const std::string &rel, uint64_t &hash) { return logo_build::input_hash(env, rel, hash_file, hash); }, file_clock};
    auto key_inputs = [&](const std::set<std::string> &inputs, const logo_cache::Stamps &before) {
        DWORD t0 = GetTickCount();
        logo_cache::Keyed k = logo_cache::key_archives(key, keyed_files, inputs, stamp_env, before);
        if (k.hashed)
            report.lines.push_back("team.logo: " + std::to_string(k.hashed) + " input archive(s) (" +
                                   std::to_string(k.hashed_bytes >> 20) + " MB) hashed in " +
                                   std::to_string(GetTickCount() - t0) + " ms");
        return k;
    };
    logo_cache::Stamps before = logo_cache::parse(stamps_before);
    std::set<std::string> inputs = logo_cache::inputs_of(before);
    inputs.insert(logo_build::DATABASES);
    logo_cache::Keyed keyed = key_inputs(inputs, before);
    std::string id = runtime::hex16(keyed.key).substr(2);
    step(steps.lap_ms(),
         "list and hash (" + std::to_string(keyed_files.size()) + " archive(s), " + std::to_string(keyed.hashed) + " hashed)");
    std::string manifest_name = "logos_" + id + ".manifest";
    std::string manifest = runtime::read_file(runtime::under_swofty(dir + manifest_name), size_t(1) << 20);
    logo_cache::Routes routes;
    logo_cache::Sources sources;
    int cached_built = -1;
    if (!manifest.empty())
        logo_cache::manifest_parse(
            manifest,
            [&](const std::string &file) {
                return GetFileAttributesW(runtime::under_swofty(dir + file).c_str()) != INVALID_FILE_ATTRIBUTES;
            },
            cached_built, routes, &sources);
    if (routes.empty() && unwritable) {
        report.lines.push_back(logo_deliver::unwritable_line("swofty/" + dir, unwritable));
        report.refused += int(wants.size());
        return;
    }
    struct Built {
        std::vector<std::string> lines;
        int built = 0, refused = 0;
        size_t changes = 0;
        DWORD t0 = 0;
        logo_cache::Sources sources;
    } made;
    bool layered = false;
    auto make = [&](const std::set<std::string> &dropped) {
        logo_deliver::Attempt a;
        made = {};
        made.t0 = GetTickCount();
        DeleteFileW(runtime::under_swofty(dir + manifest_name).c_str());
        std::vector<logo_build::Want> kept;
        for (auto &w : wants)
            if (dropped.count(w.array))
                made.refused++;
            else
                kept.push_back(w);
        logo_build::Result res;
        perf::Stopwatch build_steps;
        logo_build::build(env, kept, res);
        double built_ms = build_steps.lap_ms();
        step(res.encode_ms, "encode (" + std::to_string(res.encodes) + " slice(s): " + std::to_string(res.slices.cached) +
                                " cached, " + std::to_string(res.slices.encoded) + " encoded, " +
                                std::to_string(res.slices.decoded) + " image(s) decoded)");
        write_plan(slices_dir, res, path_of);
        prune_slices(slices_dir, res);
        step(built_ms - res.encode_ms, "read and plan (" + std::to_string(res.read.size()) + " archive(s) read)");
        made.lines = res.lines;
        made.refused += res.refused;
        made.built = res.built;
        made.changes = res.changes.size();
        std::set<std::string> read = res.read;
        read.insert(logo_build::DATABASES);
        if (read != inputs) {
            inputs = read;
            keyed = key_inputs(read, keyed.stamps);
            id = runtime::hex16(keyed.key).substr(2);
            manifest_name = "logos_" + id + ".manifest";
        }
        layered = false;
        for (auto &[rel, changes] : res.changes) {
            auto src = env.open(rel);
            std::string why;
            std::string file = "logos_" + id + "_" + flat(rel);
            uint64_t whole = 0;
            bool hashed = rel == logo_build::DATABASES && hash_file(rel, whole);
            uint32_t error = 0;
            bool ok = write_copy(
                runtime::under_swofty(dir + file),
                [&](const erpfile::Sink &sink, std::string &w) {
                    erpfile::Archive arc;
                    return src && erpfile::open(*src, arc, w) && erpfile::rewrite(*src, arc, changes, sink, w);
                },
                why, &error);
            a.arrays[rel] = logo_deliver::arrays_of(changes, logo_build::ARRAY_URI);
            if (!ok) {
                made.lines.push_back("team.logo: " + rel + " could not be rewritten" + (why.empty() ? "" : ": " + why) +
                                     (error ? " (error " + std::to_string(error) + " on swofty/" + dir + file + ")" : ""));
                made.refused++;
                a.unwritten.push_back({rel, "it could not be rewritten" + (why.empty() ? "" : ": " + why)});
                continue;
            }
            layered = layered || overlay_rels.count(rel);
            a.routes.emplace_back(rel, file);
            if (hashed)
                made.sources[rel] = whole;
        }
        step(build_steps.lap_ms(), "archive rewrite (" + std::to_string(res.changes.size()) + " archive(s))");
        return a;
    };
    logo_deliver::Sink sink;
    sink.overlay = [&](const std::string &rel) { return overlay_rels.count(rel) != 0; };
    sink.route = [&](const std::string &rel, const std::string &file, std::string &why) {
        auto o = files::redirect(rel, CACHE_ADDON, file, "team.logo", ROUTE_ID, "boot");
        if (!o.refused() && !(layered && o.kind == files::Kind::shadowed))
            return true;
        why = o.line;
        return false;
    };
    sink.unroute = [&] { files::withdraw(CACHE_ADDON, "team.logo", ROUTE_ID); };
    sink.offer = [&](const logo_cache::Routes &copies, std::string &why) {
        std::map<std::string, logo_deliver::Copy> held;
        for (auto &[rel, file] : copies) {
            logo_deliver::Copy c;
            c.path = logo_deliver::overlay_key(base + dir + file);
            if (!runtime::file_size(runtime::under_swofty(dir + file), &c.size) || 6 + c.path.size() > files::GAME_PATH_MAX) {
                why = "its copy " + c.path + " is missing or its path is too long for the game";
                return false;
            }
            held[logo_deliver::overlay_key(rel)] = std::move(c);
        }
        if (logo_deliver::overlay_copies().offer(held)) {
            std::map<std::string, std::string> active;
            for (const auto &[rel, copy] : held)
                active[rel] = copy.path;
            badge_images::overlay_routes(active);
            return true;
        }
        why = "the resource overlay was handed to the game before the logo build ended";
        return false;
    };
    auto databases_again = [&] {
        for (auto &[rel, file] : routes) {
            if (rel != logo_build::DATABASES)
                continue;
            uint64_t whole = 0;
            if (!hash_file(rel, whole))
                return false;
            auto was = sources.find(rel);
            if (was != sources.end() && was->second == whole)
                return true;
            perf::Stopwatch took;
            std::wstring path = runtime::under_swofty(dir + file);
            std::string copy = runtime::read_file(path, size_t(256) << 20), why;
            auto now = env.open(rel);
            if (copy.empty() || !now ||
                !write_copy(
                    path,
                    [&](const erpfile::Sink &sink, std::string &w) { return logo_build::databases_copy(*now, copy, sink, w); },
                    why)) {
                report.lines.push_back("team.logo: the cached copy of " + rel + " could not be made again" +
                                       (why.empty() ? "" : ": " + why) + "; the logos are built again");
                return false;
            }
            sources[rel] = whole;
            runtime::write_file(runtime::under_swofty(dir + manifest_name),
                                logo_cache::manifest_text(cached_built, routes, sources));
            double ms = took.lap_ms();
            step(ms, "databases.erp copy");
            report.lines.push_back("team.logo: " + rel +
                                   " changed outside its overrides.uidb; its cached copy is made again in " +
                                   std::to_string(int(ms)) + " ms");
        }
        return true;
    };
    if (!routes.empty() && !databases_again())
        routes.clear();
    logo_deliver::Attempt first;
    if (!routes.empty()) {
        first.routes = routes;
        first.cached = true;
        for (auto &r : routes)
            layered = layered || overlay_rels.count(r.first);
    } else
        first = make({});
    steps.lap_ms();
    logo_deliver::Outcome done = logo_deliver::deliver(std::move(first), make, sink);
    step(steps.lap_ms(), done.last.cached ? "deliver (cached archives)" : "build and deliver");
    routes = done.last.routes;
    if (done.last.cached) {
        report.built = cached_built;
        report.lines.push_back("team.logo: " + std::to_string(routes.size()) + " cached archive(s) of " + id + " reused");
    } else {
        for (auto &l : made.lines)
            report.lines.push_back(l);
        for (auto &l : done.lines)
            report.lines.push_back(l);
        report.refused += made.refused;
        if (logo_cache::cacheable(routes.size(), made.changes, made.refused))
            runtime::write_file(runtime::under_swofty(dir + manifest_name),
                                logo_cache::manifest_text(made.built, routes, made.sources));
        report.built = made.built;
        report.lines.push_back("team.logo: " + std::to_string(made.built) + " slice(s) built into " +
                               std::to_string(routes.size()) + " archive(s) in " + std::to_string(GetTickCount() - made.t0) +
                               " ms");
    }
    for (auto &f : done.failed) {
        bool unwritten =
            std::any_of(done.last.unwritten.begin(), done.last.unwritten.end(), [&](auto &u) { return u.rel == f.rel; });
        if (unwritten)
            continue;
        report.refused++;
        report.lines.push_back("team.logo: " + f.rel + " is not delivered: " + f.why);
    }
    std::string stamps_after = logo_cache::text(keyed.stamps);
    if (stamps_after != stamps_before)
        runtime::write_file(stamps_path, stamps_after);
    std::set<std::string> keep;
    for (auto &r : routes)
        keep.insert(r.second);
    keep.insert(manifest_name);
    WIN32_FIND_DATAW fd;
    HANDLE h = FindFirstFileW((runtime::under_swofty(dir) + L"logos_*").c_str(), &fd);
    if (h != INVALID_HANDLE_VALUE) {
        do {
            std::string n = runtime::narrow(fd.cFileName);
            if (!keep.count(n))
                DeleteFileW((runtime::under_swofty(dir) + runtime::wide(n)).c_str());
        } while (FindNextFileW(h, &fd));
        FindClose(h);
    }
    step(steps.total_ms(), "total");
}

}
