// Host-native unit test for the database first-read probe (native/src/core/assets/db_first_read_logic.hpp): the switch,
// the boot step table and the step a stack finds, which lookups the probe lists, the call-site dedup, and the log lines
// the load-core lane reads.
// Built and run by native/test/run_tests.sh db_first_read.

#include "core/assets/db_first_read_logic.hpp"

#include <cstdio>
#include <string>
#include <vector>

namespace fr = runtime::db_first_read;
using fr::Phase;
using fr::Take;

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

static void switch_off_by_default()
{
    CHECK(fr::env_on("1"));
    CHECK(!fr::env_on(nullptr));
    CHECK(!fr::env_on(""));
    CHECK(!fr::env_on("0"));
    CHECK(!fr::env_on("10"));
    CHECK(!fr::env_on("true"));
    CHECK(std::string(fr::ENV) == "SWOFTY_DB_FIRST_READ");
}

static void step_table()
{
    for (size_t i = 1; i < fr::STEP_RETURNS.size(); i++)
        CHECK(fr::STEP_RETURNS[i] > fr::STEP_RETURNS[i - 1]);
    CHECK(fr::STEP_RETURNS[9] == 0x1fe);
    CHECK(fr::STEP_CALLS[9] == 0x1443c45e0);
    CHECK(fr::STEP_CALLS[14] == 0x1443c4fa0);
    CHECK(fr::STEP_CALLS[16] == 0x1443c8d70);
    CHECK(fr::STEP_CALLS[18] == 0x1443cb030);
    CHECK(fr::STEP_CALLS[21] == 0x1443cc250);
}

static void steps_on_a_stack()
{
    const uint64_t boot = 0x7ff6'0000'0000ull + 0x43c0560;
    CHECK(fr::step_at(boot, boot + 0xcb) == 1);
    CHECK(fr::step_at(boot, boot + 0x1fe) == 10);
    CHECK(fr::step_at(boot, boot + 0x3f5) == 28);
    CHECK(fr::step_at(boot, boot + 0x1ff) == 0);
    CHECK(fr::step_at(boot, boot + 0xca) == 0);
    CHECK(fr::step_at(boot, boot + 0x3f6) == 0);
    CHECK(fr::step_at(boot, 0) == 0);

    std::vector<uint64_t> stack{0x1234, boot, boot + 0x21b, 0x7ff6'0000'1000ull, boot + 0x2b2, boot + 0x1fe};
    CHECK(fr::step_in(boot, stack.data(), stack.size()) == 17);
    CHECK(fr::step_in(boot, stack.data(), 4) == 0);
    CHECK(fr::step_in(0, stack.data(), stack.size()) == 0);
    CHECK(fr::step_in(boot, stack.data(), 0) == 0);
}

static void what_is_listed()
{
    CHECK(fr::take(Phase::idle, true) == Take::skip);
    CHECK(fr::take(Phase::idle, false) == Take::skip);
    CHECK(fr::take(Phase::edits, true) == Take::own);
    CHECK(fr::take(Phase::edits, false) == Take::list);
    CHECK(fr::take(Phase::after, true) == Take::list);
    CHECK(fr::take(Phase::after, false) == Take::list);
    CHECK(fr::take(Phase::done, true) == Take::skip);
    CHECK(fr::take(Phase::done, false) == Take::skip);
    CHECK(fr::AFTER == 24 && fr::DURING == 8);
}

static void sites_dedup()
{
    fr::Sites<3> s;
    CHECK(s.add(0x10));
    CHECK(!s.add(0x10));
    CHECK(s.add(0x20));
    CHECK(!s.full());
    CHECK(s.add(0x30));
    CHECK(s.full());
    CHECK(!s.add(0x40));
    CHECK(!s.add(0x20));
    CHECK(s.n == 3);
}

