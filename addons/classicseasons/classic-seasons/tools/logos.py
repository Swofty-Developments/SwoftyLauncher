#!/usr/bin/env python3
import argparse
import glob
import hashlib
import json
import os
import struct
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor
from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[2]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/rnd-tyres-thermal/tools")]

import bc7enc
from common import bc7_decode
from erp import parse as erp_parse
from erpw import ZSTD, Erp, zstd_c
from logo_surfaces import Surface
from pkg import Pkg
from stock import Stock, fragments

UI = "2025_asset_groups/ui_package/"
DATABASES = UI + "databases.erp"
OVERRIDES = "eaid://ui_package/databases/overrides.uidb"
ARRAY = "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.{}.texarray"
TEXTURE = "ui_package/art/textures/classic_seasons/logos/{}.tif"
BC7 = 70
REF = "williams"
TOWER = {"redbull": "red_bull", "forceindia": "aston_martin", "renault": "alpine", "tororosso": "alphatauri",
         "sauber": "alfa_romeo"}
ARRAYS = {
    "team_logos.f1_2023": ("card", {}, "Grand Prix and Time Trial Team Select tiles (entrant list ui_path f1_2023)"),
    "teams.mp_team_select_logos": ("mp", {}, "Multiplayer Team Select cards"),
    "teams.driver_select_team_logos": ("mark", {}, "Driver Select team logos"),
    "team_logos.driver_career_2024_team_select": ("mark", {}, "Team select logos"),
    "teams.badges_broadcast": ("mark", {}, "Standings, results and post race lists"),
    "broadcast_ui_generic.grid_team_logos": ("mark", {}, "Pre-race grid rundown"),
    "osd.timing_tower.team_badges_hud": ("mark", TOWER, "Race HUD timing tower"),
    "osd.timing_tower.team_badges_broadcast": ("mark", TOWER, "Broadcast timing tower"),
}
BRANDS = {"Force India": "force_india", "Racing Point": "racing_point", "Renault": "renault",
          "Toro Rosso": "toro_rosso", "AlphaTauri": "alphatauri", "Sauber": "sauber", "Alfa Romeo": "alfa_romeo",
          "Manor": "manor"}
OUT = ROOT / "erp/logos"
W2 = np.array([0, 21, 43, 64])


def season_teams():
    data = json.loads((ROOT / "data/seasons.json").read_text())
    return [(f"cs{s['year'] % 100:02d}_{t.get('key', t['folder'])}", s["year"], t["name"], data["folders"][t["folder"]]["team"],
             BRANDS.get(t["name"])) for s in data["seasons"] for t in s["teams"]]


def carriers(game, stock, names):
    found = {n: [] for n in names}
    for e in sorted(glob.glob(str(game / UI / "*.erp")) + glob.glob(str(game / UI / "screens/*.erp"))):
        rel = os.path.relpath(e, game)
        _, res = erp_parse(str(stock.path(rel)))
        for r in res:
            if r[0] in found:
                found[r[0]].append(rel)
    return found


def overrides_list(root, path):
    node = root["overrides"]["Contents"]
    parts = path.split(".")
    for p in parts[:-1]:
        node = node[p]["Contents"]
    return [e["Name"].value for e in node[parts[-1]]["Contents"].items]


def decode(job):
    path, name, px, w, h = job
    return path, name, Image.frombytes("RGBA", (w, h), bc7_decode(px, w, h))


