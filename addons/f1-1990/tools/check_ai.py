#!/usr/bin/env python3
"""Checks the 1990 AI wiring: difficulty keys, package folders, stock rows, and the car pack's AI asset ops.

usage: check_ai.py <stock gamedatabases.2025.bin> <built gamedatabases.2025.bin> [--car-pack] [--laps <laps.json>]
                   [--assets <f1-1990-cars/assets/ai.json> [--game <F1 25 folder>]]

Database (the built file against stock):
- every stock key of `tracks.Track_N.ai_track_difficulties.*`, `tracks.Track_N.Tyres_N.track_grip_modifiers_N.*`
  and of the stock vehicle package rows is unchanged;
- each of the 16 calendar tracks has the five `ai_track_difficulties.f1_1990_vehicle_package` keys, with the type of
  the f1_2026_vehicle_package key and the value tools/ai_data.py derives (time scaled by the lap ratio, the rest as in
  2026), and no other track has any;
- the stock `ai.asset_data.<package>` rows are unchanged;
- with --car-pack (a build with swofty.f1-1990-cars), f1_1990 and f1_1990b read `f1_1990_vehicle_package`, f1_1990
  is available and `ai.asset_data.f1_1990_vehicle_package` has a row per car folder (the 2026 row, base_car
  `<team_name>`); without it both read `f1_2026_vehicle_package` and that table is absent.

Asset ops (--assets): one file.put and one group.register per calendar track and car folder, at the paths the game
builds (`asset_groups/environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.assetgroup`,
ERP `2025_<same path>.erp`), each templated on the matching f1_2026_vehicle_package group, and with --game every
template ERP exists in that folder. Exit code 1 on any failure.
"""

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import ai_data  # noqa: E402
import career  # noqa: E402
import cars as C  # noqa: E402

PROTECTED_PREFIXES = ("tracks.Track_N.ai_track_difficulties.", "tracks.Track_N.Tyres_N.track_grip_modifiers_N.")
AI_ROOT = "environment_package/tracks"


def h(path):
    return career.gamedb.fnv1a64(path.encode())


def expand(db, template, n_track):
    out = []
    for i in range(n_track):
        row = template.replace("tracks.Track_N", f"tracks.Track_{i}", 1)
        if "_N" not in row:
            out.append(row)
            continue
        for k in range(4):
            out.append(row.replace("_N", f"_{k}"))
    return out


def protected(stock):
    out = {}
    n_track = stock.count("tracks", "Track")
    templates = [k for k in stock.cat if k.startswith(PROTECTED_PREFIXES)]
    templates += [k.rsplit(".", 1)[0] + ".max_slider_position" for k in templates
                  if k.endswith(".max_dificulty_no_balast_slider_position")]
    for template in templates:
        for path in expand(stock, template, n_track):
            if h(path) in stock.d:
                out[h(path)] = path
    folders = {stock.get(f"entrants.vehicles.vehicle_packages.vehicle_package_{i}.ai_package_folder")
               for i in range(stock.count("entrants.vehicles.vehicle_packages", "vehicle_package"))}
    for pkg in sorted(f for f in folders if f):
        table = f"{ai_data.ASSET_DATA}.{pkg}"
        for i in range(stock.count(table, "asset_data") or 0):
            for field in ai_data.ASSET_FIELDS:
                path = f"{table}.asset_data_{i}.{field}"
                if h(path) in stock.d:
                    out[h(path)] = path
    container = "entrants.vehicles.vehicle_packages"
    for i in range(stock.count(container, "vehicle_package")):
        label = stock.get(f"{container}.vehicle_package_{i}._metadata_")
        for field in stock.fields(container, "vehicle_package", label, probe=False):
            out[h(f"{container}.vehicle_package_{i}.{field}")] = f"{container}.vehicle_package_{i}.{field} ({label})"
    return out


