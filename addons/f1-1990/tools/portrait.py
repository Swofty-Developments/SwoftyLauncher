#!/usr/bin/env python3
"""Writes erp/portrait.json and erp/portrait/portrait.erpk: Flavio Briatore's message avatar as a new slice
`message_flavio` of the character message-avatar texture array. Elena's slice and every stock slice stay as they are.

usage: portrait.py --game <F1 25> --source <photo> [--baseline <launcher baseline>/<build>] [--preview <png>]

Needs Pillow, numpy and OpenCV (nix-shell -p "python3.withPackages (ps: [ ps.pillow ps.numpy ps.opencv4 ])").
"""
import argparse
import glob
import hashlib
import json
import os
import struct
import subprocess
import sys
from functools import lru_cache
from pathlib import Path

import cv2
import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/rnd-tyres-thermal/tools")]

import bc7enc  # noqa: E402
from erp import parse as erp_parse  # noqa: E402
from erpw import Erp  # noqa: E402
from pkg import Pkg  # noqa: E402

UI = "2025_asset_groups/ui_package/"
DATABASES = UI + "databases.erp"
OVERRIDES = "eaid://ui_package/databases/overrides.uidb"
ARRAY = "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.{}.texarray"
BC7 = 70

LIST = "characters.message_avatars"
NAME = "message_flavio"
STOCK_AGENT = "message_elena"
TEXTURE = "ui_package/art/textures/f1990/characters/message_flavio.tif"
OUT = ROOT / "erp/portrait"
SIZE = 256
CROP_X, CROP_SIZE = 945, 760
HEAD = (190, 118, 88, 112)


class Stock:
    def __init__(self, game, baseline):
        self.game = game
        self.files, self.objects = {}, None
        if baseline:
            self.files = json.loads((baseline / "baseline.json").read_text())["files"]
            self.objects = baseline.parent / "objects"

    @lru_cache(None)
    def path(self, rel):
        p = self.game / rel
        entry = self.files.get(rel)
        if entry and hashlib.sha256(p.read_bytes()).hexdigest() != entry["sha256"]:
            obj = self.objects / entry["sha256"][:2] / entry["sha256"]
            if not obj.exists():
                sys.exit(f"{rel}: changed in the game folder and no stock copy in the baseline")
            return obj
        return p


def fragments(path, name):
    meta, res = erp_parse(str(path))
    r = {x[0]: x for x in res}[name]
    out = []
    with open(path, "rb") as f:
        for _, off, _, _, comp, packed in r[3]:
            f.seek(meta["data_off"] + off)
            raw = f.read(packed)
            if comp == 0x11:
                raw = subprocess.run(["zstd", "-dc"], input=raw, capture_output=True, check=True).stdout
            out.append(raw)
    return out


def carriers(game, names):
    found = {n: [] for n in names}
    for e in sorted(glob.glob(str(game / UI / "*.erp")) + glob.glob(str(game / UI / "screens/*.erp"))):
        for r in erp_parse(e)[1]:
            if r[0] in found:
                found[r[0]].append(os.path.relpath(e, game))
    return found


def overrides_list(root, path):
    node = root["overrides"]["Contents"]
    parts = path.split(".")
    for p in parts[:-1]:
        node = node[p]["Contents"]
    return [e["Name"].value for e in node[parts[-1]]["Contents"].items]


def cutout(source):
    src = cv2.imread(str(source))
    x0 = CROP_X - CROP_SIZE // 2
    crop = src[0:CROP_SIZE, x0:x0 + CROP_SIZE].astype(np.float32)
    n = 380
    small = cv2.resize(crop, (n, n), interpolation=cv2.INTER_AREA)
    yy, xx = np.mgrid[0:n, 0:n]
    cx, cy, rx, ry = HEAD
    head = ((xx - cx) / rx) ** 2 + ((yy - cy) / ry) ** 2 <= 1

    mask = np.full((n, n), cv2.GC_PR_BGD, np.uint8)
    mask[head] = cv2.GC_PR_FGD
    mask[yy > 250] = cv2.GC_PR_FGD
    mask[:6, :] = cv2.GC_BGD
    mask[(xx < 95) & (yy < 230)] = cv2.GC_BGD
    mask[(xx > 285) & (yy < 230)] = cv2.GC_BGD
    bgm, fgm = np.zeros((1, 65)), np.zeros((1, 65))
    cv2.grabCut(small.astype(np.uint8), mask, None, bgm, fgm, 10, cv2.GC_INIT_WITH_MASK)
    body = np.where((mask == 1) | (mask == 3), 1.0, 0.0).astype(np.float32)
    body = cv2.morphologyEx(body, cv2.MORPH_CLOSE, np.ones((9, 9), np.uint8))

    near = cv2.dilate(body, np.ones((41, 41), np.uint8)) > 0
    sure_bg = ~near & ~(((xx - cx) / (rx * 1.35)) ** 2 + ((yy - cy) / (ry * 1.25)) ** 2 <= 1) & (yy < 300)
    pts = np.argwhere(sure_bg)
    py, px = pts[:, 0] / n, pts[:, 1] / n
    basis = np.stack([np.ones_like(px), px, py, px * px, py * py, px * py], 1)
    gy, gx = yy / n, xx / n
    full = np.stack([np.ones_like(gx), gx, gy, gx * gx, gy * gy, gx * gy], -1)
    plate = np.zeros_like(small)
    for c in range(3):
        coef, *_ = np.linalg.lstsq(basis, small[sure_bg][:, c], rcond=None)
        plate[:, :, c] = full @ coef
    diff = np.abs(cv2.GaussianBlur(small, (3, 3), 0) - plate).max(2)
    hair = np.clip((diff - 9) / 16, 0, 1)
    halo = cv2.dilate(head.astype(np.uint8), np.ones((25, 25), np.uint8)) > 0
    hair[~halo] = 0
    alpha = np.maximum(cv2.GaussianBlur(body, (5, 5), 0), hair)

    big = cv2.resize(small, (SIZE, SIZE), interpolation=cv2.INTER_AREA)
    plate_big = cv2.resize(plate, (SIZE, SIZE), interpolation=cv2.INTER_AREA)
    a = cv2.resize(alpha, (SIZE, SIZE), interpolation=cv2.INTER_AREA)
    ys = np.arange(SIZE)[:, None] / SIZE
    xs = np.arange(SIZE)[None, :] / SIZE
    a *= np.clip((0.98 - ys) / 0.20, 0, 1)
    a *= 1 - np.clip((ys - 0.66) / 0.16, 0, 1) * (1 - np.clip((0.47 - np.abs(xs - 0.5)) / 0.14, 0, 1))
    safe = np.maximum(a, 0.05)[:, :, None]
    rgb = np.clip((big - (1 - a)[:, :, None] * plate_big) / safe, 0, 255)
    rgb = np.where(a[:, :, None] > 0.02, rgb, big)
    out = np.dstack([rgb[:, :, ::-1], a * 255]).round().astype(np.uint8)
    return Image.fromarray(out, "RGBA")


