#!/usr/bin/env python3
"""Writes erp/wizard.json: the routes the 1990 Driver Career wizard fires in place of stock ones.

usage: wizard.py --game <F1 25 folder> [--baseline <launcher baseline>/<build>]

Every route is a copy of a stock route in the same grouping with some steps pointed elsewhere; no stock route changes.
Only f1_1990_wizard.dll fires the copies, and only for the 1990 wizard.

- fe_career_dlc_select / career_wizard_driver_career_1990 (from career_wizard_driver_career_to_f1): the 1990 tile
  opens the settings step instead of the season length step, as fe_home's career_wizard_driver_career does.
- driver_career_driver_select / screen_group_back_1990 (from screen_group_back): Choose Driver's back runs the
  header's back_1990 instead of its back.
- driver_career_driver_select_header / back_1990 (from back_1): closes the driver select screen group and opens the
  settings step again (back_1 opens the driver type step, which the 1990 wizard skips), recorded as opened from the
  Career tiles like the first time.
- career_wizard_settings_select / back_1990 (from back_8, which goes to the home screen): returns to the Career tiles,
  the same steps as the season length step's back_4 with the settings screen transitioned out.
"""
import argparse
import copy
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools")]

from pkg import Arr, Obj, Pkg, Val  # noqa: E402
from stock import Stock, fragments  # noqa: E402

ERP = "2025_asset_groups/ui_package/persistent.erp"
FRONTEND = "eaid://ui_package/routes/routes_frontend_general.uir"
CUSTOMISATION = "eaid://ui_package/routes/routes_career_customisation.uir"
WIZARD = "eaid://ui_package/routes/routes_career_wizard.uir"
TILES = "fe_career_dlc_select"
SEASON = "career_wizard_season_length_select"
SETTINGS = "career_wizard_settings_select"
DRIVER_SELECT = "driver_career_driver_select"
HEADER = "driver_career_driver_select_header"
DRIVER_TYPE = "driver_career_driver_type_select"
CAREER = "fe_career"
HOME = "fe_home"

# (resource, grouping, source route, new route, {path: (stock value, new value)})
ROUTES = [
    (FRONTEND, TILES, "career_wizard_driver_career_to_f1", "career_wizard_driver_career_1990", {
        "Commands[Dependencies=3].Attributes[Id=To].Value": (SEASON, SETTINGS),
        "Commands[Dependencies=6].Attributes[Id=Target].Value": (SEASON, SETTINGS),
    }),
    (CUSTOMISATION, DRIVER_SELECT, "screen_group_back", "screen_group_back_1990", {
        "Commands[0].Attributes[Id=SubRoute].Value": ("back", "back_1990"),
    }),
    (CUSTOMISATION, HEADER, "back_1", "back_1990", {
        "Attributes[Id=Destination].Value": (DRIVER_TYPE, SETTINGS),
        "Commands[0].Attributes[Id=To].Value": (DRIVER_TYPE, SETTINGS),
        "Commands[1].Attributes[Id=Target].Value": (DRIVER_TYPE, SETTINGS),
        "Commands[1].Attributes[Id=From].Value": (HEADER, TILES),
    }),
    (WIZARD, SETTINGS, "back_8", "back_1990", {
        "Attributes[Id=Destination].Value": (HOME, CAREER),
        "Commands[Type=Migrate].Attributes[Id=Activator].Value": ("enter_fe_home", "enter_fe_career"),
        "Commands[Type=FrontendSceneSwitch].Attributes[Id=Target].Value": (HOME, CAREER),
        "Commands[Dependencies=10,12].Attributes[Id=Target].Value": (HOME, CAREER),
    }),
]
# The settings step's back_1990 must end up as the season length step's back_4 with its own screen transitioned out.
SAME_AS = {(WIZARD, SETTINGS, "back_1990"): (SEASON, "back_4", {"Commands[0].Attributes[Id=Target].Value": SETTINGS})}


def flat(o):
    if isinstance(o, Val):
        return o.value
    if isinstance(o, Obj):
        return {k.value: flat(v) for k, v in o.items}
    if isinstance(o, Arr):
        return [flat(x) for x in o.items]
    raise SystemExit(f"unexpected node {o!r}")


def steps(path):
    return [p for p in path.replace("[", ".[").split(".") if p]


def walk(node, parts):
    for step in parts:
        if step.startswith("["):
            sel = step[1:-1]
            if "=" in sel:
                k, v = sel.split("=", 1)
                node = next(x for x in node if isinstance(x, dict) and str(x.get(k)) == v)
            else:
                node = node[int(sel)]
        else:
            node = node[step]
    return node


def get(node, path):
    return walk(node, steps(path))


def put(node, path, value):
    parts = steps(path)
    walk(node, parts[:-1])[parts[-1]] = value


def groupings(stock, resource):
    doc = flat(Pkg(fragments(stock.path(ERP), resource)[0]).root)
    return {g["Id"]: {r["Id"]: r for r in g["Routes"]} for g in doc["RouteGroupings"]}


def clone(doc, grouping, source, name, edits):
    routes = doc.get(grouping)
    if routes is None:
        raise SystemExit(f"no grouping {grouping}")
    if name in routes:
        raise SystemExit(f"{grouping}: the stock grouping already has {name}")
    if source not in routes:
        raise SystemExit(f"{grouping}: no route {source}")
    made = copy.deepcopy(routes[source])
    made["Id"] = name
    for path, (was, now) in edits.items():
        try:
            got = get(made, path)
        except (KeyError, IndexError, StopIteration):
            raise SystemExit(f"{grouping}/{source}: no {path}")
        if got != was:
            raise SystemExit(f"{grouping}/{source}: {path} is {got!r}, want {was!r}")
        put(made, path, now)
    return made


def build(stock):
    docs, ops = {}, {}
    for resource, grouping, source, name, edits in ROUTES:
        doc = docs.setdefault(resource, groupings(stock, resource))
        made = clone(doc, grouping, source, name, edits)
        ref = SAME_AS.get((resource, grouping, name))
        if ref:
            other, ref_route, overrides = ref
            want = copy.deepcopy(doc[other][ref_route])
            want["Id"] = name
            for path, value in overrides.items():
                put(want, path, value)
            if made != want:
                raise SystemExit(f"{grouping}/{name} differs from {other}/{ref_route}")
        ops.setdefault(resource, []).append({
            "op": "clone-entry", "at": f"RouteGroupings[Id={grouping}].Routes", "src": f"[Id={source}]", "as": name,
            "set": {path: now for path, (_, now) in edits.items()},
        })
    return {"schema_version": 1, "ops": [
        {"op": "patch-pkg", "erp": ERP, "resource": resource, "fragment": 0, "edits": edits}
        for resource, edits in ops.items()
    ]}


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path)
    a = ap.parse_args(argv[1:])
    out = ROOT / "erp/wizard.json"
    with open(out, "w", encoding="utf-8") as f:
        json.dump(build(Stock(a.game, a.baseline)), f, indent=1)
        f.write("\n")
    print(f"wrote {out}")


if __name__ == "__main__":
    main(sys.argv)
