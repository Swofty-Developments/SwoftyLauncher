import sys, struct, zlib, subprocess, io, os
import numpy as np
sys.path.insert(0, '/mnt/work/VSC/F1ModManager/tools')
import erp
from PIL import Image

FMT = {0x36: ('DXT1', 0.5), 0x39: ('DXT5', 1), 0x34: ('ATI1', 0.5), 0x41: ('ATI2', 1), 0x35: ('DXT3', 1), 0x37: ('DXT3', 1)}

class Erp:
    def __init__(self, path):
        self.path = path
        self.meta, self.res = erp.parse(path)
        self.f = open(path, 'rb')
        self.by = {r[0]: r for r in self.res}

    def frag(self, fr):
        fn, off, size, fl, comp, packed = fr
        self.f.seek(self.meta['data_off'] + off)
        d = self.f.read(packed)
        if d[:4] == b'\x28\xb5\x2f\xfd':
            d = subprocess.run(['zstd', '-dc'], input=d, capture_output=True).stdout
        elif d[:1] == b'\x78':
            try:
                d = zlib.decompress(d)
            except zlib.error:
                pass
        return d

    def find(self, sub, rtype=None):
        return [r for r in self.res if sub in r[0] and (rtype is None or r[1] == rtype)]

    def texture(self, name):
        r = self.by[name]
        hdr = self.frag(r[3][0])
        data = self.frag(r[3][1])
        fmt, w, h = struct.unpack_from('<III', hdr, 8)
        return fmt, w, h, data

def dds_image(fmt, w, h, data):
    code, bpp = FMT[fmt]
    size = int(w * h * bpp) if w >= 4 else int(16 * bpp)
    size = max(size, int(16 * bpp))
    top = data[:size]
    hdr = struct.pack('<4sIIIIIII44s', b'DDS ', 124, 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000, h, w, size, 0, 1, b'\0' * 44)
    hdr += struct.pack('<II4sIIIII', 32, 0x4, code.encode(), 0, 0, 0, 0, 0)
    hdr += struct.pack('<IIIII', 0x1000, 0, 0, 0, 0)
    if code in ('ATI1', 'ATI2'):
        return decode_bc45(code, w, h, top)
    return Image.open(io.BytesIO(hdr + top)).convert('RGBA')

