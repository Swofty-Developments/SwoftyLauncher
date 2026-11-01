#include "runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstring>
#include <mutex>
#include <string>

#include "netplay_room.hpp"

// Headless LAN host and join (research/multiplayer/headless-trace.md, gamesessionui.md).
//
// netplay_drive reaches a room the way a player does: Local Multiplayer, the LAN tile, the LAN Game dialog,
// then the Car Category screen for a host or the Join via IP prompt for a client. With the game on its
// main menu and the private LAN item initialised (the probe in netplay_drive.cpp), the same game state can
// be written and the same game functions called with none of those screens:
//
//  - the mode switch is the LAN tile's getAction on the private item (online_custom, then eLan);
//  - a host writes what the Car Category confirm writes (the DLC domain, the cluster id) and calls
//    GameSessionUI's CustomCreate (0x1430ae3f0) on *(ns+0x460), which sets up the session and hands a
//    SessionCreateCustom invite to the invite manager;
//  - a client fills the pending request record the Join via IP prompt fills; the pump at 0x1443dca90 hands
//    it to the invite manager as a SessionInvite.
//
// Either invite raises an interrupt the game routes to the multiplayer screen's onInviteJoinNotification,
// which asks the invite manager to join. With no screen that routing may not happen. The run watches the
// invite manager's join delegate (+0xb78); if the invite is there and the delegate is still empty after
// INTERRUPT_MS it calls onInviteJoinNotification(gsui, 0) itself, and logs that it did. The game's
// StateOnlineLobby then ends the run.
//
// Everything runs on the flow thread, one step per runtime::netplay_tick, and the game is never touched
// before the whole plan has checked out: a missing precondition makes set_up return false so the caller can
// take the screen-driven path instead.

namespace runtime::netplay_room::bind {
void *network();
void *registry();
bool on_home();
bool in_lobby();
bool lan_mode();
bool armed();
bool provide_context(void *registry, void *context);
std::string mode_switch();
}

namespace runtime {
bool car_category_choose(const std::string &value, std::string &why);
}

