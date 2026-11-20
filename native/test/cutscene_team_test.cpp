// Host-native unit test for the cutscene team record guard (native/src/core/guards/garage/cutscene_team_logic.hpp): when
// the game's own no-team path is taken, and where it is found.
// Built and run by native/test/run_tests.sh cutscene_team.

#include "core/guards/garage/cutscene_team_logic.hpp"

#include <cstdio>
#include <vector>

namespace ct = runtime::cutscene_team;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static std::vector<uint8_t> window(const std::vector<uint8_t> &head, const std::vector<uint8_t> &jcc, size_t tail)
{
    std::vector<uint8_t> w(head);
    w.insert(w.end(), jcc.begin(), jcc.end());
    w.insert(w.end(), tail, 0x90);
    return w;
}

int main()
{
    const std::vector<uint8_t> cmp = {0x81, 0xf9, 0x00, 0x00, 0xff, 0x3f};

    expect(ct::record_missing(0), "a null record is missing");
    expect(ct::record_missing(0x18), "a record in the first page is missing");
    expect(ct::record_missing(0xffffffffffffffffull), "a kernel pointer is missing");
    expect(!ct::record_missing(0x582a55c20ull), "a heap record is there");

    expect(ct::team_of(0x1f70) == 503, "the scaled id the game keeps in rcx is the team id");
    expect(ct::team_of(0x1f6u << 4 | 0xf) == 502, "the low nibble is not part of the id");

    std::vector<uint8_t> short_je = window({0x89, 0xc1, 0x81, 0xe1, 0x00, 0x00, 0xff, 0xff}, {}, 0);
    short_je.insert(short_je.end(), cmp.begin(), cmp.end());
    short_je.push_back(0x74);
    short_je.push_back(0x3e);
    size_t n = short_je.size();
    const uint64_t at = 0x1402898a3 - 2, site = at + n + 0x30;
    uint64_t target = ct::no_team_target(short_je.data(), n, at, site);
    expect(target == at + n + 0x3e, "a short je after the sentinel compare gives the game's no-team path");

    std::vector<uint8_t> near_je(cmp);
    near_je.insert(near_je.end(), {0x0f, 0x84, 0x00, 0x01, 0x00, 0x00});
    n = near_je.size();
    target = ct::no_team_target(near_je.data(), n, at, at + n + 0x20);
    expect(target == at + n + 0x100, "a near je gives its target");

    std::vector<uint8_t> back_je(cmp);
    back_je.insert(back_je.end(), {0x74, 0xe0});
    expect(ct::no_team_target(back_je.data(), back_je.size(), at, at + 0x10) == 0, "a target before the site is not used");

    std::vector<uint8_t> no_jcc(cmp);
    no_jcc.insert(no_jcc.end(), {0x90, 0x90});
    expect(ct::no_team_target(no_jcc.data(), no_jcc.size(), at, at + 0x40) == 0, "a compare not followed by a je is not used");
    expect(ct::no_team_target(cmp.data(), cmp.size(), at, at + 0x40) == 0, "a compare cut off before its jump is not used");

    std::vector<uint8_t> two(cmp);
    two.insert(two.end(), {0x74, 0x10});
    two.insert(two.end(), cmp.begin(), cmp.end());
    two.insert(two.end(), {0x74, 0x20});
    expect(ct::no_team_target(two.data(), two.size(), at, at + 0x10) == at + two.size() + 0x20,
           "the compare nearest the site wins");
    expect(ct::no_team_target(nullptr, 0, at, at + 1) == 0 && ct::no_team_target(cmp.data(), 3, at, at + 1) == 0,
           "a window too short for the compare gives nothing");

    expect(ct::resume_for(0, 0x1402898ef) == 0x1402898ef, "a missing record resumes at the no-team path");
    expect(ct::resume_for(0x582a55c20ull, 0x1402898ef) == 0, "a loaded record runs the game's own lookup");
    expect(ct::resume_for(0, 0) == 0, "no path found, nothing to resume at");

    ct::Seen seen;
    expect(seen.first(1, 503) && !seen.first(1, 503), "a site and team log once");
    expect(seen.first(2, 503) && seen.first(1, 504), "another site or team logs again");
    bool all = true;
    for (uint32_t i = 0; i < 40; i++)
        all = seen.first(3, 1000 + i) && all;
    expect(!all && !seen.first(9, 9), "the log is capped");

    return failed ? 1 : 0;
}
