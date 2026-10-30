import os, sys
os.environ.setdefault('PYOPENGL_PLATFORM', 'egl')
import numpy as np
import trimesh
import pyrender
from PIL import Image, ImageDraw


def look_at(eye, target, up=(0, 1, 0)):
    eye, target, up = (np.asarray(v, float) for v in (eye, target, up))
    f = target - eye; f /= np.linalg.norm(f)
    r = np.cross(f, up); r /= np.linalg.norm(r)
    u = np.cross(r, f)
    m = np.eye(4); m[:3, 0] = r; m[:3, 1] = u; m[:3, 2] = -f; m[:3, 3] = eye
    return m


VIEWS = {'front34': ((1, 0.45, 1), 2.0), 'rear34': ((-1, 0.5, -1.0), 2.0), 'side': ((1, 0.05, 0), 1.9), 'top': ((0.001, 1, 0.0001), 1.9),
         'front': ((0, 0.2, 1), 2.0), 'nose': ((0.6, 0.8, 1.2), 0.75), 'sidepod': ((1, 0.35, 0.15), 0.95)}


def main(path, out_prefix, w, h, names):
    tm = trimesh.load(path, force='scene', process=False)
    scene = pyrender.Scene(bg_color=[0.55, 0.6, 0.65, 1.0], ambient_light=[0.45, 0.45, 0.45])
    for node_name in tm.graph.nodes_geometry:
        T, gname = tm.graph[node_name]
        try:
            m = pyrender.Mesh.from_trimesh(tm.geometry[gname], smooth=False)
        except Exception:
            continue
        scene.add(m, pose=T)
    lo, hi = tm.bounds
    c = (lo + hi) / 2
    r = np.linalg.norm(hi - lo) * 0.62
    rend = pyrender.OffscreenRenderer(w, h)
    for name in names:
        d, k = VIEWS[name]
        d = np.asarray(d, float); d /= np.linalg.norm(d)
        tgt = c.copy()
        if name == 'nose':
            tgt = np.array([0.0, c[1], hi[2] - 0.75])
        cam = pyrender.PerspectiveCamera(yfov=np.radians(32), aspectRatio=w / h)
        eye = tgt + d * r * k
        up = (0, 0, 1) if name == 'top' else (0, 1, 0)
        cn = scene.add(cam, pose=look_at(eye, tgt, up))
        ln = scene.add(pyrender.DirectionalLight(intensity=3.5), pose=look_at(eye + np.array([0, 2, 0]), tgt, up))
        col, _ = rend.render(scene)
        scene.remove_node(cn); scene.remove_node(ln)
        Image.fromarray(col).save(f'{out_prefix}_{name}.png')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5].split(','))
