import argparse
import copy
import json
import os
import re
import sys

import numpy as np
from PIL import Image
from scipy import ndimage

sys.path.insert(0, '/mnt/work/.mp45b/kn5')
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import porter_layout as pl
import numplates
import atlas
import zones
from gl import G

CLASSIC = '/mnt/work/.mp45b/classic'
TEX = f'{CLASSIC}/tex_full'
TEX_TAIL = f'{CLASSIC}/tex'
GRID = {1: (1, 1), 2: (2, 1), 4: (2, 2)}
CARBON_GREY = (34, 34, 36)
TILE = 2048
AUX_MAX = 1024

CARS = {
    'c_ferrari_90': {'mode': 'uv1', 'glyphs': [(0.40, 0.20, 0.58, 0.28), (0.77, 0.68, 0.91, 0.74)]},
    'c_williams_92': {'mode': 'uv1', 'glyphs': []},
    'c_benetton_94': {'mode': 'bake', 'glyphs': [(0.69, 0.41, 0.78, 0.53), (0.80, 0.41, 0.89, 0.53), (0.89, 0.75, 0.94, 0.86)]},
    'c_mclaren_90': {'mode': 'bake', 'glyphs': [(0.31, 0.41, 0.41, 0.51), (0.68, 0.66, 0.84, 0.78)]},
    'c_mclaren_91': {'mode': 'uv1', 'glyphs': [], 'recolour': 'lotus', 'no_convert': ['c_mclaren_91_carbon4']},
    'c_jordan_91': {'mode': 'uv0', 'glyphs': [], 'recolour': 'tyrrell'},
}
SCHEMES = {'lotus': ((242, 197, 0), (28, 63, 148)), 'tyrrell': ((31, 47, 107), (236, 236, 232))}

STEERING = ('steering_wheel', 'left_paddle', 'right_paddle', 'mix_dial', 'tyre_dial')
MOUNTS = ('x0_suspension_fixed_fl', 'x0_suspension_fixed_fr', 'x0_suspension2_fixed_bl', 'x0_suspension2_fixed_br')
CHAINS = {
    'frontflap_fl': ['frontwing_fl_01'], 'frontflap_fr': ['frontwing_fr_01'],
    'rearwing_bl': ['rearwing_top'], 'rearwing_br': ['rearwing_top'], 'rearwing_top_2': ['rearwing_top'],
    'rearfin_bl': ['rearwing_top'], 'rearfin_br': ['rearwing_top'],
    'topcanopy_bm': ['topcanpopy_bm'], 'windscreen': ['cockpit'], 'diffuser_bm': ['diffuser'], 'diffuser': ['diffuser_bm'],
    'camera_wing': ['topcanopy_bm', 'topcanpopy_bm'],
}


SHADOW_LABELS = ('paint', 'carbon1', 'detail', 'wheel', 'disc')


def shadow_copy(p):
    q = copy.copy(p)
    q.label = 'shadow'
    q.ac_mat = 'SHADOW'
    return q


def map_part(name, available):
    if name in available:
        return name
    m = re.match(r'driveshaft\d*_(bl|br)$', name)
    if m:
        chain = [f'suspension3_{m.group(1)}', f'suspension2_{m.group(1)}', f'suspension_{m.group(1)}']
    else:
        m = re.match(r'suspension_steeringarm_(fl|fr)$', name)
        if m:
            chain = [f'suspension4_{m.group(1)}', f'suspension2_{m.group(1)}', f'suspension_{m.group(1)}']
        else:
            m = re.match(r'suspension\d*_(fl|fr|bl|br)$', name)
            chain = [f'suspension2_{m.group(1)}', f'suspension_{m.group(1)}'] if m else CHAINS.get(name, [])
    for c in chain:
        if c in available:
            return c
    return 'safetycell'


