// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstring>
#include <string>

#include <swofty/db.hpp>
#include <swofty/mixin.hpp>

namespace f1990 {

inline bool db_text_is(const std::string &key, const char *want)
{
    swofty::db::Value v{};
    if (swofty::db::get(key, &v) != swofty::db::Result::ok || !v.s)
        return false;
    return !std::strcmp(v.s, want);
}

// 1 = the database carries the 1990 career content (a ModeChampionship row that saves to
// driver_career_1990 or runs the F1_career_1990 event list), 0 = it does not, -1 = not readable yet.
inline int detect_1990_content(const char *who)
{
    if (!swofty::db::available())
        return -1;
    int64_t count = 0;
    if (swofty::db::int_at("game_modes._ModeChampionship_instancecount_", &count) != swofty::db::Result::ok ||
        count <= 0 || count > 256)
        return -1;
    for (int64_t i = 0; i < count; i++) {
        std::string row = "game_modes.ModeChampionship_" + std::to_string(i);
        if (db_text_is(row + ".save_data.name", "driver_career_1990") ||
            db_text_is(row + ".save_data_0.name", "driver_career_1990") || db_text_is(row + ".event_list", "F1_career_1990")) {
            swofty::log("[f1-1990] %s: 1990 career content found (%s)", who, row.c_str());
            return 1;
        }
    }
    swofty::log("[f1-1990] %s: no 1990 career content in the database; left stock", who);
    return 0;
}

// The statId of the game_modes.ModeChampionship row that saves to save_name, or -1.
inline int64_t mode_stat_id(const char *save_name)
{
    int64_t count = 0;
    if (!swofty::db::available() ||
        swofty::db::int_at("game_modes._ModeChampionship_instancecount_", &count) != swofty::db::Result::ok || count <= 0 ||
        count > 256)
        return -1;
    for (int64_t i = 0; i < count; i++) {
        std::string row = "game_modes.ModeChampionship_" + std::to_string(i);
        int64_t stat = -1;
        if ((db_text_is(row + ".save_data.name", save_name) || db_text_is(row + ".save_data_0.name", save_name)) &&
            swofty::db::int_at(row + ".statId", &stat) == swofty::db::Result::ok)
            return stat;
    }
    return -1;
}

inline int has_1990_content(const char *who)
{
    static int known = -1;
    if (known < 0)
        known = detect_1990_content(who);
    return known;
}

}
