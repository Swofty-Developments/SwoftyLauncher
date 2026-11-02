#!/usr/bin/env python3
"""Team suits and gloves for the 2016 to 2021 seasons, which F1 25 has no suits for.

Each team-season gets one suit archive in the season addon's suits/ folder (not committed), named by its label
cs<yy>-<team>: <team> is the stock team (data/seasons.json "folders"), or the team's own key, padded with "_" to the
base label's length, for a team that shares its stock team with another (2016 Manor). The archive is a copy of every
resource of the base suit (an F1 25 team suit, 2025-<team> unless "suit": {"base": ...}) whose name holds the base
label, renamed byte for byte to the new label, which has the same length. The base suit's shared resources (the body,
glove and template meshes, the charname textures) stay in the stock archive, which the asset group lists first.

Replaced textures, every level in the archive:
- livery_v2_d: a customisable design's panel mask (design, default DESIGN) painted with the team's three colours, as
  the game paints a customisable suit: white, then colour 1 by the mask's red, colour 2 by its green, colour 3 by its
  blue.
- boots_d, glove_d: the same design's boots and glove masks, painted the same way.
- logo_01_d, glove_logo_d: the decal atlases. Each decal of the base suit's logo meshes keeps its place on the body;
  its atlas rectangle gets the team logo, a sponsor cut from the car's decal atlas, or nothing. logo_01_n and
  glove_logo_n become flat.
The other textures keep their stock `.mipmaps` sidecars.

Colours are, first found: "suit": {"colours": ["#rrggbb", "#rrggbb", "#rrggbb"]} or "team_wear" of the team in
data/seasons.json, or its team_wear colours in data/team_colours.json (tools/team_colours.py). A team with none of
them takes them from its car: paint_d and the opaque texels of decal_da, the most common colour first, then the most
common ones that differ from those before.
"suit": {"sponsors": [[x, y, w, h], ...]} names the decal atlas rectangles (pixels of the 2048 atlas) to use as
sponsors, in order; without it the largest separate shapes of the atlas made of three or more parts are used, leaving
out shapes on the atlas edge, solid shapes and mirrored copies.

Writes classic-seasons-<year>/suits/<label>.erp and suits/meta.json, which gen.py reads, and a contact sheet of the
season: front and back renders, the painted livery and the logo atlas of every team, in suits/preview.png and at half
size in art/suits/<year>.png.

    python3 tools/build_suits.py --game <F1 25> [--years 2016-2021] [--cars <folder holding classic-seasons-<year>/cars>]
                                 [--teams <key>,...]

--teams rebuilds only those teams; meta.json keeps the others and the contact sheet shows only the rebuilt ones.
"""

import argparse
import json
import re
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[2]
sys.path[:0] = [str(HERE), str(REPO / "tools")]

import bcn
import erpw
import suitgeo
from carperf import kind
import suit_sources
from layout import SUIT_ROOT, SUIT_YEARS, year_dir

DATA = json.loads((ROOT / "data" / "seasons.json").read_text())
TEAM_COLOURS = json.loads((ROOT / "data" / "team_colours.json").read_text())
DESIGN = "tier01_livery76"
LIVERY = SUIT_ROOT
BODY = "eaid://character_package/condition_scene/idf/driver_body_v2_male.idf"
GLOVES = "eaid://character_package/condition_scene/idf/driver_gloves.idf"
SIZES = {"livery_v2_d": (2048, 2048), "boots_d": (1024, 512), "glove_d": (1024, 1024), "logo_01_d": (1024, 1024),
         "glove_logo_d": (512, 512), "logo_01_n": (64, 64), "glove_logo_n": (64, 64)}
BRANDS = {"Force India": "force_india", "Racing Point": "racing_point", "Renault": "renault",
          "Toro Rosso": "toro_rosso", "AlphaTauri": "alphatauri", "Sauber": "sauber", "Alfa Romeo": "alfa_romeo",
          "Manor": "manor"}
MARK_ARRAY = "teams.driver_select_team_logos"
SPONSORS = 10


def hexrgb(h):
    h = h.lstrip("#")
    return np.array([int(h[i:i + 2], 16) for i in (0, 2, 4)], np.float32)


