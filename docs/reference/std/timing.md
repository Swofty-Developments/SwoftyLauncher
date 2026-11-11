---
title: Timing tower
description: A deterministic timing tower model over plain structs, and the standard hud_tt tower leaves.
---

# Timing tower

`swofty/timing.hpp` turns the classification order into tower rows and publishes them as the standard tower leaves. The model is pure: the same inputs give the same rows, with no game access, so it runs in host tests.

> **Depends on live state.** `timing::Car` and `timing::Session` are the tower's own input structs. They are filled from `swofty::live` (the memory-backed live state) by an adapter that ships with that header; until it exists, fill them from your own reader or from literals in tests. The field names follow the live structs (`position`, `lap`, `gap_ahead_ms`, `gap_leader_ms`, `time_penalty_s`, `drive_through`, `stop_go`, `result_status` as `status`).

```cpp
#include <swofty/timing.hpp>

namespace timing = swofty::timing;

swofty::hud::Leaves L{"view_model_data.hud_tt_"};
timing::TowerLeaves leaves{L};                    // declares the standard leaves, once
timing::Tower tower;

void on_frame(double now)
{
    timing::Session session;
    session.type = 15;                            // a race
    session.header = timing::Session::safety_car; // the flag header, priority resolved by the live layer

    std::vector<timing::Car> order = read_classification();   // first place first
    timing::Config cfg;
    cfg.mode = timing::Config::Mode::compact;     // full, compact (around the subject) or top N
    cfg.rows = 5;
    cfg.gap = timing::Config::Gap::autoswitch;    // interval, leader, or flip every auto_period_s

    tower.update(session, order, subject_vehicle, now, cfg);
    leaves.publish(tower);                        // globals, then each row as one atomic group
    L.flush();
}

// rows by hand
for (int i = 0; i < tower.rows(); i++) {
    const timing::Row &r = tower.row(i);          // pos, gap text, up/down arrow, tyre, pit, out, me
}
int row = tower.row_of_car(7);                    // the cVV_row map, -1 when not shown

auto rival = timing::car_ahead(order, subject);   // P1 sees P2; std::nullopt without a classification
```

- **Gaps:** in a race a leader shows `Leader` or `Interval`; others show `+5.1` from the game's gap when its valid flag is set and the car is running or finished, `PIT` in the pits, `+N LAP(S)` from the lap counters, `Out`/`DSQ`/`NC` for the retired. Timed sessions show the best-lap difference when `best_lap_ms` is known, else the gap is unavailable (`gap_na`).
- **Arrows:** a position change lights up/down for `arrow_seconds` (4 s), cleared by `reset()`.
- **Tyre:** the letter is held for `tyre_hold_s` over a missing read.
- **Time Trial:** `session.time_trial` turns the tower off.
- **Leaves:** the tower contract is Halo's (v5): globals `on trial show rows lead race timed sess lap lapna hs sc vsc chq red yel ysec ysna`, `cVV_row` for 24 cars, and per row `rNN_{on,vid,pos,p1,up,dn,gap,gna,tyre,tS,tM,tH,tI,tW,tna,pit,pen,out,me,code,badge,bdg,cVV}`; `hs` is 0 none, 1 safety car, 2 VSC, 3 red, 4 chequered, 5 yellow.
- **Declared, empty until a source exists:** `best_lap_ms`, `last_lap_ms`, `best_sector_ms`, `purple_lap`, `purple_sector`, `green_sector`.

The tower model and leaf contract are ported from Project Halo's timing tower. Uses work from Project Halo by nyannoying, <https://github.com/nyannoying1337/f1-25-project-halo>.
