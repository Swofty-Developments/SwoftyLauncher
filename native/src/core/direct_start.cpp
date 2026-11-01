#include "runtime.hpp"

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <mutex>

#include "direct_start_logic.hpp"
#include "json.hpp"
#include "title_skip.hpp"

// The direct start: the game goes from boot to its target with no title, no legal screens' waits and no
// menu. SWOFTY_DIRECT_START in the game's environment, or swofty\direct_start.json written by the launcher for
// this launch, names the target: a session,
// or a room (netplay_drive.cpp). The title engages by itself (title_skip.cpp), which loads the profile;
// the target sets itself up on the flow thread; then the flow leaves the front end from StateFTUE, whose
// finished link is held until then and replaced by its exit link, so StateFrontend (the menu) never starts.
// StateFrontendWorld, with no child left, then takes the target's link ('session' leads to StateLaunch's
// path and StateLoadSession).
//
// The flow machine (Vm_Update_FlowStateMachine 0x144423ce0) calls each state's onStateUpdate (vt+0x20,
// the shared wrapper that updates children first) at 0x144424c75 with rdi = the state and the returned
// link written to [rsp+0x40]; a mid hook on the next instruction (0x144424c78, where it compares that
// link with g_Game_FlowLinkStay) sees every state's link and can replace it. State nodes: +0xb0 the
// name's djb2 hash, +0xbd8 the active child.

namespace runtime {
std::string hook_error(const safetyhook::MidHook::Error &e);
}

