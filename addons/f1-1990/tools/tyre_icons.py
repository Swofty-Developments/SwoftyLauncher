#!/usr/bin/env python3
"""Writes erp/tyre_icons.json and erp/tyre_icons/: 2018-style tyre icons for the 1990 compounds.

Three texture names, each a new slice in every compound texture array of overrides.uidb, and three new glyphs in
the HUD icon font (art/fonts/icons.font), drawn from the stock Soft_f1_26 art with Formula 1 TV lettering:

  SuperSoft_f1_1990  SS  red      icon_code U+E190
  UltraSoft_f1_1990  US  purple   icon_code U+E191
  Soft_f1_1990       S   yellow   icon_code U+E192

usage: tyre_icons.py --game <F1 25> [--baseline <launcher baseline>/<build>] [--sheet <png>]
"""
import argparse
import io
import json
import os
import struct
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from fontTools.ttLib import TTFont
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/rnd-tyres-thermal/tools")]

from common import bc7_decode  # noqa: E402
from erp import parse as erp_parse  # noqa: E402
from erpw import Erp  # noqa: E402
from pkg import Pkg  # noqa: E402
from stock import Stock, fragments  # noqa: E402

UI = "2025_asset_groups/ui_package/"
DATABASES = UI + "databases.erp"
FONTS = UI + "fonts_standard_icons.erp"
FONTS_EFIGS = UI + "fonts_efigs_r_p.erp"
OVERRIDES = "eaid://ui_package/databases/overrides.uidb"
ARRAY = "eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.{}.texarray"
ICON_FONT = "eaid://ui_package/art/fonts/icons.font"
LETTER_FONT = "eaid://ui_package/art/fonts/formula1-tv-600_bold_web.font"
OUT = ROOT / "erp/tyre_icons"
BC7 = 70
BC3 = 0x37

# texture name: (letters, colour, stock tyre picture of that colour, icon font codepoint)
COMPOUNDS = {
    "SuperSoft_f1_1990": ("SS", "red", "Soft_f1_26", 0xE190),
    "UltraSoft_f1_1990": ("US", "purple", "SuperSoft_f2", 0xE191),
    "Soft_f1_1990": ("S", "yellow", "Medium_f1_26", 0xE192),
}
# colour: the stock slice whose accent it is
COLOURS = {"red": "Soft_f1_26", "yellow": "Medium_f1_26", "purple": "SuperSoft_f2"}
# array: how its new slices are made ("badge": relettered Soft_f1_26, "tyre": a stock picture of the colour)
ARRAYS = {
    "tyres.tyres_small": ("badge", "Garage and pause tyre lists"),
    "tyre_allocation.compound_icons": ("badge", "Tyre allocation compound badges"),
    "tyre_allocation.key_icons": ("badge", "Tyre allocation keys"),
    "tyre_allocation.tyre_icons": ("tyre", "Tyre allocation tyre pictures"),
    "osd.car_settings_icons": ("badge", "MFD car settings (with _worn)"),
}
TEMPLATE, WORN_TEMPLATE, GLYPH_TEMPLATE = "Soft_f1_26", "Soft_worn_f1_26", 0x7E
PAGE_W, PAGE_H = 2048, 1024
GLYPH_ROW = 940
RECORD = 0x50


def overrides_node(root, path):
    node = root["overrides"]["Contents"]
    parts = path.split(".")
    for p in parts[:-1]:
        node = node[p]["Contents"]
    return node[parts[-1]]


def carriers(game, names):
    found = {n: [] for n in names}
    for dp, _dn, fn in os.walk(game / UI):
        for f in sorted(fn):
            if f.endswith(".erp"):
                path = Path(dp) / f
                for r in erp_parse(str(path))[1]:
                    if r[0] in found:
                        found[r[0]].append(str(path.relative_to(game)))
    return found


def sfnt(blob, start):
    num = struct.unpack_from(">H", blob, start + 4)[0]
    end = max(sum(struct.unpack_from(">II", blob, start + 12 + 16 * i + 8)) for i in range(num))
    return blob[start:start + end]