def build(game, baseline, source, preview):
    stock = Stock(game, baseline)
    image, view = ARRAY.format(LIST) + ".image", ARRAY.format(LIST)
    where = carriers(game, [image, view])
    if not where[image] or where[image] != where[view]:
        sys.exit(f"{LIST}: carriers {where[image]} / {where[view]}")
    frags = None
    for rel in where[image]:
        got = fragments(stock.path(rel), image) + fragments(stock.path(rel), view)
        if frags is None:
            frags = got
        elif got != frags:
            sys.exit(f"{LIST}: {rel} carries a different stock array")
    head, data, view0 = frags
    fmt, w, h = struct.unpack_from("<III", head, 8)
    mips, count = struct.unpack_from("<II", head, 24)
    root = Pkg(fragments(stock.path(DATABASES), OVERRIDES)[0]).root
    names = overrides_list(root, LIST)
    if fmt != BC7 or (w, h) != (SIZE, SIZE) or mips != 1 or len(data) != count * w * h or len(names) != count \
            or struct.unpack_from("<I", view0, 0x14)[0] != count or NAME in names or STOCK_AGENT not in names:
        sys.exit(f"{LIST}: unexpected stock array (format {fmt}, {w}x{h}, {mips} mips, {count} slices)")

    im = cutout(source)
    px = bc7enc.encode_rgba(im.tobytes(), w, h)
    head2, view2 = bytearray(head), bytearray(view0)
    struct.pack_into("<I", head2, 28, count + 1)
    struct.pack_into("<I", view2, 0x14, count + 1)

    OUT.mkdir(parents=True, exist_ok=True)
    e = Erp(str(stock.path(where[image][0])))
    r_image, r_view = e.find(image), e.find(view)
    e.set_frag(r_image, 0, bytes(head2))
    e.set_frag(r_image, 1, bytes(data) + px)
    e.set_frag(r_view, 0, bytes(view2))
    e.res = [r_image, r_view]
    e.to_file(str(OUT / "portrait.erpk"))

    ops = [{"op": "put-resource", "erp": rel, "resource": res, "source": "erp/portrait/portrait.erpk"}
           for rel in where[image] for res in (image, view)]
    at = "overrides.Contents." + ".Contents.".join(LIST.split(".")) + ".Contents"
    ops.append({"op": "patch-pkg", "erp": DATABASES, "resource": OVERRIDES, "fragment": 0, "edits": [
        {"op": "clone-entry", "at": at, "src": f"[Name={names[-1]}]", "as": NAME, "set": {"Value": TEXTURE}}]})
    (ROOT / "erp/portrait.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")

    if preview:
        from common import bc7_decode
        stock_im = Image.frombytes("RGBA", (w, h), bytes(bc7_decode(data[names.index(STOCK_AGENT) * w * h:
                                                                         (names.index(STOCK_AGENT) + 1) * w * h], w, h)))
        new_im = Image.frombytes("RGBA", (w, h), bytes(bc7_decode(px, w, h)))
        sheet = Image.new("RGBA", (w * 4 + 50, h + 20), (38, 40, 46, 255))
        for i, cell in enumerate((stock_im, new_im)):
            sheet.alpha_composite(cell, (10 + i * (w + 10), 10))
        light = Image.new("RGBA", (w, h), (225, 225, 225, 255))
        for i, cell in enumerate((stock_im, new_im)):
            bg = light.copy()
            bg.alpha_composite(cell)
            sheet.alpha_composite(bg, (30 + (2 + i) * (w + 10) - 20, 10))
        sheet.convert("RGB").save(preview)
    return [f"{LIST}: {w}x{h} BC7, {count} slices + {NAME} at {count}, {len(where[image])} archive(s): "
            + ", ".join(where[image])]


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--source", required=True, type=Path)
    ap.add_argument("--baseline", type=Path)
    ap.add_argument("--preview", type=Path)
    a = ap.parse_args(argv[1:])
    for line in build(a.game, a.baseline, a.source, a.preview):
        print(line)


if __name__ == "__main__":
    main(sys.argv)
