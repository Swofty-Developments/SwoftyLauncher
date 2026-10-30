// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>

#include <swofty/game.hpp>
#if __has_include(<swofty/career_content.hpp>)
#include <swofty/career_content.hpp>
#endif

#include "career_group.hpp"

namespace f1990 {

inline void *registry_get(void *registry, uintptr_t desc)
{
    auto **vt = *static_cast<void ***>(registry);
    if (!vt || !vt[2])
        return nullptr;
    return reinterpret_cast<void *(*)(void *, const void *, int)>(vt[2])(registry, reinterpret_cast<const void *>(desc), 1);
}

inline void *setup_component(void *championship)
{
    return registry_get(static_cast<uint8_t *>(championship) + 0x208,
                        swofty::game::k_Career_CareerSetupComponentDesc.address());
}

inline uint8_t group_index(void *component)
{
    auto **vt = *static_cast<void ***>(component);
    return reinterpret_cast<uint8_t (*)(void *)>(vt[0x268 / 8])(component);
}

#if __has_include(<swofty/career_content.hpp>)
inline bool is_1990_career(const void *career)
{
    if (swofty::career::content::available())
        return career && swofty::career::is(kCareerGroup1990, career);
    return career_data_group_is(career, kCareerGroup1990, {setup_component, group_index});
}
#else
inline bool is_1990_career(const void *career)
{
    return career_data_group_is(career, kCareerGroup1990, {setup_component, group_index});
}
#endif

}
