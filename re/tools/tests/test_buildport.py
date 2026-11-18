import os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import buildport


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

    def test_every_proof_names_known_builds_and_where_it_is(self):
        self.assertTrue(self.proofs)
        for p in self.proofs.values():
            self.assertTrue(set(p['builds']) <= set(self.order), p)
            self.assertTrue(os.path.exists(os.path.join(buildport.ROOT, p['file'])), p)

    def test_the_verified_layout_reaches_its_callers(self):
        self.assertTrue(self.proofs['VERIFIED_LAYOUT']['features'])

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


if __name__ == '__main__':
    unittest.main()
