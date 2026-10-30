#!/usr/bin/env python3
"""The 1990 team suits and gloves: one archive per team, a suit and a gloves record for each, and the database edits
that dress the 1990 looks and team kits in them.

Each team's archive, kit/<label>.erp (build output, not committed), is a copy of every resource of the stock 2026
Williams suit whose name holds its label, renamed byte for byte to the team's label (f90-<team>, padded with "_" to
the base label's 13 characters). Replaced textures, every level in the archive:
- livery_v2_d: the team's 1990 colour blocks, painted per texel from the body's position (tools/kit.py DESIGNS).
- glove_d: the base glove with its light background in the team's glove colour and the cuff in the cuff colour;
  the dark palm and knuckle pads stay.
- boots_d, cap_d, suit_attachments_d: one flat colour each.
- logo_01_d, glove_logo_d: the decal atlases, with the team's lockup on the two largest chest decals, its mark on
  the next and on the largest glove decal, the others empty. logo_01_n and glove_logo_n become flat.
The normals, specular maps and harness keep the base suit's `.mipmaps` sidecars.

Writes kit/<label>.erp, registry/kit.json (the suit and gloves records), db/kit.json (each 1990 look and team kit
row wears its team's suit and gloves) and a preview sheet (front, back, livery_v2_d and glove_d of every team).

    kit.py --game <F1 25 folder> [--sheet <png>] [--teams mclaren,...]
"""

import argparse
import json
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/classicseasons/classic-seasons/tools")]

import bcn
import erpw
import suitgeo
from carperf import kind

BASE = "2026-williams"
LIVERY = "character_package/team_livery"
BODY = "eaid://character_package/condition_scene/idf/driver_body_v2_male.idf"
GLOVES = "eaid://character_package/condition_scene/idf/driver_gloves.idf"
ARM_U = 0.672
LOOKS = "entrants.driver_customisations.folder[_metadata_=official_driver_customisations]"
LOOK_ROW = "preloaded_driver_customisation_properties"
KITS = "entrants.team_customisation.team_customisation_for_year[_metadata_=2026]"

RED, WHITE, BLACK = "#D2141E", "#F4F4F2", "#1A1A1A"
DESIGNS = {
    "mclaren": {"base": RED, "zones": [("roof", 1.27, 0.9, WHITE)], "glove": RED, "cuff": WHITE, "boots": RED},
    "ferrari": {"base": "#C4161C", "zones": [("roof_line", 1.27, 0.9, 0.012, WHITE), ("collar", WHITE)],
                "glove": "#C4161C", "cuff": WHITE, "boots": BLACK},
    "williams": {"base": WHITE, "zones": [("below", 1.0, "#1B3F8B"), ("band", 1.0, 1.06, "#F2C200"),
                                         ("cuffs", "#1B3F8B"), ("arm_stripe", "#F2C200")],
                 "glove": "#1B3F8B", "cuff": "#F2C200", "boots": "#1B3F8B"},
    "benetton": {"base": "#1F9A48", "zones": [("arm_left", "#1E4FA0"), ("arm_right", "#F2C200"),
                                             ("band", 1.18, 1.26, "#D0202A"), ("below", 0.3, "#1E4FA0")],
                 "glove": "#1F9A48", "cuff": "#D0202A", "boots": "#1E4FA0"},
    "tyrrell": {"base": "#1F2F6B", "zones": [("band", 1.14, 1.3, WHITE), ("leg_stripe", WHITE),
                                            ("arm_stripe", "#8AB4E8")],
                "glove": "#1F2F6B", "cuff": WHITE, "boots": "#1F2F6B"},
    "lotus": {"base": "#F2C500", "zones": [("leg_stripe", "#1F4E9C"), ("arm_stripe", "#1F4E9C"),
                                          ("collar", "#1F4E9C")],
              "glove": "#F2C500", "cuff": "#1F4E9C", "boots": "#1F4E9C"},
    "march": {"base": "#4FD3D9", "zones": [("leg_stripe", "#1B2A5C"), ("cuffs", "#1B2A5C"), ("collar", "#1B2A5C"),
                                          ("below", 0.2, "#1B2A5C")],
              "glove": "#4FD3D9", "cuff": "#1B2A5C", "boots": "#1B2A5C"},
    "larrousse": {"base": WHITE, "zones": [("below", 1.0, "#3E9A4E"), ("band", 1.0, 1.05, "#1F3C88"),
                                          ("cuffs", "#3E9A4E")],
                  "glove": WHITE, "cuff": "#3E9A4E", "boots": "#1F3C88"},
    "ligier_prost": {"base": "#0F5FDD", "zones": [("band", 1.2, 1.3, "#6EB4FF"), ("arm_stripe", WHITE),
                                                 ("leg_stripe", WHITE)],
                     "glove": "#0F5FDD", "cuff": WHITE, "boots": "#0F5FDD"},
    "minardi": {"base": WHITE, "zones": [("below", 1.04, "#F7AC15"), ("band", 1.04, 1.08, BLACK),
                                        ("collar", BLACK), ("cuffs", "#F7AC15")],
                "glove": "#F7AC15", "cuff": BLACK, "boots": BLACK},
    "brabham": {"base": "#2A3052", "zones": [("above", 1.12, WHITE), ("band", 1.08, 1.12, "#C8102E"),
                                            ("cuffs", "#2A3052")],
                "glove": "#2A3052", "cuff": WHITE, "boots": "#2A3052"},
    "dallara": {"base": "#960D04", "zones": [("below", 0.45, BLACK), ("band", 0.45, 0.48, WHITE),
                                            ("collar", WHITE)],
                "glove": "#960D04", "cuff": BLACK, "boots": BLACK},
    "arrows": {"base": WHITE, "zones": [("band", 1.16, 1.3, "#C8102E"), ("band", 1.08, 1.16, "#1A3C8C"),
                                       ("leg_stripe", "#C8102E"), ("cuffs", "#1A3C8C")],
               "glove": WHITE, "cuff": "#C8102E", "boots": "#1A3C8C"},
}
TOKEN = {"ligier_prost": "ligier", "march": "leytonhouse"}


