#!/usr/bin/env python3
"""F1 2020 suit mods moved onto F1 25's driver body, written as F1 25 suit mods for build_real_suits.py.

A mod of data/real_suits.json with "transfer" (its F1 2020 folder under --mods) has its suits converted here into its
"folder" under --mods, at each entry's "erp", from the F1 2020 suit or driver archive named by the entry's "from"
(2020-<team>, or 2020-<team>_<driver> for the driver archive under drivers/male/). build_real_suits.py then relabels the
converted archives like any other F1 25 suit mod.

F1 2020's team suit archive holds its own copy of the old body (condition_scene/idf/driver_body_male.idf, submeshes
`livery`), its boots (<suit>_male.idf, `boots`), its gloves (driver_gloves.idf) and its decal meshes
(<suit>_body_logos_male.idf, <suit>_glove_logos_male.idf). A driver archive (<driver>.erp) holds the driver's own
decal meshes (<driver>_body_logos.idf, <driver>_glove_logos.idf), boots (<driver>.idf), usually its own boots_d and
glove_logo_d, and copies of team textures. A texture takes the driver archive's own, else its copy of the team's,
else the team archive's. The old body, gloves and arms sit in the same model space as
F1 25's (the arm and glove vertex buffers are identical), so no alignment is needed.

For a suit:
- The old diffuse gets its decals baked in: each decal triangle is drawn in the space of its first texture coordinate
  pair (the body UV under it) with the logo atlas sampled at its second pair, alpha over the diffuse.
- livery_v2_d: every texel of F1 25's livery_v2 UV layout gets the 3D point and normal of the body under it, the
  nearest point of the old body's livery surface whose normal faces the same way (within NORMAL_DOT; a point more
  than FACING_PENALTY metres nearer wins anyway), and the baked old diffuse there (bilinear). Where that point is
  more than NEAR metres away (F1 25's cuffs, collar and zip, which the old body shapes differently), the texel takes
  the affine map from new UV to old UV fitted to the texels of its UV island within NEAR (the whole layout's for an
  island with fewer than ISLAND_MIN of them), blended in up to FAR. A texel whose nearest point lands more than
  DEVIATION texels (or four times the island's median) from that affine map's UV takes the affine map too, blended
  in up to twice that, which drops nearby but wrong matches (the old sleeve's inside under F1 25's cuff). Texels
  outside every UV island take the nearest covered texel. The map from new texel to old UV depends only on the two
  meshes and is cached in --cache.
- boots_d: the same from the old boots to the F1 25 suit's `boots` submesh (<base>_male.idf, or <stem>.idf of a
  driver suit).
- glove_d: the old glove diffuse with its glove decals baked in; F1 2020 and F1 25 share the glove mesh and UVs.
- A team suit's logo_01_d and glove_logo_d become transparent, logo_01_n and glove_logo_n flat: the decals are in the
  diffuse. A driver suit's livery_v2_d, boots_d and glove_d are its own, with the driver's decals; its copies of team
  textures take the converted team suit's (the team suit of the same mod and base team converts first).
- An F1 25 driver suit without its own livery_v2_d, boots_d or glove_d (every stock driver suit but Mercedes' wears
  the team's) gets one: a copy of its copy of the team's texture and view under the driver's textures/ name, which
  the driver's materials then name instead of the team's.
Every other texture keeps the F1 25 suit's.

    python3 tools/suit_transfer.py --game <F1 25> --mods <folder holding the mod folders> [--only <label>,...]
                                   [--cache <dir>]

--only takes build_real_suits labels (cs17-mercedes, cs17-mercedes_george_russell); a driver suit converts its team suit
too. Writes art/suits/transfer/<season>-<stock team>.png per team: for the team suit and each driver suit, front and back
renders of the old body and of F1 25's body, the baked old diffuse and the new livery_v2_d.
"""

import argparse
import hashlib
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[2]
sys.path[:0] = [str(HERE), str(REPO / "tools")]

