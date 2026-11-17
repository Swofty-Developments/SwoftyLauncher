import collections
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import binding_record
import kb


class UniqueKeys(unittest.TestCase):
    def test_signature_ids_are_unique(self):
        sigs = kb.load_signatures()["signatures"]
        dup = sorted(k for k, n in collections.Counter(s["id"] for s in sigs).items() if n > 1)
        self.assertEqual(dup, [], f"{len(dup)} signature id(s) appear more than once, e.g. {dup[:5]}")

    def test_function_and_global_addresses_are_unique(self):
        for table in ("functions", "globals"):
            rows, _ = kb.read_table(table)
            dup = sorted(k for k, n in collections.Counter(r["addr"] for r in rows).items() if n > 1)
            self.assertEqual(dup, [], f"{table} has duplicate addresses, e.g. {dup[:5]}")

    def test_names_are_unique(self):
        rows = kb.read_table("functions")[0] + kb.read_table("globals")[0]
        dup = sorted(k for k, n in collections.Counter(r["name"] for r in rows).items() if n > 1)
        self.assertEqual(dup, [], f"{len(dup)} name(s) appear more than once, e.g. {dup[:5]}")

    def test_binding_records_pass_the_static_checks(self):
        import binding
        self.assertEqual(binding_record.Store.get().errors, [])
        self.assertEqual(binding.check_paths([]), [])


    def test_structural_lint_is_clean(self):
        import kb_lint
        lint = kb_lint.KbLint(evidence_files=False, exe=False).run()
        self.assertEqual(lint.E, [])


if __name__ == "__main__":
    unittest.main()