static void times_and_paths()
{
    CHECK(fr::ms(100, 25'100, 10'000'000) == 2.5);
    CHECK(fr::ms(100, 200, 0) == 0);
    CHECK(fr::path_text(nullptr) == "(null)");
    CHECK(fr::path_text("Drivers.Driver_01.Name") == "Drivers.Driver_01.Name");
    CHECK(fr::path_text("a\"b\001c") == "a?b?c");
    std::string longer(fr::PATH_CHARS + 5, 'k');
    CHECK(fr::path_text(longer.c_str()) == std::string(fr::PATH_CHARS, 'k') + "...");
    std::string exact(fr::PATH_CHARS, 'k');
    CHECK(fr::path_text(exact.c_str()) == exact);
}

static void lines()
{
    fr::Read r;
    r.index = 1;
    r.lookup = 3;
    r.path = "Tracks.Melbourne";
    r.where = "F1_25.exe+0x65bda3 .xtls fn base+0x65bd00+0xa3";
    r.found = true;
    r.thread = 4120;
    r.boot_thread = true;
    r.since_load_ms = 5012.31;
    r.since_edits_ms = 12.5;
    r.step = 11;
    r.scanned_kib = 40;
    CHECK(fr::line(r) ==
          "[assets] gamedb: first read #1: Gdb_Lookup_Key(\"Tracks.Melbourne\") found, lookup 3 after the load, from "
          "F1_25.exe+0x65bda3 .xtls fn base+0x65bd00+0xa3, on the boot thread 4120, 5012.3 ms after the load returned and "
          "12.5 ms after the core's edits, in boot step 11 (KB 0x1443c4670)");

    r.step = 0;
    CHECK(fr::line(r).ends_with(", outside the boot steps (no step return in the 40 KiB of stack above the call)"));

    r.index = 2;
    r.lookup = 1;
    r.found = false;
    r.boot_thread = false;
    r.thread = 7788;
    r.since_load_ms = 800;
    r.since_edits_ms = -1;
    r.where = "swofty_core.dll+0x1a2b3c";
    CHECK(fr::line(r) ==
          "[assets] gamedb: first read #2: Gdb_Lookup_Key(\"Tracks.Melbourne\") not found, lookup 1 after the load, from "
          "swofty_core.dll+0x1a2b3c, on thread 7788, 800.0 ms after the load returned, during the core's edits, no boot "
          "step (another thread)");

    CHECK(fr::edits_line(4512.0, 4120, 960, 0, 0) ==
          "[assets] gamedb: first read probe: the core's edits ran 4512.0 ms on the boot thread 4120 and made 960 "
          "lookup(s) of their own; no other thread read the database during them");
    CHECK(fr::edits_line(4512.0, 4120, 960, 14, 2) ==
          "[assets] gamedb: first read probe: the core's edits ran 4512.0 ms on the boot thread 4120 and made 960 "
          "lookup(s) of their own; 14 lookup(s) from other threads came during them, from 2 call site(s) listed above: a "
          "reader concurrent with the edits");
    CHECK(fr::done_line(24, 1800) == "[assets] gamedb: first read probe done: 24 call site(s) listed from 1800 lookup(s) "
                                     "after the core's edits; later lookups are not listed");
    CHECK(fr::armed_line("Steam 1.0.145 (26566308)", 0x45a4b60, 0x43c2b30) ==
          "[assets] gamedb: first read probe on (SWOFTY_DB_FIRST_READ=1) on Steam 1.0.145 (26566308): Gdb_Lookup_Key "
          "hooked at base+0x45a4b60, Boot_Init_NeApplication at base+0x43c2b30; it lists the first 24 call sites to read "
          "after the core's edits and the first 8 from other threads during them");
    CHECK(fr::off_line("it is not proven on this build") ==
          "[assets] gamedb: first read probe off: it is not proven on this build");
}

int main()
{
    switch_off_by_default();
    step_table();
    steps_on_a_stack();
    what_is_listed();
    sites_dedup();
    times_and_paths();
    lines();
    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("database first read probe: all checks passed\n");
    return 0;
}
