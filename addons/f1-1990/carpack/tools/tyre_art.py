"""Draw the 1990 Goodyear and Pirelli sidewalls of the car pack's tyre set.

The stock tyre_sidewall shader reads two texture arrays of two 1024x1024 BC7 slices each. Slice 0 of
`sidewall_logo_surface.texarray` holds the slick lettering mask in R and the wet and intermediate mask in G (the
material's `me_logoType` 0 or 1 picks the channel); `me_logoColour` paints the mask. Slice 0 of
`sidewall_diffuse.texarray` is the rubber, with a barcode sticker in RGB. The car pack's 13-inch tyre maps its
0.175-0.307 m sidewall onto the stock 18-inch band (texel radius = 800 x radius + 213, stock 1537.5 x radius - 15.5),
so lettering is drawn in physical units and resampled into that band.

Goodyear: white GOOD-wingfoot-YEAR and EAGLE, one of each per sidewall, from the ASR McLaren MP4/5B sidewall
(`--goodyear`, an Assetto Corsa mod, private like the rest of the car pack) without its chalk and crayon marks. Pirelli:
the long-P PIRELLI wordmark of the stock mask, 1.3 times its stock size, at the top and the bottom of each sidewall.
Both lettering masks go into R and G; B and A and slice 1 stay stock. The 1990 diffuse is the stock one without the
barcode.

Writes `carpack/erp/tyres/tyre_art.erpk` (not committed): the three arrays under their f1_1990 names, which
build_pack.py adds to common.erp with the supplier materials and material bindings.

usage: tyre_art.py [--common <13-inch common.erp>] [--goodyear <png>] [--sheet <png>]
needs: numpy, Pillow, node (bc7 decode), nvcompress (NVIDIA Texture Tools)
"""
import argparse
import os
import struct
import subprocess
import sys
import tempfile

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(HERE)
ADDON = os.path.dirname(PACK)
REPO = os.path.dirname(os.path.dirname(ADDON))
sys.path[:0] = [os.path.join(REPO, 'tools'), os.path.join(REPO, 'addons', 'rnd-tyres-thermal', 'tools'),
                os.path.join(ADDON, 'tools')]

import erpw
from common import bc7_decode
import tyre_sets as T

COMMON = '/mnt/work/.mp45b/porter_a/tries/tyres13cm3/common.erp'
GOODYEAR = '/mnt/work/.mp45b/gltf/asr_1990_mclaren_mp4-5b/textures/png/G9X_twall-r.png'
OUT = os.path.join(PACK, 'erp', 'tyres', 'tyre_art.erpk')
SIZE = 1024
STOCK_MAP = (1537.5, -15.5)
CM13_MAP = (800.0, 213.0)
AC_MAP = (1630.0, 12.0)
SIDEWALL = (0.175, 0.307)
OUTER = 0.351
GOODYEAR_BAND = (355.0, 470.0)
GOODYEAR_CHALK = (np.radians(150), np.radians(240), 376.0)
GOODYEAR_LEVELS = (0.17, 0.27)
PIRELLI_BAND = (370.0, 456.0)
PIRELLI_SCALE = 1.3
PIRELLI_CENTRE = 0.247
BARCODE_LEVEL = 0.35
BARCODE_SHIFT = np.radians(25)


def polar(size=SIZE):
    yy, xx = np.mgrid[0:size, 0:size] + 0.5
    dx, dy = xx - size / 2, yy - size / 2
    return np.hypot(dx, dy), np.arctan2(dx, -dy)


def wrap(a):
    return (a + np.pi) % (2 * np.pi) - np.pi


def sample(img, x, y):
    h, w = img.shape[:2]
    x, y = x - 0.5, y - 0.5
    x0, y0 = np.floor(x).astype(int), np.floor(y).astype(int)
    fx, fy = x - x0, y - y0
    if img.ndim == 3:
        fx, fy = fx[..., None], fy[..., None]

    def at(yi, xi):
        return img[np.clip(yi, 0, h - 1), np.clip(xi, 0, w - 1)]
    return ((at(y0, x0) * (1 - fx) + at(y0, x0 + 1) * fx) * (1 - fy)
            + (at(y0 + 1, x0) * (1 - fx) + at(y0 + 1, x0 + 1) * fx) * fy)