import bcn
import build_suits
import erpw
import suit_sources
import suitgeo

REAL = suit_sources.REAL
TEAMS = "character_package/team_livery"
DRIVERS = "character_package/drivers/male"
OLD_BODY = "eaid://character_package/condition_scene/idf/driver_body_male.idf"
OLD_GLOVES = "eaid://character_package/condition_scene/idf/driver_gloves.idf"
NEW_BODY = build_suits.BODY
SIZES = build_suits.SIZES
ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"
NORMAL_DOT = 0.2
FACING_PENALTY = 0.05
CANDIDATES = 24
NEAR, FAR = 0.004, 0.008
ISLAND_SAMPLES, ISLAND_MIN = 20000, 64
DEVIATION = 16
MATERIAL_TEXTURE = 2
OWN_PARTS = ("livery_v2_d", "boots_d", "glove_d")
TEAM_PARTS = ("livery_v2_d", "boots_d", "glove_d", "logo_01_d", "glove_logo_d", "logo_01_n", "glove_logo_n")
PREVIEW = ROOT / "art" / "suits" / "transfer"


class OldErp(erpw.Erp):
    def frag_data(self, r, i):
        raw = r.frags[i].raw
        if raw[:4] != ZSTD_MAGIC:
            return raw
        import zstandard
        return zstandard.ZstdDecompressor().decompressobj().decompress(raw)


class OldMesh:
    def __init__(self, e, base, lod=0):
        d = e.frag_data(e.find(f"{base}?lod={lod}&mesh"), 0)
        n, _, _, _, _, refs, _, names = struct.unpack_from("<8i", d, 0)
        refs, names = 36 + refs, 36 + names

        def text(at):
            return d[at:d.index(b"\0", at)].decode()

        self.e, self.parts = e, []
        for i in range(n):
            f = struct.unpack_from("<22I", d, 36 + 88 * i)
            self.parts.append((text(names + f[12]), f[3], f[4], [text(refs + f[14 + k]) for k in range(f[0])]))
        self.ib = f"{base}?lod=all&ib=0"

    def buffer(self, name):
        r = self.e.find(name)
        head = struct.unpack("<4I", self.e.frag_data(r, 0))
        dtype, n = suitgeo.LAYOUTS[head[3]]
        return np.frombuffer(self.e.frag_data(r, 1), dtype).reshape(-1, n).astype(np.float64)

    def submeshes(self, name):
        r = self.e.find(self.ib)
        head = struct.unpack("<4I", self.e.frag_data(r, 0))
        idx = np.frombuffer(self.e.frag_data(r, 1), "<u2" if head[1] == 2 else "<u4").astype(np.int64)
        out, seen = [], set()
        for part, start, count, streams in self.parts:
            if part != name:
                continue
            tris = idx[start:start + count].reshape(-1, 3)
            key = (tuple(streams), hashlib.md5(tris.tobytes()).hexdigest())
            if key in seen:
                continue
            seen.add(key)
            out.append((self.buffer(streams[0]), self.buffer(streams[1])[:, :3], self.buffer(streams[3]), tris))
        return out


def new_submeshes(e, base, name):
    m = suitgeo.lod0(e, base)
    out, seen = [], set()
    for i in range(m.n):
        if m.names[i] != name:
            continue
        tris = suitgeo.triangles(e, m, i)
        key = hashlib.md5(tris.tobytes()).hexdigest()
        if key in seen:
            continue
        seen.add(key)
        out.append((suitgeo.stream(e, m, i, suitgeo.POSITION), suitgeo.stream(e, m, i, 2)[:, :3],
                    suitgeo.stream(e, m, i, suitgeo.TEXCOORD), tris))
    return out


def merge(parts):
    pos, nrm, uv, tris, at = [], [], [], [], 0
    for p, n, u, t in parts:
        pos.append(p)
        nrm.append(n)
        uv.append(u)
        tris.append(t + at)
        at += len(p)
    return np.concatenate(pos), np.concatenate(nrm), np.concatenate(uv), np.concatenate(tris)


