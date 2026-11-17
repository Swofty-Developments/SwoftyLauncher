import os, struct, sys, tempfile, unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
try:
    import numpy, pefile  # noqa: F401
except ImportError:
    raise unittest.SkipTest('numpy and pefile are required')
import sigcheck
from test_bindverify import pe, page, CODE, DATA


class ExceptionDirectory(unittest.TestCase):
    def test_section_names_do_not_select_the_unwind_table(self):
        real = struct.pack('<III', 0x1100, 0x1180, 0)
        unrelated = struct.pack('<III', 0x1200, 0x1280, 0)
        raw = pe({'.tls$': (0x1000, CODE, page()),
                  '.actual': (0x3000, DATA, page((0, real))),
                  '.srdata': (0x4000, DATA, page((0, unrelated)))}, exception=(0x3000, 12))
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'fixture.exe'
            path.write_bytes(raw)
            image = sigcheck.Image(str(path))
            self.assertEqual(image.func_of(0x140001100), (0x140001100, 0x140001180))
            self.assertIsNone(image.func_of(0x140001200))
            image.data._mmap.close()


class FindAll(unittest.TestCase):
    class Img:
        base = 0x140000000

        def __init__(self, data):
            self.data = data

        def section_bytes(self, name):
            return 0x1000, self.data

    def test_overlapping_matches_count_like_the_core_scan(self):
        unit = bytes.fromhex('4883EC28E8') + bytes(3)
        img = self.Img(b'\xcc' * 8 + unit * 2 + bytes.fromhex('4883EC28') + b'\xcc' * 8)
        pat = sigcheck.parse_pattern('48 83 EC 28 E8 ?? ?? ?? 48 83 EC 28')
        self.assertEqual(sigcheck.find_all(img, '.xtls', pat), [0x140001008, 0x140001010])
        self.assertEqual(sigcheck.find_all(img, '.xtls', pat, limit=1), [0x140001008])
