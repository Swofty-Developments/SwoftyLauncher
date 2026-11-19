import os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import bindverify


def res(i, match, target=None, matches=1, method='scan', result='resolved'):
    return {'id': i, 'result': result, 'method': method, 'matches': matches, 'match': match,
            'target': match if target is None else target}


class PickHints(unittest.TestCase):
    def test_a_single_scan_hit_the_live_run_agrees_with_becomes_a_hint(self):
        hints, skipped = bindverify.pick_hints([res('A', 0x1000), res('B', 0x2008, target=0x2000)],
                                               {'A': 0x1000, 'B': 0x2000}, {})
        self.assertEqual(hints, {'A': {'match': '0x1000'}, 'B': {'match': '0x2008'}})
        self.assertEqual(skipped, {})

    def test_a_binding_the_live_run_did_not_resolve_still_takes_its_single_hit(self):
        hints, _ = bindverify.pick_hints([res('A', 0x1000)], {}, {})
        self.assertEqual(hints, {'A': {'match': '0x1000'}})

    def test_only_one_hit_by_scan_counts(self):
        hints, _ = bindverify.pick_hints([res('A', 0x1000, matches=2), res('B', 0x2000, method='anchor'),
                                          res('C', None, result='deferred'), res('D', 0x3000, method='hint')], {}, {})
        self.assertEqual(hints, {})

    def test_a_target_the_live_run_put_elsewhere_is_left_out(self):
        hints, skipped = bindverify.pick_hints([res('A', 0x1000)], {'A': 0x5000}, {})
        self.assertEqual(hints, {})
        self.assertIn('0x5000', skipped['A'])

    def test_an_existing_hint_is_never_moved(self):
        hints, skipped = bindverify.pick_hints([res('A', 0x1000), res('B', 0x2000)], {},
                                               {'A': {'match': '0x1000'}, 'B': {'match': '0x2100'}})
        self.assertEqual(hints, {'A': {'match': '0x1000'}})
        self.assertIn('0x2100', skipped['B'])


if __name__ == '__main__':
    unittest.main()
