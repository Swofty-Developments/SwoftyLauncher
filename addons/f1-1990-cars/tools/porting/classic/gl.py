import json, base64, struct, numpy as np
CT = {5126: np.float32, 5123: np.uint16, 5125: np.uint32, 5121: np.uint8, 5122: np.int16, 5120: np.int8}
NC = {'SCALAR': 1, 'VEC2': 2, 'VEC3': 3, 'VEC4': 4, 'MAT4': 16}
class G:
    def __init__(self, path):
        self.j = json.load(open(path))
        self.bufs = []
        for b in self.j['buffers']:
            u = b['uri']
            self.bufs.append(base64.b64decode(u.split(',', 1)[1]) if u.startswith('data:') else open(path.rsplit('/', 1)[0] + '/' + u, 'rb').read())
    def acc(self, i):
        a = self.j['accessors'][i]; bv = self.j['bufferViews'][a['bufferView']]
        dt = CT[a['componentType']]; n = NC[a['type']]
        off = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
        stride = bv.get('byteStride', 0)
        isz = np.dtype(dt).itemsize * n
        buf = self.bufs[bv['buffer']]
        if stride and stride != isz:
            raw = np.frombuffer(buf, np.uint8, count=stride * (a['count'] - 1) + isz, offset=off)
            idx = np.arange(a['count'])[:, None] * stride + np.arange(isz)[None, :]
            out = raw[idx].copy().view(dt).reshape(a['count'], n)
        else:
            out = np.frombuffer(buf, dt, count=a['count'] * n, offset=off).reshape(a['count'], n).copy()
        if a.get('normalized'):
            out = out.astype(np.float32) / np.iinfo(dt).max
        return out
    def image(self, name):
        for im in self.j.get('images', []):
            if im.get('name') == name:
                return base64.b64decode(im['uri'].split(',', 1)[1])
    def world(self):
        mats = {}
        def rec(i, M):
            n = self.j['nodes'][i]
            L = np.eye(4)
            if 'matrix' in n: L = np.array(n['matrix']).reshape(4, 4).T
            else:
                T = np.eye(4); T[:3, 3] = n.get('translation', [0, 0, 0])
                x, y, z, w = n.get('rotation', [0, 0, 0, 1])
                R = np.eye(4); R[:3, :3] = [[1-2*(y*y+z*z), 2*(x*y-z*w), 2*(x*z+y*w)], [2*(x*y+z*w), 1-2*(x*x+z*z), 2*(y*z-x*w)], [2*(x*z-y*w), 2*(y*z+x*w), 1-2*(x*x+y*y)]]
                S = np.diag(list(n.get('scale', [1, 1, 1])) + [1])
                L = T @ R @ S
            W = M @ L
            mats[i] = W
            for c in n.get('children', []): rec(c, W)
        for r in self.j['scenes'][self.j.get('scene', 0)]['nodes']: rec(r, np.eye(4))
        return mats
    def prims(self):
        W = self.world()
        for i, n in enumerate(self.j['nodes']):
            if 'mesh' not in n: continue
            for p in self.j['meshes'][n['mesh']]['primitives']:
                at = {k: self.acc(v) for k, v in p['attributes'].items() if not k.startswith('_')}
                idx = self.acc(p['indices']).reshape(-1) if 'indices' in p else np.arange(len(at['POSITION']))
                mat = self.j['materials'][p['material']]['name'] if 'material' in p else None
                yield i, n.get('name'), mat, at, idx.reshape(-1, 3), W[i]
