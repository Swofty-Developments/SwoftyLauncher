#include "core/guards/frontend/fe_tile_guard_logic.hpp"
#include "core/guards/guard.hpp"
#include "core/runtime/runtime.hpp"

#include <windows.h>

#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

// Front-end tile list guard (re/db/functions.tsv FeTile_Get_ListFirstEntrantId, FeTile_Check_EntrantListUsable).
//
// FeTile_Build_List populates every tile of a menu through the tile item's populate slot. The item behind the career
// entrant-list tiles (the showroom's series tiles, for one) is built for a list label such as f2_2026 and looks the
// list up twice, with no check that it exists:
//
// FeTile_Check_EntrantListUsable (0x142428790) finds the label's row in the game database with Gdb_Find_RowByLabel and
// reads its stat_id through Gdb_Get_FieldInt. A label with no row makes Gdb_Get_FieldInt return its miss pointer, whose
// first qword is not a canonical address, and the load of [rax+8] faults (ACCESS_VIOLATION at
// F1_25.exe+0x242883a, FeTile_Build_List+0x70df). An import that removes the f2_2026 entrant lists leaves the tile's
// label without a row. At site_FeTile_ListRowFieldLoad a non-canonical [rax] continues at site_FeTile_ListRowNone, the
// function's own return of 0 (the list is not usable), which the populate already handles.
//
// FeTile_Get_ListFirstEntrantId (0x141738eb0) scans the slot array of the career's entrant lists for the record with
// the label's name and reads its first entrant id. The scan has no end check: a label with no record walks past the
// array and faults in Ne_Crt_Strcmp (F1_25.exe+0x535d858, FeTile_Build_List+0x70df). At site_FeTile_ListScanStart,
// where the scan begins, the guard walks the same slots up to the array end. When no record carries the label it
// continues at site_FeTile_ListScanNone, the function's own return for an item without a list (the found byte stays 0).
// A label that is in the array runs the game's code unchanged.
//
// The tile is left without a list and the rest of the menu builds. Each label is logged once.
//
// SWOFTY_FE_TILE_GUARD=0 turns both off.

namespace {

namespace mem = swofty::mem;
namespace ft = runtime::fe_tile;
namespace guards = swofty_core::guards;

constexpr const char *MIXIN = "swofty_core.dll (front-end tile list guard)";
constexpr const char *SCAN_START = "site_FeTile_ListScanStart";
constexpr const char *SCAN_NONE = "site_FeTile_ListScanNone";
constexpr const char *ROW_ENTRY = "site_FeTile_ListRowEntry";
constexpr const char *ROW_FIELD_LOAD = "site_FeTile_ListRowFieldLoad";
constexpr const char *ROW_NONE = "site_FeTile_ListRowNone";
constexpr uint64_t CONTAINER_SLOTS_END = 0x568;

uintptr_t scan_none = 0, row_none = 0;
guards::FirstKeys<std::string, ft::LOG_LABELS> logged{"lists"};
thread_local const char *row_label = nullptr;

struct Game {
    bool u64(uint64_t a, uint64_t &out) { return mem::read(static_cast<uintptr_t>(a), out); }

    bool text(uint64_t a, std::string &out) { return mem::c_string(static_cast<uintptr_t>(a), out, 128); }
};

void say(const std::string &label)
{
    if (auto line = logged.note(label))
        swofty::log("[fe] tiles: entrant list %s is not in the game data; its tile is left without a list%s", label.c_str(),
                    line.tail);
}

void scan_start(SwoftyContext *c)
{
    std::string label;
    uint64_t end = 0;
    if (!mem::c_string(static_cast<uintptr_t>(c->rdi), label, 128) ||
        !mem::read(static_cast<uintptr_t>(c->rax + CONTAINER_SLOTS_END), end))
        return;
    Game game;
    if (ft::list_present(game, c->rbx, end, label))
        return;
    c->rip = scan_none;
    say(label);
}

void row_entry(SwoftyContext *c) { row_label = reinterpret_cast<const char *>(c->r8); }

void row_field_load(SwoftyContext *c)
{
    uint64_t value = 0;
    if (mem::read(static_cast<uintptr_t>(c->rax), value) && ft::canonical(value))
        return;
    c->rip = row_none;
    std::string label;
    if (!row_label || !mem::c_string(row_label, label, 128))
        label = "(unreadable)";
    say(label);
}

guards::SiteGuard guard{guards::CoreSetId::fe_tile_guard,
                        {
                            .set = MIXIN,
                            .name = "[fe] tiles guard",
                            .env = "SWOFTY_FE_TILE_GUARD",
                            .keeps = {{SCAN_NONE, &scan_none}, {ROW_NONE, &row_none}},
                            .needs = {SCAN_START, SCAN_NONE, ROW_ENTRY, ROW_FIELD_LOAD, ROW_NONE},
                            .mixins = {{SCAN_START, SWOFTY_MIXIN_SITE, 100, (void *)&scan_start, nullptr},
                                       {ROW_ENTRY, SWOFTY_MIXIN_SITE, 100, (void *)&row_entry, nullptr},
                                       {ROW_FIELD_LOAD, SWOFTY_MIXIN_SITE, 100, (void *)&row_field_load, nullptr}},
                            .armed = "a tile whose entrant list is gone is left without a list instead of faulting",
                        }};

}
