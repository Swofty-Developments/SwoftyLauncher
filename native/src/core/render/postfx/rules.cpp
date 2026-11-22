#include "core/render/postfx/rules.hpp"

#include "core/live/live_car_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace render::postfx {

namespace {

std::string rule_name(const ActiveSpec &active, size_t i)
{
    const RuleSpec &r = active.rules[i];
    return r.name.empty() ? "#" + std::to_string(r.order) : r.name;
}

std::string describe_cond(const RuleCond &c, const Signals &sig)
{
    const SignalInfo &info = SIGNALS[c.signal];
    std::string id(info.id), unit(info.unit);
    char buf[96];
    if (c.op == OP_IS && c.signal == S_CAR_SURFACE) {
        std::string_view name;
        for (int w : sig.wheel)
            if (w >= 0 && w < 32 && (c.mask >> w & 1)) {
                name = signal_value_name(c.signal, size_t(w));
                break;
            }
        std::snprintf(buf, sizeof buf, " = %.*s, %d wheel%s", int(name.size()), name.data(), c.wheels, c.wheels == 1 ? "" : "s");
        return id + buf;
    }
    if (c.op == OP_IS) {
        std::string_view name = signal_value_name(c.signal, size_t(sig.num[c.signal]));
        std::snprintf(buf, sizeof buf, " = %.*s", int(name.size()), name.data());
        return id + buf;
    }
    if (info.kind == SK_NUMBER) {
        std::snprintf(buf, sizeof buf, " = %.3g%s%s", sig.num[c.signal], unit.empty() ? "" : " ", unit.c_str());
        return id + buf;
    }
    if (info.kind == SK_EVENT && sig.n_events[c.signal] && sig.magnitude[c.signal][0] != 0) {
        std::snprintf(buf, sizeof buf, " = %.3g%s%s", sig.magnitude[c.signal][0], unit.empty() ? "" : " ", unit.c_str());
        return id + buf;
    }
    return id;
}

std::string describe_rule(const ActiveSpec &active, size_t i, const Signals &sig)
{
    const RuleSpec &r = active.rules[i];
    std::string out;
    if (r.has_ramp)
        return describe_cond({r.ramp_signal, OP_ABOVE}, sig);
    for (auto &c : r.when)
        out += (out.empty() ? "" : ", ") + describe_cond(c, sig);
    return out;
}

std::string surface_label(int game_id)
{
    int v = surface_value(game_id);
    if (v < 0)
        return "none";
    return std::string(signal_value_name(S_CAR_SURFACE, size_t(v)));
}

}

void RuleDriver::reset(const ActiveSpec &active, const Plan &plan, std::vector<RenderPass> &passes)
{
    engine.reset(active.rules, plan);
    rule_log.assign(active.rules.size(), RuleLog{});
    loaded_at = GetTickCount64();
    minute_at = loaded_at + 120000;
    clock = {};
    reads_car = false;
    for (auto &r : active.rules) {
        auto car_signal = [](int sg) {
            return (sg >= S_CAR_SPEED && sg <= S_CAR_SURFACE) || sg == S_SESSION_WETNESS || sg == S_CAR_WETNESS;
        };
        reads_car |= r.has_ramp && car_signal(r.ramp_signal);
        for (auto &c : r.when)
            reads_car |= car_signal(c.signal);
    }
    for (auto &n : pending_events)
        n.store(0);
    for (auto &p : passes)
        if (p.inst >= 0)
            p.ruled_off = false;
    for (size_t i = 0; i < active.rules.size(); i++) {
        const RuleSpec &r = active.rules[i];
        int unread = r.has_ramp && !SIGNALS[r.ramp_signal].in_game ? r.ramp_signal : -1;
        for (auto &c : r.when)
            if (unread < 0 && !SIGNALS[c.signal].in_game)
                unread = c.signal;
        if (unread >= 0)
            swofty::log("[shaders] rule \"%s\": %.*s isn't read in game yet; it never fires", rule_name(active, i).c_str(),
                        int(SIGNALS[unread].id.size()), SIGNALS[unread].id.data());
    }
}

