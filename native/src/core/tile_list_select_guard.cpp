#include "runtime.hpp"

#include <windows.h>

#include "tile_list_select_logic.hpp"

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdint>
#include <cstring>
#include <vector>

// Tile list select guard (re/db/functions.tsv Game_TileList_ApplyBoundSelection, F1_25.exe+0x3838fe8; EA +0x3826228).
//
// A list widget whose selection is bound to a view-model property (Choose Driver's tile_list, selection=@selected_driver)
// listens on that binding with Game_TileList_ApplyBoundSelection (0x143838d90). When the property changes, it walks the
// widget's children [widget + 0x130 .. + 0x138] for the one whose data-context component is bound to the new node:
// [child + 0x30] is the child's component array, scanned with no end test and no null test for the component of the
// data-context type. A child with no component array faults reading 0 at F1_25.exe+0x3838fe8 (fn + 0x258).
// Choose Driver hits it on the F2 drivers tab: the tile list respawns its tiles, raises children_triggered (0x20f2f60a),
// and the view model's slot-10 handler (0x141a5df70 + 0xa4, EA 0x141a58460) sets selected_driver (vm + 0x298) to
// drivers[vm + 0x430] through Ne_Property_SetS64IfChanged, which runs the walk while one of the tiles has no components.
// Eight reports, Steam and EA, with More Driver Models, Driver Manager, Progressive's cards or the Classic Seasons wizard.
//
// site_Game_TileListSelect_ChildStep is the head of the walk (mov rdi,[r15]; mov r14,[rdi+0x30]), r15 the cursor into
// the children and r12 their end. When the child under the cursor is not readable, has no component array or has an
// unreadable first component, the guard continues at site_Game_TileListSelect_ChildNext, the walk's own step to the
// next child (add r15,8 ... jne head; past the last child the game's no-match exit). The tile stays on screen without
// the selection: a child that cannot carry the data context is never the bound child. Children that read through run
// the game's code unchanged.
//
// SWOFTY_TILE_LIST_SELECT_GUARD=0 turns it off.

namespace {

constexpr const char *MIXIN = "swofty_core.dll (tile list select guard)";
constexpr const char *STEP = "site_Game_TileListSelect_ChildStep";
constexpr const char *NEXT = "site_Game_TileListSelect_ChildNext";

namespace ts = runtime::tile_list_select;

uintptr_t next_child = 0;
bool offered = false, settled = false;
std::atomic<int> skipped{0};

bool read_memory(uint64_t address, void *out, size_t n, void *)
{
    if (!swofty::mem::readable(address, n))
        return false;
    memcpy(out, reinterpret_cast<const void *>(address), n);
    return true;
}

void child_step(SwoftyContext *c)
{
    ts::Child child;
    ts::Position at;
    {
        swofty::mem::Trust trust(runtime::HEAP_TRUST_MS);
        child = ts::check_child(c->r15, read_memory, nullptr);
        if (child == ts::Child::Usable)
            return;
        uint64_t widget = 0, begin = 0;
        if (read_memory(c->rsp + ts::WIDGET_AT_RSP, &widget, sizeof widget, nullptr) && ts::user_pointer(widget) &&
            read_memory(widget + ts::CHILDREN_BEGIN_AT, &begin, sizeof begin, nullptr))
            at = ts::position(c->r15, c->r12, begin);
    }
    c->rip = next_child;
    int n = ++skipped;
    if (n > ts::LOG_FIRST)
        return;
    if (at.known)
        swofty::log("[tile-list] tile %llu of %llu %s, so the list's selection skips it%s", (unsigned long long)at.index,
                    (unsigned long long)at.count, ts::why(child),
                    n == ts::LOG_FIRST ? " (further skips are not logged)" : "");
    else
        swofty::log("[tile-list] a tile %s, so the list's selection skips it%s", ts::why(child),
                    n == ts::LOG_FIRST ? " (further skips are not logged)" : "");
}

bool guard_off()
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_TILE_LIST_SELECT_GUARD", v, sizeof v);
    if (n && n < sizeof v && (!strcmp(v, "0") || !strcmp(v, "off"))) {
        swofty::log("[tile-list] select guard off (SWOFTY_TILE_LIST_SELECT_GUARD=%s)", v);
        return true;
    }
    return false;
}

}

namespace runtime {

std::vector<CoreMixins> tile_list_select_mixins()
{
    std::vector<CoreMixins> out;
    if (offered || !code_ready())
        return out;
    offered = true;
    if (guard_off())
        return out;
    next_child = swofty::api->resolve(NEXT);
    if (!next_child || !swofty::api->resolve(STEP)) {
        next_child = 0;
        swofty::log("[tile-list] select guard off: needs %s and %s, which this build does not resolve", STEP, NEXT);
        return out;
    }
    out.push_back({MIXIN, {{STEP, SWOFTY_MIXIN_SITE, 100, (void *)&child_step, nullptr}}});
    return out;
}

void tile_list_select_commit()
{
    if (!offered || settled || !next_child)
        return;
    settled = true;
    swofty::log("[tile-list] select guard %s; a tile with no components is skipped when a list moves its selection",
                core_mixins_active(MIXIN) ? "armed" : "did not install (see the [core] lines)");
}

}
