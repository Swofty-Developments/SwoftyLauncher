import os, re, unittest

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
GUARD = os.path.join(ROOT, 'native', 'src', 'core', 'guards', 'frontend', 'setup_vm_guard.cpp')

KB_CHANGED_B = bytes.fromhex('83B8409F0000647510 4889F1 E888E8FFFF 4889F1 E820F0FFFF 80BE6A7E000000 752D'.replace(' ', ''))
EA_1_0_145_CHANGED_B = bytes.fromhex('83B8409F0000647510 4889F1 E888E8FFFF 4889F1 E820F0FFFF 80BE627E000000 752D'.replace(' ', ''))
KB_INIT = bytes.fromhex('83B8409F0000647510 4889F1 E8131E0000 4889F1 E8AB250000 C744243004000000'.replace(' ', ''))
KB_VALUE_SET = bytes.fromhex('83B8409F0000647510 4889F1 E8750B0000 4889F1 E80D130000 4C8DAED8760000'.replace(' ', ''))


def pattern(name):
    text = open(GUARD).read()
    m = re.search(r'Sig\s+' + name + r'\s*\{\s*"[^"]*",\s*"([^"]+)"', text)
    return [None if t == '??' else int(t, 16) for t in m.group(1).split()]


def matches(pat, code):
    return len(code) >= len(pat) and all(p is None or p == b for p, b in zip(pat, code))


class ChangedB(unittest.TestCase):
    def test_matches_the_kb_build_and_ea_1_0_145(self):
        pat = pattern('S_CHANGED_B')
        self.assertTrue(matches(pat, KB_CHANGED_B))
        self.assertTrue(matches(pat, EA_1_0_145_CHANGED_B))

    def test_does_not_match_the_other_sites_that_share_its_start(self):
        pat = pattern('S_CHANGED_B')
        self.assertFalse(matches(pat, KB_INIT))
        self.assertFalse(matches(pat, KB_VALUE_SET))

    def test_the_hooked_compare_and_its_jne_stay_fixed(self):
        self.assertEqual(pattern('S_CHANGED_B')[:9], list(bytes.fromhex('83B8409F0000647510')))


if __name__ == '__main__':
    unittest.main()
