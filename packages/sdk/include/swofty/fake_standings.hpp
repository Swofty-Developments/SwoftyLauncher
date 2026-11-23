// SPDX-License-Identifier: MIT
#pragma once
#include <cstring>
#include <initializer_list>
#include <vector>
#include <swofty/standings.hpp>

// A fake core standings table for host tests and previews: set the season and the rows the addon should see, install
// it, and swofty::standings::* answers from it. One fake is installed at a time.
//
//   swofty::fake::Standings fake;
//   fake.season().round = 3;
//   fake.season().rounds = 24;
//   fake.add_driver(9, 3, 1, {25, 18}).wins = 1;
//   fake.add_constructor(3, {40, 33});
//   fake.install();
//   assert(swofty::standings::drivers().front().points == 43);

namespace swofty::fake {

class Standings {
public:
    Standings()
    {
        std::memset(&season_, 0, sizeof season_);
        season_.mode = SWOFTY_CAREER_DRIVER;
        table_ = {SWOFTY_STANDINGS_VERSION, &Standings::t_available, &Standings::t_season, &Standings::t_drivers,
                  &Standings::t_constructors};
    }

    ~Standings() { uninstall(); }
    Standings(const Standings &) = delete;
    Standings &operator=(const Standings &) = delete;

    void install()
    {
        current() = this;
        standings::detail::set_table(&table_);
    }

    void uninstall()
    {
        if (current() == this) {
            current() = nullptr;
            standings::detail::set_table(nullptr);
        }
    }

    void set_available(bool on) { available_ = on; }
    // While off, every call answers late, as the core does with no career loaded.
    void set_loaded(bool on) { loaded_ = on; }
    SwoftyStandingsSeason &season() { return season_; }

    // A driver row after the rows added before it; points is the sum of round_points.
    SwoftyStandingsRow &add_driver(uint32_t driver, uint32_t team, uint8_t race_number, std::initializer_list<int32_t> rounds)
    {
        SwoftyStandingsRow &r = add(drivers_, rounds);
        r.driver = driver;
        r.team = team;
        r.race_number = race_number;
        return r;
    }

    SwoftyStandingsRow &add_constructor(uint32_t team, std::initializer_list<int32_t> rounds)
    {
        SwoftyStandingsRow &r = add(constructors_, rounds);
        r.team = team;
        return r;
    }

    // Bumps season().seq, as the core does when the snapshot changes.
    void touch() { season_.seq++; }

private:
    static Standings *&current()
    {
        static Standings *c = nullptr;
        return c;
    }

    SwoftyStandingsRow &add(std::vector<SwoftyStandingsRow> &rows, std::initializer_list<int32_t> rounds)
    {
        SwoftyStandingsRow r;
        std::memset(&r, 0, sizeof r);
        r.driver = r.team = SWOFTY_STANDINGS_NONE;
        r.position = static_cast<int32_t>(rows.size() + 1);
        size_t i = 0;
        for (int32_t p : rounds) {
            if (i < SWOFTY_STANDINGS_MAX_ROUNDS)
                r.round_points[i++] = p;
            r.points += p;
        }
        rows.push_back(r);
        return rows.back();
    }

    int answer() const { return loaded_ ? SWOFTY_ASSET_OK : SWOFTY_ASSET_LATE; }

    int rows(const std::vector<SwoftyStandingsRow> &from, SwoftyStandingsRow *out, uint32_t cap, uint32_t *count) const
    {
        if (!count || (cap && !out))
            return SWOFTY_ASSET_INVALID;
        if (int r = answer(); r != SWOFTY_ASSET_OK)
            return r;
        *count = static_cast<uint32_t>(from.size());
        for (size_t i = 0; i < from.size() && i < cap; i++)
            out[i] = from[i];
        return SWOFTY_ASSET_OK;
    }

    static int t_available() { return current() && current()->available_ ? 1 : 0; }

    static int t_season(SwoftyStandingsSeason *out)
    {
        Standings *f = current();
        if (!f || !out)
            return SWOFTY_ASSET_INVALID;
        if (int r = f->answer(); r != SWOFTY_ASSET_OK)
            return r;
        *out = f->season_;
        out->drivers = static_cast<uint32_t>(f->drivers_.size());
        out->constructors = static_cast<uint32_t>(f->constructors_.size());
        return SWOFTY_ASSET_OK;
    }

    static int t_drivers(SwoftyStandingsRow *out, uint32_t cap, uint32_t *count)
    {
        return current() ? current()->rows(current()->drivers_, out, cap, count) : SWOFTY_ASSET_LATE;
    }

    static int t_constructors(SwoftyStandingsRow *out, uint32_t cap, uint32_t *count)
    {
        return current() ? current()->rows(current()->constructors_, out, cap, count) : SWOFTY_ASSET_LATE;
    }

    SwoftyStandings table_{};
    SwoftyStandingsSeason season_{};
    std::vector<SwoftyStandingsRow> drivers_, constructors_;
    bool available_ = true, loaded_ = true;
};

}
