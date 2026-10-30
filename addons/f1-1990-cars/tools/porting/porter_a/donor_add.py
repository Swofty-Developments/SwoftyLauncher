import json, sys
import numpy as np
sys.path.insert(0, '/mnt/work/.mp45b/classic/tools')
from gl import G

PRODUCED = ['camera_wing', 'cockpit', 'diffuser', 'diffuser_bm', 'engine', 'frontwing_fl_01', 'frontwing_fr_01', 'nosecone',
            'rearwing_top', 'safetycell', 'topcanopy_bm', 'windscreen', 'wing_mirror_l', 'wing_mirror_r']
MOUNTS = ['x0_suspension2_fixed_bl', 'x0_suspension2_fixed_br', 'x0_suspension_fixed_fl', 'x0_suspension_fixed_fl_01',
          'x0_suspension_fixed_fr', 'x0_suspension_fixed_fr_01']


def components(tri):
    parent = {}

    def find(a):
        while parent.setdefault(a, a) != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a
    for t in tri:
        for a, b in ((t[0], t[1]), (t[1], t[2])):
            ra, rb = find(int(a)), find(int(b))
            if ra != rb:
                parent[ra] = rb
    groups = {}
    for i, t in enumerate(tri):
        groups.setdefault(find(int(t[0])), []).append(i)
    return list(groups.values())


def main(team):
    g = G(f'/mnt/work/.mp45b/porter_a/stock26_export/{team}/{team}_LOD0.gltf')
    W = g.world()
    names = {i: n.get('name') for i, n in enumerate(g.j['nodes'])}
    steer = next(W[i][:3, 3] for i, n in names.items() if n == 'steer_master' and i in W)
    parts = sorted({n.get('name') for n in g.j['nodes'] if 'mesh' in n})
    mounts = sorted({n for n in names.values() if n in MOUNTS})
    tex = sorted({im['name'] for im in g.j.get('images', []) if im.get('name', '').startswith(team + '_')})
    plates = []
    for i, name, mat, at, tri, Wm in g.prims():
        if not mat or 'number' not in mat.lower() or 'TEXCOORD_1' not in at:
            continue
        P = (Wm[:3, :3] @ at['POSITION'].T).T + Wm[:3, 3]
        N = (Wm[:3, :3] @ at['NORMAL'].T).T
        for comp in components(tri):
            t = tri[comp]
            v = np.unique(t)
            e1 = P[t[:, 1]] - P[t[:, 0]]
            e2 = P[t[:, 2]] - P[t[:, 0]]
            area = float(np.linalg.norm(np.cross(e1, e2), axis=1).sum() / 2)
            n = N[v].mean(0)
            n = n / max(np.linalg.norm(n), 1e-9)
            uv = at['TEXCOORD_1'][v]
            plates.append({'node': name, 'n': int(len(t)), 'lo': [round(float(x), 3) for x in P[v].min(0)],
                           'hi': [round(float(x), 3) for x in P[v].max(0)], 'nrm': [round(float(x), 2) for x in n],
                           'uv1': [round(float(x), 3) for x in (*uv.min(0), *uv.max(0))], 'area': area})
    donor = {'steer': [round(float(x), 4) for x in steer], 'missing': [p for p in PRODUCED if p not in parts],
             'parts': parts, 'mounts': mounts, 'tex': tex,
             'render': f'eaid://f1_2026_vehicle_package/teams/{team}/idf/{team}.idf?render'}
    return donor, plates


if __name__ == '__main__':
    dj = json.load(open('/mnt/work/.mp45b/porter_a/donors26.json'))
    pj = json.load(open('/mnt/work/.mp45b/porter_a/stock_number_plates.json'))
    for team in sys.argv[1:]:
        d, p = main(team)
        dj[team] = d
        pj[team] = p
        print(team, d['steer'], d['missing'], d['mounts'], len(d['tex']), len(p), [x['uv1'] for x in p if x['lo'][2] > 1.2][:3])
    json.dump(dj, open('/mnt/work/.mp45b/porter_a/donors26.json', 'w'), indent=1)
    json.dump(pj, open('/mnt/work/.mp45b/porter_a/stock_number_plates.json', 'w'), indent=1)