def label(team):
    key = {"ligier_prost": "ligier"}.get(team, team)
    out = f"f90-{key}".ljust(len(BASE), "_")
    if len(out) != len(BASE):
        raise SystemExit(f"{team}: label {out} is not {len(BASE)} characters")
    return out


def rgb(h):
    h = h.lstrip("#")
    return np.array([int(h[i:i + 2], 16) for i in (0, 2, 4)], np.float32)


def surface(e, name):
    r = e.find(name)
    return [e.frag_data(r, i) for i in range(len(r.frags))]


def normals(pos, tris):
    n = np.zeros_like(pos)
    face = np.cross(pos[tris[:, 1]] - pos[tris[:, 0]], pos[tris[:, 2]] - pos[tris[:, 0]])
    for k in range(3):
        np.add.at(n, tris[:, k], face)
    return n / np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-12)


def fill(img, cover):
    from scipy import ndimage
    _, (iy, ix) = ndimage.distance_transform_edt(~cover, return_indices=True)
    return img[iy, ix]


def uv_maps(e, mesh, part, size):
    m = suitgeo.lod0(e, mesh)
    i = suitgeo.submesh(e, m, part)
    pos, uv, tris = suitgeo.stream(e, m, i, 1), suitgeo.stream(e, m, i, 5)[:, :2], suitgeo.triangles(e, m, i)
    attrs = np.concatenate([pos, normals(pos, tris)], 1)[tris].astype(np.float32)
    out, cover, _ = suitgeo.raster(uv[tris] * size, attrs, size, size)
    return fill(out, cover)


class Body:
    def __init__(self, maps):
        h, w = maps.shape[:2]
        self.x, self.y, self.z = maps[..., 0], maps[..., 1], maps[..., 2]
        self.nx, self.ny = maps[..., 3], maps[..., 4]
        u = (np.arange(w) + 0.5) / w
        v = (np.arange(h) + 0.5) / h
        self.arm = (u[None, :] > ARM_U) & (v[:, None] < 0.95)
        self.ax = np.abs(self.x)
        self.leg_edge = self.edge(~self.arm & (self.y < 0.95), self.y, self.ax)
        self.arm_top = self.edge(self.arm, self.ax, self.y)

    @staticmethod
    def edge(where, along, across, step=0.01):
        bins = np.floor(along / step).astype(np.int64)
        lo = bins[where].min()
        top = np.full(bins[where].max() - lo + 1, -np.inf)
        np.maximum.at(top, bins[where] - lo, across[where])
        top = np.maximum.reduce([np.roll(top, k) for k in (-2, -1, 0, 1, 2)])
        return top[np.clip(bins - lo, 0, len(top) - 1)]

    def zone(self, rule):
        k, *a = rule
        trunk = ~self.arm
        if k == "below":
            return trunk & (self.y < a[0])
        if k == "above":
            return (trunk & (self.y >= a[0])) | (self.arm & (a[0] < 1.4))
        if k == "band":
            return trunk & (self.y >= a[0]) & (self.y < a[1])
        if k == "roof":
            return trunk & (self.y < a[0] - a[1] * self.ax)
        if k == "roof_line":
            return trunk & (np.abs(self.y - (a[0] - a[1] * self.ax)) < a[2])
        if k == "arm_left":
            return self.arm & (self.x > 0)
        if k == "arm_right":
            return self.arm & (self.x < 0)
        if k == "cuffs":
            return (self.arm & (self.ax > 0.52)) | (trunk & (self.y < 0.17))
        if k == "arm_stripe":
            return self.arm & (self.y > self.arm_top - 0.022)
        if k == "leg_stripe":
            return trunk & (self.y < 0.95) & (self.ax > self.leg_edge - 0.022)
        if k == "collar":
            return trunk & (self.y > 1.47)
        raise SystemExit(f"unknown zone {k}")

    def paint(self, design):
        out = np.empty(self.x.shape + (3,), np.float32)
        out[:] = rgb(design["base"])
        for rule in design["zones"]:
            out[self.zone(rule)] = rgb(rule[-1])
        return np.dstack([out, np.full(self.x.shape, 255, np.float32)]).astype(np.uint8)


