#include "core/apply/content/box_ui.hpp"

#include "core/apply/applier.hpp"
#include "core/apply/content/appliers.hpp"
#include "core/apply/content/garage_build.hpp"
#include "core/apply/files_api.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <algorithm>
#include <cstring>
#include <filesystem>
#include <map>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <vector>

// ui.per_package (Applies::content, box_ui.hpp): at each box press the box's records are read; for every archive a
// record lists, the core writes (or reuses) a copy under swofty/addons/swofty.box_ui/ with the record's textures
// swapped in, and routes the archive to it in the scope box_ui::SCOPE. The next box press, leaving the box and a
// career load put the scope back, so the game's own archive (or another route's copy) is opened again. Off unless
// SWOFTY_BOX_UI=1. Careers are left out in this release (a career's own activation swaps nothing).

namespace swofty_core::apply::content {

namespace {

namespace bu = box_ui;

bool fence_on()
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_BOX_UI", v, sizeof v);
    return n && n < sizeof v && v[0] == '1';
}

std::wstring game_file(const std::string &rel)
{
    std::string p = rel;
    std::replace(p.begin(), p.end(), '/', '\\');
    return runtime::game_root() + L"\\" + runtime::wide(p);
}

// The file the game opens for `rel` while no box UI route holds it: another applier's copy (team.logo, HUD), else the
// game folder's.
std::wstring current_file(const std::string &rel)
{
    std::string target;
    if (runtime::file_route_target(rel, target) && target.find(bu::CACHE_ADDON) == std::string::npos)
        return game_file(target);
    return game_file(rel);
}

struct FileSource {
    HANDLE h = INVALID_HANDLE_VALUE;
    uint64_t stamp = 0;
    erpfile::Source src;
    ~FileSource()
    {
        if (h != INVALID_HANDLE_VALUE)
            CloseHandle(h);
    }
};

std::unique_ptr<FileSource> open_file(const std::wstring &path)
{
    auto fs = std::make_unique<FileSource>();
    fs->h = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING, 0, nullptr);
    LARGE_INTEGER size;
    FILETIME written;
    if (fs->h == INVALID_HANDLE_VALUE || !GetFileSizeEx(fs->h, &size) || !GetFileTime(fs->h, nullptr, nullptr, &written))
        return nullptr;
    fs->src.size = uint64_t(size.QuadPart);
    fs->stamp = (uint64_t(written.dwHighDateTime) << 32) | written.dwLowDateTime;
    HANDLE h = fs->h;
    fs->src.read = [h](uint64_t off, size_t n, char *out) {
        LARGE_INTEGER at;
        at.QuadPart = LONGLONG(off);
        DWORD got = 0;
        return SetFilePointerEx(h, at, nullptr, FILE_BEGIN) && ReadFile(h, out, DWORD(n), &got, nullptr) && got == n;
    };
    return fs;
}

struct Payload {
    std::string bytes;
    erpfile::Source src;
    erpfile::Archive arc;
    std::set<std::string> only;
    std::string why;
    bool ok = false;
};