def rgbhex(c):
    return "#" + "".join(f"{int(round(v)):02x}" for v in c)


def luma(c):
    c = np.asarray(c, np.float32)
    return float((0.299 * c[..., 0] + 0.587 * c[..., 1] + 0.114 * c[..., 2]).mean()) / 255


def surface(e, name):
    r = e.find(name)
    return [e.frag_data(r, i) for i in range(len(r.frags))]


def small_level(frags, most):
    fmt, w, h, levels = bcn.Surface.header(frags[0])
    sizes = bcn.level_sizes(fmt, w, h, levels)
    k = 0
    while k < levels - 1 and max(w >> k, h >> k) > most:
        k += 1
    return bcn.decode(fmt, frags[1][sum(sizes[:k]):], max(1, w >> k), max(1, h >> k))


def car_archive(cars, year, team):
    folder = team["folder"]
    sub = "extra/teams" if "car" in team else "teams"
    return cars / f"classic-seasons-{year}" / "cars" / sub / folder / "wep" / f"{folder}.erp"


def car_textures(e, folder):
    def find(suffix):
        hits = [r.name for r in e.res if kind(r) == "GfxSurfaceRes" and r.name.endswith(f"/{folder}{suffix}")]
        return hits[0] if hits else None
    return find("_paint_d.tif.image"), find("_decal_da.tif.image")


