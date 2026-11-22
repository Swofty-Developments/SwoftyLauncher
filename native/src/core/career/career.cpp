#include "core/runtime/runtime.hpp"

#include "core/career/career_entity.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstring>
#include <mutex>
#include <string>
#include <unordered_map>

#include "core/apply/moments.hpp"
#include "core/career/career_state.hpp"
#include "core/assets/dbread_logic.hpp"
#include "core/runtime/layout.hpp"
#include "core/guards/career/myteam_vehicle_heal_logic.hpp"
#include "core/diagnostics/perf.hpp"
#include "core/frontend/view_state.hpp"

// Flow states (site_FeFlow_SetStateStatus, FlowState). Every flow state's status change goes
// through FeFlow_Set_StateStatus(state, status); the site sits after its prologue with rcx = the
// state and edx = the new status, while FlowState.status still holds the old one. A state is keyed
// by FlowState.name_hash (djb2 of its name); the name buffer is k_Gdb_StringXorKey-encoded until
// the game decodes it in place, so the core decodes a copy itself. Read only.
//
// Team HQ answer (site_Career_TeamHqIsMyTeamResult). StateTeamHQ's onStateEnter asks
// Career_Is_MyTeamGameMode which Team HQ to open; the site reads that answer (al) right after the
// call. It answers in both career modes. Read only.
//
// Bus (g_Game_MessageBus, GameMessageBusHolder, NeMessageBus, Ne_Post_BusMessage). post() hands a
// caller-built message to the game's synchronous publish while the bus is active.
//
// The two sites are core mixins of their own (runtime/loader/mixin_manager.cpp), so a build without them loses only
// this library; every other binding resolves lazily and each lookup that fails reports
// unavailable or null for that call alone.

