"""The 1990 Goodyear and Pirelli looks of the car pack's tyre set, and which car runs which.

tyre_art.py draws the texture arrays into erp/tyres/tyre_art.erpk. add_sets() puts them into the renamed common.erp
with, per supplier, a copy of every sidewall material (`materials/sidewall*_<supplier>.material`: the supplier's
logo array, the 1990 diffuse and the lettering in the compound's colour, LETTERING for the 1990 class's three
compounds, the stock colour for the rest). A car's Tyres override names the shared meshes and its own material
bindings, `eaid://f1_1990_vehicle_package/teams/<folder>/tyres/tyres.emb?context=<compound>` and
`.../tyres_wet.emb?context=wet`, the stock bindings with one supplier's sidewalls. They live in a small per-car
archive, `teams/<folder>/wep/<folder>_tyres.erp`, built once per supplier (erp/tyres/<supplier>/<folder>_tyres.erp);
option `tyre_supplier` picks the file (historical: the team's 1990 supplier) and the car's asset groups list it.
Every such archive also holds both suppliers' bindings under their own names (`tyres_goodyear.emb`,
`tyres_wet_goodyear.emb`, `tyres_pirelli.emb`, `tyres_wet_pirelli.emb`), for a car whose override is pointed at one.
"""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(HERE)
ADDON = os.path.dirname(PACK)
REPO = os.path.dirname(os.path.dirname(ADDON))
for _p in (os.path.join(REPO, 'tools'), os.path.join(ADDON, 'tools')):
    if _p not in sys.path:
        sys.path.insert(0, _p)

import erpw
import gfx_material as G
import cars as C

SUPPLIERS = ('goodyear', 'pirelli')
PIRELLI_TEAMS = ('tyrrell', 'minardi', 'brabham', 'dallara')
LOGO = 'sidewall_logo_surface.texarray'
DIFFUSE = 'sidewall_diffuse.texarray'
DIFFUSE_1990 = 'sidewall_diffuse_1990.texarray'
LETTERING = {'sidewall_super_soft.material': (0.95, 0.14, 0.17),
             'sidewall_soft.material': (0.995, 0.909, 0.123),
             'sidewall_ultra_soft.material': (0.74, 0.38, 0.86)}
ART = os.path.join(PACK, 'erp', 'tyres', 'tyre_art.erpk')
SET = f'eaid://{C.PACKAGE_FOLDER}/shared/tyres/'
MATBIND = 'GfxMatBindRes'
MATERIAL = 'GfxMaterialRes'


def supplier(c):
    return 'pirelli' if c.team in PIRELLI_TEAMS else 'goodyear'


def logo_array(s):
    return f'sidewall_logo_surface_{s}.texarray'


def stock_name(array):
    return f'eaid://{C.SOURCE_PACKAGE}/shared/tyres/texture_arrays/{array}'


def new_name(array):
    return f'{SET}texture_arrays/{array}'


def binding(c, wet=False, s=None):
    return (f'eaid://{C.PACKAGE_FOLDER}/teams/{c.folder}/tyres/' + ('tyres_wet' if wet else 'tyres')
            + (f'_{s}' if s else '') + '.emb')


def paths(c):
    return {'tyre model': SET + 'idf/tyres.idf?context=render',
            'tyre wet model': SET + 'idf/tyres_wet.idf?context=render',
            'material': binding(c),
            'wet material': binding(c, True) + '?context=wet'}


def archive(c):
    return f'2025_asset_groups/{C.PACKAGE_FOLDER}/teams/{c.folder}/wep/{c.folder}_tyres.erp'


def built(c, s):
    return os.path.join(PACK, 'erp', 'tyres', s, f'{c.folder}_tyres.erp')


def ops(src):
    out = []
    for c in C.CARS:
        for choice in ('historical',) + SUPPLIERS:
            s = supplier(c) if choice == 'historical' else choice
            out.append({'op': 'file.put', 'when': {'tyre_supplier': choice},
                        'source': f'{src}/erp/tyres/{s}/{c.folder}_tyres.erp', 'target': archive(c)})
    return out


def kind(r):
    return r.type.rstrip(b'\0').decode()


def clone(e, r, name):
    q = erpw.Res()
    q.name, q.type, q.unk, q.hash = name, r.type, r.unk, r.hash
    q.frags = []
    for f in r.frags:
        g = erpw.Frag()
        g.name, g.off, g.size, g.flags, g.comp, g.packed, g.raw = f.name, f.off, f.size, f.flags, f.comp, f.packed, f.raw
        q.frags.append(g)
    return q


def srv_retarget(data, image):
    end = data.index(b'\0', 24)
    if not data[24:end].endswith(b'.image') or end != len(data) - 1:
        raise ValueError(f'unexpected SRV {data!r}')
    return data[:24] + image.encode() + b'\0'


def parse_emb(data):
    count, count2, flags = struct.unpack_from('<3I', data, 0)
    if count != count2:
        raise ValueError(f'material binding with {count}/{count2} entries')
    strings = data[12 + 8 * count:].split(b'\0')[:-1]
    if len(strings) != 2 * count:
        raise ValueError(f'material binding with {len(strings)} strings for {count} entries')
    return flags, [s.decode() for s in strings[:count]], [s.decode() for s in strings[count:]]


