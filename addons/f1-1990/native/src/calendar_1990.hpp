// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

#include "json_strings.hpp"
#include "weekend.hpp"

namespace f1990 {

inline constexpr char kTrackList1990[] = "F1_1990";
inline constexpr size_t kCalendarEntry = 0x28;

// A run of 0x28-byte calendar entries {vtable, u8 event stat id +8, activity +0x10, u32 +0x18, date +0x20}:
// the season length component's calendar (+0x2b0..+0x2b8) or one track list of the career's dynamic
// event data.
struct Events {
    const uint8_t *begin = nullptr, *end = nullptr;
    size_t size() const { return begin && end > begin ? size_t(end - begin) / kCalendarEntry : 0; }
};

inline Events calendar_events(const void *component)
{
    return {at<const uint8_t *>(component, 0x2b0), at<const uint8_t *>(component, 0x2b8)};
}

// The career's dynamic event data (career +0xf58) keeps one entry per track list at +0xb8..+0xc0,
// 0x28 each {track list name, allocator, begin, end, capacity}.
inline Events track_list_events(const void *career, const char *name)
{
    auto *data = career ? at<const uint8_t *>(career, 0xf58) : nullptr;
    auto *b = data ? at<const uint8_t *>(data, 0xb8) : nullptr, *e = data ? at<const uint8_t *>(data, 0xc0) : nullptr;
    for (; b && b < e; b += kCalendarEntry) {
        const char *list = at<const char *>(b, 0);
        if (list && !std::strcmp(list, name))
            return {at<const uint8_t *>(b, 0x10), at<const uint8_t *>(b, 0x18)};
    }
    return {};
}

inline bool has_event(const Events &events, uint8_t id)
{
    for (auto *p = events.begin; p && p < events.end; p += kCalendarEntry)
        if (p[8] == id)
            return true;
    return false;
}

inline bool same_events(const Events &calendar, const Events &list)
{
    if (calendar.size() != list.size())
        return false;
    for (size_t i = 0; i < list.size(); i++)
        if (calendar.begin[i * kCalendarEntry + 8] != list.begin[i * kCalendarEntry + 8])
            return false;
    return true;
}

inline size_t missing_events(const Events &calendar, const Events &list)
{
    size_t missing = 0;
    for (auto *p = list.begin; p && p < list.end; p += kCalendarEntry)
        missing += !has_event(calendar, p[8]);
    return missing;
}

// A 1990 round: its event's stat id and, from data/calendar-1990.json, the day of the event's stock
// date_time and the day it moves to (the 1990 one in the career year), both in days since 1970-01-01.
struct EventShift {
    uint8_t id = 0;
    int64_t stock = 0, date = 0;
};

inline int64_t days_from_civil(int y, unsigned m, unsigned d)
{
    y -= m <= 2;
    const int64_t era = (y >= 0 ? y : y - 399) / 400;
    const unsigned yoe = unsigned(y - era * 400);
    const unsigned doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1;
    const unsigned doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + int64_t(doe) - 719468;
}

inline bool parse_day(const std::string &iso, int64_t &days)
{
    int y = 0;
    unsigned m = 0, d = 0;
    char tail = 0;
    if (iso.size() != 10 || std::sscanf(iso.c_str(), "%4d-%2u-%2u%c", &y, &m, &d, &tail) != 3 || m < 1 || m > 12 ||
        d < 1 || d > 31)
        return false;
    days = days_from_civil(y, m, d);
    return true;
}

// data/calendar-1990.json: {"events": {"<stat id>": {"stock": "YYYY-MM-DD", "date": "YYYY-MM-DD", ...}, ...}}.
inline std::vector<EventShift> read_shifts(const std::vector<StringLeaf> &leaves)
{
    struct Pending {
        int64_t stock = 0, date = 0;
        bool has_stock = false, has_date = false;
    };
    std::vector<std::pair<unsigned, Pending>> rows;
    for (auto &l : leaves) {
        if (l.path.size() != 3 || l.path[0] != "events")
            continue;
        unsigned id = 0;
        char tail = 0;
        if (std::sscanf(l.path[1].c_str(), "%u%c", &id, &tail) != 1 || id > 0xff)
            continue;
        auto it = rows.begin();
        while (it != rows.end() && it->first != id)
            ++it;
        if (it == rows.end())
            it = rows.insert(rows.end(), {id, Pending{}});
        if (l.path[2] == "stock")
            it->second.has_stock = parse_day(l.text, it->second.stock);
        else if (l.path[2] == "date")
            it->second.has_date = parse_day(l.text, it->second.date);
    }
    std::vector<EventShift> out;
    for (auto &[id, p] : rows)
        if (p.has_stock && p.has_date)
            out.push_back({uint8_t(id), p.stock, p.date});
    return out;
}

// data/calendar-1990.json "season_start": the first day of the 1990 season in the career year (the
// event list's start_date), in days since 1970-01-01.
inline bool read_season_start(const std::vector<StringLeaf> &leaves, int64_t &day)
{
    for (auto &l : leaves)
        if (l.path.size() == 1 && l.path[0] == "season_start")
            return parse_day(l.text, day);
    return false;
}

inline int64_t utc_day(int64_t t)
{
    return t >= 0 ? t / 86400 : -((-t + 86399) / 86400);
}

// Sets each entry's minute offset (u32 +0x18) from the season start, as the game's date sort does
// (difftime(date, start) / 60). Dates and the season start are midnights of the same time zone with
// no daylight saving, so the zone is taken from the first entry that sits on its 1990 day. Returns
// how many offsets were written; 0 when no entry anchors the zone.
inline size_t set_offsets(uint8_t *begin, uint8_t *end, const std::vector<EventShift> &shifts, int64_t season_start)
{
    bool anchored = false;
    int64_t zone = 0;
    for (auto *p = begin; p && p < end && !anchored; p += kCalendarEntry) {
        int64_t date = at<int64_t>(p, 0x20);
        for (auto &s : shifts)
            if (s.id == p[8] && date && utc_day(date) >= s.date - 1 && utc_day(date) <= s.date + 1) {
                zone = date - s.date * 86400;
                anchored = true;
                break;
            }
    }
    if (!anchored)
        return 0;
    size_t written = 0;
    for (auto *p = begin; p && p < end; p += kCalendarEntry) {
        int64_t date = at<int64_t>(p, 0x20);
        int64_t minutes = (date - zone - season_start * 86400) / 60;
        if (!date || minutes < 0 || minutes > 0xffffffffll)
            continue;
        uint32_t m = uint32_t(minutes);
        std::memcpy(p + 0x18, &m, sizeof m);
        written++;
    }
    return written;
}

// Moves the date (+0x20, a time_t from the game's mktime of date_time) of each calendar entry whose
// event has a shift by whole days, when the date still falls on the stock day (within a day either
// side, for the time zone); a moved date is left alone, so a second pass changes nothing. Returns how
// many moved.
inline size_t shift_dates(uint8_t *begin, uint8_t *end, const std::vector<EventShift> &shifts)
{
    size_t moved = 0;
    for (auto *p = begin; p && p < end; p += kCalendarEntry) {
        int64_t date = at<int64_t>(p, 0x20);
        int64_t day = utc_day(date);
        for (auto &s : shifts) {
            if (s.id != p[8] || s.date == s.stock || day < s.stock - 1 || day > s.stock + 1)
                continue;
            date += (s.date - s.stock) * 86400;
            std::memcpy(p + 0x20, &date, sizeof date);
            moved++;
            break;
        }
    }
    return moved;
}

inline std::string civil_day(int64_t days)
{
    days += 719468;
    const int64_t era = (days >= 0 ? days : days - 146096) / 146097;
    const unsigned doe = unsigned(days - era * 146097);
    const unsigned yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    const unsigned doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    const unsigned mp = (5 * doy + 2) / 153;
    const unsigned d = doy - (153 * mp + 2) / 5 + 1;
    const unsigned m = mp < 10 ? mp + 3 : mp - 9;
    const int64_t y = int64_t(yoe) + era * 400 + (m <= 2);
    char b[32];
    std::snprintf(b, sizeof b, "%04lld-%02u-%02u", static_cast<long long>(y), m, d);
    return b;
}

// "id@YYYY-MM-DD" per entry, the date's day taken at noon UTC so a local midnight in any time zone
// within 12 hours reads as its own day.
inline std::string event_days(const Events &events)
{
    std::string out;
    for (auto *p = events.begin; p && p < events.end && out.size() < 400; p += kCalendarEntry) {
        int64_t date = at<int64_t>(p, 0x20) + 43200;
        int64_t day = date >= 0 ? date / 86400 : -((-date + 86399) / 86400);
        char b[8];
        std::snprintf(b, sizeof b, "%u@", p[8]);
        out += out.empty() ? "" : ",";
        out += b + civil_day(day);
    }
    return out;
}

inline std::string event_ids(const Events &events)
{
    std::string out;
    char b[8];
    for (auto *p = events.begin; p && p < events.end && out.size() < 200; p += kCalendarEntry) {
        std::snprintf(b, sizeof b, "%s%u", out.empty() ? "" : ",", p[8]);
        out += b;
    }
    return out;
}

}
