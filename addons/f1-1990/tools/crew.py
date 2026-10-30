#!/usr/bin/env python3
"""Writes erp/crew.json and erp/crew/ (build output, not committed): 1990 team kit for the pit and garage crews.

The garage crews ask the 2026 crew livery archive for the material context `2026-<livery_name>` of their team's row
(`racecrew_kitpart.emb` of the male and female crew, `headphones_02.emb`) and fall back to `default`. For each 1990
team this adds those three contexts to crew_livery.erp: copies of the 2026 Williams binds and of its shirt, trousers,
shorts, trainers, rain jacket and headphone materials, under team_livery/<label> (the label of tools/kit.py), each
pointing at a diffuse texture of its own. The normal and specular maps stay the Williams ones. No stock resource
changes.

The diffuse textures are the neutral `noyear-custom-classic01` crew textures, recoloured: the shirt in the team's
shirt colour with the shoulders, sleeves and upper chest in its panel colour and the team lockup on the chest and
back, with the stock shirt's light and shade kept faintly, the trousers, shorts, trainers and rain jacket in one
colour each, the headphone cups in the shirt colour. All levels are in the archive (shirt 1024, the rest 512).

usage: crew.py --game <F1 25 folder> [--baseline <launcher baseline>/<build>] [--sheet <png>]
"""
import argparse
import json
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/classicseasons/classic-seasons/tools")]

import bcn
import erpw
import grid
import kit
from season import slots
from stock import Stock

ERP = "2025_asset_groups/animation_package/scenes/garagecrew/2026/crew_livery.erp"
BASE = "2026-williams"
SOURCE = "noyear-custom-classic01"
LIVERY = "eaid://character_package/team_livery/"
BINDS = ["eaid://character_package/racecrew/male/idf/racecrew_kitpart.emb",
         "eaid://character_package/racecrew/female/idf/racecrew_kitpart.emb",
         "eaid://character_package/kitpart_assets/generic/headwear/headphones_02/idf/headphones_02.emb"]
PARTS = {
    "racecrew_shirt": ("racecrew_shirt", 1024),
    "racecrew_trousers": ("racecrew_trousers", 512),
    "racecrew_shorts": ("racecrew_shorts", 512),
    "racecrew_trainers": ("racecrew_trainers", 512),
    "rainjacket": ("racecrew_rainjacket", 512),
    "racecrew_headphones": ("racecrew_headphones", 512),
}
OUT = ROOT / "erp" / "crew"

RED, WHITE, BLACK = kit.RED, kit.WHITE, kit.BLACK
CREW = {
    "mclaren": (RED, WHITE, WHITE, RED),
    "ferrari": ("#C4161C", WHITE, "#C4161C", BLACK),
    "williams": (WHITE, "#1B3F8B", "#1B3F8B", "#1B3F8B"),
    "benetton": ("#1F9A48", "#1E4FA0", "#1E4FA0", "#1E4FA0"),
    "tyrrell": ("#1F2F6B", WHITE, "#1F2F6B", "#1F2F6B"),
    "lotus": ("#F2C500", "#1F4E9C", "#1F4E9C", "#1F4E9C"),
    "march": ("#4FD3D9", "#1B2A5C", "#1B2A5C", "#1B2A5C"),
    "larrousse": (WHITE, "#3E9A4E", "#3E9A4E", "#1F3C88"),
    "ligier_prost": ("#0F5FDD", WHITE, "#0F5FDD", "#0F5FDD"),
    "minardi": (WHITE, "#F7AC15", BLACK, BLACK),
    "brabham": (WHITE, "#2A3052", "#2A3052", "#2A3052"),
    "dallara": ("#960D04", WHITE, BLACK, BLACK),
    "arrows": (WHITE, "#C8102E", "#1A3C8C", "#1A3C8C"),
}
LOGOS = [(0.25, 0.56, 0.17, 0.07), (0.75, 0.56, 0.3, 0.12)]


def surface(e, name):
    r = e.find(name)
    if r is None:
        sys.exit(f"{name}: not in {ERP}")
    return [e.frag_data(r, i) for i in range(len(r.frags))]