namespace {

namespace room = runtime::netplay_room;

const runtime::Sig S_CUSTOM_CREATE{"Game_Network_CustomCreate", "56 48 81 EC 70 01 00 00 48 89 CE E8", 0x30ae3f0};
const runtime::Sig S_INVITE_NOTIFY{"Game_GameSessionUI_OnInviteJoinNotification",
                                   "56 57 53 48 83 EC 40 48 89 CE 80 3D ?? ?? ?? ?? 01 75 66 B8 70 01 00 00", 0x30acca0};
const runtime::Sig S_REQUEST_KEY{"g_Game_JoinRequestOwnerKey",
                                 "48 8D 15 ?? ?? ?? ?? 48 89 F9 FF 50 28 48 85 C0 0F 84 ?? ?? ?? ?? 48 8B 48 18", 0x6cb8278,
                                 runtime::Sig::riprel, 0, 3, 7};

using CustomCreate_t = void(void *gsui);
using InviteNotify_t = void(void *gsui, uint32_t what);
using Lookup_t = void *(void *registry, const void *key);

constexpr size_t REGISTRY_LOOKUP_SLOT = 5;

std::mutex lock;
room::Role role = room::Role::host;
room::Category category;
room::Run run;
room::HostPlan host_plan;
room::JoinPlan join_plan;
room::Status state;
std::string error_now;
std::string join_ip;
uintptr_t a_create, a_notify, a_key;
bool resolved, asked, host_by_screens;
room::Role asked_role = room::Role::host, request_role = room::Role::host;
std::string request_ip;
bool requested;

bool read_memory(uint64_t address, void *out, size_t n, void *)
{
    if (!runtime::readable(reinterpret_cast<const void *>(address), n))
        return false;
    std::memcpy(out, reinterpret_cast<const void *>(address), n);
    return true;
}

room::Memory memory()
{
    return {read_memory, nullptr};
}

bool code_pointer(const void *p)
{
    auto a = reinterpret_cast<uintptr_t>(p);
    uintptr_t base = swofty::api->module_base();
    return p && a >= base && a < base + 0x20000000 && runtime::readable(p, 1);
}

int session_role_now()
{
    void *ns = room::bind::network();
    return ns ? room::session_role(memory(), reinterpret_cast<uint64_t>(ns)) : -1;
}

void resolve_once()
{
    if (resolved)
        return;
    resolved = true;
    a_create = runtime::find(S_CUSTOM_CREATE, "netplay", "netplay");
    a_notify = runtime::find(S_INVITE_NOTIFY, "netplay", "netplay");
    a_key = runtime::find(S_REQUEST_KEY, "netplay", "netplay");
}

void *request_owner(std::string &why)
{
    void *registry = room::bind::registry();
    if (!registry || !runtime::readable(registry, 8)) {
        why = "the init context's registry was not captured (no LAN systems probe line in loader.log)";
        return nullptr;
    }
    auto **vt = *static_cast<void ***>(registry);
    if (!runtime::readable(vt, (REGISTRY_LOOKUP_SLOT + 1) * sizeof(void *)) || !code_pointer(vt[REGISTRY_LOOKUP_SLOT])) {
        why = "the captured registry has no lookup function";
        return nullptr;
    }
    if (!a_key) {
        why = "the join request key (g_Game_JoinRequestOwnerKey) did not resolve on this build";
        return nullptr;
    }
    void *owner = reinterpret_cast<Lookup_t *>(vt[REGISTRY_LOOKUP_SLOT])(registry, reinterpret_cast<const void *>(a_key));
    if (!owner) {
        why = "the registry has no join request owner";
        return nullptr;
    }
    return owner;
}

std::string plan_for(room::Role r, const std::string &ip)
{
    void *ns = room::bind::network();
    if (!ns)
        return "the network subsystem was not reached (no LAN systems probe line in loader.log)";
    if (r == room::Role::host) {
        if (!a_create)
            return "CustomCreate did not resolve on this build";
        if (!a_notify)
            return "onInviteJoinNotification did not resolve on this build";
        std::string why;
        if (!runtime::car_category_choose(category.addon, why))
            return why;
        return room::plan_host(memory(), reinterpret_cast<uint64_t>(ns), category, host_plan);
    }
    if (!a_notify)
        return "onInviteJoinNotification did not resolve on this build";
    std::string why;
    void *owner = request_owner(why);
    if (!owner)
        return why;
    return room::plan_join(memory(), reinterpret_cast<uint64_t>(ns), reinterpret_cast<uint64_t>(owner), ip, join_plan);
}

bool apply(const std::vector<room::Write> &writes)
{
    for (const auto &w : writes) {
        if (!runtime::readable(reinterpret_cast<void *>(w.address), w.bytes.size()))
            return false;
    }
    for (const auto &w : writes) {
        std::atomic_thread_fence(std::memory_order_release);
        std::memcpy(reinterpret_cast<void *>(w.address), w.bytes.data(), w.bytes.size());
    }
    std::atomic_thread_fence(std::memory_order_release);
    return true;
}

void finish(room::Phase phase, const std::string &detail)
{
    state.phase = phase;
    state.detail = detail;
    swofty::log("[netplay] room %s: %s %s", room::role_name(role), phase == room::Phase::done ? "ready:" : "failed:",
                detail.c_str());
}

void act(room::Do d)
{
    std::string err;
    switch (d) {
    case room::Do::mode_switch:
        err = room::bind::mode_switch();
        break;
    case room::Do::host_call: {
        err = plan_for(room::Role::host, {});
        if (!err.empty())
            break;
        if (!apply(host_plan.writes)) {
            err = "the Car Category values could not be written";
            break;
        }
        swofty::log("[netplay] room host: wrote DLC domain 0x%x at ns+0x4f4 and cluster id -1 (settings %p), calling "
                    "CustomCreate(gsui %p)%s%s",
                    unsigned(category.preset), reinterpret_cast<void *>(host_plan.settings),
                    reinterpret_cast<void *>(host_plan.gsui), category.addon.empty() ? "" : ", addon category ",
                    category.addon.c_str());
        reinterpret_cast<CustomCreate_t *>(a_create)(reinterpret_cast<void *>(host_plan.gsui));
        break;
    }
    case room::Do::join_write: {
        err = plan_for(room::Role::join, join_ip);
        if (!err.empty())
            break;
        if (!apply(join_plan.writes)) {
            err = "the join request could not be written";
            break;
        }
        swofty::log("[netplay] room join: wrote the pending request at %p: LAN, %s port %u, connection id 0x%08x",
                    reinterpret_cast<void *>(join_plan.request), join_ip.c_str(), unsigned(join_plan.port),
                    room::connection_id(join_plan.ip, join_plan.port));
        break;
    }
    case room::Do::fallback_notify: {
        uint64_t gsui = role == room::Role::host ? host_plan.gsui : join_plan.gsui;
        swofty::log("[netplay] room %s: the invite reached the invite manager but the interrupt raised no join "
                    "within the wait; calling onInviteJoinNotification(gsui %p, 0) by hand (fallback path)",
                    room::role_name(role), reinterpret_cast<void *>(gsui));
        reinterpret_cast<InviteNotify_t *>(a_notify)(reinterpret_cast<void *>(gsui), 0);
        break;
    }
    default:
        return;
    }
    if (d != room::Do::nothing)
        swofty::log("[netplay] room %s: step %s%s%s", room::role_name(role), room::do_name(d),
                    err.empty() ? "" : " failed: ", err.c_str());
    if (!err.empty())
        run.fail(err);
}

}