def old_sidecar(desc):
    n = desc[0]
    path = desc[1:1 + n].rstrip(b"\0").decode().replace("\\", "/")
    count = struct.unpack_from("<I", desc, 1 + n)[0]
    return path, [struct.unpack_from("<BQQQ", desc, 1 + n + 4 + 25 * i) for i in range(count)]


def old_surface(e, name, root):
    r = e.find(name)
    frags = [e.frag_data(r, i) for i in range(len(r.frags))]
    fmt, w, h, _ = bcn.Surface.header(frags[0])
    if len(frags) > 2 and frags[2]:
        path, levels = old_sidecar(frags[2])
        if levels:
            full = Path(root) / path
            if not full.exists():
                hits = sorted(Path(root).rglob(Path(path).name))
                if not hits:
                    print(f"  {name.split('/')[-1]}: no sidecar {Path(path).name} in the mod, using the {w}x{h} "
                          f"archive level")
                    return bcn.decode(fmt, frags[1], w, h)
                full = hits[0]
            comp, off, packed, size = levels[0]
            with open(full, "rb") as f:
                f.seek(off)
                raw = f.read(packed)
            if len(raw) < packed:
                raise SystemExit(f"{name}: sidecar {full} is {full.stat().st_size} bytes, level 0 needs {off + packed}")
            if comp == bcn.LZ4:
                import lz4.block
                raw = lz4.block.decompress(raw, uncompressed_size=size)
            k = len(levels)
            return bcn.decode(fmt, raw, w << k, h << k)
    return bcn.decode(fmt, frags[1], w, h)


def bilinear(tex, uv):
    h, w = tex.shape[:2]
    x = (uv[..., 0] % 1.0) * w - 0.5
    y = (uv[..., 1] % 1.0) * h - 0.5
    x0, y0 = np.floor(x).astype(np.int64), np.floor(y).astype(np.int64)
    fx, fy = (x - x0)[..., None], (y - y0)[..., None]
    t = tex.astype(np.float32)

    def at(yy, xx):
        return t[yy % h, xx % w]
    top = at(y0, x0) * (1 - fx) + at(y0, x0 + 1) * fx
    bottom = at(y0 + 1, x0) * (1 - fx) + at(y0 + 1, x0 + 1) * fx
    return top * (1 - fy) + bottom * fy


def bake_decals(diffuse, decals, atlas):
    out = diffuse[..., :3].astype(np.float32)
    h, w = out.shape[:2]
    for _, _, uv, tris in decals:
        body = uv[:, :2] * [w, h]
        found, cover, _ = suitgeo.raster(body[tris], uv[:, 2:4][tris], h, w)
        ys, xs = np.nonzero(cover)
        px = bilinear(atlas, found[ys, xs])
        a = px[:, 3:4] / 255
        out[ys, xs] = out[ys, xs] * (1 - a) + px[:, :3] * a
    return np.dstack([np.clip(np.rint(out), 0, 255).astype(np.uint8), np.full((h, w), 255, np.uint8)])


