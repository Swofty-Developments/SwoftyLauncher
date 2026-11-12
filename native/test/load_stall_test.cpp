// Host-native unit test for the core's load-stall watchdog trigger (native/src/core/load_stall_logic.hpp): when a
// load that has stopped moving gets its reports, replayed from the flow and screen sequences of the 1.9.9 Suzuka
// Time Trial stall (session 20261112-013117) and of working loads. Built and run by
// native/test/run_load_stall_test.sh.

#include "load_stall_logic.hpp"

#include <cstdio>
#include <cstring>

namespace ls = runtime::load_stall;

static int g_fail;

#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            g_fail++;                                                                                                  \
        }                                                                                                              \
    } while (0)

static void menus(ls::Watch &w, uint64_t t)
{
    w.flow(0x5f9ae18b, ls::Entering, "#5f9ae18b", t);
    w.flow(0x5f9ae18b, ls::Updating, "#5f9ae18b", t + 10);
    w.screen("fe_home", t + 1000);
    w.screen("solo_container", t + 2000);
    w.screen("track_select", t + 3000);
}

static void stall_start(ls::Watch &w, uint64_t t)
{
    w.flow(0x5f9ae18b, ls::Exiting, "#5f9ae18b", t);
    w.flow(0x455cf081, ls::Entering, "#455cf081", t + 1);
    w.screen("loading_transitions", t + 20);
    w.flow(0x455cf081, ls::Exiting, "#455cf081", t + 30);
}