def write_emb(flags, names, materials):
    out = bytearray(struct.pack('<3I', len(names), len(names), flags))
    n_off = m_off = 0
    for n, m in zip(names, materials):
        out += struct.pack('<II', n_off, m_off)
        n_off += len(n) + 1
        m_off += len(m) + 1
    for s in names + materials:
        out += s.encode() + b'\0'
    return bytes(out)


def insert(e, q):
    at = max(i for i, r in enumerate(e.res) if r.type == q.type and r.name.startswith(SET))
    e.res.insert(at + 1, q)


def sidewall_material(stock_data, s, colour):
    m = G.parse25(stock_data)
    swaps = {'me_diffuseMap': new_name(DIFFUSE_1990), 'me_logoSurfaceMap': new_name(logo_array(s))}
    for p in m.params:
        if p.kind == G.TEXTURE and p.name in swaps:
            p.value = swaps[p.name]
        elif p.kind == G.CONST and p.heap and p.name[:-len('HeapIndex')] in swaps:
            p.link = swaps[p.name[:-len('HeapIndex')]]
        elif p.name == 'me_logoColour' and colour:
            p.value = struct.pack('<3f', *colour)
    return G.write25(m)


def add_sets(e, art=ART):
    if not os.path.exists(art):
        raise FileNotFoundError(f'{art} is not built; run carpack/tools/tyre_art.py')
    a = erpw.Erp(art)
    want = {new_name(x) + ext for x in [logo_array(s) for s in SUPPLIERS] + [DIFFUSE_1990] for ext in ('', '.image')}
    have = {r.name for r in a.res}
    if have != want:
        raise ValueError(f'{art}: resources {sorted(have ^ want)} differ from the expected set')
    names = {r.name for r in e.res}
    for r in a.res:
        if r.name in names:
            raise ValueError(f'{r.name} is already in common.erp')
        insert(e, clone(a, r, r.name))
    walls = [r for r in e.res if kind(r) == MATERIAL and r.name.startswith(SET + 'materials/sidewall')]
    report = {'arrays': len(a.res), 'materials': 0}
    for s in SUPPLIERS:
        for r in walls:
            q = clone(e, r, supplier_material(r.name, s))
            e.set_frag(q, 0, sidewall_material(e.frag_data(r, 0), s, LETTERING.get(r.name.rsplit('/', 1)[1])))
            insert(e, q)
            report['materials'] += 1
    return report


def supplier_material(name, s):
    return name[:-len('.material')] + f'_{s}.material'


def stock_bindings(e):
    stock = (SET + 'idf/tyres.emb', SET + 'idf/tyres_wet.emb')
    out = [r for r in e.res if kind(r) == MATBIND and r.name.split('?')[0] in stock]
    if len(out) != 10:
        raise ValueError(f'{len(out)} stock tyre material bindings')
    return out


def write_archives(e):
    names = {r.name for r in e.res}
    keep, written = e.res, 0
    stock = stock_bindings(e)
    try:
        for c in C.CARS:
            for s in SUPPLIERS:
                res = []
                for name_as, look in ((None, s),) + tuple((x, x) for x in SUPPLIERS):
                    for r in stock:
                        base, _, query = r.name.partition('?')
                        flags, slots, materials = parse_emb(e.frag_data(r, 0))
                        if write_emb(flags, slots, materials) != e.frag_data(r, 0):
                            raise ValueError(f'{r.name}: material binding does not re-encode')
                        mapped = [supplier_material(m, look) if m.startswith(SET + 'materials/sidewall') else m
                                  for m in materials]
                        missing = [m for m in mapped if m not in names]
                        if missing:
                            raise ValueError(f'{c.team} {look}: {missing} not in common.erp')
                        q = clone(e, r, binding(c, base.endswith('_wet.emb'), name_as) + '?' + query)
                        e.set_frag(q, 0, write_emb(flags, slots, mapped))
                        res.append(q)
                e.res = res
                os.makedirs(os.path.dirname(built(c, s)), exist_ok=True)
                e.to_file(built(c, s))
                written += 1
    finally:
        e.res = keep
    return written


def check(e):
    names = {r.name for r in e.res}
    for s in SUPPLIERS:
        for r in e.res:
            if kind(r) == MATERIAL and r.name.endswith(f'_{s}.material'):
                m = G.parse25(e.frag_data(r, 0))
                for p in m.params:
                    if p.kind == G.TEXTURE and p.value not in names:
                        return f'{r.name}: {p.value}'
    for c in C.CARS:
        for s in SUPPLIERS:
            a = erpw.Erp(built(c, s))
            got = {r.name for r in a.res}
            for k, v in paths(c).items():
                if k.startswith('tyre') and not any(n.startswith(v.split('?')[0] + '?') for n in names):
                    return v
                if k == 'material' and not any(n.startswith(v + '?') for n in got):
                    return f'{built(c, s)}: {v}'
                if k == 'wet material' and v not in got:
                    return f'{built(c, s)}: {v}'
            if len(a.res) != 30:
                return f'{built(c, s)}: {len(a.res)} bindings'
            for r in a.res:
                look = next((x for x in SUPPLIERS if f'_{x}.emb?' in r.name), s)
                for m in parse_emb(a.frag_data(r, 0))[2]:
                    if m not in names or ('sidewall' in m and not m.endswith(f'_{look}.material')):
                        return f'{built(c, s)}: {r.name} names {m}'
    return None
