#!/usr/bin/env python3
"""Writes db/box.json and lang/box.json: the F1 1990 box of the Grand Prix and Time Trial era picker.

usage: box.py [--check]
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GRID = ROOT / "db/grid.json"
OUT = ROOT / "db/box.json"
LANG = ROOT / "lang/box.json"

HOST = "swofty_f1"
SOURCE = "f1_1990"
GP_LIST = "f1_1990_gp"
TT_LIST = "f1_1990_tt"
TITLE = "lng_f1990_box"
TT_CARS = 13
HOST_MAX_ENTRANTS = "26"
SCENARIO_IDS = {GP_LIST: "90", TT_LIST: "91"}
LIST_SET = {"display_name": TITLE, "leaderboard": "classic"}
LIST_ROW = re.compile(r"^entrants\.entrant_lists\.entrant_list\[_metadata_=" + SOURCE + r"\](\.entrant_(\d+))?$")


def source_ops():
    ops = json.loads(GRID.read_text())["ops"]
    out = []
    for op in ops:
        if op.get("table") == "entrants.entrant_lists" and op["set"].get("_metadata_") == SOURCE:
            out.append(("list", op))
            continue
        target = op.get("select") or op.get("table") or ""
        if LIST_ROW.match(target):
            out.append(("entry", op))
    return out


def rename(op, target_list):
    op = json.loads(json.dumps(op))
    for key in ("select", "table", "from"):
        if key in op:
            op[key] = op[key].replace(f"[_metadata_={SOURCE}]", f"[_metadata_={target_list}]")
    return op


def list_row(src, name):
    row = {k: v for k, v in src["set"].items() if k != "stat_id"}
    row.update({"_metadata_": name, **LIST_SET})
    return {"op": "add-row", "table": "entrants.entrant_lists", "from": src["from"], "set": row}


def gp_list(src):
    ops = []
    for kind, op in src:
        ops.append(list_row(op, GP_LIST) if kind == "list" else rename(op, GP_LIST))
    return ops


def tt_list(src):
    head = next(op for kind, op in src if kind == "list")
    entries = {}
    for kind, op in src:
        if kind != "entry" or op["op"] != "set" or "when" in op:
            continue
        m = LIST_ROW.match(op["select"])
        if m and m.group(2) is not None:
            entries.setdefault(int(m.group(2)), {})[op["field"]] = op["value"]
    leaders = [e for _, e in sorted(entries.items()) if e.get("team_position") == "0"]
    for kind, op in src:
        if kind == "entry" and op["op"] == "add-row" and op["set"].get("team_position") == "0":
            leaders.append(dict(op["set"]))
    leaders = leaders[:TT_CARS]
    if len(leaders) != TT_CARS:
        sys.exit(f"box.py: {len(leaders)} team leaders in {GRID.name}, expected {TT_CARS}")
    ops = [list_row(head, TT_LIST)]
    sel = f"entrants.entrant_lists.entrant_list[_metadata_={TT_LIST}]"
    base = len(entries)
    for i, e in enumerate(leaders):
        fields = dict(e)
        if i < base:
            for field, value in fields.items():
                ops.append({"op": "set", "select": f"{sel}.entrant_{i}", "field": field, "value": value})
        else:
            ops.append({"op": "add-row", "table": sel, "from": f"{sel}.entrant_0", "set": fields})
    ops.append({"op": "set", "select": sel, "field": "_entrant_instancecount_", "value": str(TT_CARS)})
    return ops


def host_ops(name):
    mode = f"entrants.entrant_setup_modes.entrant_setup_mode[_metadata_={HOST}]"
    ops = [{"op": "list-append", "select": mode, "field": field, "value": name}
           for field in ("entrant_lists", "selectable_entrant_lists")]
    ops += [{"op": "list-append", "select": f"content_sets.content_set[_metadata_={year}].available_entrant_lists_0",
             "field": "entrant_lists", "value": name} for year in ("2025", "2026")]
    ops.append({"op": "add-row", "table": "game_setup.scenarios", "from": "game_setup.scenarios.scenario[_metadata_=f1]",
                "set": {"_metadata_": name, "display_name": TITLE, "entrant_list": name, "game_setup_mode": HOST,
                        "statId": SCENARIO_IDS[name]}})
    return ops


def build():
    src = source_ops()
    ops = gp_list(src) + tt_list(src) + host_ops(GP_LIST) + host_ops(TT_LIST)
    for row in ("track_data.track_layers.TrackLayer", "track_data.game_setup.TrackDataSet", "tyres.compound_classes.CompoundClass"):
        ops.append({"op": "list-append", "select": f"{row}[_metadata_={SOURCE}]", "field": "game_setup_modes", "value": HOST})
    ops.append({"op": "set", "select": f"entrants.entrant_setup_modes.entrant_setup_mode[_metadata_={HOST}]",
                "field": "max_entrants", "value": HOST_MAX_ENTRANTS})
    ops += [
        {"op": "list-append", "select": "content_sets.content_set[_metadata_=2026]",
         "field": "available_game_setup_modes_0.setup_modes", "value": HOST},
        {"op": "list-append", "select": "entrants.drivers.seasonal_infos.seasonal_info[_metadata_=lando_norris]",
         "field": "game_setup_modes", "value": HOST},
    ]
    db = {"schema_version": 1, "ops": ops}
    lang = {"schema_version": 1, "ops": [{"op": "lang.set", "language": "*", "key": TITLE, "value": "F1 1990"}]}
    return db, lang


def dump(obj):
    return json.dumps(obj, indent=1, ensure_ascii=False) + "\n"


def main():
    db, lang = build()
    if "--check" in sys.argv[1:]:
        stale = [p.name for p, o in ((OUT, db), (LANG, lang)) if not p.exists() or p.read_text() != dump(o)]
        if stale:
            sys.exit("box.py: stale " + ", ".join(stale))
        return
    OUT.write_text(dump(db))
    LANG.write_text(dump(lang))
    print(f"box.py: {len(db['ops'])} ops in {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