def glove(base, maps, design):
    px = base[..., :3].astype(np.float32)
    bg = np.median(px[(px.mean(-1) > 170)], 0)
    cloth = np.abs(px - bg).sum(-1) < 60
    shade = np.clip(px.mean(-1) / max(bg.mean(), 1), 0.75, 1.1)[..., None]
    ax = np.abs(maps[..., 0])
    cuff = ax < ax.min() + 0.035
    colour = np.where(cuff[..., None], rgb(design["cuff"]), rgb(design["glove"]))
    out = np.where(cloth[..., None], np.clip(colour * shade, 0, 255), px)
    return np.dstack([out, np.full(out.shape[:2], 255, np.float32)]).astype(np.uint8)


def flat(size, colour):
    return np.dstack([np.broadcast_to(rgb(colour), size + (3,)), np.full(size, 255, np.float32)]).astype(np.uint8)


def dark(colour, f=0.75):
    return "#" + "".join(f"{int(v * f):02x}" for v in rgb(colour))


def trim(im):
    a = np.asarray(im)[..., 3]
    ys, xs = np.nonzero(a > 8)
    return im.crop((xs.min(), ys.min(), xs.max() + 1, ys.max() + 1)) if len(xs) else im


def luma(c):
    c = np.asarray(c, np.float32)
    return float((0.299 * c[..., 0] + 0.587 * c[..., 1] + 0.114 * c[..., 2]).mean()) / 255