def _bc4_block(b):
    r0, r1 = b[0], b[1]
    if r0 > r1:
        pal = [r0, r1] + [((6 - i) * r0 + (1 + i) * r1) // 7 for i in range(6)]
    else:
        pal = [r0, r1] + [((4 - i) * r0 + (1 + i) * r1) // 5 for i in range(4)] + [0, 255]
    bits = int.from_bytes(b[2:8], 'little')
    return [pal[(bits >> (3 * i)) & 7] for i in range(16)]

def decode_bc45(code, w, h, data):
    import numpy as np
    bw, bh = max(1, w // 4), max(1, h // 4)
    out = np.zeros((bh * 4, bw * 4, 4), np.uint8)
    out[..., 3] = 255
    step = 8 if code == 'ATI1' else 16
    o = 0
    for by in range(bh):
        for bx in range(bw):
            blk = data[o:o + step]; o += step
            r = _bc4_block(blk[:8])
            out[by * 4:by * 4 + 4, bx * 4:bx * 4 + 4, 0] = np.array(r, np.uint8).reshape(4, 4)
            if code == 'ATI2':
                g = _bc4_block(blk[8:16])
                out[by * 4:by * 4 + 4, bx * 4:bx * 4 + 4, 1] = np.array(g, np.uint8).reshape(4, 4)
                out[by * 4:by * 4 + 4, bx * 4:bx * 4 + 4, 2] = 255
            else:
                out[by * 4:by * 4 + 4, bx * 4:bx * 4 + 4, 1] = out[by * 4:by * 4 + 4, bx * 4:bx * 4 + 4, 0]
                out[by * 4:by * 4 + 4, bx * 4:bx * 4 + 4, 2] = out[by * 4:by * 4 + 4, bx * 4:bx * 4 + 4, 0]
    return Image.fromarray(out[:h, :w], 'RGBA')

if __name__ == '__main__':
    import os
    e = Erp(sys.argv[1]); out = sys.argv[2]; os.makedirs(out, exist_ok=True)
    for r in e.res:
        if r[1] != 'GfxSurfaceRes': continue
        name = r[0]
        if '/teams/' not in name and '/generics/' not in name: continue
        stem = name.split('/')[-1].replace('.tif.image', '')
        try:
            fmt, w, h, data = e.texture(name)
            im = dds_image(fmt, w, h, data)
            im.save(f'{out}/{stem}.png')
            print(stem, hex(fmt), w, h)
        except Exception as ex:
            print('FAIL', stem, ex)


def lz4_block(src, off, out_size):
    out = bytearray()
    i = off
    n = len(src)
    while len(out) < out_size and i < n:
        tok = src[i]; i += 1
        lit = tok >> 4
        if lit == 15:
            while True:
                b = src[i]; i += 1
                lit += b
                if b != 255:
                    break
        out += src[i:i + lit]; i += lit
        if len(out) >= out_size or i >= n:
            break
        mo = src[i] | (src[i + 1] << 8); i += 2
        ml = tok & 15
        if ml == 15:
            while True:
                b = src[i]; i += 1
                ml += b
                if b != 255:
                    break
        ml += 4
        start = len(out) - mo
        if mo >= ml:
            out += out[start:start + ml]
        else:
            for k in range(ml):
                out.append(out[start + k])
    return bytes(out[:out_size]), i - off


def sidecar_desc(e, name):
    r = e.by[name]
    if len(r[3]) < 3:
        return None
    d = e.frag(r[3][2])
    n = d[0]
    path = d[1:1 + n].decode()
    cnt = struct.unpack_from('<I', d, 1 + n)[0]
    mips = []
    o = 5 + n
    for _ in range(cnt):
        comp = d[o]
        off, packed, size = struct.unpack_from('<QQQ', d, o + 1)
        mips.append((comp, off, packed, size))
        o += 25
    return path, mips


def texture_full(e, name, sidecar_root, max_size=None):
    fmt, w, h, data = e.texture(name)
    desc = sidecar_desc(e, name)
    if not desc:
        return fmt, w, h, data, 'tail'
    path, mips = desc
    leaf = path.split('/')
    car = leaf[-3]
    f = os.path.join(sidecar_root, car, 'textures', leaf[-1])
    if not os.path.exists(f):
        return fmt, w, h, data, 'tail (no sidecar)'
    buf = open(f, 'rb').read()
    code, bpp = FMT[fmt]
    best = None
    for comp, off, packed, size in sorted(mips, key=lambda m: -m[3]):
        k = int(round(np.log2(np.sqrt(size / (w * h * bpp)))))
        tw, th = w << k, h << k
        if max_size and max(tw, th) > max_size:
            continue
        if comp == 0:
            raw = buf[off:off + size]
        elif comp == 3:
            raw, used = lz4_block(buf, off, size)
        else:
            continue
        if len(raw) == size:
            best = (fmt, tw, th, raw, f'sidecar {tw}x{th} comp {comp}')
            break
    return best or (fmt, w, h, data, 'tail (sidecar failed)')


def extract_full(erp_path, sidecar_root, out_dir, stems=('decal_da', 'paint_d', 'detail_d', 'wheel_d', 'steering_wheel_d', 'steering_wheel_decal_da')):
    e = Erp(erp_path)
    os.makedirs(out_dir, exist_ok=True)
    log = {}
    for r in e.res:
        if r[1] != 'GfxSurfaceRes' or '/teams/' not in r[0]:
            continue
        stem = r[0].split('/')[-1].replace('.tif.image', '')
        if not any(stem.endswith('_' + s) for s in stems):
            continue
        fmt, w, h, data, src = texture_full(e, r[0], sidecar_root)
        dds_image(fmt, w, h, data).save(f'{out_dir}/{stem}.png')
        log[stem] = src
    return log