void RuleDriver::live_trace_tick(ULONGLONG now, const std::wstring &swofty_root)
{
    if (now - live_trace_checked >= 1000) {
        live_trace_checked = now;
        std::wstring file = swofty_root + L"shaders\\live.trace";
        if (GetFileAttributesW(file.c_str()) != INVALID_FILE_ATTRIBUTES) {
            std::string text = runtime::read_file(file, 64);
            DeleteFileW(file.c_str());
            unsigned secs = unsigned(std::strtoul(text.c_str(), nullptr, 10));
            live_trace_until = now + 1000ull * std::min(secs ? secs : 30u, 3600u);
            live_trace_line_at = 0;
            for (int &s : live_trace_surface)
                s = -2;
            swofty::log("[shaders] live car trace for %u s", secs ? secs : 30u);
        }
    }
}

bool RuleDriver::live_trace_on(ULONGLONG now) const { return live_trace_until && now < live_trace_until; }

void RuleDriver::live_trace_line(ULONGLONG now, const live_car::LiveCar &c)
{
    bool changed = false;
    for (int i = 0; i < 4; i++)
        if (live_trace_surface[i] != c.surface[i]) {
            changed = true;
            live_trace_surface[i] = c.surface[i];
        }
    if (!changed && now - live_trace_line_at < 1000)
        return;
    live_trace_line_at = now;
    const uint8_t *r = c.surface;
    swofty::log("[shaders] live car: speed=%.1f km/h gear=%d rpm=%.0f/%.0f thr=%.2f brk=%.2f steer=%.2f g=%.2f/%.2f "
                "surface=FL:%s FR:%s RL:%s RR:%s (raw %d %d %d %d) player=%d ai=%d wetness=%.3f wheels_wet=%.3f",
                c.speed_kmh, c.gear, c.rpm, c.max_rpm, c.throttle, c.brake, c.steer, c.g_lateral, c.g_long,
                surface_label(r[0]).c_str(), surface_label(r[1]).c_str(), surface_label(r[2]).c_str(),
                surface_label(r[3]).c_str(), r[0], r[1], r[2], r[3], c.player, c.ai ? 1 : 0,
                c.has_wetness ? double(c.wetness) : -1.0, c.has_wheel_wetness ? double(c.wheel_wetness) : -1.0);
}

void RuleDriver::live_frame(ULONGLONG now, const std::wstring &swofty_root)
{
    live_trace_tick(now, swofty_root);
    bool tracing = live_trace_on(now);
    live_ok = false;
    if (!reads_car && !tracing)
        return;
    live_ok = live_car::read(live_now);
    if (tracing && live_ok)
        live_trace_line(now, live_now);
    else if (tracing && now - live_trace_line_at >= 1000) {
        live_trace_line_at = now;
        swofty::log("[shaders] live car: no player car; wetness=%.3f", live_now.has_wetness ? double(live_now.wetness) : -1.0);
    }
}

