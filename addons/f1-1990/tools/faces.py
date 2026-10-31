#!/usr/bin/env python3
"""Real faces for the 1990 drivers: a face zip (<driver>/faces_entry.json plus <driver>_head_d.tga.mipmaps) becomes a
driver head that only the driver's 1990 row wears.

usage: faces.py [<driver>.zip ...] [--base [<driver>=]<head> ...] [--game <F1 25>] [--baseline <launcher baseline>/<build>]
       faces.py --bases [--game <F1 25>]

Each zip is recorded in faces/faces.json and its face map copied to faces/src/. Then every face of faces/faces.json is
built: both archives of `base`, a stock scanned head under character_package/heads/male_driver/, read from the game
folder (the baseline's copy of a file the launcher changed), with `base` renamed byte for byte to `code` (the same
length) in every resource name and fragment, and the head_d surface replaced by the face map (BC1 levels 2048 to 256,
to which the tool adds 128 to 4) with all 10 levels in the archive and no sidecar. The archives go to
faces/heads/<label>.erp and faces/morph_poses/<label>.erp (label f90_<driver>), faces/assets.json puts them and
registers their asset groups (lando_norris's as templates), and db/faces.json adds the head row (a copy of male_ls222
with the archive's model_path) and sets the driver's 1990 row (f1990_<driver>) to wear it.

A zip's `base` and `code` may still be the template's <...> placeholders: the base is then --base (<driver>=<head> for
one driver, <head> for every zip), the base faces.json already records for the driver, or felipe_massa; the code is
f90_<driver> cut or padded with _ to the base's length.
--bases lists the scanned heads whose hair is painted from the face map (no <head>_hair.idf hair mesh).

faces/src/<driver>_sheet.png shows the base's face map, the new one and the zip's preview.
"""

import argparse
import json
import re
import shutil
import struct
import sys
import zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path[:0] = [str(HERE), str(REPO / "tools"), str(REPO / "addons/classicseasons/classic-seasons/tools")]

import bcn  # noqa: E402
import erpw  # noqa: E402
import grid  # noqa: E402
import carperf  # noqa: E402
import heads as cs  # noqa: E402
from stock import Stock  # noqa: E402

FACES = ROOT / "faces"
DATA = FACES / "faces.json"
SRC = FACES / "src"
ASSETS = FACES / "assets.json"
DB = ROOT / "db" / "faces.json"
SCENES = "animation_package/scenes/cinematics/drivers"
KINDS = ("heads", "morph_poses")
HEADS = "entrants.animation.driver_heads"
STAT_IDS = json.loads((HERE / "stat_ids.json").read_text())[HEADS]
ROW_TEMPLATE = "male_ls222"
GROUP_TEMPLATE = "lando_norris"
DEFAULT_BASE = "felipe_massa"
BASELINE = "~/.local/share/swofty/baseline/25251167-d873a1c8d204"
GAME = "/mnt/gaming/SteamLibrary/steamapps/common/F1 25"
LOD_SHIFT = 2
LASH_LOD = 1
LASH_KEEP = 3
LEVELS = cs.FACE_LEVELS
FORMAT = cs.FACE_FORMAT


def label(driver):
    return f"f90_{driver}"


def placeholder(v):
    return not isinstance(v, str) or not v or v.startswith("<")


def fit(text, n):
    return text[:n] if len(text) >= n else text + "_" * (n - len(text))


def archive(kind, name):
    return f"2025_asset_groups/{SCENES}/{kind}/{name}.erp"


def scanned(stock, head):
    rel = archive("heads", head)
    if not stock.path(rel).is_file():
        return False
    return any(f"/heads/male_driver/{head}/" in r.name for r in erpw.Erp(str(stock.path(rel))).res)


def load():
    return json.loads(DATA.read_text()) if DATA.is_file() else {}


def save(faces):
    DATA.parent.mkdir(parents=True, exist_ok=True)
    DATA.write_text(json.dumps(dict(sorted(faces.items())), indent=1) + "\n")


