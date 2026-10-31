import hashlib
import struct
import unittest
import zlib

import build_incardriver as incar
from build_incardriver import erpw

TEX = b"eaid://character_package/body_template/male/textures/male_template_a.tga"
STOCK = b"eaid://character_package_shared/shared/textures/blank_n.tif"
SIDECAR = b"\x49character_package/body_template/male/textures/male_template_a.tga.mipmaps\x03\0\0\0"


def frag(data, comp):
    f = erpw.Frag()
    f.name, f.flags, f.comp, f.size = b"temp", 0x10, comp, len(data)
    f.raw = zlib.compress(data) if comp == incar.ZLIB else erpw.zstd_c(data) if comp in (0x10, 0x11) else data
    f.packed = len(f.raw)
    return f


def res(name, kind, version, *frags):
    r = erpw.Res()
    r.name, r.type, r.unk = name, kind.ljust(16, b"\0"), struct.pack("<I", version) + b"\0\0"
    r.frags = list(frags)
    r.hash = hashlib.md5(b"".join(incar.frag_bytes(r, i) for i in range(len(r.frags)))).digest()
    return r


def archive():
    e = erpw.Erp.__new__(erpw.Erp)
    e.res = [
        res(TEX.decode() + ".image", b"GfxSurfaceRes", 7, frag(b"\x02\0\0\0", incar.ZLIB),
            frag(b"pixels", incar.ZLIB), frag(SIDECAR, incar.ZLIB)),
        res(TEX.decode(), b"GfxSRVResource", 2, frag(TEX + b".image\0", 0x91)),
        res("eaid://character_package/heads/head_template/materials/skin.material", b"GfxMaterialRes", 2,
            frag(b"me_baseTex\0" + TEX + b"\0me_normal\0" + STOCK + b"\0" + TEX + b".sampler\0", 0x10)),
        res(TEX.decode() + ".sampler", b"GfxSampleState", 1, frag(b"\0" * 8, 0x91)),
    ]
    return e


class InCarDriver(unittest.TestCase):
    def test_character_resources_move_to_the_season_folder_and_references_follow(self):
        e = archive()
        before = [(r.name, [incar.frag_bytes(r, i) for i in range(len(r.frags))]) for r in e.res]
        self.assertEqual(incar.season_characters(e, 2017), (4, 2))
        moved = TEX.replace(b"/body_template/", b"/body_tempcs17/")
        self.assertEqual([r.name for r in e.res], [moved.decode() + ".image", moved.decode(),
                                                  "eaid://character_package/hcs17/head_template/materials/skin.material",
                                                  moved.decode() + ".sampler"])
        self.assertEqual(incar.frag_bytes(e.res[1], 0), moved + b".image\0")
        self.assertEqual(incar.frag_bytes(e.res[2], 0),
                         b"me_baseTex\0" + moved + b"\0me_normal\0" + STOCK + b"\0" + moved + b".sampler\0")
        self.assertEqual(incar.frag_bytes(e.res[0], 2), SIDECAR)
        for (name, frags), r in zip(before, e.res):
            self.assertEqual(len(r.name), len(name))
            self.assertEqual([len(incar.frag_bytes(r, i)) for i in range(len(r.frags))], [len(f) for f in frags])
            self.assertEqual(r.hash, hashlib.md5(b"".join(incar.frag_bytes(r, i)
                                                          for i in range(len(r.frags)))).digest())
        self.assertEqual([f.comp for f in e.res[2].frags], [0x10])
        self.assertEqual(e.res[2].frags[0].raw[:4], incar.ZSTD_MAGIC)

    def test_two_seasons_never_share_a_character_resource_name(self):
        a, b = archive(), archive()
        incar.season_characters(a, 2016)
        incar.season_characters(b, 2017)
        self.assertFalse({r.name for r in a.res} & {r.name for r in b.res})


if __name__ == "__main__":
    unittest.main()