std::unique_ptr<Payload> load_payload(const bu::Record &r, std::vector<std::string> &lines)
{
    auto p = std::make_unique<Payload>();
    const std::string who = std::string(bu::REGISTRY) + " " + r.id + ": ";
    files::Resolved at = files::resolve(r.addon, r.folder.empty() ? r.erp : r.folder);
    if (!at.ok()) {
        lines.push_back(who + at.why);
        return nullptr;
    }
    if (!r.folder.empty()) {
        std::filesystem::path root(runtime::under_swofty(at.target));
        std::vector<garage::Input> inputs;
        std::error_code ec;
        for (std::filesystem::recursive_directory_iterator it(root, ec), end; !ec && it != end; it.increment(ec)) {
            if (!it->is_regular_file(ec))
                continue;
            std::string rel = std::filesystem::relative(it->path(), root).generic_string();
            if (garage::texture_file(rel))
                inputs.push_back({rel, runtime::read_file(it->path().wstring(), size_t(64) << 20)});
        }
        std::sort(inputs.begin(), inputs.end(), [](const garage::Input &a, const garage::Input &b) { return a.rel < b.rel; });
        if (inputs.empty()) {
            lines.push_back(who + r.folder + " holds no .dds or .png file in swofty/" + at.target);
            return nullptr;
        }
        // UI sprites: no stream descriptor (docs/reference/ui/art-resources.md).
        garage::Built built = garage::build(inputs, r.root, r.srgb, false);
        for (auto &l : built.lines)
            lines.push_back(who + l);
        p->bytes = std::move(built.erp);
    } else {
        p->bytes = runtime::read_file(runtime::under_swofty(at.target), size_t(256) << 20);
        p->only.insert(r.resources.begin(), r.resources.end());
    }
    if (p->bytes.empty()) {
        lines.push_back(who + "no textures could be read from swofty/" + at.target);
        return nullptr;
    }
    Payload *raw = p.get();
    p->src.size = p->bytes.size();
    p->src.read = [raw](uint64_t off, size_t n, char *out) {
        if (off + n > raw->bytes.size())
            return false;
        std::memcpy(out, raw->bytes.data() + off, n);
        return true;
    };
    if (!erpfile::open(p->src, p->arc, p->why)) {
        lines.push_back(who + "the textures archive does not open: " + p->why);
        return nullptr;
    }
    return p;
}