def place(src, src_map, src_theta, src_rho, out_theta, out_rho, scale):
    r, th = polar()
    rho = (r - CM13_MAP[1]) / CM13_MAP[0]
    arc = rho * wrap(th - out_theta)
    rho_s = src_rho + (rho - out_rho) / scale
    th_s = src_theta + arc / scale / np.maximum(rho_s, 1e-6)
    r_s = src_map[0] * rho_s + src_map[1]
    n = src.shape[0]
    out = sample(src, n / 2 + r_s * np.sin(th_s), n / 2 - r_s * np.cos(th_s))
    return out * ((rho >= SIDEWALL[0]) & (rho <= SIDEWALL[1]))


def smoothstep(a, b, v):
    t = np.clip((v - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


def goodyear_mask(path):
    ac = np.asarray(Image.open(path).convert('RGB')).astype(np.float64) / 255
    r, th = polar(ac.shape[0])
    lum = ac.mean(-1)
    grey = (ac.max(-1) - ac.min(-1)) < 0.08
    band = (r >= GOODYEAR_BAND[0]) & (r <= GOODYEAR_BAND[1])
    a0, a1, inner = GOODYEAR_CHALK
    chalk = (np.mod(th, 2 * np.pi) >= a0) & (np.mod(th, 2 * np.pi) <= a1) & (r < inner)
    mask = smoothstep(*GOODYEAR_LEVELS, lum) * (band & grey & ~chalk)
    core = Image.fromarray(((mask > 0.5) * 255).astype(np.uint8)).filter(ImageFilter.MinFilter(5))
    mask *= np.asarray(core.filter(ImageFilter.MaxFilter(11))) > 0
    rho_c = 0.25
    return np.clip(place(mask, AC_MAP, 0.0, rho_c, 0.0, rho_c, 1.0), 0, 1)


def wordmark_span(mask):
    r, th = polar()
    band = (r >= PIRELLI_BAND[0]) & (r <= PIRELLI_BAND[1])
    bins = 720
    idx = ((wrap(th) + np.pi) / (2 * np.pi) * bins).astype(int) % bins
    prof = np.zeros(bins)
    np.maximum.at(prof, idx[band], mask[band])
    on = prof > 0.5
    zero = bins // 2
    lo = hi = zero
    gap = 6
    while any(on[(lo - k) % bins] for k in range(1, gap)):
        lo -= 1
    while any(on[(hi + k) % bins] for k in range(1, gap)):
        hi += 1
    return (lo - zero - 1) * 2 * np.pi / bins, (hi - zero + 2) * 2 * np.pi / bins


def pirelli_mask(stock_logo):
    src = stock_logo[..., 0]
    r, th = polar()
    t0, t1 = wordmark_span(src)
    keep = (r >= PIRELLI_BAND[0]) & (r <= PIRELLI_BAND[1]) & (wrap(th) >= t0) & (wrap(th) <= t1)
    word = src * keep
    rho_src = ((PIRELLI_BAND[0] + PIRELLI_BAND[1]) / 2 - STOCK_MAP[1]) / STOCK_MAP[0]
    centre = (t0 + t1) / 2
    top = place(word, STOCK_MAP, centre, rho_src, 0.0, PIRELLI_CENTRE, PIRELLI_SCALE)
    bottom = place(word, STOCK_MAP, centre, rho_src, np.pi, PIRELLI_CENTRE, PIRELLI_SCALE)
    return np.clip(np.maximum(top, bottom), 0, 1), (t0, t1)


def strip_barcode(diffuse):
    out = diffuse.copy()
    rgb = diffuse[..., :3]
    sticker = rgb.mean(-1) > BARCODE_LEVEL
    sticker = np.asarray(Image.fromarray((sticker * 255).astype(np.uint8)).filter(ImageFilter.MaxFilter(9))) > 0
    r, th = polar()
    th2 = th + BARCODE_SHIFT
    patch = sample(rgb, SIZE / 2 + r * np.sin(th2), SIZE / 2 - r * np.cos(th2))
    out[..., :3][sticker] = patch[sticker]
    return out, int(sticker.sum())


def decode_slices(e, name):
    r = e.find(name + '.image')
    head, data = e.frag_data(r, 0), e.frag_data(r, 1)
    fmt, w, h = struct.unpack_from('<III', head, 8)
    mips, count = struct.unpack_from('<II', head, 24)
    sizes = [max(1, (w >> m) // 4) * max(1, (h >> m) // 4) * 16 for m in range(mips)]
    per = sum(sizes)
    if (w, h, count, len(data)) != (SIZE, SIZE, 2, per * count):
        raise ValueError(f'{name}: {w}x{h}, {count} slices, {len(data)} bytes')
    first = np.frombuffer(bc7_decode(data[:sizes[0]], w, h), np.uint8).reshape(h, w, 4)
    return first.astype(np.float64) / 255, data, per, mips


def nv_bc7(rgba):
    with tempfile.TemporaryDirectory() as d:
        src, dst = os.path.join(d, 'in.png'), os.path.join(d, 'out.dds')
        Image.fromarray(rgba, 'RGBA').save(src)
        subprocess.run(['nvcompress', '-silent', '-nocuda', '-nomips', '-bc7', src, dst], check=True,
                       capture_output=True)
        dds = open(dst, 'rb').read()
    head = 148 if dds[84:88] == b'DX10' else 128
    size = max(1, rgba.shape[1] // 4) * max(1, rgba.shape[0] // 4) * 16
    if len(dds) != head + size:
        raise ValueError(f'nvcompress: {len(dds)} bytes for {rgba.shape[1]}x{rgba.shape[0]}')
    return dds[head:]


def encode_chain(rgba, mips):
    level = Image.fromarray(np.clip(np.rint(rgba * 255), 0, 255).astype(np.uint8), 'RGBA')
    out = []
    for m in range(mips):
        if m:
            level = level.resize((max(4, SIZE >> m), max(4, SIZE >> m)), Image.Resampling.BOX)
        out.append(nv_bc7(np.asarray(level)))
    return b''.join(out)


def array_res(e, stock, new, slice0, stock_data, per, mips):
    image, view = e.find(stock + '.image'), e.find(stock)
    data = encode_chain(slice0, mips)
    if len(data) != per:
        raise ValueError(f'{new}: slice of {len(data)} bytes, stock {per}')
    q_image = T.clone(e, image, new + '.image')
    e.set_frag(q_image, 1, data + stock_data[per:])
    q_view = T.clone(e, view, new)
    e.set_frag(q_view, 0, T.srv_retarget(e.frag_data(view, 0), new + '.image'))
    return [q_image, q_view]


def wheel_view(mask, diffuse, px=420):
    yy, xx = np.mgrid[0:px, 0:px] + 0.5
    dx, dy = (xx - px / 2) / (px / 2) * OUTER, (yy - px / 2) / (px / 2) * OUTER
    rho, th = np.hypot(dx, dy), np.arctan2(dx, -dy)
    tex_r = CM13_MAP[0] * rho + CM13_MAP[1]
    x, y = SIZE / 2 + tex_r * np.sin(th), SIZE / 2 - tex_r * np.cos(th)
    m = sample(mask, x, y)[..., None]
    rubber = (0.05 + 0.12 * sample(diffuse[..., :3].mean(-1), x, y))[..., None] * np.ones(3)
    img = rubber * (1 - m) + np.array([0.97, 0.97, 0.96]) * m
    img[rho < SIDEWALL[0]] = (0.32, 0.32, 0.34)
    img[rho < SIDEWALL[0] * 0.55] = (0.18, 0.18, 0.2)
    img[(rho > SIDEWALL[1]) & (rho <= OUTER)] = (0.035, 0.035, 0.035)
    img[rho > OUTER] = (0.16, 0.16, 0.16)
    return Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8), 'RGB')


def contact_sheet(path, looks, stock_view):
    font = ImageFont.load_default(size=20)
    small = ImageFont.load_default(size=15)
    tile, pad = 420, 16
    tiles = [stock_view] + looks
    sheet = Image.new('RGB', (pad + len(tiles) * (tile + pad), tile + 100), (22, 22, 24))
    draw = ImageDraw.Draw(sheet)
    for i, (label, note, view) in enumerate(tiles):
        x = pad + i * (tile + pad)
        sheet.paste(view, (x, 40))
        draw.text((x, 10), label, fill=(240, 240, 240), font=font)
        draw.text((x, tile + 52), note, fill=(170, 170, 170), font=small)
    sheet.save(path)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    ap.add_argument('--common', default=COMMON, help='the 13-inch common.erp (f1_2026 names)')
    ap.add_argument('--goodyear', default=GOODYEAR, help='the ASR MP4/5B sidewall PNG')
    ap.add_argument('--sheet', help='write a preview PNG of both sidewalls')
    args = ap.parse_args()
    e = erpw.Erp(args.common)
    stock_logo, logo_data, logo_per, logo_mips = decode_slices(e, T.stock_name(T.LOGO))
    stock_diffuse, diffuse_data, diffuse_per, diffuse_mips = decode_slices(e, T.stock_name(T.DIFFUSE))
    masks = {'goodyear': goodyear_mask(args.goodyear)}
    masks['pirelli'], span = pirelli_mask(stock_logo)
    diffuse, stripped = strip_barcode(stock_diffuse)
    res = []
    for supplier, mask in masks.items():
        slice0 = stock_logo.copy()
        slice0[..., 0] = mask
        slice0[..., 1] = mask
        res += array_res(e, T.stock_name(T.LOGO), T.new_name(T.logo_array(supplier)), slice0, logo_data, logo_per,
                         logo_mips)
    res += array_res(e, T.stock_name(T.DIFFUSE), T.new_name(T.DIFFUSE_1990), diffuse, diffuse_data, diffuse_per,
                     diffuse_mips)
    e.res = res
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    e.to_file(OUT)
    check = erpw.Erp(OUT)
    for r in check.res:
        for i in range(len(r.frags)):
            check.frag_data(r, i)
    print(f'{OUT}: {len(res)} resources, {os.path.getsize(OUT) / 1e6:.1f} MB; PIRELLI span '
          f'{np.degrees(span[0]):.1f} to {np.degrees(span[1]):.1f} deg; barcode {stripped} texels; '
          f'mask coverage goodyear {masks["goodyear"].mean():.4f} pirelli {masks["pirelli"].mean():.4f}')
    if args.sheet:
        got_diffuse = decode_slices(check, T.new_name(T.DIFFUSE_1990))[0]
        notes = {'goodyear': 'McLaren, Ferrari, Williams, Benetton, Lotus,\nLeyton House, Larrousse, Ligier, Arrows',
                 'pirelli': 'Tyrrell, Minardi, Brabham, Dallara'}
        looks = []
        for supplier in masks:
            got = decode_slices(check, T.new_name(T.logo_array(supplier)))[0]
            looks.append((f'1990 {supplier.capitalize()} (slick and wet)', notes[supplier],
                          wheel_view(got[..., 0], got_diffuse)))
        stock_map = stock_logo[..., 0]
        stock_view = ('Before: stock 2026 on the 13-inch tyre', 'lettering in the compound colour',
                      wheel_view(stock_map, stock_diffuse))
        contact_sheet(args.sheet, looks, stock_view)
    return 0


if __name__ == '__main__':
    sys.exit(main())
