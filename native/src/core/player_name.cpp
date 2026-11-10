#include "main_menu_logic.hpp"
#include "player_name_logic.hpp"
#include "runtime.hpp"
#include "ui_vm_probe.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "apply/moments.hpp"

#include <atomic>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>

namespace runtime {
std::string hook_error(const safetyhook::MidHook::Error &e);
}

// The player's own name in Time Trial. Findings so far (FINDINGS "Player name"): the presence name cache lookup return does not
// feed the top-left panel, and a hook on the global UI string setter crashed the game, so neither replaces anything now.
//
// SWOFTY_TT_DIAG=1 (read-only): a mid hook at the name cache lookup's entry only remembers the cache pointer (safetyhook saves
// and restores every register and keeps the stack aligned); once, some seconds into a Time Trial session, the tick (an existing
// game-thread tick, runtime::player_name_tick) reads the stock persona name from the cache and walks the UI view-model registry
// for string leaves equal to it, logging up to 40 hits as the chain of djb2 name hashes from the root plus the node address.
//
// SWOFTY_TT_PLAYER_NAME_LEAF=1 (default off, needs the probe's path): on the game thread, every 2 s in a Time Trial session with
// the launcher signed in, the string leaf named by SWOFTY_TT_PLAYER_NAME_PATH (a dotted registry path such as
// "hud.xxx.driver_name") is set to the account name with the game's own setter; the old -> new line is logged for the first
// writes. SWOFTY_TT_PLAYER_NAME=0 turns everything off before any hook is installed.

