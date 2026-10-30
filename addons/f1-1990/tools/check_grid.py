#!/usr/bin/env python3
"""Checks the 1990 grid in a built database.

usage: check_grid.py <stock db> <db built without the grid ops> <db built with them> [--cars 22|26] [--lists game-lists.json]

1. Stock: the grid ops change no stock record except the row counts of the tables they add rows to.
2. Rows: every seat of f1_1990 resolves to a 1990 team, an entrant, a 1990 driver with a statistics holder and a look;
   labels, stat ids and car numbers are unique; the career list, pool, setup mode, standings and the per-team rows
   of the f1_career_1990 data group exist; the f1_1990 track layer loads the 1990 worlds and no other layer lists
   f1_1990; the f1_1990 game setup mode uses the f1_1990 entrant setup mode. Missing <team>_1990 vehicle rows are
   reported, not failed (cars lane).
"""
import argparse
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT.parents[1] / "tools"))
import gamedb as g  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import grid  # noqa: E402
import tracks  # noqa: E402
from season import slots  # noqa: E402


class Db:
    def __init__(self, path):
        self.recs = {r.key: r.value for r in g.read(Path(path).read_bytes())}

    def get(self, path):
        v = self.recs.get(g.fnv1a64(path))
        return v.decode("utf-8", "replace") if v is not None else None

    def rows(self, table, word):
        n = int(self.get(f"{table}._{word}_instancecount_") or 0)
        return [(f"{table}.{word}_{i}", self.get(f"{table}.{word}_{i}._metadata_")) for i in range(n)]

    def find(self, table, word, label, field="_metadata_"):
        hits = [p for p, _ in self.rows(table, word) if self.get(f"{p}.{field}") == label]
        return hits[0] if len(hits) == 1 else None


def stock_check(stock, base, built, fail):
    counts = 0
    for key in stock.recs:
        before, after = base.recs.get(key), built.recs.get(key)
        if before == after:
            continue
        counts += 1
        if before is None or after is None or not (before.isdigit() and after.isdigit()):
            fail(f"stock record {key:016x} changed: {before!r} -> {after!r}")
    return counts


