#include "runtime.hpp"

#include <windows.h>

#include "theatre_list_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// Theatre list guard (re/db/functions.tsv Game_TheatreList_FindItemSaveSlot, F1_25.exe+0x2bfc6a7; EA +0x2bf0c37).
//
// Opening Theatre builds its list of saved replays and highlights: the row builder FUN_141f0bbd0 asks the list object
// at [vm + 0x970] for the save slot of each saved-item record (slot +8 of the function table it is handed, which
// FUN_14217f380 fills with FUN_142bfc520; FUN_141f11ec0 calls it directly), passing the record index.
// The record (0x1a0 bytes at [list + 0x2a8]) names its owner by id at +0x8, and the function walks the flat hash set at
// [[list + 0x40] + 0x58] (control words at +0x60, one control byte per 8 bits of the tzcnt) for the entry whose
// [[[entry + 0x38] + 0x260] + 0x2a8] equals that id, then takes that owner's save slot manager and returns the slot
// record for the item, or 0 when it has none. The walk is the same one the garage setup guard covers
// (garage_setup.cpp): it has no end test and assumes the id is in the set. When no entry carries the id, because the
// item was recorded with a car or team the current list does not have, the walk reaches the set's end marker, takes
// the slot past the end array as an entry and faults reading [entry + 0x38] with a non-canonical entry
// (r9 = 0x2a00002f33000020 in every report, "reading 0xffffffffffffffff", 0.4 s after screen theatre_container).
//
// site_Game_TheatreList_EntryStep is the lea that follows the entry load in the walk's loop. When the entry in r9 is
// not a readable chain down to the id, the guard sets rax to 0 and continues at site_Game_TheatreList_NoSlot, the
// function's epilogue, which is how the function itself answers when the item has no slot record; the row builder
// then treats the item as unsaved, as it does for any item without a slot. An entry that reads through runs the
// game's code unchanged. The clone for the second record list (Game_TheatreList_FindItemSaveSlot_Alt, [list + 0x2c8])
// has the same walk but no reports and is not covered.
//
// SWOFTY_THEATRE_LIST_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (theatre list guard)";
constexpr const char *STEP = "site_Game_TheatreList_EntryStep";
constexpr const char *NO_SLOT = "site_Game_TheatreList_NoSlot";

namespace tl = runtime::theatre_list;

uintptr_t no_slot = 0;
bool offered = false, settled = false;
std::atomic<int> skipped{0};

bool read_memory(uint64_t address, void *out, size_t n, void *)
{
    if (!swofty::mem::readable(address, n))
        return false;
    memcpy(out, reinterpret_cast<const void *>(address), n);
    return true;
}

void entry_step(SwoftyContext *c)
{
    tl::Exit exit;
    bool usable;
    {
        swofty::mem::Trust trust(runtime::HEAP_TRUST_MS);
        usable = tl::entry_step(c->r9, no_slot, read_memory, nullptr, &exit);
    }
    if (usable)
        return;
    c->rax = exit.rax;
    c->rip = exit.rip;
    int n = ++skipped;
    if (n > tl::LOG_FIRST)
        return;
    swofty::log("[theatre] a saved item names an owner that is not in the current list, so it has no save slot%s",
                n == tl::LOG_FIRST ? " (further skips are not logged)" : "");
}

}

namespace runtime {

std::vector<CoreMixins> theatre_list_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_THEATRE_LIST_GUARD", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[theatre] list guard off (SWOFTY_THEATRE_LIST_GUARD=%s)", v);
        return out;
    }
    no_slot = swofty::api->resolve(NO_SLOT);
    if (!no_slot || !swofty::api->resolve(STEP)) {
        no_slot = 0;
        swofty::log("[theatre] list guard off: needs %s and %s, which this build does not resolve", STEP, NO_SLOT);
        return out;
    }
    out.push_back({MIXIN, {{STEP, SWOFTY_MIXIN_SITE, 100, (void *)&entry_step, nullptr}}});
    return out;
}

void theatre_list_commit()
{
    if (!offered || settled || !no_slot)
        return;
    settled = true;
    swofty::log("[theatre] list guard %s",
                core_mixins_active(MIXIN) ? "armed: a saved item whose owner is gone has no save slot"
                                          : "did not install (see the [core] lines)");
}

}