def atlas(slots, size, under, art, lockups):
    out = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    chest = [s for s in slots if abs(np.mean([p[0] for p in s["at"]])) < 0.2 and np.mean([p[1] for p in s["at"]]) > 1.1]
    torso = chest or slots
    big = sorted(torso, key=lambda s: -(s["box"][2] - s["box"][0]) * (s["box"][3] - s["box"][1]))
    plan = [(s, "lockup") for s in big[:lockups]] + [(s, "mark") for s in big[lockups:lockups + 1]]
    for s, what in plan:
        x0, y0, x1, y1 = s["box"]
        w, h = x1 - x0, y1 - y0
        bg = under(s["uv"])
        logo = art["mark"]
        if what == "lockup" and w / max(1, h) >= 1.6:
            logo = art["dark" if luma(bg) < 0.5 else "light"]
        logo = trim(logo)
        k = min(w * 0.86 / logo.width, h * 0.8 / logo.height)
        logo = logo.resize((max(1, round(logo.width * k)), max(1, round(logo.height * k))), Image.LANCZOS)
        out.alpha_composite(logo, (x0 + (w - logo.width) // 2, y0 + (h - logo.height) // 2))
    return np.asarray(out)


def logo_art(team):
    folder = ROOT / "art" / "logos" / TOKEN.get(team, team)
    return {"mark": Image.open(folder / "mark.png").convert("RGBA"),
            "light": Image.open(folder / "lockup_light.png").convert("RGBA"),
            "dark": Image.open(folder / "lockup_dark.png").convert("RGBA")}


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


def archive(base_erp, name, textures):
    e = erpw.Erp(str(base_erp))
    old, new = BASE.encode(), name.encode()
    e.res = [r for r in e.res if old in r.name.encode()]
    for r in e.res:
        r.name = r.name.replace(BASE, name)
        side = kind(r) == "GfxSurfaceRes"
        for i in range(len(r.frags)):
            if side and i == 2:
                continue
            d = e.frag_data(r, i)
            if old in d:
                e.set_frag(r, i, d.replace(old, new))
    prefix = f"eaid://{LIVERY}/{name}/textures/{name}_"
    for part, img in textures.items():
        tex = f"{prefix}{part}.tif.image"
        head0 = e.frag_data(e.find(tex), 0)
        fmt = bcn.Surface.header(head0)[0]
        if img is None:
            w, h = 64, 64
            data, levels = flat_normal(w, h)
            head = bytearray(head0)
            struct.pack_into("<2I", head, 12, w, h)
            struct.pack_into("<I", head, 24, levels)
            put_texture(e, tex, bytes(head), data, levels, fmt)
            continue
        head, data, levels = bcn.surface_frags(head0, fmt, img)
        put_texture(e, tex, head, data, levels, fmt)
    left = [r.name for r in e.res for i in range(len(r.frags))
            if not (kind(r) == "GfxSurfaceRes" and i == 2) and old in e.frag_data(r, i)]
    if left:
        raise SystemExit(f"{name}: {BASE} left in {left[:3]}")
    return e


def render(e_base, e_kit, name, tex):
    meshes = {"feet": f"eaid://{LIVERY}/{name}/idf/{name}_male.idf",
              "logos": f"eaid://{LIVERY}/{name}/idf/{name}_body_logos_male.idf",
              "glove_logos": f"eaid://{LIVERY}/{name}/idf/{name}_glove_logos_male.idf"}
    body, gl = suitgeo.lod0(e_base, BODY), suitgeo.lod0(e_base, GLOVES)
    feet, logos = suitgeo.lod0(e_kit, meshes["feet"]), suitgeo.lod0(e_kit, meshes["logos"])
    glogos = suitgeo.lod0(e_kit, meshes["glove_logos"])
    views = []
    for back in (False, True):
        v = suitgeo.View(520, 360, back)
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


def build_team(game, team, base_erp, e_base, body, glove_maps, slots, gslots):
    d = DESIGNS[team]
    name = label(team)
    pre = f"eaid://{LIVERY}/{BASE}/textures/{BASE}_"
    livery = body.paint(d)
    gloves = glove(bcn.top_level(surface(e_base, pre + "glove_d.tif.image"), game), glove_maps, d)

    def size(part):
        return bcn.top_level(surface(e_base, pre + part + ".tif.image"), game).shape[:2]

    def under(uvs):
        h, w = livery.shape[:2]
        u, v = np.mean(uvs, 0)
        x, y = int(np.clip(u * w, 0, w - 1)), int(np.clip(v * h, 0, h - 1))
        return livery[max(0, y - 8):y + 8, max(0, x - 8):x + 8, :3].reshape(-1, 3).mean(0)

    def glove_under(uvs):
        return rgb(d["glove"])

    art = logo_art(team)
    tex = {"livery_v2_d": livery, "glove_d": gloves, "boots_d": flat(size("boots_d"), d["boots"]),
           "cap_d": flat(size("cap_d"), d["base"]), "suit_attachments_d": flat(size("suit_attachments_d"),
                                                                              dark(d["base"])),
           "logo_01_d": atlas(slots, size("logo_01_d")[0], under, art, 2),
           "glove_logo_d": atlas(gslots, size("glove_logo_d")[0], glove_under, art, 0),
           "logo_01_n": None, "glove_logo_n": None}
    e = archive(base_erp, name, tex)
    return name, e, tex


def colours(team):
    d = DESIGNS[team]
    zone = [z[-1] for z in d["zones"]]
    picked = [d["base"]] + [c for c in zone if c != d["base"]]
    return (picked + [d["cuff"], d["glove"], WHITE, WHITE])[:4]


def records(teams):
    suit, gloves = {}, {}
    for team, donor in teams:
        name = label(team)
        common = {"label": name, "archive": f"kit/{name}.erp", "team": f"game/{donor}", "colours": colours(team)}
        suit[f"kit_{team}"] = {**common, "from": f"game/{BASE}"}
        gloves[f"kit_{team}"] = {**common, "from": f"game/{BASE}"}
    return {"suit": suit, "gloves": gloves}


def edits():
    import grid
    from season import Season, slots
    season = Season.load()
    lineup, free, team_of, _, _ = grid.lineup_of(season)
    ops = []
    for did in [seat.driver for _, _, seat in lineup] + free:
        team = team_of[did].team
        look = f"{LOOKS}.{LOOK_ROW}[_metadata_=f1990-{grid.token(team)}_{did}]"
        for field in ("suit", "gloves"):
            ops.append({"op": "set", "select": look, "field": field, "value": label(team)})
    for s in slots():
        kit = f"{KITS}.team_customisation[_metadata_=2026-f1990_{grid.token(s.team)}]"
        for field in ("suit", "gloves"):
            op = {"op": "set", "select": kit, "field": field, "value": label(s.team)}
            if s.team in grid.EXTRA_TEAMS:
                op["when"] = {"cars": 26}
            ops.append(op)
    teams = [(s.team, grid.DONOR_ROW.get(s.team_row, s.team_row)) for s in slots()]
    return {"schema_version": 1, "ops": ops}, teams


def font(size):
    for f in ("/run/current-system/sw/share/X11/fonts/DejaVuSans.ttf", "DejaVuSans.ttf"):
        try:
            return ImageFont.truetype(f, size)
        except OSError:
            continue
    return ImageFont.load_default()


def sheet(rows, out):
    tw, th, sq = 360, 520, 260
    cell_w, cell_h = 2 * tw + sq + 30, th + 50
    cols = 2
    n = (len(rows) + cols - 1) // cols
    img = Image.new("RGB", (cols * cell_w + 20, 60 + n * cell_h), (30, 31, 35))
    d = ImageDraw.Draw(img)
    d.text((20, 16), "F1 1990 team suits (front, back, livery_v2_d, glove_d)", fill=(235, 235, 235), font=font(26))
    for k, (team, name, views, tex) in enumerate(rows):
        x, y = 10 + (k % cols) * cell_w, 60 + (k // cols) * cell_h
        d.text((x + 10, y + 6), f"{team}  ·  {name}", fill=(220, 220, 220), font=font(20))
        for j, c in enumerate(colours(team)):
            d.rectangle((x + cell_w - 40 - 30 * (4 - j), y + 6, x + cell_w - 40 - 30 * (4 - j) + 24, y + 30), fill=c,
                        outline=(200, 200, 200))
        y += 40
        img.paste(Image.fromarray(views[0]), (x, y))
        img.paste(Image.fromarray(views[1]), (x + tw, y))
        img.paste(Image.fromarray(tex["livery_v2_d"][..., :3]).resize((sq, sq), Image.LANCZOS), (x + 2 * tw + 10, y))
        img.paste(Image.fromarray(tex["glove_d"][..., :3]).resize((sq, sq), Image.LANCZOS),
                  (x + 2 * tw + 10, y + sq))
    img.save(out, optimize=True)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--sheet", type=Path, default=ROOT / "kit" / "preview.png")
    ap.add_argument("--teams", help="comma-separated team ids to rebuild (default: all)")
    a = ap.parse_args(argv[1:])
    db, teams = edits()
    (ROOT / "db" / "kit.json").write_text(json.dumps(db, indent=1) + "\n")
    (ROOT / "registry").mkdir(exist_ok=True)
    (ROOT / "registry" / "kit.json").write_text(json.dumps(records(teams), indent=1) + "\n")
    print(f"db/kit.json: {len(db['ops'])} ops; registry/kit.json: {len(teams)} suits, {len(teams)} gloves")
    base_erp = a.game / "2025_asset_groups" / LIVERY / BASE / f"{BASE}.erp"
    e_base = erpw.Erp(str(base_erp))
    body = Body(uv_maps(e_base, BODY, "livery_v2", 2048))
    glove_maps = uv_maps(e_base, GLOVES, "gloves", 2048)
    slots = suitgeo.decal_slots(e_base, f"eaid://{LIVERY}/{BASE}/idf/{BASE}_body_logos_male.idf", "livery_v2_logo",
                                1024)
    gslots = suitgeo.decal_slots(e_base, f"eaid://{LIVERY}/{BASE}/idf/{BASE}_glove_logos_male.idf", "gloves_logo",
                                 512)
    out = ROOT / "kit"
    out.mkdir(exist_ok=True)
    rows = []
    for team, _ in teams:
        if a.teams and team not in a.teams.split(","):
            continue
        name, e, tex = build_team(str(a.game), team, base_erp, e_base, body, glove_maps, slots, gslots)
        e.to_file(str(out / f"{name}.erp"))
        rows.append((team, name, render(e_base, e, name, tex), tex))
        print(f"{team}: kit/{name}.erp")
    a.sheet.parent.mkdir(parents=True, exist_ok=True)
    sheet(rows, a.sheet)
    print(f"sheet: {a.sheet}")


if __name__ == "__main__":
    main(sys.argv)
