import os, struct, sys, tempfile, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
try:
    import numpy, capstone  # noqa: F401
except ImportError:
    raise unittest.SkipTest('numpy and capstone are required')
import bindverify, sigport, sigverify
from test_bindverify import CODE, DATA, RWX, page, pe

BODY = bytes.fromhex('4883ec28488b4108488b5010488d4a18e8000000004883c428c3')
CALLER = bytes.fromhex('48895c24085748') + b'\x83\xec\x20\x48\x8b\xf9'
CALLER_TAIL = bytes.fromhex('4883c4205fc3')
KB_FILLER = bytes.fromhex('0f8011223344') + bytes.fromhex('4ce955667788') + bytes.fromhex('0f8299aabbcc')
PORT_FILLER = bytes.fromhex('d9c1cc54486395')
LONG = ['4883ec28', '48c744242000000000', '4889cb', '488b4308', '488b5010', '488d4a18', 'e800000000', '488b4b08',
        '4883c428', 'c3']
MUTATED = LONG[:1] + ['ba3f1cdfb3', '81c2c1e3204c', '4889542420'] + LONG[2:]
MOVED = LONG[:3] + ['488b430c'] + LONG[4:]


def body(insns):
    return bytes.fromhex(''.join(insns))


def rel(op, at, to):
    return op + struct.pack('<i', to - (at + len(op) + 4))


def image(thunk, body, caller, filler, decoy=None, code=BODY):
    """.xtls: a protector thunk (a jmp to its body in .data1, then build-specific filler) and a function calling it;
    .data1: the body; .rdata: the exception directory with the caller's entry. decoy puts a jmp followed by the other
    build's filler elsewhere in .xtls."""
    call = caller + len(CALLER)
    parts = [(thunk - 0x1000, rel(b'\xe9', thunk, body) + filler),
             (caller - 0x1000, CALLER + rel(b'\xe8', call, thunk) + CALLER_TAIL)]
    if decoy:
        parts.append((decoy - 0x1000, rel(b'\xe9', decoy, 0x1800) + KB_FILLER))
    xtls = page(*parts)
    data1 = page((body - 0x2000, code))
    end = call + 5 + len(CALLER_TAIL)
    rdata = page((0x100, b'\x01\x04\x01\x00\x04\x42\x00\x00'), (0x200, struct.pack('<3I', caller, end, 0x3100)))
    return pe({'.xtls': (0x1000, CODE, xtls), '.data1': (0x2000, RWX, data1), '.rdata': (0x3000, DATA, rdata)},
              exception=(0x3200, 12))


class ProtectorThunk(unittest.TestCase):
    def load(self, data):
        f = tempfile.NamedTemporaryFile(suffix='.img', delete=False)
        f.write(data)
        f.close()
        self.addCleanup(os.unlink, f.name)
        return bindverify.Image(f.name)

    def setUp(self):
        self.kb = self.load(image(0x1100, 0x2100, 0x1300, KB_FILLER))
        self.port = self.load(image(0x1140, 0x2180, 0x1340, PORT_FILLER, decoy=0x1600))

    def test_the_twin_of_a_thunk_comes_through_its_call_site_not_its_filler(self):
        r = sigport.port_one(self.kb, self.port, {}, 'thunk', 0x1100)
        self.assertEqual(r['port'], 0x1140)
        self.assertEqual(r['kind'], 'rel32')
        self.assertEqual(r['verdict'], 'same')

    def test_a_witness_of_a_thunk_compares_the_twin_bodies(self):
        w = sigport.witness(self.kb, self.port, {}, 0x1100, [0x8, 0x10])
        self.assertEqual((w['port'], w['body'], w['port_body']), (0x1140, 0x2100, 0x2180))
        self.assertTrue(w['identical'])
        self.assertTrue(all(p['same_place'] for p in w['offsets'].values()))

    def test_a_use_after_a_protector_rewrite_still_aligns(self):
        kb = self.load(image(0x1100, 0x2100, 0x1300, KB_FILLER, code=body(LONG)))
        port = self.load(image(0x1140, 0x2180, 0x1340, PORT_FILLER, code=body(MUTATED)))
        w = sigport.witness(kb, port, {}, 0x1100, [0x8, 0x10])
        self.assertFalse(w['identical'])
        self.assertFalse(w['offsets'][0x8]['same_place'])
        self.assertEqual({o: (p['kb'], p['port'], p['aligned']) for o, p in w['offsets'].items()},
                         {0x8: (2, 2, True), 0x10: (1, 1, True)})

    def test_a_moved_field_is_not_witnessed(self):
        kb = self.load(image(0x1100, 0x2100, 0x1300, KB_FILLER, code=body(LONG)))
        port = self.load(image(0x1140, 0x2180, 0x1340, PORT_FILLER, code=body(MOVED)))
        w = sigport.witness(kb, port, {}, 0x1100, [0x8, 0xc, 0x10])
        self.assertEqual({o: (p['kb'], p['port'], p['aligned']) for o, p in w['offsets'].items()},
                         {0x8: (2, 1, False), 0xc: (0, 1, False), 0x10: (1, 1, True)})

    def test_the_sigverify_twin_of_a_thunk_is_the_call_site_one(self):
        self.assertEqual(sigverify.twin(self.kb, self.port, {}, 0x1100), 0x1140)

    def test_a_thunk_with_no_call_site_has_no_twin(self):
        kb = self.load(pe({'.xtls': (0x1000, CODE, page((0x100, rel(b'\xe9', 0x1100, 0x2100) + KB_FILLER))),
                           '.data1': (0x2000, RWX, page((0x100, BODY)))}))
        r = sigport.port_one(kb, self.port, {}, 'thunk', 0x1100)
        self.assertIn('error', r)


if __name__ == '__main__':
    unittest.main()
