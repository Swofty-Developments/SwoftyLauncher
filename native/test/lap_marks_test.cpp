#include "core/services/lap_marks_logic.hpp"

#include <swofty_api.h>

#include <cstddef>
#include <cstdio>
#include <string>

namespace m = runtime::lap_marks;

static int failed = 0;
static void expect(bool ok, const std::string &what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    if (!ok)
        failed++;
}

static void table_layout()
{
    expect(SWOFTY_LAP_HOST_VERSION == 4, "the lap host table is version 4");
    expect(offsetof(SwoftyLapHost, attach) == 8 && offsetof(SwoftyLapHost, sealing) == 8 + 34 * sizeof(void *) &&
               offsetof(SwoftyLapHost, name_tick) == 8 + 38 * sizeof(void *),
           "the version 1 fields keep their offsets");
    expect(SWOFTY_LAP_HOST_V1_SIZE == 8 + 39 * sizeof(void *), "a version 1 table ends where mark begins");
    expect(offsetof(SwoftyLapHost, mark) == SWOFTY_LAP_HOST_V1_SIZE &&
               SWOFTY_LAP_HOST_V2_SIZE == SWOFTY_LAP_HOST_V1_SIZE + sizeof(void *),
           "mark is the one field version 2 adds");
    expect(offsetof(SwoftyLapHost, core_version) == SWOFTY_LAP_HOST_V2_SIZE &&
               SWOFTY_LAP_HOST_V3_SIZE == SWOFTY_LAP_HOST_V2_SIZE + sizeof(void *),
           "core_version is the one field version 3 adds");
    expect(offsetof(SwoftyLapHost, input) == SWOFTY_LAP_HOST_V3_SIZE &&
               sizeof(SwoftyLapHost) == SWOFTY_LAP_HOST_V3_SIZE + sizeof(void *),
           "input is the one field version 4 adds, at the end");
}

static void marks()
{
    expect(std::string(m::name(0)) == "start" && std::string(m::name(1)) == "s1" && std::string(m::name(2)) == "s2" &&
               std::string(m::name(3)) == "end" && std::string(m::name(-1)).empty(),
           "the four timing lines have their wire names");
    expect(std::string(m::name(4)) == "p1" && std::string(m::name(5)) == "r1" && std::string(m::name(10)) == "p4" &&
               std::string(m::name(11)) == "r4" && std::string(m::name(12)).empty(),
           "pause n is 2n+2 and its resume 2n+3, up to four pauses a lap");
    expect(m::valid(0, 0, 0) && m::valid(7, 3, 91234) && m::valid(m::MAX_LAP, 3, m::MAX_GAME_MS) && m::valid(1, 4, 49830) &&
               m::valid(1, 11, 77937),
           "a lap's marks are taken");
    expect(!m::valid(-1, 1, 10) && !m::valid(m::MAX_LAP + 1, 1, 10) && !m::valid(1, 12, 10) && !m::valid(1, -1, 10) &&
               !m::valid(1, 1, -5) && !m::valid(1, 3, m::MAX_GAME_MS + 1),
           "a lap past the server's counter range, an unknown line, a negative clock or a lap over an hour is refused");

    m::Mark k{12, 3, 1, 28411, 1795000123456};
    expect(m::json(k) == R"({"seq":12,"lap":3,"mark":"s1","game_ms":28411,"at_ms":1795000123456})", "a mark's file body");
    expect(m::file_name(k) == "1795000123456-12.json", "a mark's file is named by its moment and sequence");
}

static void queue()
{
    m::Queue q;
    expect(q.push(1, 0, 0, 100) && q.push(1, 1, 30000, 130000), "marks queue in order");
    expect(!q.push(1, 12, 0, 140000) && q.marks.size() == 2, "a refused mark is not queued");
    expect(q.marks[0].seq == 1 && q.marks[1].seq == 2 && q.marks[1].which == 1 && q.marks[1].at_ms == 130000,
           "each mark gets the next sequence number");
    for (int i = 0; i < 40; i++)
        q.push(2, i % 4, 1000 * i, 200000 + i);
    expect(q.marks.size() == m::MAX_PENDING && q.dropped == 42 - m::MAX_PENDING, "a full queue drops its oldest marks");
    expect(q.marks.back().seq == 42 && q.marks.front().seq == 42 - m::MAX_PENDING + 1, "the newest marks are kept");
}

int main()
{
    table_layout();
    marks();
    queue();
    std::printf("%s\n", failed ? "FAILED" : "all ok");
    return failed ? 1 : 0;
}
