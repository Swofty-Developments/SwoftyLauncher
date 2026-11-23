#!/usr/bin/env python3
"""Write native/test/fonts/swofty-test-cff.otf: a small OpenType font with CFF (cubic) outlines for the overlay font tests.

  nix-shell -p "python3.withPackages(p: [p.fonttools])" --run "python3 native/tools/test_font_cff.py"

Its glyphs are drawn here: H, A with a hole, V, an O of cubic curves with a hole, a descending g, space and ?. The kern
feature moves V 80 units towards A.
"""
import os

from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.t2CharStringPen import T2CharStringPen

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "test", "fonts", "swofty-test-cff.otf")
K = 0.5523


def poly(pen, pts):
    pen.moveTo(pts[0])
    for p in pts[1:]:
        pen.lineTo(p)
    pen.closePath()


def ellipse(pen, cx, cy, rx, ry, clockwise):
    arcs = [((cx + rx, cy), (cx + rx, cy + ry * K), (cx + rx * K, cy + ry), (cx, cy + ry)),
            ((cx, cy + ry), (cx - rx * K, cy + ry), (cx - rx, cy + ry * K), (cx - rx, cy)),
            ((cx - rx, cy), (cx - rx, cy - ry * K), (cx - rx * K, cy - ry), (cx, cy - ry)),
            ((cx, cy - ry), (cx + rx * K, cy - ry), (cx + rx, cy - ry * K), (cx + rx, cy))]
    if clockwise:
        arcs = [(d, c, b, a) for a, b, c, d in reversed(arcs)]
    pen.moveTo(arcs[0][0])
    for _, c1, c2, end in arcs:
        pen.curveTo(c1, c2, end)
    pen.closePath()


def glyphs():
    out = {}

    def draw(name, width, fn):
        pen = T2CharStringPen(width, None)
        fn(pen)
        out[name] = (pen.getCharString(), width)

    draw(".notdef", 500, lambda p: poly(p, [(50, 0), (450, 0), (450, 700), (50, 700)]))
    draw("space", 250, lambda p: None)
    draw("H", 640, lambda p: (poly(p, [(60, 0), (160, 0), (160, 300), (480, 300), (480, 0), (580, 0), (580, 700), (480, 700),
                                       (480, 400), (160, 400), (160, 700), (60, 700)])))
    draw("A", 660, lambda p: (poly(p, [(20, 0), (640, 0), (330, 700)]), poly(p, [(250, 150), (330, 400), (410, 150)])))
    draw("V", 660, lambda p: poly(p, [(20, 700), (130, 700), (330, 150), (530, 700), (640, 700), (380, 0), (280, 0)]))
    draw("O", 720, lambda p: (ellipse(p, 360, 350, 320, 360, False), ellipse(p, 360, 350, 200, 240, True)))
    draw("g", 560, lambda p: poly(p, [(60, -220), (500, -220), (500, 500), (60, 500), (60, 400), (400, 400), (400, -120),
                                       (60, -120)]))
    draw("question", 500, lambda p: (poly(p, [(200, 0), (300, 0), (300, 100), (200, 100)]),
                                      poly(p, [(200, 200), (300, 200), (300, 600), (60, 600), (60, 500), (200, 500)])))
    return out


def main():
    gl = glyphs()
    order = list(gl)
    fb = FontBuilder(1000, isTTF=False)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({0x20: "space", ord("H"): "H", ord("A"): "A", ord("V"): "V", ord("O"): "O", ord("g"): "g",
                          ord("?"): "question"})
    fb.setupCFF("SwoftyTestCFF", {"FullName": "Swofty Test CFF"}, {n: g[0] for n, g in gl.items()}, {})
    fb.setupHorizontalMetrics({n: (g[1], 0) for n, g in gl.items()})
    fb.setupHorizontalHeader(ascent=800, descent=-250)
    fb.setupNameTable({"familyName": "Swofty Test CFF", "styleName": "Regular", "fullName": "Swofty Test CFF"})
    fb.setupOS2(sTypoAscender=800, sTypoDescender=-250, usWinAscent=800, usWinDescent=250, sCapHeight=700, sxHeight=500)
    fb.setupPost()
    addOpenTypeFeaturesFromString(fb.font, "languagesystem DFLT dflt;\nfeature kern { pos A V -80; } kern;\n")
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    fb.save(OUT)
    print(OUT, os.path.getsize(OUT), "bytes")


if __name__ == "__main__":
    main()
