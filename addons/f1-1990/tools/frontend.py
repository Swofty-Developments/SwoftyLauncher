#!/usr/bin/env python3
"""Writes erp/frontend.json and erp/frontend/: Driver Select and Team Select backdrop names for the 1990 driver and
team rows, as new textures, materials and material contexts in main_front_end.erp. No stock resource changes.

usage: frontend.py --game <F1 25 folder> [--baseline <launcher baseline>/<build>] [--preview <dir>]
"""
import argparse
import json
import os
import struct
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor
from functools import lru_cache
from pathlib import Path

import lz4.block
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/rnd-tyres-thermal/tools")]

import bc7enc
import grid
from season import Season, slots
from stock import Stock, fragments

MFE = "2025_asset_groups/environment_package/second_world/main_front_end/wep/main_front_end.erp"
IDF = "eaid://environment_package/core_assets/objects/idf/main_front_end/"
MATERIALS = "eaid://environment_package/core_assets/objects/materials/main_front_end/names/"
TEX = "eaid://environment_package/core_assets/objects/textures/main_front_end/names/"
NAMES_DIR = "environment_package/core_assets/objects/textures/main_front_end/names/"
FONT = "DejaVu Sans:style=Bold"
STRETCH = 1.25
OUT = ROOT / "erp/frontend"
GAMEDB = "game_package/gamedatabases/gamedatabases.2025.bin"
KINDS = {
    "driver": {"bind": IDF + "mfe_2025_main_menu_driver_select_name.emb?context=", "template_context": "zenith_driverfirst",
               "folder": "f1_driver_names", "prefix": "f1_drivername_", "template": "sonny_hayes",
               "material": "mfe_2025_driver_select_{}_a.material", "side": ("f1_driver_names~", "f1990_drv_names~"),
               "id": "f1990_d_{:03d}"},
    "team": {"bind": IDF + "mfe_2025_main_menu_vehicle_select_name.emb?context=", "template_context": "mercedes_26",
             "folder": "f1_team_names", "prefix": "f1_teamname_", "template": "mercedes",
             "material": "mfe_2025_team_select_{}_a.material", "side": ("f1_team_names~", "f1990_tmnames~"),
             "id": "f1990_{:02d}"},
}


@lru_cache(None)
def font_path():
    return subprocess.run(["fc-match", "-f", "%{file}", FONT], capture_output=True, text=True, check=True).stdout


def line_mask(text, cap, outline):
    font = ImageFont.truetype(font_path(), 400)
    l, t, r, b = font.getbbox(text)
    im = Image.new("L", (r - l + 40, b - t + 40), 0)
    ImageDraw.Draw(im).text((20 - l, 20 - t), text, font=font, fill=255)
    cap_px = font.getbbox("H")[3] - font.getbbox("H")[1]
    im = im.crop(im.getbbox())
    scale = cap / cap_px
    im = im.resize((max(1, round(im.width * scale * STRETCH)), max(1, round(im.height * scale))), Image.LANCZOS)
    if outline:
        padded = Image.new("L", (im.width + 8, im.height + 8), 0)
        padded.paste(im, (4, 4))
        im = padded
        grown = im.filter(ImageFilter.MaxFilter(7))
        shrunk = im.filter(ImageFilter.MinFilter(7))
        im = ImageChops.subtract(grown, shrunk)
    return im


def fit(mask, width):
    return mask if mask.width <= width else mask.resize((width, mask.height), Image.LANCZOS)


def driver_image(w, h, first, last):
    s = 2
    W, H = w * s, h * s
    alpha = Image.new("L", (W, H), 0)
    right = W - round(0.049 * W)
    avail = right - round(0.03 * W)
    cap = round(0.33 * H)
    for text, top, outline in ((first, 0.148, False), (last, 0.518, True)):
        m = fit(line_mask(text, cap, outline), avail)
        alpha.paste(m, (right - m.width, round(top * H)), m)
    return alpha.resize((w, h), Image.LANCZOS)