def import_zip(path, faces, base, stock):
    with zipfile.ZipFile(path) as z:
        entries = [n for n in z.namelist() if n.endswith("faces_entry.json")]
        if len(entries) != 1:
            sys.exit(f"{path}: {len(entries)} faces_entry.json files")
        folder = entries[0][: -len("faces_entry.json")]
        entry = json.loads(z.read(entries[0]))
        out = []
        for driver, e in entry.items():
            if driver not in grid.DRIVER_INFO:
                sys.exit(f"{path}: {driver} is not a 1990 driver (tools/grid.py DRIVER_INFO)")
            old = faces.get(driver, {})
            b = base.get(driver) or base.get(None) or (None if placeholder(e.get("base")) else e["base"]) or old.get("base") or DEFAULT_BASE
            if not scanned(stock, b):
                sys.exit(f"{driver}: {b} is not a scanned head under heads/male_driver/")
            code = e.get("code")
            if placeholder(code) or len(code) != len(b):
                code = fit(label(driver), len(b))
            mip = e["mipmaps"]
            name = mip if mip in z.namelist() else folder + mip.split("/")[-1]
            SRC.mkdir(parents=True, exist_ok=True)
            (SRC / f"{driver}_head_d.tga.mipmaps").write_bytes(z.read(name))
            for n in z.namelist():
                if n.endswith("_preview.jpg") and n.startswith(folder):
                    (SRC / f"{driver}_preview.jpg").write_bytes(z.read(n))
            faces[driver] = {"source": Path(path).name, "mipmaps": f"src/{driver}_head_d.tga.mipmaps", "base": b,
                             "code": code, "label": label(driver)}
            out.append(driver)
    return out


def chain(raw, driver):
    sizes = bcn.level_sizes(FORMAT, 2048, 2048, 4)
    if len(raw) != sum(sizes):
        sys.exit(f"{driver}: the face map is {len(raw)} bytes, not BC1 levels 2048 to 256")
    level = bcn.decode(FORMAT, raw[sum(sizes[:3]):], 256, 256)
    out = [raw]
    while level.shape[0] > 4:
        level = bcn.downsample(level)
        out.append(bcn.encode(FORMAT, level))
    return b"".join(out)


def lighten(e):
    groups = {}
    for m in carperf.meshes(e):
        if m.lod >= 0:
            groups.setdefault((m.base, m.part), {})[m.lod] = m
    for lods in groups.values():
        top = max(lods)
        if top < 1 or sorted(lods) != list(range(top + 1)):
            continue
        shift = min(LOD_SHIFT, top)
        source = {k: (bytes(m.d), m.names) for k, m in lods.items()}
        for k, m in lods.items():
            data, names = source[min(k + shift, top)]
            d = bytearray(data)
            struct.pack_into("<i", d, 72, k)
            if k >= LASH_LOD:
                for i, name in enumerate(names):
                    at = carperf.MESH_HEADER + i * carperf.MESH_RECORD + 16
                    if name == "lashes" and struct.unpack_from("<I", d, at)[0] > LASH_KEEP:
                        struct.pack_into("<I", d, at, LASH_KEEP)
            e.set_frag(m.r, 0, bytes(d))


