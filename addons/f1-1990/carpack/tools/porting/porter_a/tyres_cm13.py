import json
import os
import struct
import sys

import numpy as np

STOCK_BEAD = 0.238
STOCK_SHOULDER = 0.309
NEW_BEAD = 0.175
AXLE = {'f': {'outer': 0.353, 'new_outer': 0.323, 'half_width': (0.166, 0.152)},
        'b': {'outer': 0.354, 'new_outer': 0.330, 'half_width': (0.2075, 0.225)}}


def radial(r, ax):
    a = AXLE[ax]
    shift = a['new_outer'] - a['outer']
    shoulder = STOCK_SHOULDER + shift
    k = (shoulder - NEW_BEAD) / (STOCK_SHOULDER - STOCK_BEAD)
    f = np.where(r >= STOCK_SHOULDER, r + shift,
                 np.where(r >= STOCK_BEAD, NEW_BEAD + (r - STOCK_BEAD) * k, r * NEW_BEAD / STOCK_BEAD))
    d = np.where(r >= STOCK_SHOULDER, 1.0, np.where(r >= STOCK_BEAD, k, NEW_BEAD / STOCK_BEAD))
    return f, d


def view(j, binbuf, acc):
    a = j['accessors'][acc]
    bv = j['bufferViews'][a['bufferView']]
    n = {'VEC2': 2, 'VEC3': 3, 'VEC4': 4}[a['type']]
    assert a['componentType'] == 5126 and 'byteStride' not in bv
    off = bv.get('byteOffset', 0) + a.get('byteOffset', 0)
    return np.frombuffer(binbuf, np.float32, a['count'] * n, off).reshape(a['count'], n).astype(np.float64), off


def put(binbuf, off, arr):
    data = arr.astype(np.float32).tobytes()
    binbuf[off:off + len(data)] = data


def reshape(src_gltf, out_glb):
    j = json.load(open(src_gltf))
    binbuf = bytearray(open(os.path.join(os.path.dirname(src_gltf), j['buffers'][0]['uri']), 'rb').read())
    report = {}
    for node in j['nodes']:
        name = node.get('name', '')
        if 'mesh' not in node or '_tyre_' not in name:
            continue
        ax = name.split('_tyre_')[1][0]
        sx = AXLE[ax]['half_width'][1] / AXLE[ax]['half_width'][0]
        for prim in j['meshes'][node['mesh']]['primitives']:
            at = prim['attributes']
            p, po = view(j, binbuf, at['POSITION'])
            rho = np.hypot(p[:, 1], p[:, 2])
            safe = np.maximum(rho, 1e-9)
            er = np.c_[np.zeros(len(p)), p[:, 1] / safe, p[:, 2] / safe]
            et = np.c_[np.zeros(len(p)), -p[:, 2] / safe, p[:, 1] / safe]
            f, d = radial(rho, ax)
            s = np.where(rho > 1e-9, f / safe, NEW_BEAD / STOCK_BEAD)
            q = np.c_[p[:, 0] * sx, p[:, 1] * s, p[:, 2] * s]
            put(binbuf, po, q)
            acc = j['accessors'][at['POSITION']]
            acc['min'] = q.min(0).tolist()
            acc['max'] = q.max(0).tolist()

            def frame(v):
                return v[:, 0], (v * er).sum(1), (v * et).sum(1)
            if 'NORMAL' in at:
                n, no = view(j, binbuf, at['NORMAL'])
                nx, nr, nt = frame(n)
                m = np.c_[nx / sx, np.zeros(len(n)), np.zeros(len(n))] + er * (nr / d)[:, None] + et * (nt / s)[:, None]
                m /= np.maximum(np.linalg.norm(m, axis=1, keepdims=True), 1e-12)
                put(binbuf, no, m)
            if 'TANGENT' in at:
                t, to = view(j, binbuf, at['TANGENT'])
                tx, tr, tt = frame(t[:, :3])
                m = np.c_[tx * sx, np.zeros(len(t)), np.zeros(len(t))] + er * (tr * d)[:, None] + et * (tt * s)[:, None]
                m /= np.maximum(np.linalg.norm(m, axis=1, keepdims=True), 1e-12)
                put(binbuf, to, np.c_[m, t[:, 3]])
            key = f"{name}|{prim.get('material')}"
            report[key] = {'r': [round(float(rho.min()), 4), round(float(rho.max()), 4)],
                           'r_new': [round(float(np.hypot(q[:, 1], q[:, 2]).min()), 4), round(float(np.hypot(q[:, 1], q[:, 2]).max()), 4)],
                           'x_new': [round(float(q[:, 0].min()), 4), round(float(q[:, 0].max()), 4)]}
    j['buffers'] = [{'byteLength': len(binbuf)}]
    j['asset']['generator'] = j['asset'].get('generator', '') + ' + tyres_cm13'
    js = json.dumps(j, separators=(',', ':')).encode()
    js += b' ' * ((4 - len(js) % 4) % 4)
    binbuf += b'\0' * ((4 - len(binbuf) % 4) % 4)
    with open(out_glb, 'wb') as fh:
        fh.write(struct.pack('<III', 0x46546C67, 2, 12 + 8 + len(js) + 8 + len(binbuf)))
        fh.write(struct.pack('<II', len(js), 0x4E4F534A))
        fh.write(js)
        fh.write(struct.pack('<II', len(binbuf), 0x004E4942))
        fh.write(binbuf)
    return report


if __name__ == '__main__':
    if len(sys.argv) > 4:
        AXLE['f']['new_outer'], AXLE['b']['new_outer'] = float(sys.argv[3]), float(sys.argv[4])
    rep = reshape(sys.argv[1], sys.argv[2])
    for k in sorted(rep):
        if k.startswith('x0_') or k.startswith('x7_'):
            print(k, rep[k])
