#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>
#include <unordered_set>
#include <vector>

namespace runtime::track_list_guard {

constexpr uintptr_t ENTRY = 0x28;
constexpr uintptr_t CALENDAR_BEGIN = 0x2b0;
constexpr uintptr_t CALENDAR_END = 0x2b8;
constexpr uintptr_t SEASON_EVENTS = 0x2c8;
constexpr size_t FALLBACK_LIMIT = 64;
constexpr uint32_t SEASON_MAX = 255;

struct Load {
    uintptr_t component = 0;
    uint64_t entry = UINT64_MAX;
    int prev_id = -1;
    size_t stored = 0, season = 0, limit = 0;
    bool blown = false;
    size_t kept = 0, dropped = 0, reused = 0, extra = 0;
    std::unordered_set<uintptr_t> rows;
};

inline size_t entries(uintptr_t begin, uintptr_t end)
{
    return end > begin ? (end - begin) / ENTRY : 0;
}

inline size_t season_events(uint32_t raw)
{
    return raw >= 1 && raw <= SEASON_MAX ? raw : 0;
}

inline size_t limit_for(size_t season)
{
    return season ? season : FALLBACK_LIMIT;
}

inline void reset(Load &load, uintptr_t component, size_t stored, size_t season)
{
    load = Load{};
    load.component = component;
    load.stored = stored;
    load.season = season;
    load.limit = limit_for(season);
    load.blown = stored > load.limit;
}

inline bool enter(Load &load, uintptr_t component, size_t calendar, size_t stored, size_t season, uint64_t entry, uint8_t id)
{
    if (calendar == 0 || component != load.component)
        reset(load, component, stored, season);
    if (entry == load.entry)
        return false;
    load.entry = entry;
    bool run = load.prev_id == id;
    load.prev_id = id;
    if (load.blown && (run || load.kept >= load.limit)) {
        ++load.dropped;
        return false;
    }
    return true;
}

inline uintptr_t choose(Load &load, const std::vector<uintptr_t> &matches)
{
    if (matches.empty())
        return 0;
    uintptr_t pick = 0;
    for (uintptr_t row : matches)
        if (!load.rows.count(row)) {
            pick = row;
            break;
        }
    if (!pick) {
        pick = matches.front();
        ++load.reused;
    }
    load.rows.insert(pick);
    load.extra += matches.size() - 1;
    ++load.kept;
    return pick;
}

inline bool last_entry(uint64_t index, uint64_t stored)
{
    return index + 1 >= stored;
}

inline Load finish(Load &load, uintptr_t component)
{
    Load done = load.component == component ? load : Load{};
    load = Load{};
    return done;
}

inline std::string load_line(uintptr_t component, size_t stored, size_t loaded, const Load *guard)
{
    char buf[400];
    snprintf(buf, sizeof buf, "[saves] custom_track_list loaded (schedule component 0x%llx): %zu stored entr%s -> %zu calendar event%s",
             (unsigned long long)component, stored, stored == 1 ? "y" : "ies", loaded, loaded == 1 ? "" : "s");
    std::string s = buf;
    if (!guard)
        return s;
    if (guard->season)
        snprintf(buf, sizeof buf, " (season event list %zu)", guard->season);
    else
        snprintf(buf, sizeof buf, " (season event list unknown, limit %zu)", guard->limit);
    s += buf;
    if (guard->extra) {
        snprintf(buf, sizeof buf, "; %zu more matching event%s the game would also have loaded skipped", guard->extra,
                 guard->extra == 1 ? "" : "s");
        s += buf;
    }
    if (guard->reused) {
        snprintf(buf, sizeof buf, "; %zu entr%s repeat an event already in the calendar", guard->reused, guard->reused == 1 ? "y" : "ies");
        s += buf;
    }
    if (guard->blown) {
        snprintf(buf, sizeof buf, "; blown up (more than %zu entries): a run of one id reads as one entry, %zu stored entr%s dropped",
                 guard->limit, guard->dropped, guard->dropped == 1 ? "y" : "ies");
        s += buf;
    }
    if (!guard->extra && !guard->reused && !guard->blown)
        s += ", no repeats";
    return s;
}

inline std::string save_line(uintptr_t component, size_t count)
{
    char buf[200];
    snprintf(buf, sizeof buf, "[saves] custom_track_list saved (schedule component 0x%llx): %zu calendar event%s",
             (unsigned long long)component, count, count == 1 ? "" : "s");
    return buf;
}

struct SaveLog {
    uintptr_t component[8] = {};
    size_t count[8] = {};
    size_t next = 0;

    bool changed(uintptr_t comp, size_t n)
    {
        for (size_t i = 0; i < 8; i++)
            if (component[i] == comp) {
                if (count[i] == n)
                    return false;
                count[i] = n;
                return true;
            }
        component[next] = comp;
        count[next] = n;
        next = (next + 1) % 8;
        return true;
    }
};

inline bool switch_off(const char *value)
{
    if (!value)
        return false;
    return (value[0] == '0' && value[1] == 0) || (value[0] == 'o' && value[1] == 'f' && value[2] == 'f' && value[3] == 0);
}

}