def build(stock, driver, face):
    base, code = face["base"].encode(), face["code"].encode()
    if len(base) != len(code):
        sys.exit(f"{driver}: code {face['code']} is not as long as {face['base']}")
    data = chain((FACES / face["mipmaps"]).read_bytes(), driver)
    placed = 0
    for kind in KINDS:
        e = erpw.Erp(str(stock.path(archive(kind, face["base"]))))
        if any(code in r.name.encode() for r in e.res):
            sys.exit(f"{driver}: {face['base']} already names {face['code']}")
        cs.rename(e, base, code)
        lighten(e)
        for i, r in enumerate(e.res):
            if r.name.endswith(f"/{face['code']}_head_d.tif.image") and r.type.rstrip(b"\0") == cs.SURFACE:
                copy = cs.embedded_surface(r, data)
                copy.name = r.name
                e.res[i] = copy
                placed += 1
            elif r.name.endswith(f"/{face['code']}_head_d.tif") and r.type.rstrip(b"\0") == cs.SRV:
                srv = bytearray(cs.frag(r, 0))
                struct.pack_into("<I", srv, 4, FORMAT)
                struct.pack_into("<I", srv, 12, LEVELS)
                cs.put_frag(r, 0, bytes(srv))
        out = FACES / kind / f"{face['label']}.erp"
        out.parent.mkdir(parents=True, exist_ok=True)
        e.to_file(str(out))
    if not placed:
        sys.exit(f"{driver}: {face['base']} has no head_d surface")
    return data


def leftovers(face, game):
    base = face["base"].encode()
    out = []
    for kind in KINDS:
        path = FACES / kind / f"{face['label']}.erp"
        e = erpw.Erp(str(path))
        for r in e.res:
            surface = r.type.rstrip(b"\0") == cs.SURFACE
            for i in range(len(r.frags)):
                data = cs.frag(r, i)
                if surface and i == 2 and data and data[0]:
                    side = data[1:1 + data[0]].rstrip(b"\0").decode()
                    if face["base"] in side and not (Path(game) / side).is_file():
                        out.append(side)
                elif base in data or base in r.name.encode():
                    out.append(r.name)
            if r.name.endswith(f"/{face['code']}_head_d.tif.image") and len(r.frags) != 2:
                out.append(f"{r.name} (a sidecar)")
        out += cs.broken_links(path)
    return sorted(set(out))


def model_path(face):
    e = erpw.Erp(str(FACES / "heads" / f"{face['label']}.erp"))
    models = [r.name for r in e.res if r.name.endswith(".idf?model") and "/kitpart_" not in r.name]
    want = f"eaid://character_package/heads/male_driver/{face['code']}/idf/{face['code']}_rig_head_morph_v1.idf?model"
    if models != [want]:
        sys.exit(f"{face['label']}: head models {models}")
    return want.replace("eaid://character_package/", "eaid://[CHARACTER_PACKAGE]/")[: -len(".idf?model")]


def gate(driver):
    for op in json.loads((ROOT / "db" / "grid.json").read_text())["ops"]:
        if op.get("op") == "add-row" and op.get("table") == grid.DRIVERS and \
                op["set"].get("_metadata_") == grid.driver_row(driver):
            return op.get("when")
    sys.exit(f"{driver}: db/grid.json adds no {grid.driver_row(driver)} row")


def write_ops(faces):
    assets, db = [], []
    for driver, face in sorted(faces.items()):
        for kind in KINDS:
            assets.append({"op": "file.put", "source": f"faces/{kind}/{face['label']}.erp",
                           "target": archive(kind, face["label"])})
            assets.append({"op": "group.register",
                           "group": f"asset_groups/{SCENES}/{kind}/{face['label']}.assetgroup",
                           "erp": archive(kind, face["label"]),
                           "template": f"asset_groups/{SCENES}/{kind}/{GROUP_TEMPLATE}.assetgroup"})
        when = gate(driver)
        ops = [
            {"op": "add-row", "table": HEADS, "from": f"{HEADS}.driver_head[_metadata_={ROW_TEMPLATE}]",
             "set": {"_metadata_": face["label"], "model_path": model_path(face), "has_morph_data": "True",
                     "available_for_customisation": "False", "stat_id": str(STAT_IDS[face["label"]])}},
            {"op": "set", "select": f"{grid.DRIVERS}.driver[_metadata_={grid.driver_row(driver)}]", "field": "head",
             "value": face["label"]},
        ]
        for op in ops:
            if when:
                op["when"] = when
        db += ops
    ASSETS.write_text(json.dumps({"schema_version": 1, "ops": assets}, indent=1) + "\n")
    DB.write_text(json.dumps({"schema_version": 1, "ops": db}, indent=1) + "\n")


