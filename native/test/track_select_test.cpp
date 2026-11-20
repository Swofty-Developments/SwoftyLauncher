// Host-native unit test for the track select guard's confirm decision (native/src/core/guards/tracks/track_select_logic.hpp):
// which screen commands are a confirm, when a confirm has a looked-up track, and which base track that lookup names.
// Built and run by native/test/run_tests.sh track_select.

#include "core/guards/tracks/track_select_logic.hpp"

#include <cstdio>

static int failures;

#define CHECK(cond, msg)                                                                                                         \
    do {                                                                                                                         \
        if (cond) {                                                                                                              \
            printf("ok  : %s\n", msg);                                                                                           \
        } else {                                                                                                                 \
            printf("FAIL: %s\n", msg);                                                                                           \
            failures++;                                                                                                          \
        }                                                                                                                        \
    } while (0)

namespace ts = runtime::track_select;
namespace trk = swofty_core::apply::tracks;

static trk::BaseTrack track(const char *id, const char *code, int stat_id)
{
    trk::BaseTrack t;
    t.id = id;
    t.code = code;
    t.stat_id = stat_id;
    t.name = id;
    return t;
}

int main()
{
    CHECK(ts::command(ts::ADVANCE_PRESSED, ts::ADVANCE_PRESSED) == ts::Command::confirm,
          "advance_pressed is the Grand Prix screen's confirm");
    CHECK(ts::command(ts::SELECT_PRESSED, ts::SELECT_PRESSED) == ts::Command::confirm,
          "select_pressed is the Time Trial screen's confirm");
    CHECK(ts::command(ts::SELECT_PRESSED, ts::ADVANCE_PRESSED) == ts::Command::other,
          "the Time Trial confirm on the Grand Prix screen is passed on");
    CHECK(ts::command(ts::ADVANCE_PRESSED, ts::SELECT_PRESSED) == ts::Command::other,
          "the Grand Prix confirm on the Time Trial screen is passed on");
    CHECK(ts::command(ts::SELECTED_CHANGED, ts::ADVANCE_PRESSED) == ts::Command::selection_changed &&
              ts::command(ts::SELECTED_CHANGED, ts::SELECT_PRESSED) == ts::Command::selection_changed,
          "selected_track_changed is a selection change on both screens");
    CHECK(ts::command(0, ts::ADVANCE_PRESSED) == ts::Command::other &&
              ts::command(0x12345678u, ts::SELECT_PRESSED) == ts::Command::other,
          "any other command is passed on");

    constexpr uintptr_t RECORD = 0x7ff600001000;
    CHECK(!ts::looked_up(0, 0, 0), "a confirm before any lookup has no track");
    CHECK(!ts::looked_up(0, 5, 1), "a confirm with no record has no track, whatever the counter says");
    CHECK(!ts::looked_up(RECORD, 0, 0), "a record with no lookup since the screen opened is not the confirmed track");
    CHECK(ts::looked_up(RECORD, 3, 2), "a lookup after the last selection change is the confirmed track");
    CHECK(!ts::looked_up(RECORD, 2, 3), "a lookup before the last selection change is stale");
    CHECK(ts::looked_up(RECORD, 1, 0), "the first lookup on a screen with no selection change counts");

    trk::Base base;
    base.tracks = {track("monza", "monza", 11), track("imola", "imola", 27), track("community", "community_ring", 40)};
    CHECK(!ts::track_of(base, "monza", 11), "no track before the boot pass fills the base");
    base.ready = true;
    const trk::BaseTrack *t = ts::track_of(base, "imola", 27);
    CHECK(t && t->id == "imola", "the record's code names its track");
    t = ts::track_of(base, "monza", 40);
    CHECK(t && t->id == "monza", "the code wins over a stat id another track has");
    t = ts::track_of(base, "", 40);
    CHECK(t && t->id == "community", "a record whose name was not read is found by its stat id");
    t = ts::track_of(base, "unknown_code", 27);
    CHECK(t && t->id == "imola", "a code the base doesn't know falls back to the stat id");
    CHECK(!ts::track_of(base, "unknown_code", 99), "a record the base has neither by code nor by stat id names no track");
    CHECK(!ts::track_of(base, "", 0xff), "a record with no name and an unknown stat id names no track");

    if (failures) {
        printf("%d failure(s)\n", failures);
        return 1;
    }
    printf("all track select checks passed\n");
    return 0;
}
