#!/usr/bin/env python3
"""Build each season's in-car driver archive from fanfreluche's zips.

The zips replace animation_package/scenes/incardriver/2025/incardriver_f1.erp, whose scene maps every car's steering
animation (driver_in_car_<year>_<team>.idf, shipped in the same archive). Each season keeps its own copy under
incardriver/cs<yy>/: the scene `eaid://animation_package/scenes/incardriver/2025/` is renamed byte for byte to
`.../incardriver/cs<yy>/`, which the season's driver animation set loads. Writes the season addon's
cars/incardriver/incardriver_f1.erp.

The archive also carries the drivers' bodies, heads and suits under stock character package names, in the format of
the game each season comes from (F1 2016 surfaces are version 4, F1 2017 version 7, F1 2018 version 8), so the
seasons ship different resources under one name. A game process that loaded the 2016 copy of
body_template/male/textures/male_template_a.tga.image crashed loading the 2017 copy (ACCESS_VIOLATION reading 0xa1 at
F1_25.exe+0x3bed92b). Every character package resource the archive defines moves to a season folder of the same
length (the first folder under the package ends in cs<yy>: body_template -> body_tempcs17), and every eaid reference
to it inside the archive follows. References to resources the archive does not define, and .mipmaps sidecar names,
stay as they are.
"""

import argparse
import bisect
import hashlib
import os
import re
import sys
import tempfile
import zipfile
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT.parents[2] / "tools"))
sys.path.insert(0, str(Path(__file__).resolve().parent))

import erpw
from build_cars import find_zip, rewrite
from layout import cars_dir

OLD = b"eaid://animation_package/scenes/incardriver/2025/"
MEMBER = "2025_asset_groups/animation_package/scenes/incardriver/2025/incardriver_f1.erp"
CHARACTER = re.compile(rb"eaid://(character_package(?:_shared)?)/([a-z0-9_]+)/([^\x00]*)")
ZLIB = 0x01
ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"


def folder(year):
    return f"cs{year % 100:02d}"


def season_folder(name, year):
    tag = folder(year).encode()
    if len(name) <= len(tag):
        raise SystemExit(f"{year}: character folder {name.decode()} is too short for {tag.decode()}")
    return name[:len(name) - len(tag)] + tag


def frag_bytes(r, i):
    fr = r.frags[i]
    if fr.raw[:4] == ZSTD_MAGIC:
        return erpw.zstd_d(fr.raw)
    if fr.comp == ZLIB:
        return zlib.decompress(fr.raw)
    if fr.size != fr.packed:
        raise SystemExit(f"{r.name}: fragment {i} has unknown compression {fr.comp:#x}")
    return fr.raw


def put_frag(r, i, data):
    fr = r.frags[i]
    if fr.raw[:4] == ZSTD_MAGIC:
        fr.raw = erpw.zstd_c(data)
    elif fr.comp == ZLIB:
        fr.raw = zlib.compress(data, 9)
    else:
        fr.raw = data
    fr.size, fr.packed = len(data), len(fr.raw)
    r.hash = hashlib.md5(b"".join(frag_bytes(r, j) for j in range(len(r.frags)))).digest()


def season_characters(e, year):
    names = sorted(r.name.encode() for r in e.res)

    def defined(ref):
        i = bisect.bisect_left(names, ref)
        return i < len(names) and names[i].startswith(ref)

    def repl(m):
        if not defined(m.group(0)):
            return m.group(0)
        return b"eaid://" + m.group(1) + b"/" + season_folder(m.group(2), year) + b"/" + m.group(3)

    moved = rewritten = 0
    for r in e.res:
        for i in range(len(r.frags)):
            d = frag_bytes(r, i)
            nd = CHARACTER.sub(repl, d)
            if nd != d:
                put_frag(r, i, nd)
                rewritten += 1
    for r in e.res:
        n = CHARACTER.sub(repl, r.name.encode()).decode()
        if n != r.name:
            r.name = n
            moved += 1
    return moved, rewritten


def build(year, src_zip):
    with zipfile.ZipFile(src_zip) as z, tempfile.TemporaryDirectory() as tmp:
        hits = [n for n in z.namelist() if n.endswith("/" + MEMBER) or n == MEMBER]
        if not hits:
            return None
        e = erpw.Erp(z.extract(hits[0], tmp))
        new = OLD.replace(b"/2025/", f"/{folder(year)}/".encode())
        pattern = re.compile(re.escape(OLD) + rb"[^\x00]*")
        renamed, rewritten = rewrite(e, pattern, lambda m: new + m.group(0)[len(OLD):])
        scene = f"{new.decode()}incardriver_f1.scene"
        if not any(r.name == scene for r in e.res):
            raise SystemExit(f"{year}: no {scene} after the rename")
        moved, linked = season_characters(e, year)
        out = cars_dir(year) / "incardriver" / "incardriver_f1.erp"
        out.parent.mkdir(parents=True, exist_ok=True)
        e.to_file(str(out))
        return renamed, rewritten, moved, linked, out.stat().st_size


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--downloads", default=os.path.expanduser("~/Downloads"))
    ap.add_argument("--years", default="2016-2024")
    a = ap.parse_args()
    lo, _, hi = a.years.partition("-")
    for year in range(int(lo), int(hi or lo) + 1):
        r = build(year, find_zip(a.downloads, year))
        print(f"{year}: " + ("no in-car archive" if r is None else
                             f"{r[0]} renamed, {r[1]} rewritten, {r[2]} character resources moved to "
                             f"{folder(year)} with {r[3]} fragments relinked, {r[4]} bytes"), flush=True)


if __name__ == "__main__":
    main()
