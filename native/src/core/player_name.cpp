#include "hooks/guard.hpp"
#include "main_menu_logic.hpp"
#include "mem_scan_logic.hpp"
#include "player_name_logic.hpp"
#include "ui_registry_walk.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/game.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "apply/moments.hpp"

#include <atomic>
#include <algorithm>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace runtime {
std::string hook_error(const safetyhook::InlineHook::Error &e);
}

// The player's own name in Time Trial. Every presence name the game shows (HUD, session cars, results) goes through the name
// cache lookup FUN_140816b10 (cache +0x30 keys, +0x5030 names, 0x81-byte slots; 256 entries, most recently used first; the
// callers include CreateHumanPlayers, Team_Build_SessionCars, ResolveGamerTags, SaveFullReplay): it returns a pointer to the
// name of the user id it is given. In a Time Trial session started with the board bridge on (tt.json "tt_bridge") and the
// launcher signed in (the same home_menu.json the main menu panel reads), the hook returns the account display name instead
// of the stock persona name; outside Time Trial, signed out, or for a name the HUD font cannot draw it returns the stock
// result untouched. Read-only: the name cache and every save are never written. SWOFTY_TT_PLAYER_NAME=0 turns it off.
//
// Unconfirmed in game: that the HUD's top-left name comes through this lookup (the log line below says whether the hook was
// hit and what it replaced).

