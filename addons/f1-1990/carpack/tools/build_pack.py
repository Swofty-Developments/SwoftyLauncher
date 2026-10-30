"""Build the private 1990 car pack: the 13 car archives and the package's common.erp.

Each source archive is a porter output patched onto a stock 2026 team archive, so its resources are named
`eaid://f1_2026_vehicle_package/teams/<donor>/...`. The build renames them byte for byte to
`eaid://f1_1990_vehicle_package/teams/<stem>/...` (same length, so no offset in any payload moves) and leaves the
bare `.mipmaps` sidecar paths on the stock 2026 files. It drops the car's own handling file, which the f1-1990 addon
ships per option in `<folder>_handling.erp`, and gives every carmodel layer's AudioF1Car `switchName` the car's
1990 engine audio label (addons/f1-1990/tools/audio.py: the donor's label in other letter case, the same Wwise
switch state) so it plays the 1990 sound rows of the f1-1990 addon. A car
whose folder differs from its stem also gets its World, event graph and layers under the folder name.

The package's common.erp renames its 13-inch tyre set with everything else, to
`eaid://f1_1990_vehicle_package/shared/tyres/`. The stock set of the same 2026 names stays loaded beside it (the
frontend's f1_2026 fom_car group and every stock 2026 car list the stock common.erp), and two sets under one name pair
one's meshes with the other's buffers, so the tyres render hollow. The car templates (F1CarModern2026,
F1ShowRoomCarModern2026 in `eaid://wote/types`) name the stock set in their Tyres component, so every instance of each
carmodel and carshowroommodel layer, A and B-spec, gets a Tyres override naming the renamed set (TYRE_PATHS).

Outputs (relative to the car pack folder `carpack/`, not committed): erp/teams/<folder>/wep/<folder>.erp, erp/teams/common.erp,
assets/cars.json (the file.put and group.register ops), assets/ai.json (the AI archive ops of tools/ai/build_ai.py) and
db/test.json (the test_grand_prix option's seat swaps); the last three are committed. The tyre compound archive the
group.register op names comes from addons/f1-1990/tools/tyres.py (tyres/<supplier>/tyrecompounds.erp). A full build fails at the end when an AI archive that assets/ai.json names is not in erp/ai.
"""
import argparse
import json
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(HERE)
ADDON = os.path.dirname(PACK)
REPO = os.path.dirname(os.path.dirname(ADDON))
SRC = os.path.relpath(PACK, ADDON).replace(os.sep, '/')
sys.path.insert(0, os.path.join(REPO, 'tools'))
sys.path.insert(0, os.path.join(ADDON, 'tools'))

import erpw
import vtf
import audio
import cars as C

sys.path.insert(0, os.path.join(HERE, 'ai'))
import build_ai
from pkg import Obj, Pkg, Val, to_py