def closest_on_triangles(p, a, b, c):
    ab, ac, ap = b - a, c - a, p - a
    bp, cp = p - b, p - c

    def dot(x, y):
        return np.einsum("...i,...i->...", x, y)
    d1, d2 = dot(ab, ap), dot(ac, ap)
    d3, d4 = dot(ab, bp), dot(ac, bp)
    d5, d6 = dot(ab, cp), dot(ac, cp)
    va, vb, vc = d3 * d6 - d5 * d4, d5 * d2 - d1 * d6, d1 * d4 - d3 * d2
    denom = va + vb + vc
    denom = np.where(np.abs(denom) < 1e-30, 1e-30, denom)
    v, w = vb / denom, vc / denom
    bary = np.stack([1 - v - w, v, w], -1)

    def put(mask, value):
        bary[mask] = value[mask] if value.ndim == bary.ndim else value
    with np.errstate(divide="ignore", invalid="ignore"):
        t = (d4 - d3) / ((d4 - d3) + (d5 - d6))
        put((va <= 0) & (d4 - d3 >= 0) & (d5 - d6 >= 0), np.stack([np.zeros_like(t), 1 - t, t], -1))
        t = d2 / (d2 - d6)
        put((vb <= 0) & (d2 >= 0) & (d6 <= 0), np.stack([1 - t, np.zeros_like(t), t], -1))
        put((d6 >= 0) & (d5 <= d6), np.broadcast_to(np.array([0.0, 0.0, 1.0]), bary.shape))
        t = d1 / (d1 - d3)
        put((vc <= 0) & (d1 >= 0) & (d3 <= 0), np.stack([1 - t, t, np.zeros_like(t)], -1))
        put((d3 >= 0) & (d4 <= d3), np.broadcast_to(np.array([0.0, 1.0, 0.0]), bary.shape))
        put((d1 <= 0) & (d2 <= 0), np.broadcast_to(np.array([1.0, 0.0, 0.0]), bary.shape))
    q = bary[..., :1] * a + bary[..., 1:2] * b + bary[..., 2:] * c
    return q, bary


def surface_points(pos, nrm, uv, tris, w, h):
    px = uv[:, :2] * [w, h]
    attrs = np.concatenate([pos, nrm], 1)[tris]
    found, cover, _ = suitgeo.raster(px[tris], attrs, h, w)
    return found, cover


def uv_map(new, old, w, h):
    from scipy.spatial import cKDTree
    npos, nnrm, nuv, ntris = new
    opos, onrm, ouv, otris = old
    found, cover = surface_points(npos, nnrm, nuv, ntris, w, h)
    ys, xs = np.nonzero(cover)
    p = found[ys, xs, :3]
    pn = found[ys, xs, 3:6]
    pn /= np.maximum(np.linalg.norm(pn, axis=1, keepdims=True), 1e-12)
    a, b, c = opos[otris[:, 0]], opos[otris[:, 1]], opos[otris[:, 2]]
    samples = np.concatenate([a, b, c, (a + b) / 2, (b + c) / 2, (a + c) / 2, (a + b + c) / 3])
    owner = np.tile(np.arange(len(otris)), 7)
    tree = cKDTree(samples)
    out = np.full((h, w, 2), np.nan, np.float32)
    dist = np.full((h, w), np.nan, np.float32)
    for s in range(0, len(p), 65536):
        q = p[s:s + 65536]
        qn = pn[s:s + 65536]
        _, near = tree.query(q, CANDIDATES, workers=-1)
        cand = owner[near]
        t = otris[cand]
        hit, bary = closest_on_triangles(q[:, None], opos[t[..., 0]], opos[t[..., 1]], opos[t[..., 2]])
        d = np.linalg.norm(hit - q[:, None], axis=2)
        on = (bary[..., :1] * onrm[t[..., 0]] + bary[..., 1:2] * onrm[t[..., 1]] + bary[..., 2:] * onrm[t[..., 2]])
        on /= np.maximum(np.linalg.norm(on, axis=2, keepdims=True), 1e-12)
        facing = np.einsum("nki,ni->nk", on, qn)
        best = np.argmin(d + FACING_PENALTY * (facing < NORMAL_DOT), 1)
        rows = np.arange(len(q))
        tb, bb = t[rows, best], bary[rows, best]
        uv = bb[:, :1] * ouv[tb[:, 0], :2] + bb[:, 1:2] * ouv[tb[:, 1], :2] + bb[:, 2:] * ouv[tb[:, 2], :2]
        out[ys[s:s + 65536], xs[s:s + 65536]] = uv
        dist[ys[s:s + 65536], xs[s:s + 65536]] = d[rows, best]
    return out, cover, dist


