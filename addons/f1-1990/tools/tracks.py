#!/usr/bin/env python3
"""Writes erp/tracks.json and erp/tracks/: a 1990 world per track, the My Team world with its own copies of the layers
Custom Grid Size widens, widened only when `cars` is 26.

usage: tracks.py --game <F1 25 folder> [--baseline <launcher baseline>/<build>]
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

from pkg import Pkg, text
from stock import Stock, fragments

CGS = REPO / "addons/custom-grid-size/tracks"
WIDE = {"cars": 26}
OUT = ROOT / "erp/tracks"
TRACK_LAYER = "eaid://environment_package/tracks/[FILENAME]/wep/[FILENAME]_f1_1990.wep"


def world_name(track):
    return f"eaid://environment_package/tracks/{track}/wep/{track}_f1_1990.wep"


def copy_name(layer):
    return layer[:-len(".layer")] + "_f1990.layer"


def edit_world(raw, track, layers):
    pkg = Pkg(raw)
    if pkg.to_bytes() != raw:
        sys.exit(f"{track}: the stock world does not round-trip")
    root = pkg.root
    name = next(v for k, v in root.items if k.value == "name")
    name.value = f"{track}_f1_1990"
    found = set()
    for v in next(v for k, v in root.items if k.value == "layers").items:
        if v.value in layers:
            found.add(v.value)
            v.value = copy_name(v.value)
    if found != layers:
        sys.exit(f"{track}: the world does not list {sorted(layers - found)}")
    out = pkg.to_bytes()
    edited = text(Pkg(out).root)
    expect = text(Pkg(raw).root)
    expect = [line.replace(f'"{track}_f1_26_myteam"', f'"{track}_f1_1990"') for line in expect]
    for layer in layers:
        expect = [line.replace(f'"{layer}"', f'"{copy_name(layer)}"') for line in expect]
    if edited != expect:
        sys.exit(f"{track}: the edited world differs from the stock one beyond the name and the layers")
    return out


def build(stock):
    ops, report = [], []
    OUT.mkdir(parents=True, exist_ok=True)
    for f in sorted(CGS.glob("*.json")):
        track = f.stem
        wide = [o for o in json.loads(f.read_text())["ops"] if o["op"] == "patch-pkg" and o.get("when") == WIDE]
        erps = {o["erp"] for o in wide}
        if len(erps) != 1:
            sys.exit(f"{track}: Custom Grid Size edits {sorted(erps)}")
        erp = erps.pop()
        layers = {o["resource"] for o in wide}
        stock_world = f"eaid://environment_package/tracks/{track}/wep/{track}_f1_26_myteam.wep"
        raw = fragments(stock.path(erp), stock_world)[0]
        source = OUT / f"{track}_f1_1990.world"
        source.write_bytes(edit_world(raw, track, layers))
        ops.append({"op": "add-resource", "erp": erp, "resource": world_name(track), "type": "World",
                    "sources": [str(source.relative_to(ROOT))]})
        for layer in sorted(layers):
            ops.append({"op": "copy-resource", "from_erp": erp, "erp": erp, "resource": layer,
                        "as": copy_name(layer)})
        for o in wide:
            o = copy.deepcopy(o)
            o["resource"] = copy_name(o["resource"])
            ops.append(o)
        report.append(f"{track}: {len(layers)} layers, {sum(len(o['edits']) for o in wide)} edits at 26 cars")
    (ROOT / "erp/tracks.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    return report


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path)
    a = ap.parse_args(argv[1:])
    for line in build(Stock(a.game, a.baseline)):
        print(line)


if __name__ == "__main__":
    main(sys.argv)
