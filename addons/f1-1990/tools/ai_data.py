#!/usr/bin/env python3
"""Writes db/ai_difficulty.json and carpack/db/ai.json: the AI difficulty and AI data folder of the 1990 cars.

usage: ai_data.py [stock gamedatabases.2025.bin] [--laps <laps.json>] [--check]

The game builds a car's AI archive path from the AI asset rows' `track_data_file_path`
(`environment_package/tracks/<track_name>/ai/<vehicle_package>/<team_name>/`), with `<vehicle_package>` the
vehicle package row's `ai_package_folder` and `<team_name>` the vehicle row's `asset_filename`. On every track load
it reads `tracks.Track_N.ai_track_difficulties.<ai_package_folder>.{difficulty_ratio,mass_ratio,time,
max_dificulty_no_balast_slider_position,max_slider_position}` for each available vehicle package and keeps the
package only when all five keys exist.

db/ai_difficulty.json (this addon) gives each of the 16 calendar tracks those five keys under
`f1_1990_vehicle_package`: the f1_2026_vehicle_package values, with `time` times the track's 1990/2026 lap ratio. They
are new keys that only a package whose folder is f1_1990_vehicle_package reads.

The game finds a car's AI asset row in the table `ai.asset_data.<ai_package_folder>` by the vehicle's folder; the row
names the path templates and a `base_car` (`fom_car` in every 2026 row). carpack/db/ai.json adds that table for
`f1_1990_vehicle_package` (the 13 folders, copies of the 2026 rows with `base_car` `<team_name>`, a value other stock
packages use, so no path leads to a fom_car folder the 1990 AI build lacks), points the f1_1990 and f1_1990b packages
at `f1_1990_vehicle_package` and makes the f1_1990 package available, next to the 1990 AI archives that
carpack/assets/ai.json installs.

The lap ratio of a track is `tracks.<track>.summary.median_ratio` of the car pack's AI build (`carpack/erp/ai/laps.json`,
committed here as data/ai-laps-1990.json): the median over the 13 folders of the simulated 1990 lap over the stock
2026 lap. A track without one gets PLACEHOLDER_RATIO and the op file carries a warn op. --check compares with the shipped files and
exits 1 on any difference.
"""

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ADDON = HERE.parent
REPO = ADDON.parent.parent
CAR_PACK = ADDON / "carpack"
sys.path.insert(0, str(REPO / "tools"))
sys.path.insert(0, str(HERE))

import gamedb  # noqa: E402
import cars as C  # noqa: E402
from career import ROUNDS  # noqa: E402

STOCK_DB = "/mnt/work/.f1-1990/data-physics/fakegame-full/game_package/gamedatabases/gamedatabases.2025.bin"
LAPS = [ADDON / "data" / "ai-laps-1990.json", CAR_PACK / "erp" / "ai" / "laps.json"]
OUT = ADDON / "db" / "ai_difficulty.json"
SWITCH_OUT = CAR_PACK / "db" / "ai.json"

SOURCE = C.SOURCE_PACKAGE
TARGET = C.AI_PACKAGE_FOLDER
FIELDS = ("time", "difficulty_ratio", "mass_ratio", "max_dificulty_no_balast_slider_position", "max_slider_position")
SCALED = ("time",)
PLACEHOLDER_RATIO = 1.0
TRACKS = [r[1] for r in ROUNDS]
PACKAGES = "entrants.vehicles.vehicle_packages.vehicle_package"
ASSET_DATA = "ai.asset_data"
ASSET_FIELDS = ["_metadata_", "base_car", "brake_line_settings_file_name", "track_data_file_name",
                "track_data_file_path", "vehicle_performance_file_name", "vehicle_performance_file_path"]
BASE_CAR = "<team_name>"


def track_select(track):
    return f"tracks.Track[_metadata_={track}]"


def key(track_index, package, field):
    return f"tracks.Track_{track_index}.ai_track_difficulties.{package}.{field}"


class Stock:
    def __init__(self, path):
        self.d = {r.key: (r.type, r.value.decode("utf-8")) for r in gamedb.read(Path(path).read_bytes())}

    def get(self, path):
        return self.d.get(gamedb.fnv1a64(path.encode()))

    def track_index(self, track):
        n = int(self.get("tracks._Track_instancecount_")[1])
        for i in range(n):
            v = self.get(f"tracks.Track_{i}._metadata_")
            if v and v[1] == track:
                return i
        raise KeyError(track)


def ratio_of(entry):
    if isinstance(entry, (int, float)):
        return float(entry)
    if isinstance(entry, dict):
        if "summary" in entry:
            return ratio_of(entry["summary"])
        if "median_ratio" in entry:
            return float(entry["median_ratio"])
        if "ratio" in entry:
            return float(entry["ratio"])
        new = [v for k, v in entry.items() if "1990" in k and isinstance(v, (int, float))]
        old = [v for k, v in entry.items() if "2026" in k and isinstance(v, (int, float))]
        if len(new) == 1 and len(old) == 1 and old[0]:
            return float(new[0]) / float(old[0])
    raise ValueError(f"no lap ratio in {entry!r}")


