#pragma once
#include <cstdint>
#include <cstring>
#include <string>

#include <swofty/systems/ui.hpp>

namespace frontend::mfd_pages {

inline constexpr int CAPACITY = 8;
inline constexpr int STOCK_COUNT = 6;
inline constexpr const char *STOCK[STOCK_COUNT] = {"car_settings", "pit_settings", "car_damage",
                                                   "car_engine",   "car_temp",     "mods"};
inline constexpr const char *ENABLED_PARENT = "view_model_data.hud.content.mfd_complete.mfd";

inline std::string enabled_path(const char *panel) { return std::string(ENABLED_PARENT) + "." + panel + "_enabled"; }

struct Outcome {
    uint32_t mask = 0;
    bool reordered = false;
    bool resized = false;
    int unknown = 0;
};

inline int stock_index(const char *name)
{
    if (!name)
        return -1;
    for (int i = 0; i < STOCK_COUNT; i++)
        if (!std::strcmp(name, STOCK[i]))
            return i;
    return -1;
}

inline void fill(swofty::ui::events::MfdPage (&pages)[CAPACITY])
{
    for (int i = 0; i < CAPACITY; i++)
        pages[i] = {i < STOCK_COUNT ? STOCK[i] : nullptr, 0, {}};
}

inline Outcome read(const swofty::ui::events::MfdPage *pages, int count_after)
{
    Outcome out;
    out.resized = count_after != STOCK_COUNT;
    int n = count_after < STOCK_COUNT ? count_after : STOCK_COUNT;
    for (int i = 0; i < n; i++) {
        int at = stock_index(pages[i].name);
        if (at < 0) {
            out.unknown++;
            continue;
        }
        if (at != i)
            out.reordered = true;
        if (pages[i].hide)
            out.mask |= 1u << at;
    }
    return out;
}

}