namespace {

namespace ds = runtime::direct_start;

const runtime::Sig SIG_FLOW_LINK{"vm.flow_state_update_link",
                                 "48 8B 07 48 8D 54 24 40 48 89 F9 C5 F9 28 D6 FF 50 20 8B 44 24 40 39 05", 0x4424c78,
                                 runtime::Sig::addr, 0, 0x12};
const runtime::Sig SIG_GAME_MANAGER{"game.manager", "48 8B B8 98 01 00 00 48 85 FF 0F 84 ?? ?? ?? ?? 48 8D 4C 24 70 E8 ?? ?? ?? ?? 48 8B 05", 0x604c2c8, runtime::Sig::riprel, 26, 29, 7};
const runtime::Sig SIG_SET_MODE{"game.game_set_current_game_mode", "41 56 56 57 55 53 48 81 EC 80 00 00 00 48 8B 81 88 00 00 00", 0x7d07a0};
const runtime::Sig SIG_MODE_BY_NAME{"game.game_modes_find_by_name",
                                    "48 8B 89 A0 02 00 00 48 8D 15 ?? ?? ?? ?? E8 ?? ?? ?? ?? 48 85 C0 74 20 48 89 C7 48 89 C1 48 81 C1 30 10 00 00",
                                    0x7d0de0, runtime::Sig::rel32, 14, 15, 5};
const runtime::Sig SIG_MODE_BY_PREDICATE{"game.game_modes_find_by_predicate",
                                         "48 8D 54 24 28 E8 ?? ?? ?? ?? 48 85 C0 74 22 48 89 C7 48 8B 8E 08 03 00 00",
                                         0x7d2280, runtime::Sig::rel32, 5, 6, 5};
const runtime::Sig SIG_GP_PREDICATE{"game.grand_prix_mode_predicate",
                                    "48 8D 05 ?? ?? ?? ?? 48 89 44 24 28 48 8D 54 24 28 E8 ?? ?? ?? ?? 48 85 C0 74 22",
                                    0x77cff0, runtime::Sig::riprel, 0, 3, 7};
const runtime::Sig SIG_ROUNDS_PREPARE{"game.championship_rounds_prepare", "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 48 80 B9 C0 02", 0xbc3900};

// The Grand Prix modes a Continue picks from, in the order GrandPrixContinueViewModel's lookup
// (Game_GrandPrixContinueVm_FindModeEntity 0x141bccc00) tries them: the first whose entity's component
// registry (+0x1030) reports a season (slot 0x78 == 1).
constexpr const char *SEASON_MODES[] = {"F2_2026_Championship_Season", "F2_2025_Championship_Season",
                                        "F2_2024_Championship_Season", "F1_Championship_Season",
                                        "F1_2026_Championship_Season"};
constexpr uintptr_t MANAGER_MODES = 0xc0, ENTITY_REGISTRY = 0x1030, ENTITY_MODE = 0xf08;
constexpr uintptr_t ROUNDS_COMPONENT = 0x605ad20;
constexpr uint64_t CONTINUE_WAIT_MS = 20000;

using ModeByName_t = uint8_t *(void *modes, const char *name);
using ModeByPredicate_t = uint8_t *(void *modes, void *const *predicate);
using SetMode_t = void(void *modes, void *mode);
using RoundsPrepare_t = void(void *component);

struct Game {
    uintptr_t manager = 0, set_mode = 0, by_name = 0, by_predicate = 0, gp_predicate = 0, prepare = 0;
} game;

// The game modes context SetCurrentGameMode takes, as the game passes it (it sets the mode once at boot).
safetyhook::InlineHook h_set_mode;
std::atomic<void *> modes_seen{nullptr};

void set_mode_detour(void *modes, void *mode)
{
    if (modes && !modes_seen.exchange(modes))
        swofty::log("[direct] the game modes context is %p (SetCurrentGameMode, mode %p)", modes, mode);
    h_set_mode.call<void>(modes, mode);
}

constexpr size_t STATE_HASH = 0xb0, STATE_CHILD = 0xbd8, LINK_SLOT = 0x40;

std::mutex lock;
ds::Driver driver;
json::Value request;
safetyhook::MidHook h_link;
std::atomic<bool> armed{false};
ds::Phase logged = ds::Phase::off;

void note_phase()
{
    if (driver.phase == logged)
        return;
    logged = driver.phase;
    if (driver.phase == ds::Phase::failed)
        swofty::log("[direct] %s start failed: %s; the menu opens as usual", driver.target.name.c_str(),
                    driver.why.c_str());
    else
        swofty::log("[direct] %s start: %s", driver.target.name.c_str(), ds::phase_name(driver.phase));
}

void link_mid(safetyhook::Context &c)
{
    if (!armed.load())
        return;
    auto *state = reinterpret_cast<uint8_t *>(c.rdi);
    if (!state || !runtime::readable(state, STATE_CHILD + 8))
        return;
    auto *slot = reinterpret_cast<uint32_t *>(c.rsp + LINK_SLOT);
    uint32_t hash = *reinterpret_cast<uint32_t *>(state + STATE_HASH);
    bool child = *reinterpret_cast<void **>(state + STATE_CHILD) != nullptr;
    std::lock_guard<std::mutex> g(lock);
    uint32_t link = *slot, out = driver.on_update(hash, link, child);
    if (out != link) {
        *slot = out;
        if (out != ds::LINK_STAY)
            swofty::log("[direct] state %08x: link %08x replaced by %08x", hash, link, out);
    }
    note_phase();
}

void *registry_call(uint8_t *entity, size_t slot, const void *a2, int a3)
{
    auto *reg = entity + ENTITY_REGISTRY;
    auto **vt = *reinterpret_cast<void ***>(reg);
    return reinterpret_cast<void *(*)(void *, const void *, int)>(vt[slot / 8])(reg, a2, a3);
}

// A Grand Prix Continue as GrandPrixContinueViewModel's press runs it (0x141bce150): the mode entity, the
// rounds component's prepare, then the mode is made current. The save slot is current from the profile load.
ds::Setup continue_grand_prix(std::string &why)
{
    if (!game.manager || !game.set_mode || !game.by_name || !game.prepare) {
        why = "the game manager, SetCurrentGameMode, the game mode lookup or the rounds prepare did not resolve";
        return ds::Setup::failed;
    }
    void *mgr = *reinterpret_cast<void **>(game.manager);
    void *from_mgr = mgr && runtime::readable(mgr, MANAGER_MODES + 8)
                         ? *reinterpret_cast<void **>(static_cast<uint8_t *>(mgr) + MANAGER_MODES)
                         : nullptr;
    void *modes = modes_seen.load();
    static bool told = false;
    if (!told) {
        told = true;
        swofty::log("[direct] Grand Prix: game manager %p, its +0xc0 %p, the context SetCurrentGameMode saw %p", mgr,
                    from_mgr, modes);
    }
    if (!modes)
        modes = from_mgr;
    if (!modes)
        return ds::Setup::pending;
    static bool grand_prix_set = false;
    static uint8_t *grand_prix_entity = nullptr;
    if (!grand_prix_set) {
        // The F1 World Grand Prix tile (0x141a78329): the Grand Prix mode entity by its predicate, made current,
        // which builds the season entities the Continue lookup finds by name.
        if (!game.by_predicate || !game.gp_predicate) {
            why = "the Grand Prix mode lookup did not resolve";
            return ds::Setup::failed;
        }
        void *pred = reinterpret_cast<void *>(game.gp_predicate);
        uint8_t *gp = reinterpret_cast<ModeByPredicate_t *>(game.by_predicate)(modes, &pred);
        if (!gp || !runtime::readable(gp, ENTITY_MODE + 8))
            return ds::Setup::pending;
        grand_prix_entity = gp;
        reinterpret_cast<SetMode_t *>(game.set_mode)(modes, *reinterpret_cast<void **>(gp + ENTITY_MODE));
        grand_prix_set = true;
        swofty::log("[direct] Grand Prix: the Grand Prix mode is current (entity %p)", gp);
        return ds::Setup::pending;
    }
    uint8_t *entity = nullptr;
    const char *picked = nullptr;
    for (const char *name : SEASON_MODES) {
        uint8_t *e = reinterpret_cast<ModeByName_t *>(game.by_name)(modes, name);
        if (!e || !runtime::readable(e, ENTITY_MODE + 8))
            continue;
        auto *reg = e + ENTITY_REGISTRY;
        auto **vt = *reinterpret_cast<void ***>(reg);
        if (reinterpret_cast<int (*)(void *)>(vt[0x78 / 8])(reg) == 1) {
            entity = e;
            picked = name;
            break;
        }
    }
    if (!entity && grand_prix_entity) {
        auto *reg = grand_prix_entity + ENTITY_REGISTRY;
        int season = reinterpret_cast<int (*)(void *)>((*reinterpret_cast<void ***>(reg))[0x78 / 8])(reg);
        swofty::log("[direct] Grand Prix: no named season mode has a season; the Grand Prix mode entity %p reports "
                    "season %d and is the one a Continue falls back to",
                    grand_prix_entity, season);
        entity = grand_prix_entity;
        picked = "Grand Prix mode";
    }
    if (!entity) {
        static uint64_t first = GetTickCount64();
        static bool logged = false;
        if (!logged) {
            logged = true;
            for (const char *name : SEASON_MODES) {
                uint8_t *e = reinterpret_cast<ModeByName_t *>(game.by_name)(modes, name);
                int season = -1;
                if (e && runtime::readable(e, ENTITY_MODE + 8)) {
                    auto *reg = e + ENTITY_REGISTRY;
                    season = reinterpret_cast<int (*)(void *)>((*reinterpret_cast<void ***>(reg))[0x78 / 8])(reg);
                }
                swofty::log("[direct] Grand Prix: mode %s is %p, season %d", name, e, season);
            }
        }
        if (GetTickCount64() - first < CONTINUE_WAIT_MS)
            return ds::Setup::pending;
        why = "the profile has no saved Grand Prix to continue";
        return ds::Setup::failed;
    }
    void *rounds = registry_call(entity, 0x10, reinterpret_cast<void *>(swofty::api->module_base() + ROUNDS_COMPONENT), 1);
    if (rounds)
        reinterpret_cast<RoundsPrepare_t *>(game.prepare)(rounds);
    void *mode = *reinterpret_cast<void **>(entity + ENTITY_MODE);
    reinterpret_cast<SetMode_t *>(game.set_mode)(modes, mode);
    swofty::log("[direct] Grand Prix: continues the saved %s (mode entity %p, mode %p, rounds %p)", picked, entity,
                mode, rounds);
    return ds::Setup::done;
}

runtime::DirectStartSetup room_setup = nullptr;

ds::Setup set_up_target(std::string &why)
{
    if (driver.target.name == "session")
        return continue_grand_prix(why);
    if (driver.target.name == "room") {
        if (!room_setup) {
            why = "no room setup is registered (netplay_drive.cpp)";
            return ds::Setup::failed;
        }
        return room_setup(why);
    }
    return ds::Setup::done;
}

}

