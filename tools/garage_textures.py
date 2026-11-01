#!/usr/bin/env python3
import argparse
import os
import struct
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from erp import parse

ROOT = "eaid://environment_package/core_assets/objects/textures/"
DXGI = {52: 71, 54: 72, 55: 77, 57: 78, 65: 83, 69: 98, 70: 99}
BLOCK = {52: 8, 54: 8, 55: 16, 57: 16, 65: 16, 69: 16, 70: 16}


def chain_bytes(fmt, w, h, levels):
    total = 0
    for _ in range(levels):
        total += -(-w // 4) * -(-h // 4) * BLOCK[fmt]
        w, h = max(w // 2, 1), max(h // 2, 1)
    return total


def surface_header(fmt, w, h, levels):
    return struct.pack("<10I", 2, 0x108, fmt, w, h, 1, levels, 1, 0, 0)


def view_data(fmt, levels, surface):
    return struct.pack("<6I", 5, fmt, 0, levels, 0, 0) + surface.encode() + b"\0"


def dds(fmt, w, h, levels, pixels):
    head = bytearray(148)
    struct.pack_into("<4s", head, 0, b"DDS ")
    struct.pack_into("<I", head, 4, 124)
    struct.pack_into("<I", head, 8, 0x1 | 0x2 | 0x4 | 0x1000 | 0x20000 | 0x80000)
    struct.pack_into("<IIII", head, 12, h, w, -(-w // 4) * -(-h // 4) * BLOCK[fmt], 0)
    struct.pack_into("<I", head, 28, levels)
    struct.pack_into("<II4s", head, 76, 32, 4, b"DX10")
    struct.pack_into("<I", head, 108, 0x1000 | 0x8 | (0x400000 if levels > 1 else 0))
    struct.pack_into("<III", head, 128, DXGI[fmt], 3, 0)
    struct.pack_into("<I", head, 140, 1)
    return bytes(head) + pixels


class Erp:
    def __init__(self, path):
        self.path = path
        self.meta, self.res = parse(path)
        self.by_name = {r[0]: r for r in self.res}
        self.f = open(path, "rb")

    def fragment(self, res, i):
        _, off, _, _, comp, packed = res[3][i]
        self.f.seek(self.meta["data_off"] + off)
        raw = self.f.read(packed)
        if comp == 0x11:
            return subprocess.run(["zstd", "-dc"], input=raw, capture_output=True, check=True).stdout
        return raw


def main():
    ap = argparse.ArgumentParser(description="Write the garage and pit lane textures of an ERP archive as DDS files named like the stock resources.")
    ap.add_argument("erp")
    ap.add_argument("out")
    ap.add_argument("--root", default=ROOT)
    ap.add_argument("--match", default="", help="only resources whose name below the root contains this text")
    ap.add_argument("--check", action="store_true", help="verify the headers and views the core builds equal the archive's")
    a = ap.parse_args()
    erp = Erp(a.erp)
    n = bad = 0
    for name, res in erp.by_name.items():
        if res[1] != "GfxSurfaceRes" or not name.startswith(a.root) or not name.endswith(".tif.image"):
            continue
        rel = name[len(a.root):-len(".tif.image")]
        if a.match not in rel:
            continue
        head = erp.fragment(res, 0)
        fmt, w, h, levels = struct.unpack_from("<IIII", head, 8)[0], struct.unpack_from("<I", head, 12)[0], struct.unpack_from("<I", head, 16)[0], struct.unpack_from("<I", head, 24)[0]
        pixels = erp.fragment(res, 1)
        if a.check:
            view = erp.by_name.get(name[:-len(".image")])
            ok = head == surface_header(fmt, w, h, levels) and len(pixels) == chain_bytes(fmt, w, h, levels)
            ok = ok and view is not None and erp.fragment(view, 0) == view_data(fmt, levels, name)
            ok = ok and res[2] == 9 and view[2] == 2
            if not ok:
                bad += 1
                print("MISMATCH", rel, file=sys.stderr)
        path = os.path.join(a.out, rel + ".dds")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(dds(fmt, w, h, levels, pixels))
        n += 1
    print(f"{n} texture(s) written to {a.out}" + (f", {bad} mismatch(es)" if a.check else ""))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
