#!/usr/bin/env python3
"""Writes art/logos/<team>/lockup_light.png, lockup_dark.png and mark.png from the sources in art/logos/src.

lockup_light is the full logo for a light or team-colour card, lockup_dark the same for the dark UI, and mark the
compact symbol for square and small badges. Raster sources are traced to vectors per colour with potrace first.

usage: logo_art.py [--check]

Needs Pillow, numpy, resvg and potrace.
"""
import argparse
import io
import os
import re
import subprocess
import sys
import tempfile
from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ART = HERE.parent / "art/logos"
SRC = ART / "src"
LOCKUP_W = 1200
MARK_W = 600
WHITE = (255, 255, 255)


def hexrgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def svg_text(name, drop=(), replace=()):
    s = (SRC / name).read_text(encoding="utf-8")
    for i in drop:
        s = re.sub(r'<(path|rect)\b[^>]*\bid="%s"[^>]*/>' % re.escape(i), "", s, flags=re.S)
    for a, b in replace:
        s = re.sub(re.escape(a), b, s, flags=re.I)
    return s


def render(svg, width):
    with tempfile.TemporaryDirectory() as d:
        src, out = os.path.join(d, "in.svg"), os.path.join(d, "out.png")
        Path(src).write_text(svg, encoding="utf-8")
        subprocess.run(["resvg", "-w", str(width), src, out], check=True, capture_output=True)
        return Image.open(out).convert("RGBA").copy()


def trim(im, pad=2):
    box = im.getchannel("A").point(lambda v: 255 if v > 8 else 0).getbbox()
    if box is None:
        return im
    l, t, r, b = box
    return im.crop((max(0, l - pad), max(0, t - pad), min(im.width, r + pad), min(im.height, b + pad)))


def key_out(im, bg, lo, hi):
    a = np.asarray(im.convert("RGB")).astype(np.float32)
    bg = np.array(bg, np.float32)
    alpha = np.clip((np.sqrt(((a - bg) ** 2).sum(-1)) - lo) / (hi - lo), 0, 1)
    f = np.where(alpha > 0, alpha, 1)[..., None]
    rgb = np.clip((a - (1 - f) * bg) / f, 0, 255)
    return Image.fromarray(np.dstack([rgb, alpha * 255]).astype(np.uint8), "RGBA")


def recolour(im, mapping, tol=90):
    a = np.asarray(im).astype(np.float32)
    rgb = a[..., :3].copy()
    src = np.array([hexrgb(s) for s, _ in mapping], np.float32)
    dst = np.array([hexrgb(d) for _, d in mapping], np.float32)
    dist = np.sqrt(((rgb[..., None, :] - src[None, None]) ** 2).sum(-1))
    near = dist.min(-1) < tol
    rgb[near] = dst[dist.argmin(-1)[near]]
    return Image.fromarray(np.dstack([rgb, a[..., 3:]]).astype(np.uint8), "RGBA")


def to_white(im, *colours):
    return recolour(im, [(c, "#FFFFFF") for c in colours])


def trace(im, palette, scale=4, turd=4, drop=()):
    big = im.convert("RGBA").resize((im.width * scale, im.height * scale), Image.LANCZOS)
    a = np.asarray(big).astype(np.float32)
    pal = np.array([hexrgb(p) for p in palette], np.float32)
    idx = np.sqrt(((a[..., None, :3] - pal[None, None]) ** 2).sum(-1)).argmin(-1)
    solid = a[..., 3] > 127
    groups, box = [], None
    with tempfile.TemporaryDirectory() as tmp:
        for i, colour in enumerate(palette):
            mask = (idx == i) & solid
            if colour in drop or not mask.any():
                continue
            pbm, svg = os.path.join(tmp, f"{i}.pbm"), os.path.join(tmp, f"{i}.svg")
            Image.fromarray(np.where(mask, 0, 255).astype(np.uint8)).convert("1").save(pbm)
            subprocess.run(["potrace", "-s", "-t", str(turd), "-a", "1.0", "-O", "0.2", "--flat", "-C", colour,
                            "-o", svg, pbm], check=True)
            s = Path(svg).read_text()
            groups.append(re.search(r"(<g\b.*</g>)", s, re.S).group(1))
            box = re.search(r'viewBox="([^"]+)"', s).group(1)
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{box}">' + "".join(groups) + "</svg>"