def team_image(w, h, name):
    s = 2
    W, H = w * s, h * s
    alpha = Image.new("L", (W, H), 0)
    m = fit(line_mask(name, round(0.80 * H), True), round(0.93 * W))
    alpha.paste(m, ((W - m.width) // 2, round(0.098 * H)), m)
    return alpha.resize((w, h), Image.LANCZOS)



def team_label(team, short):
    return "McLAREN" if team == "mclaren" else short.upper()


def same_length(data, old, new):
    old, new = old.encode(), new.encode()
    if len(old) != len(new) or old not in data:
        sys.exit(f"cannot swap {old!r} for {new!r}")
    return data.replace(old, new)


def encode(job):
    kind, new_id, args, hdr, f1_len, f2 = job
    k = KINDS[kind]
    _, _, fmt, w0, h0, _, erp_levels = struct.unpack_from("<7I", hdr)
    at = 1 + f2[0] + 1
    side_n = struct.unpack_from("<I", f2, at)[0]
    w, h = w0 << side_n, h0 << side_n
    alpha = (driver_image if kind == "driver" else team_image)(w, h, *args)
    levels = []
    for i in range(side_n + erp_levels):
        lw, lh = w >> i, h >> i
        a = alpha.resize((lw, lh), Image.BOX) if i else alpha
        rgba = Image.merge("RGBA", (Image.new("L", (lw, lh), 255),) * 3 + (a,))
        levels.append(bc7enc.encode(rgba.tobytes(), lw, lh))
    f1 = b"".join(levels[side_n:])
    assert len(f1) == f1_len
    old_side = f"{k['side'][0]}{k['prefix']}{k['template']}"
    new_side = f"{k['side'][1]}{k['prefix']}{new_id}"
    f2 = bytearray(same_length(f2, old_side, new_side))
    side, off = b"", 0
    for i, lvl in enumerate(levels[:side_n]):
        packed = lz4.block.compress(lvl, store_size=False, mode="high_compression")
        f2[at + 4 + 26 * i:at + 4 + 26 * (i + 1)] = struct.pack("<BBQQQ", 0, 3, off, len(packed), len(lvl))
        side += packed
        off += len(packed)
    path = bytes(f2[1:1 + f2[0]]).rstrip(b"\0").decode()
    return kind, new_id, f1, bytes(f2), path, side, alpha


def build(stock, preview=None):
    arc = stock.path(MFE)
    season = Season.load()
    _, _, report = grid.build(grid.Stock(str(stock.path(GAMEDB))), season)
    rows = []
    for did, (label, _) in report["rows"].items():
        d = season.drivers[did]
        rows.append(("driver", label, (d.first.upper(), d.last.upper())))
    for s in slots():
        rows.append(("team", grid.team_row(s.team), (team_label(s.team, grid.TEAM_INFO[s.team][1]),)))
    templates, jobs, ops = {}, [], []
    for kind, k in KINDS.items():
        tex = f"{TEX}{k['folder']}/{k['prefix']}{k['template']}.tif"
        material = MATERIALS + k["material"].format(k["template"])
        templates[kind] = {
            "image": fragments(arc, tex + ".image"), "srv": fragments(arc, tex)[0],
            "material": fragments(arc, material)[0], "bind": fragments(arc, k["bind"] + k["template_context"])[0],
        }
    counters = {kind: 0 for kind in KINDS}
    ids = []
    for kind, label, args in rows:
        counters[kind] += 1
        new_id = KINDS[kind]["id"].format(counters[kind])
        hdr, f1, f2 = templates[kind]["image"]
        ids.append((kind, label, new_id))
        jobs.append((kind, new_id, args, hdr, len(f1), f2))
    with ProcessPoolExecutor(max_workers=int(os.environ.get("JOBS", "8"))) as pool:
        encoded = {(kind, new_id): rest for kind, new_id, *rest in pool.map(encode, jobs)}

    if OUT.exists():
        for p in sorted(OUT.rglob("*"), reverse=True):
            p.unlink() if p.is_file() else p.rmdir()
    (OUT / "mipmaps").mkdir(parents=True)
    for kind in KINDS:
        (OUT / f"{kind}.head").write_bytes(templates[kind]["image"][0])
    for kind, label, new_id in ids:
        k, t = KINDS[kind], templates[kind]
        f1, f2, side_path, side, alpha = encoded[(kind, new_id)]
        old_tex, new_tex = f"{k['prefix']}{k['template']}", f"{k['prefix']}{new_id}"
        old_mat, new_mat = k["material"].format(k["template"]), k["material"].format(new_id)
        files = {
            "f1": f1, "f2": f2,
            "srv": same_length(t["srv"], old_tex, new_tex),
            "material": same_length(t["material"], old_tex, new_tex),
            "bind": same_length(t["bind"], old_mat, new_mat),
        }
        for ext, data in files.items():
            (OUT / f"{new_id}.{ext}").write_bytes(data)
        (OUT / "mipmaps" / side_path[len(NAMES_DIR):]).write_bytes(side)
        tex = f"{TEX}{k['folder']}/{new_tex}.tif"
        src = lambda ext: f"erp/frontend/{new_id}.{ext}"
        ops += [
            {"op": "add-resource", "erp": MFE, "resource": tex + ".image", "type": "GfxSurfaceRes",
             "sources": [f"erp/frontend/{kind}.head", src("f1"), src("f2")]},
            {"op": "add-resource", "erp": MFE, "resource": tex, "type": "GfxSRVResource", "sources": [src("srv")]},
            {"op": "add-resource", "erp": MFE, "resource": MATERIALS + new_mat, "type": "GfxMaterialRes",
             "sources": [src("material")]},
            {"op": "add-resource", "erp": MFE, "resource": k["bind"] + label, "type": "GfxMatBindRes",
             "sources": [src("bind")]},
        ]
        if preview:
            preview.mkdir(parents=True, exist_ok=True)
            alpha.save(preview / f"{new_id}_{label}.png")
    ops.append({"op": "add-mipmaps", "source": "erp/frontend/mipmaps/", "target": NAMES_DIR})
    (ROOT / "erp/frontend.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    return ids


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path)
    ap.add_argument("--preview", type=Path)
    a = ap.parse_args(argv[1:])
    ids = build(Stock(a.game, a.baseline), a.preview)
    for kind, label, new_id in ids:
        print(f"{kind:6} {label:28} {new_id}")


if __name__ == "__main__":
    main(sys.argv)
