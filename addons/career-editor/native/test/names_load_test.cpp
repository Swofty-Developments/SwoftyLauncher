// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Host unit test for the sliced gamedb name load: a roster read a row at a time must not be reported ready until
// every row is in, and the page must be told when it is, whichever caller finishes the read.
#include <cstdio>
#include <functional>
#include <map>
#include <string>

#include "names_load.hpp"

static int fails = 0;
#define CHECK(c) do { if (!(c)) { std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c); ++fails; } } while (0)

struct Db {
    bool up = true;
    int64_t rows[ce::names::kStages] = {199, 358, 5};
    std::map<int, std::map<int64_t, std::string>> got;
    int finished_calls = 0;
    bool finished_loaded = false;

    bool available() const { return up; }
    int64_t count(int stage) const { return rows[stage]; }
    void row(int stage, int64_t i) { got[stage][i] = "n" + std::to_string(i); }
    bool filled() const { return got.count(0) && !got.at(0).empty(); }
    void finished(bool loaded) { ++finished_calls; finished_loaded = loaded; }
};

struct Clock {
    double t = 0;
    double operator()() { return t += 1.0; }
};

int main() {
    {
        Db db;
        ce::names::Load l;
        Clock clock;
        int idles = 0, reports = 0;
        for (; idles < 10000 && !l.loaded; ++idles) {
            bool was_pending = ce::names::pending(l, db);
            int r = ce::names::idle(l, db, std::ref(clock), 2.0);
            if (r == 2) {
                ++reports;
                CHECK(db.got[0].size() == 199 && db.got[1].size() == 358 && db.got[2].size() == 5);
            }
            if (!l.loaded)
                CHECK(was_pending);
        }
        CHECK(l.loaded && reports == 1 && db.finished_calls == 1 && db.finished_loaded);
        CHECK(idles > 100);
        CHECK(!ce::names::pending(l, db));
        CHECK(ce::names::idle(l, db, std::ref(clock), 2.0) == 0);
        CHECK(db.got[0].count(198) == 1 && db.got[1].count(357) == 1);
    }
    {
        Db db;
        ce::names::Load l;
        Clock clock;
        for (int i = 0; i < 20; ++i)
            ce::names::idle(l, db, std::ref(clock), 2.0);
        CHECK(!l.settled && ce::names::pending(l, db));
        CHECK(ce::names::step(l, db, std::ref(clock), 1e9));
        CHECK(l.loaded && db.got[0].size() == 199);
        CHECK(ce::names::idle(l, db, std::ref(clock), 2.0) == 2);
        CHECK(ce::names::idle(l, db, std::ref(clock), 2.0) == 0);
    }
    {
        Db db;
        db.up = false;
        ce::names::Load l;
        Clock clock;
        CHECK(!ce::names::pending(l, db));
        CHECK(ce::names::idle(l, db, std::ref(clock), 2.0) == 0);
    }
    {
        Db db;
        db.rows[0] = 0;
        ce::names::Load l;
        Clock clock;
        CHECK(ce::names::idle(l, db, std::ref(clock), 1e9) == 2);
        CHECK(!l.loaded && !ce::names::pending(l, db));
        db.rows[0] = 50;
        int r = 0;
        for (int i = 0; i < 100 && r != 2; ++i)
            r = ce::names::idle(l, db, std::ref(clock), 1e9);
        CHECK(r == 2 && l.loaded && db.got[0].size() == 50);
    }
    if (fails) return 1;
    std::printf("ok\n");
    return 0;
}
