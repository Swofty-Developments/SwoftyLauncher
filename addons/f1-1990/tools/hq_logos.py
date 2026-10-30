#!/usr/bin/env python3
"""Writes erp/hq_logos.json and erp/hq_logos/: 1990 team folders and notebooks for the Driver Career HQ.

The HQ asks each team-dressed prop for the material context `2026-<livery_name>` and falls back to `default`. For each
1990 team this adds that context to the R&D folder and notebook in f1_driver_hq.erp, with a material and a diffuse
texture of its own: the stock unbranded prop recoloured in the team colour where the stock Williams prop is coloured,
with the team logo where Williams has its logo. No stock resource changes.

usage: hq_logos.py --game <F1 25 folder> [--baseline <launcher baseline>/<build>] [--preview <png>]
"""
import argparse
import json
import struct
import sys
from pathlib import Path

import lz4.block
import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools")]

import grid
from logo_surfaces import KEEP_LIGHT, art, fit, lum, rgb
from season import slots
from stock import Stock, fragments

HQ = "2025_asset_groups/environment_package/second_world/f1_driver_hq/wep/f1_driver_hq.erp"
IDF = "eaid://environment_package/core_assets/objects/idf/race_day_r_n_d_area/"
MATERIALS = "eaid://environment_package/core_assets/objects/materials/race_day_r_n_d_area/"
TEX_DIR = "environment_package/core_assets/objects/textures/race_day_r_n_d/"
TEX = "eaid://" + TEX_DIR
YEAR = 2026
PROPS = {
    "raceday_r_n_d_folder_a": {"stock": "williams_c", "mirror": False},
    "raceday_r_n_d_notebook_a": {"stock": "01_williams_c", "mirror": True},
}
IDS = "0123456789abc"
OUT = ROOT / "erp/hq_logos"


def same_length(data, old, new):
    old, new = old.encode(), new.encode()
    if len(old) != len(new) or old not in data:
        sys.exit(f"cannot swap {old!r} for {new!r}")
    return data.replace(old, new)


def rgb565(v):
    v = v.astype(np.int32)
    return np.stack([((v >> 11) & 31) * 527 + 23 >> 6, ((v >> 5) & 63) * 259 + 33 >> 6, (v & 31) * 527 + 23 >> 6], -1)