def frag0(e, name):
    return surface(e, name)[0]


def swap(data, old, new):
    old, new = old.encode(), new.encode()
    if len(old) != len(new):
        sys.exit(f"cannot swap {old!r} for {new!r}")
    if old not in data:
        sys.exit(f"{old!r} not found")
    return data.replace(old, new)


def luma(img):
    f = img[..., :3].astype(np.float32) / 255
    return 0.299 * f[..., 0] + 0.587 * f[..., 1] + 0.114 * f[..., 2]


def shading(lum, where):
    blur = np.asarray(Image.fromarray((lum * 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(48)),
                      np.float32) / 255
    med = np.median(blur[where]) if where.any() else 1.0
    return np.clip(blur / max(med, 1e-3), 0.96, 1.02)


def paint(src, size, fills):
    img = np.asarray(Image.fromarray(src).convert("RGB").resize((size, size), Image.LANCZOS))
    lum = luma(img)
    out = img.astype(np.float32)
    for mask, colour in fills(lum):
        out[mask] = (kit.rgb(colour) * shading(lum, mask)[..., None])[mask]
    return np.clip(out, 0, 255).astype(np.uint8)


def shirt_fills(shirt, panel):
    def fills(lum):
        h, w = lum.shape
        v = (np.arange(h)[:, None] + 0.5) / h
        top = np.broadcast_to(v < 0.46, lum.shape)
        return [(~top, shirt), (top, panel)]
    return fills


def flat_fill(colour):
    return lambda lum: [(np.ones(lum.shape, bool), colour)]


def cup_fill(colour):
    return lambda lum: [(lum > 0.6, colour)]


def with_logos(img, team, colour):
    art = kit.logo_art(team)
    pick = art["dark" if kit.luma(kit.rgb(colour)) < 0.5 else "light"]
    out = Image.fromarray(img).convert("RGBA")
    s = img.shape[0]
    for cx, cy, w, h in LOGOS:
        logo = kit.trim(pick)
        k = min(w * s / logo.width, h * s / logo.height)
        logo = logo.resize((max(1, round(logo.width * k)), max(1, round(logo.height * k))), Image.LANCZOS)
        out.alpha_composite(logo, (round(cx * s - logo.width / 2), round(cy * s - logo.height / 2)))
    return np.asarray(out.convert("RGB"))


def textures(e, game, team):
    shirt, panel, trousers, shoes = CREW[team]
    src = {p: bcn.top_level(surface(e, f"{LIVERY}{SOURCE}/textures/{SOURCE}_{p}_d.tif.image"), game)
           for p in PARTS}
    return {
        "racecrew_shirt": with_logos(paint(src["racecrew_shirt"], 1024, shirt_fills(shirt, panel)), team, shirt),
        "racecrew_trousers": paint(src["racecrew_trousers"], 512, flat_fill(trousers)),
        "racecrew_shorts": paint(src["racecrew_shorts"], 512, flat_fill(trousers)),
        "racecrew_trainers": paint(src["racecrew_trainers"], 512, flat_fill(shoes)),
        "rainjacket": paint(src["rainjacket"], 512, flat_fill(panel if panel != WHITE else shirt)),
        "racecrew_headphones": paint(src["racecrew_headphones"], 512, cup_fill(shirt)),
    }


def texture_frags(e, part, label, img):
    old = f"{LIVERY}{BASE}/textures/{BASE}_{part}_d.tif"
    head0 = frag0(e, old + ".image")
    fmt = bcn.Surface.header(head0)[0]
    rgba = np.dstack([img, np.full(img.shape[:2], 255, np.uint8)])
    head, data, levels = bcn.surface_frags(head0, fmt, rgba)
    srv = bytearray(swap(frag0(e, old), BASE, label))
    struct.pack_into("<I", srv, 4, fmt)
    struct.pack_into("<I", srv, 12, levels)
    return head, data, bytes(srv)


def build(stock, sheet=None):
    erp_path = stock.path(ERP)
    e = erpw.Erp(str(erp_path))
    game = str(stock.game)
    if OUT.exists():
        for p in sorted(OUT.rglob("*"), reverse=True):
            p.unlink() if p.is_file() else p.rmdir()
    OUT.mkdir(parents=True, exist_ok=True)
    ops, rows = [], []
    for s in slots():
        team, row = s.team, grid.team_row(s.team)
        label = kit.label(team)
        tex = textures(e, game, team)
        rows.append((team, label, tex))
        folder = OUT / label
        folder.mkdir()

        def put(name, kind, frags):
            stem = name.split("/")[-1].replace("?", "_").replace("=", "_")
            paths = []
            for i, data in enumerate(frags):
                path = folder / f"{stem}.{i}"
                path.write_bytes(data)
                paths.append(str(path.relative_to(ROOT)))
            ops.append({"op": "add-resource", "erp": ERP, "resource": name, "type": kind, "sources": paths})

        for part, img in tex.items():
            head, data, srv = texture_frags(e, part, label, img)
            name = f"{LIVERY}{label}/textures/{label}_{part}_d.tif"
            put(name + ".image", "GfxSurfaceRes", [head, data])
            put(name, "GfxSRVResource", [srv])
        for part, (material, _) in PARTS.items():
            old = f"{LIVERY}{BASE}/materials/{material}.material"
            data = swap(frag0(e, old), f"{BASE}/textures/{BASE}_{part}_d.tif", f"{label}/textures/{label}_{part}_d.tif")
            put(f"{LIVERY}{label}/materials/{material}.material", "GfxMaterialRes", [data])
        for bind in BINDS:
            data = frag0(e, f"{bind}?context={BASE}")
            for material, _ in PARTS.values():
                old = f"{BASE}/materials/{material}.material"
                if old.encode() in data:
                    data = swap(data, old, f"{label}/materials/{material}.material")
            if BASE.encode() in data.replace(f"{BASE}/materials/racecrew_shirt_logo".encode(), b"").replace(
                    f"{BASE}/materials/racecrew_rainjacket_logo".encode(), b""):
                sys.exit(f"{bind}: {BASE} left after the swap")
            put(f"{bind}?context=2026-{row}", "GfxMatBindRes", [data])
    (ROOT / "erp" / "crew.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    if sheet:
        contact_sheet(rows, sheet)
    return len(ops)


def font(size):
    for f in ("/run/current-system/sw/share/X11/fonts/DejaVuSans.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            continue
    return ImageFont.load_default()


def contact_sheet(rows, out):
    cell, big = 200, 400
    order = ["racecrew_trousers", "racecrew_shorts", "racecrew_trainers", "rainjacket", "racecrew_headphones"]
    w = big + 3 * cell + 40
    img = Image.new("RGB", (2 * w + 20, 60 + ((len(rows) + 1) // 2) * (big + 50)), (30, 31, 35))
    d = ImageDraw.Draw(img)
    d.text((20, 16), "F1 1990 crew kit (shirt; trousers, shorts, trainers, rain jacket, headphones)",
           fill=(235, 235, 235), font=font(26))
    for k, (team, label, tex) in enumerate(rows):
        x, y = 10 + (k % 2) * w, 60 + (k // 2) * (big + 50)
        d.text((x + 10, y + 4), f"{team}  ·  2026-{grid.team_row(team)}  ·  {label}", fill=(220, 220, 220),
               font=font(18))
        y += 34
        img.paste(Image.fromarray(tex["racecrew_shirt"]).resize((big, big), Image.LANCZOS), (x, y))
        for j, part in enumerate(order):
            img.paste(Image.fromarray(tex[part]).resize((cell, cell), Image.LANCZOS),
                      (x + big + 10 + (j % 3) * cell, y + (j // 3) * cell))
    img.save(out, optimize=True)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path)
    ap.add_argument("--sheet", type=Path)
    a = ap.parse_args(argv[1:])
    print(build(Stock(a.game, a.baseline), a.sheet), "ops")


if __name__ == "__main__":
    main(sys.argv)