def load_laps(path):
    if path is None:
        path = next((p for p in LAPS if p.exists()), None)
    if path is None:
        return {}, None
    doc = json.loads(Path(path).read_text())
    table = doc.get("tracks", doc) if isinstance(doc, dict) else {}
    if isinstance(table, list):
        table = {e["track"]: e for e in table}
    return {t: ratio_of(e) for t, e in table.items() if t in TRACKS}, Path(path)


def fmt(v):
    return f"{v:.6g}"


def scale(value, ratio):
    return ",".join(fmt(float(x) * ratio) for x in value.split(","))


def difficulty(stock, ratios):
    rows = []
    for track in TRACKS:
        i = stock.track_index(track)
        ratio = ratios.get(track, PLACEHOLDER_RATIO)
        for field in FIELDS:
            tag, value = stock.get(key(i, SOURCE, field))
            rows.append((track, field, tag, scale(value, ratio) if field in SCALED else value, ratio))
    return rows


def difficulty_ops(stock, ratios, source):
    ops = []
    missing = [t for t in TRACKS if t not in ratios]
    if missing:
        where = f" ({source.name} lacks them)" if source else ""
        ops.append({"op": "warn", "message": f"F1 1990 AI difficulty: placeholder lap ratio {PLACEHOLDER_RATIO} on "
                                              f"{', '.join(missing)}{where}"})
    for track, field, tag, value, _ in difficulty(stock, ratios):
        ops.append({"op": "set", "select": track_select(track), "field": f"ai_track_difficulties.{TARGET}.{field}",
                    "value": value, "create": True, "type": tag})
    return {"schema_version": 1, "ops": ops}


def asset_rows():
    return sorted({c.folder for c in C.CARS})


def asset_row_index(stock, folder):
    n = int(stock.get(f"{ASSET_DATA}.{SOURCE}._asset_data_instancecount_")[1])
    for i in range(n):
        v = stock.get(f"{ASSET_DATA}.{SOURCE}.asset_data_{i}._metadata_")
        if v and v[1] == folder:
            return i
    raise KeyError(folder)


def put(key, tag, value):
    return {"op": "put-record", "key": key, "type": tag, "value": value.encode().hex()}


def switch_ops(stock):
    count = stock.get(f"{ASSET_DATA}.{SOURCE}._asset_data_instancecount_")
    ops = [put(f"{ASSET_DATA}.{TARGET}._asset_data_instancecount_", count[0], str(len(asset_rows())))]
    for n, folder in enumerate(asset_rows()):
        src = f"{ASSET_DATA}.{SOURCE}.asset_data_{asset_row_index(stock, folder)}"
        for field in ASSET_FIELDS:
            tag, value = stock.get(f"{src}.{field}")
            ops.append(put(f"{ASSET_DATA}.{TARGET}.asset_data_{n}.{field}", tag,
                           BASE_CAR if field == "base_car" else value))
    ops += [{"op": "set", "select": f"{PACKAGES}[_metadata_={name}]", "field": "ai_package_folder", "value": TARGET}
            for name in (C.PACKAGE, C.PACKAGE_B)]
    ops.append({"op": "set", "select": f"{PACKAGES}[_metadata_={C.PACKAGE}]", "field": "available", "value": "True"})
    return {"schema_version": 1, "ops": ops}


def dump(doc):
    return json.dumps(doc, indent=1, ensure_ascii=False) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("stock", nargs="?", default=STOCK_DB)
    ap.add_argument("--laps", type=Path, help="lap ratio file (default: data/ai-laps-1990.json, then the car pack's)")
    ap.add_argument("--check", action="store_true", help="compare with the shipped files instead of writing them")
    ap.add_argument("-v", action="store_true", help="print the 1990 and 2026 values per track")
    args = ap.parse_args()
    stock = Stock(args.stock)
    ratios, source = load_laps(args.laps)
    files = {OUT: dump(difficulty_ops(stock, ratios, source)), SWITCH_OUT: dump(switch_ops(stock))}
    print(f"lap ratios: {source or 'none'}; placeholder on {len([t for t in TRACKS if t not in ratios])} of "
          f"{len(TRACKS)} tracks")
    if args.v:
        for track, field, tag, value, ratio in difficulty(stock, ratios):
            if field == "time":
                old = stock.get(key(stock.track_index(track), SOURCE, field))[1]
                print(f"{track:18} x{ratio:.4f}  2026 {old}\n{'':26} 1990 {value}")
    if args.check:
        bad = [p for p, text in files.items() if not p.exists() or p.read_text() != text]
        for p in bad:
            print(f"differs: {p.relative_to(REPO)}")
        return 1 if bad else 0
    for p, text in files.items():
        p.write_text(text)
        print(f"wrote {p.relative_to(REPO)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