def dominant(pixels, weights, n=3, apart=(110, 110)):
    q = (pixels // 24).astype(np.int64)
    key = q[:, 0] * 4096 + q[:, 1] * 64 + q[:, 2]
    order = np.argsort(key, kind="stable")
    key, pixels, weights = key[order], pixels[order], weights[order]
    _, start = np.unique(key, return_index=True)
    wsum = np.add.reduceat(weights, start)
    means = np.add.reduceat(pixels * weights[:, None], start) / np.maximum(wsum, 1e-9)[:, None]
    picked = []
    for i in np.argsort(-wsum, kind="stable"):
        if all(np.abs(means[i] - p).sum() > apart[min(len(picked), len(apart)) - 1] for p in picked):
            picked.append(means[i].astype(np.float32))
            if len(picked) == n:
                break
    while len(picked) < n:
        picked.append(np.array([20, 20, 20] if luma(picked[0]) > 0.5 else [240, 240, 240], np.float32))
    return picked


def car_colours(e, folder):
    paint, decal = car_textures(e, folder)
    pix, wt = [], []
    if paint:
        im = small_level(surface(e, paint), 256).reshape(-1, 4)[:, :3].astype(np.float32)
        pix.append(im)
        wt.append(np.full(len(im), 65536 / len(im), np.float32))
    if decal:
        im = small_level(surface(e, decal), 256).reshape(-1, 4).astype(np.float32)
        keep = im[:, 3] > 200
        pix.append(im[keep, :3])
        wt.append(np.full(int(keep.sum()), 0.5 * 65536 / len(im), np.float32))
    return dominant(np.concatenate(pix), np.concatenate(wt))


def sponsors(e, folder, rects=None, count=SPONSORS):
    from scipy import ndimage
    _, decal = car_textures(e, folder)
    if not decal:
        return []
    frags = surface(e, decal)
    fmt, w, h, _ = bcn.Surface.header(frags[0])
    full = bcn.decode(fmt, frags[1], w, h)
    if rects:
        sx, sy = w / 2048, h / 2048
        return [Image.fromarray(full[round(y * sy):round((y + rh) * sy), round(x * sx):round((x + rw) * sx)])
                for x, y, rw, rh in rects]
    s = max(1, w // 512)
    a = full[::s, ::s, 3] > 140
    grown = ndimage.binary_dilation(a, iterations=4)
    lab, n = ndimage.label(grown)
    found = []
    for i, sl in enumerate(ndimage.find_objects(lab), 1):
        ys, xs = sl
        bh, bw = ys.stop - ys.start, xs.stop - xs.start
        if bw < 24 or bh < 8 or bw / bh > 9 or bh / bw > 3:
            continue
        if ys.start == 0 or xs.start == 0 or ys.stop == a.shape[0] or xs.stop == a.shape[1]:
            continue
        inner = a[sl] & (lab[sl] == i)
        if ndimage.label(inner)[1] < 3 or inner.mean() > 0.6:
            continue
        found.append((bw * bh, xs.start * s, ys.start * s, bw * s, bh * s))
    out, seen = [], []
    for _, x, y, bw, bh in sorted(found, key=lambda f: (-f[0], f[2], f[1])):
        crop = full[y:y + bh, x:x + bw]
        thumb = np.asarray(Image.fromarray(crop).resize((32, 12))).astype(np.float32)
        flips = (thumb, thumb[::-1], thumb[:, ::-1], thumb[::-1, ::-1])
        if any(np.abs(f - t).mean() < 18 for t in seen for f in flips):
            continue
        seen.append(thumb)
        out.append(Image.fromarray(crop))
        if len(out) == count:
            break
    return out


def trim(im):
    a = np.asarray(im)[..., 3]
    ys, xs = np.nonzero(a > 8)
    return im.crop((xs.min(), ys.min(), xs.max() + 1, ys.max() + 1)) if len(xs) else im


def fit(im, w, h):
    s = min(w / im.width, h / im.height)
    return im.resize((max(1, round(im.width * s)), max(1, round(im.height * s))), Image.LANCZOS)


def contrast(im, background):
    a = np.asarray(im).astype(np.float32)
    solid = a[..., 3] > 128
    if not solid.any():
        return im
    if abs(luma(a[solid][:, :3]) - luma(background)) > 0.3:
        return im
    ink = 245 if luma(background) < 0.5 else 18
    a[..., :3] = ink
    return Image.fromarray(a.astype(np.uint8), "RGBA")


class Logos:
    def __init__(self, game):
        self.game = game
        self.marks = None

    def stock_mark(self, team):
        if self.marks is None:
            from logos import ARRAY, DATABASES, OVERRIDES, carriers, overrides_list
            from pkg import Pkg
            from stock import Stock, fragments
            stock = Stock(self.game)
            root = Pkg(fragments(stock.path(DATABASES), OVERRIDES)[0]).root
            image = ARRAY.format(MARK_ARRAY) + ".image"
            rel = carriers(self.game, stock, [image])[image][0]
            head, data = fragments(stock.path(rel), image)[:2]
            fmt, w, h = struct.unpack_from("<III", head, 8)
            self.marks = (overrides_list(root, MARK_ARRAY), data, w, h)
        listed, data, w, h = self.marks
        i = listed.index(team)
        px = bcn.bc7_decode(data[i * w * h:(i + 1) * w * h], w, h)
        return trim(Image.fromarray(px, "RGBA"))

    def team(self, team, stock):
        brand = BRANDS.get(team["name"])
        if brand:
            art = ROOT / "art" / "logos" / brand
            return {"mark": trim(Image.open(art / "mark.png").convert("RGBA")),
                    "light": trim(Image.open(art / "lockup_light.png").convert("RGBA")),
                    "dark": trim(Image.open(art / "lockup_dark.png").convert("RGBA"))}
        mark = self.stock_mark(stock)
        return {"mark": mark, "light": mark, "dark": mark}


def paint(mask, colours, size):
    m = np.asarray(Image.fromarray(mask).resize(size, Image.BILINEAR)).astype(np.float32) / 255
    out = np.full(m.shape[:2] + (3,), 255, np.float32)
    for c in range(3):
        out += (colours[c] - out) * m[..., c:c + 1]
    return np.dstack([np.clip(out, 0, 255), np.full(m.shape[:2], 255)]).astype(np.uint8)


def under(livery, uv):
    h, w = livery.shape[:2]
    u, v = np.mean(uv, 0)
    x, y = int(np.clip(u * w, 0, w - 1)), int(np.clip(v * h, 0, h - 1))
    return livery[max(0, y - 8):y + 8, max(0, x - 8):x + 8, :3].reshape(-1, 3).mean(0)


def atlas(slots, size, livery, team_art, extra, roles):
    out = Image.new("RGBA", size, (0, 0, 0, 0))
    plan = []
    pool = list(extra)
    torso = [s for s in slots if abs(np.mean([p[0] for p in s["at"]])) < 0.2]
    big = sorted(torso or slots, key=lambda s: -(s["box"][2] - s["box"][0]) * (s["box"][3] - s["box"][1]))
    lockups = {id(s) for s in big[:roles.get("lockups", 2)]}
    marks = set()
    for s in big:
        x0, y0, x1, y1 = s["box"]
        if id(s) not in lockups and (x1 - x0) / max(1, y1 - y0) < 1.6:
            marks.add(id(s))
            break
    for s in sorted(slots, key=lambda s: -(s["box"][2] - s["box"][0]) * (s["box"][3] - s["box"][1])):
        x0, y0, x1, y1 = s["box"]
        w, h = x1 - x0, y1 - y0
        bg = under(livery, s["uv"])
        if id(s) in lockups:
            what = "team"
            logo = team_art["dark" if luma(bg) < 0.5 else "light"]
            if w / max(1, h) < 1.6:
                logo = team_art["mark"]
        elif id(s) in marks:
            what, logo = "team", team_art["mark"]
        elif pool:
            aspect = w / max(1, h)
            pick = min(range(len(pool)), key=lambda i: abs(np.log((pool[i].width / pool[i].height) / aspect)))
            what, logo = "sponsor", contrast(pool.pop(pick), bg)
        else:
            plan.append({"box": s["box"], "what": "empty"})
            continue
        logo = fit(trim(logo), w * 0.86, h * 0.8)
        out.alpha_composite(logo, (x0 + (w - logo.width) // 2, y0 + (h - logo.height) // 2))
        plan.append({"box": s["box"], "what": what})
    return np.asarray(out), plan


def flat_normal(w, h):
    chain = []
    while True:
        chain.append(bytes([128, 128, 0, 0, 0, 0, 0, 0]) * 2 * max(1, (w + 3) // 4) * max(1, (h + 3) // 4))
        if min(w, h) <= 4:
            break
        w, h = w // 2, h // 2
    return b"".join(chain), len(chain)


def put_texture(e, name, head, data, levels, fmt):
    r = e.find(name)
    r.frags = r.frags[:2]
    e.set_frag(r, 0, head)
    e.set_frag(r, 1, data)
    srv = e.find(name[:-len(".image")])
    view = bytearray(e.frag_data(srv, 0))
    struct.pack_into("<I", view, 4, fmt)
    struct.pack_into("<I", view, 12, levels)
    e.set_frag(srv, 0, bytes(view))


def suit_archive(base_erp, base, label, textures):
    e = erpw.Erp(str(base_erp))
    old, new = base.encode(), label.encode()
    e.res = [r for r in e.res if old in r.name.encode()]
    for r in e.res:
        r.name = r.name.replace(base, label)
        side = kind(r) == "GfxSurfaceRes"
        for i in range(len(r.frags)):
            if side and i == 2:
                continue
            d = e.frag_data(r, i)
            if old in d:
                e.set_frag(r, i, d.replace(old, new))
    prefix = f"eaid://{LIVERY}/{label}/textures/{label}_"
    for part, img in textures.items():
        name = f"{prefix}{part}.tif.image"
        r = e.find(name)
        fmt = bcn.Surface.header(e.frag_data(r, 0))[0]
        if part.endswith("_n"):
            w, h = SIZES[part]
            data, levels = flat_normal(w, h)
            head = bytearray(e.frag_data(r, 0))
            struct.pack_into("<2I", head, 12, w, h)
            struct.pack_into("<I", head, 24, levels)
            put_texture(e, name, bytes(head), data, levels, fmt)
            continue
        head, data, levels = bcn.surface_frags(e.frag_data(r, 0), fmt, img)
        put_texture(e, name, head, data, levels, fmt)
    left = [r.name for r in e.res for i in range(len(r.frags))
            if not (kind(r) == "GfxSurfaceRes" and i == 2) and old in e.frag_data(r, i)]
    if left:
        raise SystemExit(f"{label}: {base} left in {left[:3]}")
    return e


def design_masks(game, design):
    tier = design.split("_")[0]
    folder = Path(game) / "2025_asset_groups/livery_package" / tier / design
    suit, gloves = erpw.Erp(str(folder / f"{design}_suit.erp")), erpw.Erp(str(folder / f"{design}_gloves.erp"))
    prefix = f"eaid://livery_package_shared/{tier}/{design}/textures/{design}_"

    def mask(e, part):
        return bcn.top_level(surface(e, f"{prefix}{part}_m.tif.image"), game)
    return {"livery_v2_d": mask(suit, "livery_v2"), "boots_d": mask(suit, "boots"), "glove_d": mask(gloves, "glove")}


def team_meshes(label):
    return {"feet": f"eaid://{LIVERY}/{label}/idf/{label}_male.idf",
            "logos": f"eaid://{LIVERY}/{label}/idf/{label}_body_logos_male.idf",
            "glove_logos": f"eaid://{LIVERY}/{label}/idf/{label}_glove_logos_male.idf"}


def render(e_base, e_kit, meshes, tex):
    views = []
    body = suitgeo.lod0(e_base, BODY)
    gl = suitgeo.lod0(e_base, GLOVES)
    feet = suitgeo.lod0(e_kit, meshes["feet"])
    logos = suitgeo.lod0(e_kit, meshes["logos"])
    glogos = suitgeo.lod0(e_kit, meshes["glove_logos"])
    for back in (False, True):
        v = suitgeo.View(640, 440, back)
        for e, m, part, key, blend in ((e_base, body, "livery_v2", "livery_v2_d", False),
                                       (e_kit, feet, "boots", "boots_d", False),
                                       (e_base, gl, "gloves", "glove_d", False),
                                       (e_kit, logos, "livery_v2_logo", "logo_01_d", True),
                                       (e_kit, glogos, "gloves_logo", "glove_logo_d", True)):
            i = suitgeo.submesh(e, m, part)
            if i is None or tex.get(key) is None:
                continue
            v.draw(suitgeo.stream(e, m, i, 1), suitgeo.stream(e, m, i, 5), suitgeo.triangles(e, m, i), tex[key],
                   bias=0.004 if blend else 0.0, blend=blend)
        views.append(v.image((52, 54, 60)))
    return views


def kit_textures(e, prefixes, roots):
    out = {}
    names = {r.name for r in e.res}
    for part in ("livery_v2_d", "boots_d", "glove_d", "logo_01_d", "glove_logo_d"):
        for prefix in prefixes:
            name = f"{prefix}{part}.tif.image"
            if name in names:
                out[part] = bcn.top_level(surface(e, name), roots)
                break
    return out


def build_team(game, cars, year, team, logos, original_games=None, cache=None):
    suit = team.get("suit", {})
    if suit.get("source") == suit_sources.ORIGINAL:
        if original_games is None:
            raise ValueError(f"{year} {team['name']}: --original-games is required for the original suit artwork")
        from original_suits import build
        return build(game, original_games / f"F1 {year}", year, team, cache)
    stock = DATA["folders"][team["folder"]]["team"]
    base = suit.get("base", f"2025-{stock}")
    label = suit_sources.generated_label(year, team, base)
    base_erp = Path(game) / "2025_asset_groups" / LIVERY / base / f"{base}.erp"
    car = erpw.Erp(str(car_archive(cars, year, team)))
    wear = team.get("team_wear") or \
        TEAM_COLOURS.get(f"cs{year % 100:02d}_{team.get('key', team['folder'])}", {}).get("team_wear")
    colours = [hexrgb(c) for c in suit.get("colours") or wear] if suit.get("colours") or wear else \
        car_colours(car, team["folder"])
    design = suit.get("design", DESIGN)
    masks = design_masks(game, design)
    livery = paint(masks["livery_v2_d"], colours, SIZES["livery_v2_d"])
    boots = paint(masks["boots_d"], [colours[0] * 0.35, colours[1] * 0.35, colours[2] * 0.6], SIZES["boots_d"])
    glove = paint(masks["glove_d"], colours, SIZES["glove_d"])
    e_base = erpw.Erp(str(base_erp))
    body_logos = f"eaid://{LIVERY}/{base}/idf/{base}_body_logos_male.idf"
    glove_logos = f"eaid://{LIVERY}/{base}/idf/{base}_glove_logos_male.idf"
    slots = suitgeo.decal_slots(e_base, body_logos, "livery_v2_logo", SIZES["logo_01_d"][0])
    gslots = suitgeo.decal_slots(e_base, glove_logos, "gloves_logo", SIZES["glove_logo_d"][0])
    team_art = logos.team(team, stock)
    extra = sponsors(car, team["folder"], suit.get("sponsors"))
    logo, plan = atlas(slots, SIZES["logo_01_d"], livery, team_art, extra, {"lockups": 2})
    glogo, gplan = atlas(gslots, SIZES["glove_logo_d"], glove, team_art, [], {"lockups": 2})
    textures = {"livery_v2_d": livery, "boots_d": boots, "glove_d": glove, "logo_01_d": logo, "glove_logo_d": glogo,
                "logo_01_n": None, "glove_logo_n": None}
    e = suit_archive(base_erp, base, label, textures)
    views = render(e_base, e, team_meshes(label), textures)
    meta = {"label": label, "base": base, "design": design, "colours": [rgbhex(c) for c in colours],
            "sponsors": len(extra), "logo_slots": plan, "glove_slots": gplan}
    return label, e, meta, views, livery, logo


def font(size):
    for f in ("/run/current-system/sw/share/X11/fonts/DejaVuSans.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            continue
    return ImageFont.load_default()


def contact_sheet(title, rows, out, small):
    tw, th = 440, 640
    cw = 2 * tw + 2 * th + 60
    sheet = Image.new("RGB", (cw, 70 + len(rows) * (th + 84)), (30, 31, 35))
    d = ImageDraw.Draw(sheet)
    d.text((20, 18), f"{title} (front, back, livery_v2_d, logo_01_d)", fill=(235, 235, 235), font=font(26))
    for n, (team, meta, views, livery, logo) in enumerate(rows):
        y = 70 + n * (th + 84)
        d.text((20, y + 4), f"{team['name']}  ·  {meta['label']}  ·  base {meta['base']}  ·  {meta['design']}",
               fill=(220, 220, 220), font=font(20))
        if meta.get("drivers"):
            d.text((20, y + th + 44), "  ·  ".join(meta["drivers"]), fill=(170, 200, 255), font=font(16))
        for k, c in enumerate(meta["colours"]):
            d.rectangle((cw - 40 - 34 * (3 - k), y + 2, cw - 40 - 34 * (3 - k) + 28, y + 30), fill=c, outline=(200, 200, 200))
        y += 40
        sheet.paste(Image.fromarray(views[0]), (20, y))
        sheet.paste(Image.fromarray(views[1]), (20 + tw, y))
        sheet.paste(Image.fromarray(livery[..., :3]).resize((th, th), Image.LANCZOS), (40 + 2 * tw, y))
        lg = Image.new("RGBA", (th, th), (70, 72, 80, 255))
        lg.alpha_composite(Image.fromarray(logo).resize((th, th), Image.LANCZOS))
        dd = ImageDraw.Draw(lg)
        s = th / logo.shape[1]
        for slot in meta["logo_slots"]:
            x0, y0, x1, y1 = (v * s for v in slot["box"])
            dd.rectangle((x0, y0, x1, y1), outline={"team": (90, 200, 255), "sponsor": (255, 200, 60),
                                                     "empty": (110, 110, 110)}[slot["what"]])
        sheet.paste(lg.convert("RGB"), (60 + 2 * tw + th, y))
    sheet.save(out, optimize=True)
    shrink(sheet).save(small, optimize=True)


def shrink(sheet):
    half = sheet.resize((sheet.width // 2, sheet.height // 2), Image.LANCZOS)
    return half.quantize(256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE)


def real_row(game, team, choice, drivers):
    label = choice["label"]
    if choice["kind"] == "mod":
        meta = next(m for m in json.loads((ROOT / "suits" / "meta.json").read_text())["suits"] if m["label"] == label)
        e = erpw.Erp(str(ROOT / meta["source"]))
        e_base = erpw.Erp(str(Path(game) / meta["base_erp"]))
        roots = [ROOT / "suits" / "sidecars", game]
        what = suit_sources.REAL["mods"][choice["mod"]]["name"]
    else:
        e = e_base = erpw.Erp(str(Path(game) / "2025_asset_groups" / LIVERY / label / f"{label}.erp"))
        roots = [game]
        what = "F1 25"
    tex = kit_textures(e, [f"eaid://{LIVERY}/{label}/textures/{label}_"], roots)
    views = render(e_base, e, team_meshes(label), tex)
    logo = tex.get("logo_01_d")
    if logo is None:
        logo = np.zeros((1024, 1024, 4), np.uint8)
    return team, {"label": label, "base": choice.get("base", label), "design": f"real, {what} {choice['season']}",
                  "colours": [], "logo_slots": [], "drivers": drivers}, views, tex["livery_v2_d"], logo


def years_arg(text):
    if "-" in text:
        a, b = text.split("-")
        return list(range(int(a), int(b) + 1))
    return [int(y) for y in text.split(",")]


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--years", type=years_arg, default=list(SUIT_YEARS))
    ap.add_argument("--cars", type=Path, default=ROOT.parent,
                    help="folder holding classic-seasons-<year>/cars (default: this checkout)")
    ap.add_argument("--teams", help="comma-separated team keys to build (default: all)")
    ap.add_argument("--output", type=Path, help="write to a new parent folder instead of this checkout")
    ap.add_argument("--original-games", type=Path, help="folder containing the original F1 2020 and F1 2021 games")
    a = ap.parse_args(argv[1:])
    logos = Logos(a.game)
    seasons = {s["year"]: s for s in DATA["seasons"]}
    for year in a.years:
        if year not in SUIT_YEARS:
            raise SystemExit(f"{year}: suits are built for {SUIT_YEARS.start} to {SUIT_YEARS.stop - 1}")
        out = (a.output / f"classic-seasons-{year}" if a.output else year_dir(year)) / "suits"
        if a.output and out.exists():
            raise SystemExit(f"{out}: already exists; choose a new --output directory")
        out.mkdir(parents=True, exist_ok=True)
        meta_path = out / "meta.json"
        meta = json.loads(meta_path.read_text()) if meta_path.exists() else {"suits": {}}
        rows = []
        for team in seasons[year]["teams"]:
            key = team.get("key", team["folder"])
            if a.teams and key not in a.teams.split(","):
                continue
            choice = suit_sources.pick(year, team)
            drivers = [f"{d}: {suit_sources.describe(suit_sources.pick(year, team, d))}" for d, _ in team["drivers"]]
            if choice:
                meta["suits"].pop(key, None)
                rows.append(real_row(a.game, team, choice, drivers))
                print(f"{year} {key}: {suit_sources.describe(choice)}")
                continue
            label, e, m, views, livery, logo = build_team(a.game, a.cars, year, team, logos, a.original_games,
                                                       (a.output or ROOT) / ".cache/suit-transfer")
            e.to_file(str(out / f"{label}.erp"))
            meta["suits"][key] = m
            rows.append((team, {**m, "drivers": drivers}, views, livery,
                         logo))
            print(f"{year} {key}: {label} from {m['base']}, {m['design']}, {' '.join(m['colours'])}, "
                  f"{sum(s['what'] != 'empty' for s in m['logo_slots'])}/{len(m['logo_slots'])} decals filled")
        for stale in out.glob("*.erp"):
            if stale.stem not in {m["label"] for m in meta["suits"].values()}:
                stale.unlink()
        meta["suits"] = {t.get("key", t["folder"]): meta["suits"][t.get("key", t["folder"])]
                         for t in seasons[year]["teams"] if t.get("key", t["folder"]) in meta["suits"]}
        meta_path.write_text(json.dumps(meta, indent=1) + "\n")
        sheet = (a.output if a.output else ROOT) / "art" / "suits" / f"{year}.png"
        sheet.parent.mkdir(parents=True, exist_ok=True)
        contact_sheet(f"Classic Seasons F1 {year}: team suits", rows, out / "preview.png", sheet)
        print(f"{year}: {out / 'preview.png'}, {sheet}")


if __name__ == "__main__":
    main(sys.argv)
