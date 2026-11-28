// Host-native unit test for the core's My Team part rates guard (native/src/core/guards/career/myteam_partrates_logic.hpp).
// Built and run by native/test/run_tests.sh myteam_partrates.

#include "core/guards/career/myteam_partrates_logic.hpp"

#include <cstdio>
#include <map>
#include <string>

namespace mp = runtime::myteam_partrates;
namespace mh = runtime::myteam_heal;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

struct Fake {
    std::map<uint64_t, uint64_t> q;
    std::map<uint64_t, uint32_t> d;

    bool u64(uint64_t a, uint64_t &out)
    {
        auto it = q.find(a);
        if (it == q.end())
            return false;
        out = it->second;
        return true;
    }

    bool u32(uint64_t a, uint32_t &out)
    {
        auto it = d.find(a);
        if (it == d.end())
            return false;
        out = it->second;
        return true;
    }

    bool text(uint64_t, std::string &) { return false; }
};

constexpr uint64_t ENGINE = 0x340000000, RECORDS = 0x341000000, CAREER = 0x342000000, ENTITY = 0x343000000, LIST = 0x344000000;

static Fake game(size_t records, size_t entrants, size_t humans)
{
    Fake f;
    f.q[ENGINE + mp::RECORDS_BEGIN] = RECORDS;
    f.q[ENGINE + mp::RECORDS_END] = RECORDS + records * mp::RECORD_SIZE;
    for (size_t i = 0; i < records; i++)
        f.d[RECORDS + i * mp::RECORD_SIZE] = static_cast<uint32_t>(0x00010000 * (i + 1) + 1);
    f.q[ENGINE + mp::CAREER] = CAREER;
    f.q[CAREER + mp::CAREER_ENTITY] = ENTITY;
    f.q[ENTITY + mp::ENTRANTS_BEGIN] = LIST;
    f.q[ENTITY + mp::ENTRANTS_END] = LIST + entrants * mh::ENTRANT_SIZE;
    for (size_t i = 0; i < entrants; i++) {
        f.d[LIST + i * mh::ENTRANT_SIZE + mh::ENTRANT_VEHICLE] = mh::NO_VEHICLE;
        f.d[LIST + i * mh::ENTRANT_SIZE + mh::ENTRANT_FLAGS] = i < humans ? mh::HUMAN_FLAG : 0;
    }
    return f;
}

int main()
{
    expect(mp::skips(0), "a null part-rate record is skipped");
    expect(!mp::skips(0x7ff000001000), "a part-rate record that exists runs the game's code");

    Fake f = game(3, 5, 0);
    mp::Records r = mp::records(f, ENGINE);
    expect(r.read && r.count == 3, "the part-rate records are counted from the engine management's range");
    expect(r.keys == "00010001, 00020001, 00030001", "the record keys are listed in order");
    expect(!mp::records(f, 0).read, "no engine management reads no records");

    Fake ragged = game(3, 5, 0);
    ragged.q[ENGINE + mp::RECORDS_END] += 8;
    expect(!mp::records(ragged, ENGINE).read, "a range that is not whole records is not read");

    Fake many = game(40, 2, 0);
    mp::Records m = mp::records(many, ENGINE);
    expect(m.count == 40 && m.keys.size() > 0 && m.keys.substr(m.keys.size() - 3) == "...",
           "a long record list is cut with an ellipsis");

    uint64_t first = 0, end = 0;
    expect(mp::entrant_range(f, ENGINE, first, end) && first == LIST && end == LIST + 5 * mh::ENTRANT_SIZE,
           "the entrant range follows the engine management's career entity");
    expect(!mp::entrant_range(f, 0, first, end), "no engine management has no entrant range");

    mp::Registers regs;
    regs.engine = ENGINE;
    regs.key = mh::NO_VEHICLE;
    std::string line = mp::describe(f, regs);
    expect(line.find("no entrant matches the local player (key 3fff0000)") != std::string::npos,
           "the empty key reads as no entrant matching the local player");
    expect(line.find("holds 3 part-rate records with keys 00010001, 00020001, 00030001") != std::string::npos,
           "the line names the part-rate records");
    expect(line.find("5 entrants: 0 on kind 1 cars, 0 on stock cars, 5 on no car, 0 unread, no human") != std::string::npos,
           "the line carries the entrant census with no human");

    Fake withhuman = game(2, 4, 1);
    regs.key = 0x00050001;
    std::string human = mp::describe(withhuman, regs);
    expect(human.find("local player key 00050001 has no record") != std::string::npos, "a key with no record reads as one");
    expect(human.find("1 human (car 3fff0000)") != std::string::npos, "the line counts the human entrant");

    Fake empty;
    regs.engine = 0;
    std::string none = mp::describe(empty, regs);
    expect(none.find("holds 0 part-rate records") != std::string::npos && none.find("is unreadable") != std::string::npos,
           "an unreadable engine management logs with no records and no entrant list");

    if (failed) {
        std::printf("%d failed\n", failed);
        return 1;
    }
    std::printf("my team part rates: all checks passed\n");
    return 0;
}
