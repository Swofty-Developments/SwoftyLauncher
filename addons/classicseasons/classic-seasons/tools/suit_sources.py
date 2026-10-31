"""Which suit each team-season and each driver of 2016 to 2023 wears.

A real suit is a mod's suit or F1 23's own 2022 and 2023 kit (data/real_suits.json "suits", "mod" "f1_23"; both built
by build_real_suits.py into this addon's suits/) or a
stock F1 25 suit of a season in "stock_seasons" (the team's <year>-<stock team> suit and its drivers'
<year>-<stock team>_<driver> suits). A team-season takes the real team suit of its own season, else the one of the
nearest season with the same team name in data/seasons.json, the earlier season on a tie, else its generated suit
(build_suits.py). Only team suits decide the season. Every driver of the team-season wears that season's kit: his own
suit of that season and team when there is one, else that season's team suit. "suit": {"source": "<label>" |
"generated"} on a team overrides the team's choice.

2022 and 2023 (F1_23_YEARS) choose only among the built F1 23 suits, the other seasons never take one; a 2022 or 2023
look with none keeps its seat's stock kit.

Labels: a mod's team suit is cs<yy>-<stock team> of its own season, a mod's driver suit cs<yy>-<base folder>, both the
length of the base label they replace. A generated suit is cg<yy>-<stock team>, or cg<yy>-<key> padded with "_" to the
base label's length for a team with its own key (2016 Manor: cg16-manor_).
"""

import json
from functools import lru_cache
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = json.loads((ROOT / "data" / "seasons.json").read_text())
REAL = json.loads((ROOT / "data" / "real_suits.json").read_text())
SUIT_YEARS = range(2016, 2022)
F1_23_YEARS = range(2022, 2024)
REAL_YEARS = range(SUIT_YEARS.start, F1_23_YEARS.stop)
F1_23 = "f1_23"
GENERATED = "generated"
ORIGINAL = "original"


def stock_team(team):
    return DATA["folders"][team["folder"]]["team"]


def generated_label(year, team, base):
    key = team.get("key", team["folder"])
    name = f"cg{year % 100:02d}-{stock_team(team) if key == team['folder'] else key}"
    if len(name) > len(base):
        raise SystemExit(f"{year} {key}: label {name} is longer than its base {base}")
    return name.ljust(len(base), "_")


def mod_label(entry):
    base = entry["base"]
    label = f"cs{entry['season'] % 100:02d}" + base[4:]
    if not base.startswith("20") or len(label) != len(base):
        raise SystemExit(f"{base}: no same-length label")
    return label


@lru_cache(None)
def real():
    out = []
    for e in REAL["suits"]:
        if e.get("off"):
            continue
        out.append({**e, "kind": "mod", "label": mod_label(e), "helmet_label": mod_label(e) if e.get("helmet") else None})
    for season in DATA["seasons"]:
        if season["year"] not in REAL["stock_seasons"]:
            continue
        for team in season["teams"]:
            label = f"{season['year']}-{stock_team(team)}"
            out.append({"season": season["year"], "team": team["name"], "kind": "stock", "label": label,
                        "helmet_label": None})
            for driver, _ in team["drivers"]:
                out.append({"season": season["year"], "team": team["name"], "driver": driver, "kind": "stock",
                            "label": f"{label}_{driver}", "helmet_label": None})
    return out


def by_label(label):
    hits = [e for e in real() if e["label"] == label]
    if len(hits) != 1:
        raise SystemExit(f"suit source {label}: {len(hits)} real suits")
    return hits[0]


def built():
    path = ROOT / "suits" / "meta.json"
    if not path.exists():
        return set()
    return {m["label"] for m in json.loads(path.read_text())["suits"] if (ROOT / m["source"]).exists()}


def pick(year, team, driver=None, made=None):
    """made: the labels of the built mod suits (built()); a mod suit that is not built is not a choice."""
    made = built() if made is None else made
    usable = [e for e in real() if e["team"] == team["name"] and (e["kind"] == "stock" or e["label"] in made)
              and (e.get("mod") == F1_23) == (year in F1_23_YEARS)]
    source = team.get("suit", {}).get("source")
    if source in (GENERATED, ORIGINAL):
        return None
    if source:
        chosen = by_label(source)
    else:
        teams = [e for e in usable if "driver" not in e]
        if not teams:
            return None
        chosen = min(teams, key=lambda e: (abs(e["season"] - year), e["season"]))
    if driver is None:
        return chosen
    own = [e for e in usable if e.get("driver") == driver and e["season"] == chosen["season"]]
    return own[0] if own else chosen


def describe(entry):
    if entry is None:
        return GENERATED
    what = REAL["mods"][entry["mod"]]["name"] if entry["kind"] == "mod" else "F1 25"
    return f"{entry['label']} ({what}, {entry['season']})"