def letter_font(stock):
    blob = fragments(stock.path(FONTS_EFIGS), LETTER_FONT)[0]
    at = blob.index(b"\x00\x01\x00\x00\x00\x0c", 0x2b0)
    return sfnt(blob, at)


def unpack(rgba):
    return np.asarray(rgba, dtype=np.float32) / 255.0


def accent(im):
    rgb, a = im[..., :3], im[..., 3]
    sat = np.where(a > 0.9, rgb.max(-1) - rgb.min(-1), 0)
    return np.median(rgb[sat >= 0.6 * sat.max()], 0)


def dark(im):
    rgb, a = im[..., :3], im[..., 3]
    pick = (a > 0.9) & (rgb.max(-1) < 0.25)
    return np.median(rgb[pick], 0)


def radius_in(im, acc, drk, cx, cy):
    """Radius of the disc the letter sits on, and whether that disc is the accent colour (a worn badge, a filled
    disc) rather than dark inside a ring: walking in from the edge along the diagonals, where the accent starts and
    where it ends."""
    outer, inner = [], []
    for dx, dy in ((1, 1), (1, -1), (-1, 1), (-1, -1)):
        r, start = min(cx, cy) - 0.5, None
        while r > 1:
            x, y = int(round(cx + dx * r / 2 ** 0.5)), int(round(cy + dy * r / 2 ** 0.5))
            t = weight(im[y, x, :3], acc, drk)
            if t > 0.5 and start is None and im[y, x, 3] > 0.9:
                start = r
            elif start is not None and t <= 0.5 and im[y, x, 3] > 0.9:
                outer.append(start)
                inner.append(r)
                break
            r -= 0.25
    r_out, r_end = float(np.median(outer)), float(np.median(inner))
    worn = r_out - r_end > 0.3 * r_out
    return (r_out - 0.5 if worn else r_end), worn


def weight(c, acc, drk):
    v = acc - drk
    return np.clip(((c - drk) * v).sum(-1) / (v * v).sum(), 0.0, 1.0)


def draw_letters(text, font_bytes, height, max_width, size, centre, scale=8):
    """Coverage map (h, w) of text with cap height `height` px, condensed to max_width, centred at centre."""
    probe = ImageFont.truetype(io.BytesIO(font_bytes), 200)
    box = probe.getbbox("S")
    cap = box[3] - box[1]
    pt = 200 * height * scale / cap
    font = ImageFont.truetype(io.BytesIO(font_bytes), max(1, int(round(pt))))
    tb = font.getbbox(text)
    tw, th = tb[2] - tb[0], tb[3] - tb[1]
    canvas = Image.new("L", (tw + 8, th + 8), 0)
    ImageDraw.Draw(canvas).text((4 - tb[0], 4 - tb[1]), text, font=font, fill=255)
    width = min(tw, max_width * scale)
    canvas = canvas.resize((max(1, int(round(width))) + 8, th + 8), Image.LANCZOS)
    w, h = size
    big = Image.new("L", (w * scale, h * scale), 0)
    big.paste(canvas, (int(round(centre[0] * scale - canvas.width / 2)), int(round(centre[1] * scale - canvas.height / 2))))
    return np.asarray(big.resize((w, h), Image.BOX), dtype=np.float32) / 255.0


