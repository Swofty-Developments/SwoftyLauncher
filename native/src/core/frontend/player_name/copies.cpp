#include "core/hooks/guard.hpp"
#include "core/frontend/mem_scan_logic.hpp"
#include "core/frontend/player_name/copies.hpp"
#include "core/frontend/player_name_logic.hpp"
#include "core/frontend/ui_registry_walk.hpp"
#include "core/frontend/vm_string.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <atomic>
#include <condition_variable>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

// The source swap (swap_sources) scans the game's writable memory for copies of the stock persona and rewrites them. It runs
// on a thread started the first time it is needed, only in a Time Trial session on our content, and only when the pin pass
// asks: once for that session, again when the stock or the shown name changes, and again when the pin pass still finds the
// stock name on a screen 30 s or more after the last scan (two times a session at most). It stops when the session ends.
// SWOFTY_TT_PLAYER_NAME_SWAP=0 turns it off.

namespace frontend::player_name {

constinit SourceSwap source_swap;

namespace {

// Reads of game memory. Small reads (node fields) are guarded copies; anything bigger goes through ReadProcessMemory, which reports a
// short read instead of faulting. A fault inside the big memcpy of the core's runtime cannot be dispatched under Wine (run67: "invalid
// frame ... unable to dispatch exception"), so no large read may rely on the SEH guard.
bool read_process(uint64_t at, void *out, size_t n, size_t *got = nullptr)
{
    SIZE_T done = 0;
    bool ok = ReadProcessMemory(GetCurrentProcess(), reinterpret_cast<const void *>(at), out, n, &done) != 0 && done == n;
    if (got)
        *got = size_t(done);
    return ok;
}

struct Region {
    uint64_t base, size;
    unsigned type;
};

std::vector<Region> rw_regions()
{
    std::vector<Region> out;
    MEMORY_BASIC_INFORMATION m{};
    uint64_t at = 0x10000;
    while (at < 0x7fffffff0000ull && VirtualQuery(reinterpret_cast<void *>(at), &m, sizeof m)) {
        bool ok = m.State == MEM_COMMIT && m.Type == MEM_PRIVATE && m.Protect == PAGE_READWRITE;
        if (ok && m.RegionSize <= (uint64_t(1) << 30))
            out.push_back({uint64_t(m.BaseAddress), uint64_t(m.RegionSize), unsigned(m.Type)});
        uint64_t next = uint64_t(m.BaseAddress) + m.RegionSize;
        if (next <= at)
            break;
        at = next;
    }
    return out;
}

struct Variant {
    std::string needle;
    const char *label;
    bool wide;
};

std::string widen(const std::string &s)
{
    std::string w;
    for (unsigned char c : s) {
        w.push_back(char(c));
        w.push_back('\0');
    }
    return w;
}

std::string hex_of(const uint8_t *b, size_t n)
{
    std::string hex;
    char h[4];
    for (size_t i = 0; i < n; i++) {
        snprintf(h, sizeof h, "%02x", b[i]);
        hex += h;
        if (i % 8 == 7)
            hex += ' ';
    }
    return hex;
}

bool write_process(uint64_t at, const void *data, size_t n)
{
    SIZE_T done = 0;
    return WriteProcessMemory(GetCurrentProcess(), reinterpret_cast<void *>(at), data, n, &done) != 0 && done == n;
}

std::string fit_name(const std::string &name, size_t max)
{
    if (name.size() <= max)
        return name;
    size_t cut = max;
    while (cut > 0 && (uint8_t(name[cut]) & 0xc0) == 0x80)
        cut--;
    return name.substr(0, cut);
}

struct SwapResult {
    size_t records = 0, strings = 0, scanned_mb = 0, skipped = 0;
    bool limit = false, stopped = false;
};

// Rewrites, in place, every copy of the stock persona the game keeps as input: refcounted VM string records (length word at +6, characters
// from +9) and MSVC std::string objects in small-string form (16 byte buffer, u64 size, u64 capacity 15) inside private read-write memory.
// The game's consumers (entrant names, the timing tower and HUD leaves, the leaderboard rows) are all built from such copies. Only strings
// that read exactly the stock persona change; the new text is cut to the stock length (a record's or small string's storage is never
// grown, so nothing is allocated or freed). Reads and writes go through ReadProcessMemory / WriteProcessMemory, which fail instead of faulting.
// The scan stops the moment the session it was planned for ends.
SwapResult swap_sources(const std::string &stock, const std::string &name_in, uint64_t serial)
{
    namespace ms = frontend::mem_scan;
    SwapResult res;
    constexpr size_t CHUNK = size_t(1) << 20, OVERLAP = 256;
    const int64_t deadline = int64_t(GetTickCount64()) + 20000;
    std::string name = fit_name(name_in, std::min<size_t>(stock.size(), 15));
    if (name.empty() || name == stock || stock.size() > SWAP_MAX_STOCK)
        return res;
    NT_TIB *tib = reinterpret_cast<NT_TIB *>(NtCurrentTeb());
    uint64_t stack_lo = uint64_t(tib->StackLimit), stack_hi = uint64_t(tib->StackBase);
    std::vector<uint8_t> buf(CHUNK + OVERLAP);
    uint64_t scanned = 0;
    for (auto &r : rw_regions()) {
        for (uint64_t off = 0; off < r.size; off += CHUNK) {
            if (int64_t(GetTickCount64()) > deadline) {
                res.limit = true;
                res.scanned_mb = size_t(scanned >> 20);
                return res;
            }
            if (!swap_continues(runtime::session_life::time_trial_live(runtime::session_kind()), serial,
                                runtime::session_serial())) {
                res.stopped = true;
                res.scanned_mb = size_t(scanned >> 20);
                return res;
            }
            size_t n = size_t(std::min<uint64_t>(CHUNK + OVERLAP, r.size - off));
            if (!read_process(r.base + off, buf.data(), n))
                continue;
            scanned += std::min<uint64_t>(CHUNK, n);
            Sleep(0);
            std::vector<size_t> at;
            ms::find_all(buf.data(), n, stock, at, 256);
            for (size_t o : at) {
                if (o >= CHUNK && off + CHUNK < r.size)
                    continue;
                uint64_t where = r.base + off + o;
                if ((where >= uint64_t(buf.data()) && where < uint64_t(buf.data()) + buf.size()) ||
                    (where >= stack_lo && where < stack_hi) || where < 0x10000 + 16) {
                    res.skipped++;
                    continue;
                }
                uint8_t obj[32] = {};
                bool sso = false;
                if (read_process(where, obj, sizeof obj)) {
                    uint64_t size, cap;
                    std::memcpy(&size, obj + 16, 8);
                    std::memcpy(&cap, obj + 24, 8);
                    sso = size == stock.size() && cap == 15;
                    for (size_t i = stock.size(); sso && i < 16; i++)
                        sso = obj[i] == 0;
                }
                if (sso) {
                    uint8_t out[32] = {};
                    std::memcpy(out, name.data(), name.size());
                    uint64_t size = name.size(), cap = 15;
                    std::memcpy(out + 16, &size, 8);
                    std::memcpy(out + 24, &cap, 8);
                    if (write_process(where, out, sizeof out))
                        res.strings++;
                    continue;
                }
                uint8_t head[9] = {};
                if (!read_process(where - 9, head, 9))
                    continue;
                auto c = ms::classify(head, stock.size());
                if (!c.is_record)
                    continue;
                uint32_t word = 0;
                std::memcpy(&word, head + 6, 3);
                word = (word & ~0xfffffu) | uint32_t(name.size());
                std::string chars = name;
                chars.push_back('\0');
                if (write_process(where - 3, &word, 3) && write_process(where, chars.data(), chars.size()))
                    res.records++;
            }
        }
    }
    res.scanned_mb = size_t(scanned >> 20);
    return res;
}

}

bool registry_read(uint64_t at, void *out, size_t n)
{
    if (n > 64)
        return read_process(at, out, n);
    return swofty_core::hooks::guard::copy_memory(out, reinterpret_cast<const void *>(at), n);
}

void probe_registry(int pass)
{
    namespace ur = frontend::ui_registry;
    if (!runtime::proven(TT_NAME) || !swofty::game::g_Vm_Registry.address())
        return;
    uint64_t base = swofty::api->module_base(), reg = uint64_t(*swofty::game::g_Vm_Registry);
    ur::Layout lay = registry_layout(runtime::running_build(), base);
    char stock[96] = "Swooftyy";
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_STOCK", stock, sizeof stock);
    std::vector<ur::Hit> hits;
    ur::Stats st;
    auto rd = [](uint64_t at, void *out, size_t n) { return registry_read(at, out, n); };
    ur::find_string(rd, reg, lay, stock, hits, 40, st);
    swofty::log("[boards] player name probe %d: stock name \"%s\": %zu nodes (%zu object, %zu list, %zu string leaves of which "
                "%zu readable, "
                "%zu other kinds, %zu failed reads, %zu bad maps), root vtable base+0x%llx, %zu leaf(s) equal it",
                pass, stock, st.visited, st.objects, st.lists, st.string_leaves, st.strings_read, st.skipped_kinds,
                st.failed_reads, st.bad_maps, (unsigned long long)(st.root_vt - base), st.matches);
    for (auto v : st.unknown_vt)
        if (v)
            swofty::log("[boards] player name probe %d: other node class vtable base+0x%llx", pass,
                        (unsigned long long)(v - base));
    for (size_t i = 0; i < hits.size(); i++) {
        std::string path;
        for (int d = 0; d < hits[i].depth && d < ur::CHAIN; d++) {
            char h[16];
            snprintf(h, sizeof h, "%s%08x", d ? "." : "", hits[i].chain[d]);
            path += h;
        }
        swofty::log("[boards] player name probe %d: hit %zu node 0x%llx depth %d path(djb2) %s value \"%s\"", pass, i,
                    (unsigned long long)hits[i].node, hits[i].depth, path.c_str(), hits[i].value.c_str());
    }
}

void scan_copies(std::string stock)
{
    namespace ms = frontend::mem_scan;
    constexpr size_t CHUNK = size_t(1) << 20, OVERLAP = 256;
    const int64_t deadline = int64_t(GetTickCount64()) + 25000;
    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
    auto regions = rw_regions();
    std::vector<uint8_t> buf(CHUNK + OVERLAP);
    uint64_t base = swofty::api->module_base();
    std::string upper = stock;
    for (char &c : upper)
        c = char(toupper((unsigned char)c));
    std::vector<Variant> variants = {{stock, "ascii", false},
                                     {upper, "ascii upper case", false},
                                     {widen(stock), "utf-16", true},
                                     {widen(upper), "utf-16 upper case", true}};
    struct Hit {
        uint64_t at;
        bool record;
        int rc;
        unsigned type;
        const char *label;
        uint64_t region_base, region_size;
        std::string before;
    };
    std::vector<Hit> hits;
    uint64_t scanned = 0;
    size_t per_variant[4] = {};
    for (auto &r : regions) {
        for (uint64_t off = 0; off < r.size; off += CHUNK) {
            if (int64_t(GetTickCount64()) > deadline) {
                swofty::log("[boards] name copies: the scan stopped at its 25 s limit");
                goto scanned_all;
            }
            size_t n = size_t(std::min<uint64_t>(CHUNK + OVERLAP, r.size - off));
            if (!read_process(r.base + off, buf.data(), n))
                continue;
            scanned += std::min<uint64_t>(CHUNK, n);
            Sleep(0);
            for (size_t vi = 0; vi < variants.size(); vi++) {
                auto &v = variants[vi];
                if (per_variant[vi] >= 48)
                    continue;
                std::vector<size_t> at;
                ms::find_all(buf.data(), n, v.needle, at, 48);
                for (size_t o : at) {
                    if (o >= CHUNK && off + CHUNK < r.size)
                        continue;
                    uint64_t found_at = r.base + off + o;
                    if (found_at >= uint64_t(buf.data()) && found_at < uint64_t(buf.data()) + buf.size())
                        continue;
                    uint8_t head[0x50] = {};
                    bool rec = false;
                    int rc = 0;
                    std::string before;
                    if (found_at >= 0x10000 + sizeof head && read_process(found_at - sizeof head, head, sizeof head)) {
                        before = hex_of(head, sizeof head);
                        if (!v.wide) {
                            auto c = ms::classify(head + sizeof head - 9, v.needle.size());
                            rec = c.is_record;
                            rc = c.refcount;
                        }
                    }
                    hits.push_back({found_at, rec, rc, r.type, v.label, r.base, r.size, before});
                    per_variant[vi]++;
                }
            }
        }
    }
scanned_all:
    swofty::log("[boards] name copies: \"%s\" variants found %zu time(s) in %.0f MB of writable memory (%zu regions)",
                stock.c_str(), hits.size(), scanned / 1048576.0, regions.size());
    std::vector<uint64_t> targets;
    for (auto &h : hits) {
        if (h.record) {
            swofty::log("[boards] name copies: %s string record at 0x%llx refcount %d", h.label, (unsigned long long)h.at, h.rc);
            targets.push_back(h.at - 9);
            continue;
        }
        swofty::log("[boards] name copies: %s plain at 0x%llx (region 0x%llx size 0x%llx type 0x%x); 0x50 bytes before: %s",
                    h.label, (unsigned long long)h.at, (unsigned long long)h.region_base, (unsigned long long)h.region_size,
                    h.type, h.before.c_str());
        targets.push_back(h.at);
    }
    if (targets.empty())
        return;
    std::vector<std::pair<uint64_t, uint64_t>> refs;
    for (auto &r : regions) {
        for (uint64_t off = 0; off < r.size && refs.size() < 120; off += CHUNK) {
            if (int64_t(GetTickCount64()) > deadline + 15000)
                break;
            size_t n = size_t(std::min<uint64_t>(CHUNK, r.size - off));
            if (!read_process(r.base + off, buf.data(), n))
                continue;
            Sleep(0);
            ms::find_pointers(buf.data(), n, r.base + off, targets, refs, 120);
        }
    }
    swofty::log("[boards] name copies: %zu pointer(s) to those copies", refs.size());
    int shown_record_refs = 0;
    for (auto &p : refs) {
        bool to_record = false;
        for (auto &h : hits)
            if (h.record && p.second == h.at - 9)
                to_record = true;
        if (to_record && shown_record_refs++ >= 6)
            continue;
        uint8_t ctx[0x40] = {};
        uint64_t from = p.first >= 0x30 ? p.first - 0x30 : p.first;
        bool got = registry_read(from, ctx, sizeof ctx);
        bool in_image = p.first >= base && p.first < base + 0x20000000ull;
        swofty::log("[boards] name copies: 0x%llx -> 0x%llx%s%s; 0x30 bytes before it and 0x10 from it: %s",
                    (unsigned long long)p.first, (unsigned long long)p.second, in_image ? " (inside the game image)" : "",
                    to_record ? " (a string record)" : " (a PLAIN copy)", got ? hex_of(ctx, sizeof ctx).c_str() : "(unreadable)");
    }
}

int pin_names(const std::string &name, const std::string &stock)
{
    namespace ur = frontend::ui_registry;
    if (!swofty::game::g_Vm_Registry.address() || !swofty::game::Vm_Set_String.address() ||
        !swofty::game::Ne_Make_StrFromCStr.address())
        return 0;
    uint64_t base = swofty::api->module_base(), reg = uint64_t(*swofty::game::g_Vm_Registry);
    ur::Layout lay = registry_layout(runtime::running_build(), base);
    if (!lay.object_vt)
        return 0;
    std::vector<ur::Hit> hits;
    ur::Stats st;
    auto rd = [](uint64_t at, void *out, size_t n) { return registry_read(at, out, n); };
    ur::find_string(rd, reg, lay, stock, hits, 64, st);
    static std::atomic<int> logged{0};
    int written = 0;
    for (auto &h : hits) {
        const char *leaf = h.depth >= 1 ? frontend::player_name::leaf_name(h.chain[h.depth - 1]) : nullptr;
        if (!leaf || !h.parent || h.depth < 2 || !frontend::player_name::root_allowed(h.chain[0]))
            continue;
        frontend::vm::set_string(reinterpret_cast<void *>(reg), reinterpret_cast<void *>(h.parent + 8), leaf, name.c_str());
        written++;
    }
    if (written && logged.fetch_add(1) < 6)
        swofty::log("[boards] player name: set %d leaf(s) that read \"%s\" to \"%s\" (of %zu string leaves walked)", written,
                    stock.c_str(), name.c_str(), st.strings_read);
    return written;
}

// Sleeps until the pin pass hands it a job; scans once per job and goes back to sleep. Nothing runs on a timer.
void SourceSwap::swap_worker()
{
    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
    for (;;) {
        SwapJob job;
        {
            std::unique_lock<std::mutex> lk(swap_lock);
            swap_cv.wait(lk, [this] { return swap_queued; });
            job = swap_job;
            swap_queued = false;
        }
        Sleep(DWORD(SWAP_SETTLE_MS));
        SwapResult r;
        int64_t ms = 0;
        bool live = swap_continues(runtime::session_life::time_trial_live(runtime::session_kind()), job.serial,
                                   runtime::session_serial());
        if (live) {
            int64_t t0 = int64_t(GetTickCount64());
            r = swap_sources(job.stock, job.name, job.serial);
            ms = int64_t(GetTickCount64()) - t0;
        }
        {
            std::lock_guard<std::mutex> g(swap_lock);
            swap_plan.done_at = GetTickCount64();
        }
        swap_busy = false;
        if (!live)
            swofty::log("[boards] player name source swap (%s): skipped, the session ended first", swap_word(job.why));
        else
            swofty::log("[boards] player name source swap (%s): rewrote %zu string record(s) and %zu std::string object(s) that "
                        "read \"%s\" to \"%s\" "
                        "(source %s; scanned %zu MB in %lld ms%s, %zu own copies skipped)",
                        swap_word(job.why), r.records, r.strings, job.stock.c_str(), job.name.c_str(), job.source, r.scanned_mb,
                        (long long)ms,
                        r.limit     ? ", stopped at the time limit"
                        : r.stopped ? ", stopped as the session ended"
                                    : "",
                        r.skipped);
    }
}

void SourceSwap::plan_swap(bool live_ours, const std::string &stock, const std::string &name, const char *source, bool visible)
{
    char sw[8] = {};
    if (swap_busy.load() || (GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_SWAP", sw, sizeof sw) && !strcmp(sw, "0")))
        return;
    uint64_t serial = runtime::session_serial(), now = GetTickCount64();
    {
        std::lock_guard<std::mutex> g(swap_lock);
        SwapWhy why = swap_due(swap_plan, live_ours, serial, stock, name, visible, now);
        if (why == SwapWhy::none)
            return;
        swap_taken(swap_plan, why, serial, stock, name, now);
        swap_job = SwapJob{serial, stock, name, source, why};
        swap_queued = true;
        swap_busy = true;
    }
    std::call_once(swap_thread_once, [this] { std::thread([this] { swap_worker(); }).detach(); });
    swap_cv.notify_one();
}

}
