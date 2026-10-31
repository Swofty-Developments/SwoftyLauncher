"""BC1, BC3, BC4 and BC5 texture blocks in numpy, BC7 decode through rnd-tyres-thermal's bc7dec.mjs, and the
F1 25 surface (`.tif.image`) levels: embedded mips plus the `.mipmaps` sidecar levels (LZ4 or stored).

Formats: 52 and 54 BC1, 55 and 57 BC3, 65 BC5, 69 and 70 BC7.
"""

import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parents[4]
BC7_DEC = REPO / "addons/rnd-tyres-thermal/tools/bc7dec.mjs"
BC1, BC3, BC5, BC7 = (52, 54), (55, 57), (65,), (69, 70)
LZ4 = 3


def block_bytes(fmt):
    return 8 if fmt in BC1 or fmt == 63 else 16  # BC4, used by original tyre damage maps


def level_sizes(fmt, w, h, levels):
    return [max(1, (max(1, w >> k) + 3) // 4) * max(1, (max(1, h >> k) + 3) // 4) * block_bytes(fmt)
            for k in range(levels)]


def _565(c):
    r = ((c >> 11) & 31).astype(np.int32)
    g = ((c >> 5) & 63).astype(np.int32)
    b = (c & 31).astype(np.int32)
    return np.stack([(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)], -1)


def _unblock(px, w, h):
    bw, bh = (w + 3) // 4, (h + 3) // 4
    img = px.reshape(bh, bw, 4, 4, -1).transpose(0, 2, 1, 3, 4).reshape(bh * 4, bw * 4, -1)
    return img[:h, :w]


def _colour(raw, four):
    c0 = raw[:, 0:2].copy().view("<u2")[:, 0]
    c1 = raw[:, 2:4].copy().view("<u2")[:, 0]
    bits = raw[:, 4:8].copy().view("<u4")[:, 0]
    e0, e1 = _565(c0), _565(c1)
    mode4 = np.ones(len(raw), bool) if four else c0 > c1
    pal = np.empty((len(raw), 4, 4), np.int32)
    pal[:, 0, :3], pal[:, 1, :3] = e0, e1
    pal[:, 2, :3] = np.where(mode4[:, None], (2 * e0 + e1) // 3, (e0 + e1) // 2)
    pal[:, 3, :3] = np.where(mode4[:, None], (e0 + 2 * e1) // 3, 0)
    pal[..., 3] = 255
    pal[:, 3, 3] = np.where(mode4, 255, 0)
    idx = (bits[:, None] >> (2 * np.arange(16))) & 3
    return np.take_along_axis(pal, idx[..., None].astype(np.int64), 1)


def _alpha(raw):
    a0 = raw[:, 0].astype(np.int32)
    a1 = raw[:, 1].astype(np.int32)
    bits = np.zeros(len(raw), np.uint64)
    for i in range(6):
        bits |= raw[:, 2 + i].astype(np.uint64) << np.uint64(8 * i)
    pal = np.empty((len(raw), 8), np.int32)
    pal[:, 0], pal[:, 1] = a0, a1
    eight = a0 > a1
    for i in range(1, 7):
        pal[:, 1 + i] = np.where(eight, ((7 - i) * a0 + i * a1) // 7, 0)
    for i in range(1, 5):
        pal[:, 1 + i] = np.where(eight, pal[:, 1 + i], ((5 - i) * a0 + i * a1) // 5)
    pal[:, 6] = np.where(eight, pal[:, 6], 0)
    pal[:, 7] = np.where(eight, pal[:, 7], 255)
    idx = ((bits[:, None] >> (3 * np.arange(16, dtype=np.uint64))) & np.uint64(7)).astype(np.int64)
    return np.take_along_axis(pal, idx, 1)


def bc7_decode(data, w, h):
    with tempfile.TemporaryDirectory() as d:
        src, dst = os.path.join(d, "in.bc7"), os.path.join(d, "out.rgba")
        Path(src).write_bytes(data)
        subprocess.run(["node", str(BC7_DEC), src, str(w), str(h), dst], check=True)
        return np.frombuffer(Path(dst).read_bytes(), np.uint8).reshape(h, w, 4).copy()


def decode(fmt, data, w, h):
    if fmt in BC7:
        return bc7_decode(bytes(data[:level_sizes(fmt, w, h, 1)[0]]), w, h)
    n = ((w + 3) // 4) * ((h + 3) // 4)
    raw = np.frombuffer(data, np.uint8, n * block_bytes(fmt)).reshape(n, -1)
    if fmt in BC1:
        px = _colour(raw, False)
    elif fmt in BC3:
        px = _colour(raw[:, 8:], True)
        px[..., 3] = _alpha(raw[:, :8])
    elif fmt in BC5:
        px = np.zeros((n, 16, 4), np.int32)
        px[..., 0], px[..., 1] = _alpha(raw[:, :8]), _alpha(raw[:, 8:])
        px[..., 3] = 255
    else:
        raise ValueError(f"format {fmt}: no decoder")
    return _unblock(px.astype(np.uint8), w, h)


def _blocks(img):
    h, w = img.shape[:2]
    return img.reshape(h // 4, 4, w // 4, 4, -1).transpose(0, 2, 1, 3, 4).reshape(-1, 16, img.shape[2])


def _q565(c):
    c = np.clip(np.rint(c), 0, 255).astype(np.int32)
    return ((c[..., 0] * 31 + 127) // 255 << 11) | ((c[..., 1] * 63 + 127) // 255 << 5) | ((c[..., 2] * 31 + 127) // 255)


def _encode_colour(px):
    src = px[..., :3].astype(np.float64)
    mean = src.mean(1)
    dev = src - mean[:, None]
    cov = np.einsum("nki,nkj->nij", dev, dev)
    axis = np.ones((len(px), 3)) / np.sqrt(3)
    for _ in range(8):
        axis = np.einsum("nij,nj->ni", cov, axis)
        axis /= np.maximum(np.linalg.norm(axis, axis=1, keepdims=True), 1e-9)
    t = np.einsum("nki,ni->nk", dev, axis)
    lo, hi = t.min(1), t.max(1)
    e0 = mean + hi[:, None] * axis
    e1 = mean + lo[:, None] * axis
    w = np.array([0, 3, 1, 2]) / 3.0
    for _ in range(2):
        c0, c1 = _q565(e0), _q565(e1)
        p0, p1 = _565(c0).astype(np.float64), _565(c1).astype(np.float64)
        pal = p0[:, None] * (1 - w)[None, :, None] + p1[:, None] * w[None, :, None]
        idx = ((src[:, :, None] - pal[:, None]) ** 2).sum(-1).argmin(2)
        f = w[idx]
        g = 1 - f
        a, b, c = (g * g).sum(1), (g * f).sum(1), (f * f).sum(1)
        x0, x1 = (g[..., None] * src).sum(1), (f[..., None] * src).sum(1)
        det = a * c - b * b
        ok = np.abs(det) > 1e-9
        safe = np.where(ok, det, 1)[:, None]
        e0 = np.where(ok[:, None], (c[:, None] * x0 - b[:, None] * x1) / safe, e0)
        e1 = np.where(ok[:, None], (a[:, None] * x1 - b[:, None] * x0) / safe, e1)
    c0, c1 = _q565(e0), _q565(e1)
    swap = c0 < c1
    c0, c1 = np.where(swap, c1, c0), np.where(swap, c0, c1)
    p0, p1 = _565(c0).astype(np.float64), _565(c1).astype(np.float64)
    pal = p0[:, None] * (1 - w)[None, :, None] + p1[:, None] * w[None, :, None]
    idx = ((src[:, :, None] - pal[:, None]) ** 2).sum(-1).argmin(2)
    idx[c0 == c1] = 0
    bits = (idx.astype(np.uint64) << (2 * np.arange(16, dtype=np.uint64))).sum(1)
    out = np.zeros((len(px), 8), np.uint8)
    out[:, 0:2] = c0.astype("<u2").view(np.uint8).reshape(-1, 2)
    out[:, 2:4] = c1.astype("<u2").view(np.uint8).reshape(-1, 2)
    out[:, 4:8] = bits.astype("<u4").view(np.uint8).reshape(-1, 4)
    return out


def _encode_alpha(a):
    a = a.astype(np.int32)
    a0, a1 = a.max(1), a.min(1)
    pal = np.empty((len(a), 8), np.int32)
    pal[:, 0], pal[:, 1] = a0, a1
    for i in range(1, 7):
        pal[:, 1 + i] = ((7 - i) * a0 + i * a1) // 7
    idx = np.abs(a[:, :, None] - pal[:, None]).argmin(2)
    idx[a0 == a1] = 0
    bits = (idx.astype(np.uint64) << (3 * np.arange(16, dtype=np.uint64))).sum(1)
    out = np.zeros((len(a), 8), np.uint8)
    out[:, 0], out[:, 1] = a0, a1
    out[:, 2:8] = bits.astype("<u8").view(np.uint8).reshape(-1, 8)[:, :6]
    return out


def encode(fmt, img):
    h, w = img.shape[:2]
    pad = np.pad(img, ((0, (-h) % 4), (0, (-w) % 4), (0, 0)), mode="edge")
    px = _blocks(pad)
    if fmt in BC1:
        return _encode_colour(px).tobytes()
    if fmt in BC3:
        return np.concatenate([_encode_alpha(px[..., 3]), _encode_colour(px)], 1).tobytes()
    raise ValueError(f"format {fmt}: no encoder")


def downsample(img):
    h, w = img.shape[:2]
    f = img.astype(np.float32)
    if h > 1:
        f = (f[0::2] + f[1::2]) / 2
    if w > 1:
        f = (f[:, 0::2] + f[:, 1::2]) / 2
    return np.clip(np.rint(f), 0, 255).astype(np.uint8)


def mip_chain(img, smallest=4):
    out = [img]
    while min(out[-1].shape[:2]) > smallest:
        out.append(downsample(out[-1]))
    return out


class Surface:
    def __init__(self, head, levels):
        self.head = head
        self.fmt, self.w, self.h, self.levels = levels

    @staticmethod
    def header(data):
        h = struct.unpack("<10I", data[:40])
        return h[2], h[3], h[4], h[6]


def sidecar(desc):
    n = desc[0]
    path = desc[1:1 + n].rstrip(b"\0").decode().replace("\\", "/")
    at = 1 + n + 1
    count = struct.unpack_from("<I", desc, at)[0]
    levels = [struct.unpack_from("<BBQQQ", desc, at + 4 + 26 * i) for i in range(count)]
    return path, levels


def read_level(full, entry):
    _, comp, off, packed, size = entry
    with open(full, "rb") as f:
        f.seek(off)
        raw = f.read(packed)
    if comp == LZ4:
        import lz4.block
        raw = lz4.block.decompress(raw, uncompressed_size=size)
    return raw


def top_level(frags, game):
    """The surface's largest level as RGBA: from the sidecar when it has levels (the first of the game folders in
    game, a path or a list, that holds it), else the first embedded level."""
    fmt, w, h, levels = Surface.header(frags[0])
    roots = [game] if isinstance(game, (str, Path)) else list(game or [])
    if len(frags) > 2 and frags[2] and frags[2][0]:
        path, side = sidecar(frags[2])
        found = [Path(r) / path for r in roots if (Path(r) / path).exists()]
        if side and found:
            n = len(side)
            return decode(fmt, read_level(found[0], side[0]), w << n, h << n)
    return decode(fmt, frags[1], w, h)


def surface_frags(head0, fmt, img):
    """Header and embedded mip chain for img, every level in the archive."""
    chain = mip_chain(img)
    head = bytearray(head0)
    struct.pack_into("<I", head, 8, fmt)
    struct.pack_into("<2I", head, 12, img.shape[1], img.shape[0])
    struct.pack_into("<I", head, 24, len(chain))
    return bytes(head), b"".join(encode(fmt, lv) for lv in chain), len(chain)


if __name__ == "__main__":
    sys.exit(__doc__)