def runs(mask):
    out, start = [], None
    for i, v in enumerate(mask):
        if v and start is None:
            start = i
        if not v and start is not None:
            out.append((start, i))
            start = None
    if start is not None:
        out.append((start, len(mask)))
    return out


def opaque(im, axis):
    return (np.asarray(im.getchannel("A")) > 128).any(axis)


def stack(top, bottom, gap):
    w = max(top.width, bottom.width)
    out = Image.new("RGBA", (w, top.height + gap + bottom.height), (0, 0, 0, 0))
    out.alpha_composite(top, ((w - top.width) // 2, 0))
    out.alpha_composite(bottom, ((w - bottom.width) // 2, top.height + gap))
    return out


@lru_cache(None)
def font(name):
    return subprocess.run(["fc-match", "-f", "%{file}", name], capture_output=True, text=True, check=True).stdout


def wordmark(text, colour, height, stretch=1.3):
    f = ImageFont.truetype(font("DejaVu Sans:style=Bold"), 400)
    l, t, r, b = f.getbbox(text)
    im = Image.new("RGBA", (r - l + 40, b - t + 40), (0, 0, 0, 0))
    ImageDraw.Draw(im).text((20 - l, 20 - t), text, font=f, fill=hexrgb(colour) + (255,))
    im = trim(im, 0)
    im = im.resize((round(im.width * stretch), im.height), Image.LANCZOS)
    return im.resize((round(im.width * height / im.height), height), Image.LANCZOS)


def mclaren():
    white = [(".st0{fill-rule:evenodd;clip-rule:evenodd;}", ".st0{fill-rule:evenodd;clip-rule:evenodd;fill:#FFFFFF;}")]
    return (render(svg_text("mclaren.svg"), LOCKUP_W), render(svg_text("mclaren.svg", replace=white), LOCKUP_W),
            render(svg_text("mclaren_icon.svg", replace=[(".s0 { fill: #000000 }", ".s0 { fill: #FFFFFF }")]), MARK_W))


def ferrari():
    shield = render(svg_text("ferrari.svg"), LOCKUP_W // 2)
    return shield, shield, shield


def williams():
    plate = ["path1029", "path1031"]
    words = render(svg_text("williams.svg", drop=plate), LOCKUP_W)
    plated = render(svg_text("williams.svg"), LOCKUP_W)
    return plated, to_white(words, "#192c4e"), plated


def benetton():
    dark = [('fill="#221F20"', 'fill="#FFFFFF"')]
    flags = [('fill="#221F20"', 'fill="none"')]
    return (render(svg_text("benetton.svg"), LOCKUP_W), render(svg_text("benetton.svg", replace=dark), LOCKUP_W),
            render(svg_text("benetton.svg", replace=flags), LOCKUP_W))


def tyrrell():
    light = render(svg_text("tyrrell.svg"), LOCKUP_W)
    return light, to_white(light, "#2e3192"), to_white(light, "#2e3192")


def lotus():
    roundel = trim(render(svg_text("lotus.svg"), MARK_W))
    navy, yellow = "#1F4E9C", "#F2C500"
    h = roundel.height
    words = stack(wordmark("CAMEL", navy, round(h * 0.3), 1.15), wordmark("TEAM LOTUS", navy, round(h * 0.2), 1.15),
                  round(h * 0.09))
    pad = round(h * 0.1)
    plate = Image.new("RGBA", (pad * 3 + roundel.width + words.width, h + pad * 2), hexrgb(yellow) + (255,))
    plate.alpha_composite(roundel, (pad, pad))
    plate.alpha_composite(words, (pad * 2 + roundel.width, pad + (h - words.height) // 2))
    return plate, plate, roundel


def leytonhouse():
    src = trim(Image.open(SRC / "leytonhouse.png").convert("RGBA"))
    palette = ["#10C8C4", "#222222", "#E8F8F8"]
    plate = render(trace(src, palette, scale=6), LOCKUP_W)
    ink = trim(render(trace(src, palette, scale=6, drop=("#10C8C4", "#E8F8F8")), LOCKUP_W))
    letters = runs(opaque(ink, 0))
    gaps = [letters[i + 1][0] - letters[i][1] for i in range(len(letters) - 1)]
    k = int(np.argmax(gaps))
    split = (letters[k][1] + letters[k + 1][0]) // 2
    leyton = trim(ink.crop((0, 0, split, ink.height)), 0)
    house = trim(ink.crop((split, 0, ink.width, ink.height)), 0)
    words = stack(leyton, house, round(leyton.height * 0.16))
    return plate, plate, recolour(words, [("#222222", "#10C8C4")], 200)


def larrousse():
    keyed = key_out(Image.open(SRC / "larrousse.jpg"), WHITE, 25, 80)
    logo = trim(render(trace(keyed, ["#C82828", "#38A858", "#F8F028", "#B8B8B8"]), LOCKUP_W))
    return logo, logo, logo


def ligier():
    t = trim(render(svg_text("ligier.svg"), LOCKUP_W))
    bands = runs(opaque(t, 1))
    flags = trim(t.crop((0, bands[0][0], t.width, bands[0][1])))
    word = trim(t.crop((0, bands[1][0], t.width, bands[1][1])))
    gap = round(word.height * 0.35)
    return stack(flags, word, gap), stack(flags, to_white(word, "#132451"), gap), flags


def minardi():
    shield = render(svg_text("minardi.svg"), LOCKUP_W // 2)
    return shield, shield, shield


def brabham():
    src = Image.open(SRC / "brabham.gif").convert("RGBA")
    b = src.crop((0, 0, src.width, round(src.height * 0.74)))
    light_b = trim(render(trace(b, ["#182868", "#4898D8", "#FFFFFF"], scale=8, turd=8, drop=("#FFFFFF",)), MARK_W))
    dark_b = recolour(light_b, [("#182868", "#FFFFFF")], 80)
    h = round(light_b.height * 0.22)
    return (stack(light_b, wordmark("BRABHAM", "#182868", h), round(h * 0.45)),
            stack(dark_b, wordmark("BRABHAM", "#FFFFFF", h), round(h * 0.45)), dark_b)


def dallara():
    src = Image.open(SRC / "dallara.jpg").convert("RGB")
    bg = tuple(int(v) for v in np.median(np.asarray(src)[:6].reshape(-1, 3), 0))
    light = render(trace(key_out(src, bg, 25, 75), ["#182888", "#2C9A48", "#D02C3C"]), LOCKUP_W)
    bms = trim(key_out(Image.open(SRC / "dallara_bms.gif"), WHITE, 30, 90))
    bms = bms.crop((0, 0, bms.width, round(bms.height * 0.58)))
    mark = render(trace(bms, ["#606060", "#08A850", "#F80808"], scale=8, turd=6), MARK_W)
    return light, recolour(light, [("#182888", "#FFFFFF")], 80), recolour(mark, [("#606060", "#FFFFFF")], 80)


def plated(im, pad):
    im = trim(im, 0)
    p = round(max(im.width, im.height) * pad)
    out = Image.new("RGBA", (im.width + 2 * p, im.height + 2 * p), WHITE + (255,))
    out.alpha_composite(im, (p, p))
    return out


def arrows():
    logo = trim(Image.open(SRC / "arrows.png").convert("RGBA"), 0)
    bands = runs(opaque(logo, 1))
    a = max(bands, key=lambda band: band[1] - band[0])
    symbol = logo.crop((0, a[0], logo.width, a[1]))
    return logo, plated(logo, 0.06), plated(symbol, 0.08)


TEAMS = {f.__name__: f for f in (mclaren, ferrari, williams, benetton, tyrrell, lotus, leytonhouse, larrousse,
                                  ligier, minardi, brabham, dallara, arrows)}
KINDS = ("lockup_light", "lockup_dark", "mark")


def fit_width(im, width):
    im = trim(im)
    if im.width <= width:
        return im
    return im.resize((width, max(1, round(im.height * width / im.width))), Image.LANCZOS)


def png(im):
    b = io.BytesIO()
    im.save(b, "PNG", optimize=True)
    return b.getvalue()


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="exit 1 when a shipped file differs from a fresh build")
    a = ap.parse_args(argv[1:])
    stale = []
    for team, make in TEAMS.items():
        for kind, im in zip(KINDS, make()):
            data = png(fit_width(im, MARK_W if kind == "mark" else LOCKUP_W))
            path = ART / team / f"{kind}.png"
            if a.check:
                if not path.exists() or path.read_bytes() != data:
                    stale.append(str(path.relative_to(ART)))
                continue
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        print(team)
    if stale:
        sys.exit("stale: " + ", ".join(stale))


if __name__ == "__main__":
    main(sys.argv)
