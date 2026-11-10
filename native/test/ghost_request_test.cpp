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
    expect(gr::parse(R"({"lap":"Z:\\a.trace","label":"SWOFTY 1:28.117"})", r) == gr::Status::ok && r.mode == gr::Mode::ghost &&
               r.lap == "Z:\\a.trace" && r.label == "SWOFTY 1:28.117",
           "the old two-key file is a ghost request");
    expect(gr::parse(R"({"lap":"a.swlap","mode":"replay","driver":"Swooftyy","team":"ferrari"})", r) == gr::Status::ok &&
               r.mode == gr::Mode::replay && r.driver == "Swooftyy" && r.team == "ferrari" && r.label == "Swooftyy",
           "a replay request carries driver and team, the label falls back to the driver");
    expect(gr::parse(R"({"lap":"a","mode":"ghost"})", r) == gr::Status::ok && r.mode == gr::Mode::ghost, "explicit ghost mode");
    expect(gr::parse(R"({"lap":"a","mode":"fly"})", r) == gr::Status::bad_mode, "an unknown mode is refused");
    expect(gr::parse(R"({"label":"x"})", r) == gr::Status::no_lap, "no lap is refused");
    expect(gr::parse("[1]", r) == gr::Status::not_object && gr::parse("", r) == gr::Status::not_object, "non-objects are refused");
    expect(gr::target_slot(gr::Mode::replay, 4, true) == 3, "replay always uses the personal best car");
    expect(gr::target_slot(gr::Mode::ghost, -1, true) == 4 && gr::target_slot(gr::Mode::ghost, -1, false) == 3, "ghost prefers a bound rival");
    expect(gr::target_slot(gr::Mode::ghost, 2, false) == 2, "the env slot overrides in ghost mode");
    if (failed)
        return 1;
    std::printf("all passed\n");
}