TRIES = '/mnt/work/.mp45b/porter_a/tries'
TEAM_ERP = 'F1 25/2025_asset_groups/f1_2026_vehicle_package/teams/{d}/wep/{d}.erp'
SOURCES = {
    'mclaren': 'gridc13_mclaren', 'ferrari': 'gridc13_ferrari', 'williams': 'gridc13_williams',
    'benetton': 'gridc13_lotus', 'tyrrell': 'gridc13_mercedes', 'lotus': 'gridc13_redbull',
    'march': 'gridc13_march', 'larrousse': 'gridc13_larrousse', 'ligier_prost': 'gridc13_ligier_prost',
    'minardi': 'gridc13_minardi', 'brabham': 'gridc13_brabham', 'dallara': 'gridc13_dallara',
    'arrows': 'gridc13_arrows',
}
SOURCES_B = {
    'mclaren': 'gridc13b_mclaren', 'ferrari': 'gridc13b_ferrari', 'williams': 'gridc13b_williams',
    'benetton': 'gridc13b_lotus', 'tyrrell': 'gridc13b_mercedes', 'lotus': 'gridc13b_redbull',
    'march': 'gridc13b_march', 'larrousse': 'gridc13b_larrousse', 'ligier_prost': 'gridc13b_ligier_prost',
    'minardi': 'gridc13b_minardi', 'brabham': 'gridc13b_brabham', 'dallara': 'gridc13b_dallara',
    'arrows': 'gridc13b_arrows',
}
COMMON = '/mnt/work/.mp45b/porter_a/tries/tyres13cm3/common.erp'
POD_CAMERA = {
    'mclaren': (0.658, 0.119), 'ferrari': (0.652, 0.123), 'williams': (0.626, -0.109), 'benetton': (0.630, 0.166),
    'tyrrell': (0.634, 0.256), 'lotus': (0.602, -0.095), 'march': (0.634, 0.256), 'larrousse': (0.652, 0.123),
    'ligier_prost': (0.658, 0.119), 'minardi': (0.634, 0.256), 'brabham': (0.602, -0.095), 'dallara': (0.626, -0.109),
    'arrows': (0.652, 0.123),
}
POD_ATTACHMENTS = ('pod', 'podoffset', 'pod_left', 'pod_right')
POD_FOV = 100.0
MESH_HEADER, MESH_RECORD = 76, 88
SHADOW_MAX_TRIANGLES = 40000
TYRE_SET = f'eaid://{C.PACKAGE_FOLDER}/shared/tyres/'
TYRE_PATHS = {'tyre model': TYRE_SET + 'idf/tyres.idf?context=render',
              'tyre wet model': TYRE_SET + 'idf/tyres_wet.idf?context=render',
              'material': TYRE_SET + 'idf/tyres.emb',
              'wet material': TYRE_SET + 'idf/tyres_wet.emb?context=wet'}
TYRE_LAYERS = {'-carmodel.layer': 'F1CarModern2026', '-carshowroommodel.layer': 'F1ShowRoomCarModern2026'}
STOCK_TYRES = f'{C.SOURCE_PACKAGE}/shared/tyres/'.encode()
TYRE_GROUP = (b'F1_2026', b'F1_1990')

OLD = f'eaid://{C.SOURCE_PACKAGE}/'.encode()
NEW = f'eaid://{C.PACKAGE_FOLDER}/'.encode()
NEW_B = f'eaid://{C.PACKAGE_B_FOLDER}/'.encode()
EAID = re.compile(re.escape(OLD) + rb'[^\x00]*')
FOLDER_COPIES = ('World', 'EventGraph', 'WOInstances')


def kind(r):
    return r.type.rstrip(b'\0').decode()


def local_shared(e):
    keys = set()
    for r in e.res:
        n = r.name.encode()
        if not n.startswith(OLD + b'shared/'):
            continue
        n = n[len(OLD):].split(b'?')[0]
        keys.add(n)
        for ext in (b'.image', b'.sampler'):
            if n.endswith(ext):
                keys.add(n[:-len(ext)])
    return keys


def rename_bytes(data, donor=None, stem=None, bspec=False, shared=frozenset()):
    vtf = f'teams/{donor}/handling/{donor}.vtf'.encode() if donor else None

    def sub(m):
        rest = m.group(0)[len(OLD):]
        if rest.startswith(b'teams/'):
            to_b = bspec and not (vtf and rest.startswith(vtf))
        else:
            to_b = bspec and rest.split(b'?')[0] in shared
        s = (NEW_B if to_b else NEW) + rest
        if donor and donor != stem:
            s = s.replace(donor.encode(), stem.encode())
        assert len(s) == len(m.group(0))
        return s
    return EAID.sub(sub, data)


def rename_archive(e, donor=None, stem=None, bspec=False):
    shared = local_shared(e) if bspec else frozenset()
    renamed = rewritten = 0
    for r in e.res:
        n = rename_bytes(r.name.encode(), donor, stem, bspec, shared).decode()
        if n != r.name:
            r.name = n
            renamed += 1
        for i in range(len(r.frags)):
            d = e.frag_data(r, i)
            nd = rename_bytes(d, donor, stem, bspec, shared)
            if nd != d:
                e.set_frag(r, i, nd)
                rewritten += 1
    return renamed, rewritten


