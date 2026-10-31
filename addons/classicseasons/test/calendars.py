#!/usr/bin/env python3
"""Check packages/sdk/std/data/venues.json and Classic Seasons' calendars.

venues.json: every key and country is [a-z0-9_]+, every name and alias is unique among all venues once normalised the
way the launcher matches a track's name (lower case, each run of other characters one space), and every stock code is
a stock track with assets that provides only that venue. Each season: its registry/content.json is what calendars.py
writes (the calendar from Progressive's data/rounds.json), the file validates against the registry schemas
(career.calendar 1.0 and content.package 1.0), every round names a venue in venues.json, required exactly when a stock
track provides it, and the package names the calendar optionally.
"""

import json
import re
import sys
from pathlib import Path

CS = Path(__file__).resolve().parents[1]
REPO = CS.parents[1]
sys.path.insert(0, str(CS / "classic-seasons" / "tools"))
sys.path.insert(0, str(REPO / "tools" / "std"))

import calendars  # noqa: E402
import validate  # noqa: E402

STOCK = {"abu_dhabi", "austria", "bahrain", "baku", "brazil", "catalunya", "hungaroring", "imola", "jeddah",
         "las_vegas", "losail", "madrid", "melbourne", "mexico", "miami", "monaco", "montreal", "monza", "shanghai",
         "silverstone", "singapore", "spa_francorchamps", "suzuka", "texas", "zandvoort"}
KEY = re.compile(r"^[a-z0-9_]+$")

failures = 0


def check(ok, what):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'}  {what}")
    failures += not ok


def normal(text):
    return re.sub(r"[^a-z0-9]+", " ", text.lower()).strip()


def venues():
    doc = json.loads(calendars.VENUES_FILE.read_text())
    check(doc.get("version") == 1, "venues.json version 1")
    seen, stock = {}, {}
    for key, v in doc["venues"].items():
        check(bool(KEY.match(key)) and bool(KEY.match(v["country"])), f"venue/{key}: key and country {v['country']!r}")
        check(set(v) == {"name", "country", "aliases", "stock"}, f"venue/{key}: name, country, aliases and stock")
        for text in [v["name"], *v["aliases"]]:
            n = normal(text)
            if n in seen and seen[n] != key:
                check(False, f"venue/{key}: {text!r} also matches venue/{seen[n]}")
            seen[n] = key
        if v["stock"] is not None:
            check(v["stock"] in STOCK, f"venue/{key}: stock {v['stock']} is a stock track with assets")
            check(v["stock"] not in stock, f"venue/{key}: stock {v['stock']} provides one venue only")
            stock[v["stock"]] = key
    check(set(stock) == STOCK, f"every stock track with assets provides a venue ({len(stock)} of {len(STOCK)})")
    circuits = {r["circuit"] for rounds in calendars.YEARS.values() for r in rounds}
    missing = sorted(circuits - set(doc["venues"]))
    check(not missing, f"every circuit of rounds.json ({len(circuits)}) is a venue" + (f"; missing {missing}" if missing else ""))
    check(len(seen) > len(doc["venues"]), f"{len(doc['venues'])} venues, {len(seen)} names to match")


def season(year):
    path = calendars.content_file(year)
    text = path.read_text()
    content = json.loads(text)
    check(text == calendars.render(calendars.with_calendar(content, year)), f"{year}: content.json is what calendars.py writes")
    check(validate.main(["--repo", str(REPO), "--data", "--addon", calendars.own(year), str(path)]) == 0,
          f"{year}: content.json validates against the registry schemas")
    cal = content["career.calendar"][calendars.key(year)]
    rounds = calendars.YEARS[str(year)]
    check([r["round"] for r in cal["rounds"]] == [r["round"] for r in rounds], f"{year}: {len(rounds)} rounds in order")
    for mine, real in zip(cal["rounds"], rounds):
        venue = calendars.VENUES[real["circuit"]]
        want = f"venue/{real['circuit']}" + ("" if venue["stock"] else "?")
        if mine["venue"] != want or mine["date"] != real["date"] or mine["race"] != {"literal": real["race"]}:
            check(False, f"{year} round {real['round']}: {mine} is not {real['race']} at {want} on {real['date']}")
    optional = [r["venue"] for r in cal["rounds"] if r["venue"].endswith("?")]
    print(f"      {year}: optional {', '.join(optional) or 'none'}")
    package = content["content.package"][calendars.key(year)]
    check(package.get("calendar") == f"{calendars.own(year)}/{calendars.key(year)}?",
          f"{year}: the package names its calendar, optionally")


def main():
    venues()
    years = calendars.seasons()
    check(years == list(range(2016, 2025)), f"seasons {years[0]} to {years[-1]}")
    for year in years:
        season(year)
    print(f"calendars: {failures} failed")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
