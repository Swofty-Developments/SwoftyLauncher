#include "core/hooks/guard.hpp"
#include "core/frontend/main_menu_logic.hpp"
#include "core/frontend/player_name/copies.hpp"
#include "core/frontend/player_name_logic.hpp"
#include "core/frontend/ui_registry_walk.hpp"
#include "core/runtime/hook_errors.hpp"
#include "core/runtime/runtime.hpp"
#include "core/services/lap_host.hpp"

#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/game.hpp>
#include <swofty/json.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "core/apply/moments.hpp"
#include "core/runtime/loader/core_rank.hpp"

#include <atomic>
#include <algorithm>
#include <cstring>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

// The player's own name in Time Trial. Every presence name the game shows (HUD, session cars, results) goes through the name
// cache lookup FUN_140816b10 (cache +0x30 keys, +0x5030 names, 0x81-byte slots; 256 entries, most recently used first; the
// callers include CreateHumanPlayers, Team_Build_SessionCars, ResolveGamerTags, SaveFullReplay): it returns a pointer to the
// name of the user id it is given. In a Time Trial session started with the board bridge on (tt.json "tt_bridge") and the
// launcher signed in (the same home_menu.json the main menu panel reads), the hook returns the account display name instead
// of the stock persona name; outside Time Trial, signed out, or for a name the HUD font cannot draw it returns the stock
// result untouched. Read-only: the name cache and every save are never written. SWOFTY_TT_PLAYER_NAME=0 turns it off.
// The rename is armed for one session at a time (session load and enter, then the 3 s pin pass from the ghost tick); an
// unarmed or other-mode lookup costs atomic loads only.
//
// Unconfirmed in game: that the HUD's top-left name comes through this lookup (the log line below says whether the hook was
// hit and what it replaced).

namespace frontend::player_name {

namespace {

const runtime::Sig S_NAME{"Game_PresenceNameCache_Lookup",
                          "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 68 01 00 00 C5 F8 29 B4 24 50 01 00 00 48 89 D7 48 89 CE "
                          "8B 49 08 4C 8D 76 30 89 CD EB 31 48 89 D9",
                          0x816b10};

// The launcher account is read from home_menu.json on a worker thread (the main menu's own state is not loaded when the game
// starts through the chain entry); the file is read again only when its write time changes, and not during a session of
// another mode. The lookup hook never reads it.
class LauncherAccount {
public:
    void watch();
    bool name(std::string &out);

private:
    std::mutex lock;
    std::string cached;
    bool signed_in = false;
};

void LauncherAccount::watch()
{
    const std::wstring path = runtime::swofty_root() + L"home_menu.json";
    FILETIME stamp{};
    bool read_once = false, had = false;
    for (;;) {
        if (runtime::session_kind() != runtime::session_life::OTHER) {
            WIN32_FILE_ATTRIBUTE_DATA d{};
            bool have = GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &d) != 0;
            bool same = have && had && CompareFileTime(&d.ftLastWriteTime, &stamp) == 0;
            if (account_reread(read_once, had, have, same)) {
                auto state = frontend::main_menu::parse_state(have ? runtime::read_file(path, size_t(1) << 20) : std::string());
                {
                    std::lock_guard<std::mutex> g(lock);
                    signed_in = state.loaded && state.signed_in && !state.name.empty();
                    cached = signed_in ? state.name : std::string();
                }
                read_once = true;
                had = have;
                stamp = have ? d.ftLastWriteTime : FILETIME{};
            }
        }
        Sleep(2000);
    }
}

bool LauncherAccount::name(std::string &out)
{
    std::lock_guard<std::mutex> g(lock);
    if (!signed_in)
        return false;
    out = cached;
    return true;
}

bool session_ours()
{
    return swofty_core::apply::moments::session_mode_track().first == "time_trial" && runtime::lap_host::board_session();
}

class TimeTrialName {
public:
    void setup();
    const char *lookup(void *cache, void *id);
    void probe_tick();
    void pin_tick();
    void session();

private:
    bool wanted_name(std::string &clean, const char *&source);
    bool arm_rename(uint64_t serial);

    safetyhook::InlineHook h_lookup;
    std::atomic<int> replaced{0}, busy{0};
    std::atomic<bool> bridge_on{false};
    LauncherAccount account;

    std::mutex stock_lock;
    std::string stock_learned;
    std::atomic<uint64_t> stock_serial{~0ull};