def copy_res(e, r, name):
    q = erpw.Res()
    q.name, q.type, q.unk, q.hash = name, r.type, r.unk, r.hash
    q.frags = []
    for f in r.frags:
        g = erpw.Frag()
        g.name, g.off, g.size, g.flags, g.comp, g.packed, g.raw = f.name, f.off, f.size, f.flags, f.comp, f.packed, f.raw
        q.frags.append(g)
    return q


def leftovers(e, needle):
    n = 0
    for r in e.res:
        for i in range(len(r.frags)):
            for m in re.finditer(rb'eaid://[\x21-\x7e]*', e.frag_data(r, i)):
                n += needle in m.group(0)
        n += needle.decode() in r.name
    return n


def move_pod_cameras(e, c):
    y, z = POD_CAMERA[c.team]
    moved = []
    for r in e.res:
        if kind(r) != 'WOInstances' or not re.search(r'-camscargame(_ss)?\.layer$', r.name):
            continue
        suffix = '_ss' if r.name.endswith('_ss.layer') else ''
        data = bytearray(e.frag_data(r, 0))
        offsets = {}
        for inst in to_py(Pkg(bytes(data)).root)['instances']:
            ap = inst.get('overrides', {}).get('AttachmentPoint') or {}
            if ap.get('Attachment name') in {a + suffix for a in POD_ATTACHMENTS} and 'Offset' in ap:
                offsets[ap['Attachment name']] = ap['Offset']
        pod = offsets.get('pod' + suffix)
        if pod is None:
            raise ValueError(f'{c.team}: {r.name} has no pod attachment')
        dy, dz = y - pod[1], z - pod[2]
        for name, off in offsets.items():
            hits = [i for i in range(len(data) - 15)
                    if all(abs(a - b) < 2e-6 for a, b in zip(struct.unpack_from('<4f', data, i), off))]
            if len(hits) != 1:
                raise ValueError(f'{c.team}: {r.name} {name} offset {off} found {len(hits)} times')
            struct.pack_into('<2f', data, hits[0] + 4, off[1] + dy, off[2] + dz)
            moved.append(name)
        if not suffix:
            pkg = Pkg(bytes(data))
            pods = [i for i in pkg.root['instances'].items if i['name'].value.endswith('$CamPod')]
            if len(pods) != 1:
                raise ValueError(f'{c.team}: {r.name} has {len(pods)} CamPod instances')
            pods[0]['overrides']['Camera'].set('FOV', Val('fp32', POD_FOV))
            data = bytearray(pkg.to_bytes())
            moved.append('pod_fov')
        e.set_frag(r, 0, bytes(data))
    return moved


def set_audio(e, c):
    layers = [r for r in e.res if kind(r) == 'WOInstances' and r.name.endswith('-carmodel.layer')]
    if not layers:
        raise ValueError(f'{c.team}: no carmodel layer')
    labels = set()
    for r in layers:
        d = e.frag_data(r, 0)
        old = audio.switch_name(d)
        new = audio.label_1990(c, old)
        e.set_frag(r, 0, audio.rewrite_switch(d, old, new))
        labels.add(new)
    if len(labels) != 1:
        raise ValueError(f'{c.team}: carmodel layers disagree on the audio label: {labels}')
    return labels.pop()


def set_tyres(e, c):
    done = 0
    for r in e.res:
        suffix = next((k for k in TYRE_LAYERS if r.name.endswith(k)), None)
        if kind(r) != 'WOInstances' or not suffix:
            continue
        pkg = Pkg(e.frag_data(r, 0))
        for inst in pkg.root['instances'].items:
            if inst['template'].value != TYRE_LAYERS[suffix]:
                raise ValueError(f'{c.team}: {r.name} instance of {inst["template"].value}')
            over = inst['overrides']
            tyres = over.get('Tyres')
            if not isinstance(tyres, Obj):
                tyres = Obj()
                over.set('Tyres', tyres)
            for k, v in TYRE_PATHS.items():
                tyres.set(k, Val('stri', v))
            done += 1
        e.set_frag(r, 0, pkg.to_bytes())
    layers = sum(kind(r) == 'WOInstances' and r.name.endswith(tuple(TYRE_LAYERS)) for r in e.res)
    if layers < 2 or done < layers:
        raise ValueError(f'{c.team}: Tyres override on {done} instance(s) of {layers} model layer(s)')
    return done


