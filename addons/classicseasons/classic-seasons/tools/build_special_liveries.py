#!/usr/bin/env python3
"""Build official player-car special liveries on their original EA/Codemasters chassis.

The source season addon and original game are read only. --output must not exist.
F1 25's existing season wrapper supplies physics, cameras, sound and driver setup;
the original game's player-car render model, binding and textures supply the livery.
"""

import argparse
import copy
import hashlib
import json
import re
import shutil
import struct
from pathlib import Path

from season_assets import OriginalErp, upgrade_materials
import erpw
import old_helmets
from layout import package
from pkg import Arr, Obj, Pkg, Val

ROOT = Path(__file__).resolve().parent.parent
CATALOG = json.loads((ROOT / "data/special_liveries.json").read_text())
REFERENCE = re.compile(rb"eaid://[\x21-\x7e]+")
MODEL_LAYERS = ("-carmodel.layer", "-carshowroommodel.layer")


def suffix(year, entry):
    return f"_cs{year % 100:02d}_{entry['id']}"


def rename_scene(archive, year, entry):
    old = f"eaid://{package(year)}/teams/{entry['team']}/wep/{entry['team']}"
    new = old + suffix(year, entry)

    def replace(node):
        if isinstance(node, Val) and node.type == "stri":
            node.value = node.value.replace(old, new)
        elif isinstance(node, Obj):
            for key, value in node.items:
                replace(key)
                replace(value)
        elif isinstance(node, Arr):
            for value in node.items:
                replace(value)

    for resource in archive.res:
        resource.name = resource.name.replace(old, new)
        if resource.type.rstrip(b"\0") not in (b"World", b"WOInstances", b"EventGraph"):
            continue
        for i in range(len(resource.frags)):
            if resource.type.startswith(b"GfxSurfaceRes") and i > 0:
                continue
            data = archive.frag_data(resource, i)
            if old.encode() not in data:
                continue
            if not data.startswith(b"!pkg"):
                raise ValueError(f"{resource.name}: scene reference in an unsupported binary fragment")
            document = Pkg(data)
            replace(document.root)
            archive.set_frag(resource, i, document.to_bytes())


def references(archive, resource):
    found = set()
    for i in range(len(resource.frags)):
        if resource.type.startswith(b"GfxSurfaceRes") and i > 0:
            continue
        found.update(m.group().decode() for m in REFERENCE.finditer(archive.frag_data(resource, i)))
    return found


def render_resources(original, model, binding):
    """Keep the complete dependency closure, with no unrelated player-car skins."""
    resources = {r.name: r for r in original.res}
    # Model proxies also contain local binary identifiers, not only text EAIDs.
    # Keep every geometry record in this model's idf folder to cover those links.
    idf = model.split("/idf/", 1)[0] + "/idf/"
    pending = [r.name for r in original.res if r.name.startswith(idf)
               and not r.type.startswith(b"GfxMatBindRes")] + [model, binding]
    kept = {}
    while pending:
        name = pending.pop()
        if name not in resources:
            name = name.replace("?context=", "?")
        if name in kept:
            continue
        resource = resources.get(name)
        if resource is None:
            raise ValueError(f"original player car is missing {name}")
        kept[name] = resource
        for reference in references(original, resource):
            if reference not in resources:
                reference = reference.replace("?context=", "?")
            if reference in resources:
                pending.append(reference)
            elif reference.startswith("eaid://f1_"):
                raise ValueError(f"{name}: original dependency not in shared archive: {reference}")
    return kept


