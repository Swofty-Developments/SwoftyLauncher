#pragma once

#include "core/guards/tracks/gp_weekend_track_logic.hpp"

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

// The track data lookup guard's choice (re/db/structs/TrackDataSet.h, TrackDataTrack.h, TrackDataRecord.h). At
// site_Trk_TrackData_SetMatched, Game_TrackData_GetTrack has matched a TrackDataSet (r11) and reads the stat id of the
// TrackDataRecord it was given (rdx +0x01). A caller that found no record for its track passes null. The guard gives
// it the record of the set's melbourne track, else of the set's first track, the weekend track guard's rule; a set
// with no readable track gets a placeholder record whose stat id is past every set, which GetTrack's own code answers
// with the set's first track, or null for an empty set.

namespace runtime::track_data_lookup {

constexpr uint64_t SET_ITEMS_BEGIN = 0x08, SET_ITEMS_END = 0x10, ITEM_STRIDE = 0x10, ITEM_TRACK = 0x08;
constexpr uint64_t TRACK_RECORD = 0x00, RECORD_STAT_ID = 0x01, RECORD_NAME = 0xd0;
constexpr std::size_t NAME_MAX = 64;
constexpr uint8_t PLACEHOLDER_STAT_ID = 0xff;

using Read = bool (*)(uint64_t address, void *out, std::size_t n, void *user);

struct Pick {
    uint64_t record = 0;
    int stat_id = -1;
    std::string name;
    std::size_t items = 0;
};

template <class T> bool get(Read read, void *user, uint64_t at, T &out) { return read(at, &out, sizeof out, user); }

inline std::string c_string(Read read, void *user, uint64_t at)
{
    std::string out;
    char c = 0;
    while (at && out.size() < NAME_MAX && get(read, user, at + out.size(), c) && c)
        out.push_back(c);
    return out;
}

inline Pick pick(uint64_t set, Read read, void *user)
{
    Pick p;
    uint64_t begin = 0, end = 0;
    if (!set || !get(read, user, set + SET_ITEMS_BEGIN, begin) || !get(read, user, set + SET_ITEMS_END, end) || end < begin ||
        (end - begin) % ITEM_STRIDE || (end - begin) / ITEM_STRIDE > gp_weekend_track::MAX_ITEMS)
        return p;
    p.items = std::size_t((end - begin) / ITEM_STRIDE);
    std::vector<gp_weekend_track::Item> items(p.items);
    std::vector<uint64_t> records(p.items, 0);
    for (std::size_t i = 0; i < p.items; ++i) {
        uint64_t track = 0, record = 0, name = 0;
        uint8_t stat = 0;
        if (!get(read, user, begin + i * ITEM_STRIDE + ITEM_TRACK, track) || !track ||
            !get(read, user, track + TRACK_RECORD, record) || !record || !get(read, user, record + RECORD_STAT_ID, stat))
            continue;
        records[i] = record;
        items[i].present = true;
        if (get(read, user, record + RECORD_NAME, name))
            items[i].name = c_string(read, user, name);
    }
    int at = gp_weekend_track::stock_id(items);
    if (at < 0)
        return p;
    p.record = records[std::size_t(at)];
    uint8_t stat = 0;
    get(read, user, p.record + RECORD_STAT_ID, stat);
    p.stat_id = stat;
    p.name = items[std::size_t(at)].name;
    return p;
}

// The record the site's relocated read gets: the caller's own, else the pick from the matched set, else the placeholder.
inline uint64_t record_at_site(uint64_t rdx, uint64_t set, uint64_t placeholder, Read read, void *user, Pick &p)
{
    if (rdx)
        return rdx;
    p = pick(set, read, user);
    return p.record ? p.record : placeholder;
}

inline std::string line(const Pick &p)
{
    std::string head = "a track data lookup with no track record (its caller found no record for the track it names)";
    if (p.record)
        return head + " reads " + (p.name.empty() ? std::string("?") : p.name) + " (stat id " + std::to_string(p.stat_id) +
               ") of its track data set (" + std::to_string(p.items) + " track slot(s)) instead of faulting";
    return head + " reads the first track of its track data set (" + std::to_string(p.items) +
           " track slot(s), none readable) instead of faulting";
}

}
