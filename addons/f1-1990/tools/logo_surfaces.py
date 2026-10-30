from functools import lru_cache
from pathlib import Path

import numpy as np
from PIL import Image, ImageFilter

ART = Path(__file__).resolve().parent.parent / "art/logos"
BASE = ("ferrari", "forceindia", "haas", "mclaren", "mercedes", "redbull", "renault", "sauber", "tororosso", "williams")

CARD = {
    "mclaren": ("#F2F2F2", "light"),
    "ferrari": ("#B3191F", "light"),
    "williams": ("#16427C", "light"),
    "benetton": ("#F2F2F2", "light"),
    "tyrrell": ("#1F2F6B", "dark"),
    "lotus": ("#F2C500", "light"),
    "leytonhouse": ("#10C8C4", "light"),
    "larrousse": ("#F2F2F2", "light"),
    "ligier": ("#0F5FDD", "dark"),
    "minardi": ("#F7AC15", "light"),
    "brabham": ("#2A3052", "dark"),
    "dallara": ("#F2F2F2", "light"),
    "arrows": ("#F2F2F2", "light"),
}
KEEP_LIGHT = {"ferrari", "lotus", "leytonhouse", "minardi"}
SMALL = {"larrousse": (130, 0, 470, 208), "ligier": (320, 0, 600, 174), "dallara": (195, 5, 395, 82)}


def rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def lum(c):
    return (0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]) / 255


@lru_cache(None)
def art(token, kind):
    return Image.open(ART / token / f"{kind}.png").convert("RGBA")


def fit(im, w, h):
    s = min(w / im.width, h / im.height)
    nw, nh = max(1, round(im.width * s)), max(1, round(im.height * s))
    if s >= 1:
        return im.resize((nw, nh), Image.LANCZOS)
    a = np.asarray(im).astype(np.float32) / 255
    pre = Image.fromarray((np.dstack([a[..., :3] * a[..., 3:], a[..., 3:]]) * 255).round().astype(np.uint8), "RGBA")
    b = np.asarray(pre.resize((nw, nh), Image.LANCZOS, reducing_gap=3.0)).astype(np.float32) / 255
    al = b[..., 3:]
    col = np.where(al > 0, b[..., :3] / np.maximum(al, 1e-6), 0)
    return Image.fromarray((np.dstack([np.clip(col, 0, 1), al]) * 255).round().astype(np.uint8), "RGBA")


def place(canvas, logo, cx, cy):
    canvas.alpha_composite(logo, (round(cx - logo.width / 2), round(cy - logo.height / 2)))
    return canvas


def box_of(mask):
    ys, xs = np.where(mask)
    return xs.min(), ys.min(), xs.max() + 1, ys.max() + 1


def alpha_box(im):
    a = np.asarray(im.getchannel("A")) > 8
    return box_of(a) if a.any() else None


def white_box(im):
    a = np.asarray(im).astype(np.int32)
    return box_of((a[..., :3].min(-1) > 200) & (a[..., 3] > 128))


def colour_box(im):
    a = np.asarray(im.convert("RGB")).astype(np.int32)
    return box_of(np.abs(a - a[2, 2]).sum(-1) > 60)


def median_box(slices):
    boxes = [b for b in map(alpha_box, slices) if b]
    med = lambda f: float(np.median([f(b) for b in boxes]))
    return med(lambda b: b[2] - b[0]), med(lambda b: b[3] - b[1])


def profile(im, box, axis):
    a = np.asarray(im).astype(np.float32)
    keep = np.ones(a.shape[:2], bool)
    l, t, r, b = box
    keep[max(0, t - 6):b + 6, max(0, l - 6):r + 6] = False
    out = np.empty_like(a)
    if axis == "y":
        for y in range(a.shape[0]):
            out[y] = np.median(a[y][keep[y]] if keep[y].any() else a[y], 0)
    else:
        for x in range(a.shape[1]):
            out[:, x] = np.median(a[:, x][keep[:, x]] if keep[:, x].any() else a[:, x], 0)
    return Image.fromarray(np.clip(out, 0, 255).astype(np.uint8), "RGBA")


def fill(im, box, grow=14, radius=24):
    l, t, r, b = box
    mask = Image.new("L", im.size, 255)
    mask.paste(0, (l - grow, t - grow, r + grow, b + grow))
    a = np.asarray(im).astype(np.float32)
    m = np.asarray(mask).astype(np.float32) / 255
    blur = lambda x: np.asarray(Image.fromarray(x.astype(np.uint8)).filter(ImageFilter.GaussianBlur(radius)))
    den = blur(m * 255).astype(np.float32) / 255
    num = np.dstack([blur(a[..., c] * m) for c in range(4)]).astype(np.float32)
    filled = num / np.maximum(den, 1e-3)[..., None]
    soft = np.asarray(mask.filter(ImageFilter.GaussianBlur(grow))).astype(np.float32)[..., None] / 255
    out = a * soft + filled * (1 - soft)
    return Image.fromarray(np.clip(out, 0, 255).astype(np.uint8), "RGBA")