def copy_resource(original, source, old, new, game, sidecars):
    resource = copy.deepcopy(source)
    resource.name = resource.name.replace(old, new)
    for i, fragment in enumerate(resource.frags):
        payload = original.frag_data(source, i)
        if source.type.startswith(b"GfxSurfaceRes") and i == 2 and payload and payload[0]:
            path, entries = old_helmets.descriptor(payload, source.unk[0])
            roots = game if isinstance(game, (list, tuple)) else [game]
            full = next((root / path for root in roots if (root / path).is_file()), roots[0] / path)
            data = full.read_bytes()
            if any(off + packed > len(data) for _, off, packed, _ in entries):
                raise ValueError(f"{path}: incomplete original mipmap file")
            target = path.replace(old, new)
            if target == path:
                # A few shared textures use a different package's loose files.
                target = new + "original/" + path
            sidecars[target] = data
            encoded = target.encode()
            if len(encoded) > 255:
                raise ValueError(f"{target}: sidecar path too long")
            payload = bytes([len(encoded)]) + encoded + b"\0" + struct.pack("<I", len(entries))
            payload += b"".join(struct.pack("<BBQQQ", 0, *entry) for entry in entries)
            resource.unk = struct.pack("<I", 9)
        elif not source.type.startswith(b"GfxSurfaceRes") or i == 0:
            payload = payload.replace(old.encode(), new.encode())
        fragment.comp = erpw.STORED
        fragment.raw, fragment.size, fragment.packed = payload, len(payload), len(payload)
    resource.hash = hashlib.md5(b"".join(f.raw for f in resource.frags)).digest()
    return resource


def build_one(base, original, game, year, entry, index):
    old, new = f"f1_{year}_vehicle_package/", f"l{index:x}_{year}_vehicle_package/"
    if len(new) != len(old):
        raise ValueError("too many special liveries for a fixed-length namespace")
    model_team = entry.get("model_team", "fom_car")
    stem = f"eaid://{old}teams/{model_team}/idf/{model_team}"
    model, binding = stem + ".idf?context=render", stem + ".emb?context=" + entry["context"]
    kept = render_resources(original, model, binding)
    sidecars = {}
    # Keep the modern wrapper and team's original handling, sound and cameras.
    archive = copy.deepcopy(base)
    added = [copy_resource(original, r, old, new, game, sidecars) for r in kept.values()
             if r.name.startswith("eaid://" + old)]
    fixed = copy.deepcopy(next(r for r in added if r.name == binding.replace(old, new)))
    fixed.name = stem.replace(old, new) + ".emb?context=default"
    added = [r for r in added if r.name != fixed.name] + [fixed]
    archive.res.extend(added)
    for resource in archive.res:
        if not resource.name.startswith(f"eaid://{package(year)}/") or not resource.name.endswith(MODEL_LAYERS):
            continue
        layer = Pkg(archive.frag_data(resource, 0))
        for instance in layer.root["instances"].items:
            if not isinstance(instance, Obj):
                continue
            overrides = instance["overrides"]
            overrides["Model"].set("model", Val("stri", model.replace(old, new)))
            overrides["Model"].set("mat binding", Val("stri", fixed.name))
            custom = overrides.get("CustomMaterialModel")
            if isinstance(custom, Obj):
                custom.set("mat binding", Val("stri", stem.replace(old, new) + ".emb"))
        archive.set_frag(resource, 0, layer.to_bytes())
    if len({r.name for r in archive.res}) != len(archive.res):
        raise ValueError(f"{entry['id']}: duplicate resource names")
    rename_scene(archive, year, entry)
    upgrade_materials(archive)
    return archive, sidecars, len(kept)