def relabel(template, colour, text, font_bytes):
    """Template badge (coloured ring and letter on a disc) recoloured to colour and relettered with text."""
    im = unpack(template)
    h, w = im.shape[:2]
    acc, drk = accent(im), dark(im)
    a = im[..., 3]
    ys, xs = np.nonzero(a > 0.5)
    cx, cy = (xs.min() + xs.max() + 1) / 2, (ys.min() + ys.max() + 1) / 2
    r_in, worn = radius_in(im, acc, drk, cx, cy)
    t = weight(im[..., :3], acc, drk)
    rgb = np.clip(im[..., :3] + t[..., None] * (colour - acc), 0, 1)
    yy, xx = np.mgrid[0:h, 0:w]
    r = np.hypot(xx + 0.5 - cx, yy + 0.5 - cy)
    inner = r < r_in - 1.0
    fill_t = 1.0 if worn else 0.0
    fill = colour if worn else drk
    letter_rgb = drk if worn else colour
    letter = inner & (np.abs(t - fill_t) > 0.5)
    ly, lx = np.nonzero(letter)
    cap = ly.max() - ly.min() + 1
    centre = ((lx.min() + lx.max() + 1) / 2, (ly.min() + ly.max() + 1) / 2)
    m = np.clip(r_in - 0.5 - r, 0, 1)[..., None]
    rgb = fill * m + rgb * (1 - m)
    height = cap * (1.0 if len(text) == 1 else 0.86)
    cov = draw_letters(text, font_bytes, height, 2 * r_in * (0.74 if len(text) == 1 else 0.86), (w, h), centre)[..., None]
    rgb = rgb * (1 - cov) + letter_rgb * cov
    return Image.fromarray((np.dstack([rgb, a]) * 255 + 0.5).astype(np.uint8), "RGBA")


