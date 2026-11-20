import os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import sitereg  # noqa: E402


class Img:
    def __init__(self, code):
        self.code = code

    def read(self, va, n):
        return self.code[va:va + n]


class ExpectBytes(unittest.TestCase):
    def test_a_call_or_jump_expects_only_its_opcode(self):
        self.assertEqual(sitereg.expect_bytes(Img(bytes.fromhex('E888 18DAFE 0FB783')), 0, None), 'E8')
        self.assertEqual(sitereg.expect_bytes(Img(bytes.fromhex('E9 00 00 00 00')), 0, None), 'E9')

    def test_other_instructions_keep_five_bytes(self):
        self.assertEqual(sitereg.expect_bytes(Img(bytes.fromhex('4889 5C24 08 55 48')), 0, None), '48 89 5C 24 08')

    def test_an_explicit_length_wins(self):
        self.assertEqual(sitereg.expect_bytes(Img(bytes.fromhex('E8 11 22 33 44')), 0, 3), 'E8 11 22')


if __name__ == '__main__':
    unittest.main()