def label_of(mat):
    m = (mat or 'default').lower()
    if m.endswith('_paint'):
        return 'paint'
    if 'carbon' in m:
        return 'carbon1'
    if 'wheel_hub' in m:
        return 'wheel'
    if 'disc' in m:
        return 'disc'
    if 'glass' in m:
        return 'glass'
    if 'rear_light' in m:
        return 'rear_light'
    if 'mirror_left' in m:
        return 'mirror_left'
    if 'mirror_right' in m:
        return 'mirror_right'
    if 'steeringwheel' in m or 'steering_wheel' in m:
        return 'steering_wheel'
    if 'revs' in m or 'rev_light' in m:
        return 'revs'
    if 'lcd' in m:
        return 'lcd'
    return 'detail'


def load_rgba(path):
    return np.asarray(Image.open(path).convert('RGBA')).astype(np.float32) / 255.0


def sample(img, uv, wrap):
    h, w = img.shape[:2]
    u = uv[:, 0] * w - 0.5
    v = uv[:, 1] * h - 0.5
    x0 = np.floor(u).astype(np.int64)
    y0 = np.floor(v).astype(np.int64)
    fx = (u - x0)[:, None]
    fy = (v - y0)[:, None]

    def px(x, y):
        if wrap:
            x, y = x % w, y % h
        else:
            x, y = np.clip(x, 0, w - 1), np.clip(y, 0, h - 1)
        return img[y, x]
    return (px(x0, y0) * (1 - fx) * (1 - fy) + px(x0 + 1, y0) * fx * (1 - fy) +
            px(x0, y0 + 1) * (1 - fx) * fy + px(x0 + 1, y0 + 1) * fx * fy)


def strip_glyphs(dec, boxes):
    h, w = dec.shape[:2]
    for u0, v0, u1, v1 in boxes:
        dec[int(v0 * h):int(np.ceil(v1 * h)), int(u0 * w):int(np.ceil(u1 * w)), 3] = 0
    return dec


def recolour_decal(dec, scheme):
    prim, _ = SCHEMES[scheme]
    white = (dec[..., 3] > 0.5) & (dec[..., :3].min(-1) > 0.75)
    lab, n = ndimage.label(white)
    sizes = ndimage.sum(white, lab, range(1, n + 1))
    keep = np.isin(lab, 1 + np.nonzero(sizes > 0.002 * white.size)[0])
    keep = ndimage.binary_fill_holes(ndimage.binary_closing(keep, iterations=2))
    out = np.zeros_like(dec)
    out[..., :3] = np.array(prim, np.float32) / 255
    out[..., 3] = ndimage.gaussian_filter(keep.astype(np.float32), 0.7)
    return out


def recolour_palette(img, scheme):
    prim, sec = SCHEMES[scheme]
    rgb = img[..., :3]
    out = rgb.copy()
    mx, mn = rgb.max(-1), rgb.min(-1)
    sat = (mx - mn) / np.maximum(mx, 1e-3)
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    green = (sat > 0.25) & (g >= r) & (g >= b)
    blue = (sat > 0.25) & (b > r) & (b > g)
    red = (sat > 0.25) & (r > g) & (r > b)
    shade = mx[..., None]
    out[green] = (np.array(prim, np.float32) / 255 * np.clip(shade[green] / 0.6, 0.6, 1.2)).clip(0, 1)
    out[blue] = np.array(sec, np.float32) / 255
    out[red] = np.array(sec, np.float32) / 255
    res = img.copy()
    res[..., :3] = out
    return res


def composite(dec, base_rgb):
    a = dec[..., 3:4]
    return dec[..., :3] * a + np.array(base_rgb, np.float32)[None, None, :] / 255 * (1 - a)


def up(img, size):
    im = Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8))
    return np.asarray(im.resize(size, Image.BICUBIC)).astype(np.float32) / 255


def carbon_density(prims):
    d = []
    for p in prims:
        t = p.tri[:2000]
        e = np.linalg.norm(p.pos[t[:, 1]] - p.pos[t[:, 0]], axis=1)
        du = np.linalg.norm(p.uv[t[:, 1]] - p.uv[t[:, 0]], axis=1)
        ok = e > 1e-3
        d.append(du[ok] / e[ok])
    return float(np.median(np.concatenate(d))) if d else 0.0