def sheet(stock, driver, face, data, game):
    import head_sheet
    from PIL import Image, ImageDraw
    tile = 512
    _, before = head_sheet.face(stock.path(archive("heads", face["base"])), game)
    after = bcn.decode(FORMAT, data[: 2048 * 2048 // 2], 2048, 2048)
    tiles = [(f"{face['base']} (base)", Image.fromarray(before[..., :3]).resize((tile, tile))),
             (f"{face['label']} (code {face['code']})", Image.fromarray(after[..., :3]).resize((tile, tile)))]
    preview = SRC / f"{driver}_preview.jpg"
    if preview.is_file():
        p = Image.open(preview).convert("RGB")
        tiles.append(("zip preview", p.resize((round(p.width * tile / p.height), tile))))
    img = Image.new("RGB", (sum(t.width for _, t in tiles), tile + 24), "white")
    d, x = ImageDraw.Draw(img), 0
    for name, t in tiles:
        img.paste(t, (x, 24))
        d.text((x + 6, 6), name, fill="black")
        x += t.width
    out = SRC / f"{driver}_sheet.png"
    img.save(out)
    return out


def bases(stock):
    folder = Path(stock.game) / f"2025_asset_groups/{SCENES}/heads"
    for path in sorted(folder.glob("*.erp")):
        head = path.stem
        names = [r.name for r in erpw.Erp(str(stock.path(archive("heads", head)))).res]
        if not any(f"/heads/male_driver/{head}/" in n for n in names):
            continue
        morph = stock.path(archive("morph_poses", head))
        names += [r.name for r in erpw.Erp(str(morph)).res] if morph.is_file() else []
        if not any(re.search(r"_hair\.idf\?model$", n) for n in names):
            print(head)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("zips", nargs="*", help="face zips to add or replace (<driver>.zip)")
    ap.add_argument("--base", action="append", default=[],
                    help="the stock scanned head the zips' faces go on, or <driver>=<head>; repeat for more")
    ap.add_argument("--game", default=GAME)
    ap.add_argument("--baseline", default=BASELINE, help="launcher baseline folder holding baseline.json")
    ap.add_argument("--bases", action="store_true", help="list the scanned heads with no hair mesh")
    args = ap.parse_args()
    game = Path(args.game)
    if not (game / "F1_25.exe").is_file():
        sys.exit(f"--game {game} is not the F1 25 folder")
    baseline = Path(args.baseline).expanduser()
    stock = Stock(game, baseline if (baseline / "baseline.json").is_file() else None)
    if args.bases:
        bases(stock)
        return
    faces = load()
    base = {k if v else None: v or k for k, _, v in (b.partition("=") for b in args.base)}
    for z in args.zips:
        for driver in import_zip(Path(z).expanduser(), faces, base, stock):
            print(f"{driver}: from {Path(z).name}, base {faces[driver]['base']}, code {faces[driver]['code']}")
    save(faces)
    for kind in KINDS:
        shutil.rmtree(FACES / kind, ignore_errors=True)
    for driver, face in sorted(faces.items()):
        data = build(stock, driver, face)
        for kind in KINDS:
            relabelled = cs.relabel(FACES / kind / f"{face['label']}.erp")
            if relabelled:
                print(f"{driver}: resource version 2 for {', '.join(relabelled)}")
        left = leftovers(face, game)
        if left:
            sys.exit(f"{driver}: {face['base']} is still named in {', '.join(left[:3])}")
        print(f"{driver}: {face['base']} as {face['code']}, head {face['label']}, sheet {sheet(stock, driver, face, data, game)}")
    write_ops(faces)
    size = sum(p.stat().st_size for kind in KINDS for p in (FACES / kind).glob("*.erp"))
    print(f"faces: {len(faces)} head(s), {size / 1e6:.1f} MB; {ASSETS.relative_to(ROOT)}, {DB.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