void RuleDriver::frame(const frontend::view::State &v, ULONGLONG now, const ActiveSpec &active, const Plan &plan,
                       std::vector<RenderPass> &passes)
{
    if (!engine.rules())
        return;
    LARGE_INTEGER t, freq;
    QueryPerformanceCounter(&t);
    QueryPerformanceFrequency(&freq);
    double ms = clock.QuadPart ? double(t.QuadPart - clock.QuadPart) * 1000.0 / double(freq.QuadPart) : 0.0;
    clock = t;
    Signals sig;
    if (const int camera = view_camera_value(v.kind); v.valid && camera >= 0)
        sig.set(S_VIEW_CAMERA, camera);
    sig.set(S_VIEW_REPLAY, (v.flags & frontend::view::REPLAY) != 0);
    sig.set(S_VIEW_PAUSED, (v.flags & frontend::view::PAUSED) != 0);
    sig.set(S_VIEW_PHOTO, (v.flags & frontend::view::PHOTO) != 0);
    sig.set(S_VIEW_MENU, !in_session.load(std::memory_order_relaxed));
    sig.set(S_VIEW_CUTSCENE, (v.flags & frontend::view::CUTSCENE) != 0);
    sig.set(S_VIEW_SPECTATING, (v.flags & frontend::view::SPECTATING) != 0);
    if (live_ok && reads_car) {
        const live_car::LiveCar &c = live_now;
        if (c.has_speed)
            sig.set(S_CAR_SPEED, c.speed_kmh);
        if (c.has_rpm)
            sig.set(S_CAR_RPM, std::clamp(double(c.rpm) / double(c.max_rpm), 0.0, 1.0));
        if (c.has_gear)
            sig.set(S_CAR_GEAR, c.gear);
        if (c.has_pedals) {
            sig.set(S_CAR_THROTTLE, c.throttle);
            sig.set(S_CAR_BRAKE, c.brake);
        }
        if (c.has_steer)
            sig.set(S_CAR_STEER, c.steer);
        if (c.has_g) {
            sig.set(S_CAR_G_LATERAL, c.g_lateral);
            sig.set(S_CAR_G_LONG, c.g_long);
        }
        if (c.has_surface) {
            int w[4];
            for (int i = 0; i < 4; i++)
                w[i] = surface_value(c.surface[i]);
            sig.set_surface(w);
        }
    }
    if (reads_car && live_now.has_wetness)
        sig.set(S_SESSION_WETNESS, live_now.wetness);
    if (live_ok && reads_car && live_now.has_wheel_wetness)
        sig.set(S_CAR_WETNESS, live_now.wheel_wetness);
    for (int s : {S_SESSION_START, S_RACE_START, S_SESSION_LAP, S_SESSION_END}) {
        unsigned n = pending_events[s].exchange(0);
        for (unsigned i = 0; i < n && i < Signals::MAX_EVENTS; i++)
            sig.add_event(s);
    }
    engine.step(sig, ms);
    for (auto &p : passes)
        if (p.inst >= 0 && size_t(p.inst) < plan.instances.size()) {
            memcpy(p.options, engine.options(size_t(p.inst)), sizeof p.options);
            p.ruled_off = engine.off(size_t(p.inst));
        }
    bool early = now - loaded_at < 60000;
    for (size_t i = 0; i < rule_log.size(); i++) {
        const auto &st = engine.state(i);
        RuleLog &l = rule_log[i];
        if (!st.changed)
            continue;
        if (st.on) {
            l.since = now;
            if (!early)
                l.count++;
            else
                swofty::log("[shaders] rule \"%s\" on (%s)", rule_name(active, i).c_str(), describe_rule(active, i, sig).c_str());
        } else if (early) {
            swofty::log("[shaders] rule \"%s\" off after %.1f s", rule_name(active, i).c_str(), double(now - l.since) / 1000.0);
        }
    }
    if (now >= minute_at) {
        minute_at += 60000;
        for (size_t i = 0; i < rule_log.size(); i++) {
            if (rule_log[i].count)
                swofty::log("[shaders] rule \"%s\": %u time(s) on in the last 60 s", rule_name(active, i).c_str(),
                            rule_log[i].count);
            rule_log[i].count = 0;
        }
    }
}

// The vehicle table and CarLive.is_ai +0x124 are the live car's reads (live_car::CAR_TABLE names their builds).
bool RuleDriver::player_car(int64_t index)
{
    if (!car_tried) {
        car_tried = true;
        if (runtime::build_proof::proven_on(live_car::CAR_TABLE, runtime::running_build()) && swofty::api) {
            car_table = swofty::api->resolve("g_Sess_VehicleTable");
            car_count = swofty::api->resolve("g_Sess_VehicleCount");
        }
        if (!car_table || !car_count)
            swofty::log("[shaders] session.lap is not read: the vehicle table does not resolve, so the player's car "
                        "cannot be told from the AI's");
    }
    if (!car_table || !car_count || index < 0 || index >= 26)
        return false;
    auto *n = reinterpret_cast<const uint32_t *>(car_count);
    auto *const *cars = reinterpret_cast<uint8_t *const *>(car_table);
    if (!runtime::readable(n, 4) || !runtime::readable(cars, 8 * 26) || uint64_t(index) >= *n)
        return false;
    const uint8_t *car = cars[index];
    return car && runtime::readable(car, 0x130) && car[0x124] == 0;
}

void RuleDriver::on_event(const SwoftyEvent *ev, void *user)
{
    const Feed &feed = *static_cast<const Feed *>(user);
    RuleDriver &d = *feed.driver;
    int s = feed.signal;
    if (s == S_SESSION_LAP && !d.player_car(ev->a))
        return;
    if (s == S_SESSION_START || s == S_SESSION_END)
        d.in_session.store(s == S_SESSION_START, std::memory_order_relaxed);
    d.pending_events[s].fetch_add(1);
}

void RuleDriver::subscribe(const SwoftyEvents *ev)
{
    const char *names[4] = {SWOFTY_EVENT_SESSION_START, SWOFTY_EVENT_RACE_START, SWOFTY_EVENT_LAP, SWOFTY_EVENT_SESSION_END};
    for (int i = 0; i < 4; i++) {
        uint32_t token = 0;
        if (ev->subscribe(names[i], on_event, &feeds[i], &token) != SWOFTY_ASSET_OK)
            swofty::log("[shaders] %s is not read: the core's events are off", names[i]);
    }
}

}