namespace runtime::netplay_room {

namespace {

bool begin(Role r, const char *host_ip, bool from_home, std::string &why)
{
    auto refuse = [&](const std::string &reason) {
        why = error_now = reason;
        swofty::log("[netplay] room %s not started headless: %s", role_name(r), reason.c_str());
        return false;
    };
    if (!bind::armed())
        return refuse("netplay is not armed (SWOFTY_NETPLAY is unset)");
    if (run.active())
        return refuse("a headless set-up is already running");
    if (from_home && !bind::on_home())
        return refuse("the game is not on its main menu");
    if (bind::in_lobby())
        return refuse("the game is already in a LAN lobby");
    resolve_once();
    role = r;
    join_ip = host_ip ? host_ip : "";
    std::string reason = plan_for(r, join_ip);
    if (!reason.empty())
        return refuse(reason);
    if (int now = session_role_now(); now == 1 || now == 2)
        return refuse("the game already has a session (role " + std::to_string(now) + ")");
    error_now.clear();
    run.start(r, GetTickCount64(), from_home);
    state = {Phase::running, "mode switch"};
    swofty::log("[netplay] room %s: headless set-up started %s (plan checked: %s)", role_name(r),
                from_home ? "from the main menu" : "before the main menu",
                r == Role::host ? "GameSessionUI, session settings, invite manager, request clear"
                                : "GameSessionUI, own socket port, invite manager, request record");
    return true;
}

}

bool provide_context(void *registry, void *context)
{
    return bind::provide_context(registry, context);
}

bool set_up_from_home(Role r, const char *host_ip, std::string &why)
{
    std::lock_guard<std::mutex> g(lock);
    return begin(r, host_ip, true, why);
}

Setup set_up(Role r, const char *host_ip, std::string &why)
{
    tick();
    std::lock_guard<std::mutex> g(lock);
    if (!asked) {
        asked = true;
        asked_role = r;
        static const char *screens = [] {
            static char v[16];
            DWORD n = GetEnvironmentVariableA("SWOFTY_ROOM_HOST", v, sizeof v);
            return n && n < sizeof v ? v : "";
        }();
        if (r == Role::host && !std::strcmp(screens, "screens")) {
            host_by_screens = true;
            swofty::log("[netplay] room host: SWOFTY_ROOM_HOST=screens, so only the LAN mode switch is made here and "
                        "the Car Category screen has to be reached by the screens");
        }
        if (!begin(r, host_ip, false, why)) {
            state = {Phase::failed, why};
            return Setup::failed;
        }
    } else if (asked_role != r) {
        why = "a headless set-up for the other role was already asked for";
        return Setup::failed;
    }
    if (host_by_screens && run.stage >= Stage::call && run.active()) {
        state = {Phase::done, "the game is in LAN mode; the Car Category screen is next"};
        run.stage = Stage::done;
        swofty::log("[netplay] room host: LAN mode is in; handing over to the screens");
        return Setup::done;
    }
    why = state.detail;
    return state.phase == Phase::done ? Setup::done : state.phase == Phase::failed ? Setup::failed : Setup::pending;
}

void set_room_request(Role r, const std::string &host_ip)
{
    std::lock_guard<std::mutex> g(lock);
    request_role = r;
    request_ip = host_ip;
    requested = true;
}

Setup room_poll(std::string &why)
{
    Role r;
    std::string ip;
    {
        std::lock_guard<std::mutex> g(lock);
        if (!requested) {
            why = "no room request (role and host address) was set";
            return Setup::failed;
        }
        r = request_role;
        ip = request_ip;
    }
    return set_up(r, ip.c_str(), why);
}

void set_category(const Category &c)
{
    std::lock_guard<std::mutex> g(lock);
    category = c;
}

Status status()
{
    std::lock_guard<std::mutex> g(lock);
    return state;
}

std::string last_error()
{
    std::lock_guard<std::mutex> g(lock);
    return error_now;
}

void tick()
{
    std::lock_guard<std::mutex> g(lock);
    if (!run.active())
        return;
    Facts f;
    f.home = bind::on_home();
    f.lobby = bind::in_lobby();
    f.lan_mode = bind::lan_mode();
    f.session_role = session_role_now();
    if (run.role == Role::join) {
        uint8_t pending = 1;
        f.pending_taken = join_plan.request && read_memory(join_plan.request + REQ_PENDING, &pending, 1, nullptr) && !pending;
    }
    uint64_t invites = run.role == Role::host ? host_plan.invites : join_plan.invites;
    if (invites)
        f.invite = invite_state(memory(), invites);
    Stage before = run.stage;
    Do d = run.tick(f, GetTickCount64());
    if (d != Do::nothing)
        act(d);
    if (run.stage != before && run.stage != Stage::failed && run.stage != Stage::done)
        state.detail = run.stage == Stage::call        ? "calling the game"
                       : run.stage == Stage::interrupt ? "waiting for the invite to reach the join"
                                                       : "waiting for the session";
    if (run.done())
        finish(Phase::done, run.by_hand ? "the session is established (join requested by hand)" : "the session is established");
    else if (run.failed())
        finish(Phase::failed, run.error);
}

}