def stock_carbon_density():
    g = G('/mnt/work/.mp45b/porter_a/stock26_export/mclaren_LOD0.gltf')
    out = []
    for i, name, mat, at, tri, W in g.prims():
        if mat and 'carbon' in mat.lower() and 'TEXCOORD_0' in at:
            p = (W[:3, :3] @ at['POSITION'].T).T
            t = tri[:2000]
            e = np.linalg.norm(p[t[:, 1]] - p[t[:, 0]], axis=1)
            du = np.linalg.norm(at['TEXCOORD_0'][t[:, 1]] - at['TEXCOORD_0'][t[:, 0]], axis=1)
            ok = e > 1e-3
            out.append(du[ok] / e[ok])
    return float(np.median(np.concatenate(out)))


def collect(path, available, y_axle):
    g = G(path)
    W = g.world()
    names = {i: n.get('name') for i, n in enumerate(g.j['nodes'])}
    prims = []
    for i, name, mat, at, tri, Wm in g.prims():
        if name.endswith('_water'):
            continue
        R = Wm[:3, :3]
        pos = (R @ at['POSITION'].T).T + Wm[:3, 3]
        pos[:, 1] -= y_axle
        nrm = (np.linalg.inv(R).T @ at['NORMAL'].T).T
        tan = (R @ at['TANGENT'][:, :3].T).T if 'TANGENT' in at else np.tile([1.0, 0, 0], (len(pos), 1))
        lbl = label_of(mat)
        part = map_part(name, available)
        p = pl.Prim(part, 'world', mat or 'default', lbl, pos, nrm, tan, at.get('TEXCOORD_0', np.zeros((len(pos), 2))).astype(np.float64),
                    tri.astype(np.int64), name)
        p.uv0 = p.uv.copy()
        p.uv1src = at.get('TEXCOORD_1', p.uv).astype(np.float64)
        p.orig_part = name
        prims.append(p)
    empt = {}
    for i, n in enumerate(g.j['nodes']):
        if 'mesh' not in n and n.get('name') and i in W:
            t = W[i][:3, 3].copy()
            t[1] -= y_axle
            empt[n['name']] = t
    return prims, empt


def unweld(p, keys, newuv):
    key = np.stack([p.tri.reshape(-1)] + [k.reshape(-1).astype(np.int64) for k in keys], 1).astype(np.int64)
    uniq, inv = np.unique(key, axis=0, return_inverse=True)
    inv = inv.reshape(-1)
    vid = uniq[:, 0]
    nuv = np.zeros((len(uniq), 2))
    nuv[inv] = newuv.reshape(-1, 2)
    q = pl.Prim(p.part, p.mode, p.ac_mat, p.label, p.pos[vid], p.nrm[vid], p.tan[vid], nuv, inv.reshape(-1, 3), p.src)
    for a in ('uv0', 'uv1src'):
        setattr(q, a, getattr(p, a)[vid])
    q.orig_part = p.orig_part
    return q


def tile_uv(uv, tile, ntiles):
    gc, gr = GRID[ntiles]
    u = np.clip(uv[..., 0], 0.002, 0.998)
    v = np.clip(uv[..., 1], 0.002, 0.998)
    col, row = tile % gc, tile // gc
    return np.stack([(col + u) / gc, (row + v) / gr], -1)


def tex_path(car, stem):
    p = f'{TEX}/{car}/{car}_{stem}.png'
    return p if os.path.exists(p) else f'{TEX_TAIL}/{car}/{car}_{stem}.png'


def bake_layout(p, ntiles, D):
    tri_uv = p.uv0[p.tri]
    cen = tri_uv.mean(1)
    fl = np.floor(cen)
    loc = tri_uv - fl[:, None, :]
    t1 = p.uv1src[p.tri]
    ws = np.array([[1, 0, 0], [0, 1, 0], [0, 0, 1], [1 / 3, 1 / 3, 1 / 3], [.6, .2, .2], [.2, .6, .2], [.2, .2, .6],
                   [.5, .5, 0], [0, .5, .5], [.5, 0, .5]])
    pts = np.einsum('kj,tjc->tkc', ws, t1).reshape(-1, 2)
    alpha = sample(D, np.clip(pts, 0, 1), False)[:, 3].reshape(len(p.tri), -1).max(1)
    flag = (alpha > 0.02).astype(np.int64)
    fk = np.repeat(flag[:, None], 3, 1)
    flu = np.repeat(fl[:, None, 0], 3, 1)
    flv = np.repeat(fl[:, None, 1], 3, 1)
    newuv = np.where(fk[..., None] == 1, tile_uv(t1, 1, ntiles), tile_uv(loc, 0, ntiles))
    q = unweld(p, [fk, flu, flv], newuv)
    q.tri_flag = flag
    return q