def check_db(stock, source_db, built, car_pack, laps):
    bad = []
    guard = protected(stock)
    for key, path in guard.items():
        if built.d.get(key) != stock.d[key]:
            new = built.d.get(key)
            bad.append(f"stock key changed: {path}: {stock.d[key][1]!r} -> {new[1] if new else 'MISSING'!r}")
    ratios, source = ai_data.load_laps(laps)
    want = {(track, field): (tag, value) for track, field, tag, value, _ in ai_data.difficulty(source_db, ratios)}
    n_track = stock.count("tracks", "Track")
    found = 0
    for i in range(n_track):
        track = stock.get(f"tracks.Track_{i}._metadata_")
        for field in ai_data.FIELDS:
            got = built.d.get(h(ai_data.key(i, ai_data.TARGET, field)))
            if (track, field) in want:
                if got != want[(track, field)]:
                    bad.append(f"{track}.{field}: {got!r}, want {want[(track, field)]!r}")
                else:
                    found += 1
            elif got is not None:
                bad.append(f"{track} (not a calendar track) has ai_track_difficulties.{ai_data.TARGET}.{field}")
    container = "entrants.vehicles.vehicle_packages"
    folder = ai_data.TARGET if car_pack else C.AI_FOLDER
    for name in (C.PACKAGE, C.PACKAGE_B):
        try:
            i = built.row_index(container, "vehicle_package", name)
        except KeyError:
            bad.append(f"missing vehicle package {name}")
            continue
        got = built.get(f"{container}.vehicle_package_{i}.ai_package_folder")
        if got != folder:
            bad.append(f"{name}.ai_package_folder = {got!r}, want {folder!r}")
        if name == C.PACKAGE and car_pack and built.get(f"{container}.vehicle_package_{i}.available") != "True":
            bad.append(f"{name}.available is not True")
    table = f"{ai_data.ASSET_DATA}.{ai_data.TARGET}"
    if car_pack:
        for folder in ai_data.asset_rows():
            try:
                i = built.row_index(table, "asset_data", folder)
            except KeyError:
                bad.append(f"no {table} row for {folder}")
                continue
            src = f"{ai_data.ASSET_DATA}.{ai_data.SOURCE}.asset_data_{stock.row_index(f'{ai_data.ASSET_DATA}.{ai_data.SOURCE}', 'asset_data', folder)}"
            for field in ai_data.ASSET_FIELDS:
                want_v = ai_data.BASE_CAR if field == "base_car" else stock.get(f"{src}.{field}")
                if built.get(f"{table}.asset_data_{i}.{field}") != want_v:
                    bad.append(f"{table}[{folder}].{field} = {built.get(f'{table}.asset_data_{i}.{field}')!r}, want {want_v!r}")
    elif built.count(table, "asset_data"):
        bad.append(f"{table} exists without the car pack")
    placeholder = [t for t in ai_data.TRACKS if t not in ratios]
    return bad, len(guard), found, source, placeholder


def check_assets(path, game):
    bad = []
    ops = json.loads(Path(path).read_text())["ops"]
    puts = {o["target"]: o["source"] for o in ops if o["op"] == "file.put"}
    groups = {o["group"]: o for o in ops if o["op"] == "group.register"}
    want = 0
    for track in ai_data.TRACKS:
        for folder in sorted({c.folder for c in C.CARS}):
            want += 1
            base = f"{AI_ROOT}/{track}/ai/{ai_data.TARGET}/{folder}/{folder}"
            group, erp = f"asset_groups/{base}.assetgroup", f"2025_asset_groups/{base}.erp"
            template = f"asset_groups/{AI_ROOT}/{track}/ai/{C.SOURCE_PACKAGE}/{folder}/{folder}.assetgroup"
            g = groups.get(group)
            if g is None:
                bad.append(f"no group.register for {group}")
                continue
            erps = ([g["erp"]] if "erp" in g else []) + g.get("erps", [])
            if erps != [erp]:
                bad.append(f"{group}: erps {erps}, want [{erp}]")
            if erp not in puts:
                bad.append(f"no file.put for {erp}")
            if g.get("template") != template:
                bad.append(f"{group}: template {g.get('template')!r}, want {template!r}")
            elif game and not (Path(game) / ("2025_" + template[:-len(".assetgroup")] + ".erp")).exists():
                bad.append(f"{group}: template archive missing in {game}")
    ai_groups = [g for g in groups if f"/ai/{ai_data.TARGET}/" in g]
    if len(ai_groups) != want:
        bad.append(f"{len(ai_groups)} AI groups, want {want}")
    return bad, len(ai_groups)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument("stock")
    ap.add_argument("built")
    ap.add_argument("--car-pack", action="store_true", help="the build includes swofty.f1-1990-cars")
    ap.add_argument("--laps", type=Path, help="lap ratio file the shipped ops were made with")
    ap.add_argument("--assets", type=Path, help="the car pack's assets/ai.json")
    ap.add_argument("--game", type=Path, help="stock F1 25 folder for the template archives")
    args = ap.parse_args()
    stock, built = career.Db(args.stock), career.Db(args.built)
    bad, guarded, found, source, placeholder = check_db(stock, ai_data.Stock(args.stock), built, args.car_pack, args.laps)
    print(f"protected stock keys: {guarded}; 1990 difficulty keys: {found} of {len(ai_data.TRACKS) * len(ai_data.FIELDS)}"
          f"; lap ratios: {source or 'none'}")
    if placeholder:
        print(f"note: placeholder lap ratio {ai_data.PLACEHOLDER_RATIO} on {len(placeholder)} track(s)")
    if args.assets:
        more, n = check_assets(args.assets, args.game)
        print(f"AI asset groups: {n}")
        bad += more
    if bad:
        print(f"FAIL: {len(bad)} problem(s)")
        for line in bad[:60]:
            print("  " + line)
        return 1
    print("ok: stock AI difficulty, grip and package rows unchanged; 1990 AI keys and folders as built")
    return 0


if __name__ == "__main__":
    sys.exit(main())
