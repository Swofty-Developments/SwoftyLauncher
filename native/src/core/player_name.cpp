#include "player_name_logic.hpp"
#include "runtime.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "apply/moments.hpp"

#include <atomic>
#include <cstring>
#include <string>

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

const char *lookup_hook(void *cache, void *id)
{
    const char *stock = h_lookup.call<const char *>(cache, id);
    if (!bridge_on.load(std::memory_order_relaxed) || !stock)
        return stock;
    if (busy.exchange(1))
        return stock;
    const char *result = stock;
    std::string account;
    if (swofty_core::apply::moments::session_mode_track().first == "time_trial" && runtime::main_menu_account(account)) {
        std::string clean;
        if (runtime::player_name::sanitize(account, clean)) {
            if (clean != shown_for) {
                std::memcpy(shown, clean.c_str(), clean.size() + 1);
                shown_for = clean;
            }
            result = shown;
            int n = replaced.fetch_add(1) + 1;
            if (n <= 3 || n == 50)
                swofty::log("[boards] player name: showing \"%s\" (source home_menu.json discord name), replacing \"%.40s\" in the "
                            "name cache lookup (call %d)",
                            clean.c_str(), runtime::readable(stock, 4) ? stock : "?", n);
        } else if (hits.fetch_add(1) == 0)
            swofty::log("[boards] player name: the account name \"%s\" has characters the HUD font lacks; the stock name stays",
                        account.c_str());
    }
    busy = 0;
    return result;
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
    swofty::log("[boards] player name: hook armed at base+0x%llx; in Time Trial the launcher account's name replaces the stock name",
                (unsigned long long)(at - swofty::api->module_base()));
}

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("time trial player name", setup);
}