def cached_map(cache, new, old, w, h):
    key = hashlib.sha1(b"".join(np.ascontiguousarray(x).tobytes() for x in (*new, *old)) +
                       f"{w}x{h}:{NORMAL_DOT}:{FACING_PENALTY}:{CANDIDATES}".encode()).hexdigest()[:16]
    path = Path(cache) / f"uvmap-{key}.npz"
    if path.exists():
        z = np.load(path)
        return z["uv"], z["cover"], z["dist"]
    uv, cover, dist = uv_map(new, old, w, h)
    path.parent.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(path, uv=uv, cover=cover, dist=dist)
    return uv, cover, dist


def affine(src, dst):
    a = np.concatenate([src, np.ones((len(src), 1))], 1)
    m = np.linalg.lstsq(a, dst, rcond=None)[0]
    err = np.linalg.norm(a @ m - dst, axis=1)
    keep = err <= 3 * np.median(err) + 1e-6
    return np.linalg.lstsq(a[keep], dst[keep], rcond=None)[0]


def continue_map(uv, cover, dist):
    from scipy import ndimage
    h, w = cover.shape
    ys, xs = np.mgrid[0:h, 0:w]
    grid = np.stack([(xs + 0.5) / w, (ys + 0.5) / h], -1)
    d = np.nan_to_num(dist, nan=np.inf)
    good = cover & (d < NEAR)
    if good.sum() < 3:
        return uv
    rng = np.random.default_rng(0)

    def fit(sl, mask):
        pick = np.flatnonzero(mask.ravel())
        if len(pick) > ISLAND_SAMPLES:
            pick = np.sort(rng.choice(pick, ISLAND_SAMPLES, replace=False))
        return affine(grid[sl].reshape(-1, 2)[pick], uv[sl].reshape(-1, 2)[pick].astype(np.float64))

    whole = fit((slice(None), slice(None)), good)
    carried = grid @ whole[:2] + whole[2]
    limit = np.full((h, w), DEVIATION / w)
    islands, n = ndimage.label(cover)
    for i, sl in enumerate(ndimage.find_objects(islands), 1):
        part = islands[sl] == i
        near = part & good[sl]
        m = fit(sl, near) if near.sum() >= ISLAND_MIN else whole
        carried[sl][part] = grid[sl][part] @ m[:2] + m[2]
        if near.sum() >= ISLAND_MIN:
            spread = np.median(np.linalg.norm(uv[sl][near] - carried[sl][near], axis=1))
            limit[sl][part] = max(DEVIATION / w, 4 * spread)
    off = np.linalg.norm(np.nan_to_num(uv) - carried, axis=2)
    t = np.maximum(np.clip((d - NEAR) / (FAR - NEAR), 0, 1), np.clip(off / limit - 1, 0, 1))[..., None]
    return np.where(cover[..., None], (1 - t) * uv + t * carried, uv).astype(np.float32)


def apply_map(tex, uv, cover):
    from scipy import ndimage
    h, w = cover.shape
    out = np.zeros((h, w, 3), np.float32)
    out[cover] = bilinear(tex[..., :3], uv[cover])
    idx = ndimage.distance_transform_edt(~cover, return_distances=False, return_indices=True)
    out = out[idx[0], idx[1]]
    return np.dstack([np.clip(np.rint(out), 0, 255).astype(np.uint8), np.full((h, w), 255, np.uint8)])


def resize(img, size):
    return np.asarray(Image.fromarray(img).resize(size, Image.LANCZOS))


def transparent(size):
    return np.zeros((size[1], size[0], 4), np.uint8)