def build(game, addon, year, output, monza_archive=None, target_game=None):
    sources = [path for path in (game, addon, monza_archive, target_game) if path is not None]
    if any(output.resolve().is_relative_to(path.resolve()) for path in sources):
        raise ValueError("output must be outside the original game and source addon")
    if output.exists():
        raise FileExistsError(output)
    if year in (2022, 2023) and not (addon / "cars/teams/common.erp").exists():
        if target_game is None:
            raise ValueError("--f1-25 is required to supply the season's shared materials archive")
        if not (target_game / "2025_asset_groups/f1_2025_vehicle_package/teams/common.erp").is_file():
            raise FileNotFoundError("F1 25 shared materials archive not found")
    entries = CATALOG[str(year)]
    shared = game / f"{year}_asset_groups/f1_{year}_vehicle_package/teams/common_fom.erp"
    if not shared.exists():
        shared = shared.parent / "fom_car/wep/fom_car.erp"
    original = OriginalErp(str(shared))
    common = game / f"{year}_asset_groups/f1_{year}_vehicle_package/teams/common.erp"
    known = {r.name for r in original.res}
    original.res.extend(r for r in OriginalErp(str(common)).res if r.name not in known)
    shutil.copytree(addon, output)
    if year in (2022, 2023):
        common_target = output / "cars/teams/common.erp"
        if not common_target.exists():
            if target_game is None:
                raise ValueError("--f1-25 is required to supply the season's shared materials archive")
            common_target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(target_game / "2025_asset_groups/f1_2025_vehicle_package/teams/common.erp", common_target)
        asset_path = output / "assets/cars.json"
        assets = json.loads(asset_path.read_text())
        target = f"2025_asset_groups/cs_{year}_vehicle_package/teams/common.erp"
        if not any(op.get("target") == target for op in assets["ops"]):
            assets["ops"].insert(0, {"op": "file.put", "source": "cars/teams/common.erp", "target": target})
        asset_path.write_text(json.dumps(assets, indent=1) + "\n")
    special_dir = output / "specials"
    special_dir.mkdir()
    registry, ops, strings, reports = {}, [], [], []
    for index, entry in enumerate(entries):
        source, roots = original, game
        if entry.get("model_team") == "ferrari":
            if monza_archive is None:
                raise ValueError("Ferrari Monza requires --monza-archive pointing to the extracted F1 22 folder")
            source = OriginalErp(str(monza_archive / "2022_asset_groups/f1_2022_vehicle_package/teams/ferrari/wep/ferrari.erp"))
            seen = {r.name for r in source.res}
            source.res.extend(r for r in OriginalErp(str(common)).res if r.name not in seen)
            roots = [monza_archive, game]
        path = addon / f"cars/teams/{entry['team']}/wep/{entry['team']}.erp"
        archive, sidecars, count = build_one(erpw.Erp(str(path)), source, roots, year, entry, index)
        archive.to_file(str(special_dir / f"{entry['id']}.erp"), reframe=False)
        for rel, data in sorted(sidecars.items()):
            target = special_dir / "sidecars" / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            ops.append({"op": "file.put", "source": f"specials/sidecars/{rel}", "target": rel})
        key = f"lng_cs_{year}_{entry['id']}"
        registry[entry["id"]] = {"vehicle": f"swofty.classic-seasons-{year}/{entry['team']}",
                                 "archive": f"specials/{entry['id']}.erp", "special": True,
                                 "suffix": suffix(year, entry), "picker_name": key, "unlock": "game"}
        strings.append({"op": "lang.set", "language": "*", "key": key, "value": entry["name"]})
        reports.append({**entry, "resources": count, "sidecars": len(sidecars),
                        "source_sha256": hashlib.sha256(shared.read_bytes()).hexdigest(),
                        "model": "original-game player car"})
        print(f"{year} {entry['id']}: {count} render resources, {len(sidecars)} original sidecars", flush=True)
    for rel, doc in (("registry/special_liveries.json", {"livery": registry}),
                     ("assets/special_liveries.json", {"schema_version": 1, "ops": ops}),
                     ("lang/special_liveries.json", {"schema_version": 1, "ops": strings}),
                     ("specials/provenance.json", reports)):
        target = output / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(doc, indent=1) + "\n")
    manifest = json.loads((output / "swofty.json").read_text())
    manifest["operations"]["assets"].extend(["assets/special_liveries.json", "lang/special_liveries.json"])
    (output / "swofty.json").write_text(json.dumps(manifest, indent=1) + "\n")


if __name__ == "__main__":
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--game", type=Path, required=True)
    ap.add_argument("--addon", type=Path, required=True)
    ap.add_argument("--year", type=int, choices=range(2021, 2025), required=True)
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument("--monza-archive", type=Path)
    ap.add_argument("--f1-25", type=Path, help="F1 25 installation supplying shared materials for the 2022–23 packs")
    args = ap.parse_args()
    build(args.game, args.addon, args.year, args.output, args.monza_archive, args.f1_25)
