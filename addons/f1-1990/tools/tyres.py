#!/usr/bin/env python3
"""Writes tyres/<supplier>/tyrecompounds.erp and tyres/compounds.json: the 1990 Goodyear and Pirelli compounds.

The game names a wheel's compound <TyrePhysicsGroup>_<physics code><F|R>. The Goodyear teams' handling files carry
the group F1_1990 and the Pirelli teams' (Tyrrell, Minardi, Brabham, Dallara) F1_1990P (build_cars.py). Each archive
holds one compound file, eaid://f1_1990_vehicle_package/tyrecompounds/f1_1990_compounds.vcmpd, with the 16 stock 2026
compounds twice: once as F1_1990_* and once as F1_1990P_*. The option tyre_supplier picks the archive:

  historical  F1_1990 = Goodyear, F1_1990P = Pirelli
  goodyear    both groups Goodyear
  pirelli     both groups Pirelli

The f1_1990 compound class runs three dry physics codes: uso (C5, the qualifier, badge US), sup (C4, the soft race
tyre, badge SS) and sof (C3, the race tyre, badge S). Each supplier scales the stock compound of that code: grip (Peak,
PeakMin, PeakMax of every load row), wear (WearRate and the pressure WearRateMin/Max), fade (how far the WearGrip spline
falls: 1 - (1 - y) x fade) and heat (WorkEnergyRate and the pressure WorkEnergyRateMin/Max, the warm-up). The other
codes, the intermediate and the wet keep the stock values.

usage: tyres.py --game <pristine F1 25> [--check]
"""
import argparse
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ADDON = HERE.parent
sys.path[:0] = [str(HERE), str(ADDON.parents[1] / "tools")]

import cars as C  # noqa: E402
import erpw  # noqa: E402

SOURCE = "2025_asset_groups/f1_2026_vehicle_package/tyrecompounds/tyrecompounds.erp"
STOCK_FILE = "eaid://f1_2026_vehicle_package/tyrecompounds/f1_2026_compounds.vcmpd"
FILE = f"eaid://{C.PACKAGE_FOLDER}/tyrecompounds/f1_1990_compounds.vcmpd"
STOCK_GROUP = "F1_2026"
GROUPS = {"goodyear": "F1_1990", "pirelli": "F1_1990P"}

GOODYEAR = {
    "uso": {"name": "Goodyear qualifier", "grip": 1.04, "wear": 6.0, "fade": 2.0, "heat": 1.2},
    "sup": {"name": "Goodyear D", "grip": 1.0, "wear": 0.9, "fade": 1.0, "heat": 1.0},
    "sof": {"name": "Goodyear C", "grip": 1.0, "wear": 0.85, "fade": 1.0, "heat": 1.0},
}
PIRELLI = {
    "uso": {"name": "Pirelli qualifier", "grip": 1.06, "wear": 7.0, "fade": 2.4, "heat": 1.3},
    "sup": {"name": "Pirelli soft", "grip": 1.015, "wear": 1.3, "fade": 1.35, "heat": 1.1},
    "sof": {"name": "Pirelli medium", "grip": 1.015, "wear": 1.25, "fade": 1.3, "heat": 1.1},
}
PHYSICS = {"goodyear": GOODYEAR, "pirelli": PIRELLI}
VARIANTS = {
    "historical": {"goodyear": "goodyear", "pirelli": "pirelli"},
    "goodyear": {"goodyear": "goodyear", "pirelli": "goodyear"},
    "pirelli": {"goodyear": "pirelli", "pirelli": "pirelli"},
}
MIN_WEAR_GRIP = 0.3

BLOCK = re.compile(rb'(<Compound name="' + STOCK_GROUP.encode() + rb'_(\w{3})([FR])">[^\n]*\n.*?</Compound>\r?\n)',
                   re.S)


def scale_attrs(text, names, k):
    def one(m):
        return m.group(1) + f'{float(m.group(2)) * k:.6g}'.encode() + b'"'
    for name in names:
        text = re.sub(rb'(\b' + name + rb'=")([-0-9.eE]+)"', one, text)
    return text


