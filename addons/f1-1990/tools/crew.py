#!/usr/bin/env python3
"""Writes erp/crew.json and erp/crew/ (build output, not committed): 1990 team kit for the garage and pit stop crews.

A 1990 car's crews ask for the material context `2026-<livery_name>` of its team row (the f1_2026 driver animation set's
`[YEAR]-[LIVERY]`) and fall back to `default`. The garage crew reads it from the 2026 crew livery archive
(`racecrew_kitpart.emb` of the male and female crew, `headphones_02.emb`), the pit stop crew from pitstop_26.erp
(`pitcrew.emb`), the vehicle package's `crew_livery_asset_group` and `pit_stop_asset_group`. For each 1990 team this
adds those contexts to both archives: copies of the 2026 Williams binds and of the materials that hold a diffuse
texture (garage: shirt, trousers, shorts, trainers, rain jacket, headphones; pit: overall, helmet, visor), under
team_livery/<label> (the label of tools/kit.py), each pointing at a diffuse texture of its own. The normal and specular
maps and the logo materials stay the Williams ones. No stock resource changes.

The diffuse textures are the neutral `noyear-custom-classic01` crew textures, recoloured: the garage shirt in the
team's shirt colour with the shoulders, sleeves and upper chest in its panel colour and the team lockup on the chest
and back, the trousers, shorts, trainers and rain jacket in one colour each, the headphone cups in the shirt colour;
the pit overall and helmet in the shirt colour with the stock trim lines in the panel colour and the lockup on the
chest. The stock light and shade stays faintly. All levels are in the archive (shirt and overall 1024, the rest 512).

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
    "racecrew_shirt": ["racecrew_shirt"],
    "racecrew_trousers": ["racecrew_trousers"],
    "racecrew_shorts": ["racecrew_shorts"],
    "racecrew_trainers": ["racecrew_trainers"],
    "rainjacket": ["racecrew_rainjacket"],
    "racecrew_headphones": ["racecrew_headphones"],
}
PIT_ERP = "2025_asset_groups/animation_package/scenes/pitstops/f1/pitstop_26.erp"
PIT_BINDS = ["eaid://character_package/pitcrew/idf/pitcrew.emb"]
PIT_PARTS = {"pitcrew": ["pitcrew"], "pitcrew_helmet": ["pitcrew_helmet", "pitcrew_visor"]}
PIT_LOGOS = [(0.33, 0.215, 0.2, 0.06)]
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
    size = size if isinstance(size, tuple) else (size, size)
    img = np.asarray(Image.fromarray(src).convert("RGB").resize(size, Image.LANCZOS))
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


def overall_fills(body, trim):
    return lambda lum: [(lum > 0.55, body), ((lum > 0.2) & (lum <= 0.55), trim)]


def with_logos(img, team, colour, places=LOGOS):
    art = kit.logo_art(team)
    pick = art["dark" if kit.luma(kit.rgb(colour)) < 0.5 else "light"]
    out = Image.fromarray(img).convert("RGBA")
    s = img.shape[0]
    for cx, cy, w, h in places:
        logo = kit.trim(pick)
        k = min(w * s / logo.width, h * s / logo.height)
        logo = logo.resize((max(1, round(logo.width * k)), max(1, round(logo.height * k))), Image.LANCZOS)
        out.alpha_composite(logo, (round(cx * s - logo.width / 2), round(cy * s - logo.height / 2)))
    return np.asarray(out.convert("RGB"))


def sources(e, game, parts):
    return {p: bcn.top_level(surface(e, f"{LIVERY}{SOURCE}/textures/{SOURCE}_{p}_d.tif.image"), game) for p in parts}


def pit_textures(e, game, team):
    shirt, panel, _, _ = CREW[team]
    src = sources(e, game, PIT_PARTS)
    overall = paint(src["pitcrew"], 1024, overall_fills(shirt, panel))
    return {"pitcrew": with_logos(overall, team, shirt, PIT_LOGOS),
            "pitcrew_helmet": paint(src["pitcrew_helmet"], (512, 256), overall_fills(shirt, panel))}


def textures(e, game, team):
    shirt, panel, trousers, shoes = CREW[team]
    src = sources(e, game, PARTS)
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


def dress(e, erp, binds, parts, tex, label, row, put):
    keep = [m for ms in parts.values() for m in ms]
    for part, img in tex.items():
        head, data, srv = texture_frags(e, part, label, img)
        name = f"{LIVERY}{label}/textures/{label}_{part}_d.tif"
        put(erp, name + ".image", "GfxSurfaceRes", [head, data])
        put(erp, name, "GfxSRVResource", [srv])
    for part, materials in parts.items():
        for material in materials:
            old = f"{LIVERY}{BASE}/materials/{material}.material"
            data = swap(frag0(e, old), f"{BASE}/textures/{BASE}_{part}_d.tif",
                        f"{label}/textures/{label}_{part}_d.tif")
            put(erp, f"{LIVERY}{label}/materials/{material}.material", "GfxMaterialRes", [data])
    for bind in binds:
        data = frag0(e, f"{bind}?context={BASE}")
        for material in keep:
            old = f"{BASE}/materials/{material}.material"
            if old.encode() in data:
                data = swap(data, old, f"{label}/materials/{material}.material")
        left = data
        for logo in ("racecrew_shirt_logo", "racecrew_rainjacket_logo", "pitcrew_logo"):
            left = left.replace(f"{BASE}/materials/{logo}.material".encode(), b"")
        if BASE.encode() in left:
            sys.exit(f"{bind}: {BASE} left after the swap")
        put(erp, f"{bind}?context=2026-{row}", "GfxMatBindRes", [data])


def build(stock, sheet=None):
    garage = erpw.Erp(str(stock.path(ERP)))
    pit = erpw.Erp(str(stock.path(PIT_ERP)))
    game = str(stock.game)
    if OUT.exists():
        for p in sorted(OUT.rglob("*"), reverse=True):
            p.unlink() if p.is_file() else p.rmdir()
    OUT.mkdir(parents=True, exist_ok=True)
    ops, rows = [], []
    for s in slots():
        team, row = s.team, grid.team_row(s.team)
        label = kit.label(team)
        tex, ptex = textures(garage, game, team), pit_textures(pit, game, team)
        rows.append((team, label, {**tex, **ptex}))
        folder = OUT / label
        folder.mkdir()

        def put(erp, name, kind, frags):
            stem = ("pit_" if erp == PIT_ERP else "") + name.split("/")[-1].replace("?", "_").replace("=", "_")
            paths = []
            for i, data in enumerate(frags):
                path = folder / f"{stem}.{i}"
                path.write_bytes(data)
                paths.append(str(path.relative_to(ROOT)))
            ops.append({"op": "add-resource", "when": {"crew_kit": True}, "erp": erp, "resource": name, "type": kind,
                        "sources": paths})

        dress(garage, ERP, BINDS, PARTS, tex, label, row, put)
        dress(pit, PIT_ERP, PIT_BINDS, PIT_PARTS, ptex, label, row, put)
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
    w = 2 * big + 3 * cell + 50
    img = Image.new("RGB", (2 * w + 20, 60 + ((len(rows) + 1) // 2) * (big + 50)), (30, 31, 35))
    d = ImageDraw.Draw(img)
    d.text((20, 16), "F1 1990 crew kit (garage shirt; trousers, shorts, trainers, rain jacket, headphones, "
                     "pit helmet; pit stop overall)",
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
        img.paste(Image.fromarray(tex["pitcrew_helmet"]).resize((cell, cell // 2), Image.LANCZOS),
                  (x + big + 10 + 2 * cell, y + cell + cell // 4))
        img.paste(Image.fromarray(tex["pitcrew"]).resize((big, big), Image.LANCZOS), (x + big + 3 * cell + 20, y))
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
