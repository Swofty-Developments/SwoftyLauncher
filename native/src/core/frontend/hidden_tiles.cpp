#include "core/frontend/hidden_tiles.hpp"

#include "core/runtime/build.hpp"

#include <swofty/mixin.hpp>

#include <windows.h>

#include <cstdint>
#include <cstring>

// F1 THE MOVIE home tile. The home case of FeTile_Build_List adds it (FeHome_Add_MovieTile at index
// r15b+2, then r12 = r15) only while feature flag bit 42 ([rsi+5] & 4) is set; with the flag clear the
// je skips to the F1 World block, which numbers itself r12+1 from the tile before. The anchor is the
// movie block's first instruction, the je's fall-through right after the 6 bytes the
// site_FeHome_AfterCareerCall hook relocates; a short jmp there to the F1 World block builds the
// flag-clear list, so the button numbering stays contiguous. No branch lands in the 2 replaced bytes.
//
// Braking Point home tile. The same home case adds it (FeHome_Add_StoryTile, id "story", at index r15b+1 after
// inc r15b) only while feature flag bit 0 ([rsi] & 1) is set; the je over it lands on the Career tile, which numbers
// itself r15+1. The anchor is that je; turned into a jmp it always skips the block without the inc, so Career takes
// Braking Point's index and the numbering stays contiguous.
namespace frontend {

namespace {

const runtime::Sig S_MOVIE_TILE{"FeHome_MovieTileBlock",
                                "41 80 C7 02 4C 89 F1 44 89 FA E8 ?? ?? ?? ?? 45 89 FC 45 8D 7C 24 01 4C 89 F1 44 89 FA E8",
                                0x1742fdc,
                                runtime::Sig::addr,
                                0,
                                0,
                                0};

const runtime::Sig S_STORY_TILE{"FeHome_StoryTileBlock",
                                "A8 01 74 0E 41 FE C7 4C 89 F1 44 89 FA E8 ?? ?? ?? ?? 45 8D 67 01 4C 89 F1 44 89 E2 E8",
                                0x1742fb7,
                                runtime::Sig::addr,
                                0,
                                2,
                                0};

bool overwrite(const runtime::Sig &sig, const uint8_t *bytes, size_t size)
{
    auto at = reinterpret_cast<void *>(runtime::find(sig, "home", "core"));
    DWORD old;
    if (!at || !VirtualProtect(at, size, PAGE_EXECUTE_READWRITE, &old))
        return false;
    memcpy(at, bytes, size);
    VirtualProtect(at, size, old, &old);
    FlushInstructionCache(GetCurrentProcess(), at, size);
    return true;
}

// Local Multiplayer list. FeTile_Build_List (base+0x173df80) builds menu_kind 2 as Splitscreen
// (base+0x1747f90, button_1) and then an inline LAN tile (id "lan", item vtable base+0x5704740,
// button_2) that the shared tail pushes into the item vector. The anchor is the instruction after the
// Splitscreen call; a jmp there to the tail the adder-only menus jump to (base+0x1745016, after the
// temporaries' cleanup, at the item init/populate loop) leaves Splitscreen as the only entry. No
// branch lands in the 9 replaced bytes.
const runtime::Sig S_LAN_TILE{"FeTile_LocalMp_LanTile",
                              "4C 8D 05 ?? ?? ?? ?? 4C 89 F1 B2 01 E8 ?? ?? ?? ?? 48 C7 44 24 38 00 00 00 00",
                              0x173e7a7,
                              runtime::Sig::addr,
                              0,
                              0x11,
                              0};
const runtime::Sig S_TILE_TAIL{"FeTile_Build_List_ItemsTail",
                               "4C 89 F1 E8 ?? ?? ?? ?? E9 ?? ?? ?? ?? E8 ?? ?? ?? ?? B2 01",
                               0x1745016,
                               runtime::Sig::rel32,
                               8,
                               9,
                               5};

void hide_lan_tile()
{
    uintptr_t lan = runtime::find(S_LAN_TILE, "core", "core"), tail = runtime::find(S_TILE_TAIL, "core", "core");
    bool hidden = false;
    if (lan && tail && tail > lan && tail - lan < 0x10000) {
        uint8_t skip_lan[9] = {0xE9, 0, 0, 0, 0, 0x90, 0x90, 0x90, 0x90};
        int32_t rel = int32_t(tail - (lan + 5));
        memcpy(skip_lan + 1, &rel, sizeof rel);
        auto at = reinterpret_cast<void *>(lan);
        DWORD old;
        hidden = VirtualProtect(at, sizeof skip_lan, PAGE_EXECUTE_READWRITE, &old);
        if (hidden) {
            memcpy(at, skip_lan, sizeof skip_lan);
            VirtualProtect(at, sizeof skip_lan, old, &old);
            FlushInstructionCache(GetCurrentProcess(), at, sizeof skip_lan);
        }
    }
    swofty::log("[core] LAN Game entry in Local Multiplayer %s",
                hidden ? "hidden (Splitscreen only)" : "left in place (signature did not resolve)");
}

}

void hide_movie_tile()
{
    static constexpr uint8_t skip_movie[] = {0xEB, 0x10};
    bool hidden = overwrite(S_MOVIE_TILE, skip_movie, sizeof skip_movie);
    swofty::log("[core] F1 THE MOVIE home tile %s", hidden ? "hidden" : "left in place (signature did not resolve)");
}

void hide_story_tile()
{
    static constexpr uint8_t skip_story[] = {0xEB};
    bool hidden = overwrite(S_STORY_TILE, skip_story, sizeof skip_story);
    swofty::log("[core] Braking Point home tile %s", hidden ? "hidden" : "left in place (signature did not resolve)");
}

}

namespace runtime {

void local_tiles_setup() { frontend::hide_lan_tile(); }

}
