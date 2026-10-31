#!/usr/bin/env python3
"""Each season's real calendar as a career.calendar record, from Progressive's data/rounds.json.

A season addon registers its calendar as career.calendar f1_<year> in registry/content.json, one round per race of
that year in round order: the race's name as a literal, its date, and its venue, venue/<Jolpica circuit id> from
packages/sdk/std/data/venues.json. A venue a stock track provides (its "stock" is set) is required; any other venue is
optional ("venue/sepang?"), so the round is dropped while no enabled track provides it. The season's package names the
calendar optionally (content.package calendar "<addon>/f1_<year>?"), so a calendar that doesn't resolve leaves the
package as it is.

gen.py adds the calendars when it builds the seasons. Run on its own, this updates each season's registry/content.json
in place (--write) or reports a season whose file differs from what it would write (--check).
"""

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = ROOT.parents[2]
ROUNDS = REPO / "addons" / "progressive" / "data" / "rounds.json"
VENUES_FILE = REPO / "packages" / "sdk" / "std" / "data" / "venues.json"
VENUES = json.loads(VENUES_FILE.read_text())["venues"]
YEARS = json.loads(ROUNDS.read_text())["years"]


def own(year):
    return f"swofty.classic-seasons-{year}"


def key(year):
    return f"f1_{year}"


def venue_ref(circuit):
    venue = VENUES.get(circuit)
    if venue is None:
        raise SystemExit(f"{VENUES_FILE.relative_to(REPO)} has no venue {circuit!r}")
    return f"venue/{circuit}" if venue["stock"] else f"venue/{circuit}?"


def record(year):
    rounds = YEARS.get(str(year))
    if not rounds:
        raise SystemExit(f"{ROUNDS.relative_to(REPO)} has no {year} calendar")
    return {"rounds": [{"round": r["round"], "race": {"literal": r["race"]}, "venue": venue_ref(r["circuit"]),
                        "date": r["date"]} for r in rounds]}


def package_ref(year):
    return f"{own(year)}/{key(year)}?"


def with_calendar(records, year):
    out = {}
    for name, body in records.items():
        if name == "content.package":
            out["career.calendar"] = {key(year): record(year)}
            body = {k: with_package_ref(v, year) if k == key(year) else v for k, v in body.items()}
        if name != "career.calendar":
            out[name] = body
    return out


def with_package_ref(package, year):
    out = {}
    for field, value in package.items():
        if field == "calendar":
            continue
        out[field] = value
        if field == "season":
            out["calendar"] = package_ref(year)
    return out


def seasons():
    return sorted(int(p.name.rsplit("-", 1)[1]) for p in ROOT.parent.glob("classic-seasons-[0-9][0-9][0-9][0-9]")
                  if (p / "registry" / "content.json").is_file())


def content_file(year):
    return ROOT.parent / f"classic-seasons-{year}" / "registry" / "content.json"


def render(data):
    return json.dumps(data, indent=1, ensure_ascii=False) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    how = ap.add_mutually_exclusive_group(required=True)
    how.add_argument("--write", action="store_true", help="update each season's registry/content.json in place")
    how.add_argument("--check", action="store_true", help="fail when a season's file differs from what --write writes")
    args = ap.parse_args(argv)
    stale = []
    for year in seasons():
        path = content_file(year)
        text = path.read_text()
        want = render(with_calendar(json.loads(text), year))
        rounds = len(record(year)["rounds"])
        if text == want:
            print(f"{year}: calendar {own(year)}/{key(year)}, {rounds} rounds")
            continue
        if args.write:
            path.write_text(want)
            print(f"{year}: wrote calendar {own(year)}/{key(year)}, {rounds} rounds")
        else:
            print(f"{year}: {path.relative_to(REPO)} is not what calendars.py writes")
            stale.append(year)
    return 1 if stale else 0


if __name__ == "__main__":
    sys.exit(main())