def scale_values(text, tags, k):
    def one(m):
        return m.group(1) + f'{float(m.group(2)) * k:.6g}'.encode() + m.group(3)
    for tag in tags:
        text = re.sub(rb'(<' + tag + rb' value=")([-0-9.eE]+)(")', one, text)
    return text


def fade(text, k):
    def spline(m):
        def one(p):
            y = max(MIN_WEAR_GRIP, 1 - (1 - float(p.group(2))) * k)
            return p.group(1) + f'{y:.6g}'.encode() + b'"'
        return re.sub(rb'(<SplineElement x="[-0-9.eE]+" y=")([-0-9.eE]+)"', one, m.group(0))
    return re.sub(rb'<WearGrip .*?</WearGrip>', spline, text, flags=re.S)


def compound(block, code, end, group, physics):
    out = block.replace(f'name="{STOCK_GROUP}_{code.upper()}{end}"'.encode(),
                        f'name="{group}_{code.upper()}{end}"'.encode(), 1)
    p = physics.get(code)
    if p is None:
        return out
    out = re.sub(rb'<!--[^\n]*?-->', f'<!-- {p["name"]} {"front" if end == "F" else "rear"} -->'.encode(), out, count=1)
    out = scale_attrs(out, (b'Peak', b'PeakMin', b'PeakMax'), p["grip"])
    out = scale_values(out, (b'WearRate', b'WearRateMin', b'WearRateMax'), p["wear"])
    out = scale_values(out, (b'WorkEnergyRate', b'WorkEnergyRateMin', b'WorkEnergyRateMax'), p["heat"])
    return fade(out, p["fade"]) if p["fade"] != 1.0 else out


def vcmpd(stock, variant):
    blocks = BLOCK.findall(stock)
    if len(blocks) != 16:
        raise SystemExit(f"stock compound file: {len(blocks)} {STOCK_GROUP} compounds, expected 16")
    head = stock[:stock.index(blocks[0][0])]
    tail = stock[stock.index(blocks[-1][0]) + len(blocks[-1][0]):]
    body = b""
    for team_supplier, group in GROUPS.items():
        physics = PHYSICS[VARIANTS[variant][team_supplier]]
        for block, code, end in blocks:
            body += compound(block, code.decode().lower(), end.decode(), group, physics)
    return head + body + tail


def build(game):
    files = {}
    ops = []
    for variant in VARIANTS:
        e = erpw.Erp(str(Path(game) / SOURCE))
        res = [r for r in e.res if r.type.rstrip(b"\0") == b"VCMPD"]
        if len(res) != 1 or res[0].name != STOCK_FILE:
            raise SystemExit(f"{SOURCE}: expected the one stock compound file")
        r = res[0]
        e.set_frag(r, 0, vcmpd(e.frag_data(r, 0), variant))
        r.name = FILE
        rel = f"tyres/{variant}/tyrecompounds.erp"
        out = ADDON / "build" / rel
        out.parent.mkdir(parents=True, exist_ok=True)
        e.to_file(str(out))
        files[rel] = out.read_bytes()
        ops.append({"op": "file.put", "when": {"tyre_supplier": variant}, "source": rel, "target": C.TYRE_ARCHIVE})
    files["tyres/compounds.json"] = (json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n").encode()
    return files


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("--game", required=True, help="pristine F1 25 folder (read only)")
    ap.add_argument("--check", action="store_true", help="compare with the shipped files instead of writing them")
    args = ap.parse_args()
    files = build(args.game)
    differ = [rel for rel, blob in files.items()
              if not (ADDON / rel).exists() or (ADDON / rel).read_bytes() != blob]
    if args.check:
        for rel in differ:
            print(f"differs: {rel}")
        return 1 if differ else 0
    for rel, blob in files.items():
        (ADDON / rel).parent.mkdir(parents=True, exist_ok=True)
        (ADDON / rel).write_bytes(blob)
        print(f"wrote {rel} ({len(blob)} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