namespace runtime {

void direct_start_setup()
{
    char env[1024] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_DIRECT_START", env, sizeof env);
    std::string target, text;
    if (n && n < sizeof env && env[0] == '@')
        text = runtime::read_file(runtime::wide(env + 1), size_t(1) << 16);
    else if (n && n < sizeof env)
        target = env;
    else
        text = runtime::read_file(runtime::swofty_root() + L"direct_start.json", size_t(1) << 16);
    if (target.empty()) {
        if (text.empty())
            return;
        if (!json::parse(text, request)) {
            swofty::log("[direct] the direct start request is unusable; the game starts as usual");
            return;
        }
        target = request["target"].str();
    }
    ds::Target t;
    if (target == "session")
        t = ds::session_target();
    else if (target == "room")
        t = ds::room_target();
    else {
        swofty::log("[direct] swofty\\direct_start.json names no known target ('%s'); the game starts as usual",
                    target.c_str());
        return;
    }
    game.manager = runtime::find(SIG_GAME_MANAGER, "game manager", "direct");
    game.set_mode = runtime::find(SIG_SET_MODE, "SetCurrentGameMode", "direct");
    game.by_name = runtime::find(SIG_MODE_BY_NAME, "game mode lookup", "direct");
    game.prepare = runtime::find(SIG_ROUNDS_PREPARE, "rounds prepare", "direct");
    game.by_predicate = runtime::find(SIG_MODE_BY_PREDICATE, "game mode lookup by predicate", "direct");
    game.gp_predicate = runtime::find(SIG_GP_PREDICATE, "Grand Prix mode predicate", "direct");
    if (game.set_mode) {
        auto h = safetyhook::InlineHook::create(reinterpret_cast<void *>(game.set_mode),
                                                reinterpret_cast<void *>(set_mode_detour));
        if (h)
            h_set_mode = std::move(*h);
    }
    runtime::title::setup();
    if (!runtime::title::why().empty()) {
        swofty::log("[direct] off: the title can't pass by itself (%s)", runtime::title::why().c_str());
        return;
    }
    if (t.exit_state || t.from_state) {
        uintptr_t at = runtime::same_build() ? runtime::find(SIG_FLOW_LINK, "flow link", "direct") : 0;
        if (!at) {
            swofty::log("[direct] off: the flow link site is the KB build's and did not resolve on this build");
            return;
        }
        auto h = safetyhook::MidHook::create(reinterpret_cast<void *>(at), link_mid);
        if (!h) {
            swofty::log("[direct] off: the hook on the flow link site failed: %s", runtime::hook_error(h.error()).c_str());
            return;
        }
        h_link = std::move(*h);
    }
    {
        std::lock_guard<std::mutex> g(lock);
        driver.arm(t);
        note_phase();
    }
    runtime::title::want(true);
    armed = true;
}

void log_stack(const char *what)
{
    void *frames[24] = {};
    USHORT n = RtlCaptureStackBackTrace(1, 24, frames, nullptr);
    swofty::log("[direct] %s, thread %lu, from:", what, GetCurrentThreadId());
    for (USHORT i = 0; i < n; i++)
        swofty::log("[direct]   #%u %s", unsigned(i), runtime::code_where(reinterpret_cast<uintptr_t>(frames[i])).c_str());
}

uintptr_t direct_start_set_mode()
{
    return game.set_mode;
}

void direct_start_register_room(DirectStartSetup setup)
{
    room_setup = setup;
}

void direct_start_profile_read()
{
    static std::atomic<bool> once{false};
    if (armed.load() && !once.exchange(true))
        log_stack("the first profile file read");
}

void direct_start_profile_loaded(bool ok)
{
    if (!armed.load())
        return;
    static std::atomic<bool> once{false};
    if (!once.exchange(true))
        log_stack("the profile root load");
    std::lock_guard<std::mutex> g(lock);
    if (!ok) {
        swofty::log("[direct] the profile load failed; the start waits for the next one");
        return;
    }
    driver.profile_loaded();
    note_phase();
}

void direct_start_tick()
{
    if (!armed.load())
        return;
    std::lock_guard<std::mutex> g(lock);
    if (driver.phase == ds::Phase::setup) {
        std::string why;
        ds::Setup r = set_up_target(why);
        driver.setup_result(r, why);
    }
    if (!driver.engage_title() && runtime::title::wanted() && !runtime::test_run())
        runtime::title::want(false);
    note_phase();
}

}
