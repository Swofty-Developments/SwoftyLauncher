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
#include <mutex>
#include <string>

namespace runtime {
std::string hook_error(const safetyhook::InlineHook::Error &e);
}


// The player's own name in Time Trial. Round 1 replaced the return of the presence name cache lookup (FUN_140816b10) and the
// HUD kept the stock name, so the HUD does not read that return. The lookup hook now only learns the stock persona name
// (the first lookup's result). The display-side hook is Vm_NodeSet_String (0x140828440), the one setter of every UI string
// node: when the value record is exactly the stock name, in a Time Trial session with the board bridge on (tt.json
// "tt_bridge") and the launcher signed in (home_menu.json, the main menu's source), the node gets an interned copy of the
// account display name made by the game's own Ne_Make_StrFromCStr. Everything else, outside Time Trial, signed out, or a name
// the HUD font cannot draw, passes through untouched. Nothing is written to the name cache or to any save: only the UI node's
// value changes. SWOFTY_TT_PLAYER_NAME=0 turns it off.

namespace {

const runtime::Sig S_NAME{"Game_PresenceNameCache_Lookup",
                          "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 68 01 00 00 C5 F8 29 B4 24 50 01 00 00 48 89 D7 48 89 CE "
                          "8B 49 08 4C 8D 76 30 89 CD EB 31 48 89 D9",
                          0x816b10};

const runtime::Sig S_SETSTR{"Vm_NodeSet_String",
                            "41 56 56 57 55 53 48 81 EC 20 03 00 00 F6 41 26 01 75 31 44 89 CB 44 89 C7 49 89 D6 48 89 CE 48 8B 49 "
                            "58 48 8B 01 FF 50 38 84 DB 0F 85 87",
                            0x828440};

// A VM string value is a refcounted record: refcount i16 at +4, length in the low 20 bits of the u32 at +6, characters from +9.
using MakeStrFn = uint32_t (*)(void **out, const char *s, uint32_t flags, uintptr_t len);
using SetStrFn = uint64_t (*)(void *node, void **value, uint32_t a, uint32_t b);
constexpr uintptr_t RVA_MAKE_STR = 0x19974e0;
MakeStrFn make_str = nullptr;
safetyhook::InlineHook h_setstr;
char stock[runtime::player_name::MAX_BYTES + 1] = {};
std::atomic<size_t> stock_len{0};
std::mutex make_lock;
void *replacement = nullptr;
std::string replacement_for;
std::atomic<int> display_hits{0}, display_misses{0};

safetyhook::InlineHook h_lookup;
std::atomic<bool> bridge_on{false};

const char *lookup_hook(void *cache, void *id)
{
    const char *stock_name = h_lookup.call<const char *>(cache, id);
    if (bridge_on.load(std::memory_order_relaxed) && stock_name && stock_len.load() == 0 && runtime::readable(stock_name, 2)) {
        size_t n = strnlen(stock_name, sizeof stock - 1);
        std::memcpy(stock, stock_name, n);
        stock[n] = 0;
        stock_len = n;
        swofty::log("[boards] player name: the stock persona name is \"%s\" (first name cache lookup)", stock);
    }
    return stock_name;
}

uint64_t setstr_hook(void *node, void **value, uint32_t a, uint32_t b)
{
    size_t want = stock_len.load(std::memory_order_relaxed);
    if (want && value && *value) {
        const uint8_t *rec = static_cast<const uint8_t *>(*value);
        if (!runtime::readable(rec, 9 + want + 1))
            return h_setstr.call<uint64_t>(node, value, a, b);
        uint32_t len;
        std::memcpy(&len, rec + 6, 4);
        len &= 0xfffff;
        if (len == want && std::memcmp(rec + 9, stock, want) == 0) {
            std::string account, clean;
            bool tt = swofty_core::apply::moments::session_mode_track().first == "time_trial";
            if (tt && runtime::main_menu_account(account) && runtime::player_name::sanitize(account, clean)) {
                std::lock_guard<std::mutex> g(make_lock);
                if (clean != replacement_for || !replacement) {
                    void *made = nullptr;
                    if (make_str(&made, clean.c_str(), 0, clean.size()) == 0 && made) {
                        replacement = made;
                        replacement_for = clean;
                    }
                }
                if (replacement) {
                    int n = display_hits.fetch_add(1) + 1;
                    if (n <= 6)
                        swofty::log("[boards] player name: showing \"%s\" (source home_menu.json discord name) in a UI string at node "
                                    "0x%llx, was \"%s\" (value record 0x%llx, write %d)",
                                    clean.c_str(), (unsigned long long)uintptr_t(node), stock, (unsigned long long)uintptr_t(*value), n);
                    void *use = replacement;
                    return h_setstr.call<uint64_t>(node, &use, a, b);
                }
            } else if (display_misses.fetch_add(1) < 5)
                swofty::log("[boards] player name: a UI string holds the stock name \"%s\" but is left alone (time trial %d, account name %s)",
                            stock, tt ? 1 : 0, account.empty() ? "unknown" : "ok");
        }
    }
    return h_setstr.call<uint64_t>(node, value, a, b);
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
    make_str = reinterpret_cast<MakeStrFn>(swofty::api->module_base() + RVA_MAKE_STR);
    if (uintptr_t ss = runtime::find(S_SETSTR, "boards", "boards")) {
        auto sh = safetyhook::InlineHook::create(reinterpret_cast<void *>(ss), reinterpret_cast<void *>(&setstr_hook));
        if (sh)
            h_setstr = std::move(*sh);
        else
            swofty::log("[boards] player name: the UI string hook failed: %s", runtime::hook_error(sh.error()).c_str());
    } else
        swofty::log("[boards] player name: the UI string hook needs %s (byte signature matched nothing unique)", S_SETSTR.what);
    bridge_on = true;
    swofty::log("[boards] player name: hook armed at base+0x%llx; in Time Trial the launcher account's name replaces the stock name",
                (unsigned long long)(at - swofty::api->module_base()));
}

}

SWOFTY_ON_LOAD
{
    runtime::when_code_ready("time trial player name", setup);
}
