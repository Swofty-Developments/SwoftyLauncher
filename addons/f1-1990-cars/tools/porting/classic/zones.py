import json

import numpy as np
from PIL import Image

import porter_layout as pl

CELLS = 4
CELL = 256
ATTRS = ('pos', 'nrm', 'tan', 'uv', 'uv0', 'uv1src')


def load(path, centres):
    spec = json.load(open(path))
    zf = float(np.mean([centres['fl'][2], centres['fr'][2]]))
    zr = float(np.mean([centres['bl'][2], centres['br'][2]]))
    names = list(spec['palette'])
    rules = []
    for r in spec['rules']:
        hs = []
        box = r.get('box', {})
        for ax, k in (('x', 0), ('y', 1)):
            if ax in box:
                lo, hi = box[ax]
                e = np.zeros(3)
                e[k] = 1
                hs.append((-e, -lo))
                hs.append((e, hi))
        if 't' in box:
            lo, hi = box['t']
            e = np.array([0, 0, 1.0])
            hs.append((-e, -(zr + lo * (zf - zr))))
            hs.append((e, zr + hi * (zf - zr)))
        for a, b, c, d in r.get('planes', []):
            n = np.array([a, b, c / (zf - zr)], float)
            hs.append((n, d + c * zr / (zf - zr)))
        rules.append({'colour': names.index(r['colour']), 'parts': r.get('parts'), 'not_parts': r.get('not_parts'),
                      'hs': hs, 'ny': r.get('ny'), 'nx': r.get('nx'), 'nz': r.get('nz')})
    pal = [tuple(spec['palette'][n]) for n in names]
    return pal, rules, spec.get('convert_carbon')


def _interp(p, i, j, t):
    out = {}
    for a in ATTRS:
        v = getattr(p, a)
        w = v[i] * (1 - t) + v[j] * t
        if a in ('nrm', 'tan'):
            w = w / max(np.linalg.norm(w), 1e-9)
        out[a] = w
    return out


def split(p, n, d, side):
    pos = p.pos
    s = pos @ n - d
    eps = 1e-6
    sg = np.where(s > eps, 1, np.where(s < -eps, -1, 0))
    cx = pos[p.tri].mean(1)[:, 0]
    ts = sg[p.tri]
    cross = (ts.max(1) == 1) & (ts.min(1) == -1)
    if side is not None:
        cross &= (cx >= 0) if side > 0 else (cx < 0)
    if not cross.any():
        return p
    extra = {a: [] for a in ATTRS}
    nv = len(pos)
    cache = {}

    def mid(i, j):
        nonlocal nv
        key = (min(i, j), max(i, j))
        if key in cache:
            return cache[key]
        t = s[i] / (s[i] - s[j])
        for a, v in _interp(p, i, j, t).items():
            extra[a].append(v)
        cache[key] = nv
        nv += 1
        return cache[key]
    new = []
    for tri in p.tri[cross]:
        tri = [int(v) for v in tri]
        g = [sg[v] for v in tri]
        if 0 in g:
            k = g.index(0)
            a, b, c = tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]
            m = mid(b, c)
            new += [(a, b, m), (a, m, c)]
        else:
            k = next(i for i in range(3) if g[i] != g[(i + 1) % 3] and g[i] != g[(i + 2) % 3])
            a, b, c = tri[k], tri[(k + 1) % 3], tri[(k + 2) % 3]
            mab, mac = mid(a, b), mid(a, c)
            new += [(a, mab, mac), (mab, b, c), (mab, c, mac)]
    for a in ATTRS:
        v = getattr(p, a)
        setattr(p, a, np.concatenate([v, np.array(extra[a]).reshape(-1, v.shape[1])]) if extra[a] else v)
    p.tri = np.concatenate([p.tri[~cross], np.array(new, np.int64)])
    return p


def _part_ok(r, name):
    if r['parts'] is not None and not any(name == q or name.startswith(q) for q in r['parts']):
        return False
    if r['not_parts'] is not None and any(name == q or name.startswith(q) for q in r['not_parts']):
        return False
    return True


def colour(p, rules):
    P = p.pos[p.tri]
    c = P.mean(1)
    c[:, 0] = np.abs(c[:, 0])
    fn = np.cross(P[:, 1] - P[:, 0], P[:, 2] - P[:, 0])
    fn /= np.maximum(np.linalg.norm(fn, axis=1, keepdims=True), 1e-12)
    fn[:, 0] = np.abs(fn[:, 0])
    out = np.zeros(len(c), np.int64)
    for r in rules:
        if not _part_ok(r, p.orig_part):
            continue
        m = np.ones(len(c), bool)
        for n, d in r['hs']:
            m &= c @ n <= d + 1e-5
        for key, k in (('nx', 0), ('ny', 1), ('nz', 2)):
            if r[key] is not None:
                m &= (fn[:, k] >= r[key][0]) & (fn[:, k] <= r[key][1])
        out[m] = r['colour']
    return out


def apply(prims, rules, unweld):
    planes = []
    for r in rules:
        for n, d in r['hs']:
            if abs(n[0]) > 1e-9:
                planes.append((n, d, 1))
                m = n.copy()
                m[0] = -m[0]
                planes.append((m, d, -1))
            else:
                planes.append((n, d, None))
    out = []
    for p in prims:
        if p.label != 'paint':
            out.append(p)
            continue
        if any(abs(n[0]) > 1e-9 for n, _, _ in planes):
            p = split(p, np.array([1.0, 0, 0]), 0.0, None)
        for n, d, side in planes:
            p = split(p, n, d, side)
        col = colour(p, rules)
        kk = np.repeat(col[:, None], 3, 1)
        f = np.clip(p.uv1src[p.tri] - np.floor(p.uv1src[p.tri].mean(1, keepdims=True)), 0, 1)
        cu = (kk % CELLS + 0.25 + 0.5 * f[..., 0]) / CELLS
        cv = (kk // CELLS + 0.25 + 0.5 * f[..., 1]) / CELLS
        q = unweld(p, [kk], np.stack([cu, cv], -1))
        q.zone = col
        out.append(q)
    return out


def texture(pal, path):
    tex = np.zeros((CELLS * CELL, CELLS * CELL, 3), np.uint8)
    tex[:] = pal[0]
    for k, c in enumerate(pal):
        r, q = divmod(k, CELLS)
        tex[r * CELL:(r + 1) * CELL, q * CELL:(q + 1) * CELL] = c
    Image.fromarray(tex).save(path)
