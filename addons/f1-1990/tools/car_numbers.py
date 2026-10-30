#!/usr/bin/env python3
"""Writes erp/numbers.json and erp/numbers/numbers.erpk: Choose Driver car numbers for the 1990 numbers stock lacks.

The driver card number is the slice `nr_<car number>` of overrides.drivers.driver_numbers (driver_numbers_26 for a
driver in a 2026 car); without a slice the game hides the number. Stock has slices for the numbers of the 2025 and
earlier drivers only, so 1990 numbers such as Berger's 28 show nothing. This grows the array by one slice per 1990
car number that has none, drawn like the stock slices: the number in Formula 1 TV Bold (the game's font), centred in
the 80x80 slice at the stock height, with a top-lit gradient in the team's colour (its second colour when the first is
too dark for the card). Stock F2 cards show no number, and no stock F1 card carries these numbers.

usage: car_numbers.py --game <F1 25> [--baseline <launcher baseline>/<build>] [--sheet <png>]
"""
import argparse
import io
import json
import struct
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/rnd-tyres-thermal/tools")]

import bc7enc  # noqa: E402
import grid  # noqa: E402
from common import bc7_decode  # noqa: E402
from erpw import Erp  # noqa: E402
from pkg import Pkg  # noqa: E402
from season import Season  # noqa: E402
from stock import Stock, fragments  # noqa: E402
from tyre_icons import carriers, letter_font, overrides_node  # noqa: E402

UI = "2025_asset_groups/ui_package/"
DATABASES = UI + "databases.erp"
OVERRIDES = "eaid://ui_package/databases/overrides.uidb"
LIST = "drivers.driver_numbers"
ARRAY = f"eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.{LIST}.texarray"
TEXTURE = "ui_package/art/textures/f1990/driver_numbers/{}.tif"
OUT = ROOT / "erp/numbers"
BC7 = 70
HEIGHT, MAX_WIDTH, CENTRE = 46, 70, (40, 40)
TOP_LIGHT, BOTTOM_SHADE = 0.32, 0.9
MIN_LUMINANCE = 0.08
CARD = (88, 14, 22)


def luminance(hex_colour):
    c = np.array([int(hex_colour[i:i + 2], 16) for i in (1, 3, 5)], dtype=np.float64) / 255
    c = np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)
    return float(c @ [0.2126, 0.7152, 0.0722])


def colour_of(team):
    colours = grid.TEAM_INFO[team][3]
    base = grid.UI_COLOUR.get(team, colours[0])
    if base.upper() == "#FFFFFF" or luminance(base) < MIN_LUMINANCE:
        base = colours[1]
    return tuple(int(base[i:i + 2], 16) for i in (1, 3, 5))


def seats():
    lineup, free, team_of, number_of, _ = grid.lineup_of(Season.load())
    drivers = [seat.driver for _, _, seat in lineup] + free
    team_of_number = {}
    for did in drivers:
        team = team_of[did].team
        if team_of_number.setdefault(number_of[did], team) != team:
            sys.exit(f"car number {number_of[did]} is on two teams")
    return [(did, number_of[did], team_of[did].team) for did in drivers], team_of_number


def coverage(text, font_bytes, size, scale=8):
    probe = ImageFont.truetype(io.BytesIO(font_bytes), 200)
    box = probe.getbbox("0123456789")
    font = ImageFont.truetype(io.BytesIO(font_bytes), max(1, round(200 * HEIGHT * scale / (box[3] - box[1]))))
    tb = font.getbbox(text)
    tw, th = tb[2] - tb[0], tb[3] - tb[1]
    glyphs = Image.new("L", (tw + 8, th + 8), 0)
    ImageDraw.Draw(glyphs).text((4 - tb[0], 4 - tb[1]), text, font=font, fill=255)
    width = min(tw, MAX_WIDTH * scale)
    glyphs = glyphs.resize((width + 8, th + 8), Image.LANCZOS)
    w, h = size
    big = Image.new("L", (w * scale, h * scale), 0)
    big.paste(glyphs, (round(CENTRE[0] * scale - glyphs.width / 2), round(CENTRE[1] * scale - glyphs.height / 2)))
    return np.asarray(big.resize((w, h), Image.BOX), dtype=np.float32) / 255


def render(number, colour, font_bytes, w, h):
    a = coverage(str(number), font_bytes, (w, h))
    ys = np.nonzero(a.max(1) > 0.02)[0]
    t = np.clip((np.arange(h) - ys.min()) / max(1, ys.max() - ys.min()), 0, 1)[:, None, None]
    base = np.array(colour, dtype=np.float32)[None, None, :]
    top = base + (255 - base) * TOP_LIGHT
    rgb = top * (1 - t) + base * BOTTOM_SHADE * t
    rgba = np.concatenate([np.broadcast_to(rgb, (h, w, 3)), a[..., None] * 255], -1)
    return Image.fromarray(np.clip(np.rint(rgba), 0, 255).astype(np.uint8), "RGBA")


