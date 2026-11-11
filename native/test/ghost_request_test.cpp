#include "ghost_request_logic.hpp"

#include <cstdio>

namespace gr = runtime::ghost_request;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    gr::Request r;
    expect(gr::parse(R"({"lap":"Z:\\a.trace","label":"SWOFTY 1:28.117","track":"Suzuka"})", r) == gr::Status::ok && r.mode == gr::Mode::ghost &&
               r.lap == "Z:\\a.trace" && r.label == "SWOFTY 1:28.117",
           "a ghost request with a track (lowercased)");
    expect(gr::parse(R"({"lap":"a.swlap","mode":"replay","driver":"Swooftyy","team":"ferrari","track":"suzuka"})", r) == gr::Status::ok &&
               r.mode == gr::Mode::replay && r.driver == "Swooftyy" && r.team == "ferrari" && r.label == "Swooftyy",
           "a replay request carries driver and team, the label falls back to the driver");
    expect(gr::parse(R"({"lap":"a","mode":"ghost","track":"x"})", r) == gr::Status::ok && r.mode == gr::Mode::ghost, "explicit ghost mode");
    expect(gr::parse(R"({"lap":"a","mode":"fly","track":"x"})", r) == gr::Status::bad_mode, "an unknown mode is refused");
    expect(gr::parse(R"({"lap":"a"})", r) == gr::Status::no_track && gr::parse(R"({"lap":"a","label":"x"})", r) == gr::Status::no_track,
           "a legacy request without a track is refused");
    expect(gr::parse(R"({"label":"x"})", r) == gr::Status::no_lap, "no lap is refused");
    expect(gr::parse("[1]", r) == gr::Status::not_object && gr::parse("", r) == gr::Status::not_object, "non-objects are refused");
    expect(gr::target_slot(gr::Mode::replay, 4, true) == 3, "replay always uses the personal best car");
    expect(gr::target_slot(gr::Mode::ghost, -1, true) == 4 && gr::target_slot(gr::Mode::ghost, -1, false) == 3, "ghost prefers a bound rival");
    expect(gr::target_slot(gr::Mode::ghost, 2, false) == 2, "the env slot overrides in ghost mode");
    gr::parse(R"({"lap":"a","track":"suzuka","content":"swofty.swofty-seasons-2016/sw_2016","weather":"dry","expires":2000})", r);
    gr::Session on{true, "swofty.swofty-seasons-2016/sw_2016", "Suzuka", "dry"};
    expect(gr::check(r, on, 1500, 1000) == gr::Verdict::ok, "the exact board is accepted");
    expect(gr::check(r, on, 2001, 1000) == gr::Verdict::expired, "past expires is discarded");
    gr::Session bahrain = on;
    bahrain.track = "bahrain";
    expect(gr::check(r, bahrain, 1500, 1000) == gr::Verdict::wrong_track, "another track is discarded (the Bahrain incident)");
    gr::Session other = on;
    other.content = "game/f1_2025";
    expect(gr::check(r, other, 1500, 1000) == gr::Verdict::wrong_content, "another content is discarded");
    gr::Session wet = on;
    wet.weather = "wet";
    expect(gr::check(r, wet, 1500, 1000) == gr::Verdict::wrong_weather, "another weather is discarded");
    gr::Session menu;
    expect(gr::check(r, menu, 1500, 1000) == gr::Verdict::not_time_trial, "no Time Trial session yet");
    gr::parse(R"({"lap":"a","track":"suzuka"})", r);
    expect(gr::check(r, on, 1000 + 1700, 1000) == gr::Verdict::ok && gr::check(r, on, 1000 + 1801, 1000) == gr::Verdict::expired,
           "without expires a request lives 30 minutes after it was read");
    if (failed)
        return 1;
    std::printf("all passed\n");
}
