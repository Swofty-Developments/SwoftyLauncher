import struct
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

import bcn
import build_special_liveries as special
import erpw
import gfx_material
import season_assets
import suit_sources
from pkg import Pkg, Val


def resource(name, kind, *payloads):
    return SimpleNamespace(name=name, type=kind.encode().ljust(16, b"\0"), unk=struct.pack("<I", 8),
                           frags=[SimpleNamespace(comp=erpw.STORED, raw=data, size=len(data), packed=len(data))
                                  for data in payloads], hash=b"\0" * 16)


def archive(*resources):
    result = erpw.Erp.__new__(erpw.Erp)
    result.res = list(resources)
    return result


class SeasonAssetsTest(unittest.TestCase):
    def test_old_materials_become_valid_f1_25_materials_with_original_textures(self):
        texture = 'eaid://cs_2021_vehicle_package/shared/tyres/textures/tyre_d.tif'
        shader = 'eaid://shader_package/tyre_sidewall/tyre_sidewall.nefx2'
        layout = gfx_material.catalog()[shader]
        slot = next(p[0] for p in layout['params'] if p[1] == gfx_material.TEXTURE)
        old = gfx_material.Material(23, shader, [gfx_material.Param(slot, gfx_material.TEXTURE, texture)])
        r = resource('tyre.material', 'GfxMaterialRes', gfx_material.write23(old))
        e = archive(r)
        self.assertEqual(len(season_assets.upgrade_materials(e)), 1)
        converted = gfx_material.parse25(e.frag_data(r, 0))
        self.assertEqual(converted.textures()[slot], texture)
        self.assertEqual(r.unk, struct.pack('<I', 2))
        self.assertTrue(any(p.link == texture for p in converted.params if p.heap))
        self.assertEqual(season_assets.upgrade_materials(e), [])

    def test_black_mercedes_never_inherits_the_white_kit(self):
        made = {entry["label"] for entry in suit_sources.real()}
        for season in suit_sources.DATA["seasons"]:
            if season["year"] not in (2020, 2021):
                continue
            team = next(t for t in season["teams"] if t["folder"] == "mercedes")
            self.assertEqual(team["suit"]["source"], suit_sources.ORIGINAL)
            for driver in (None, "lewis_hamilton", "valtteri_bottas"):
                self.assertIsNone(suit_sources.pick(season["year"], team, driver, made))

    def test_bc4_damage_mips_have_eight_byte_blocks(self):
        self.assertEqual(bcn.level_sizes(63, 8, 8, 3), [32, 8, 8])

    def test_existing_destination_is_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            destination = root / "out/classic-seasons-2021"
            destination.mkdir(parents=True)
            sentinel = destination / "original"
            sentinel.write_bytes(b"keep")
            with self.assertRaises(FileExistsError):
                season_assets.build(root / "addons", root / "game", 2021, root / "out")
            self.assertEqual(sentinel.read_bytes(), b"keep")

    def test_output_cannot_write_inside_a_game_installation(self):
        with tempfile.TemporaryDirectory() as directory:
            game = Path(directory)
            with self.assertRaises(ValueError):
                season_assets.build(game.parent / "addons", game, 2021, game / "new")
            with self.assertRaises(ValueError):
                special.build(game, game.parent / "addon", 2021, game / "new")

    def test_scene_suffix_rebuilds_package_string_offsets(self):
        old = "eaid://cs_2021_vehicle_package/teams/redbull/wep/redbull"
        document = Pkg()
        document.root.set("scene", Val("stri", old + "-carmodel.layer"))
        document.root.set("unrelated", Val("stri", "keep"))
        data = archive(resource(old + ".wep", "World", document.to_bytes()))
        entry = {"id": "redbull_honda", "team": "redbull"}
        special.rename_scene(data, 2021, entry)
        expected = old + special.suffix(2021, entry)
        self.assertEqual(data.res[0].name, expected + ".wep")
        result = Pkg(data.frag_data(data.res[0], 0)).root
        self.assertEqual(result["scene"].value, expected + "-carmodel.layer")
        self.assertEqual(result["unrelated"].value, "keep")

    def test_tyres_keep_the_original_buffer_resource_version(self):
        stem = "eaid://f1_2021_vehicle_package/shared/tyres/"
        buffer = resource(stem + "idf/tyres.idf?lod=all&vb=0", "GfxBufferRes", b"geometry")
        buffer.unk = struct.pack("<I", 1)
        image = resource(stem + "textures/tyre_d.tif.image", "GfxSurfaceRes", b"header", b"pixels")
        old = resource(buffer.name.replace("f1_2021", "cs_2021"), "GfxBufferRes", b"old")
        old.unk = struct.pack("<I", 2)
        target = archive(old)
        season_assets.restore_tyres(target, archive(buffer, image), Path("unused"), 2021)
        self.assertEqual(old.unk, buffer.unk)
        self.assertEqual(target.frag_data(old, 0), b"geometry")

    def test_geometry_with_binary_links_is_kept_without_other_liveries(self):
        stem = "eaid://f1_2021_vehicle_package/teams/fom_car/idf/fom_car"
        model, binding = stem + ".idf?render", stem + ".emb?context=chosen"
        mesh = stem + ".idf?lod=0&mesh"
        data = archive(resource(model, "ModelProxyRes", b"binary links"),
                       resource(mesh, "GfxMeshRes", b"mesh"),
                       resource(binding, "GfxMatBindRes", b"binding"),
                       resource(stem + ".emb?context=other", "GfxMatBindRes", b"unrelated"))
        kept = special.render_resources(data, stem + ".idf?context=render", binding)
        self.assertIn(mesh, kept)
        self.assertNotIn(stem + ".emb?context=other", kept)

    def test_bd_mipmaps_are_copied_losslessly_and_namespaced(self):
        old, new = "f1_2023_vehicle_package/", "l0_2023_vehicle_package/"
        path = old + "teams/fom_car/textures/liveries/test.tif.mipmaps"
        descriptor = bytes([len(path)]) + path.encode() + struct.pack("<I", 1)
        descriptor += struct.pack("<BQQQ", 7, 0, 4, 1024)
        source = resource("eaid://" + path.removesuffix(".mipmaps") + ".image", "GfxSurfaceRes",
                          b"header", b"embedded", descriptor)
        with tempfile.TemporaryDirectory() as directory:
            game = Path(directory)
            full = game / path
            full.parent.mkdir(parents=True)
            full.write_bytes(b"BD!!")
            sidecars = {}
            copied = special.copy_resource(archive(source), source, old, new, game, sidecars)
        self.assertEqual(copied.unk, struct.pack("<I", 9))
        target, levels = bcn.sidecar(copied.frags[2].raw)
        self.assertTrue(target.startswith(new))
        self.assertEqual(sidecars[target], b"BD!!")
        self.assertEqual(levels, [(0, 7, 0, 4, 1024)])
        self.assertEqual(copied.frags[1].raw, b"embedded")


if __name__ == "__main__":
    unittest.main()