def build(game, baseline, sheet):
    stock = Stock(game, baseline)
    root = Pkg(fragments(stock.path(DATABASES), OVERRIDES)[0]).root
    names = [e["Name"].value for e in overrides_node(root, LIST)["Contents"].items]
    image, view = ARRAY + ".image", ARRAY
    where = carriers(game, [image, view])
    if not where[image] or where[image] != where[view]:
        sys.exit(f"{LIST}: carriers {where[image]} / {where[view]}")
    frags = None
    for rel in where[image]:
        got = fragments(stock.path(rel), image) + fragments(stock.path(rel), view)
        if frags is not None and got != frags:
            sys.exit(f"{LIST}: {rel} carries a different stock array")
        frags = got
    head, data, view0 = frags
    fmt, w, h = struct.unpack_from("<III", head, 8)
    mips, count = struct.unpack_from("<II", head, 24)
    if fmt != BC7 or mips != 1 or len(data) != count * w * h or len(names) != count \
            or struct.unpack_from("<I", view0, 0x14)[0] != count:
        sys.exit(f"{LIST}: unexpected stock array (format {fmt}, {w}x{h}, {mips} mips, {count} slices)")

    drivers, team_of_number = seats()
    new = sorted(n for n in team_of_number if f"nr_{n}" not in names)
    font_bytes = letter_font(stock)
    slices = {n: render(n, colour_of(team_of_number[n]), font_bytes, w, h) for n in new}
    encoded = {n: bc7enc.encode_rgba(im.tobytes(), w, h) for n, im in slices.items()}

    head2, view2 = bytearray(head), bytearray(view0)
    struct.pack_into("<I", head2, 28, count + len(new))
    struct.pack_into("<I", view2, 0x14, count + len(new))
    data2 = data + b"".join(encoded[n] for n in new)

    if OUT.exists():
        for p in OUT.iterdir():
            p.unlink()
    OUT.mkdir(parents=True, exist_ok=True)
    e = Erp(str(stock.path(where[image][0])))
    r_image, r_view = e.find(image), e.find(view)
    e.set_frag(r_image, 0, bytes(head2))
    e.set_frag(r_image, 1, data2)
    e.set_frag(r_view, 0, bytes(view2))
    e.res = [r_image, r_view]
    e.to_file(str(OUT / "numbers.erpk"))

    ops = [{"op": "put-resource", "erp": rel, "resource": res, "source": "erp/numbers/numbers.erpk"}
           for rel in where[image] for res in (image, view)]
    at = "overrides.Contents." + ".Contents.".join(LIST.split(".")) + ".Contents"
    edits = [{"op": "clone-entry", "at": at, "src": f"[Name={names[-1]}]", "as": f"nr_{n}",
              "set": {"Value": TEXTURE.format(n)}} for n in new]
    ops.append({"op": "patch-pkg", "erp": DATABASES, "resource": OVERRIDES, "fragment": 0, "edits": edits})
    (ROOT / "erp/numbers.json").write_text(json.dumps({"schema_version": 1, "ops": ops}, indent=1) + "\n")

    keys = names + [f"nr_{n}" for n in new]
    missing = [(did, n) for did, n, _ in drivers if f"nr_{n}" not in keys]
    if missing:
        sys.exit(f"numbers without a slice: {missing}")
    if sheet:
        decoded = {k: Image.frombytes("RGBA", (w, h), bc7_decode(data2[i * w * h:(i + 1) * w * h], w, h))
                   for i, k in enumerate(keys)}
        contact_sheet(drivers, decoded, set(new), sheet, font_bytes)
    return [f"{LIST}: {w}x{h}, {count} + {len(new)} slices ({', '.join(map(str, new))}), "
            f"{len(where[image])} archive(s): {', '.join(where[image])}",
            f"{len(drivers)} 1990 drivers, every number resolves"]


def contact_sheet(drivers, decoded, new, out, font_bytes):
    font, small = (ImageFont.truetype(io.BytesIO(font_bytes), size) for size in (15, 12))
    cols, cw, ch, pad = 6, 230, 150, 10
    rows = (len(drivers) + cols - 1) // cols
    sheet = Image.new("RGBA", (pad + cols * (cw + pad), pad + rows * (ch + pad)), (40, 40, 46, 255))
    d = ImageDraw.Draw(sheet)
    for i, (did, n, team) in enumerate(drivers):
        x, y = pad + (i % cols) * (cw + pad), pad + (i // cols) * (ch + pad)
        d.rectangle((x, y, x + cw, y + ch), fill=CARD + (255,))
        im = decoded[f"nr_{n}"]
        sheet.alpha_composite(im.resize((160, 160), Image.LANCZOS).crop((0, 20, 160, 140)), (x + 35, y + 4))
        d.text((x + 6, y + ch - 36), did.replace("_", " ").title(), fill=(255, 255, 255, 255), font=font)
        tag = "1990 slice" if n in new else "stock slice"
        d.text((x + 6, y + ch - 18), f"#{n}  {team}  nr_{n} ({tag})", fill=(225, 205, 205, 255), font=small)
    sheet.convert("RGB").save(out)


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--game", required=True, type=Path)
    ap.add_argument("--baseline", type=Path, help="launcher baseline folder holding baseline.json (stock copies)")
    ap.add_argument("--sheet", type=Path, help="write a sheet PNG: every 1990 driver with the slice of its number")
    a = ap.parse_args(argv[1:])
    for line in build(a.game, a.baseline, a.sheet):
        print(line)


if __name__ == "__main__":
    main(sys.argv)
