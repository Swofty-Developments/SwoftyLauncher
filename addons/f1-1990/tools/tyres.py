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
falls: 1 - (1 - y) x fade), heat (WorkEnergyRate and the pressure WorkEnergyRateMin/Max, the warm-up), spin
(SpinSkiddRate) and hot (the grip lost on the overheating side of TemperatureGrip, from 373 K, full from 403 K). The
race tyres give wear, heat and spin per axle (F, R). The other
codes, the intermediate and the wet keep the stock values.

The stock 2026 compounds share one Peak, so grip sets the gap between them. The qualifiers set the car's qualifying pace
(Goodyear 0.97, Pirelli 0.99: Las Vegas 1:40.0-1:42.4 in the AI build's car model); the race tyres sit at 0.895 to 0.90 (Goodyear)
and 0.91 to 0.915 (Pirelli), which with the race engine (native quali_engine.cpp) puts the best race lap 4-6.2 % over the
qualifying lap in the AI build's car model, the 1990 gap (pole to race fastest lap: Monza +4.5 %, Monaco +3.9 %, Hungary
+5.3 %, Suzuka +7.5 %).

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
    "uso": {"name": "Goodyear qualifier", "grip": 0.97, "wear": 6.0, "fade": 2.0, "heat": 1.2},
    "sup": {"name": "Goodyear D", "grip": 0.90, "wear": {"F": 0.30, "R": 0.22}, "fade": 0.30,
            "heat": {"F": 0.95, "R": 0.80}, "spin": {"F": 0.8, "R": 0.6}, "hot": 0.5},
    "sof": {"name": "Goodyear C", "grip": 0.895, "wear": {"F": 0.22, "R": 0.17}, "fade": 0.28,
            "heat": {"F": 0.95, "R": 0.80}, "spin": {"F": 0.8, "R": 0.6}, "hot": 0.45},
}
PIRELLI = {
    "uso": {"name": "Pirelli qualifier", "grip": 0.99, "wear": 7.0, "fade": 2.4, "heat": 1.3},
    "sup": {"name": "Pirelli soft", "grip": 0.915, "wear": {"F": 0.35, "R": 0.26}, "fade": 0.38,
            "heat": {"F": 1.05, "R": 0.88}, "spin": {"F": 0.8, "R": 0.6}, "hot": 0.65},
    "sof": {"name": "Pirelli medium", "grip": 0.91, "wear": {"F": 0.26, "R": 0.20}, "fade": 0.34,
            "heat": {"F": 1.05, "R": 0.88}, "spin": {"F": 0.8, "R": 0.6}, "hot": 0.6},
}
HOT_FROM = 373.15
HOT_FULL = 403.15
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


def per_end(v, end):
    return v[end] if isinstance(v, dict) else v


def spin(text, k):
    return scale_values(text, (b'SpinSkiddRate',), k)


def hot_side(text, k):
    def temperature(m):
        def one(p):
            x = float(p.group(1))
            if x <= HOT_FROM:
                return p.group(0)
            ramp = min(1.0, (x - HOT_FROM) / (HOT_FULL - HOT_FROM))
            keep = 1 - ramp * (1 - k)
            y = 1 - (1 - float(p.group(2))) * keep
            return f'<SplineElement x="{p.group(1)}" y="{y:.6g}"'.encode()
        return re.sub(rb'<SplineElement x="([-0-9.eE]+)" y="([-0-9.eE]+)"', one, m.group(0))
    return re.sub(rb'<TemperatureGrip .*?</TemperatureGrip>', temperature, text, flags=re.S)


def compound(block, code, end, group, physics):
    out = block.replace(f'name="{STOCK_GROUP}_{code.upper()}{end}"'.encode(),
                        f'name="{group}_{code.upper()}{end}"'.encode(), 1)
    p = physics.get(code)
    if p is None:
        return out
    out = re.sub(rb'<!--[^\n]*?-->', f'<!-- {p["name"]} {"front" if end == "F" else "rear"} -->'.encode(), out, count=1)
    out = scale_attrs(out, (b'Peak', b'PeakMin', b'PeakMax'), p["grip"])
    out = scale_values(out, (b'WearRate', b'WearRateMin', b'WearRateMax'), per_end(p["wear"], end))
    out = scale_values(out, (b'WorkEnergyRate', b'WorkEnergyRateMin', b'WorkEnergyRateMax'), per_end(p["heat"], end))
    if "spin" in p:
        out = spin(out, per_end(p["spin"], end))
    if "hot" in p:
        out = hot_side(out, p["hot"])
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