int main()
{
    {
        ls::Watch w;
        CHECK(!w.poll(0).report);
        CHECK(!w.poll(10'000'000).report);
        CHECK(w.threshold() == 0);
    }
    {
        ls::Watch w;
        menus(w, 1000);
        CHECK(w.threshold() == 0);
        CHECK(!w.poll(3'600'000).report);
    }
    {
        ls::Watch w;
        menus(w, 1000);
        stall_start(w, 10000);
        uint64_t last = 10030;
        CHECK(w.threshold() == ls::kLoadingMs);
        CHECK(!w.poll(last + ls::kLoadingMs - 1).report);
        ls::Due d = w.poll(last + ls::kLoadingMs);
        CHECK(d.report && d.index == 1 && d.stalled_ms == ls::kLoadingMs && d.threshold_ms == ls::kLoadingMs);
        CHECK(!w.poll(last + ls::kLoadingMs + 1).report);
        CHECK(!w.poll(last + 3 * ls::kLoadingMs - 1).report);
        d = w.poll(last + 3 * ls::kLoadingMs);
        CHECK(d.report && d.index == 2);
        CHECK(!w.poll(last + 100 * ls::kLoadingMs).report);
        CHECK(w.reports() == 2);
        char text[2048];
        size_t n = w.describe(text, sizeof text, last + 170000);
        CHECK(n > 0 && n < sizeof text);
        CHECK(std::strstr(text, "newest screen \"loading_transitions\", opened 170.0 s ago (a loading screen)"));
        CHECK(std::strstr(text, "last flow transition: \"#455cf081\" -> Exiting, 170.0 s ago"));
        CHECK(std::strstr(text, "\"#455cf081\" in Exiting for 170.0 s"));
        CHECK(std::strstr(text, "\"#5f9ae18b\" in Exiting"));
        CHECK(std::strstr(text, "session load flow: none"));
    }
    {
        ls::Watch w;
        menus(w, 1000);
        stall_start(w, 10000);
        w.screen("loading", 10500);
        w.flow(0xe2dcf787, ls::Entering, "#e2dcf787", 10550);
        w.flow(0xe2dcf787, ls::Exiting, "#e2dcf787", 10551);
        w.flow(0xe2dcf787, ls::None, "#e2dcf787", 10552);
        w.flow(0x1, ls::Entering, "StateLoadSession", 10560);
        CHECK(w.threshold() == ls::kSessionLoadMs);
        w.flow(0x1, ls::Updating, "StateLoadSession", 10561);
        CHECK(!w.poll(10561 + 169000).report);
        w.screen("render_badge", 10561 + 169000);
        w.flow(0x1, ls::Exiting, "StateLoadSession", 10561 + 170000);
        w.flow(0x1, ls::None, "StateLoadSession", 10561 + 170001);
        w.flow(0x2, ls::Entering, "StateSession", 10561 + 170010);
        w.flow(0x2, ls::Updating, "StateSession", 10561 + 170020);
        w.screen("hud", 10561 + 171000);
        CHECK(w.threshold() == 0);
        CHECK(!w.poll(10561 + 3'600'000).report);
        CHECK(w.reports() == 0);
    }
    {
        ls::Watch w;
        w.flow(0x1, ls::Entering, "StateLoadSession", 0);
        w.flow(0x1, ls::Updating, "StateLoadSession", 5);
        ls::Due d = w.poll(5 + ls::kSessionLoadMs);
        CHECK(d.report && d.index == 1 && d.threshold_ms == ls::kSessionLoadMs);
        char text[2048];
        w.describe(text, sizeof text, 5 + ls::kSessionLoadMs);
        CHECK(std::strstr(text, "session load flow: \"StateLoadSession\" in Updating for 180.0 s"));
    }
    {
        ls::Watch w;
        stall_start(w, 0);
        CHECK(w.poll(30 + ls::kLoadingMs).report);
        w.flow(0x77, ls::Entering, "#00000077", 30 + ls::kLoadingMs + 10);
        CHECK(!w.poll(30 + ls::kLoadingMs + 11).report);
        ls::Due d = w.poll(30 + 2 * ls::kLoadingMs + 10);
        CHECK(d.report && d.index == 1);
        CHECK(w.poll(30 + 4 * ls::kLoadingMs + 10).report);
        CHECK(w.reports() == 3);
        w.screen("loading", 30 + 5 * ls::kLoadingMs);
        CHECK(w.poll(30 + 6 * ls::kLoadingMs).report);
        CHECK(w.reports() == ls::kMaxReports);
        w.screen("loading", 30 + 7 * ls::kLoadingMs);
        CHECK(!w.poll(30 + 20 * ls::kLoadingMs).report);
    }
    {
        ls::Watch w;
        w.loading_ms = 15000;
        stall_start(w, 0);
        CHECK(!w.poll(30 + 14999).report);
        CHECK(w.poll(30 + 15000).report);
        CHECK(w.poll(30 + 45000).report);
    }
    {
        ls::Watch w;
        w.screen("loading_transitions", 0);
        CHECK(!w.poll(0).report);
        CHECK(w.in_loading_screen());
        w.screen("pause", 1);
        CHECK(!w.in_loading_screen() && w.threshold() == 0);
    }
    {
        ls::Watch w;
        for (uint32_t i = 0; i < 100; i++)
            w.flow(0x1000 + i, ls::Updating, "#menu", i);
        w.flow(0x1, ls::Entering, "StateUnloadSession", 200);
        for (uint32_t i = 0; i < 100; i++)
            w.flow(0x2000 + i, ls::Entering, "#later", 300 + i);
        CHECK(w.session_flow() && !std::strcmp(w.session_flow()->name, "StateUnloadSession"));
        w.flow(0x1, ls::None, "StateUnloadSession", 500);
        CHECK(!w.session_flow());
    }
    {
        ls::Watch w;
        stall_start(w, 0);
        char small[24];
        std::memset(small, 'x', sizeof small);
        size_t n = w.describe(small, sizeof small, 1000);
        CHECK(n < sizeof small && small[n] == 0 && std::strlen(small) == n);
    }
    CHECK(ls::loading_screen("loading_transitions") && ls::loading_screen("loading") && !ls::loading_screen("hud"));
    CHECK(!ls::loading_screen(nullptr));
    CHECK(ls::session_load_flow("StateLoadSession") && ls::session_load_flow("StateUnloadSession"));
    CHECK(!ls::session_load_flow("StateSession") && !ls::session_load_flow(nullptr));
    CHECK(!std::strcmp(ls::status_name(3), "Exiting") && !std::strcmp(ls::status_name(9), "?"));
    if (g_fail) {
        std::printf("load stall test: %d failure(s)\n", g_fail);
        return 1;
    }
    std::printf("load stall test OK\n");
    return 0;
}