namespace {

namespace g = swofty::game;
namespace layout = runtime::layout;
namespace mem = swofty::mem;
using career::game::binding;
using career::state::PollSchedule;
using career::state::Tracker;

constexpr const char *FLOW_MIXIN = "swofty_core.dll (flow states)";
constexpr const char *TEAM_HQ_MIXIN = "swofty_core.dll (Team HQ answer)";
constexpr const char *STATUS[4] = {"None", "Entering", "Updating", "Exiting"};

using Post = uint64_t(void *bus, void *msg);

std::mutex lock;
Tracker tracker;
std::atomic<bool> armed, set_up;
bool flow_live, team_hq_live;
std::atomic<uint64_t> last_poll;
PollSchedule schedule;
std::atomic<bool> in_session;
std::mutex names_lock;
std::unordered_map<uint32_t, std::string> names;

std::mutex bind_lock;
std::unordered_map<std::string, uintptr_t> bound;

std::mutex perf_lock;
std::unordered_map<uint32_t, LARGE_INTEGER> entering_at;

bool perf_on()
{
    static const bool on = [] {
        char v[8] = {};
        DWORD n = GetEnvironmentVariableA("SWOFTY_PERF", v, sizeof v);
        return n && n < sizeof v && v[0] && v[0] != '0';
    }();
    return on;
}

void perf_flow(uint32_t hash, uint32_t from, uint32_t to, const std::string &name)
{
    LARGE_INTEGER now, f;
    QueryPerformanceCounter(&now);
    QueryPerformanceFrequency(&f);
    std::lock_guard<std::mutex> g(perf_lock);
    if (to == 1) {
        entering_at[hash] = now;
        return;
    }
    auto it = entering_at.find(hash);
    if (from != 1 || it == entering_at.end())
        return;
    double ms = double(now.QuadPart - it->second.QuadPart) * 1000.0 / double(f.QuadPart);
    entering_at.erase(it);
    swofty::log("[perf] flow \"%s\" Entering -> %s took %.1f ms", name.c_str(), STATUS[to & 3], ms);
}

uint64_t now_ms() { return GetTickCount64(); }

void fire(const char *event, int64_t a, int64_t b, const void *data) { runtime::events_fire(event, a, b, data); }

const char *mode_name(uint32_t m)
{
    return m == SWOFTY_CAREER_MY_TEAM ? "my_team" : m == SWOFTY_CAREER_DRIVER ? "driver_career" : "none";
}

void publish(const career::state::Edges &e, const SwoftyCareerState &s)
{
    if (e.hub == 0) {
        perf::mark("career", "hub left");
        swofty::log("[career] career.hub left (mode %s)", mode_name(e.unloaded ? e.old_mode : s.mode));
        fire(SWOFTY_EVENT_CAREER_HUB, 0, e.unloaded ? e.old_mode : s.mode, &s);
    }
    if (e.unloaded) {
        perf::mark("career", "unloaded");
        swofty::log("[career] career.unloaded (mode %s)", mode_name(e.old_mode));
        fire(SWOFTY_EVENT_CAREER_UNLOADED, e.old_mode, 0, &s);
        swofty_core::apply::moments::career_unloaded();
        runtime::era_career_unloaded();
    }
    if (e.loaded) {
        perf::mark("career", "loaded");
        swofty::log("[career] career.loaded: mode %s, key '%s' (components %s, component mode %s, Team HQ answer %s)",
                    mode_name(s.mode), s.key,
                    s.components == SWOFTY_CAREER_COMPONENTS_LIVE       ? "live"
                    : s.components == SWOFTY_CAREER_COMPONENTS_FALLBACK ? "fallback to flow"
                                                                        : "unseen",
                    mode_name(s.component_mode), mode_name(s.team_hq_mode));
        if (s.mode == SWOFTY_CAREER_MY_TEAM)
            career::game::heal_my_team();
        std::string id = runtime::snapshots_career_loaded(s.key, s.mode);
        swofty_core::apply::moments::career_loaded(id.c_str(), s.mode);
        fire(SWOFTY_EVENT_CAREER_LOADED, s.mode, 0, &s);
    }
    if (e.hub == 1) {
        perf::mark("career", "hub entered");
        swofty::log("[career] career.hub entered (mode %s)", mode_name(s.mode));
        runtime::hub_calendar_team_hq();
        fire(SWOFTY_EVENT_CAREER_HUB, 1, s.mode, &s);
    }
}

void settle_and_publish()
{
    career::state::Edges e;
    SwoftyCareerState s;
    {
        std::lock_guard<std::mutex> g(lock);
        e = tracker.settle();
        s = tracker.state();
    }
    if (e.loaded || e.unloaded || e.hub >= 0)
        publish(e, s);
}

template <class S = g::FlowState> bool read_state(uintptr_t at, uint32_t &hash, int32_t &status, uint64_t &size, uintptr_t &name)
{
    if constexpr (layout::flow_state<S>) {
        return mem::read(at + offsetof(S, name_hash), hash) && mem::read(at + offsetof(S, status), status) &&
               mem::read(at + offsetof(S, name_size), size) && mem::read(at + offsetof(S, name), name);
    } else {
        (void)at, (void)hash, (void)status, (void)size, (void)name;
        return false;
    }
}

bool state_name(uint32_t hash, uint64_t size, uintptr_t chars, std::string &out)
{
    if (size < 2 || size > 128 || !mem::readable(chars, size_t(size)))
        return false;
    out.assign(reinterpret_cast<const char *>(chars), size_t(size - 1));
    if (out.find('\0') == std::string::npos && mem::printable_utf8(out) && career::state::djb2(out.c_str()) == hash)
        return true;
    uintptr_t key = binding("k_Gdb_StringXorKey");
    if (key && mem::readable(key, 4) &&
        dbread::decode_text(reinterpret_cast<const uint8_t *>(chars), size, reinterpret_cast<const uint8_t *>(key), out) &&
        career::state::djb2(out.c_str()) == hash)
        return true;
    out.clear();
    return false;
}

void on_flow(SwoftyContext *c)
{
    uint32_t hash = 0, to = uint32_t(c->rdx);
    int32_t from = 0;
    uint64_t size = 0;
    uintptr_t chars = 0;
    if (to > 3 || !read_state(c->rcx, hash, from, size, chars) || from < 0 || from > 3)
        return;
    const char *known = tracker.known_name(hash);
    std::string name;
    bool first = false;
    if (known) {
        name = known;
    } else {
        std::lock_guard<std::mutex> g(names_lock);
        auto it = names.find(hash);
        if (it != names.end()) {
            name = it->second;
        } else {
            first = true;
            if (!state_name(hash, size, chars, name)) {
                char b[16];
                snprintf(b, sizeof b, "#%08x", hash);
                name = b;
            }
            if (names.size() < career::state::MAX_STATES)
                names[hash] = name;
        }
    }
    bool named = name[0] != '#';
    SwoftyFlowState f;
    {
        std::lock_guard<std::mutex> g(lock);
        uint64_t now = now_ms();
        tracker.flow(hash, uint32_t(from), to, name.c_str(), named, now);
        f = *tracker.last();
        if (tracker.settles(hash))
            schedule.transition(now);
        in_session = tracker.session();
    }
    if (known || first)
        swofty::log("[career] flow \"%s\" %s -> %s%s", name.c_str(), STATUS[from & 3], STATUS[to & 3],
                    known ? "" : " (first sighting)");
    if (to == 1 || to == 3)
        perf::mark("flow", name + " " + STATUS[to]);
    if (perf_on())
        perf_flow(hash, uint32_t(from), to, name);
    runtime::watchdog_flow(hash, to, name.c_str());
    runtime::hub_calendar_flow(hash, to);
    runtime::load_stall_flow(hash, to, name.c_str());
    runtime::session_job_probe_flow(to, name.c_str());
    runtime::session_host_flow(hash, to);
    runtime::entitlements_flow(hash, to);
    runtime::testagent_flow(name, to);
    frontend::view::flow(hash, to);
    if (known) {
        runtime::era_session_trace(known, int(to));
        runtime::era_session_unloaded(known, int(to));
    }
    if (known && to == 1 && !std::strcmp(known, "StateLoadSession")) {
        runtime::era_session_load();
        swofty_core::apply::moments::session_load("StateLoadSession");
    }
    runtime::session_flow(known, int(to));
    if (known && to == 1 && !std::strcmp(known, "StateUnloadSession")) {
        swofty_core::apply::moments::session_end("StateUnloadSession");
    }
    fire(SWOFTY_EVENT_FLOW_STATE, to, from, &f);
    settle_and_publish();
}

void on_team_hq(SwoftyContext *c)
{
    bool my_team = (c->rax & 0xff) != 0;
    uint32_t was;
    {
        std::lock_guard<std::mutex> g(lock);
        was = tracker.state().team_hq_mode;
        tracker.team_hq_answer(my_team);
    }
    uint32_t now = my_team ? SWOFTY_CAREER_MY_TEAM : SWOFTY_CAREER_DRIVER;
    if (was != now)
        swofty::log("[career] Team HQ entered: the game reports %s (Career_Is_MyTeamGameMode)",
                    my_team ? "My Team" : "not My Team");
    settle_and_publish();
}

int api_available() { return (set_up ? armed.load() : bool(layout::FLOW)) && runtime::enabled("career", nullptr); }

int api_state(SwoftyCareerState *out)
{
    if (!out)
        return SWOFTY_ASSET_INVALID;
    if (!api_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard<std::mutex> g(lock);
    *out = tracker.state();
    return SWOFTY_ASSET_OK;
}

int api_flow(SwoftyFlowState *out)
{
    if (!out)
        return SWOFTY_ASSET_INVALID;
    if (!api_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard<std::mutex> g(lock);
    if (!tracker.last())
        return SWOFTY_ASSET_LATE;
    *out = *tracker.last();
    return SWOFTY_ASSET_OK;
}

int api_flow_status(uint32_t hash, uint32_t *status, uint64_t *age_ms)
{
    if (!api_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    uint32_t s = 0;
    uint64_t age = 0;
    bool seen;
    {
        std::lock_guard<std::mutex> g(lock);
        seen = tracker.status_of(hash, s, age, now_ms());
    }
    if (status)
        *status = s;
    if (age_ms)
        *age_ms = age;
    return seen ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

void *api_entity()
{
    if (!runtime::enabled("career", nullptr))
        return nullptr;
    return reinterpret_cast<void *>(career::game::entity());
}

void *api_component(const char *type)
{
    if (!runtime::enabled("career", nullptr) || !type || !*type)
        return nullptr;
    uintptr_t e = career::game::entity();
    return e ? reinterpret_cast<void *>(career::game::component(e, type)) : nullptr;
}

void *api_component_by_desc(const void *desc)
{
    if (!runtime::enabled("career", nullptr) || !desc)
        return nullptr;
    uintptr_t e = career::game::entity();
    return e ? reinterpret_cast<void *>(career::game::component_by_desc(e, reinterpret_cast<uintptr_t>(desc))) : nullptr;
}

template <class H = g::GameMessageBusHolder, class B = g::NeMessageBus> int api_post(void *msg)
{
    if (!runtime::enabled("career", nullptr))
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!msg || !mem::object_like(msg))
        return SWOFTY_ASSET_INVALID;
    if constexpr (layout::message_bus<H, B>) {
        uintptr_t slot = binding("g_Game_MessageBus"), fn = binding("Ne_Post_BusMessage"), holder = 0, bus = 0;
        uint8_t active = 0;
        if (!slot || !fn)
            return SWOFTY_ASSET_UNAVAILABLE;
        if (!mem::read(slot, holder) || !holder || !mem::read(holder + offsetof(H, bus), bus) || !mem::object_like(bus) ||
            !mem::read(bus + offsetof(B, active), active) || !active)
            return SWOFTY_ASSET_LATE;
        reinterpret_cast<Post *>(fn)(reinterpret_cast<void *>(bus), msg);
        return SWOFTY_ASSET_OK;
    } else {
        return SWOFTY_ASSET_UNAVAILABLE;
    }
}

int api_post_msg(void *msg) { return api_post(msg); }

int api_block(uint32_t mode, const char *title, const char *body)
{
    return runtime::career_block(__builtin_return_address(0), mode, title, body);
}

int api_unblock(uint32_t mode) { return runtime::career_unblock(__builtin_return_address(0), mode); }

const SwoftyCareer table = {api_available,         api_state,    api_flow,  api_flow_status, api_entity, api_component,
                            api_component_by_desc, api_post_msg, api_block, api_unblock};

void flow_entry(SwoftyContext *c) { on_flow(c); }

void team_hq_entry(SwoftyContext *c) { on_team_hq(c); }

}

uintptr_t career::game::binding(const std::string &id)
{
    std::lock_guard<std::mutex> g(bind_lock);
    auto it = bound.find(id);
    if (it != bound.end())
        return it->second;
    uintptr_t a = swofty::api->resolve(id.c_str());
    if (a || runtime::code_ready())
        bound[id] = a;
    return a;
}

namespace runtime {

bool in_session_now() { return in_session.load(std::memory_order_relaxed); }

const SwoftyCareer *career_table() { return &table; }

std::vector<CoreMixins> career_mixins()
{
    std::vector<CoreMixins> out;
    if (!enabled("career", "career")) {
        swofty::log("[career] feature off; the career library reports unavailable");
        return out;
    }
    if (!layout::FLOW) {
        swofty::log("[career] this core was built without the FlowState layout; the career library reports unavailable");
        return out;
    }
    out.push_back({FLOW_MIXIN, {{"site_FeFlow_SetStateStatus", SWOFTY_MIXIN_SITE, 100, (void *)&flow_entry, nullptr}}});
    out.push_back(
        {TEAM_HQ_MIXIN, {{"site_Career_TeamHqIsMyTeamResult", SWOFTY_MIXIN_SITE, 100, (void *)&team_hq_entry, nullptr}}});
    return out;
}

void career_setup()
{
    if (set_up || !enabled("career", nullptr))
        return;
    flow_live = core_mixins_active(FLOW_MIXIN);
    team_hq_live = core_mixins_active(TEAM_HQ_MIXIN);
    armed = flow_live;
    set_up = true;
    if (armed)
        swofty_core::apply::moments::mode_watch();
    career_start_setup();
    swofty::log("[career] setup: %s; flow states %s, Team HQ answer %s, career layout %s, entrants %s, bus %s, "
                "string key %s",
                armed ? "live" : "the flow-state site is not installed, so the career library reports unavailable",
                flow_live ? "armed" : "off", team_hq_live ? "armed" : "off", layout::CAREER ? "yes" : "no",
                layout::ENTRANT ? "yes" : "no", layout::BUS ? "yes" : "no",
                binding("k_Gdb_StringXorKey") ? "bound" : "missing (unknown state names stay hashes)");
}

void career_tick()
{
    if (!armed || !layout::CAREER)
        return;
    uint64_t now = now_ms(), was = last_poll.load();
    if (!schedule.due(now, was, in_session.load(std::memory_order_relaxed)) || !last_poll.compare_exchange_strong(was, now))
        return;
    namespace facts = swofty_core::apply::facts;
    facts::ModeKind kind = swofty_core::apply::moments::mode_kind();
    career::state::Where where = kind == facts::ModeKind::career    ? career::state::Where::career
                                 : kind == facts::ModeKind::unknown ? career::state::Where::unknown
                                                                    : career::state::Where::elsewhere;
    bool reads = career::state::poll_reads(where);
    static std::atomic<int> said{-1};
    if (said.exchange(reads) != int(reads))
        swofty::log("[career] component poll %s (game mode %s)",
                    reads ? "reads the game" : "off: no career can be loaded here, it reports none without a read",
                    facts::mode_kind_name(kind));
    career::state::Components c = reads ? career::game::poll() : career::state::Components{};
    {
        std::lock_guard<std::mutex> g(lock);
        tracker.components(c, now);
    }
    settle_and_publish();
}

}
