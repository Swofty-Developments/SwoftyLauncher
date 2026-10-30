// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>

#include "quali_laps.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

namespace q = f1990::qlaps;

static q::Act out(q::Counter &c, uint8_t v, double t)
{
    c.status(v, 10, t);
    c.status(v, 11, t);
    c.status(v, 12, t);
    q::Act a = c.status(v, q::kLeavingGarage, t);
    c.status(v, q::kLeavingPitLane, t + 10);
    c.status(v, q::kOnTrack, t + 20);
    return a;
}

static void garage(q::Counter &c, uint8_t v, double t)
{
    c.status(v, q::kEnteringPitLane, t);
    c.status(v, q::kEnteringPitBox, t + 5);
    c.status(v, 4, t + 10);
    c.status(v, 9, t + 20);
}

int main()
{
    check("qualifying and shootout ids 5..14 only",
          !q::qualifying_session(4) && q::qualifying_session(5) && q::qualifying_session(14) && !q::qualifying_session(15));

    q::Counter c;
    check("a crossing before the car has left its garage counts nothing", c.crossing(3, 0) == q::Act::none);
    check("leaving the garage starts lap 1", out(c, 3, 100) == q::Act::counted && c.car(3).used == 1);
    check("a crossing in the pit lane on the way out counts nothing",
          (c.status(3, q::kLeavingPitLane, 105), c.crossing(3, 106) == q::Act::none) && c.car(3).used == 1);
    c.status(3, q::kOnTrack, 110);
    check("a crossing 30 s after leaving the garage (pit exit before the line) counts nothing",
          c.crossing(3, 130) == q::Act::none && c.car(3).used == 1);
    check("the crossing after the out-lap starts lap 2", c.crossing(3, 200) == q::Act::counted && c.car(3).used == 2);
    check("the same crossing again 10 s later (a flashback) counts nothing", c.crossing(3, 210) == q::Act::none);
    check("lap 3 starts at the next crossing", c.crossing(3, 290) == q::Act::counted && c.car(3).used == 3);
    garage(c, 3, 350);
    check("a crossing in the pit lane on the way in counts nothing", c.crossing(3, 400) == q::Act::none);

    double t = 1000;
    check("the next run's garage exit starts lap 4", out(c, 3, t) == q::Act::counted && c.car(3).used == 4);
    q::Act a = q::Act::none;
    for (int lap = 5; lap <= 12; lap++)
        a = c.crossing(3, t += 90);
    check("lap 12 is reported as the last one", a == q::Act::last && c.car(3).used == 12 && c.left(3) == 0);
    check("the crossing that would start lap 13 sends the car in", c.crossing(3, t += 90) == q::Act::send_in);
    check("the counter stays at 12", c.car(3).used == 12);
    check("the send-in queues once", c.queue(3) && !c.queue(3));
    c.sent(3);
    garage(c, 3, t + 30);
    check("leaving the garage with no laps left sends it back", out(c, 3, t + 500) == q::Act::send_in);

    check("a release from the pit box with laps left counts nothing",
          (out(c, 11, 0), c.status(11, q::kInPitBox, 50), c.status(11, q::kLeavingPitBox, 60)) == q::Act::none &&
              c.car(11).used == 1);
    c.status(3, q::kInPitBox, t + 600);
    check("a release from the pit box with no laps left sends it back", c.status(3, q::kLeavingPitBox, t + 610) == q::Act::send_in);
    check("a drive-through crossing counts", (out(c, 7, 0), c.status(7, q::kDrivingThrough, 80), c.crossing(7, 90)) ==
                                                   q::Act::counted && c.car(7).used == 2);
    check("another car keeps its own count", c.car(5).used == 0 && c.left(5) == q::kLimit);
    check("vehicle ids past the table are ignored", c.crossing(40, 0) == q::Act::none && c.status(40, 13, 0) == q::Act::none);
    check("a repeated LeavingGarage status counts once", (c.status(9, 12, 0), c.status(9, 13, 1), c.status(9, 13, 2)) ==
                                                              q::Act::none && c.car(9).used == 1);
    c.reset();
    check("a session start resets every car", c.car(3).used == 0 && c.car(7).used == 0 && c.car(3).status == q::kUnknown);
    check("cars in or entering the garage or box are not sent", !q::out_of_garage(2) && !q::out_of_garage(4) &&
                                                                    !q::out_of_garage(9) && q::out_of_garage(1) &&
                                                                    q::out_of_garage(7) && q::out_of_garage(13));

    q::Counter t3(3);
    out(t3, 1, 0);
    t3.crossing(1, 90);
    check("a test limit of 3 makes lap 3 the last", t3.crossing(1, 180) == q::Act::last && t3.left(1) == 0);
    check("and sends the car in at the next crossing", t3.crossing(1, 270) == q::Act::send_in);

    std::printf("%s\n", failures ? "FAILED" : "quali laps: all checks passed");
    return failures ? 1 : 0;
}
