import os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
try:
    import numpy, capstone  # noqa: F401
except ImportError:
    raise unittest.SkipTest('numpy and capstone are required')
import sigverify


class Restore(unittest.TestCase):
    class Img:
        def __init__(self, mem):
            self.mem, self._scans = mem, {'stale': 1}

    def test_puts_back_the_fixed_bytes_of_each_logged_hit(self):
        mem = bytearray(0x40)
        mem[0x10:0x18] = bytes.fromhex('e9112233449090ff')
        img = self.Img(mem)
        log = ['2026-11-17 10:51:36.133 [ 2340] scan .arch [41 56 ?? ?? 53 48 81] -> 1 hit at 0x140000010 '
               '(base+0x10), the address cached for this exe\n']
        self.assertEqual(sigverify.restore(img, log), (1, 5))
        self.assertEqual(bytes(img.mem[0x10:0x18]), bytes.fromhex('41562233534881ff'))
        self.assertEqual(img._scans, {})

    def test_ignores_scans_that_did_not_hit_once(self):
        img = self.Img(bytearray(0x20))
        log = ['scan .arch [41 57 41 56] -> 0 hits, rejected: need exactly 1\n',
               'scan .arch [41 57] -> 2 hits, rejected: need exactly 1\n']
        self.assertEqual(sigverify.restore(img, log), (0, 0))
        self.assertEqual(bytes(img.mem), bytes(0x20))

    def test_a_hit_that_matches_the_dump_changes_nothing(self):
        mem = bytearray(0x20)
        mem[4:7] = b'\x48\x89\xce'
        img = self.Img(mem)
        log = ['scan .arch [48 89 CE] -> 1 hit at 0x140000004 (base+0x4), the address cached for this exe\n']
        self.assertEqual(sigverify.restore(img, log), (0, 0))


class Code:
    def __init__(self, hexes):
        self.code = bytes.fromhex(''.join(hexes)) + b'\xcc' * 16

    def read(self, rva, n):
        return self.code[rva:rva + n]


# vmovupd [rip+d], xmm0, for which capstone gives disp_size 2; mov rcx, [rip+d]; ret.
KB = Code(['c5f91105f42b9502', '488b0d10203040', 'c3'])
PORT = Code(['c5f911058487a902', '488b0d99887766', 'c3'])


class RipDisplacement(unittest.TestCase):
    def test_functions_that_differ_only_in_rip_displacements_compare_equal(self):
        a, b = sigverify.norm(KB, 0, 16), sigverify.norm(PORT, 0, 16)
        self.assertEqual(a, b)
        self.assertEqual(a[0], 'c5f9110500000000')

    def test_a_pattern_leaves_out_all_four_displacement_bytes(self):
        pat = sigverify.masked_pattern(KB, 0, 16)
        self.assertEqual(pat[0], ['C5', 'F9', '11', '05', '??', '??', '??', '??'])
        self.assertEqual(pat[1], ['48', '8B', '0D', '??', '??', '??', '??'])


if __name__ == '__main__':
    unittest.main()
