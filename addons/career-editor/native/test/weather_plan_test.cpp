// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Host unit test for weather_plan.cpp: parsing a session plan, building it and clamping its intensities.
#include <cmath>
#include <cstdio>
#include <string>

#include "weather_plan.hpp"

using namespace ce::weather;
static int fails = 0;
#define CHECK(c) do { if (!(c)) { std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c); ++fails; } } while (0)

static bool plan(const char *text, SessionPlan &p, std::string &err) {
    ce::Json v; if (!json::parse(text, v)) { err = "invalid JSON"; return false; }
    p = SessionPlan{}; return parse_session(v, "r", p, err); }
static bool near(float a, float b) { return std::fabs(a - b) < 1e-4f; }

int main() {
    const TypeRange suzuka[6] = {{0, 0}, {0, 0}, {0, 0}, {0.1f, 0.4f}, {0.41f, 0.7f}, {1, 1}};
    SessionPlan p; std::string err;

    // the reported plan: clear, then light rain at a third of the race with 0% intensity
    CHECK(plan(R"({"start":"clear","timeline":[{"at_fraction":0.333,"state":"light_rain","intensity":0}]})", p, err));
    Build b = make_build(p, 7200, 1);
    CHECK(b.n == 2 && b.type[0] == 0 && b.type[1] == 3);
    CHECK(b.inten[0] < 0 && b.inten[1] == 0);
    auto lines = clamp_build(b, suzuka);
    CHECK(lines.size() == 1);
    CHECK(near(b.inten[1], 0.1f));
    CHECK(b.inten[0] < 0);
    CHECK(lines.size() == 1 && lines[0] == "state 1 light_rain intensity 0.00 -> 0.10 (game range 0.10..0.40)");

    // in-range values stay, out-of-range ones move to the nearest bound, dry types go to their 0..0
    CHECK(plan(R"({"laps":53,"timeline":[{"at_lap":1,"state":"overcast","intensity":0.3},{"at_lap":10,"state":"light_rain","intensity":0.25},
                  {"at_lap":20,"state":"heavy_rain","intensity":0.95},{"at_lap":30,"state":"storm"}]})", p, err));
    b = make_build(p, 7200, -1);
    CHECK(b.n == 4);
    lines = clamp_build(b, suzuka);
    CHECK(lines.size() == 2);
    CHECK(b.inten[0] == 0 && near(b.inten[1], 0.25f) && near(b.inten[2], 0.7f) && b.inten[3] < 0);

    // an unusable range leaves the state to the game's draw
    const TypeRange broken[6] = {{0, 0}, {0, 0}, {0, 0}, {0.4f, 0.1f}, {NAN, 0.7f}, {0, 2}};
    CHECK(plan(R"({"timeline":[{"at_fraction":0,"state":"light_rain","intensity":0.2},{"at_fraction":0.5,"state":"heavy_rain","intensity":0.5},
                  {"at_fraction":0.8,"state":"storm","intensity":1}]})", p, err));
    b = make_build(p, 3600, -1);
    lines = clamp_build(b, broken);
    CHECK(lines.size() == 3);
    CHECK(b.inten[0] < 0 && b.inten[1] < 0 && b.inten[2] < 0);

    // no zero-length hold or transition reaches the builder, whatever the ramp or position
    CHECK(plan(R"({"timeline":[{"at_fraction":0.5,"state":"light_rain","ramp_s":0},{"at_fraction":0.5001,"state":"overcast","ramp_s":0},
                  {"at_fraction":1,"state":"heavy_rain"}]})", p, err));
    b = make_build(p, 3600, -1);
    CHECK(b.n >= 2);
    for (uint32_t i = 0; i + 1 < b.n; ++i)
        CHECK(b.trans[i] >= kMinRampS && b.hold[i] >= 0 && (i == 0 || b.hold[i] >= kMinHoldS));
    CHECK(b.hold[b.n - 1] >= 1.0f && b.n + b.dropped == 4);

    // a plan already inside the range is not touched
    CHECK(plan(R"({"timeline":[{"at_fraction":0,"state":"light_rain","intensity":0.3}]})", p, err));
    b = make_build(p, 3600, -1);
    CHECK(clamp_build(b, suzuka).empty() && near(b.inten[0], 0.3f));

    // the parser still bounds intensity to 0..1 and needs exactly one position unit
    CHECK(!plan(R"({"timeline":[{"at_fraction":0,"state":"light_rain","intensity":1.5}]})", p, err));
    CHECK(err == "sessions.r: timeline[0].intensity must be 0..1");
    CHECK(!plan(R"({"timeline":[{"at_fraction":0,"at_minute":3,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"timeline":[{"at_lap":3,"state":"clear"}]})", p, err));
    CHECK(err == "sessions.r: timeline[0].at_lap needs laps on the session (laps is missing or 0)");

    // 0 laps, negative laps and non-finite times are refused instead of becoming a time
    CHECK(!plan(R"({"laps":0,"timeline":[{"at_lap":5,"state":"clear"}]})", p, err));
    CHECK(err.find("laps must be an integer 1..200") != std::string::npos);
    CHECK(!plan(R"({"laps":-3,"timeline":[{"at_lap":1,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"laps":53,"timeline":[{"at_lap":0,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"timeline":[{"at_minute":-1,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"timeline":[{"at_minute":1e999,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"timeline":[{"at_fraction":1e999,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"laps":1e999,"timeline":[{"at_lap":1,"state":"clear"}]})", p, err));

    // a zero or non-finite session length cannot produce a NaN or infinite point
    CHECK(plan(R"({"timeline":[{"at_minute":10,"state":"heavy_rain"}]})", p, err));
    for (float len : {0.0f, -5.0f, (float)NAN, (float)INFINITY}) {
        b = make_build(p, len, -1);
        for (uint32_t i = 0; i < b.n; ++i)
            CHECK(std::isfinite(b.hold[i]) && std::isfinite(b.trans[i]) && b.hold[i] >= 0 && (i + 1 == b.n || b.trans[i] >= kMinRampS));
    }

    // a lap point built by hand with laps 0 converts to a finite time, not a division by zero
    SessionPlan z; z.unit = kLap; z.laps = 0; z.pts.push_back({20, 4, -1, -1});
    b = make_build(z, 3600, -1);
    for (uint32_t i = 0; i < b.n; ++i)
        CHECK(std::isfinite(b.hold[i]) && std::isfinite(b.trans[i]) && b.hold[i] >= 0 && b.trans[i] >= 0);


    // no built plan, whatever the plan, length or unit, holds a tiny, zero, non-finite or out-of-range value
    {
        const char *plans[] = {
            R"({"timeline":[{"at_fraction":1e-12,"state":"heavy_rain"}]})",
            R"({"timeline":[{"at_minute":0.0001,"state":"heavy_rain"}]})",
            R"({"timeline":[{"at_fraction":0.001,"state":"storm","intensity":1},{"at_fraction":0.002,"state":"clear"}]})",
            R"({"timeline":[{"at_fraction":1,"state":"storm"}]})",
            R"({"timeline":[{"at_minute":240,"state":"storm"}]})",
            R"({"minutes":1,"timeline":[{"at_minute":1,"state":"storm"}]})",
            R"({"laps":1,"timeline":[{"at_lap":1,"state":"light_rain"}]})",
            R"({"laps":200,"timeline":[{"at_lap":2,"state":"light_rain"},{"at_lap":3,"state":"clear"}]})",
            R"({"laps":2,"timeline":[{"at_lap":2,"state":"light_rain"}]})",
            R"({"timeline":[{"at_fraction":0.5,"state":"light_rain","ramp_s":1800},{"at_fraction":0.51,"state":"clear","ramp_s":1800},
                {"at_fraction":0.52,"state":"storm"},{"at_fraction":0.53,"state":"clear"},{"at_fraction":0.54,"state":"overcast"}]})",
        };
        for (const char *text : plans) {
            CHECK(plan(text, p, err));
            for (float len : {0.0f, 0.5f, 59.0f, 60.0f, 90.0f, 3600.0f, 36000.0f, 1e9f, (float)NAN, (float)INFINITY, -1.0f}) {
                b = make_build(p, len, -1);
                float eff = (std::isfinite(len) && len >= 60 && len <= 36000) ? len : 3600.0f;
                std::string why = validate_build(b, eff);
                if (!why.empty()) std::printf("  %s len %g: %s\n", text, len, why.c_str());
                CHECK(why.empty());
            }
        }
    }

    // validate_build refuses what the builder must never emit
    {
        Build v; v.n = 2; v.type[0] = 0; v.type[1] = 3; v.hold[0] = 100; v.trans[0] = 10; v.hold[1] = 100; v.trans[1] = 0;
        CHECK(validate_build(v, 3600).empty());
        Build x = v; x.hold[0] = 0; CHECK(!validate_build(x, 3600).empty());
        x = v; x.hold[0] = 1e-6f; CHECK(!validate_build(x, 3600).empty());
        x = v; x.hold[1] = (float)NAN; CHECK(!validate_build(x, 3600).empty());
        x = v; x.trans[0] = (float)INFINITY; CHECK(!validate_build(x, 3600).empty());
        x = v; x.trans[0] = 0; CHECK(!validate_build(x, 3600).empty());
        x = v; x.trans[1] = 5; CHECK(!validate_build(x, 3600).empty());
        x = v; x.inten[1] = (float)NAN; CHECK(!validate_build(x, 3600).empty());
        x = v; x.inten[1] = 1.5f; CHECK(!validate_build(x, 3600).empty());
        x = v; x.type[1] = 9; CHECK(!validate_build(x, 3600).empty());
        x = v; x.n = 0; CHECK(!validate_build(x, 3600).empty());
        x = v; x.hold[1] = 5000; CHECK(!validate_build(x, 3600).empty());
        CHECK(!validate_build(v, 0).empty() && !validate_build(v, (float)NAN).empty());
    }

    // two changes at the same point are refused; a change on the heels of another is held apart
    CHECK(!plan(R"({"timeline":[{"at_minute":5,"state":"clear"},{"at_minute":5,"state":"storm"}]})", p, err));
    CHECK(!plan(R"({"laps":10,"timeline":[{"at_lap":4,"state":"clear"},{"at_lap":4,"state":"storm"}]})", p, err));
    CHECK(plan(R"({"timeline":[{"at_minute":5,"state":"clear"},{"at_minute":5.0001,"state":"storm"}]})", p, err));
    b = make_build(p, 3600, -1);
    CHECK(validate_build(b, 3600).empty() && b.hold[1] >= kMinHoldS);

    // a first change right after the start keeps the opening state at least kMinHoldS long
    CHECK(plan(R"({"timeline":[{"at_fraction":1e-9,"state":"storm"}]})", p, err));
    b = make_build(p, 3600, -1);
    CHECK(b.hold[0] >= kMinHoldS);

    // lap changes are refused in timed sessions, which have no lap count
    for (const char *key : {"p1", "p2", "p3", "q", "ss"}) {
        ce::Json v; CHECK(json::parse(R"({"laps":10,"timeline":[{"at_lap":3,"state":"clear"}]})", v));
        SessionPlan t; std::string e;
        CHECK(!parse_session(v, key, t, e));
        CHECK(e.find("timed") != std::string::npos);
    }
    for (const char *key : {"r", "r2", "r3"}) {
        ce::Json v; CHECK(json::parse(R"({"laps":10,"timeline":[{"at_lap":3,"state":"clear"}]})", v));
        SessionPlan t; std::string e;
        CHECK(parse_session(v, key, t, e));
    }

    // temperatures, ramps and minutes outside their ranges, and non-numbers, are refused
    CHECK(!plan(R"({"air_temp":1e999,"timeline":[{"at_minute":3,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"track_temp":-50,"timeline":[{"at_minute":3,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"timeline":[{"at_minute":3,"state":"clear","ramp_s":-1}]})", p, err));
    CHECK(!plan(R"({"timeline":[{"at_minute":3,"state":"clear","ramp_s":1e999}]})", p, err));
    CHECK(!plan(R"({"timeline":[{"at_minute":3,"state":"clear","intensity":-0.1}]})", p, err));
    CHECK(!plan(R"({"minutes":0,"timeline":[{"at_minute":3,"state":"clear"}]})", p, err));
    CHECK(!plan(R"({"minutes":10,"timeline":[{"at_minute":11,"state":"clear"}]})", p, err));

    // a hand-built empty plan or a non-finite position never reaches a division
    SessionPlan e0; b = make_build(e0, 3600, -1);
    CHECK(validate_build(b, 3600).empty());
    SessionPlan nf; nf.unit = kMinute; nf.pts.push_back({(double)NAN, 3, -1, -1});
    b = make_build(nf, 3600, -1);
    CHECK(validate_build(b, 3600).empty());


    // a plan follows its round when the calendar changes
    {
        std::vector<std::string> before = {"rw_a", "rw_b", "rw_c", "rw_d"};
        std::vector<PlanRef> refs = {{3, "rw_d"}, {2, "rw_c"}, {-1, ""}};
        auto m = plan_moves(refs, before);
        CHECK(m.size() == 3 && !m[0].drop && m[0].to == 3 && !m[1].drop && m[1].to == 2 && m[2].to == -1 && !m[2].stamp);
        std::vector<std::string> added = {"rw_a", "rw_b", "rw_sochi", "rw_c", "rw_d"};
        m = plan_moves(refs, added);
        CHECK(!m[0].drop && m[0].to == 4 && !m[1].drop && m[1].to == 3 && m[2].to == -1);
        std::vector<std::string> removed = {"rw_a", "rw_d"};
        m = plan_moves(refs, removed);
        CHECK(!m[0].drop && m[0].to == 1 && m[1].drop);
        std::vector<std::string> reordered = {"rw_d", "rw_a", "rw_c", "rw_b"};
        m = plan_moves(refs, reordered);
        CHECK(m[0].to == 0 && !m[0].drop && m[1].to == 2 && !m[1].drop);
        std::vector<std::string> replaced = {"rw_a", "rw_b", "rw_x", "rw_d"};
        m = plan_moves({{2, "rw_c"}}, replaced);
        CHECK(m[0].drop);
        m = plan_moves({{2, ""}, {9, ""}}, before);
        CHECK(m[0].stamp && !m[0].drop && m[0].to == 2 && !m[1].stamp && !m[1].drop && m[1].to == 9);
    }

    // a wet state the track's range cannot draw is refused, dry types and a good range are not
    {
        Build wet; wet.n = 2; wet.type[0] = 0; wet.type[1] = 3;
        const TypeRange good[6] = {{0, 0}, {0, 0}, {0, 0}, {0.1f, 0.4f}, {0.41f, 0.7f}, {1, 1}};
        CHECK(unsupported_state(wet, good).empty());
        const TypeRange none[6] = {{0, 0}, {0, 0}, {0, 0}, {0, 0}, {0.41f, 0.7f}, {1, 1}};
        CHECK(unsupported_state(wet, none) == "light_rain");
        Build storm; storm.n = 1; storm.type[0] = 5;
        const TypeRange nostorm[6] = {{0, 0}, {0, 0}, {0, 0}, {0.1f, 0.4f}, {0.41f, 0.7f}, {0, 0}};
        CHECK(unsupported_state(storm, nostorm) == "storm" && unsupported_state(storm, good).empty());
        const TypeRange bad[6] = {{0, 0}, {0, 0}, {0, 0}, {0.1f, 0.4f}, {(float)NAN, 0.7f}, {1, 1}};
        Build heavy; heavy.n = 1; heavy.type[0] = 4;
        CHECK(unsupported_state(heavy, bad) == "heavy_rain");
        Build dry; dry.n = 1; dry.type[0] = 2;
        CHECK(unsupported_state(dry, none).empty());
    }

    std::printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL OK", fails);
    return fails ? 1 : 0;
}
