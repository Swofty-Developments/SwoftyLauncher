#include <swofty/fake_live.hpp>

#include <cstdio>

static int failures;
#define CHECK(c)                                                                                                       \
    do {                                                                                                               \
        if (!(c)) {                                                                                                    \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                    \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

namespace live = swofty::live;

static SwoftyLiveTurn turn(int number, float apex)
{
    SwoftyLiveTurn t{};
    t.version = SWOFTY_LIVE_VERSION;
    t.size = sizeof t;
    t.number = number;
    t.apex_lap_m = apex;
    t.source = SWOFTY_LIVE_TURN_OFFICIAL;
    std::snprintf(t.label, sizeof t.label, "%d", number);
    return t;
}

int main()
{
    CHECK(!live::available() && !live::session() && !live::car(0) && !live::subject() && live::order().ids.empty());

    {
        swofty::fake::Live fake;
        fake.session().type = 15;
        auto &a = fake.add_car(0, 2);
        a.have |= SWOFTY_LIVE_HAVE_SPEED | SWOFTY_LIVE_HAVE_ERS_STORE | SWOFTY_LIVE_HAVE_PROGRESS | SWOFTY_LIVE_HAVE_GAP_AHEAD;
        a.speed = 80.f;
        a.ers_store = 1e6f;
        a.ers_capacity = 4e6f;
        a.lap_m = 400.f;
        a.gap_ahead_ms = 950;
        auto &b = fake.add_car(1, 1);
        fake.add_car(2, 3).pit_state = 4;
        fake.set_order({1, 0, 2});
        fake.set_player(0);
        fake.set_turns({turn(1, 500.f), turn(2, 1200.f)});
        fake.set_badge(1, "ferrari_2026");
        fake.install();
        (void)b;

        CHECK(live::available());
        auto s = live::session();
        CHECK(s && s->type == 15 && s->cars == 3 && s->player == 0 && s->subject == 0);
        auto c = live::car(0);
        CHECK(c && c->position == 2 && *live::speed_kmh(*c) == 80.f * 3.6f && *live::ers_fraction(*c) == 0.25f);
        CHECK(live::gap_ahead_ms(*c) == 950 && !live::gap_leader_ms(*c));
        CHECK(live::is_player(*c) && live::player() && live::player()->vehicle == 0 && !live::watched());
        CHECK(live::subject() && live::subject()->vehicle == 0);
        CHECK(!live::car(9) && !live::car(40));
        auto t = live::turn(*c);
        CHECK(t && t->number == 1 && t->distance_ahead_m == 100.f);
        CHECK(live::track_turns().size() == 2);
        CHECK(live::team_badge(1) == std::string("ferrari_2026") && !live::team_badge(0));
        auto o = live::order();
        CHECK(o.ids.size() == 3 && o.player_rank == 1 && o.ahead(0) && o.ahead(0)->vehicle == 1);
        CHECK(o.ahead(1) && o.ahead(1)->vehicle == 0 && !o.behind(2) && o.behind(1)->vehicle == 0);
        CHECK(live::in_pit(*live::car(2)) == false);
        fake.car(2).have |= SWOFTY_LIVE_HAVE_PIT;
        CHECK(live::in_pit(*live::car(2)));
        CHECK(!live::pit_status(*live::car(2)) && !live::pit_stops(*live::car(2)));
        fake.car(2).have |= SWOFTY_LIVE_HAVE_PIT_LANE;
        fake.car(2).pit_status = 2;
        fake.car(2).pit_stops = 3;
        CHECK(live::pit_status(*live::car(2)) == 2 && live::pit_stops(*live::car(2)) == 3);
        auto f0 = live::frame();
        fake.advance();
        CHECK(f0 && live::frame() && *live::frame() == *f0 + 1);
        fake.set_watched(2);
        CHECK(live::watched() && live::watched()->vehicle == 2 && live::subject()->vehicle == 2);
        fake.set_available(false);
        CHECK(!live::session() && !live::car(0));
    }
    CHECK(!live::available());
    if (failures)
        return 1;
    std::printf("live sdk ok\n");
    return 0;
}
