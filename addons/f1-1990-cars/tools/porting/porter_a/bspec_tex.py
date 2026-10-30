import os, shutil, sys
import numpy as np
from PIL import Image
GOLD = np.array([212, 175, 55], np.float32) / 255.0


def tint(src_dir, dst_dir, stem):
    if os.path.exists(dst_dir):
        shutil.rmtree(dst_dir)
    shutil.copytree(src_dir, dst_dir)
    p = os.path.join(dst_dir, f'{stem}_wheel_d.png')
    a = np.asarray(Image.open(p).convert('RGB')).astype(np.float32) / 255.0
    lum = a.mean(-1, keepdims=True)
    out = np.clip(0.25 + lum * 1.1, 0, 1) * GOLD
    Image.fromarray((out * 255).astype(np.uint8)).save(p)


if __name__ == '__main__':
    tint(sys.argv[1], sys.argv[2], sys.argv[3])