def old_kit(root, slot, asset_year=2020, archive_type=OldErp):
    team, _, name = slot.partition("_")
    team_erp = archive_type(str(root / f"{asset_year}_asset_groups" / TEAMS / team / f"{team}.erp"))
    tex = f"eaid://{TEAMS}/{team}/textures/{team}_"
    if name:
        e = archive_type(str(root / f"{asset_year}_asset_groups" / DRIVERS / slot / f"{name}.erp"))
        own = f"eaid://{DRIVERS}/{slot}/textures/{name}_"
        idf = f"eaid://{DRIVERS}/{slot}/idf/{name}"
        logos, glove_logos, feet = f"{idf}_body_logos.idf", f"{idf}_glove_logos.idf", f"{idf}.idf"
    else:
        e, own = team_erp, tex
        idf = f"eaid://{TEAMS}/{team}/idf/{team}"
        logos, glove_logos, feet = f"{idf}_body_logos_male.idf", f"{idf}_glove_logos_male.idf", f"{idf}_male.idf"
    names = {r.name for r in e.res}

    def texture(part):
        for archive, prefix in ((e, own), (e, tex), (team_erp, tex)):
            name = f"{prefix}{part}.tif.image"
            if name in names or archive is team_erp:
                return old_surface(archive, name, root)

    diffuse, atlas, glove, gatlas, boots = (texture(p) for p in ("livery_d", "logo_01_d", "glove_d", "glove_logo_d",
                                                                 "boots_d"))
    return {
        "body": merge(OldMesh(team_erp, OLD_BODY).submeshes("livery")),
        "boots_mesh": merge(OldMesh(e, feet).submeshes("boots")),
        "gloves_mesh": merge(OldMesh(team_erp, OLD_GLOVES).submeshes("gloves")),
        "livery": bake_decals(diffuse, OldMesh(e, logos).submeshes("livery_logo"), atlas),
        "glove": bake_decals(glove, OldMesh(e, glove_logos).submeshes("gloves_logo"), gatlas),
        "boots": boots,
    }


def old_render(old):
    views = []
    for back in (False, True):
        v = suitgeo.View(640, 440, back)
        for (pos, _, uv, tris), tex in ((old["body"], old["livery"]), (old["boots_mesh"], old["boots"]),
                                        (old["gloves_mesh"], old["glove"])):
            v.draw(pos, uv, tris, tex)
        views.append(v.image((52, 54, 60)))
    return views


def put(e, name, img, size):
    r = e.find(name)
    head = e.frag_data(r, 0)
    fmt = bcn.Surface.header(head)[0]
    if img is None:
        data, levels = build_suits.flat_normal(*size)
        head = bytearray(head)
        struct.pack_into("<2I", head, 12, *size)
        struct.pack_into("<I", head, 24, levels)
        head = bytes(head)
    else:
        head, data, levels = bcn.surface_frags(head, fmt, img)
    build_suits.put_texture(e, name, head, data, levels, fmt)


def retarget(b, swap):
    n, ncb, vsz, o_sh, o_nm, o_t1, o_t2, o_cb = struct.unpack_from("<8I", b, 0)
    base = 0x20 + 36 * n
    recs = [list(struct.unpack_from("<9I", b, 0x20 + 36 * i)) for i in range(n)]
    pool, seen, moved, hit = bytearray(), {}, {}, False
    for r in recs:
        if r[1] != MATERIAL_TEXTURE:
            continue
        at = base + o_t1 + r[7]
        name = b[at:b.index(b"\0", at)].decode()
        hit |= name in swap
        name = swap.get(name, name)
        if name not in seen:
            seen[name] = len(pool)
            pool += name.encode() + b"\0"
        moved[r[7]] = seen[name]
        r[7], r[4] = seen[name], len(name) + 1
    if not hit:
        return b
    for r in recs:
        if r[1] != MATERIAL_TEXTURE and r[8] in moved:
            r[8] = moved[r[8]]
    starts = {r[7] for r in recs if r[1] == MATERIAL_TEXTURE}
    if any(r[1] != MATERIAL_TEXTURE and r[8] and r[8] not in starts for r in recs):
        raise SystemExit("a material parameter names a texture offset no texture starts at")
    grow = len(pool) - (o_t2 - o_t1)
    head = struct.pack("<8I", n, ncb, vsz, o_sh, o_nm, o_t1, o_t2 + grow, o_cb + grow)
    return (head + b"".join(struct.pack("<9I", *r) for r in recs) + b[base:base + o_t1] + bytes(pool) +
            b[base + o_t2:])


