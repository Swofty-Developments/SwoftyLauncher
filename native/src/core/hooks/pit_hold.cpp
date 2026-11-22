#include "core/hooks/pit_hold.hpp"

#include "core/guards/guard.hpp"
#include "core/hooks/pit_hold_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdio>
#include <cstring>
#include <string>

// The extra box hold (pit_hold_logic.hpp has the rules), on these bindings:
//   site_Game_PitLane_BoxHoldSeconds                xmm0 = the box hold seconds of vehicle r12b; the car's armed seconds
//                                                   are added while it is InPitBox
//   site_Game_PitStopObserver_StopGoAnswer{Prefixed,Plain}
//                                                   al = Sess_Penalty_IsServedAtNextStop for the crew sequence name of
//                                                   the instance [r13+8]; set for a stretched stop, so the crew plays
//                                                   the game's stop-go clip
//   Game_PitStopInstance_PlaySequence               the crew's box clip starts: a StartJob clip of a held car starts
//                                                   the crew wait; any stop-go clip of a held car that does not load
//                                                   (the 5SP/10SP suffix goes on every crew clip of the stop) is
//                                                   played plain
//   site_Sess_PitStopUpdateJob_ScaledDt             xmm6 = the crew job's frame step of the instance rsi; changed
//                                                   while the crew waits, counted until the clip's Start event
//   site_Game_PitStopLogic_ClipStartEvent           the clip's Start event of the instance rbx: the crew starts working
// A PitStopInstance holds its car at +0x350 when its car count +0x358 is not 0, the game's own reading at the crew call
// sites. The crew wait style is the core config's "pit_hold_crew" (stretch, freeze or none; default stretch), or
// SWOFTY_PIT_HOLD_CREW. SWOFTY_PIT_HOLD=0 turns the hold off.

