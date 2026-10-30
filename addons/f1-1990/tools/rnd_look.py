#!/usr/bin/env python3
"""Writes erp/rnd_look.json and erp/rnd_look/rnd_look.erpk: the 1990 R&D department icons as a uidb package of
their own, `overrides.career.f1990_rnd`, read only in 1990 careers (native/f1_1990_rndlook.dll points the department
icon paths at it).

The package is a clone of the stock `overrides.career.rnd_om_xm` package. Four of its lists get new texture arrays
of their own, so no stock array or list changes and nothing collides with arrays another addon grows:

  rnd_dept_icons_small   48 x 48, white, main shape alpha 255 and secondary alpha 128 (the game tints them)
  rnd_wheel_icon         60 x 60 in the department colour; regulation and shutdown are the stock slices
  rnd_department_strips  1112 x 4 solid department colour
  dept_card_icons        148 x 148 in the department colour with a soft glow

Each list holds aero, chassis, maintenance, engine, tyres and error_department (the frame the game names for
department 4), in that order; rnd_wheel_icon also keeps regulation and shutdown. The icons are art/rnd/*.svg, the
colours data/rnd-look.json.

usage: rnd_look.py --game <F1 25 or a folder with the stock UI archives> [--baseline <launcher baseline>/<build>]
                   [--preview <png>]

Needs Pillow, numpy, resvg (or nix to fetch it), zstd and node.
"""
import argparse
import copy
import glob
import hashlib
import json
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/rnd-tyres-thermal/tools")]

import bc7enc  # noqa: E402
from erpw import Erp  # noqa: E402
from erp import parse as erp_parse  # noqa: E402
from pkg import Pkg  # noqa: E402

UI = "2025_asset_groups/ui_package/"
DATABASES = UI + "databases.erp"
OVERRIDES = "eaid://ui_package/databases/overrides.uidb"
ARRAYS = "ui_package/art/texture_arrays/overrides.uidb/overrides.career."
BC7 = 70

PACKAGE = "f1990_rnd"
SOURCE_PACKAGE = "rnd_om_xm"
ART = ROOT / "art/rnd"
LOOK = ROOT / "data/rnd-look.json"
OUT = ROOT / "erp/rnd_look"
OPS = ROOT / "erp/rnd_look.json"
TIF = "ui_package/art/textures/f1990/rnd/{}/{}.tif"

DEPARTMENTS = ["aero", "chassis", "maintenance", "engine", "tyres"]
SVG = {"aero": "aero", "chassis": "chassis", "maintenance": "reliability", "engine": "engine", "tyres": "tyres"}
ERROR = "error_department"


class Stock:
    def __init__(self, game, baseline):
        self.game = game
        self.files, self.objects = {}, None
        if baseline:
            self.files = json.loads((baseline / "baseline.json").read_text())["files"]
            self.objects = baseline.parent / "objects"

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


def frames(kind):
    if kind == "rnd_department_strips":
        return [f"rnd_{d}_strip" for d in ("aero", "chassis", "engine", "maintenance", "tyres", ERROR)]
    if kind == "rnd_wheel_icon":
        return DEPARTMENTS[:4] + ["regulation", "shutdown", "tyres", ERROR]
    return DEPARTMENTS + [ERROR]


LISTS = [
    ("rnd_dept_icons_small", f"{SOURCE_PACKAGE}.rnd_dept_icons_small", ["aero", "chassis", "maintenance", "engine"]),
    ("rnd_wheel_icon", f"{SOURCE_PACKAGE}.rnd_wheel_icon",
     ["aero", "chassis", "maintenance", "engine", "regulation", "shutdown"]),
    ("rnd_department_strips", f"{SOURCE_PACKAGE}.rnd_department_strips",
     ["rnd_aero_strip", "rnd_chassis_strip", "rnd_engine_strip", "rnd_maintenance_strip"]),
    ("dept_card_icons", "dept_card_icons", ["aero", "chassis", "maintenance", "engine"]),
]


def department_of(frame):
    for d in DEPARTMENTS:
        if frame in (d, f"rnd_{d}_strip"):
            return d
    if ERROR in frame:
        return "tyres"
    return None


def look():
    data = json.loads(LOOK.read_text())
    if data["package"] != f"#overrides.career.{PACKAGE}":
        sys.exit(f"{LOOK}: package {data['package']} is not #overrides.career.{PACKAGE}")
    out = {}
    for code, d in data["departments"].items():
        c = d["colour"].lstrip("#")
        out[d["name"]] = tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))
    if sorted(out) != sorted(DEPARTMENTS):
        sys.exit(f"{LOOK}: departments {sorted(out)}")
    return out