def nv_encode(im, fmt):
    """BC7 or BC3 blocks of an RGBA image through nvcompress (NVIDIA Texture Tools), one mip."""
    with tempfile.TemporaryDirectory() as d:
        src, dst = os.path.join(d, "in.png"), os.path.join(d, "out.dds")
        im.save(src)
        subprocess.run(["nvcompress", "-silent", "-nocuda", "-alpha", "-nomips", "-" + fmt, src, dst], check=True,
                       capture_output=True)
        dds = open(dst, "rb").read()
    head = 148 if dds[84:88] == b"DX10" else 128
    size = ((im.width + 3) // 4) * ((im.height + 3) // 4) * 16
    if len(dds) != head + size:
        sys.exit(f"nvcompress {fmt}: {len(dds)} bytes for a {im.width}x{im.height} image")
    return dds[head:]


def from565(v):
    return np.array([((v >> 11) & 31) * 255 / 31, ((v >> 5) & 63) * 255 / 63, (v & 31) * 255 / 31], dtype=np.float32)


def bc3_decode_cell(page, x0, y0, w, h):
    out = np.zeros((h, w, 4), np.uint8)
    for by in range(h // 4):
        for bx in range(w // 4):
            o = (((y0 // 4) + by) * (PAGE_W // 4) + (x0 // 4) + bx) * 16
            blk = page[o:o + 16]
            a0, a1 = blk[0], blk[1]
            apal = [a0, a1] + ([((7 - i) * a0 + i * a1) // 7 for i in range(1, 7)] if a0 > a1 else
                               [((5 - i) * a0 + i * a1) // 5 for i in range(1, 5)] + [0, 255])
            abits = int.from_bytes(blk[2:8], "little")
            q0, q1, cbits = struct.unpack_from("<HHI", blk, 8)
            p0, p1 = from565(q0), from565(q1)
            cpal = [p0, p1, (2 * p0 + p1) / 3, (p0 + 2 * p1) / 3] if q0 > q1 else [p0, p1, (p0 + p1) / 2, np.zeros(3)]
            for k in range(16):
                y, x = by * 4 + k // 4, bx * 4 + k % 4
                out[y, x, :3] = np.round(cpal[(cbits >> (2 * k)) & 3])
                out[y, x, 3] = apal[(abits >> (3 * k)) & 7]
    return out


def put_bc3_cell(page, x0, y0, im):
    blocks = nv_encode(im, "bc3")
    bw = im.width // 4
    for by in range(im.height // 4):
        for bx in range(bw):
            o = (((y0 // 4) + by) * (PAGE_W // 4) + (x0 // 4) + bx) * 16
            page[o:o + 16] = blocks[(by * bw + bx) * 16:(by * bw + bx + 1) * 16]


def font_records(blob):
    size = struct.unpack_from("<I", blob, 0x2b7)[0]
    tail = 0x2bb + size
    count, flag, count2 = struct.unpack_from("<I4sI", blob, tail)
    if count != count2 or len(blob) != tail + 12 + count * RECORD:
        sys.exit(f"{ICON_FONT}: unexpected layout ({count}/{count2} records, {len(blob)} bytes)")
    recs = [bytearray(blob[tail + 12 + i * RECORD:tail + 12 + (i + 1) * RECORD]) for i in range(count)]
    return size, tail, flag, recs


def build_font(stock, letters):
    blob, = fragments(stock.path(FONTS), ICON_FONT)
    head, data = fragments(stock.path(FONTS), ICON_FONT + ".pages")
    fmt, w, h = struct.unpack_from("<III", head, 8)
    if (fmt, w, h) != (BC3, PAGE_W, PAGE_H) or len(data) != PAGE_W * PAGE_H:
        sys.exit(f"{ICON_FONT}.pages: unexpected page ({fmt:#x}, {w}x{h})")
    size, tail, flag, recs = font_records(blob)
    by_cp = {struct.unpack_from("<I", r, 0)[0]: r for r in recs}
    taken = [cp for *_, cp in COMPOUNDS.values() if cp in by_cp]
    if taken:
        sys.exit(f"{ICON_FONT}: already maps {taken}")
    tmpl = by_cp[GLYPH_TEMPLATE]
    u0, v0, u1, v1 = struct.unpack_from("<4f", tmpl, 0x20)
    gx0, gx1 = round(u0 * PAGE_W), round(u1 * PAGE_W)
    gy0, gy1 = round((1 - v1) * PAGE_H), round((1 - v0) * PAGE_H)
    cw, ch = gx1 - gx0, gy1 - gy0
    page = bytearray(data)
    template = Image.fromarray(bc3_decode_cell(page, gx0, gy0, cw, ch), "RGBA")
    ttf = TTFont(io.BytesIO(blob[0x2bb:0x2bb + size]))
    order = ttf.getGlyphOrder()
    src = order[struct.unpack_from("<I", tmpl, 4)[0]]
    glyphs, cells, new_recs = [], [], []
    for i, (name, (text, colour, _pic, cp)) in enumerate(COMPOUNDS.items()):
        x0 = 8 + i * (cw + 8)
        if x0 % 4 or GLYPH_ROW % 4 or any(page[(((GLYPH_ROW // 4) + by) * (PAGE_W // 4) + x0 // 4 + bx) * 16]
                                          for by in range(ch // 4) for bx in range(cw // 4)):
            sys.exit(f"{ICON_FONT}.pages: the cell at {x0},{GLYPH_ROW} is not free")
        im = relabel(template, letters["colours"][colour], text, letters["font"])
        put_bc3_cell(page, x0, GLYPH_ROW, im)
        cells.append((name, Image.fromarray(bc3_decode_cell(page, x0, GLYPH_ROW, cw, ch), "RGBA")))
        gname = f"uni{cp:04X}"
        glyphs.append((gname, cp))
        rec = bytearray(tmpl)
        struct.pack_into("<II", rec, 0, cp, len(order) + i)
        struct.pack_into("<4f", rec, 0x20, x0 / PAGE_W, 1 - (GLYPH_ROW + ch) / PAGE_H, (x0 + cw) / PAGE_W,
                         1 - GLYPH_ROW / PAGE_H)
        new_recs.append(rec)
    glyf, hmtx = ttf["glyf"], ttf["hmtx"]
    for gname, cp in glyphs:
        glyf.glyphs[gname] = glyf[src]
        hmtx.metrics[gname] = hmtx.metrics[src]
    ttf.setGlyphOrder(order + [g for g, _ in glyphs])
    for table in ttf["cmap"].tables:
        if table.isUnicode():
            for gname, cp in glyphs:
                table.cmap[cp] = gname
    out = io.BytesIO()
    ttf.save(out)
    sf = out.getvalue()
    padded = sf + b"\0" * (-len(sf) % 4)
    recs = recs[:-1] + new_recs + recs[-1:]
    font = (blob[:0x2b7] + struct.pack("<I", len(padded)) + padded
            + struct.pack("<I4sI", len(recs), flag, len(recs)) + b"".join(recs))
    check = TTFont(io.BytesIO(sf)).getBestCmap()
    if any(check.get(cp) != g for g, cp in glyphs) or font_records(font)[3][-1] != recs[-1]:
        sys.exit(f"{ICON_FONT}: rebuilt font does not read back")
    return font, bytes(page), head, (template, cells), len(recs) - len(new_recs)


def grow_array(stock, game, path, kind, root, letters):
    image, view = ARRAY.format(path) + ".image", ARRAY.format(path)
    where = carriers(game, [image, view])
    if not where[image] or where[image] != where[view]:
        sys.exit(f"{path}: carriers {where[image]} / {where[view]}")
    frags = None
    for rel in where[image]:
        got = fragments(stock.path(rel), image) + fragments(stock.path(rel), view)
        if frags is None:
            frags = got
        elif got != frags:
            sys.exit(f"{path}: {rel} carries a different stock array")
    head, data, view0 = frags
    fmt, w, h = struct.unpack_from("<III", head, 8)
    mips, count = struct.unpack_from("<II", head, 24)
    names = [e["Name"].value for e in overrides_node(root, path)["Contents"].items]
    per = w * h
    if fmt != BC7 or mips != 1 or len(data) != count * per or len(names) != count \
            or struct.unpack_from("<I", view0, 0x14)[0] != count:
        sys.exit(f"{path}: unexpected stock array (format {fmt}, {w}x{h}, {mips} mips, {count} slices)")

    def stock_slice(name):
        i = names.index(name)
        return data[i * per:(i + 1) * per]

    def picture(name):
        return Image.frombytes("RGBA", (w, h), bc7_decode(stock_slice(name), w, h))

    append = []
    for name, (text, colour, pic, _cp) in COMPOUNDS.items():
        variants = [(name, TEMPLATE)] + ([(name + "_worn", WORN_TEMPLATE)] if WORN_TEMPLATE in names else [])
        for slot, tmpl in variants:
            if slot in names:
                sys.exit(f"{path}: already lists {slot}")
            if kind == "tyre":
                append.append((slot, stock_slice(pic), picture(pic)))
            else:
                im = relabel(picture(tmpl), letters["colours"][colour], text, letters["font"])
                px = nv_encode(im, "bc7")
                append.append((slot, px, Image.frombytes("RGBA", (w, h), bc7_decode(px, w, h))))
    head2 = bytearray(head)
    struct.pack_into("<I", head2, 28, count + len(append))
    view2 = bytearray(view0)
    struct.pack_into("<I", view2, 0x14, count + len(append))
    grown = (bytes(head2), data + b"".join(px for _, px, _ in append), bytes(view2))
    refs = [(n, picture(n)) for n in (TEMPLATE, WORN_TEMPLATE, "SuperSoft_f2", "Medium_f1_26") if n in names]
    return where[image], grown, append, names, (w, h), refs


def palette(stock, game, root):
    path = "tyre_allocation.compound_icons"
    image = ARRAY.format(path) + ".image"
    rel = carriers(game, [image])[image][0]
    head, data = fragments(stock.path(rel), image)
    fmt, w, h = struct.unpack_from("<III", head, 8)
    names = [e["Name"].value for e in overrides_node(root, path)["Contents"].items]
    out = {}
    for colour, name in COLOURS.items():
        i = names.index(name)
        im = unpack(Image.frombytes("RGBA", (w, h), bc7_decode(data[i * w * h:(i + 1) * w * h], w, h)))
        out[colour] = accent(im)
    return out


def build(game, baseline, sheet):
    stock = Stock(game, baseline)
    root = Pkg(fragments(stock.path(DATABASES), OVERRIDES)[0]).root
    letters = {"font": letter_font(stock), "colours": palette(stock, game, root)}
    font, page, page_head, glyph_cells, stock_glyphs = build_font(stock, letters)

    if OUT.exists():
        for p in OUT.iterdir():
            p.unlink()
    OUT.mkdir(parents=True, exist_ok=True)
    fonts = Erp(str(stock.path(FONTS)))
    r_font, r_pages = fonts.find(ICON_FONT), fonts.find(ICON_FONT + ".pages")
    fonts.set_frag(r_font, 0, font)
    fonts.set_frag(r_pages, 1, page)
    ops = [{"op": "put-resource", "erp": FONTS, "resource": res, "source": "erp/tyre_icons/tyre_icons.erpk"}
           for res in (ICON_FONT, ICON_FONT + ".pages")]
    report = [f"icons.font: {stock_glyphs} + {len(COMPOUNDS)} glyphs (" +
              ", ".join(f"U+{cp:04X} {text}" for text, _c, _p, cp in COMPOUNDS.values()) + ")"]
    edits, rows, keep, arc = [], [], [r_font, r_pages], None
    for path, (kind, label) in ARRAYS.items():
        rels, grown, append, names, (w, h), refs = grow_array(stock, game, path, kind, root, letters)
        image, view = ARRAY.format(path) + ".image", ARRAY.format(path)
        e = Erp(str(stock.path(rels[0])))
        r_image, r_view = e.find(image), e.find(view)
        e.set_frag(r_image, 0, grown[0])
        e.set_frag(r_image, 1, grown[1])
        e.set_frag(r_view, 0, grown[2])
        keep += [r_image, r_view]
        arc = arc or e
        for rel in rels:
            for res in (image, view):
                ops.append({"op": "put-resource", "erp": rel, "resource": res,
                            "source": "erp/tyre_icons/tyre_icons.erpk"})
        at = "overrides.Contents." + ".Contents.".join(path.split(".")) + ".Contents"
        for slot, *_ in append:
            edits.append({"op": "clone-entry", "at": at, "src": f"[Name={names[-1]}]", "as": slot,
                          "set": {"Value": f"ui_package/art/textures/f1990/tyres/{slot}.tif"}})
        rows.append((f"{label}    overrides.{path}  {w}x{h}", refs, [(s, im) for s, _, im in append]))
        report.append(f"{path}: {w}x{h}, {len(names)} + {len(append)} slices, {len(rels)} archive(s)")
    arc.res = keep
    arc.to_file(str(OUT / "tyre_icons.erpk"))
    ops.append({"op": "patch-pkg", "erp": DATABASES, "resource": OVERRIDES, "fragment": 0, "edits": edits})
    (ROOT / "erp/tyre_icons.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")
    template, cells = glyph_cells
    rows.insert(0, ("HUD icon font glyphs    icons.font  44x44", [("~ (Soft)", template)], cells))
    if sheet:
        contact_sheet(rows, letters, sheet)
    return report


def contact_sheet(rows, letters, out):
    font = ImageFont.truetype(io.BytesIO(letters["font"]), 15)
    small = ImageFont.truetype(io.BytesIO(letters["font"]), 12)
    pad = 10
    laid = []
    for label, refs, cells in rows:
        w, h = cells[0][1].size
        scale = 4 if max(w, h) <= 32 else 3 if max(w, h) <= 48 else 1
        laid.append((label, refs, cells, scale, w * scale, h * scale))
    width = max(pad + (len(r) + len(c)) * (cw + pad) + 20 for _, r, c, _, cw, _ in laid)
    height = pad + sum(22 + ch + 22 + pad for *_, ch in laid)
    sheet = Image.new("RGBA", (max(width, 600), height), (40, 40, 46, 255))
    d = ImageDraw.Draw(sheet)
    y = pad
    for label, refs, cells, scale, cw, ch in laid:
        d.text((pad, y), label, fill=(255, 255, 255, 255), font=font)
        y += 22
        x = pad
        for i, (name, im) in enumerate(refs + cells):
            if i == len(refs):
                x += 20
            sheet.alpha_composite(im.resize((cw, ch), Image.NEAREST if scale > 1 else Image.LANCZOS), (x, y))
            d.text((x, y + ch + 3), name, fill=(170, 170, 170, 255) if i < len(refs) else (255, 220, 120, 255),
                   font=small)
            x += cw + pad
        y += ch + 22 + pad
    sheet.convert("RGB").save(out)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path, help="launcher baseline folder holding baseline.json (stock copies)")
    ap.add_argument("--sheet", type=Path, help="write a preview PNG: stock references, then the 1990 icons")
    a = ap.parse_args(argv[1:])
    for line in build(a.game, a.baseline, a.sheet):
        print(line)


if __name__ == "__main__":
    main(sys.argv)