def blocks(im):
    w, h = im.size
    return np.asarray(im).reshape(h // 4, 4, w // 4, 4, 4).transpose(0, 2, 1, 3, 4).reshape(-1, 16, 4).astype(np.int64)


def block_error(px, dec):
    d = (px - dec).astype(np.float64) ** 2
    return (d[..., :3].sum(2) * px[..., 3] / 255 + 4 * d[..., 3]).sum(1)


def ramp(e0, e1):
    return ((64 - W2)[None, :, None] * e0[:, None] + W2[None, :, None] * e1[:, None] + 32) >> 6


def nearest(pal, src):
    return ((pal[:, None] - src[:, :, None]) ** 2).sum(-1).argmin(2)


def mode5_fit(src, wt, idx):
    f = W2[idx] / 64
    g = 1 - f
    a, b, c = (wt * g * g).sum(1), (wt * g * f).sum(1), (wt * f * f).sum(1)
    x0, x1 = ((wt * g)[..., None] * src).sum(1), ((wt * f)[..., None] * src).sum(1)
    det = a * c - b * b
    ok = (np.abs(det) > 1e-9)[:, None]
    safe = np.where(ok[:, 0], det, 1)[:, None]
    flat = (x0 + x1) / np.maximum(a + 2 * b + c, 1e-9)[:, None]
    return np.where(ok, (c[:, None] * x0 - b[:, None] * x1) / safe, flat), \
        np.where(ok, (a[:, None] * x1 - b[:, None] * x0) / safe, flat)


def mode5(px):
    src, wt = px[..., :3].astype(np.float64), px[..., 3] / 255
    mean = (src * wt[..., None]).sum(1) / np.maximum(wt.sum(1), 1e-9)[:, None]
    dev = (src - mean[:, None]) * np.sqrt(wt)[..., None]
    cov = np.einsum("npi,npj->nij", dev, dev)
    axis = np.ones((len(px), 3))
    for _ in range(8):
        axis = np.einsum("nij,nj->ni", cov, axis)
        axis /= np.maximum(np.linalg.norm(axis, axis=1, keepdims=True), 1e-12)
    t = np.einsum("npc,nc->np", src - mean[:, None], axis)
    lo, hi = np.where(wt > 0, t, np.inf).min(1), np.where(wt > 0, t, -np.inf).max(1)
    lo, hi = np.where(np.isfinite(lo), lo, 0), np.where(np.isfinite(hi), hi, 0)
    best = None
    ends = (mean + lo[:, None] * axis, mean + hi[:, None] * axis)
    for _ in range(3):
        q = [np.clip(np.rint(e / 255 * 127), 0, 127).astype(np.int64) for e in ends]
        pal = ramp(*[(v << 1) | (v >> 6) for v in q])
        idx = nearest(pal, px[..., :3])
        err = (((np.take_along_axis(pal, idx[..., None], 1) - px[..., :3]) ** 2).sum(2) * wt).sum(1)
        if best is None:
            best = [q, idx, err]
        else:
            take = err < best[2]
            best = [[np.where(take[:, None], n, o) for n, o in zip(q, best[0])],
                    np.where(take[:, None], idx, best[1]), np.where(take, err, best[2])]
        ends = mode5_fit(src, wt, best[1])
    (c0, c1), cidx = best[0], best[1]
    a0, a1 = px[..., 3].min(1), px[..., 3].max(1)
    apal = ramp(a0[:, None], a1[:, None])[..., 0]
    aidx = np.abs(apal[:, None] - px[..., 3:4]).argmin(2)
    cpal = ramp((c0 << 1) | (c0 >> 6), (c1 << 1) | (c1 >> 6))
    dec = np.concatenate([np.take_along_axis(cpal, cidx[..., None], 1),
                          np.take_along_axis(apal, aidx, 1)[..., None]], 2)
    return (c0, c1, a0, a1, cidx, aidx), block_error(px, dec)


def mode5_word(fit, n):
    c0, c1, a0, a1, cidx, aidx = [f[n].tolist() for f in fit]
    if cidx[0] >= 2:
        c0, c1, cidx = c1, c0, [3 - i for i in cidx]
    if aidx[0] >= 2:
        a0, a1, aidx = a1, a0, [3 - i for i in aidx]
    fields = [(1 << 5, 6), (0, 2)] + [(e[c], 7) for c in range(3) for e in (c0, c1)] + [(a0, 8), (a1, 8)]
    fields += [(cidx[0], 1)] + [(i, 2) for i in cidx[1:]] + [(aidx[0], 1)] + [(i, 2) for i in aidx[1:]]
    bits, pos = 0, 0
    for v, k in fields:
        bits |= v << pos
        pos += k
    return bits.to_bytes(16, "little")


def encode(job):
    path, brand, im = job
    data = bytearray(bc7enc.encode_rgba(im.tobytes(), im.width, im.height))
    px = blocks(im)
    dec = blocks(Image.frombytes("RGBA", im.size, bc7_decode(bytes(data), im.width, im.height)))
    fit, err = mode5(px)
    for n in np.nonzero(err < block_error(px, dec))[0].tolist():
        data[n * 16:(n + 1) * 16] = mode5_word(fit, n)
    return path, brand, bytes(data)


def build(game, baseline, sheet):
    stock = Stock(game, baseline)
    root = Pkg(fragments(stock.path(DATABASES), OVERRIDES)[0]).root
    teams = season_teams()
    brands = sorted({b for *_, b in teams if b})
    names = [ARRAY.format(p) + suffix for p in ARRAYS for suffix in (".image", "")]
    where = carriers(game, stock, names)

    plans, jobs = {}, []
    for path, (style, keymap, _) in ARRAYS.items():
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
        listed = overrides_list(root, path)
        if fmt != BC7 or mips != 1 or len(data) != count * w * h or len(listed) != count \
                or struct.unpack_from("<I", view0, 0x14)[0] != count:
            sys.exit(f"{path}: unexpected stock array (format {fmt}, {w}x{h}, {mips} mips, {count} slices)")
        taken = [k for k, *_ in teams if k in listed]
        if taken:
            sys.exit(f"{path}: already lists {taken}")
        donors = sorted({keymap.get(d, d) for *_, d, _ in teams})
        missing = [d for d in donors if d not in listed]
        if missing:
            sys.exit(f"{path}: no stock slice for {missing}")
        plans[path] = (where[image], head, data, view0, w, h, count, listed)
        for n in (donors if style == "mark" else [REF]):
            i = listed.index(n)
            jobs.append((path, n, data[i * w * h:(i + 1) * w * h], w, h))

    with ProcessPoolExecutor(max_workers=int(os.environ.get("JOBS", "8"))) as pool:
        stock_slices = {}
        for path, name, im in pool.map(decode, jobs):
            stock_slices.setdefault(path, {})[name] = im
        renders = []
        for path, (style, _, _) in ARRAYS.items():
            surface = Surface(style, stock_slices[path], None if style == "mark" else REF)
            w, h = plans[path][4], plans[path][5]
            for brand in brands:
                im = surface.render(brand, w, h)
                if im.size != (w, h):
                    sys.exit(f"{path}: {brand} rendered at {im.size}")
                renders.append((path, brand, im))
        encoded = {(p, b): px for p, b, px in pool.map(encode, renders)}

    if OUT.exists():
        for p in OUT.iterdir():
            p.unlink()
    OUT.mkdir(parents=True, exist_ok=True)
    grown, slices, ops, edits, report = {}, {}, [], [], []
    for path, (rels, head, data, view0, w, h, count, listed) in plans.items():
        keymap = ARRAYS[path][1]
        image, view = ARRAY.format(path) + ".image", ARRAY.format(path)
        added = []
        for key, _, _, donor, brand in teams:
            if brand:
                px = encoded[(path, brand)]
            else:
                i = listed.index(keymap.get(donor, donor))
                px = data[i * w * h:(i + 1) * w * h]
            if len(px) != w * h:
                sys.exit(f"{path}: {key} slice is {len(px)} bytes, not {w * h}")
            slices[(path, key)] = px
            added.append(px)
        n = len(added)
        head2 = bytearray(head)
        struct.pack_into("<I", head2, 28, count + n)
        view2 = bytearray(view0)
        struct.pack_into("<I", view2, 0x14, count + n)
        grown[path] = (bytes(head2), data + b"".join(added), bytes(view2))
        for rel in rels:
            for res in (image, view):
                ops.append({"op": "put-resource", "erp": rel, "resource": res, "source": "erp/logos/logos.erpk"})
        at = "overrides.Contents." + ".Contents.".join(path.split(".")) + ".Contents"
        for key, *_ in teams:
            edits.append({"op": "clone-entry", "at": at, "src": f"[Name={listed[-1]}]", "as": key,
                          "set": {"Value": TEXTURE.format(key)}})
        report.append(f"{path}: {w}x{h}, {count} + {n} slices, {len(rels)} archive(s)")

    write_archive(stock, plans, grown, OUT / "logos.erpk")
    ops.append({"op": "patch-pkg", "erp": DATABASES, "resource": OVERRIDES, "fragment": 0, "edits": edits})
    (ROOT / "erp/logos.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    if sheet:
        contact_sheet(teams, plans, slices, sheet)
    extracted = sorted({(y, n) for _, y, n, _, b in teams if b})
    report.append(f"{len(teams)} keys, {len(extracted)} with new art, {len(teams) - len(extracted)} reuse stock slices")
    return report


def zstd(data):
    return zstd_c(data, threads=8)


def set_frags(r, datas):
    for fr, data in zip(r.frags, datas):
        fr.raw = zstd(data) if fr.comp == ZSTD else data
        fr.size, fr.packed = len(data), len(fr.raw)
    r.hash = hashlib.md5(b"".join(datas)).digest()


def write_archive(stock, plans, grown, out):
    archives, keep = {}, []
    for path, (rels, *_rest) in plans.items():
        if rels[0] not in archives:
            archives[rels[0]] = Erp(str(stock.path(rels[0])))
        e = archives[rels[0]]
        r_image, r_view = e.find(ARRAY.format(path) + ".image"), e.find(ARRAY.format(path))
        if len(r_image.frags) != 2 or len(r_view.frags) != 1:
            sys.exit(f"{path}: {len(r_image.frags)} image and {len(r_view.frags)} view fragments")
        h2, d2, v2 = grown[path]
        set_frags(r_image, [h2, d2])
        set_frags(r_view, [v2])
        keep += [r_image, r_view]
    arc = next(iter(archives.values()))
    arc.res = keep
    arc.to_file(str(out))


@lru_cache(None)
def sheet_font(size):
    path = subprocess.run(["fc-match", "-f", "%{file}", "DejaVu Sans"], capture_output=True, text=True).stdout
    return ImageFont.truetype(path, size)


def contact_sheet(teams, plans, slices, out):
    card, mark, tower = "team_logos.f1_2023", "teams.driver_select_team_logos", "osd.timing_tower.team_badges_hud"
    cw, ch = plans[card][4], plans[card][5]
    pad, label_w, cell_w, cell_h = 12, 90, cw + 12, ch + 64 + 44
    seasons = sorted({y for _, y, *_ in teams})
    cols = max(sum(1 for t in teams if t[1] == y) for y in seasons)
    sheet = Image.new("RGBA", (label_w + cols * cell_w + pad, pad + len(seasons) * cell_h + 30), (46, 46, 52, 255))
    d = ImageDraw.Draw(sheet)
    d.text((pad, 8), "Classic Seasons logos: Team Select tile (f1_2023), Driver Select mark (64), timing tower (24 x2)."
           "  Green label: new art, grey label: stock slice reused.", fill=(235, 235, 235, 255), font=sheet_font(15))
    for row, year in enumerate(seasons):
        y = 34 + row * cell_h
        d.text((pad, y + ch // 2 - 10), str(year), fill=(255, 255, 255, 255), font=sheet_font(22))
        for col, (key, _, name, _, brand) in enumerate([t for t in teams if t[1] == year]):
            x = label_w + col * cell_w
            dec = lambda path: Image.frombytes("RGBA", (plans[path][4], plans[path][5]),
                                               bc7_decode(slices[(path, key)], plans[path][4], plans[path][5]))
            sheet.alpha_composite(dec(card), (x, y))
            sheet.alpha_composite(dec(mark), (x, y + ch + 4))
            t = dec(tower)
            sheet.alpha_composite(t.resize((48, 48), Image.NEAREST), (x + 72, y + ch + 12))
            sheet.alpha_composite(t, (x + 128, y + ch + 36))
            colour = (120, 220, 120, 255) if brand else (170, 170, 170, 255)
            d.text((x, y + ch + 70), f"{name}", fill=colour, font=sheet_font(13))
            d.text((x, y + ch + 86), key, fill=(150, 150, 150, 255), font=sheet_font(11))
    sheet.convert("RGB").save(out)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path, help="launcher baseline folder holding baseline.json (stock copies)")
    ap.add_argument("--sheet", type=Path, help="write a contact sheet PNG of every season's logos")
    a = ap.parse_args(argv[1:])
    for line in build(a.game, a.baseline, a.sheet):
        print(line)


if __name__ == "__main__":
    main(sys.argv)