def resvg():
    for d in os.environ.get("PATH", "").split(os.pathsep):
        p = os.path.join(d, "resvg")
        if os.access(p, os.X_OK):
            return [p]
    return ["nix", "run", "nixpkgs#resvg", "--"]


def raster(name, size):
    with tempfile.TemporaryDirectory() as d:
        dst = os.path.join(d, "icon.png")
        subprocess.run(resvg() + ["-w", str(size), "-h", str(size), str(ART / f"{name}.svg"), dst], check=True,
                       capture_output=True)
        return np.array(Image.open(dst).convert("RGBA"))


def coloured(alpha, rgb):
    a = np.zeros(alpha.shape + (4,), np.uint8)
    a[..., :3] = rgb
    a[..., 3] = alpha
    return a


def glow_card(name, size, rgb):
    inner = round(size * 0.78)
    icon = raster(name, inner)[..., 3]
    canvas = np.zeros((size, size), np.uint8)
    o = (size - inner) // 2
    canvas[o:o + inner, o:o + inner] = icon
    halo = Image.fromarray(canvas).filter(ImageFilter.GaussianBlur(size * 0.045))
    halo = np.asarray(halo).astype(np.float32) * 0.55
    top = canvas.astype(np.float32) / 255.0
    alpha = top * 255.0 + (1.0 - top) * halo
    return coloured(np.clip(np.rint(alpha), 0, 255).astype(np.uint8), rgb)


def slice_pixels(kind, frame, w, h, colours, stock_slices, stock_names):
    dept = department_of(frame)
    if dept is None:
        return stock_slices[stock_names.index(frame)], None
    rgb = colours[dept]
    if kind == "rnd_dept_icons_small":
        rgba = coloured(raster(SVG[dept], w)[..., 3], (255, 255, 255))
    elif kind == "rnd_wheel_icon":
        rgba = coloured(raster(SVG[dept], w)[..., 3], rgb)
    elif kind == "rnd_department_strips":
        rgba = coloured(np.full((h, w), 255, np.uint8), rgb)
    else:
        rgba = glow_card(SVG[dept], w, rgb)
    raw = rgba.tobytes()
    px = bc7enc.encode_rgba(raw, w, h)
    return px, raw


def list_node(root, path):
    node = root["overrides"]["Contents"]["career"]["Contents"]
    parts = path.split(".")
    for p in parts[:-1]:
        node = node[p]["Contents"]
    return node[parts[-1]]


def view_bytes(stock_view, slices, image_name):
    if stock_view[:4] != b"\x06\x00\x00\x00" or stock_view[-1:] != b"\0":
        sys.exit(f"unexpected texture array view {stock_view[:24].hex()}")
    head = bytearray(stock_view[:0x18])
    struct.pack_into("<I", head, 0x14, slices)
    return bytes(head) + image_name.encode() + b"\0"


