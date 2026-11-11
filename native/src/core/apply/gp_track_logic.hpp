#pragma once

#include <map>
#include <string>

// A new Grand Prix keeps the last Grand Prix's track in its weekend (the track byte of Career_Create_SeasonWeekend,
// Weekend+0x278): the track select only rewrites it on a selection change, and a player who confirms the highlighted
// track or never opens the screen races the remembered one. When that track's addon is now off or gone, the tracks
// table has no row with that stat id, TrackDataSystem's item for it is null, and
// Game_ChampionshipGameMode_BuildSessionSettings reads +0x78 of it. The era guard (era_logic.hpp track_missing) only
// stops a load whose track the facts can name (a dormant row); a stat id with no row at all names nothing, so the
// decision below swaps it for the stock track before the game reads the weekend. `rows` is the tracks table as the
// game has it, stat id -> row label.

namespace swofty_core::apply::gp_track {

constexpr const char *STOCK = "melbourne";

struct Fallback {
    bool replace = false;
    int stat_id = -1;
    std::string line;
};

inline Fallback fallback(int weekend_stat_id, const std::map<int, std::string> &rows, const char *stock = STOCK)
{
    Fallback f;
    if (weekend_stat_id < 0 || weekend_stat_id >= 0xff || rows.empty() || rows.count(weekend_stat_id))
        return f;
    for (auto &[id, label] : rows)
        if (label == stock) {
            f.stat_id = id;
            break;
        }
    std::string id = std::to_string(weekend_stat_id);
    if (f.stat_id < 0) {
        f.line = "the Grand Prix weekend's track stat id " + id + " has no tracks row (its track addon is off or not installed) and the tracks table has no " +
                 stock + " row to race instead; the session load goes on";
        return f;
    }
    f.replace = true;
    f.line = "the Grand Prix weekend's track stat id " + id +
             " has no tracks row (its track addon is off or not installed; the last Grand Prix raced it); the Grand Prix starts at " + stock +
             " (stat id " + std::to_string(f.stat_id) + ") instead";
    return f;
}

}