bool write_copy(const std::wstring &path, const erpfile::Source &src, const erpfile::Archive &arc,
                const erpfile::Changes &changes, std::string &why)
{
    std::wstring tmp = path + L".tmp";
    HANDLE out = CreateFileW(tmp.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (out == INVALID_HANDLE_VALUE)
        return why = "error " + std::to_string(GetLastError()) + " creating the copy", false;
    bool ok = erpfile::rewrite(
        src, arc, changes,
        [&](const char *p, size_t n) {
            while (n) {
                DWORD put = 0, want = DWORD(std::min<size_t>(n, size_t(1) << 24));
                if (!WriteFile(out, p, want, &put, nullptr) || !put)
                    return false;
                p += put;
                n -= put;
            }
            return true;
        },
        why);
    CloseHandle(out);
    if (ok && !MoveFileExW(tmp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING)) {
        why = "error " + std::to_string(GetLastError()) + " moving the copy into place";
        ok = false;
    }
    if (!ok)
        DeleteFileW(tmp.c_str());
    return ok;
}

// The names each resident archive holds, read once.
const std::set<std::string> &resident_names(const char *rel)
{
    static std::mutex mu;
    static std::map<std::string, std::set<std::string>> held;
    std::lock_guard g(mu);
    auto it = held.find(rel);
    if (it != held.end())
        return it->second;
    std::set<std::string> &names = held[rel];
    if (auto fs = open_file(game_file(rel))) {
        erpfile::Archive arc;
        std::string why;
        if (erpfile::open(fs->src, arc, why))
            for (auto &e : arc.entries)
                names.insert(e.name);
    }
    return names;
}

class BoxUi : public Applier {
public:
    const char *registry() const override { return bu::REGISTRY; }
    Applies applies() const override { return Applies::content; }

    void activate(Store &store, const Context &ctx, Report &report) override
    {
        if (!ctx.career.empty() || ctx.content.empty())
            return;
        std::vector<bu::Record> all;
        for (auto &id : store.list(bu::REGISTRY, "")) {
            if (id.rfind("game/", 0) == 0)
                continue;
            auto rec = store.get(bu::REGISTRY, id);
            if (rec.status == swofty_core::registry::Status::ok)
                all.push_back(bu::parse(id, rec.value));
        }
        std::vector<bu::Record> mine = bu::for_box(ctx.content, all);
        if (mine.empty())
            return;
        if (!fence_on()) {
            report.lines.push_back(bu::FENCE_LINE);
            return;
        }
        std::string dir = "addons/" + std::string(bu::CACHE_ADDON) + "/";
        CreateDirectoryW(runtime::under_swofty("addons").c_str(), nullptr);
        CreateDirectoryW(runtime::under_swofty(dir).c_str(), nullptr);
        for (auto &r : mine)
            apply_record(r, dir, report);
    }

    void deactivate(Store &, Report &report) override
    {
        for (auto &l : files::put_back(bu::SCOPE)) {
            report.put_back++;
            report.lines.push_back(std::string(bu::REGISTRY) + ": " + l);
        }
    }

private:
    void apply_record(const bu::Record &r, const std::string &dir, Report &report)
    {
        const std::string who = std::string(bu::REGISTRY) + " " + r.id + ": ";
        if (!r.ok()) {
            report.refused++;
            for (auto &l : r.lines)
                report.lines.push_back(l);
            return;
        }
        auto payload = load_payload(r, report.lines);
        if (!payload) {
            report.refused++;
            return;
        }
        uint64_t key = bu::mix(0xcbf29ce484222325ull, r.id);
        key = bu::mix(key, payload->bytes);
        for (auto &n : payload->only)
            key = bu::mix(key, n);
        std::set<std::string> swapped;
        for (auto &rel : r.archives) {
            if (bu::resident(rel)) {
                report.refused++;
                report.lines.push_back(who + rel + " refused: the game loads it at boot and keeps it, so it never opens a copy");
                continue;
            }
            DWORD t0 = GetTickCount();
            auto fs = open_file(current_file(rel));
            erpfile::Archive arc;
            std::string why;
            if (!fs || !erpfile::open(fs->src, arc, why)) {
                report.refused++;
                report.lines.push_back(who + rel + " not swapped: it does not open" + (why.empty() ? "" : ": " + why));
                continue;
            }
            uint64_t k = bu::mix(bu::mix(key, rel), std::to_string(fs->src.size) + ":" + std::to_string(fs->stamp));
            std::string file = bu::copy_name(rel, k);
            std::wstring path = runtime::under_swofty(dir + file);
            uint64_t have = 0;
            bool reused = runtime::file_size(path, &have) && have > 0;
            bu::Plan plan;
            if (!bu::plan(arc, payload->src, payload->arc, payload->only, plan, why)) {
                report.refused++;
                report.lines.push_back(who + rel + " not swapped: " + why);
                continue;
            }
            for (auto &l : plan.lines)
                report.lines.push_back(who + rel + ": " + l);
            if (plan.swapped.empty()) {
                report.lines.push_back(who + rel + " holds none of the textures; not routed");
                continue;
            }
            if (!reused && !write_copy(path, fs->src, arc, plan.changes, why)) {
                report.refused++;
                report.lines.push_back(who + rel + " not swapped: " + why);
                continue;
            }
            files::Outcome o = files::redirect(rel, bu::CACHE_ADDON, file, bu::REGISTRY, r.id, bu::SCOPE);
            if (o.refused()) {
                report.refused++;
                report.lines.push_back(o.line);
                continue;
            }
            report.applied++;
            swapped.insert(plan.swapped.begin(), plan.swapped.end());
            report.lines.push_back(who + rel + " -> swofty/" + dir + file + " (" + std::to_string(plan.swapped.size()) +
                                   " resource(s), " + (reused ? "cached copy" : "written") + " in " +
                                   std::to_string(GetTickCount() - t0) + " ms)");
        }
        for (auto &e : payload->arc.entries) {
            if ((!payload->only.empty() && !payload->only.count(e.name)) || bu::stem_of(e.name) == e.name)
                continue;
            bool kept = false;
            for (const auto &res : bu::RESIDENT) {
                if (res.prefix || !resident_names(res.path).count(e.name))
                    continue;
                kept = true;
                report.lines.push_back(who + e.name + " stays stock: " + res.path +
                                       " also holds it, and the game keeps the copy it loads at boot");
                break;
            }
            if (!kept && !swapped.count(e.name))
                report.lines.push_back(who + e.name + " is in none of the listed archives");
        }
        for (auto &n : payload->only)
            if (!payload->arc.find(n))
                report.lines.push_back(who + n + " is not in " + r.erp);
    }
};

}

void register_box_ui() { make<BoxUi>(); }

}