def build(game, baseline, preview):
    stock = Stock(game, baseline)
    colours = look()
    root = Pkg(fragments(stock.path(DATABASES), OVERRIDES)[0]).root
    career = root["overrides"]["Contents"]["career"]["Contents"]
    if PACKAGE in career.keys():
        sys.exit(f"overrides.career already has {PACKAGE}")
    resources, ops, report, sheet = [], [], [], []
    header = None
    for kind, src, want in LISTS:
        names = [e["Name"].value for e in list_node(root, src)["Contents"].items]
        if names != want:
            sys.exit(f"{src}: stock frames {names}, expected {want}")
        image, view = f"eaid://{ARRAYS}{src}.texarray.image", f"eaid://{ARRAYS}{src}.texarray"
        where = carriers(game, [image, view])
        if not where[image] or where[image] != where[view]:
            sys.exit(f"{src}: carriers {where[image]} / {where[view]}")
        got = None
        for rel in where[image]:
            frags = fragments(stock.path(rel), image) + fragments(stock.path(rel), view)
            if got is None:
                got = frags
            elif frags != got:
                sys.exit(f"{src}: {rel} carries a different stock array")
        head, data, view0 = got
        fmt, w, h = struct.unpack_from("<III", head, 8)
        mips, count = struct.unpack_from("<II", head, 24)
        if fmt != BC7 or mips != 1 or count != len(names) or len(data) != count * w * h \
                or struct.unpack_from("<I", view0, 0x14)[0] != count:
            sys.exit(f"{src}: unexpected stock array (format {fmt}, {w}x{h}, {mips} mips, {count} slices)")
        stock_slices = [data[i * w * h:(i + 1) * w * h] for i in range(count)]
        out_frames = frames(kind)
        pixels = []
        for frame in out_frames:
            px, raw = slice_pixels(kind, frame, w, h, colours, stock_slices, names)
            pixels.append(px)
            if raw is not None and kind != "rnd_department_strips":
                worst, mean, _ = bc7enc.round_trip(px, raw, w, h)
                report.append(f"  {kind}.{frame}: {w}x{h} BC7, round trip max {worst}/255 mean {mean:.2f}")
            sheet.append((kind, frame, w, h, px))
        new_image = f"eaid://{ARRAYS}{PACKAGE}.{kind}.texarray.image"
        new_view = f"eaid://{ARRAYS}{PACKAGE}.{kind}.texarray"
        head2 = bytearray(head)
        struct.pack_into("<I", head2, 28, len(out_frames))
        e = Erp(str(stock.path(where[image][0])))
        if header is None:
            header = e
        r_image, r_view = copy.deepcopy(e.find(image)), copy.deepcopy(e.find(view))
        r_image.name, r_view.name = new_image, new_view
        e.set_frag(r_image, 0, bytes(head2))
        e.set_frag(r_image, 1, b"".join(pixels))
        e.set_frag(r_view, 0, view_bytes(view0, len(out_frames), new_image))
        resources += [r_image, r_view]
        ops += [{"op": "put-resource", "erp": rel, "resource": res, "source": "erp/rnd_look/rnd_look.erpk"}
                for rel in where[image] for res in (new_image, new_view)]
        report.append(f"{PACKAGE}.{kind}: {w}x{h} BC7, {len(out_frames)} slices {out_frames}, in "
                      + ", ".join(os.path.basename(r) for r in where[image]))

    OUT.mkdir(parents=True, exist_ok=True)
    header.res = resources
    header.to_file(str(OUT / "rnd_look.erpk"))
    ops.append({"op": "patch-pkg", "erp": DATABASES, "resource": OVERRIDES, "fragment": 0, "edits": edits()})
    OPS.write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    if preview:
        contact_sheet(sheet, preview)
    return report


def edits():
    at = "overrides.Contents.career.Contents"
    pkg = f"{at}.{PACKAGE}.Contents"
    out = [{"op": "clone-entry", "at": at, "src": SOURCE_PACKAGE, "as": PACKAGE, "set": {"Name": PACKAGE}}]
    for kind, src, want in LISTS:
        if kind == "dept_card_icons":
            out.append({"op": "clone-entry", "at": pkg, "src": "rnd_dept_icons_small", "as": kind,
                        "set": {"Name": kind}})
        array = {"TextureArrayName": f"{ARRAYS}{PACKAGE}.{kind}.texarray"}
        out.append({"op": "set-value", "path": f"{pkg}.{kind}.TextureArrayName", "value": array["TextureArrayName"]})
        have = want if kind != "dept_card_icons" else frames("rnd_dept_icons_small")
        for frame in frames(kind):
            if frame not in have:
                out.append({"op": "clone-entry", "at": f"{pkg}.{kind}.Contents", "src": f"[Name={have[-1]}]",
                            "as": frame, "set": {"Value": TIF.format(kind, frame)}})
                have = have + [frame]
            else:
                out.append({"op": "set-value", "path": f"{pkg}.{kind}.Contents[Name={frame}].Value",
                            "value": TIF.format(kind, frame)})
        if have != frames(kind):
            sys.exit(f"{kind}: list order {have} does not match the slices {frames(kind)}")
    return out


def contact_sheet(sheet, path):
    from common import bc7_decode
    bg = (18, 21, 28, 255)
    cells = []
    for kind, frame, w, h, px in sheet:
        im = Image.frombytes("RGBA", (w, h), bytes(bc7_decode(px, w, h)))
        if kind == "rnd_department_strips":
            im = im.resize((148, 12), Image.NEAREST)
        cells.append((kind, im))
    rows = {}
    for kind, im in cells:
        rows.setdefault(kind, []).append(im)
    width = max(sum(i.width + 12 for i in r) for r in rows.values()) + 12
    height = sum(max(i.height for i in r) + 12 for r in rows.values()) + 12
    out = Image.new("RGBA", (width, height), bg)
    y = 12
    for r in rows.values():
        x = 12
        for im in r:
            out.alpha_composite(im, (x, y))
            x += im.width + 12
        y += max(i.height for i in r) + 12
    out.convert("RGB").save(path)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path)
    ap.add_argument("--preview", type=Path)
    a = ap.parse_args(argv[1:])
    for line in build(a.game, a.baseline, a.preview):
        print(line)
    print(f"wrote {OPS.relative_to(ROOT)} and {(OUT / 'rnd_look.erpk').relative_to(ROOT)}")


if __name__ == "__main__":
    main(sys.argv)