def bc1_decode(data, w, h):
    n = (w // 4) * (h // 4)
    a = np.frombuffer(data[:n * 8], np.uint8).reshape(n, 8).astype(np.int32)
    c0, c1 = a[:, 0] | a[:, 1] << 8, a[:, 2] | a[:, 3] << 8
    p0, p1 = rgb565(c0), rgb565(c1)
    four = (c0 > c1)[:, None]
    pal = np.stack([p0, p1, np.where(four, (2 * p0 + p1) // 3, (p0 + p1) // 2),
                    np.where(four, (p0 + 2 * p1) // 3, 0)], 1)
    bits = a[:, 4] | a[:, 5] << 8 | a[:, 6] << 16 | a[:, 7] << 24
    sel = np.stack([(bits >> 2 * i) & 3 for i in range(16)], 1)
    px = pal[np.arange(n)[:, None], sel]
    return px.reshape(h // 4, w // 4, 4, 4, 3).transpose(0, 2, 1, 3, 4).reshape(h, w, 3).astype(np.uint8)


def bc1_encode(img):
    h, w = img.shape[:2]
    blocks = img.reshape(h // 4, 4, w // 4, 4, 3).transpose(0, 2, 1, 3, 4).reshape(-1, 16, 3).astype(np.float32)
    mean = blocks.mean(1, keepdims=True)
    centred = blocks - mean
    cov = np.einsum("nki,nkj->nij", centred, centred)
    axis = np.ones((len(blocks), 3), np.float32)
    for _ in range(8):
        axis = np.einsum("nij,nj->ni", cov, axis)
        axis /= np.maximum(np.linalg.norm(axis, axis=1, keepdims=True), 1e-6)
    t = np.einsum("nki,ni->nk", centred, axis)
    lo = mean[:, 0] + t.min(1)[:, None] * axis
    hi = mean[:, 0] + t.max(1)[:, None] * axis

    def q565(c):
        c = np.clip(np.round(c), 0, 255).astype(np.int32)
        return (c[:, 0] * 31 + 127) // 255 << 11 | (c[:, 1] * 63 + 127) // 255 << 5 | (c[:, 2] * 31 + 127) // 255

    c0, c1 = q565(hi), q565(lo)
    swap = c0 < c1
    c0, c1 = np.where(swap, c1, c0), np.where(swap, c0, c1)
    p0, p1 = rgb565(c0).astype(np.float32), rgb565(c1).astype(np.float32)
    pal = np.stack([p0, p1, (2 * p0 + p1) / 3, (p0 + 2 * p1) / 3], 1)
    idx = ((blocks[:, :, None, :] - pal[:, None]) ** 2).sum(-1).argmin(-1)
    idx = np.where((c0 == c1)[:, None], 0, idx)
    bits = (idx.astype(np.uint32) << (2 * np.arange(16, dtype=np.uint32))).sum(1).astype(np.uint32)
    out = np.empty((len(blocks), 8), np.uint8)
    out[:, 0], out[:, 1], out[:, 2], out[:, 3] = c0 & 255, c0 >> 8, c1 & 255, c1 >> 8
    for i in range(4):
        out[:, 4 + i] = (bits >> 8 * i) & 255
    return out.tobytes()


def side_levels(f2):
    at = 1 + f2[0] + 1
    n = struct.unpack_from("<I", f2, at)[0]
    return at, [struct.unpack_from("<BBQQQ", f2, at + 4 + 26 * i) for i in range(n)]


def full_res(stock, name):
    head, f1, f2 = fragments(stock.path(HQ), TEX + name + ".tif.image")
    _, _, fmt, w, h, _, mips = struct.unpack_from("<7I", head)
    if fmt not in (52, 54):
        sys.exit(f"{name}: format {fmt}, not BC1")
    at, levels = side_levels(f2)
    path = bytes(f2[1:1 + f2[0]]).rstrip(b"\0").decode()
    raw = stock.path(path).read_bytes()
    _, _, off, packed, size = levels[0]
    top = lz4.block.decompress(raw[off:off + packed], uncompressed_size=size)
    side = w << len(levels)
    return bc1_decode(top, side, side), (head, f1, f2, path, len(levels), w)


def logo_box(mask, cell=32, reach=320):
    h, w = mask.shape
    counts = mask[:h - h % cell, :w - w % cell].reshape(h // cell, cell, w // cell, cell).sum((1, 3))
    cy, cx = np.unravel_index(counts.argmax(), counts.shape)
    cy, cx = (cy + 0.5) * cell, (cx + 0.5) * cell
    ys, xs = np.where(mask)
    near = (np.abs(ys - cy) < reach) & (np.abs(xs - cx) < reach)
    ys, xs = ys[near], xs[near]
    return xs.min(), ys.min(), xs.max() + 1, ys.max() + 1


def dress(base, ref, colour, token, mirror):
    b, r = base.astype(np.int32), ref.astype(np.int32)
    cover = np.abs(r - b).max(-1) > 40
    white = (r.min(-1) > 200) & cover
    lb = (0.299 * b[..., 0] + 0.587 * b[..., 1] + 0.114 * b[..., 2])
    shade = lb / max(np.median(lb[cover]), 1)
    tinted = np.clip(np.array(colour, np.float32) * shade[..., None], 0, 255)
    out = np.where(cover[..., None], tinted, b).astype(np.uint8)
    img = Image.fromarray(out).convert("RGBA")
    l, t, rr, bb = logo_box(white)
    kind = "lockup_light" if lum(colour) > 0.6 or token in KEEP_LIGHT else "lockup_dark"
    src = art(token, kind)
    bw, bh = (rr - l) * 1.1, (bb - t) * 1.25
    cap = (0.45 * bw * bh / (src.width * src.height)) ** 0.5
    logo = fit(src, min(bw, src.width * cap), min(bh, src.height * cap))
    if mirror:
        logo = logo.transpose(Image.FLIP_LEFT_RIGHT)
    img.alpha_composite(logo, (round((l + rr - logo.width) / 2), round((t + bb - logo.height) / 2)))
    return np.asarray(img.convert("RGB"))


def mip_chain(img, count):
    levels, im = [], Image.fromarray(img)
    for i in range(count):
        size = img.shape[1] >> i
        levels.append(bc1_encode(np.asarray(im.resize((size, size), Image.BOX) if i else im)))
    return levels


def build(stock, preview=None):
    teams = [(s.team, grid.team_row(s.team)) for s in slots()]
    if OUT.exists():
        for p in sorted(OUT.rglob("*"), reverse=True):
            p.unlink() if p.is_file() else p.rmdir()
    (OUT / "mipmaps").mkdir(parents=True)
    ops, sheet = [], []
    for prop, spec in PROPS.items():
        base, (head, f1, f2, side_path, n_side, w) = full_res(stock, f"{prop}_01_d")
        ref, _ = full_res(stock, f"{prop}_{spec['stock']}_d")
        srv = fragments(stock.path(HQ), TEX + f"{prop}_01_d.tif")[0]
        material = fragments(stock.path(HQ), MATERIALS + f"{prop}_01.material")[0]
        bind = fragments(stock.path(HQ), IDF + f"{prop}.emb?context=default")[0]
        at = side_levels(f2)[0]
        mips = struct.unpack_from("<I", head, 24)[0]
        (OUT / f"{prop}.head").write_bytes(head)
        for (team, row), code in zip(teams, IDS):
            token, new = row[:-len("_1990")], f"g{code}"
            colour = rgb(grid.TEAM_INFO[team][3][0] if grid.TEAM_INFO[team][3][0] != "#FFFFFF"
                         else grid.TEAM_INFO[team][3][1])
            img = dress(base, ref, colour, token, spec["mirror"])
            sheet.append(img)
            levels = mip_chain(img, n_side + mips)
            body = b"".join(levels[n_side:])
            if len(body) != len(f1):
                sys.exit(f"{prop}: {len(body)} bytes of in-archive mips, stock {len(f1)}")
            new_f2 = bytearray(same_length(f2, f"{prop}_01_d.tif", f"{prop}_{new}_d.tif"))
            side, off = b"", 0
            for i, lvl in enumerate(levels[:n_side]):
                packed = lz4.block.compress(lvl, store_size=False, mode="high_compression")
                new_f2[at + 4 + 26 * i:at + 4 + 26 * (i + 1)] = struct.pack("<BBQQQ", 0, 3, off, len(packed), len(lvl))
                side += packed
                off += len(packed)
            new_side = bytes(new_f2[1:1 + new_f2[0]]).rstrip(b"\0").decode()
            (OUT / "mipmaps" / new_side[len(TEX_DIR):]).write_bytes(side)
            stem = f"{prop}_{new}"
            files = {
                "f1": body, "f2": bytes(new_f2),
                "srv": same_length(srv, f"{prop}_01_d.tif", f"{stem}_d.tif"),
                "material": same_length(material, f"{prop}_01_d.tif", f"{stem}_d.tif"),
                "bind": same_length(bind, f"{prop}_01.material", f"{stem}.material"),
            }
            for ext, data in files.items():
                (OUT / f"{stem}.{ext}").write_bytes(data)
            src = lambda ext: f"erp/hq_logos/{stem}.{ext}"
            ops += [
                {"op": "add-resource", "erp": HQ, "resource": TEX + f"{stem}_d.tif.image", "type": "GfxSurfaceRes",
                 "sources": [f"erp/hq_logos/{prop}.head", src("f1"), src("f2")]},
                {"op": "add-resource", "erp": HQ, "resource": TEX + f"{stem}_d.tif", "type": "GfxSRVResource",
                 "sources": [src("srv")]},
                {"op": "add-resource", "erp": HQ, "resource": MATERIALS + f"{stem}.material", "type": "GfxMaterialRes",
                 "sources": [src("material")]},
                {"op": "add-resource", "erp": HQ, "resource": IDF + f"{prop}.emb?context={YEAR}-{row}",
                 "type": "GfxMatBindRes", "sources": [src("bind")]},
            ]
    ops.append({"op": "add-mipmaps", "source": "erp/hq_logos/mipmaps/", "target": TEX_DIR})
    (ROOT / "erp/hq_logos.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    if preview:
        cols = len(teams)
        size = 256
        out = Image.new("RGB", (cols * size, len(PROPS) * size))
        for i, img in enumerate(sheet):
            out.paste(Image.fromarray(img).resize((size, size), Image.LANCZOS), ((i % cols) * size, (i // cols) * size))
        out.save(preview)
    return len(ops)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path)
    ap.add_argument("--preview", type=Path)
    a = ap.parse_args(argv[1:])
    print(build(Stock(a.game, a.baseline), a.preview), "ops")


if __name__ == "__main__":
    main(sys.argv)