def tyre_set_resolves(e):
    names = {r.name for r in e.res}
    for v in TYRE_PATHS.values():
        base, _, query = v.partition('?')
        if query and query != 'context=render':
            if v not in names:
                return v
        elif not any(n.startswith(base + '?') for n in names):
            return v
    return None


def shadow_triangles(e):
    worst = 0
    for r in e.res:
        if kind(r) != 'GfxMeshRes' or not r.name.endswith('&mesh'):
            continue
        d = e.frag_data(r, 0)
        names = MESH_HEADER + struct.unpack_from('<i', d, 28)[0]
        total = 0
        for i in range(struct.unpack_from('<i', d, 0)[0]):
            rec = MESH_HEADER + i * MESH_RECORD
            at = names + struct.unpack_from('<I', d, rec + 48)[0]
            if d[at:d.find(b'\0', at)] == b'shadow':
                total += struct.unpack_from('<I', d, rec + 16)[0] // 3
        worst = max(worst, total)
    return worst


def build_car(c, src, out, bspec=False):
    e = erpw.Erp(src)
    renamed, rewritten = rename_archive(e, c.donor, c.stem, bspec)
    folder_pkg = C.PACKAGE_B_FOLDER if bspec else C.PACKAGE_FOLDER
    vtf = [f'eaid://{C.PACKAGE_FOLDER}/teams/{c.stem}/handling/{c.stem}.vtf']
    before = len(e.res)
    e.res = [r for r in e.res if r.name not in vtf]
    dropped = before - len(e.res)
    extra = 0
    if c.stem != c.folder:
        old = f'/teams/{c.stem}/wep/{c.stem}'
        new = f'/teams/{c.folder}/wep/{c.folder}'
        for r in list(e.res):
            if kind(r) in FOLDER_COPIES and old in r.name:
                e.res.append(copy_res(e, r, r.name.replace(old, new)))
                extra += 1
    world = [r.name for r in e.res if kind(r) == 'World']
    want = f'eaid://{folder_pkg}/teams/{c.folder}/wep/{c.folder}.wep'
    if want not in world:
        raise ValueError(f'{c.team}: no World {want} (have {world})')
    left = leftovers(e, OLD)
    if left:
        raise ValueError(f'{c.team}: {left} eaid reference(s) still name {OLD.decode()}')
    other = [r.name for r in e.res if bspec and r.name.startswith(NEW.decode())]
    if other:
        raise ValueError(f'{c.team}: the B-spec archive carries A-package resources {other[:3]}')
    shadow = shadow_triangles(e)
    if shadow > SHADOW_MAX_TRIANGLES:
        raise ValueError(f'{c.team}: shadow caster of {shadow} triangles; give the layout its own shadow LOD')
    sound = set_audio(e, c)
    cams = move_pod_cameras(e, c)
    tyres = set_tyres(e, c)
    groups = set_tyre_group(e)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    e.to_file(out)
    return {'team': c.team, 'folder': c.folder, 'stem': c.stem, 'bspec': bspec, 'source': os.path.relpath(src, TRIES),
            'audio': sound, 'shadow_triangles': shadow, 'pod_cameras': len(cams), 'tyre_overrides': tyres,
            'tyre_groups': groups,
            'resources': len(e.res), 'renamed': renamed, 'rewritten': rewritten, 'handling_dropped': dropped,
            'folder_copies': extra, 'bytes': os.path.getsize(out)}