namespace {

const runtime::Sig S_NAME{"Game_PresenceNameCache_Lookup",
                          "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 68 01 00 00 C5 F8 29 B4 24 50 01 00 00 48 89 D7 48 89 CE "
                          "8B 49 08 4C 8D 76 30 89 CD EB 31 48 89 D9",
                          0x816b10};
constexpr uint64_t CACHE_NAMES = 0x5030, NAME_STRIDE = 0x81;
constexpr int CACHE_ENTRIES = 256, PROBE_DELAY_MS = 8000, PROBE_CAP = 40;

safetyhook::MidHook h_cache;
std::atomic<uintptr_t> cache_ptr{0};
bool diag_on = false, leaf_on = false, armed = false;
std::string leaf_path;
std::mutex account_lock;
std::string account_cached;
bool account_signed_in = false;

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

void cache_mid(safetyhook::Context &c)
{
    uintptr_t expected = 0;
    cache_ptr.compare_exchange_strong(expected, uintptr_t(c.rcx));
}

bool read_stock(std::string &stock)
{
    uintptr_t cache = cache_ptr.load();
    if (!cache)
        return false;
    for (int i = 0; i < CACHE_ENTRIES; i++) {
        uintptr_t at = cache + CACHE_NAMES + uintptr_t(i) * NAME_STRIDE;
        if (!runtime::readable(reinterpret_cast<void *>(at), NAME_STRIDE))
            return false;
        const char *p = reinterpret_cast<const char *>(at);
        size_t n = strnlen(p, NAME_STRIDE);
        if (n >= 1 && n < NAME_STRIDE) {
            stock.assign(p, n);
            return true;
        }
    }
    return false;
}

int64_t session_seen_ms = 0, last_tabs_ms = 0;
int last_tabs = -1;
bool probed = false;
int leaf_writes = 0;
int64_t last_leaf_ms = 0;

void probe()
{
    probed = true;
    std::string stock;
    if (!read_stock(stock)) {
        swofty::log("[boards] player name probe: no stock name in the name cache (cache 0x%llx)", (unsigned long long)cache_ptr.load());
        return;
    }
    UiProbeHit hits[PROBE_CAP];
    UiProbeStats st;
    size_t n = ui_vm_probe_strings(stock.c_str(), hits, PROBE_CAP, st);
    swofty::log("[boards] player name probe: stock name \"%s\": walked %zu nodes (%zu maps, %zu string leaves), %zu leaf(s) equal it", stock.c_str(),
                st.visited, st.maps, st.strings, st.matches);
    for (size_t i = 0; i < n; i++) {
        std::string chain;
        char b[16];
        for (int d = 0; d < hits[i].depth; d++) {
            snprintf(b, sizeof b, "%s%08x", d ? "." : "", hits[i].chain[d]);
            chain += b;
        }
        swofty::log("[boards] player name probe: hit %zu: node 0x%llx depth %d path(djb2 hashes) %s value \"%s\"", i + 1,
                    (unsigned long long)hits[i].node, hits[i].depth, chain.c_str(), hits[i].value);
    }
}

void leaf_write()
{
    std::string account, clean;
    if (!account_name(account) || !runtime::player_name::sanitize(account, clean))
        return;
    if (ui_vm_set_path(leaf_path, 0, clean.c_str()) && leaf_writes++ < 5)
        swofty::log("[boards] player name: wrote \"%s\" to the leaf %s (source home_menu.json discord name)", clean.c_str(), leaf_path.c_str());
    else if (leaf_writes == 0)
        swofty::log("[boards] player name: the leaf %s could not be resolved", leaf_path.c_str());
}

void setup()
{
    auto gate = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
    char v[256] = {};
    auto env = [&](const char *name) {
        v[0] = 0;
        return GetEnvironmentVariableA(name, v, sizeof v) && v[0];
    };
    bool off = env("SWOFTY_TT_PLAYER_NAME") && !strcmp(v, "0");
    diag_on = env("SWOFTY_TT_DIAG") && !strcmp(v, "1");
    leaf_on = env("SWOFTY_TT_PLAYER_NAME_LEAF") && !strcmp(v, "1");
    if (env("SWOFTY_TT_PLAYER_NAME_PATH"))
        leaf_path = v;
    if (!gate.is_object() || !gate["tt_bridge"].as_bool(false) || off || !runtime::same_build() || (!diag_on && !leaf_on))
        return;
    if (leaf_on && leaf_path.empty()) {
        swofty::log("[boards] player name: the leaf write is off, SWOFTY_TT_PLAYER_NAME_PATH is not set");
        leaf_on = false;
    }
    if (diag_on) {
        uintptr_t at = runtime::find(S_NAME, "boards", "boards");
        if (!at)
            swofty::log("[boards] player name probe: needs %s (byte signature matched nothing unique)", S_NAME.what);
        else if (auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), cache_mid))
            h_cache = std::move(*h);
        else
            swofty::log("[boards] player name probe: the cache mid hook failed: %s", runtime::hook_error(h.error()).c_str());
    }
    if (!diag_on && !leaf_on)
        return;
    armed = true;
    std::thread(account_worker).detach();
    swofty::log("[boards] player name: %s%s%s (existing game-thread tick; nothing is written unless the leaf write is on)",
                diag_on ? "probe armed" : "", diag_on && leaf_on ? " and " : "", leaf_on ? "leaf write armed" : "");
}

}

namespace runtime {

void player_name_tick(bool in_time_trial)
{
    if (!armed)
        return;
    int64_t now = int64_t(GetTickCount64());
    if (!in_time_trial) {
        session_seen_ms = 0;
        probed = false;
        return;
    }
    if (!session_seen_ms)
        session_seen_ms = now;
    if (diag_on && !probed && now - session_seen_ms >= PROBE_DELAY_MS)
        probe();
    if (diag_on && now - last_tabs_ms >= 3000) {
        last_tabs_ms = now;
        int g = ui_vm_node_exists("leaderboard_data.table.view_type_list.global") ? 1 : 0;
        int f = ui_vm_node_exists("leaderboard_data.table.view_type_list.friends") ? 1 : 0;
        if (g * 2 + f != last_tabs) {
            last_tabs = g * 2 + f;
            swofty::log("[boards] leaderboard screen: registry view_type_list entries global=%d friends=%d (the data patch that drops friends %s)", g, f,
                        g && !f ? "is in effect" : g && f ? "is NOT in effect" : "- screen not loaded");
        }
    }
    if (leaf_on && now - last_leaf_ms >= 2000) {
        last_leaf_ms = now;
        leaf_write();
    }
}

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("time trial player name", setup);
}
