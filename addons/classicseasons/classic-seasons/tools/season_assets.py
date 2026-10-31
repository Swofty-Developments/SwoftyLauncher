#!/usr/bin/env python3
"""Restore original-game car textures and tyres into a NEW addon directory.

Sources and installed addons are read only. The destination must not exist.
Uses Codemasters/EA assets from a locally installed game, never third-party models.
"""

import argparse
import copy
import json
import shutil
import struct
import sys
import zlib
from pathlib import Path

import zstandard

HERE = Path(__file__).resolve().parent
sys.path[:0] = [str(HERE), str(HERE.parents[3] / "tools")]

import bcn
import erpw
import old_helmets
import gfx_material
from f1_23_suits import put_chain
from layout import FOLDERS, package
from tyres import override_layers


def upgrade_materials(archive):
    """Retain old shader values and texture bindings in F1 25's material layout."""
    converted = []
    for resource in archive.res:
        if resource.type.rstrip(b'\0') != b'GfxMaterialRes':
            continue
        payload = archive.frag_data(resource, 0)
        try:
            gfx_material.parse25(payload)
        except ValueError:
            material = gfx_material.parse23(payload)
            replacement, notes = gfx_material.plan23to25(material)
            archive.set_frag(resource, 0, gfx_material.write25(replacement))
            resource.unk = struct.pack('<I', 2) + resource.unk[4:]
            converted.append({'resource': resource.name, 'defaulted': notes['defaulted'], 'dropped': notes['dropped']})
    return converted


class OriginalErp(erpw.Erp):
    def frag_data(self, resource, index):
        fragment = resource.frags[index]
        if fragment.comp in (0x10, 0x11):
            data = zstandard.ZstdDecompressor().decompressobj().decompress(fragment.raw)
        elif fragment.comp == 1:
            data = zlib.decompress(fragment.raw)
        else:
            data = fragment.raw
        if len(data) != fragment.size:
            raise ValueError(f"{resource.name}: incomplete fragment {index}")
        return data


def original_chain(game, archive, resource):
    """Read all original mips; refuse unsupported compression rather than lose detail."""
    fmt, w, h, n = bcn.Surface.header(archive.frag_data(resource, 0))
    embedded = archive.frag_data(resource, 1)
    levels, at = [], 0
    for size in bcn.level_sizes(fmt, w, h, n):
        levels.append(embedded[at:at + size])
        at += size
    desc = archive.frag_data(resource, 2) if len(resource.frags) > 2 else b""
    if desc and desc[0]:
        path, entries = old_helmets.descriptor(desc, resource.unk[0])
        side = [old_helmets.read_level(Path(game) / path, entry) for entry in entries]
        w, h = w << len(side), h << len(side)
        levels = side + levels
    if [len(level) for level in levels] != bcn.level_sizes(fmt, w, h, len(levels)):
        raise ValueError(f"{resource.name}: incomplete original mip chain")
    return fmt, w, h, levels


def restore_textures(archive, original, game, year, folder, sidecars=None):
    old = f"eaid://f1_{year}_vehicle_package/teams/{folder}/textures/"
    new = f"eaid://{package(year)}/teams/{folder}/textures/"
    sources = {r.name: r for r in original.res}
    restored = []
    for r in archive.res:
        if not r.name.startswith(new) or not r.name.endswith(".image"):
            continue
        stem = r.name[len(new):]
        # Only the livery atlases: unrelated car textures keep their performance cap.
        if not any(token in stem for token in ("_decal_", "_decals_")):
            continue
        source = sources.get(old + stem)
        if source is None:
            continue
        desc = original.frag_data(source, 2) if len(source.frags) > 2 else b""
        path, entries = old_helmets.descriptor(desc, source.unk[0]) if desc and desc[0] else (None, [])
        if any(entry[0] == 7 for entry in entries):
            # Preserve the game's BD-compressed high mips losslessly. F1 25 can
            # stream them; the offline texture decoder cannot decode this format.
            if sidecars is None:
                raise ValueError(f"{source.name}: a sidecar output is required")
            data = (game / path).read_bytes()
            if any(off + packed > len(data) for _, off, packed, _ in entries):
                raise ValueError(f"{path}: truncated sidecar")
            target = package(year) + "/original/" + path.split("/", 1)[1]
            sidecars[target] = data
            encoded = target.encode()
            if len(encoded) > 255:
                raise ValueError(f"{target}: sidecar path too long")
            descriptor = bytes([len(encoded)]) + encoded + b"\0" + struct.pack("<I", len(entries))
            descriptor += b"".join(struct.pack("<BBQQQ", 0, *entry) for entry in entries)
            r.frags = copy.deepcopy(source.frags)
            for i, fragment in enumerate(r.frags):
                fragment.comp = erpw.STORED
                payload = descriptor if i == 2 else original.frag_data(source, i)
                fragment.raw, fragment.size, fragment.packed = payload, len(payload), len(payload)
            archive.set_frag(r, 0, original.frag_data(source, 0))
            view = archive.find(r.name.removesuffix(".image"))
            fmt, w, h, n = bcn.Surface.header(archive.frag_data(r, 0))
            header = bytearray(archive.frag_data(view, 0))
            struct.pack_into("<I", header, 4, fmt)
            struct.pack_into("<I", header, 12, n)
            archive.set_frag(view, 0, bytes(header))
            restored.append({"resource": r.name, "width": w << len(entries), "height": h << len(entries),
                             "levels": n + len(entries), "sidecar": target})
            continue
        fmt, w, h, levels = original_chain(game, original, source)
        if [len(level) for level in levels] != bcn.level_sizes(fmt, w, h, len(levels)):
            raise ValueError(f"{source.name}: incomplete original mip chain")
        put_chain(archive, r, fmt, w, h, levels)
        restored.append({"resource": r.name, "width": w, "height": h, "levels": len(levels)})
    return restored