def rows_check(db, cars, lists, fail):
    teams = [s.team for s in slots() if cars == 26 or s.team not in grid.EXTRA_TEAMS]
    team_rows = {grid.team_row(t) for t in teams}
    missing_vehicles = [r for r in sorted(team_rows) if not db.find("entrants.vehicles", "vehicle", r)]
    for row in sorted(team_rows):
        if not db.find("entrants.teams", "team", row):
            fail(f"team row {row} missing")
    lst = db.find("entrants.entrant_lists", "entrant_list", grid.GRID_LIST)
    if not lst:
        fail(f"entrant list {grid.GRID_LIST} missing")
        return missing_vehicles, 0
    seats = db.rows(lst, "entrant")
    if len(seats) != 2 * len(teams):
        fail(f"{grid.GRID_LIST}: {len(seats)} seats for {len(teams)} teams")
    numbers, drivers = Counter(), Counter()
    for p, label in seats:
        team, ref = db.get(f"{p}.team"), db.get(f"{p}.entrant_reference")
        if team not in team_rows:
            fail(f"{label}: team {team}")
        ent = db.find(grid.ENTRANTS, "entrant", ref)
        if not ent:
            fail(f"{label}: entrant {ref} missing")
            continue
        drv, look = db.get(f"{ent}.driver"), db.get(f"{ent}.driver_customisation")
        drivers[drv] += 1
        numbers[db.get(f"{p}.car_number")] += 1
        if not drv.startswith("f1990_") or not db.find(grid.DRIVERS, "driver", drv):
            fail(f"{label}: driver {drv}")
        if not db.find(grid.STATS_TABLE, "DriverStatisticsHolder", drv, "driver"):
            fail(f"{label}: no statistics holder for {drv}")
        lrow = db.find(grid.LOOK_FOLDER, grid.LOOK_WORD, look)
        if not lrow:
            fail(f"{label}: look {look} missing")
        elif lists:
            for part in ("helmet", "suit", "gloves"):
                if f"{part}/{db.get(f'{lrow}.{part}')}" in lists:
                    fail(f"{label}: {part} {db.get(f'{lrow}.{part}')} has no game asset")
    for what, c in (("car number", numbers), ("driver", drivers)):
        for k, n in c.items():
            if n > 1:
                fail(f"{grid.GRID_LIST}: {what} {k} on {n} seats")
    for table, word, field in ((grid.DRIVERS, "driver", "stat_id"), (grid.TEAMS, "team", "stat_id"),
                               (grid.LOOK_FOLDER, grid.LOOK_WORD, "stat_id"), (grid.DRIVERS, "driver", "_metadata_"),
                               (grid.TEAMS, "team", "_metadata_")):
        for k, n in Counter(db.get(f"{p}.{field}") for p, _ in db.rows(table, word)).items():
            if n > 1 and k not in (None, ""):
                fail(f"{table}.{word}.{field} {k} used {n} times")
    career = db.find(grid.LISTS, "entrant_list", grid.CAREER_LIST)
    if not career or db.get(f"{career}.entrant_list_references") != f"{grid.GRID_LIST},{grid.POOL_LIST}":
        fail(f"{grid.CAREER_LIST} missing or not pointing at {grid.GRID_LIST},{grid.POOL_LIST}")
    pool = db.find(grid.LISTS, "entrant_list", grid.POOL_LIST)
    for p, label in db.rows(pool, "entrant") if pool else []:
        if not db.find(grid.ENTRANTS, "entrant", db.get(f"{p}.entrant_reference")):
            fail(f"{grid.POOL_LIST}.{label}: entrant missing")
    mode = db.find(grid.SETUP_MODES, "entrant_setup_mode", grid.SETUP_MODE)
    if not mode or db.get(f"{mode}.max_entrants") != str(cars):
        fail(f"entrant setup mode {grid.SETUP_MODE} missing or max_entrants != {cars}")
    layer = db.find(grid.TRACK_LAYERS, "TrackLayer", grid.SETUP_MODE)
    if not layer or db.get(f"{layer}.worldEri") != tracks.TRACK_LAYER:
        fail(f"track layer {grid.SETUP_MODE} missing or not on the 1990 worlds")
    for p, label in db.rows(grid.TRACK_LAYERS, "TrackLayer"):
        if label != grid.SETUP_MODE and grid.SETUP_MODE in (db.get(f"{p}.game_setup_modes") or "").split(","):
            fail(f"track layer {label} also lists {grid.SETUP_MODE}")
    setup = db.find(grid.GAME_SETUP_MODES.rsplit(".", 1)[0], "game_setup_mode", grid.SETUP_MODE)
    if setup and db.get(f"{setup}.entrant_setup_modes") != grid.SETUP_MODE:
        fail(f"game setup mode {grid.SETUP_MODE} does not use entrant setup mode {grid.SETUP_MODE}")
    std = db.find(grid.STANDINGS, "standings", grid.TEAM_STANDINGS)
    if not std or {db.get(f"{p}.team") for p, _ in db.rows(std, "team_reference")} != team_rows:
        fail(f"{grid.TEAM_STANDINGS} does not list the {len(team_rows)} teams")
    std = db.find(grid.STANDINGS, "standings", grid.DRIVER_STANDINGS)
    if not std or any(not db.find(grid.DRIVERS, "driver", db.get(f"{p}.driver"))
                      for p, _ in db.rows(std, "driver_reference")):
        fail(f"{grid.DRIVER_STANDINGS} missing or names a missing driver")
    for container, word, sub, child, field in grid.GROUP_TEAM_TABLES:
        group = db.find(container, word, grid.DATA_GROUP)
        if not group:
            fail(f"{container}: no {grid.DATA_GROUP} data group")
            continue
        have = {db.get(f"{p}.{field}") for p, _ in db.rows(f"{group}.{sub}", child)}
        if team_rows - have:
            fail(f"{container} {grid.DATA_GROUP}: no rows for {sorted(team_rows - have)}")
        left = sorted(v for v in have if v and v.endswith("_26"))
        if left:
            fail(f"{container} {grid.DATA_GROUP}: still lists {left}")
    return missing_vehicles, len(db.rows(grid.STATS_TABLE, "DriverStatisticsHolder"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("stock")
    ap.add_argument("base")
    ap.add_argument("built")
    ap.add_argument("--cars", type=int, default=22)
    ap.add_argument("--lists")
    a = ap.parse_args()
    errors = []
    stock, base, built = Db(a.stock), Db(a.base), Db(a.built)
    counts = stock_check(stock, base, built, errors.append)
    lists = set(json.load(open(a.lists))["missing"]) if a.lists else None
    missing_vehicles, holders = rows_check(built, a.cars, lists, errors.append)
    print(f"{counts} stock row count(s) changed, no other stock record")
    print(f"{holders} driver statistics holders")
    if missing_vehicles:
        print(f"vehicle rows not in this build (cars lane): {', '.join(missing_vehicles)}")
    for e in errors:
        print("FAIL", e)
    print("ok" if not errors else f"{len(errors)} problem(s)")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