def personalize(e, base, stem, team_base, parts):
    team_tex = f"eaid://{TEAMS}/{team_base}/textures/{team_base}_"
    own_tex = f"eaid://{DRIVERS}/{base}/textures/{stem}_"
    names = {r.name for r in e.res}
    swap = {}
    for part in parts:
        own, team = f"{own_tex}{part}.tif", f"{team_tex}{part}.tif"
        if f"{own}.image" in names or f"{team}.image" not in names:
            continue
        for src, dst in ((f"{team}.image", f"{own}.image"), (team, own)):
            r = e.find(src)
            clone = erpw.Res()
            clone.name, clone.type, clone.unk, clone.hash = dst, r.type, r.unk, r.hash
            clone.frags = []
            for fr in r.frags:
                c = erpw.Frag()
                for k in erpw.Frag.__slots__:
                    setattr(c, k, getattr(fr, k))
                clone.frags.append(c)
            e.res.insert(e.res.index(r) + 1, clone)
        srv = e.find(own)
        e.set_frag(srv, 0, e.frag_data(srv, 0)[:24] + f"{own}.image".encode() + b"\0")
        swap[team] = own
    for r in e.res:
        if erpw_kind(r) == "GfxMaterialRes" and f"/{DRIVERS}/{base}/materials/" in r.name:
            d = e.frag_data(r, 0)
            moved = retarget(d, swap)
            if moved != d:
                e.set_frag(r, 0, moved)
    return sorted(swap)


def erpw_kind(r):
    return r.type.rstrip(b"\0").decode()


def copy_resource(e, src, name):
    mine = e.find(name)
    theirs = src.find(name)
    mine.frags = theirs.frags
    mine.hash = theirs.hash


def driver_paths(entry):
    base, stem = entry["base"], Path(entry["erp"]).stem
    idf = f"eaid://{DRIVERS}/{base}/idf/{stem}"
    return (f"eaid://{DRIVERS}/{base}/textures/{stem}_",
            {"feet": f"{idf}.idf", "logos": f"{idf}_body_logos.idf", "glove_logos": f"{idf}_glove_logos.idf"})


def convert(game, mods, cache, entry, team_out):
    mod = REAL["mods"][entry["mod"]]
    root = Path(mods) / mod["transfer"]
    base = entry["base"]
    team_base = base.split("_", 1)[0]
    e = erpw.Erp(str(Path(game) / entry["erp"]))
    old = old_kit(root, entry["from"])
    if "driver" in entry:
        own, meshes = driver_paths(entry)
        e_body = team_out[(entry["mod"], team_base)]
    else:
        own, meshes = f"eaid://{TEAMS}/{base}/textures/{base}_", build_suits.team_meshes(base)
        e_body = e
    w, h = SIZES["livery_v2_d"]
    uv, cover, dist = cached_map(cache, merge(new_submeshes(e_body, NEW_BODY, "livery_v2")), old["body"], w, h)
    bw, bh = SIZES["boots_d"]
    buv, bcover, bdist = cached_map(cache, merge(new_submeshes(e, meshes["feet"], "boots")), old["boots_mesh"], bw, bh)
    tex = {"livery_v2_d": apply_map(old["livery"], continue_map(uv, cover, dist), cover),
           "boots_d": apply_map(old["boots"], continue_map(buv, bcover, bdist), bcover),
           "glove_d": resize(old["glove"], SIZES["glove_d"]),
           "logo_01_d": transparent(SIZES["logo_01_d"]), "glove_logo_d": transparent(SIZES["glove_logo_d"])}
    if "driver" in entry:
        personalize(e, base, Path(entry["erp"]).stem, team_base, OWN_PARTS)
    names = {r.name for r in e.res}
    for part in OWN_PARTS:
        put(e, f"{own}{part}.tif.image", tex[part], SIZES[part])
    if "driver" in entry:
        team = team_out[(entry["mod"], team_base)]
        prefix = f"eaid://{TEAMS}/{team_base}/textures/{team_base}_"
        for part in TEAM_PARTS:
            for name in (f"{prefix}{part}.tif.image", f"{prefix}{part}.tif"):
                if name in names:
                    copy_resource(e, team, name)
    else:
        for part in ("logo_01_d", "glove_logo_d", "logo_01_n", "glove_logo_n"):
            put(e, f"{own}{part}.tif.image", tex.get(part), SIZES[part])
    out = Path(mods) / mod["folder"] / entry["erp"]
    out.parent.mkdir(parents=True, exist_ok=True)
    e.to_file(str(out))
    views = build_suits.render(e_body, e, meshes, tex)
    far = np.nanpercentile(dist[cover], [50, 99]).round(4).tolist()
    return e, {"old": old["livery"], "new": tex["livery_v2_d"], "old_views": old_render(old), "views": views,
               "distance": far}