def tint(im, colour):
    a = np.asarray(im).astype(np.float32)
    lumin = (0.299 * a[..., 0] + 0.587 * a[..., 1] + 0.114 * a[..., 2]) / 255
    solid = a[..., 3] > 128
    ref = np.percentile(lumin[solid], 92) if solid.any() else 1
    shade = np.clip(lumin / max(ref, 1e-3), 0, 1.3)[..., None]
    col = np.array(colour, np.float32) * shade
    col += (255 - col) * np.clip(shade - 1, 0, 1)
    return Image.fromarray(np.dstack([np.clip(col, 0, 255), a[..., 3:]]).astype(np.uint8), "RGBA")


def pill(im, colour):
    a = np.asarray(im).astype(np.float32)
    lumin = 0.299 * a[..., 0] + 0.587 * a[..., 1] + 0.114 * a[..., 2]
    base = a[a.shape[0] // 2, -4, :3]
    lb = 0.299 * base[0] + 0.587 * base[1] + 0.114 * base[2]
    w = np.clip((lumin - lb) / max(np.percentile(lumin, 99.5) - lb, 1), 0, 1)[..., None]
    c = np.array(colour, np.float32)
    target = np.where(w > 0.85, c + (255 - c) * 0.35, c)
    out = np.clip(base + w * (target - base), 0, 255)
    return Image.fromarray(np.dstack([out, a[..., 3:]]).astype(np.uint8), "RGBA")


def on_colour(token, out, logo_box, centre):
    x, y = centre
    px = np.asarray(out)[min(round(y), out.height - 1), min(round(x), out.width - 1)].astype(np.float32)
    local = lum(px[:3]) * px[3] / 255
    kind = "lockup_light" if local > 0.55 or token in KEEP_LIGHT else (
        "lockup_" + CARD[token][1] if local > 0.3 else "lockup_dark")
    return place(out, fit(art(token, kind), *logo_box), x, y)


class Surface:
    def __init__(self, style, stock):
        self.style = style
        self.stock = stock
        self.template = getattr(self, "prepare_" + style, lambda: None)()

    def sample(self):
        s = [im for k, im in self.stock.items() if k.endswith("_26")]
        return s or [im for k, im in self.stock.items() if k in BASE]

    def ref(self, *keys):
        return next(self.stock[k] for k in keys if k in self.stock)

    def prepare_mark(self):
        return median_box(self.sample())

    prepare_lockup = prepare_mark

    def prepare_card(self):
        return colour_box(self.ref("williams_26"))

    def prepare_contract(self):
        return white_box(self.ref("williams_26", "williams"))

    def prepare_fade(self):
        st = self.ref("williams_26", "williams")
        box = white_box(st)
        return profile(st, box, "y"), box

    prepare_mp = prepare_fade

    def prepare_holding(self):
        st = self.ref("williams_26", "williams")
        box = white_box(st)
        return profile(st, box, "x"), box

    def prepare_room(self):
        st = self.ref("williams_26")
        box = white_box(st)
        return fill(st, box), box

    def render(self, token, primary, w, h):
        return getattr(self, "render_" + self.style)(token, primary, w, h)

    def render_mark(self, token, primary, w, h):
        side = max(self.template)
        logo = art(token, "mark")
        if max(w, h) <= 32 and token in SMALL:
            logo = logo.crop(SMALL[token])
        return place(Image.new("RGBA", (w, h), (0, 0, 0, 0)), fit(logo, min(w * 0.96, side), min(h * 0.96, side)),
                     w / 2, h / 2)

    def render_menu(self, token, primary, w, h):
        return place(Image.new("RGBA", (w, h), (0, 0, 0, 0)), fit(art(token, "lockup_dark"), w * 0.97, h * 0.8),
                     w / 2, h / 2)

    def render_lockup(self, token, primary, w, h):
        bw, bh = self.template
        return place(Image.new("RGBA", (w, h), (0, 0, 0, 0)), fit(art(token, "lockup_dark"), bw * 1.1, bh * 1.1),
                     w / 2, h / 2)

    def render_card(self, token, primary, w, h):
        l, t, r, b = self.template
        out = Image.new("RGBA", (w, h), rgb(CARD[token][0]) + (255,))
        return place(out, fit(art(token, "lockup_" + CARD[token][1]), (r - l) * 1.05, (b - t) * 1.15),
                     (l + r) / 2, (t + b) / 2)

    def render_contract(self, token, primary, w, h):
        l, t, r, b = self.template
        out = Image.new("RGBA", (w, h), rgb(CARD[token][0]) + (255,))
        return place(out, fit(art(token, "lockup_" + CARD[token][1]), (r - l) * 1.05, h * 0.6), (l + r) / 2, h / 2)

    def render_fade(self, token, primary, w, h):
        bg, (l, t, r, b) = self.template
        box, centre = ((r - l) * 1.05, (b - t) * 1.2), ((l + r) / 2, (t + b) / 2)
        return on_colour(token, tint(bg, rgb(CARD[token][0])), box, centre)

    render_mp = render_fade
    render_holding = render_fade

    def render_room(self, token, primary, w, h):
        bg, (l, t, r, b) = self.template
        return place(bg.copy(), fit(art(token, "lockup_dark"), (r - l) * 1.15, (b - t) * 1.3), (l + r) / 2, (t + b) / 2)

    def render_tint(self, token, primary, w, h):
        return tint(self.ref("williams"), rgb(CARD[token][0]))

    def render_pill(self, token, primary, w, h):
        return pill(self.ref("williams"), rgb(primary))

    def render_blank(self, token, primary, w, h):
        return self.ref("williams_26", "laptop_williams_logo", "team_select_williams").copy()