    // What the lookup hook needs, published by arm_rename (at session load and enter, and by the 3 s pin pass): the local
    // presence id packed into two words, the session it is armed for, and the name to show in one of two buffers.
    std::mutex rename_lock;
    std::atomic<bool> rename_armed{false};
    std::atomic<uint64_t> rename_serial{0}, rename_value{0};
    std::atomic<uint32_t> rename_tag{0};
    char shown_buf[2][frontend::player_name::MAX_BYTES + 1] = {};
    std::atomic<int> shown_slot{0};
    std::atomic<const char *> shown_source{""};

    std::atomic<bool> scan_started{false};
    int64_t probe_entered = 0;
    int probe_pass = 0;
    int64_t pin_last = 0;
    std::string last_decision;
};

// Not constinit: safetyhook's hooks hold a std::recursive_mutex, whose constructor is not constexpr.
TimeTrialName tt_name;

const char *lookup_hook(void *cache, void *id) { return tt_name.lookup(cache, id); }

bool TimeTrialName::wanted_name(std::string &clean, const char *&source)
{
    char env[128] = {};
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME", env, sizeof env);
    std::string name;
    account.name(name);
    std::string text;
    if (!frontend::player_name::display_source(env, name, text))
        return false;
    source = env[0] ? "SWOFTY_TT_PLAYER_NAME" : "home_menu.json discord name";
    return frontend::player_name::sanitize(text, clean);
}

bool TimeTrialName::arm_rename(uint64_t serial)
{
    std::lock_guard<std::mutex> g(rename_lock);
    std::string clean;
    const char *source = "";
    runtime::presence_id::Id me;
    bool ok = bridge_on.load(std::memory_order_relaxed) && runtime::session_life::time_trial_live(runtime::session_kind()) &&
              runtime::lap_host::board_session() && runtime::lap_host::player_id(me) && wanted_name(clean, source);
    if (!ok) {
        rename_armed.store(false, std::memory_order_release);
        return false;
    }
    int slot = shown_slot.load(std::memory_order_relaxed);
    if (clean != shown_buf[slot]) {
        slot ^= 1;
        std::memcpy(shown_buf[slot], clean.c_str(), clean.size() + 1);
        shown_slot.store(slot, std::memory_order_release);
    }
    shown_source.store(source, std::memory_order_relaxed);
    rename_value.store(me.value[0], std::memory_order_relaxed);
    rename_tag.store(id_tag(me), std::memory_order_relaxed);
    rename_serial.store(serial, std::memory_order_relaxed);
    rename_armed.store(true, std::memory_order_release);
    return true;
}

// Outside an armed Time Trial session, and for every id that is not the local presence's, the hook returns the game's own
// result after atomic loads and one guarded copy of the key: no lock, no memory check, no string copy.
const char *TimeTrialName::lookup(void *cache, void *id)
{
    const char *stock = h_lookup.call<const char *>(cache, id);
    if (!stock || !id || !rename_armed.load(std::memory_order_acquire))
        return stock;
    uint64_t serial = runtime::session_serial();
    if (!lookup_renames(runtime::session_life::time_trial_live(runtime::session_kind()), true,
                        rename_serial.load(std::memory_order_relaxed), serial))
        return stock;
    uint8_t key[0x50];
    if (!swofty_core::hooks::guard::copy_memory(key, id, sizeof key))
        return stock;
    if (!is_local_id(key, id_of(rename_tag.load(std::memory_order_relaxed), rename_value.load(std::memory_order_relaxed))))
        return stock;
    const char *result = shown_buf[shown_slot.load(std::memory_order_acquire)];
    if (busy.exchange(1))
        return result;
    int n = replaced.fetch_add(1) + 1;
    bool learn = stock_serial.load(std::memory_order_relaxed) != serial;
    if (learn || n <= 3 || n == 50) {
        char seen[frontend::player_name::MAX_BYTES + 1] = {};
        {
            swofty::mem::Trust trust(runtime::HEAP_TRUST_MS);
            if (swofty::mem::readable(stock, 4))
                strncpy(seen, stock, frontend::player_name::MAX_BYTES);
        }
        if (learn && seen[0]) {
            std::lock_guard<std::mutex> g(stock_lock);
            stock_learned = seen;
            stock_serial.store(serial, std::memory_order_relaxed);
        }
        if (n <= 3 || n == 50)
            swofty::log("[boards] player name: showing \"%s\" (source %s) for the local player's id, replacing \"%.40s\" in the "
                        "name cache "
                        "lookup (call %d)",
                        result, shown_source.load(std::memory_order_relaxed), seen[0] ? seen : "?", n);
    }
    busy = 0;
    return result;
}

void TimeTrialName::setup()
{
    auto gate = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
    char v[8] = {};
    bool off = GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME", v, sizeof v) && !strcmp(v, "0");
    if (!gate.is_object() || !gate["tt_bridge"].as_bool(false) || off || !runtime::holds(frontend::player_name::TT_NAME))
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
    std::thread([this] { account.watch(); }).detach();
    swofty::log(
        "[boards] player name: hook armed at base+0x%llx; in Time Trial the launcher account's name replaces the stock name",
        (unsigned long long)(at - swofty::api->module_base()));
}

void TimeTrialName::probe_tick()
{
    int64_t now = int64_t(GetTickCount64());
    if (swofty_core::apply::moments::session_mode_track().first != "time_trial") {
        probe_entered = 0;
        probe_pass = 0;
        return;
    }
    if (!probe_entered)
        probe_entered = now;
    static constexpr int64_t at_ms[] = {8000, 40000};
    if (probe_pass < 2 && now - probe_entered >= at_ms[probe_pass])
        frontend::player_name::probe_registry(++probe_pass);
}

void TimeTrialName::pin_tick()
{
    if (!bridge_on.load(std::memory_order_relaxed))
        return;
    int64_t now = int64_t(GetTickCount64());
    if (now - pin_last < 3000)
        return;
    pin_last = now;
    auto mode = swofty_core::apply::moments::session_mode_track().first;
    bool ours = session_ours();
    std::string clean, stock, why;
    const char *source = "";
    char env_name[128] = {}, env[96] = {};
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME", env_name, sizeof env_name);
    GetEnvironmentVariableA("SWOFTY_TT_PLAYER_NAME_STOCK", env, sizeof env);
    bool name_ok = wanted_name(clean, source);
    runtime::presence_id::Id me;
    bool have_me = runtime::lap_host::player_id(me);
    {
        std::lock_guard<std::mutex> g(stock_lock);
        stock = env[0] ? std::string(env) : stock_learned;
    }
    if (mode != "time_trial")
        why = "not a Time Trial session";
    else if (!ours)
        why = "not our content (tt_bridge content check)";
    else if (!name_ok)
        why = !strcmp(env_name, "0") ? "SWOFTY_TT_PLAYER_NAME=0 (off)"
                                     : "no usable name (unset and not signed in, or characters the HUD font lacks)";
    else if (!have_me)
        why = "the presence id was not read yet (no board request has passed the gate)";
    if (why.empty() && stock.empty() && swofty::game::g_Vm_Registry.address()) {
        namespace ur = frontend::ui_registry;
        uint64_t base = swofty::api->module_base(), reg = uint64_t(*swofty::game::g_Vm_Registry);
        ur::Layout lay = frontend::player_name::registry_layout(runtime::running_build(), base);
        std::vector<ur::Hit> all;
        ur::Stats st;
        auto rd = [](uint64_t at, void *out, size_t n) { return frontend::player_name::registry_read(at, out, n); };
        ur::find_string(rd, reg, lay, "", all, 8000, st, {}, true);
        uint32_t players = frontend::player_name::djb2("players"), player0 = frontend::player_name::djb2("player0"),
                 gamertag = frontend::player_name::djb2("gamertag");
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
    arm_rename(runtime::session_serial());
    char decision[400];
    snprintf(decision, sizeof decision, "mode %s, our session %d, name %s (%s), presence id %s, stock \"%.40s\": %s",
             mode.c_str(), ours ? 1 : 0, name_ok ? clean.c_str() : "-", name_ok ? source : "none", have_me ? "known" : "unknown",
             stock.c_str(), why.empty() ? "renaming" : why.c_str());
    if (decision != last_decision) {
        last_decision = decision;
        swofty::log("[boards] player name pass: %s", decision);
    }
    if (!why.empty())
        return;
    char diag[8] = {};
    if (GetEnvironmentVariableA("SWOFTY_TT_DIAG", diag, sizeof diag) && !strcmp(diag, "1") && !scan_started.exchange(true))
        std::thread([stock] { frontend::player_name::scan_copies(stock); }).detach();
    bool visible = frontend::player_name::pin_names(clean, stock) > 0;
    frontend::player_name::source_swap.plan_swap(true, stock, clean, source, visible);
}

void TimeTrialName::session()
{
    if (bridge_on.load(std::memory_order_relaxed))
        arm_rename(runtime::session_serial());
}

void setup() { tt_name.setup(); }

}

}

namespace runtime {

void player_name_probe_tick() { frontend::player_name::tt_name.probe_tick(); }

void player_name_pin_tick() { frontend::player_name::tt_name.pin_tick(); }

void player_name_session() { frontend::player_name::tt_name.session(); }

}

SWOFTY_CORE_RANK(player_name)

SWOFTY_ON_LOAD { runtime::when_code_ready("time trial player name", frontend::player_name::setup); }