class Sub:
    def __init__(self, p, mask):
        self.uv, self.uv0, self.uv1src, self.tri = p.uv, p.uv0, p.uv1src, p.tri[mask]


def raster(prims, ntiles, P, D, out, filled):
    H, Wd = out.shape[:2]
    for p in prims:
        tuv = p.uv[p.tri] * np.array([Wd, H])
        s0 = p.uv0[p.tri]
        s1 = p.uv1src[p.tri]
        for k in range(len(p.tri)):
            a, b, c = tuv[k]
            x0, x1 = int(np.floor(min(a[0], b[0], c[0]))), int(np.ceil(max(a[0], b[0], c[0])))
            y0, y1 = int(np.floor(min(a[1], b[1], c[1]))), int(np.ceil(max(a[1], b[1], c[1])))
            if x1 - x0 > 2500 or y1 - y0 > 2500:
                continue
            xs, ys = np.meshgrid(np.arange(x0, x1 + 1) + 0.5, np.arange(y0, y1 + 1) + 0.5)
            q = np.stack([xs.ravel(), ys.ravel()], 1)
            v0, v1 = b - a, c - a
            den = v0[0] * v1[1] - v1[0] * v0[1]
            if abs(den) < 1e-9:
                continue
            d = q - a
            l1 = (d[:, 0] * v1[1] - v1[0] * d[:, 1]) / den
            l2 = (v0[0] * d[:, 1] - d[:, 0] * v0[1]) / den
            l0 = 1 - l1 - l2
            ins = (l0 >= -0.02) & (l1 >= -0.02) & (l2 >= -0.02)
            if not ins.any():
                continue
            bc = np.stack([l0, l1, l2], 1)[ins]
            px = q[ins].astype(np.int64)
            ok = (px[:, 0] >= 0) & (px[:, 0] < Wd) & (px[:, 1] >= 0) & (px[:, 1] < H)
            px, bc = px[ok], bc[ok]
            u0 = bc @ s0[k]
            u1 = bc @ s1[k]
            col = sample(P, u0, True)[:, :3]
            dc = sample(D, u1, False)
            col = dc[:, :3] * dc[:, 3:4] + col * (1 - dc[:, 3:4])
            out[px[:, 1], px[:, 0]] = col
            filled[px[:, 1], px[:, 0]] = True


