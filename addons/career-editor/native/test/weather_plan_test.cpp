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
    CHECK(err == "sessions.r: timeline[0].at_lap needs laps on the session and at_lap <= laps");

    std::printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL OK", fails);
    return fails ? 1 : 0;
}
