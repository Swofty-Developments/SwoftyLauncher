#!/usr/bin/env python3
import argparse
import glob
import json
import os
import struct
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor
from functools import lru_cache
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/rnd-tyres-thermal/tools")]

import bc7enc
import grid
from common import bc7_decode
from erp import parse as erp_parse
from erpw import Erp
from logo_surfaces import BASE, Surface
from pkg import Pkg
from season import slots
from stock import Stock, fragments

UI = "2025_asset_groups/ui_package/"
DATABASES = UI + "databases.erp"
OVERRIDES = "eaid://ui_package/databases/overrides.uidb"
ARRAY = "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.{}.texarray"
BC7 = 70
ARRAYS = {
    "team_logos.f1_2026": ("row", "card", "Grand Prix Team Select tiles"),
    "teams.mp_team_select_logos": ("row", "mp", "Team Select cards"),
    "teams.fanzone_room": ("row", "room", "F1 World fan zone room"),
    "teams.teams_f1_2024_gp_voting": ("row", "lockup", "F1 World GP voting"),
    "team_logos.driver_career_2024_team_select": ("row", "mark", "Driver Career team logos"),
    "broadcast_ui_generic.grid_team_logos": ("row", "mark", "Pre-race grid rundown"),
    "teams.fanzone_expressions_large": ("row", "mark", "Fan zone expressions, large"),
    "teams.driver_select_team_logos": ("row", "mark", "Driver Select, R&D and Team HQ tiles"),
    "teams.badges_broadcast": ("row", "mark", "Standings, post race, press and R&D lists"),
    "teams.fanzone_expressions": ("row", "mark", "Fan zone expressions, small"),
    "osd.timing_tower.team_badges_hud": ("row", "mark", "Race HUD timing tower"),
    "osd.timing_tower.team_badges_broadcast": ("row", "mark", "Broadcast timing tower"),
    "driver_career_rewards.recognition.team_icons": ("row", "mark", "Career rewards, team recognition"),
    "teams.team_contract_logo": ("row", "contract", "Career hub contract offers"),
    "team_hq.contract_negotiation_logos": ("row", "fade", "Contract negotiation"),
    "team_hq.contract_logos_mid": ("row", "fade", "Team HQ contract cards"),
    "team_hq.contract_logos_large": ("row", "tint", "Team HQ contract backdrop"),
    "team_hq.contract_holding_screen_bgs": ("row", "holding", "Contract holding screen banner"),
    "press_interviews.pill_bg": ("row", "pill", "Press interview team pill"),
    "teams.career_laptop_header": ("laptop", "blank", "Career laptop header and garage monitor (blank, as stock)"),
    "broadcast_ui_generic.team": ("row", "blank", "Broadcast team card (blank, as stock F1 teams)"),
    "teams.career_team_select_logo_small": ("select", "blank", "Secret meetings and holding screen (blank, as stock)"),
}
REFS = ("williams_26", "williams", "laptop_williams_logo", "team_select_williams")
OUT = ROOT / "erp/badges"


def key(kind, row):
    return {"select": f"team_select_{row}", "laptop": f"laptop_{row}_logo"}.get(kind, row)


def carriers(game, names):
    found = {n: [] for n in names}
    for e in sorted(glob.glob(str(game / UI / "*.erp")) + glob.glob(str(game / UI / "screens/*.erp"))):
        _, res = erp_parse(e)
        for r in res:
            if r[0] in found:
                found[r[0]].append(os.path.relpath(e, game))
    return found


def overrides_list(root, path):
    node = root["overrides"]["Contents"]
    parts = path.split(".")
    for p in parts[:-1]:
        node = node[p]["Contents"]
    return [e["Name"].value for e in node[parts[-1]]["Contents"].items]


def primary(team):
    colours = grid.TEAM_INFO[team][3]
    return colours[1] if colours[0].upper() == "#FFFFFF" else colours[0]


