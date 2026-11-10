#include "hooks/guard.hpp"
#include "main_menu_logic.hpp"
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
    uint8_t key[0x50];
    if (session_ours() && runtime::tt_bridge_presence_id(me) &&
        swofty_core::hooks::guard::copy_memory(key, id, sizeof key) && runtime::player_name::is_local_id(key, me)) {
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

bool registry_read(uint64_t at, void *out, size_t n)
{
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
    if (!bridge_on.load(std::memory_order_relaxed))
        return;
    int64_t now = int64_t(GetTickCount64());
    if (now - last < 3000)
        return;
    last = now;
    if (!session_ours())
        return;
    std::string clean, stock;
    const char *source = "";
    char env[96] = {};
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_STOCK", env, sizeof env);
    {
        std::lock_guard<std::mutex> g(stock_lock);
        stock = env[0] ? std::string(env) : stock_learned;
    }
    if (stock.empty() || !wanted_name(clean, source) || clean == stock)
        return;
    pin_names(clean, stock);
}

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("time trial player name", setup);
}
