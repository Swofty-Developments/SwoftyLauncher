#pragma once

#include "ghost_trace_logic.hpp"
#include "lap_file_logic.hpp"

#include <string>
#include <vector>

namespace runtime::ghost_trace {

struct FromLap {
    std::vector<Sample> samples;
    Plan plan;
    bool oriented = false;
};

inline bool samples_of(const lap_file::Parsed &p, FromLap &out, std::string *why = nullptr)
{
    auto fail = [&](const char *m) {
        if (why)
            *why = m;
        return false;
    };
    const auto &L = p.layout;
    int ct = L.index("t"), cx = L.index("px"), cy = L.index("py"), cz = L.index("pz");
    if (ct < 0 || cx < 0 || cy < 0 || cz < 0)
        return fail("the lap file has no time or position columns");
    int cd = L.index("dist"), vx = L.index("vx"), vy = L.index("vy"), vz = L.index("vz"), th = L.index("throttle"),
        br = L.index("brake"), st = L.index("steer"), ge = L.index("gear"), rp = L.index("rpm"), dr = L.index("drs"),
        qx = L.index("qx"), qy = L.index("qy"), qz = L.index("qz"), qw = L.index("qw");
    bool quat = qx >= 0 && qy >= 0 && qz >= 0 && qw >= 0;
    auto val = [&](const uint8_t *s, int c) { return c >= 0 ? float(L.read(s, size_t(c))) : 0.0f; };
    out = FromLap{};
    out.samples.reserve(p.count);
    bool any_quat = false;
    for (uint32_t i = 0; i < p.count; i++) {
        const uint8_t *s = p.sample(i);
        Sample x;
        x.t = val(s, ct);
        x.dist = val(s, cd);
        x.pos[0] = val(s, cx);
        x.pos[1] = val(s, cy);
        x.pos[2] = val(s, cz);
        x.vel[0] = val(s, vx);
        x.vel[1] = val(s, vy);
        x.vel[2] = val(s, vz);
        x.throttle = val(s, th);
        x.brake = val(s, br);
        x.steer = val(s, st);
        x.gear = int(val(s, ge));
        x.rpm = val(s, rp);
        x.drs = val(s, dr);
        if (quat) {
            x.quat[0] = val(s, qx);
            x.quat[1] = val(s, qy);
            x.quat[2] = val(s, qz);
            x.quat[3] = val(s, qw);
            float n = x.quat[0] * x.quat[0] + x.quat[1] * x.quat[1] + x.quat[2] * x.quat[2] + x.quat[3] * x.quat[3];
            x.has_quat = n > 0.9f && n < 1.1f;
            any_quat = any_quat || x.has_quat;
        }
        for (float f : {x.t, x.pos[0], x.pos[1], x.pos[2]})
            if (!std::isfinite(f))
                return fail("a sample has a non-finite time or position");
        out.samples.push_back(x);
    }
    out.oriented = any_quat;
    const auto &lap = p.header["lap"];
    out.plan.lap_s = float(lap["time_ms"].as_int(0)) / 1000.0f;
    const swofty::json::Array *sec = lap["sectors_ms"].if_array();
    if (sec && sec->size() == 3)
        for (int i = 0; i < 3; i++)
            out.plan.sectors_s[i] = float((*sec)[size_t(i)].as_int(0)) / 1000.0f;
    out.plan.clean = lap["valid"].as_bool(true);
    return true;
}

inline std::optional<Trace> trace_of_lap(const lap_file::Parsed &p, std::string *why = nullptr)
{
    FromLap f;
    if (!samples_of(p, f, why))
        return std::nullopt;
    if (!(f.plan.sectors_s[0] > 0 && f.plan.sectors_s[1] > 0 && f.plan.sectors_s[2] > 0) && f.plan.lap_s > 0) {
        f.plan.sectors_s[0] = f.plan.lap_s * 0.34f;
        f.plan.sectors_s[1] = f.plan.lap_s * 0.33f;
        f.plan.sectors_s[2] = f.plan.lap_s - f.plan.sectors_s[0] - f.plan.sectors_s[1];
    }
    return from_samples(f.samples, f.plan, why);
}

}