namespace swofty_core::hooks::pit_hold {

namespace {

namespace mem = swofty::mem;
namespace guards = swofty_core::guards;

constexpr const char *SET = "swofty_core.dll (pit hold)";
constexpr const char *BOX = "site_Game_PitLane_BoxHoldSeconds";
constexpr const char *ANSWER_PREFIXED = "site_Game_PitStopObserver_StopGoAnswerPrefixed";
constexpr const char *ANSWER_PLAIN = "site_Game_PitStopObserver_StopGoAnswerPlain";
constexpr const char *PLAY = "Game_PitStopInstance_PlaySequence";
constexpr const char *SCALED_DT = "site_Sess_PitStopUpdateJob_ScaledDt";
constexpr const char *CLIP_START = "site_Game_PitStopLogic_ClipStartEvent";

constexpr uintptr_t OBSERVER_INSTANCE = 0x8, INSTANCE_CAR = 0x350, INSTANCE_CARS = 0x358, INSTANCE_SEQUENCE = 0x3b0,
                    INSTANCE_RATE = 0x3bc;

using Play = void (*)(void *instance, const char *name);

Table table;
Play play_original = nullptr;
std::atomic<bool> hold_live{false};
std::atomic<uint32_t> box_logged[CARS];

bool crew_live() { return play_original != nullptr; }

uint32_t instance_car(uintptr_t inst)
{
    uint64_t cars = 0;
    uint8_t car = 0xff;
    if (!inst || !mem::read(inst + INSTANCE_CARS, cars) || !cars || !mem::read(inst + INSTANCE_CAR, car))
        return 0xff;
    return car;
}

void box_hold(SwoftyContext *c)
{
    uint32_t car = uint32_t(c->r12 & 0xff);
    float s = table.box_extra(car);
    if (s <= 0)
        return;
    c->xmm0.f32[0] += s;
    uint32_t serves = table.car(car).serves.load(std::memory_order_relaxed) + 1;
    if (box_logged[car].exchange(serves, std::memory_order_relaxed) != serves)
        swofty::log("[pit hold] car %u: in the box, held %.2f s more than the game's %.2f s", car, s, c->xmm0.f32[0] - s);
}

void stop_go_answer(SwoftyContext *c)
{
    uintptr_t inst = 0;
    if (!mem::read(c->r13 + OBSERVER_INSTANCE, inst))
        return;
    uint32_t car = instance_car(inst);
    if (car < CARS && table.crew_serves(car))
        c->rax = (c->rax & ~uintptr_t(0xff)) | 1;
}

bool ends_with(const std::string &s, const char *tail)
{
    size_t n = std::strlen(tail);
    return s.size() >= n && s.compare(s.size() - n, n, tail) == 0;
}

void play(void *instance, const char *name)
{
    uint32_t car = instance_car(reinterpret_cast<uintptr_t>(instance));
    if (car >= CARS || !name || table.car(car).stop.load() <= 0) {
        play_original(instance, name);
        return;
    }
    std::string n = name;
    float base = ends_with(n, "10SP") ? 10.0f : ends_with(n, "5SP") ? 5.0f : 0.0f;
    play_original(instance, name);
    bool missing = false;
    if (base > 0) {
        uintptr_t seq = 0;
        if (!mem::read(reinterpret_cast<uintptr_t>(instance) + INSTANCE_SEQUENCE, seq) || !seq) {
            missing = true;
            n.resize(n.size() - (base == 10.0f ? 4 : 3));
            play_original(instance, n.c_str());
        }
    }
    if (!std::strstr(name, "StartJob")) {
        if (missing)
            swofty::log("[pit hold] car %u: crew clip %s did not load; %s plays instead", car, name, n.c_str());
        return;
    }
    uint32_t m = table.clip_started(car, base > 0, missing, base > 0 ? base : STRETCH_BASE);
    char lead[64] = "";
    if (m == CREW_STRETCH)
        std::snprintf(lead, sizeof lead, " (its Start event at clip time %.2f s)", table.car(car).base.load());
    swofty::log("[pit hold] car %u: crew clip %s%s%s, the crew %s for %.2f s%s", car, n.c_str(),
                missing ? " (its stop-go clip did not load)" : "",
                table.car(car).clips.load() > 1 ? " again in this stop (a flashback), the wait starts over" : "",
                m == CREW_STRETCH ? "plays the stop-go wait" : m == CREW_FREEZE ? "stands still" : "works at once and the car waits",
                table.car(car).stop.load(), lead);
}

void scaled_dt(SwoftyContext *c)
{
    uint32_t car = instance_car(c->rsi);
    if (car >= CARS)
        return;
    bool waiting = table.car(car).waiting.load(std::memory_order_relaxed);
    if (!waiting && !table.car(car).timing.load(std::memory_order_relaxed))
        return;
    float rate = 0;
    if (!mem::read(c->rsi + INSTANCE_RATE, rate))
        return;
    float dt = c->xmm6.f32[0];
    float out = table.crew_step(car, dt, rate);
    c->xmm6.f32[0] = out;
    if (waiting && !table.car(car).waiting.load(std::memory_order_relaxed))
        swofty::log("[pit hold] car %u: the crew wait ended after %.2f s", car, table.car(car).waited.load());
}

void clip_start(SwoftyContext *c)
{
    uint32_t car = instance_car(c->rbx);
    if (car >= CARS)
        return;
    bool timed = table.car(car).timing.load();
    bool ended = table.crew_start(car);
    if (timed || ended)
        swofty::log("[pit hold] car %u: the stop-go clip's Start event at clip time %.2f s, after %.2f s of crew wait%s",
                    car, table.clip_time(car), table.car(car).waited.load(), ended ? "" : " (the wait had already ended)");
}

void crew_mode()
{
    Crew mode = CREW_STRETCH;
    char env[16] = {};
    std::string conf = ::runtime::config()["pit_hold_crew"].str("");
    bool from_env = GetEnvironmentVariableA("SWOFTY_PIT_HOLD_CREW", env, sizeof env) && crew_from_name(env, mode);
    if (!from_env && !conf.empty() && !crew_from_name(conf.c_str(), mode))
        swofty::log("[pit hold] pit_hold_crew '%s' is not stretch, freeze or none; stretch", conf.c_str());
    table.set_crew(mode);
}

class HoldGuard final : public guards::SiteGuard {
public:
    using SiteGuard::SiteGuard;

protected:
    bool admit() override
    {
        crew_mode();
        return true;
    }
};

void hold_armed()
{
    hold_live = true;
    swofty::log("[pit hold] crew wait: %s", crew_live() ? crew_name(table.crew_mode()) : "none (its sites did not install)");
}

HoldGuard guard{guards::CoreSetId::pit_hold,
                {
                    .set = SET,
                    .name = "[pit hold]",
                    .env = "SWOFTY_PIT_HOLD",
                    .needs = {BOX},
                    .mixins = {{BOX, SWOFTY_MIXIN_SITE, 0, (void *)&box_hold, nullptr}},
                    .armed = "a car's armed seconds are added to its box hold",
                    .parts = {{
                        .set = "swofty_core.dll (pit hold crew)",
                        .name = "[pit hold] crew wait",
                        .needs = {ANSWER_PREFIXED, ANSWER_PLAIN, PLAY, SCALED_DT, CLIP_START},
                        .mixins = {{ANSWER_PREFIXED, SWOFTY_MIXIN_SITE, 0, (void *)&stop_go_answer, nullptr},
                                   {ANSWER_PLAIN, SWOFTY_MIXIN_SITE, 0, (void *)&stop_go_answer, nullptr},
                                   {PLAY, SWOFTY_MIXIN_WRAP, 0, (void *)&play, (void **)&play_original},
                                   {SCALED_DT, SWOFTY_MIXIN_SITE, 0, (void *)&scaled_dt, nullptr},
                                   {CLIP_START, SWOFTY_MIXIN_SITE, 0, (void *)&clip_start, nullptr}},
                        .armed = "the crew waits through a held car's extra seconds before it works",
                    }},
                    .on_armed = hold_armed,
                }};

}

int arm(uint32_t car, float seconds)
{
    if (car >= CARS)
        return SWOFTY_ASSET_INVALID;
    if (!hold_live.load())
        return SWOFTY_ASSET_UNAVAILABLE;
    float before = table.car(car).armed.load();
    table.arm(car, seconds);
    float now = table.car(car).armed.load();
    if (now != before && now > 0)
        swofty::log("[pit hold] car %u: next stop holds %.2f s more in the box", car, now);
    else if (now != before)
        swofty::log("[pit hold] car %u: next stop holds nothing extra", car);
    return SWOFTY_ASSET_OK;
}

int read(uint32_t car, SwoftyPitHold *out)
{
    if (car >= CARS || !out || out->size < sizeof(SwoftyPitHold))
        return SWOFTY_ASSET_INVALID;
    if (!hold_live.load())
        return SWOFTY_ASSET_UNAVAILABLE;
    Snapshot s = table.snapshot(car);
    uint32_t size = out->size;
    std::memset(out, 0, size);
    out->version = 1;
    out->size = size;
    out->armed = s.armed;
    out->holding = s.status == IN_PIT_BOX ? s.stop : 0.0f;
    out->served = s.served;
    out->serves = s.serves;
    out->crew = crew_live() ? s.crew : uint32_t(CREW_NONE);
    return SWOFTY_ASSET_OK;
}

void on_status(uint32_t car, uint32_t before, uint32_t after)
{
    if (!hold_live.load(std::memory_order_relaxed) || car >= CARS)
        return;
    float had = table.car(car).armed.load();
    float served = table.on_status(car, before, after);
    if (!in_lane(before) && in_lane(after) && had > 0)
        swofty::log("[pit hold] car %u: pit entry, this stop holds %.2f s more (crew %s)", car, had,
                    crew_live() ? crew_name(table.car(car).crew.load()) : "none");
    if (served > 0)
        swofty::log("[pit hold] car %u: served %.2f s extra in the box (status %u to %u)", car, served, before, after);
}

void reset() { table.reset_all(); }

}