def sheet(title, rows, out):
    tw, th, tex = 440, 640, 316
    font = build_suits.font
    img = Image.new("RGB", (4 * tw + tex + 70, 50 + len(rows) * (th + 40)), (30, 31, 35))
    d = ImageDraw.Draw(img)
    d.text((16, 12), title, fill=(235, 235, 235), font=font(24))
    for n, (caption, art) in enumerate(rows):
        y = 50 + n * (th + 40)
        d.text((16, y + 6), caption, fill=(220, 220, 220), font=font(18))
        y += 34
        for k, view in enumerate(art["old_views"] + art["views"]):
            img.paste(Image.fromarray(view), (10 + k * tw + (k >= 2) * 20, y))
        x = 40 + 4 * tw
        for k, tx in enumerate((art["old"], art["new"])):
            img.paste(Image.fromarray(tx[..., :3]).resize((tex, tex), Image.LANCZOS), (x, y + k * (tex + 8)))
    out.parent.mkdir(parents=True, exist_ok=True)
    build_suits.shrink(img).save(out, optimize=True)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--mods", required=True, type=Path, help="folder holding the mod folders of data/real_suits.json")
    ap.add_argument("--only", help="comma-separated labels to convert (default: all)")
    ap.add_argument("--cache", type=Path, default=ROOT / "build" / "suit-transfer")
    a = ap.parse_args(argv[1:])
    only = set(a.only.split(",")) if a.only else None
    entries = [e for e in suit_sources.real() if e["kind"] == "mod" and "transfer" in REAL["mods"][e["mod"]]]
    entries.sort(key=lambda e: ("driver" in e))
    selected = [e for e in entries if not only or e["label"] in only]
    needed = {(e["mod"], e["base"].split("_", 1)[0]) for e in selected}
    team_out, rows = {}, {}
    for entry in entries:
        team_base = entry["base"].split("_", 1)[0]
        if entry not in selected and ("driver" in entry or (entry["mod"], team_base) not in needed):
            continue
        e, art = convert(a.game, a.mods, a.cache, entry, team_out)
        if "driver" not in entry:
            team_out[(entry["mod"], team_base)] = e
        if entry not in selected:
            continue
        who = entry.get("driver", "team suit")
        caption = f"{entry['label']}: {who}, from {entry['from']}, base {entry['base']}"
        rows.setdefault((entry["mod"], entry["season"], team_base, entry["team"]), []).append((caption, art))
        print(f"{entry['label']}: {entry['season']} {entry['team']} {who} from {entry['from']} onto {entry['base']}, "
              f"distance median {art['distance'][0]} m, p99 {art['distance'][1]} m", flush=True)
    for (mod, season, team_base, team), suits in rows.items():
        name = f"{season}-{team_base[5:]}"
        sheet(f"{season} {team} from {REAL['mods'][mod]['name']}: old body, F1 25 body, old diffuse over new livery_v2_d",
              suits, PREVIEW / f"{name}.png")


if __name__ == "__main__":
    main(sys.argv)
