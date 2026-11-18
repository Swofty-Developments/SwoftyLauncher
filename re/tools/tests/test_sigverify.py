import os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
try:
    import capstone, numpy  # noqa: F401
except ImportError:
    raise unittest.SkipTest('capstone and numpy are required')
import sigverify


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
