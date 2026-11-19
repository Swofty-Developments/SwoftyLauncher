import os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import buildport
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from test_binding_check import NEEDS_EXE_MODULES


class NativeTables(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.order, cls.known = buildport.known_builds()
        cls.proofs = {p['name']: p for p in buildport.proofs(cls.order)}
        cls.live = buildport.live_groups(cls.order)

    def test_every_known_build_is_identified_by_its_stamp_and_size(self):
        self.assertIn('kb', self.order)
        for k in self.known:
            size = k['size_of_image'] + (0x1000 if k['larger'] else 0)
            self.assertEqual(buildport.identify(self.known, k['pe_timestamp'], size), k['build'])
            if k['larger']:
                self.assertIsNone(buildport.identify(self.known, k['pe_timestamp'], k['size_of_image']))
        self.assertIsNone(buildport.identify(self.known, 1, 1))

    def test_a_qualified_bit_reads_as_the_plain_one(self):
        bit = dict(self.order)
        plain = buildport.Expr('bit(Build::kb) | bit(Build::steam_1_0_145)', {}, bit).value()
        qualified = buildport.Expr(
            'build_proof::bit(build_proof::Build::kb) | build_proof::bit(build_proof::Build::steam_1_0_145)', {}, bit
        ).value()
        self.assertEqual(qualified, plain)
        self.assertEqual(plain, 1 << bit['kb'] | 1 << bit['steam_1_0_145'])

    def test_every_proof_names_known_builds_and_where_it_is(self):
        self.assertTrue(self.proofs)
        for p in self.proofs.values():
            self.assertTrue(set(p['builds']) <= set(self.order), p)
            self.assertTrue(os.path.exists(os.path.join(buildport.ROOT, p['file'])), p)

    def test_the_verified_layout_reaches_its_callers(self):
        v = self.proofs['VERIFIED_LAYOUT']
        self.assertTrue(v['features'])
        derived = [p for p in self.proofs.values() if 'VERIFIED_LAYOUT' in p['derived_from']]
        self.assertIn('SIGNIN_STATE', {p['name'] for p in derived})
        for p in derived:
            for f in p['features']:
                if ' (through ' not in f:
                    self.assertIn(f'{f} (through {p["name"]})', v['features'])

    def test_a_proof_built_from_another_proofs_builds_carries_its_features_back(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            base, user = os.path.join(d, 'base.hpp'), os.path.join(d, 'user.cpp')
            with open(base, 'w') as f:
                f.write('namespace proofs {\ninline constexpr Proof BASE{"the base layout", bit(Build::kb)};\n}\n')
            with open(user, 'w') as f:
                f.write('constexpr uint32_t BOTH = bp::proofs::BASE.builds | bp::bit(bp::Build::steam_1_0_145);\n'
                        'constexpr bp::Proof ONE{"one field", bp::proofs::BASE.builds};\n'
                        'constexpr bp::Gate G1{"feature one", &ONE};\n'
                        'constexpr bp::Proof TWO{"two fields", BOTH};\n'
                        'constexpr bp::Gate G2{"feature two", &TWO};\n'
                        'constexpr bp::Proof THREE{"three fields", ONE.builds | bp::bit(bp::Build::ea)};\n'
                        'constexpr bp::Gate G3{"feature three", &THREE};\n'
                        'constexpr bp::Proof OWN{"its own", bp::bit(bp::Build::ea)};\n'
                        'constexpr bp::Gate G4{"feature four", &OWN};\n')
            got = {p['name']: p for p in buildport.proofs(self.order, [base, user])}
        self.assertEqual(got['TWO']['builds'], ['kb', 'steam_1_0_145'])
        self.assertEqual(got['THREE']['builds'], ['kb', 'ea'])
        self.assertEqual(got['ONE']['derived_from'], ['BASE'])
        self.assertEqual(got['TWO']['derived_from'], ['BASE'], 'through the constant BOTH')
        self.assertEqual(got['THREE']['derived_from'], ['ONE', 'BASE'])
        self.assertEqual(got['OWN']['derived_from'], [])
        self.assertEqual(sorted(got['BASE']['features']),
                         ['feature one (through ONE)', 'feature three (through THREE)', 'feature two (through TWO)'])
        self.assertEqual(got['ONE']['features'], ['feature one', 'feature three (through THREE)'])
        self.assertEqual(got['OWN']['features'], ['feature four'])

    def test_every_live_build_holds_known_groups(self):
        groups = set(self.live['groups'])
        self.assertTrue(self.live['builds'])
        for b in self.live['builds']:
            self.assertTrue(set(b['groups']) <= groups, b['name'])

    def test_a_known_build_needs_no_first_step(self):
        for k in self.known:
            self.assertNotIn('first', buildport.layout_report(k['build']))

    def test_an_unknown_build_has_every_proof_off_and_a_first_step(self):
        r = buildport.layout_report(None)
        self.assertIn('enum Build', r['first'])
        off = {p['proof'] for p in r['proofs']} | set(r['unused'])
        self.assertEqual(off, set(self.proofs))
        self.assertEqual({g['group'] for g in r['live']}, set(self.live['groups']))

    def test_a_group_whose_site_does_not_resolve_is_off_with_the_binding_to_port(self):
        kb = buildport.layout_report('kb', lambda name: True)
        self.assertEqual(kb['live'], [])
        site = next(s for g in self.live['groups'] for s in self.live['sites'].get(g, []))
        r = buildport.layout_report('kb', lambda name: name != site)
        self.assertTrue(r['live'])
        self.assertTrue(all(site in ' '.join(g['why']) for g in r['live']), r['live'])


@NEEDS_EXE_MODULES
@unittest.skipUnless(os.environ.get('SWOFTY_STEAM_145_EXE'), 'set SWOFTY_STEAM_145_EXE to the Steam 1.0.145 exe')
class PortSteam145(unittest.TestCase):
    def test_the_current_build_resolves_every_binding_and_holds_every_live_group(self):
        import binding
        r = binding.port_report(os.environ['SWOFTY_STEAM_145_EXE'])
        self.assertEqual(r['build'], 'steam_1_0_145')
        self.assertNotIn('first', r)
        self.assertEqual(r['unresolved'], [])
        self.assertEqual(r['live'], [])
        self.assertTrue(all(p['off'] and p['do'] for p in r['proofs']), r['proofs'])


if __name__ == '__main__':
    unittest.main()
