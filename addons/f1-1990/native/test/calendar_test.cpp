// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstring>
#include <vector>

#include "calendar_1990.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

template <class T> static void put(uint8_t *p, size_t off, T v)
{
    std::memcpy(p + off, &v, sizeof v);
}

static std::vector<uint8_t> entries(std::initializer_list<uint8_t> ids)
{
    std::vector<uint8_t> out(ids.size() * f1990::kCalendarEntry);
    size_t i = 0;
    for (uint8_t id : ids)
        out[i++ * f1990::kCalendarEntry + 8] = id;
    return out;
}

int main(int argc, char **argv)
{
    const auto ids1990 = {134, 133, 27, 117, 116, 132, 119, 121, 111, 124, 123, 127, 128, 118, 112, 110};
    std::vector<uint8_t> list1990(ids1990.size() * f1990::kCalendarEntry);
    size_t k = 0;
    for (int id : ids1990)
        list1990[k++ * f1990::kCalendarEntry + 8] = uint8_t(id);
    auto stock = entries({110, 111, 112, 1, 2, 3});

    uint8_t lists[3 * f1990::kCalendarEntry] = {};
    static const char stock_name[] = "F1_2026_full_plus", other[] = "F1_1990_x", name1990[] = "F1_1990";
    put(lists, 0x00, stock_name);
    put(lists, 0x10, static_cast<const uint8_t *>(stock.data()));
    put(lists, 0x18, static_cast<const uint8_t *>(stock.data() + stock.size()));
    put(lists, 0x28, other);
    put(lists, 0x50, name1990);
    put(lists, 0x60, static_cast<const uint8_t *>(list1990.data()));
    put(lists, 0x68, static_cast<const uint8_t *>(list1990.data() + list1990.size()));
    uint8_t data[0xc8] = {};
    put(data, 0xb8, static_cast<const uint8_t *>(lists));
    put(data, 0xc0, static_cast<const uint8_t *>(lists + sizeof lists));
    uint8_t career[0xf60] = {};
    put(career, 0xf58, static_cast<const uint8_t *>(data));

    f1990::Events list = f1990::track_list_events(career, f1990::kTrackList1990);
    check("finds F1_1990 by name, not a name it prefixes", list.size() == 16 && list.begin == list1990.data());
    check("lists the 1990 events in order", f1990::event_ids(list) == "134,133,27,117,116,132,119,121,111,124,123,127,128,118,112,110");
    check("finds no list that is not there", f1990::track_list_events(career, "F1_1991").size() == 0);
    uint8_t empty_career[0xf60] = {};
    check("a career without dynamic event data has no list", f1990::track_list_events(empty_career, f1990::kTrackList1990).size() == 0);

    uint8_t component[0x2d0] = {};
    put(component, 0x2b0, static_cast<const uint8_t *>(list1990.data()));
    put(component, 0x2b8, static_cast<const uint8_t *>(list1990.data() + list1990.size()));
    f1990::Events full = f1990::calendar_events(component);
    check("a calendar holding F1_1990 misses none of it", f1990::missing_events(full, list) == 0);

    auto filtered = entries({110, 111, 112, 117, 116, 118, 119, 121, 123, 124, 127, 128, 132, 133, 134});
    put(component, 0x2b0, static_cast<const uint8_t *>(filtered.data()));
    put(component, 0x2b8, static_cast<const uint8_t *>(filtered.data() + filtered.size()));
    f1990::Events stock_order = f1990::calendar_events(component);
    check("a calendar cut to the stock list misses Imola", f1990::missing_events(stock_order, list) == 1 &&
                                                               !f1990::has_event(stock_order, 27) &&
                                                               f1990::has_event(stock_order, 134));
    put(component, 0x2b0, static_cast<const uint8_t *>(nullptr));
    put(component, 0x2b8, static_cast<const uint8_t *>(nullptr));
    check("an empty calendar misses all 16", f1990::missing_events(f1990::calendar_events(component), list) == 16 &&
                                                 f1990::event_ids(f1990::calendar_events(component)).empty());

    check("days count from 1970-01-01", f1990::days_from_civil(1970, 1, 1) == 0 && f1990::days_from_civil(2026, 3, 5) == 20517);
    check("days read back as dates", f1990::civil_day(20517) == "2026-03-05" && f1990::civil_day(-1) == "1969-12-31");
    int64_t day = 0;
    check("parses an ISO day", f1990::parse_day("2026-11-18", day) && day == f1990::days_from_civil(2026, 11, 18));
    check("rejects other day text", !f1990::parse_day("2026-1-18", day) && !f1990::parse_day("2026-13-01", day) &&
                                        !f1990::parse_day("2026-11-18x", day));

    std::vector<f1990::StringLeaf> leaves;
    bool parsed = f1990::parse_string_leaves(R"({"scope": "f1_career_1990", "year": "2026", "events": {
        "134": {"event": "rw_las_vegas_26", "stock": "2026-11-18", "date": "2026-03-05"},
        "27": {"event": "rw_imola", "stock": "2025-05-15", "date": "2026-05-07"},
        "127": {"event": "rw_monza_26", "stock": "2026-09-03", "date": "2026-09-03"},
        "999": {"stock": "2026-01-01", "date": "2026-01-02"},
        "110": {"stock": "2026-03-05"}}})", leaves);
    auto shifts = f1990::read_shifts(leaves);
    check("reads the round dates, skipping bad ids and half rows", parsed && shifts.size() == 3 && shifts[0].id == 134 &&
                                                                       shifts[1].id == 27 && shifts[2].id == 127);

    const int64_t tz = -10 * 3600;
    auto cal = entries({134, 27, 127, 110});
    auto set_date = [&](size_t i, int64_t t) { put(cal.data() + i * f1990::kCalendarEntry, 0x20, t); };
    set_date(0, f1990::days_from_civil(2026, 11, 18) * 86400 + tz);
    set_date(1, f1990::days_from_civil(2025, 5, 15) * 86400 + tz);
    set_date(2, f1990::days_from_civil(2026, 9, 3) * 86400 + tz);
    set_date(3, f1990::days_from_civil(2026, 3, 5) * 86400 + tz);
    size_t moved = f1990::shift_dates(cal.data(), cal.data() + cal.size(), shifts);
    f1990::Events ev{cal.data(), cal.data() + cal.size()};
    check("moves Phoenix and Imola to their 1990 days, keeps Monza and unknown events", moved == 2 &&
              f1990::event_days(ev) == "134@2026-03-05,27@2026-05-07,127@2026-09-03,110@2026-03-05");
    check("keeps the time of day", f1990::at<int64_t>(cal.data(), 0x20) == f1990::days_from_civil(2026, 3, 5) * 86400 + tz);
    check("a second pass moves nothing", f1990::shift_dates(cal.data(), cal.data() + cal.size(), shifts) == 0 &&
                                             f1990::event_days(ev) == "134@2026-03-05,27@2026-05-07,127@2026-09-03,110@2026-03-05");
    int64_t start = 0;
    std::vector<f1990::StringLeaf> with_start;
    f1990::parse_string_leaves(R"({"season_start": "2026-02-11", "events": {}})", with_start);
    check("reads the season start", f1990::read_season_start(with_start, start) && f1990::civil_day(start) == "2026-02-11");
    check("no season start without the key", !f1990::read_season_start(leaves, start));
    f1990::read_season_start(with_start, start);
    size_t written = f1990::set_offsets(cal.data(), cal.data() + cal.size(), shifts, start);
    check("sets minute offsets from the season start in the entries' zone",
          written == 4 && f1990::at<uint32_t>(cal.data(), 0x18) == uint32_t((f1990::days_from_civil(2026, 3, 5) - start) * 1440) &&
              f1990::at<uint32_t>(cal.data() + 2 * f1990::kCalendarEntry, 0x18) ==
                  uint32_t((f1990::days_from_civil(2026, 9, 3) - start) * 1440));
    auto unanchored = entries({200});
    put(unanchored.data(), 0x20, int64_t(f1990::days_from_civil(2026, 3, 5) * 86400));
    check("sets no offsets without an anchor", f1990::set_offsets(unanchored.data(), unanchored.data() + unanchored.size(),
                                                                   shifts, start) == 0);
    auto same_a = entries({134, 133}), same_b = entries({134, 133}), swapped = entries({133, 134}), shorter = entries({134});
    f1990::Events ea{same_a.data(), same_a.data() + same_a.size()}, eb{same_b.data(), same_b.data() + same_b.size()},
        es{swapped.data(), swapped.data() + swapped.size()}, eh{shorter.data(), shorter.data() + shorter.size()};
    check("same events in the same order match", f1990::same_events(ea, eb));
    check("another order or length does not", !f1990::same_events(es, eb) && !f1990::same_events(eh, eb));

    auto undated = entries({134});
    check("leaves an undated event alone", f1990::shift_dates(undated.data(), undated.data() + undated.size(), shifts) == 0);

    if (argc > 1) {
        std::FILE *f = std::fopen(argv[1], "rb");
        std::string json;
        char buf[4096];
        for (size_t n; f && (n = std::fread(buf, 1, sizeof buf, f)) > 0;)
            json.append(buf, n);
        if (f)
            std::fclose(f);
        std::vector<f1990::StringLeaf> shipped;
        auto rounds = f1990::parse_string_leaves(json, shipped) ? f1990::read_shifts(shipped) : std::vector<f1990::EventShift>{};
        bool ascending = true;
        for (size_t i = 1; i < rounds.size(); i++)
            ascending &= rounds[i].date > rounds[i - 1].date;
        bool thursdays = true;
        for (auto &r : rounds)
            thursdays &= (r.date % 7 + 7) % 7 == 0;
        check("data/calendar-1990.json has 16 rounds, Phoenix first", rounds.size() == 16 && rounds[0].id == 134 &&
                                                                         rounds.back().id == 110);
        check("its 1990 days ascend round by round", ascending);
        check("each 1990 day is a Thursday, 3 days before the race Sunday", thursdays);
        int64_t shipped_start = 0;
        check("its season starts on 2026-02-11, before Phoenix",
              f1990::read_season_start(shipped, shipped_start) && f1990::civil_day(shipped_start) == "2026-02-11" &&
                  rounds.size() && rounds[0].date > shipped_start);
        check("Phoenix moves to 2026-03-05, Adelaide to 2026-10-29",
              f1990::civil_day(rounds[0].date) == "2026-03-05" && f1990::civil_day(rounds.back().date) == "2026-10-29");
    }

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