def set_tyre_group(e):
    done = 0
    for r in e.res:
        if kind(r) != 'VTF':
            continue
        f1 = e.frag_data(r, 1)
        root = vtf.parse(e.frag_data(r, 0))
        nodes = [n for n, _ in vtf.walk(root) if n.name == b'TyrePhysicsGroup']
        if len(nodes) != 1 or nodes[0].attr(b'value') != TYRE_GROUP[0]:
            raise ValueError(f'{r.name}: TyrePhysicsGroup {[n.attr(b"value") for n in nodes]}')
        nodes[0].set_attr(b'value', TYRE_GROUP[1])
        e.set_frag(r, 0, vtf.write(root))
        e.set_frag(r, 1, vtf.tsmd(root, int.from_bytes(f1[4:8], 'little')))
        done += 1
    return done


def build_plain(src, out):
    e = erpw.Erp(src)
    renamed, rewritten = rename_archive(e)
    left = leftovers(e, OLD)
    if left:
        raise ValueError(f'{src}: {left} eaid reference(s) still name {OLD.decode()}')
    tyres = [r for r in e.res if r.name.startswith(TYRE_SET)]
    missing = tyre_set_resolves(e) if tyres else None
    if missing:
        raise ValueError(f'{src}: the renamed tyre set has no {missing}')
    if any(STOCK_TYRES in r.name.encode() for r in e.res):
        raise ValueError(f'{src}: a resource keeps a {STOCK_TYRES.decode()} name')
    groups = set_tyre_group(e)
    os.makedirs(os.path.dirname(out), exist_ok=True)
    e.to_file(out)
    return {'source': src, 'resources': len(e.res), 'renamed': renamed, 'rewritten': rewritten,
            'tyre_set': len(tyres), 'tyre_groups': groups, 'bytes': os.path.getsize(out)}


def ops():
    out = [{'op': 'file.put', 'source': f'{SRC}/erp/teams/common.erp', 'target': C.COMMON_ARCHIVE},
           {'op': 'group.register', 'group': C.TYRE_GROUP, 'erp': C.TYRE_ARCHIVE,
            'template': f'asset_groups/{C.SOURCE_PACKAGE}/tyrecompounds/tyrecompounds.assetgroup'}]
    for c in C.CARS:
        out.append({'op': 'file.put', 'source': f'{SRC}/erp/teams/{c.folder}/wep/{c.folder}.erp', 'target': C.archive(c)})
    for c in C.CARS:
        out.append({'op': 'file.put', 'source': f'{SRC}/erp/teams_b/{c.folder}/wep/{c.folder}.erp', 'target': C.archive_b(c)})
    for c in C.CARS:
        out.append({'op': 'group.register', 'group': C.group(c),
                    'erps': [C.COMMON_ARCHIVE, C.archive(c), C.handling_archive(c)],
                    'template': f'asset_groups/{C.SOURCE_PACKAGE}/teams/{c.folder}/wep/{c.folder}.assetgroup'})
    for c in C.CARS:
        out.append({'op': 'group.register', 'group': C.group_b(c),
                    'erps': [C.COMMON_ARCHIVE, C.archive_b(c), C.handling_archive(c)],
                    'template': f'asset_groups/{C.SOURCE_PACKAGE}/teams/{c.folder}/wep/{c.folder}.assetgroup'})
    return {'schema_version': 1, 'ops': out}


GP_SEATS = {'mclaren_driver': 'mclaren', 'mercedes_driver': 'mercedes', 'redbull_driver': 'redbull',
            'ferrari_driver': 'ferrari', 'williams_driver': 'williams', 'vcarb_driver': 'toro_rosso',
            'astonmartin_driver': 'force_india', 'haas_driver': 'haas', 'kick_driver': 'sauber',
            'alpine_driver': 'lotus', 'gm_cadillac': 'gm_cadillac'}
MYTEAM_SEATS = {'williams': 'myteam_ferrari', 'haas': 'myteam_honda'}


