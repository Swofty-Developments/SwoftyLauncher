import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import tt_expected as T

BOX = 'mod.season/f1_2011'
OTHER = 'mod.season/f1_2012'
ADDON = 'mod.season'


class Within(unittest.TestCase):
    def test_plain_content(self):
        self.assertTrue(T.within_holds([{'content': BOX}], BOX, ADDON))
        self.assertTrue(T.within_holds([{'content': [OTHER, BOX]}], BOX, ADDON))
        self.assertFalse(T.within_holds([{'content': OTHER}], BOX, ADDON))
        self.assertFalse(T.within_holds([{'content': BOX}, {'content': OTHER}], BOX, ADDON))

    def test_any(self):
        within = [{'any': [{'content': OTHER}, {'content': BOX}]}]
        self.assertTrue(T.within_holds(within, BOX, ADDON))
        self.assertFalse(T.within_holds([{'any': [{'content': OTHER}]}], BOX, ADDON))
        self.assertFalse(T.within_holds([{'any': []}], BOX, ADDON))
        nested = [{'any': [{'any': [{'content': BOX}]}, {'content': OTHER}]}, {'not': {'content': OTHER}}]
        self.assertTrue(T.within_holds(nested, BOX, ADDON))

    def test_other_core_forms(self):
        self.assertTrue(T.within_holds([], BOX, ADDON))
        self.assertTrue(T.within_holds([{'mode': 'time_trial'}], BOX, ADDON))
        self.assertFalse(T.within_holds([{'mode': 'career'}], BOX, ADDON))
        self.assertFalse(T.within_holds([{'data_group': 'x'}], BOX, ADDON))
        self.assertFalse(T.within_holds([{'year': [2011, 2012]}], BOX, ADDON))
        self.assertTrue(T.within_holds([{'owner': ADDON}], BOX, ADDON))

    def test_unknown_form_is_an_error(self):
        with self.assertRaises(T.SelectorError):
            T.within_holds([{'content': BOX, 'frobnicate': 1}], BOX, ADDON)
        with self.assertRaises(T.SelectorError):
            T.within_holds([{'any': {'content': BOX}}], BOX, ADDON)
        with self.assertRaises(T.SelectorError):
            T.within_holds(['content'], BOX, ADDON)
        with self.assertRaises(T.SelectorError):
            T.within_holds([{'track': 'monza'}], BOX, ADDON)


class SeasonCars(unittest.TestCase):
    def run_profiles(self, profiles):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / 'registry').mkdir()
            (root / 'swofty.json').write_text(json.dumps({'id': ADDON}))
            reg = {'content.package': {'f1_2011': {'teams': []}}, 'car.profile': profiles}
            (root / 'registry' / 'content.json').write_text(json.dumps(reg))
            return T.season_cars(str(root), None, None, [])[1]

    def test_applied_profiles_are_silent(self):
        problems = self.run_profiles({
            'a': {'within': [{'content': BOX}]},
            'b': {'within': [{'any': [{'content': OTHER}, {'content': BOX}]}]},
        })
        self.assertEqual(problems, [])

    def test_unknown_selector_is_reported(self):
        problems = self.run_profiles({'a': {'within': [{'frobnicate': 1}]}})
        self.assertEqual(len(problems), 1)
        self.assertIn('car.profile a', problems[0])
        self.assertIn('unknown selector "frobnicate"', problems[0])


if __name__ == '__main__':
    unittest.main()