namespace {

const runtime::Sig S_NAME{"Game_PresenceNameCache_Lookup",
                          "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 68 01 00 00 C5 F8 29 B4 24 50 01 00 00 48 89 D7 48 89 CE "
                          "8B 49 08 4C 8D 76 30 89 CD EB 31 48 89 D9",
                          0x816b10};

using LookupFn = const char *(*)(void *cache, void *id);

safetyhook::InlineHook h_lookup;
std::atomic<int> hits{0}, replaced{0}, busy{0};
char shown[runtime::player_name::MAX_BYTES + 1] = {};
std::string shown_for;
std::atomic<bool> bridge_on{false};
std::mutex account_lock;
std::string account_cached;
bool account_signed_in = false;

// The launcher account is read from home_menu.json on a worker thread (the main menu's own state is not loaded when the game
// starts through the chain entry); the lookup hook only copies the cached string.
void account_worker()
{
    for (;;) {
        auto state = runtime::main_menu::parse_state(runtime::read_file(runtime::swofty_root() + L"home_menu.json", size_t(1) << 20));
        {
            std::lock_guard<std::mutex> g(account_lock);
            account_signed_in = state.loaded && state.signed_in && !state.name.empty();
            account_cached = account_signed_in ? state.name : std::string();
        }
        Sleep(2000);
    }
}

bool account_name(std::string &out)
{
    std::lock_guard<std::mutex> g(account_lock);
    if (!account_signed_in)
        return false;
    out = account_cached;
    return true;
}

bool wanted_name(std::string &clean, const char *&source)
{
    char env[128] = {};
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME", env, sizeof env);
    std::string account;
    account_name(account);
    std::string text;
    if (!runtime::player_name::display_source(env, account, text))
        return false;
    source = env[0] ? "SWOFTY_TT_PLAYER_NAME" : "home_menu.json discord name";
    return runtime::player_name::sanitize(text, clean);
}

bool session_ours()
{
    return swofty_core::apply::moments::session_mode_track().first == "time_trial" && runtime::tt_bridge_ours_session();
}

std::mutex stock_lock;
std::string stock_learned;

const char *lookup_hook(void *cache, void *id)
{
    const char *stock = h_lookup.call<const char *>(cache, id);
    if (!bridge_on.load(std::memory_order_relaxed) || !stock || !id)
        return stock;
    if (busy.exchange(1))
        return stock;
    const char *result = stock;
    runtime::presence_id::Id me;
    uint8_t key[0x50] = {};
    bool ours = session_ours(), have_me = runtime::tt_bridge_presence_id(me), have_key = swofty_core::hooks::guard::copy_memory(key, id, sizeof key);
    bool local = ours && have_me && have_key && runtime::player_name::is_local_id(key, me);
    static std::atomic<int> traced{0};
    if (traced.fetch_add(1) < 12)
        swofty::log("[boards] player name: lookup %d: mode %s, our session %d, presence id %s, key %s, local player %d, stock \"%.40s\"", traced.load(),
                    swofty_core::apply::moments::session_mode_track().first.c_str(), ours ? 1 : 0, have_me ? "known" : "unknown",
                    have_key ? "read" : "unreadable", local ? 1 : 0, runtime::readable(stock, 4) ? stock : "?");
    if (local) {
        std::string clean;
        const char *source = "";
        char seen[runtime::player_name::MAX_BYTES + 1] = {};
        if (runtime::readable(stock, 4)) {
            strncpy(seen, stock, runtime::player_name::MAX_BYTES);
            std::lock_guard<std::mutex> g(stock_lock);
            stock_learned = seen;
        }
        if (wanted_name(clean, source)) {
            if (clean != shown_for) {
                std::memcpy(shown, clean.c_str(), clean.size() + 1);
                shown_for = clean;
            }
            result = shown;
            int n = replaced.fetch_add(1) + 1;
            if (n <= 3 || n == 50)
                swofty::log("[boards] player name: showing \"%s\" (source %s) for the local player's id, replacing \"%.40s\" in the name cache "
                            "lookup (call %d)",
                            clean.c_str(), source, seen, n);
        } else if (hits.fetch_add(1) == 0)
            swofty::log("[boards] player name: no usable name to show (empty, off, or characters the HUD font lacks); the stock name stays");
    }
    busy = 0;
    return result;
}

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

bool registry_read(uint64_t at, void *out, size_t n)
{
    if (n > 64)
        return read_process(at, out, n);
    return swofty_core::hooks::guard::copy_memory(out, reinterpret_cast<const void *>(at), n);
}

void probe_registry(int pass)
{
    namespace ur = runtime::ui_registry;
    if (!swofty::game::g_Vm_Registry.address())
        return;
    uint64_t base = swofty::api->module_base(), reg = uint64_t(*swofty::game::g_Vm_Registry);
    ur::Layout lay{base + 0x5a70080, base + 0x5a6ff90, base + 0x565f200, base + 0x5660370};
    char stock[96] = "Swooftyy";
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_STOCK", stock, sizeof stock);
    std::vector<ur::Hit> hits;
    ur::Stats st;
    auto rd = [](uint64_t at, void *out, size_t n) { return registry_read(at, out, n); };
    ur::find_string(rd, reg, lay, stock, hits, 40, st);
    swofty::log("[boards] player name probe %d: stock name \"%s\": %zu nodes (%zu object, %zu list, %zu string leaves of which %zu readable, "
                "%zu other kinds, %zu failed reads, %zu bad maps), root vtable base+0x%llx, %zu leaf(s) equal it",
                pass, stock, st.visited, st.objects, st.lists, st.string_leaves, st.strings_read, st.skipped_kinds, st.failed_reads,
                st.bad_maps, (unsigned long long)(st.root_vt - base), st.matches);
    for (auto v : st.unknown_vt)
        if (v)
            swofty::log("[boards] player name probe %d: other node class vtable base+0x%llx", pass, (unsigned long long)(v - base));
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

std::atomic<bool> scan_started{false};

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

void scan_copies(std::string stock)
{
    namespace ms = runtime::mem_scan;
    constexpr size_t CHUNK = size_t(1) << 20, OVERLAP = 256;
    const int64_t deadline = int64_t(GetTickCount64()) + 25000;
    SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_LOWEST);
    auto regions = rw_regions();
    std::vector<uint8_t> buf(CHUNK + OVERLAP);
    uint64_t base = swofty::api->module_base();
    std::string upper = stock;
    for (char &c : upper)
        c = char(toupper((unsigned char)c));
    std::vector<Variant> variants = {{stock, "ascii", false}, {upper, "ascii upper case", false}, {widen(stock), "utf-16", true},
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
    swofty::log("[boards] name copies: \"%s\" variants found %zu time(s) in %.0f MB of writable memory (%zu regions)", stock.c_str(), hits.size(),
                scanned / 1048576.0, regions.size());
    std::vector<uint64_t> targets;
    for (auto &h : hits) {
        if (h.record) {
            swofty::log("[boards] name copies: %s string record at 0x%llx refcount %d", h.label, (unsigned long long)h.at, h.rc);
            targets.push_back(h.at - 9);
            continue;
        }
        swofty::log("[boards] name copies: %s plain at 0x%llx (region 0x%llx size 0x%llx type 0x%x); 0x50 bytes before: %s", h.label, (unsigned long long)h.at,
                    (unsigned long long)h.region_base, (unsigned long long)h.region_size, h.type, h.before.c_str());
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
        swofty::log("[boards] name copies: 0x%llx -> 0x%llx%s%s; 0x30 bytes before it and 0x10 from it: %s", (unsigned long long)p.first, (unsigned long long)p.second,
                    in_image ? " (inside the game image)" : "", to_record ? " (a string record)" : " (a PLAIN copy)", got ? hex_of(ctx, sizeof ctx).c_str() : "(unreadable)");
    }
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
    bool limit = false;
};

// Rewrites, in place, every copy of the stock persona the game keeps as input: refcounted VM string records (length word at +6, characters
// from +9) and MSVC std::string objects in small-string form (16 byte buffer, u64 size, u64 capacity 15) inside private read-write memory.
// The game's consumers (entrant names, the timing tower and HUD leaves, the leaderboard rows) are all built from such copies. Only strings
// that read exactly the stock persona change; the new text is cut to the stock length (a record's or small string's storage is never
// grown, so nothing is allocated or freed). Reads and writes go through ReadProcessMemory / WriteProcessMemory, which fail instead of faulting.
SwapResult swap_sources(const std::string &stock, const std::string &name_in)
{
    namespace ms = runtime::mem_scan;
    SwapResult res;
    constexpr size_t CHUNK = size_t(1) << 20, OVERLAP = 256;
    const int64_t deadline = int64_t(GetTickCount64()) + 20000;
    std::string name = fit_name(name_in, std::min<size_t>(stock.size(), 15));
    if (name.empty() || name == stock || stock.size() > 15)
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
                if ((where >= uint64_t(buf.data()) && where < uint64_t(buf.data()) + buf.size()) || (where >= stack_lo && where < stack_hi) || where < 0x10000 + 16) {
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

void source_worker()
{
    static int logged = 0;
    for (;;) {
        Sleep(4000);
        char sw[8] = {};
        if (GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_SWAP", sw, sizeof sw) && !strcmp(sw, "0"))
            continue;
        std::string clean, stock;
        const char *source = "";
        char env[96] = {};
        GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_STOCK", env, sizeof env);
        {
            std::lock_guard<std::mutex> g(stock_lock);
            stock = env[0] ? std::string(env) : stock_learned;
        }
        runtime::presence_id::Id me;
        if (stock.empty() || !runtime::tt_bridge_presence_id(me) || !wanted_name(clean, source) || clean == stock)
            continue;
        int64_t t0 = int64_t(GetTickCount64());
        SwapResult r = swap_sources(stock, clean);
        int64_t ms = int64_t(GetTickCount64()) - t0;
        if ((r.records || r.strings || logged == 0) && logged < 40) {
            logged++;
            swofty::log("[boards] player name source swap: rewrote %zu string record(s) and %zu std::string object(s) that read \"%s\" to \"%s\" (source %s; scanned %zu MB in "
                        "%lld ms%s, %zu own copies skipped)",
                        r.records, r.strings, stock.c_str(), clean.c_str(), source, r.scanned_mb, (long long)ms, r.limit ? ", stopped at the time limit" : "", r.skipped);
        }
        Sleep(8000);
    }
}

void pin_names(const std::string &name, const std::string &stock)
{
    namespace ur = runtime::ui_registry;
    if (!swofty::game::g_Vm_Registry.address() || !swofty::game::Vm_Set_String.address() || !swofty::game::Ne_Make_StrFromCStr.address())
        return;
    uint64_t base = swofty::api->module_base(), reg = uint64_t(*swofty::game::g_Vm_Registry);
    ur::Layout lay{base + 0x5a70080, base + 0x5a6ff90, base + 0x565f200, base + 0x5660370};
    std::vector<ur::Hit> hits;
    ur::Stats st;
    auto rd = [](uint64_t at, void *out, size_t n) { return registry_read(at, out, n); };
    ur::find_string(rd, reg, lay, stock, hits, 64, st);
    static std::atomic<int> logged{0};
    int written = 0;
    for (auto &h : hits) {
        const char *leaf = h.depth >= 1 ? runtime::player_name::leaf_name(h.chain[h.depth - 1]) : nullptr;
        if (!leaf || !h.parent || h.depth < 2 || !runtime::player_name::root_allowed(h.chain[0]))
            continue;
        swofty::game::NeStr *str = nullptr;
        swofty::game::Ne_Make_StrFromCStr(&str, name.c_str(), 0, 0);
        swofty::game::Vm_Set_String(reinterpret_cast<void *>(reg), reinterpret_cast<void *>(h.parent + 8), leaf, &str, 1);
        swofty::game::Ne_Make_StrFromCStr(&str, nullptr, 0, 0);
        written++;
    }
    if (written && logged.fetch_add(1) < 6)
        swofty::log("[boards] player name: set %d leaf(s) that read \"%s\" to \"%s\" (of %zu string leaves walked)", written, stock.c_str(), name.c_str(),
                    st.strings_read);
}

void setup()
{
    auto gate = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
    char v[8] = {};
    bool off = GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME", v, sizeof v) && !strcmp(v, "0");
    if (!gate.is_object() || !gate["tt_bridge"].as_bool(false) || off || !runtime::same_build())
        return;
    uintptr_t at = runtime::find(S_NAME, "boards", "boards");
    if (!at) {
        swofty::log("[boards] player name: off, needs %s (byte signature matched nothing unique)", S_NAME.what);
        return;
    }
    auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(&lookup_hook));
    if (!h) {
        swofty::log("[boards] player name: off, the hook failed: %s", runtime::hook_error(h.error()).c_str());
        return;
    }
    h_lookup = std::move(*h);
    bridge_on = true;
    std::thread(account_worker).detach();
    std::thread(source_worker).detach();
    swofty::log("[boards] player name: hook armed at base+0x%llx; in Time Trial the launcher account's name replaces the stock name",
                (unsigned long long)(at - swofty::api->module_base()));
}

}

namespace runtime {

void player_name_probe_tick()
{
    static int64_t entered = 0;
    static int pass = 0;
    int64_t now = int64_t(GetTickCount64());
    if (swofty_core::apply::moments::session_mode_track().first != "time_trial") {
        entered = 0;
        pass = 0;
        return;
    }
    if (!entered)
        entered = now;
    static const int64_t at_ms[] = {8000, 40000};
    if (pass < 2 && now - entered >= at_ms[pass])
        probe_registry(++pass);
}

}

namespace runtime {

void player_name_pin_tick()
{
    static int64_t last = 0;
    static std::string last_decision;
    if (!bridge_on.load(std::memory_order_relaxed))
        return;
    int64_t now = int64_t(GetTickCount64());
    if (now - last < 3000)
        return;
    last = now;
    auto mode = swofty_core::apply::moments::session_mode_track().first;
    bool ours = session_ours();
    std::string clean, stock, why;
    const char *source = "";
    char env_name[128] = {}, env[96] = {};
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME", env_name, sizeof env_name);
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_STOCK", env, sizeof env);
    bool name_ok = wanted_name(clean, source);
    runtime::presence_id::Id me;
    bool have_me = runtime::tt_bridge_presence_id(me);
    {
        std::lock_guard<std::mutex> g(stock_lock);
        stock = env[0] ? std::string(env) : stock_learned;
    }
    if (mode != "time_trial")
        why = "not a Time Trial session";
    else if (!ours)
        why = "not our content (tt_bridge content check)";
    else if (!name_ok)
        why = !strcmp(env_name, "0") ? "SWOFTY_TT_PLAYER_NAME=0 (off)" : "no usable name (unset and not signed in, or characters the HUD font lacks)";
    else if (!have_me)
        why = "the presence id was not read yet (no board request has passed the gate)";
    if (why.empty() && stock.empty() && swofty::game::g_Vm_Registry.address()) {
        namespace ur = runtime::ui_registry;
        uint64_t base = swofty::api->module_base(), reg = uint64_t(*swofty::game::g_Vm_Registry);
        ur::Layout lay{base + 0x5a70080, base + 0x5a6ff90, base + 0x565f200, base + 0x5660370};
        std::vector<ur::Hit> all;
        ur::Stats st;
        auto rd = [](uint64_t at, void *out, size_t n) { return registry_read(at, out, n); };
        ur::find_string(rd, reg, lay, "", all, 8000, st, {}, true);
        uint32_t players = runtime::player_name::djb2("players"), player0 = runtime::player_name::djb2("player0"),
                 gamertag = runtime::player_name::djb2("gamertag");
        for (auto &h : all)
            if (h.depth == 3 && h.chain[0] == players && h.chain[1] == player0 && h.chain[2] == gamertag) {
                stock = h.value;
                std::lock_guard<std::mutex> g(stock_lock);
                stock_learned = stock;
                break;
            }
        if (stock.empty())
            why = "the stock persona was not found at players.player0.gamertag";
    }
    if (why.empty() && stock == clean)
        why = "the stock name already equals the name to show";
    char decision[400];
    snprintf(decision, sizeof decision, "mode %s, our session %d, name %s (%s), presence id %s, stock \"%.40s\": %s", mode.c_str(), ours ? 1 : 0,
             name_ok ? clean.c_str() : "-", name_ok ? source : "none", have_me ? "known" : "unknown", stock.c_str(), why.empty() ? "renaming" : why.c_str());
    if (decision != last_decision) {
        last_decision = decision;
        swofty::log("[boards] player name pass: %s", decision);
    }
    if (!why.empty())
        return;
    char diag[8] = {};
    if (GetEnvironmentVariableA("SWOFTY_TT_DIAG", diag, sizeof diag) && !strcmp(diag, "1") && !scan_started.exchange(true))
        std::thread([stock] { scan_copies(stock); }).detach();
    pin_names(clean, stock);
}

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("time trial player name", setup);
}
