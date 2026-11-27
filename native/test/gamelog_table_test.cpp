// Host-native unit test for the game log channel table's sections (native/src/gamelog/channel_table.hpp): one section
// per build, picked by the exe's PE timestamp, and no section for a build the table does not cover.
// Built and run by native/test/run_tests.sh gamelog_table.

#include "gamelog/channel_table.hpp"

#include <cstdio>
#include <fstream>
#include <iterator>
#include <string>

namespace gt = gamelog::table;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main(int argc, char **argv)
{
    const std::string text = "# hash\tsites\tchannels\n"
                             "# F1_25.exe aaaaaaaaaaaaaaaa 1789118616\n"
                             "0x005972be\t2\tVM\n"
                             "0x00597339\t39\tUI\n"
                             "# F1_25.exe bbbbbbbbbbbbbbbb 1794312636\n"
                             "0x005972be\t3\tVM\n";
    auto all = gt::sections(text);
    expect(all.size() == 2, "two sections");
    expect(gt::find(all, 1789118616) && gt::find(all, 1789118616)->rows == "0x005972be\t2\tVM\n0x00597339\t39\tUI\n",
           "the first section holds only its own rows");
    expect(gt::find(all, 1794312636) && gt::find(all, 1794312636)->rows == "0x005972be\t3\tVM\n",
           "the last section runs to the end");
    expect(!gt::find(all, 1794315435), "a build the table does not cover has no section");
    expect(gt::covered(all) == "1789118616, 1794312636", "the covered builds are listed");

    auto one = gt::sections("# F1_25.exe fake 5\n# hash\tsites\tchannels\n0x7c7c88cb\t1\tTEST_WARN\n");
    expect(one.size() == 1 && gt::find(one, 5) && gt::find(one, 5)->rows.find("TEST_WARN") != std::string::npos,
           "a single-build table (the Wine test's) parses");
    expect(gt::sections("").empty() && gt::covered(gt::sections("")) == "none", "an empty table covers nothing");
    expect(gt::sections("# F1_25.exe broken\n0x1\t1\tX\n").empty(), "a malformed header opens no section");
    if (argc > 1) {
        std::ifstream f(argv[1], std::ios::binary);
        std::string shipped((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());
        auto b = gt::sections(shipped);
        expect(gt::find(b, 1789118616) && gt::find(b, 1789118616)->rows.size() > 1000,
               "the shipped table covers Steam 1.0.141 (KB)");
        expect(gt::find(b, 1794312636) && gt::find(b, 1794312636)->rows.size() > 1000, "the shipped table covers Steam 1.0.145");
        expect(gt::find(b, 1789128892) && gt::find(b, 1789128892)->rows.size() > 1000, "the shipped table covers EA 1.0.141");
        expect(gt::find(b, 1794315435) && gt::find(b, 1794315435)->rows.size() > 1000, "the shipped table covers EA 1.0.145");
    }
    return failed ? 1 : 0;
}