def wanted(style, names):
    if style in ("mark", "lockup"):
        return [n for n in names if n.endswith("_26")] or [n for n in names if n in BASE]
    return [n for n in REFS if n in names]


def decode(job):
    path, name, px, w, h = job
    return path, name, Image.frombytes("RGBA", (w, h), bc7_decode(px, w, h))


def encode(job):
    path, token, im = job
    return path, token, bc7enc.encode_rgba(im.tobytes(), im.width, im.height)


def build(game, baseline, sheet):
    stock = Stock(game, baseline)
    root = Pkg(fragments(stock.path(DATABASES), OVERRIDES)[0]).root
    teams = [(s.team, grid.team_row(s.team)) for s in slots()]
    names = [ARRAY.format(p) + suffix for p in ARRAYS for suffix in (".image", "")]
    where = carriers(game, names)

    plans, jobs = {}, []
    for path, (kind, style, _) in ARRAYS.items():
        image, view = ARRAY.format(path) + ".image", ARRAY.format(path)
        if not where[image] or where[image] != where[view]:
            sys.exit(f"{path}: carriers {where[image]} / {where[view]}")
        frags = None
        for rel in where[image]:
            got = fragments(stock.path(rel), image) + fragments(stock.path(rel), view)
            if frags is None:
                frags = got
            elif got != frags:
                sys.exit(f"{path}: {rel} carries a different stock array")
        head, data, view0 = frags
        fmt, w, h = struct.unpack_from("<III", head, 8)
        mips, count = struct.unpack_from("<II", head, 24)
        names_ = overrides_list(root, path)
        if fmt != BC7 or mips != 1 or len(data) != count * w * h or len(names_) != count \
                or struct.unpack_from("<I", view0, 0x14)[0] != count:
            sys.exit(f"{path}: unexpected stock array (format {fmt}, {w}x{h}, {mips} mips, {count} slices)")
        append = [(key(kind, row), team, row[:-len("_1990")]) for team, row in teams]
        taken = [k for k, *_ in append if k in names_]
        if taken:
            sys.exit(f"{path}: already lists {taken}")
        plans[path] = (where[image], head, data, view0, w, h, count, append, names_)
        jobs += [(path, n, data[names_.index(n) * w * h:(names_.index(n) + 1) * w * h], w, h)
                 for n in wanted(style, names_)]

    with ProcessPoolExecutor(max_workers=int(os.environ.get("JOBS", "8"))) as pool:
        stock_slices = {}
        for path, name, im in pool.map(decode, jobs):
            stock_slices.setdefault(path, {})[name] = im
        renders = []
        for path, (kind, style, _) in ARRAYS.items():
            surface = Surface(style, stock_slices[path])
            w, h = plans[path][4], plans[path][5]
            for _, team, token in plans[path][7]:
                im = surface.render(token, primary(team), w, h)
                if im.size != (w, h):
                    sys.exit(f"{path}: {token} rendered at {im.size}")
                renders.append((path, token, im))
        encoded = {(p, t): px for p, t, px in pool.map(encode, renders)}

    if OUT.exists():
        for p in OUT.iterdir():
            p.unlink()
    OUT.mkdir(parents=True, exist_ok=True)
    grown, ops, edits, report = {}, [], [], []
    for path, (rels, head, data, view0, w, h, count, append, names_) in plans.items():
        image, view = ARRAY.format(path) + ".image", ARRAY.format(path)
        n = len(append)
        head2 = bytearray(head)
        struct.pack_into("<I", head2, 28, count + n)
        view2 = bytearray(view0)
        struct.pack_into("<I", view2, 0x14, count + n)
        grown[path] = (bytes(head2), data + b"".join(encoded[(path, t)] for _, _, t in append), bytes(view2))
        for rel in rels:
            for res in (image, view):
                ops.append({"op": "put-resource", "erp": rel, "resource": res, "source": "erp/badges/badges.erpk"})
        at = "overrides.Contents." + ".Contents.".join(path.split(".")) + ".Contents"
        for k, *_ in append:
            edits.append({"op": "clone-entry", "at": at, "src": f"[Name={names_[-1]}]", "as": k,
                          "set": {"Value": f"ui_package/art/textures/f1990/badges/{k}.tif"}})
        report.append(f"{path}: {w}x{h}, {count} + {n} slices, {len(rels)} archive(s)")

    write_archive(stock, plans, grown, OUT / "badges.erpk")
    ops.append({"op": "patch-pkg", "erp": DATABASES, "resource": OVERRIDES, "fragment": 0, "edits": edits})
    (ROOT / "erp/badges.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    if sheet:
        contact_sheet(plans, stock_slices, encoded, sheet)
    return report


def write_archive(stock, plans, grown, out):
    keep, arc = [], None
    for path, (rels, *_rest) in plans.items():
        e = Erp(str(stock.path(rels[0])))
        image, view = ARRAY.format(path) + ".image", ARRAY.format(path)
        r_image, r_view = e.find(image), e.find(view)
        h2, d2, v2 = grown[path]
        e.set_frag(r_image, 0, h2)
        e.set_frag(r_image, 1, d2)
        e.set_frag(r_view, 0, v2)
        keep += [r_image, r_view]
        if arc is None:
            arc = e
    arc.res = keep
    arc.to_file(str(out))


@lru_cache(None)
def sheet_font(size):
    path = subprocess.run(["fc-match", "-f", "%{file}", "DejaVu Sans"], capture_output=True, text=True).stdout
    return ImageFont.truetype(path, size)


def contact_sheet(plans, stock_slices, encoded, out):
    pad, label_h = 8, 22
    rows = []
    for path, (rels, head, data, view0, w, h, count, append, names_) in plans.items():
        have = stock_slices[path]
        ref = next(have[k] for k in REFS + tuple(sorted(have)) if k in have)
        cells = [("stock", ref)] + [(t, Image.frombytes("RGBA", (w, h), bc7_decode(encoded[(path, t)], w, h)))
                                    for _, _, t in append]
        scale = (3 if max(w, h) > 32 else 4) if max(w, h) <= 64 else min(1.0, 150 / h, 330 / w)
        rows.append((f"{ARRAYS[path][2]}    overrides.{path}  {w}x{h}", w, h, scale, cells))
    width = max(pad + len(cells) * (round(w * s) + pad + (w + 8 if s > 1 else 0)) for _, w, h, s, cells in rows)
    height = pad + sum(label_h + max(1, round(h * s)) + 20 + pad for _, w, h, s, _ in rows)
    sheet = Image.new("RGBA", (width, height), (52, 52, 58, 255))
    d = ImageDraw.Draw(sheet)
    y = pad
    for label, w, h, scale, cells in rows:
        d.text((pad, y), label, fill=(255, 255, 255, 255), font=sheet_font(15))
        y += label_h
        cw, ch = max(1, round(w * scale)), max(1, round(h * scale))
        x = pad
        for i, (name, im) in enumerate(cells):
            sheet.paste((110, 70, 70, 255) if i == 0 else (80, 80, 88, 255), (x - 2, y - 2, x + cw + 2, y + ch + 2))
            sheet.alpha_composite(im.resize((cw, ch), Image.NEAREST if scale > 1 else Image.LANCZOS), (x, y))
            if scale > 1:
                sheet.alpha_composite(im, (x + cw + 4, y + ch - h))
            d.text((x, y + ch + 3), name, fill=(220, 220, 220, 255), font=sheet_font(13))
            x += cw + pad + (w + 8 if scale > 1 else 0)
        y += ch + 20 + pad
    sheet.convert("RGB").save(out)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path, help="launcher baseline folder holding baseline.json (stock copies)")
    ap.add_argument("--sheet", type=Path, help="write a contact sheet PNG: a stock slice, then the 1990 slices")
    a = ap.parse_args(argv[1:])
    for line in build(a.game, a.baseline, a.sheet):
        print(line)


if __name__ == "__main__":
    main(sys.argv)
