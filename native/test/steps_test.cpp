#include "core/diagnostics/steps.hpp"

#include <cstdio>
#include <regex>
#include <string>

namespace {

int failures = 0;

void check(bool ok, const char *what)
{
    if (!ok) {
        std::printf("FAIL %s\n", what);
        failures++;
    }
}

template <class... A> std::string line(const char *fmt, A... a)
{
    char buf[512];
    std::snprintf(buf, sizeof buf, fmt, a...);
    return buf;
}

void step_line()
{
    std::regex re(R"(^\[perf\] step ([0-9]+\.[0-9]) ms: (.+)$)");
    std::smatch m;
    std::string s = line(perf::STEP_LINE, 8412.34, "addon init offline_quiet.dll");
    check(s == "[perf] step 8412.3 ms: addon init offline_quiet.dll", "a step line reads as documented");
    check(std::regex_match(s, m, re) && m[1] == "8412.3" && m[2] == "addon init offline_quiet.dll",
          "the documented pattern takes the milliseconds and the name");
    s = line(perf::STEP_LINE, 0.04, "addon init a (again, its code decrypted)");
    check(std::regex_match(s, m, re) && m[1] == "0.0", "a step under a tenth of a millisecond still matches");
}

void slow_addon_line()
{
    std::regex re(R"(^\[perf\] slow addon init ([0-9]+) ms: (.+)$)");
    std::smatch m;
    std::string s = line(perf::SLOW_ADDON_INIT_LINE, 8412.6, "twilight.offline-quiet");
    check(s == "[perf] slow addon init 8413 ms: twilight.offline-quiet", "a slow addon line reads as documented");
    check(std::regex_match(s, m, re) && m[1] == "8413" && m[2] == "twilight.offline-quiet",
          "the documented pattern takes the milliseconds and the addon");
    s = line(perf::SLOW_ADDON_INIT_LINE, 1000.4, "my addon (1).dll");
    check(std::regex_match(s, m, re) && m[2] == "my addon (1).dll", "a dll name with spaces comes back whole");
    check(perf::SLOW_ADDON_INIT_MS == 1000.0, "the slow addon threshold is one second");
}

}

int main()
{
    step_line();
    slow_addon_line();
    if (failures) {
        std::printf("%d failure(s)\n", failures);
        return 1;
    }
    std::printf("steps: ok\n");
    return 0;
}
