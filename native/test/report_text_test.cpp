// Host-native unit test for the crash report's section text (native/src/core/diagnostics/crash/report_text_logic.hpp)
// over fixed input: the Addons section, the probe and stall lines, the ray tracing build lines and their log lines,
// the loader.log tail, the pointers in a heap error message and the call before a return address.
// Built and run by native/test/run_tests.sh report_text.

#include "core/diagnostics/crash/report_text_logic.hpp"

#include <cstdarg>
#include <cstdio>
#include <string>
#include <vector>

namespace rt = crash::report_text;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

static std::string fmt(const char *f, ...)
{
    char b[512];
    va_list ap;
    va_start(ap, f);
    vsnprintf(b, sizeof b, f, ap);
    va_end(ap);
    return b;
}

static std::string probes(const char *text)
{
    std::string s(text), out;
    rt::each_line(s.data(), [&](const char *line, size_t len) { out += fmt("  %.*s\n", int(len), line); });
    return out;
}

static std::string log_section(const char *log, int max_lines)
{
    std::string s(log);
    std::vector<char> tail(s.begin(), s.end());
    tail.push_back(0);
    size_t got = s.size();
    char *start = rt::log_tail(tail.data(), got, max_lines);
    return fmt("%s%s", start, got && tail[got - 1] != '\n' ? "\n" : "");
}

static void addons()
{
    CHECK(rt::addon_text("", fmt) == "  no settings.json\n");
    CHECK(rt::addon_text("{\"addons\": [", fmt) == "  settings.json is not valid JSON\n");
    CHECK(rt::addon_text("{}", fmt) == "  profile '?', 0 addon(s)\n");
    std::string text = R"({"profile": "Default", "addons": [
        {"id": "example.seasons", "version": "0.6.9", "dlls": ["cs.dll", "cs_ui.dll"]},
        {"id": "example.helmets", "version": "1.2.0", "enabled": false, "dlls": []},
        {"id": "no.version"}
    ]})";
    CHECK(rt::addon_text(text, fmt) == "  profile 'Default', 3 addon(s)\n"
                                       "  example.seasons                      0.6.9      on  cs.dll, cs_ui.dll\n"
                                       "  example.helmets                      1.2.0      off (no dll)\n"
                                       "  no.version                           ?          on  (no dll)\n");
    std::string mods = R"({"profile": "Old", "mods": [{"id": "a", "version": "1", "enabled": true, "dlls": ["a.dll"]}]})";
    CHECK(rt::addon_text(mods, fmt) == "  profile 'Old', 1 addon(s)\n"
                                       "  a                                    1          on  a.dll\n");
}

static void lines()
{
    CHECK(probes("") == "");
    CHECK(probes("hooks 12\nframe 3.1 ms\n") == "  hooks 12\n  frame 3.1 ms\n");
    CHECK(probes("one\n\ntwo") == "  one\n  \n  two\n");

    std::string builds = "\n  ray tracing builds before the crash, 2 command list(s), the list with the newest build first; 5 "
                         "build(s) in all\n"
                         "  each list shows its newest builds, oldest first; #n is the build's place on its list in that frame\n"
                         "    list 0x1f00: the newest 2 of 4 build(s)\n"
                         "      build 3 frame 9 #1+ bottom-level into 0x5000 (flags 0x4), 1 geometries: triangles\n"
                         "   \n"
                         "      build 4 frame 9 #2 top-level update into 0x6000 (flags 0x6), 8 instances:";
    std::string report, log;
    rt::indented_lines(
        builds, [&](const char *line, size_t len) { report += fmt("%.*s", int(len), line); },
        [&](const char *line, size_t len) { log += fmt("[gfx] rtas recent: %.*s|", int(len), line); });
    CHECK(report == builds);
    CHECK(log == "[gfx] rtas recent: ray tracing builds before the crash, 2 command list(s), the list with the newest build "
                 "first; 5 build(s) in all|"
                 "[gfx] rtas recent: each list shows its newest builds, oldest first; #n is the build's place on its list in "
                 "that frame|"
                 "[gfx] rtas recent: list 0x1f00: the newest 2 of 4 build(s)|"
                 "[gfx] rtas recent: build 3 frame 9 #1+ bottom-level into 0x5000 (flags 0x4), 1 geometries: triangles|"
                 "[gfx] rtas recent: build 4 frame 9 #2 top-level update into 0x6000 (flags 0x6), 8 instances:|");
    report.clear();
    log.clear();
    rt::indented_lines(
        std::string(), [&](const char *line, size_t len) { report.append(line, len); },
        [&](const char *line, size_t len) { log.append(line, len); });
    CHECK(report.empty() && log.empty());
}

static void loader_log()
{
    CHECK(log_section("", 3) == "");
    CHECK(log_section("a\nb\nc\nd\ne\n", 3) == "c\nd\ne\n");
    CHECK(log_section("a\nb\nc\nd\ne", 3) == "c\nd\ne\n");
    CHECK(log_section("a\nb\n", 5) == "a\nb\n");
    CHECK(log_section("a\r\nb\r\nc\r\n", 2) == "b\nc\n");
    CHECK(log_section("a\nb\r\n", 5) == "a\nb\n\n");
    CHECK(log_section("\n\nx\n", 1) == "x\n");
}

static void pointers()
{
    uintptr_t found[4] = {};
    int n = 0;
    rt::scan_message_pointers("Ne heap 'Main': block 0x00007ff6a0b01230 corrupt near 7FF6A0B01300, size 12345678 id 1234567",
                              found, n, 4);
    CHECK(n == 3);
    CHECK(found[0] == 0x00007ff6a0b01230ull && found[1] == 0x7ff6a0b01300ull && found[2] == 0x12345678ull);
    n = 3;
    rt::scan_message_pointers("0x1111111111111111 0x2222222222222222", found, n, 4);
    CHECK(n == 4 && found[3] == 0x1111111111111111ull);
    n = 0;
    rt::scan_message_pointers("0x11111111111111111", found, n, 4);
    CHECK(n == 1 && found[0] == 0x1111111111111111ull);
}

static void calls()
{
    const uint8_t rel32[8] = {0x90, 0x90, 0x90, 0xE8, 0x10, 0x20, 0x30, 0x40};
    const uint8_t reg[8] = {0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0xFF, 0xD0};
    const uint8_t mem[8] = {0x90, 0x90, 0xFF, 0x15, 0x01, 0x02, 0x03, 0x04};
    const uint8_t rip7[8] = {0x90, 0xFF, 0x94, 0x24, 0x01, 0x02, 0x03, 0x04};
    const uint8_t jmp[8] = {0x90, 0x90, 0x90, 0x90, 0x90, 0x90, 0xFF, 0xE0};
    const uint8_t none[8] = {0x48, 0x89, 0x5C, 0x24, 0x08, 0x57, 0x48, 0x83};
    CHECK(rt::call_before(rel32));
    CHECK(rt::call_before(reg));
    CHECK(rt::call_before(mem));
    CHECK(rt::call_before(rip7));
    CHECK(!rt::call_before(jmp));
    CHECK(!rt::call_before(none));
}

int main()
{
    addons();
    lines();
    loader_log();
    pointers();
    calls();
    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("report_text: all checks passed\n");
    return 0;
}