def test_ops():
    by_folder = {c.folder: c for c in C.CARS}
    out = []
    for prefix, folder in GP_SEATS.items():
        for n in ('01', '02'):
            sel = f'entrants.entrant_lists.entrant_list[_metadata_=f1_2026].entrant[_metadata_={prefix}_{n}]'
            if folder == 'ferrari':
                out.append({'op': 'set', 'select': sel, 'field': 'vehicle', 'value': by_folder[folder].vehicle,
                            'when': {'test_grand_prix': 'ferrari'}})
            out.append({'op': 'set', 'select': sel, 'field': 'vehicle', 'value': by_folder[folder].vehicle,
                        'when': {'test_grand_prix': 'all'}})
            out.append({'op': 'set', 'select': sel, 'field': 'vehicle', 'value': by_folder[folder].vehicle_b,
                        'when': {'test_grand_prix': 'bspec'}})
            if folder in MYTEAM_SEATS:
                out.append({'op': 'set', 'select': sel, 'field': 'vehicle',
                            'value': by_folder[MYTEAM_SEATS[folder]].vehicle, 'when': {'test_grand_prix': 'myteam'}})
    return {'schema_version': 1, 'ops': out}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    ap.add_argument('--tries', default=TRIES, help='folder with the porter tries named in SOURCES')
    ap.add_argument('--common', default=COMMON, help='the 13-inch common.erp (f1_2026 names)')
    ap.add_argument('--only', help='comma-separated teams to build')
    ap.add_argument('--ops-only', action='store_true', help='write assets/cars.json and db/test.json and stop')
    ap.add_argument('--bspec-only', action='store_true', help='build only the B-spec archives')
    ap.add_argument('--common-only', action='store_true', help='build only common.erp')
    args = ap.parse_args()
    with open(os.path.join(PACK, 'assets', 'cars.json'), 'w') as f:
        f.write(json.dumps(ops(), indent=1) + '\n')
    with open(os.path.join(PACK, 'db', 'test.json'), 'w') as f:
        f.write(json.dumps(test_ops(), indent=1) + '\n')
    build_ai.write_json(os.path.join(PACK, 'assets', 'ai.json'), build_ai.ops(build_ai.calendar_tracks(), build_ai.FOLDERS))
    if args.ops_only:
        return 0
    report = {'cars': []}
    only = set(args.only.split(',')) if args.only else None
    for c in C.CARS:
        if (only and c.team not in only) or args.common_only:
            continue
        src = os.path.join(args.tries, SOURCES[c.team], TEAM_ERP.format(d=c.donor))
        rs = []
        if not args.bspec_only:
            rs.append(build_car(c, src, os.path.join(PACK, 'erp', 'teams', c.folder, 'wep', f'{c.folder}.erp')))
        rs.append(build_car(c, os.path.join(args.tries, SOURCES_B[c.team], TEAM_ERP.format(d=c.donor)),
                            os.path.join(PACK, 'erp', 'teams_b', c.folder, 'wep', f'{c.folder}.erp'), bspec=True))
        report['cars'] += rs
        for x in rs:
            print(f'{c.team:13} {"B" if x["bspec"] else "A"} {c.folder:15} stem {c.stem:12} {x["resources"]:5} res, '
                  f'{x["renamed"]} renamed, {x["rewritten"]} fragments, handling -{x["handling_dropped"]}, '
                  f'+{x["folder_copies"]} folder copies, audio {x["audio"]}, {x["bytes"] / 1e6:.1f} MB')
    if not only and not args.bspec_only:
        report['common'] = build_plain(args.common, os.path.join(PACK, 'erp', 'teams', 'common.erp'))
        print('common', report['common'])
    if args.common_only:
        return 0
    with open(os.path.join(PACK, 'erp', 'build_report_b.json' if args.bspec_only else 'build_report.json'), 'w') as f:
        json.dump(report, f, indent=1)
    missing = build_ai.missing_archives()
    if missing:
        print(f'error: assets/ai.json names {len(missing)} AI archives that are not built, such as {missing[0]}; '
              f'run tools/ai/build_ai.py', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
