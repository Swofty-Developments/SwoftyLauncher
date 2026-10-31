// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <string>

#include <swofty/career.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include "box_decision.hpp"
#include "career_1990.hpp"

namespace f1990 {

inline bool read_chain(uintptr_t base, size_t off, uintptr_t *out)
{
    return base && swofty::mem::read(base + off, *out) && *out;
}

inline bool box_list_selected()
{
    constexpr size_t kContext = 0xc0, kModeSetup = 0x88, kSetupModes = 0x260, kSelectedList = 0xe18, kListName = 0x28,
                     kGameSetupMode = 0x2b0, kModeName = 0xd8;
    if (!swofty::api || !swofty::api->resolve)
        return false;
    uintptr_t slot = swofty::api->resolve("g_Game_Manager"), mgr = 0, ctx = 0, owner = 0, modes = 0, list = 0, name = 0,
              mode = 0;
    if (!slot || !swofty::mem::read(slot, mgr) || !mgr || !read_chain(mgr, kContext, &ctx) ||
        !read_chain(ctx, kModeSetup, &owner) || !read_chain(owner, kSetupModes, &modes) ||
        !read_chain(modes, kSelectedList, &list) || !read_chain(list, kListName, &owner) ||
        !read_chain(owner, 0x10, &name))
        return false;
    std::string text, mode_text;
    if (!read_chain(modes, kGameSetupMode, &mode) || !read_chain(mode, kModeName, &owner) || !read_chain(owner, 0x10, &mode) ||
        !swofty::mem::c_string(mode, mode_text, 64))
        return false;
    return swofty::mem::c_string(name, text, 64) && box_list_name(text) && host_mode_name(mode_text);
}

inline uintptr_t box_host_mode()
{
    constexpr size_t kContext = 0xc0, kModeSetup = 0x88, kSetupModes = 0x260, kGameSetupMode = 0x2b0;
    if (!swofty::api || !swofty::api->resolve)
        return 0;
    uintptr_t slot = swofty::api->resolve("g_Game_Manager"), mgr = 0, ctx = 0, owner = 0, modes = 0, mode = 0;
    if (!slot || !swofty::mem::read(slot, mgr) || !mgr || !read_chain(mgr, kContext, &ctx) ||
        !read_chain(ctx, kModeSetup, &owner) || !read_chain(owner, kSetupModes, &modes) ||
        !read_chain(modes, kGameSetupMode, &mode))
        return 0;
    return mode;
}

inline Play play_now()
{
    void *career = swofty::career::entity();
    return decide_play(career && is_1990_career(career), box_list_selected(), true);
}

}
