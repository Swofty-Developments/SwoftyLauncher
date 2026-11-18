#include "core/guards/guard.hpp"
#include "core/guards/memory.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include "core/guards/garage/garage_setup_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// Garage setup list guard (re/db/functions.tsv Game_GarageSetups_FindOwnerName, F1_25.exe+0xc22c47).
//
// Opening the garage (and loading a setup in My Team practice) rebuilds the setup list: FUN_140c20280 walks the saved
// setups (0x218 bytes each at [list + 0x308]) and FUN_140c21600 fills one row. For a setup whose owner id (+0xa0) is not
// -1 the row calls FUN_140c22ad0(list, id), which walks the flat hash set at [list + 0x340] (entries at +0x58, control
// words at +0x60, one control byte per 8 bits of the tzcnt) for the entry whose [[[entry + 0x38] + 0x260] + 0x2a8]
// equals the id, then formats the name at +0x220 into a registry node path. The walk has no end test: it assumes the id
// is in the set. When no entry carries the id, because the setup was saved for a car or team the current grid no
// longer has, the walk reaches the set's end marker, takes the entry slot past the end array as an entry and faults
// reading [entry + 0x38] with a non-canonical entry (r9 = 0x2a00002f33000020 in every report, "reading
// 0xffffffffffffffff").
//
// site_Game_GarageSetups_EntryStep is the lea that follows the entry load in the walk's loop. When the entry in r9 is
// not a readable chain down to the id, the guard continues at site_Game_GarageSetups_NotFound, the function's own
// not-found exit, which returns a null name; the row is built with an empty owner name, as it is when the registry
// node is missing. An entry that reads through runs the game's code unchanged.
//
// SWOFTY_GARAGE_SETUP_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (garage setup guard)";
constexpr const char *STEP = "site_Game_GarageSetups_EntryStep";
constexpr const char *NOT_FOUND = "site_Game_GarageSetups_NotFound";

namespace gs = runtime::garage_setup;
namespace guards = swofty_core::guards;

uintptr_t not_found = 0;
guards::LogFirst<gs::LOG_FIRST> skipped{"skips"};

void entry_step(SwoftyContext *c)
{
    bool usable;
    {
        swofty::mem::Trust trust(runtime::HEAP_TRUST_MS);
        usable = gs::entry_usable(c->r9, guards::read_if_readable, nullptr);
    }
    if (usable)
        return;
    c->rip = not_found;
    auto line = skipped.note();
    if (!line)
        return;
    swofty::log("[garage] a setup names an owner that is not in the garage list, so its row has no owner name%s", line.tail);
}

guards::SiteGuard guard{guards::CoreSetId::garage_setup,
                        {
                            .set = MIXIN,
                            .name = "[garage] setup guard",
                            .env = "SWOFTY_GARAGE_SETUP_GUARD",
                            .keeps = {{NOT_FOUND, &not_found}},
                            .needs = {STEP, NOT_FOUND},
                            .mixins = {{STEP, SWOFTY_MIXIN_SITE, 100, (void *)&entry_step, nullptr}},
                            .armed = "a setup whose owner is gone gets an empty owner name",
                        }};

}