def dilate(out, filled):
    if filled.all() or not filled.any():
        return out
    _, idx = ndimage.distance_transform_edt(~filled, return_indices=True)
    return out[idx[0], idx[1]]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('car')
    ap.add_argument('out_dir')
    ap.add_argument('--team', required=True)
    ap.add_argument('--tex-out', required=True)
    ap.add_argument('--rim-radius', type=float, default=0.0)
    ap.add_argument('--numbers', action='store_true')
    ap.add_argument('--rim-fill', action='store_true')
    ap.add_argument('--lods', default='0,1,2,3,4,5')
    ap.add_argument('--zones')
    ap.add_argument('--shadow-lod', type=int, default=None)
    args = ap.parse_args()
    car, team = args.car, args.team
    cfg = CARS[car]
    dj = json.load(open('/mnt/work/.mp45b/porter_a/donors26.json'))[team]
    available = set(dj['parts'])
    exp = f'{CLASSIC}/export/{car}/{car}_LOD%d.gltf'
    probe, _ = collect(exp % 0, available, 0.0)
    centres = {}
    for c in ('fl', 'fr', 'bl', 'br'):
        pts = np.concatenate([p.pos[np.unique(p.tri)] for p in probe if p.orig_part == f'wheel_{c}'])
        centres[c] = (pts.min(0) + pts.max(0)) / 2
    y_axle = float(np.mean([v[1] for v in centres.values()]))
    frames = {}
    for c, v in centres.items():
        F = np.eye(4)
        F[3, :3] = v
        frames[c] = F
    lod_src = [int(x) for x in args.lods.split(',')]
    cache = {l: collect(exp % l, available, y_axle) for l in sorted(set(lod_src) | ({args.shadow_lod} if args.shadow_lod is not None else set()))}
    prims0, empt = cache[lod_src[0]]
    steer_ours = empt['steer_master']
    offsets = {p: (np.array(dj['steer']) - steer_ours).tolist() for p in STEERING}
    empties = {'steer_master': steer_ours.tolist()}
    for m in MOUNTS + tuple(dj.get('mounts', [])):
        if m in empt:
            empties[m] = empt[m].tolist()
    arms = {'x0_suspension_fixed_fl': 'suspension_fl', 'x0_suspension_fixed_fr': 'suspension_fr',
            'x0_suspension2_fixed_bl': 'suspension2_bl', 'x0_suspension2_fixed_br': 'suspension2_br'}
    for m, arm in arms.items():
        if m in empties:
            continue
        pts = [p.pos[np.unique(p.tri)] for p in prims0 if p.orig_part == arm]
        if pts:
            pts = np.concatenate(pts)
            ax = np.abs(pts[:, 0])
            empties[m] = pts[ax >= ax.max() - 0.03].mean(0).tolist()
    if 'dummy_driver_position' in empt:
        empties['dummy_driver_position'] = empt['dummy_driver_position'].tolist()
    rim_scale = 1.0
    wpts = np.concatenate([p.pos[np.unique(p.tri)] for p in prims0 if p.orig_part == 'wheel_fl'])
    rim_r = float(np.hypot(wpts[:, 1] - (centres['fl'][1] - y_axle), wpts[:, 2] - centres['fl'][2]).max())
    if args.rim_radius > 0:
        rim_scale = args.rim_radius / rim_r

    os.makedirs(args.tex_out, exist_ok=True)
    D = load_rgba(tex_path(car, 'decal_da'))
    D = strip_glyphs(D, cfg['glyphs'])
    P = load_rgba(tex_path(car, 'paint_d'))
    base = tuple(int(x) for x in (P[..., :3].reshape(-1, 3).mean(0) * 255))
    palette = [tuple(int(v) for v in c) for c in np.unique(np.round(P[..., :3].reshape(-1, 3) * 255).astype(int), axis=0)]
    mode = cfg['mode']
    rec = cfg.get('recolour')
    if rec == 'lotus':
        D = recolour_decal(D, 'lotus')
        base = SCHEMES['lotus'][1]
        palette = [base]
    if rec == 'tyrrell':
        P = recolour_palette(P, 'tyrrell')
        D[..., 3] = 0

    cov = {}
    for p in prims0:
        if p.label != 'carbon1':
            continue
        t = p.uv1src[p.tri].mean(1)
        a = sample(D, np.clip(t, 0, 1), False)[:, 3]
        area = np.linalg.norm(np.cross(p.pos[p.tri[:, 1]] - p.pos[p.tri[:, 0]], p.pos[p.tri[:, 2]] - p.pos[p.tri[:, 0]]), axis=1)
        cov[(p.orig_part, p.ac_mat)] = float((a * area).sum() / max(area.sum(), 1e-9))
    convert = {k for k, v in cov.items() if v > 0.03 and mode != 'uv0' and k[1] not in cfg.get('no_convert', [])}

    zspec = zones.load(args.zones, centres) if args.zones else None
    if zspec and zspec[2] is not None:
        convert = {k for k in cov if k[0] in zspec[2] or f'{k[0]}|{k[1]}' in zspec[2]}
    if mode == 'uv1' and len(palette) > 4 and not zspec:
        raise SystemExit(f'{car}: paint_d has {len(palette)} colours, use bake mode')
    pal = np.array(palette, np.float32) / 255
    ntiles = {'uv1': 1 << int(np.ceil(np.log2(len(palette) + 1))), 'bake': 4, 'uv0': 1}[mode]
    ctile = {'uv1': len(palette), 'bake': 2, 'uv0': None}[mode]
    stock_d = stock_carbon_density()
    cl_d = carbon_density([p for p in prims0 if p.label == 'carbon1'])
    cscale = stock_d / cl_d if cl_d > 0 else 1.0

    for l, (prims, _) in cache.items():
        out = []
        for p in prims:
            if p.label == 'paint' and zspec:
                p.ac_mat = 'WCCARBODY'
            elif p.label == 'paint':
                if mode == 'uv1':
                    col = sample(P, p.uv0[p.tri].mean(1), True)[:, :3]
                    k = np.argmin(((col[:, None, :] - pal[None]) ** 2).sum(-1), 1) if len(pal) > 1 else np.zeros(len(p.tri), int)
                    kk = np.repeat(k[:, None], 3, 1)
                    p = unweld(p, [kk], tile_uv(p.uv1src[p.tri], kk, ntiles))
                elif mode == 'uv0':
                    p.uv = p.uv0.copy()
                else:
                    p = bake_layout(p, ntiles, D)
                p.ac_mat = 'WCCARBODY'
            elif p.label == 'carbon1':
                if (p.orig_part, p.ac_mat) in convert:
                    p.label = 'paint'
                    p.uv = p.uv0.copy() if zspec else tile_uv(p.uv1src, ctile, ntiles)
                    p.ac_mat = 'CONVERTED_CARBON'
                else:
                    p.uv = p.uv0 * cscale
            elif p.label == 'rear_light':
                u0, v0, w, h = atlas.REAR_LIGHT_REGION
                lo, hi = p.uv0.min(0), p.uv0.max(0)
                n = (p.uv0 - lo) / np.maximum(hi - lo, 1e-6)
                p.uv = np.c_[u0 + n[:, 0] * w, v0 + n[:, 1] * h]
            out.append(p)
        prims[:] = zones.apply(out, zspec[1], unweld) if zspec else out

    T = TILE if mode != 'uv0' else 1024
    gc, gr = GRID[ntiles]
    tex = np.zeros((gr * T, gc * T, 3), np.float32)

    def put(k, img):
        col, row = k % gc, k // gc
        tex[row * T:(row + 1) * T, col * T:(col + 1) * T] = img
    if zspec:
        pass
    elif mode == 'uv1':
        Du = up(D, (T, T))
        for k, c in enumerate(palette):
            put(k, composite(Du, c))
        put(ctile, composite(Du, CARBON_GREY))
        for k in range(len(palette), ntiles):
            if k != ctile:
                put(k, np.array(palette[0], np.float32) / 255)
    elif mode == 'uv0':
        tex[:] = np.asarray(Image.fromarray((P[..., :3] * 255).astype(np.uint8)).resize((T, T), Image.NEAREST)).astype(np.float32) / 255
    else:
        filled = np.zeros(tex.shape[:2], bool)
        put(0, up(P[..., :3], (T, T)))
        for l in sorted(cache, reverse=True):
            subs = [Sub(p, p.tri_flag == 1) for p in cache[l][0] if p.label == 'paint' and p.ac_mat == 'WCCARBODY' and hasattr(p, 'tri_flag')]
            raster(subs, ntiles, P, D, tex, filled)
        tex[:T, T:2 * T] = dilate(tex[:T, T:2 * T], filled[:T, T:2 * T])
        put(2, composite(up(D, (T, T)), CARBON_GREY))
        put(3, np.array(base, np.float32) / 255)
    if zspec:
        zones.texture(zspec[0], f'{args.tex_out}/{team}_paint_d.png')
        tex = np.zeros((zones.CELLS * zones.CELL,) * 2)
    else:
        Image.fromarray((np.clip(tex, 0, 1) * 255).astype(np.uint8)).save(f'{args.tex_out}/{team}_paint_d.png')
    aux = {}
    for src, dst in (('detail_d', 'details_d'), ('wheel_d', 'wheel_d'), ('steering_wheel_d', 'steering_wheel_d')):
        im = Image.open(tex_path(car, src)).convert('RGB')
        w, h = im.size
        k = min(1.0, AUX_MAX / max(w, h))
        if k < 1.0:
            im = im.resize((int(w * k), int(h * k)), Image.LANCZOS)
        if max(im.size) < 512:
            im = im.resize((512, 512), Image.BICUBIC)
        im.save(f'{args.tex_out}/{team}_{dst}.png')
        aux[dst] = [w, h, *im.size]
    for stem, rgb in atlas.NEUTRAL.items():
        Image.new('RGB', (64, 64), rgb).save(f"{args.tex_out}/{stem.replace('mclaren_', team + '_')}.png")

    report = {'car': car, 'team': team, 'mode': 'zones' if zspec else mode, 'recolour': args.zones if zspec else rec, 'y_axle': y_axle, 'rim_r': rim_r, 'rim_scale': rim_scale,
              'steer_ours': steer_ours.round(4).tolist(), 'steering_offset': offsets['steering_wheel'],
              'empties': {k: [round(float(x), 4) for x in v] for k, v in empties.items()},
              'wheel_centres': {c: (v - [0, y_axle, 0]).round(4).tolist() for c, v in centres.items()},
              'carbon_logo_coverage': {f'{k[0]}|{k[1]}': round(v, 3) for k, v in cov.items()},
              'carbon_converted_to_paint': sorted(f'{k[0]}|{k[1]}' for k in convert),
              'carbon_uv_scale': cscale, 'base_colour': base, 'palette': palette, 'paint_tex': list(tex.shape[:2]), 'aux_tex': aux, 'decal_src': list(D.shape[:2]),
              'part_map': sorted({f'{p.orig_part}->{p.part}' for p in prims0 if p.orig_part != p.part}), 'lods': {}}

    if args.numbers:
        window = numplates.team_window('/mnt/work/.mp45b/porter_a/stock_number_plates.json', team)
        nose = [p for p in cache[lod_src[0]][0] if p.part == 'nosecone' and p.ac_mat == 'WCCARBODY']
        z = None
        for g in cfg['glyphs']:
            c = np.array([(g[0] + g[2]) / 2, (g[1] + g[3]) / 2])
            for p in nose:
                tri_uv = p.uv1src[p.tri]
                ins, bc = numplates.locate(tri_uv, c)
                for ti in np.nonzero(ins)[0]:
                    p3 = (bc[ti][:, None] * p.pos[p.tri[ti]]).sum(0)
                    if abs(p3[0]) < 0.15:
                        z = float(p3[2])
                        break
                if z is not None:
                    break
            if z is not None:
                break
        npos = np.concatenate([q.pos[np.unique(q.tri)] for q in nose])
        top = npos[np.abs(npos[:, 0]) < 0.05]
        zmax = float(top[:, 2].max())
        if z is None or z > zmax - 0.1:
            z = zmax - 0.40
        plate, rep = numplates.nose_plate(cache[lod_src[0]][0], pl.Prim, window, (0.0, z))
        rep['window'] = window
        rep['nose_tip_z'] = zmax
        for l in cache:
            cache[l][0].append(plate)
        report['number_plates'] = rep

    os.makedirs(args.out_dir, exist_ok=True)
    for f1_lod, l in enumerate(lod_src):
        prims = cache[l][0]
        if args.shadow_lod is not None and f1_lod == len(lod_src) - 1:
            prims = prims + [shadow_copy(p) for p in cache[args.shadow_lod][0] if p.label in SHADOW_LABELS]
        g, stats = pl.build_lod(prims, frames, y_axle, empties, f'{car} on {team} LOD{f1_lod} from classic LOD{l}', offsets, rim_scale,
                                (0.5, 0.5), fill_rims=args.rim_fill)
        path = os.path.join(args.out_dir, f'LOD{f1_lod}.glb')
        g.write_glb(path)
        report['lods'][f1_lod] = {'source': l, 'parts': len(stats), 'triangles': sum(s['triangles'] for s in stats.values())}
        print(f1_lod, l, len(stats), report['lods'][f1_lod]['triangles'])
    json.dump(report, open(os.path.join(args.out_dir, 'classic_report.json'), 'w'), indent=1)


if __name__ == '__main__':
    main()