def restore_tyres(common, original, game, year, source_year=None):
    """Take the original tyre geometry, materials and textures as one coherent set."""
    old = f"f1_{source_year or year}_vehicle_package/"
    new = f"{package(year)}/"
    targets = {r.name: r for r in common.res}
    restored = []
    images = 0
    for source in original.res:
        if f"eaid://{old}shared/tyres/" not in source.name:
            continue
        name = source.name.replace(old, new)
        target = targets.get(name)
        if target is None:
            target = copy.deepcopy(source)
            target.name = name
            common.res.append(target)
        else:
            target.frags = copy.deepcopy(source.frags)
        target.unk = source.unk
        # Re-encode old compression flags with the target game's stored encoding.
        for i, fr in enumerate(target.frags):
            fr.comp = erpw.STORED
            data = original.frag_data(source, i)
            if not source.type.startswith(b"GfxSurfaceRes") or i != 1:
                data = data.replace(old.encode(), new.encode())
            fr.raw, fr.size, fr.packed = data, len(data), len(data)
        common.set_frag(target, 0, common.frag_data(target, 0))
        if source.type.startswith(b"GfxSurfaceRes"):
            images += 1
            # Original texture arrays are already complete, with all slices and
            # mips embedded. Preserve their original resource version and layout.
            if len(source.frags) < 3:
                continue
            fmt, w, h, levels = original_chain(game, original, source)
            # SRV may follow the image in the source: embed after copying all resources.
            restored.append((target, fmt, w, h, levels))
    for target, fmt, w, h, levels in restored:
        put_chain(common, target, fmt, w, h, levels)
    if not images:
        raise ValueError(f"{year}: original shared archive contains no tyre textures")
    upgrade_materials(common)
    return images


def build(addons, game, year, output, tyres_only=False, source_year=None):
    source = addons / f"swofty.classic-seasons-{year}"
    destination = output / f"classic-seasons-{year}"
    if any(destination.resolve().is_relative_to(path.resolve()) for path in (addons, game)):
        raise ValueError("output must be outside the original game and installed addons")
    if destination.exists():
        raise FileExistsError(destination)
    source_year = source_year or year
    asset_root = game / f"{source_year}_asset_groups"
    if not asset_root.exists():
        asset_root = game / "asset_groups"
    original_root = asset_root / f"f1_{source_year}_vehicle_package/teams"
    # Check inputs before copying anything.
    for folder in FOLDERS:
        paths = [source / f"cars/teams/{folder}/wep/{folder}.erp"]
        if not tyres_only:
            paths.append(original_root / f"{folder}/wep/{folder}.erp")
        for path in paths:
            if not path.is_file():
                raise FileNotFoundError(path)
    shutil.copytree(source, destination)
    report = {"year": year, "original_game": str(game), "textures": {}, "tyres": 0}
    if year <= 2021:
        path = destination / "cars/teams/common.erp"
        common = erpw.Erp(str(path))
        tyre_source = original_root / "common.erp"
        if not tyre_source.exists():
            tyre_source = original_root.parent / "shared/tyres/tyres.erp"
        original = OriginalErp(str(tyre_source))
        report["tyres"] = restore_tyres(common, original, game, year, source_year)
        common.to_file(str(path), reframe=False)
    sidecars = {}
    for folder in FOLDERS:
        path = destination / f"cars/teams/{folder}/wep/{folder}.erp"
        archive = erpw.Erp(str(path))
        if not tyres_only:
            original = OriginalErp(str(original_root / f"{folder}/wep/{folder}.erp"))
            report["textures"][folder] = restore_textures(archive, original, game, year, folder, sidecars)
        if year <= 2021:
            override_layers(archive, package(year))
        archive.to_file(str(path), reframe=False)
    if sidecars:
        manifest = destination / "assets/cars.json"
        ops = json.loads(manifest.read_text())
        for rel, data in sorted(sidecars.items()):
            path = destination / "cars/sidecars" / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            ops["ops"].append({"op": "file.put", "source": f"cars/sidecars/{rel}", "target": rel})
        manifest.write_text(json.dumps(ops, indent=1) + "\n")
    (destination / "asset-restoration.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--addons", type=Path, required=True)
    ap.add_argument("--game", type=Path, required=True, help="original game for --year")
    ap.add_argument("--year", type=int, required=True, choices=range(2016, 2025))
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument("--tyres-only", action="store_true")
    ap.add_argument("--source-year", type=int, help="original tyre year when --tyres-only uses another season")
    args = ap.parse_args()
    if args.source_year and not args.tyres_only:
        ap.error("--source-year requires --tyres-only")
    report = build(args.addons, args.game, args.year, args.output, args.tyres_only, args.source_year)
    print(f"{args.year}: {sum(map(len, report['textures'].values()))} original livery textures; "
          f"{report['tyres']} original tyre textures")


if __name__ == "__main__":
    main()
