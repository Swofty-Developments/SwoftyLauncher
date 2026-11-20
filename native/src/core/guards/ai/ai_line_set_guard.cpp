#include "core/guards/guard.hpp"
#include "core/guards/memory.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/ai/ai_line_set_logic.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// AI line set guard (re/db/functions.tsv Sess_Build_AiSystem, F1_25.exe+0x5ff750).
//
// Sess_Build_AiSystem builds one 0x508-byte AI record per vehicle slot, in the vector at [builder + 0xa8]. Six pointers
// in the record come from six hash maps keyed by the resource id of the car's own file in the track's AI folder
// (environment_package/tracks/<track>/ai/<package>/<folder>/<asset>.<ext>): +0x58 acpd, +0x60 brpd, +0x68 copd, +0x70
// crpd, +0x78 thmd (no shipped folder has one), +0x80 thpd. A file no map lists stores 0, with no check.
// Ai_Job_UpdateCarsParallel (0x14063e370) hands those pointers to Ai_LineCache_UpdateIfChanged (0x140686a40) and the
// table cursor updates for every car on every tick, and the first of them reads [pointer + 0x178]: a car whose folder
// the track does not have faults reading 0x178 about 20 ms after the session starts. A third car in a team (My Team,
// garages with three cars) and an eleventh team on a track that carries the ten stock folders only are such cars: the
// same cars that the session enter guard finds in no AI asset group.
//
// The record also holds, at +0x08, the resource id of the car's <asset>.brakelinesettings. Every brake line of the
// track keeps its settings per such id (0x14060f370 fills them from the loaded files) and Ai_Update_VehicleDriver
// (0x140614d70) asks the line for the record's id (0x14066b8a0); with no settings under that id it takes no speed for
// the corner from the brake line, so the car reaches the corner flat out.
//
// site_Ai_BuildSystem_RecordPushed is the join after the record was appended (inc bpl, builder in rbx, both the in
// place and the regrown path land here). The last record is [rbx + 0xb0] - 0x508. For each of its six pointers that is
// 0, the guard copies the pointer of the earliest other record that has one, and a record with no +0x58 also takes
// the brake settings id of the earliest other record that has a +0x58, so the car races on that car's tables and
// brake line settings for this track instead of faulting. Records pushed before any record has pointers (the first
// cars of a session) are backfilled the same way on the push that supplies the first donor; with no donor in the
// vector nothing is written. +0x78 is the one table the update checks for 0 itself, so it is borrowed only when +0x58
// was empty too (a file no map lists); a listed car's empty +0x78 stays empty. The pointers are shared, read only
// objects the maps own, and every car keeps its own cursors in its record. A record with all six pointers runs the
// game's code unchanged.
//
// SWOFTY_AI_LINE_SET_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (ai line set guard)";
constexpr const char *PUSHED = "site_Ai_BuildSystem_RecordPushed";

namespace al = runtime::ai_line_set;
namespace guards = swofty_core::guards;

guards::LogFirst<al::LOG_FIRST> logged{"fixes"};

void record_pushed(SwoftyContext *c)
{
    al::Repair r = al::repair(guards::read_if_readable, guards::write_if_readable, nullptr, c->rbx);
    if (!r.ran || (r.filled == 0 && r.missing == 0 && r.backfilled == 0))
        return;
    auto line = logged.note();
    if (!line)
        return;
    swofty::log("[ai] vehicle slot %u has no AI line or tables of its own (its name is in none of the AI maps); %u of its "
                "pointers borrow another car's, %u have no donor, %u earlier cars backfilled; brake line settings %s%s",
                r.index, r.filled, r.missing, r.backfilled, r.brake_settings ? "borrowed" : "kept", line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::ai_line_set,
                        {
                            .set = MIXIN,
                            .name = "[ai] line set guard",
                            .env = "SWOFTY_AI_LINE_SET_GUARD",
                            .needs = {PUSHED},
                            .mixins = {{PUSHED, SWOFTY_MIXIN_SITE, 100, (void *)&record_pushed, nullptr}},
                            .armed = "a car no AI map lists borrows another car's tables and brake line settings",
                            .settles_when_off = true,
                        }};

}
