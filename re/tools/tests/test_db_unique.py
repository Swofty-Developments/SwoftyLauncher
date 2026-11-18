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

    def test_verify_ids_name_one_row_each(self):
        self.assertEqual(kb.colliding_verify_ids(kb.read_table("verify")[0]), {})

    def test_a_verify_id_on_two_rows_is_a_collision_above_the_shared_bound(self):
        rows = [{"notes": "V20001 a"}, {"notes": "V20001 b"}, {"notes": f"V{kb.SHARED_VERIFY_IDS_UP_TO} c"},
                {"notes": f"V{kb.SHARED_VERIFY_IDS_UP_TO} d"}, {"notes": "V20002 e"}, {"notes": "-"}]
        self.assertEqual(sorted(kb.colliding_verify_ids(rows)), [20001])

    def test_verify_ranges_do_not_overlap_and_hold_every_ranged_id(self):
        self.assertEqual(kb.verify_range_errors(kb.read_table("verify_ranges")[0], kb.read_table("verify")[0]), [])

    def test_binding_records_pass_the_static_checks(self):
        import binding
        self.assertEqual(binding_record.Store.get().errors, [])
        self.assertEqual(binding.check_paths([]), [])


    def test_function_names_follow_the_knowledge_base(self):
        with open(kb.FUNCTION_NAMES) as f:
            lines = f.read().split('\n')
        self.assertEqual((lines[0], lines[-1]), ('addr\tname', ''))
        have = [tuple(x.split('\t')) for x in lines[1:-1]]
        self.assertEqual(have, sorted(have, key=lambda r: int(r[0], 16)))
        rows = kb.read_table('functions')[0]
        merged = {(r['addr'], r['name']) for r in rows}
        stale = [r for r in have if r not in merged]
        self.assertEqual(stale, [], 'run python3 re/tools/kb.py function-names')
        listed = set(have)
        missing = [(r['addr'], r['name']) for r in rows if not r.get('_record') and (r['addr'], r['name']) not in listed]
        self.assertEqual(missing, [], 'run python3 re/tools/kb.py function-names')

    def test_structural_lint_is_clean(self):
        import kb_lint
        lint = kb_lint.KbLint(evidence_files=False, exe=False).run()
        self.assertEqual(lint.E, [])


class SubsystemsMemo(unittest.TestCase):
    def setUp(self):
        import shutil, tempfile
        from unittest import mock
        self.db = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.db)
        shutil.copy(kb.path("subsystems"), self.db)
        patcher = mock.patch.object(kb, "DB", self.db)
        patcher.start()
        self.addCleanup(patcher.stop)

    def rewrite(self, old, new):
        with open(kb.path("subsystems")) as f:
            text = f.read()
        self.assertIn(old, text)
        with open(kb.path("subsystems"), "w") as f:
            f.write(text.replace(old, new, 1))

    def test_a_changed_file_is_read_again(self):
        first = kb.subsystems()
        some = sorted(first)[0]
        owner = first[some]["owner"]
        self.rewrite(f"\t{owner}", "\t" + ("x" if owner[0] != "x" else "y") + owner[1:])
        self.assertNotEqual(kb.subsystems()[some]["owner"], owner)

    def test_callers_cannot_change_the_cached_rows(self):
        first = kb.subsystems()
        some = sorted(first)[0]
        first[some]["prefix"] = "Changed"
        first.pop(sorted(first)[-1])
        again = kb.subsystems()
        self.assertNotEqual(again[some]["prefix"], "Changed")
        self.assertEqual(len(again), len(first) + 1)


if __name__ == "__main__":
    unittest.main()


def granted(first, last, owner):
    return {"first": str(first), "last": str(last), "owner": owner, "granted": "2026-11-18", "notes": "-"}


def row(n, verifier, item="X"):
    return {"notes": f"V{n} note", "verifier": verifier, "kind": "func", "item": item}


class VerifyRanges(unittest.TestCase):
    """Verify ids come from ranges granted on the trunk, one owner per range, so two lanes cannot hand out one id."""

    A, B = 100000, 110000
    RANGES = [granted(A, A + 9999, "lane-a"), granted(B, B + 9999, "lane-b")]

    def test_the_first_id_of_an_owner_is_the_start_of_its_range(self):
        self.assertEqual(kb.next_verify_id("lane-a", [row(20030, "old")], self.RANGES), self.A)

    def test_ids_continue_after_the_highest_in_the_range_whatever_wrote_it(self):
        rows = [row(self.A + 4, "lane-a"), row(self.A + 7, "lane-a-blind"), row(self.B + 50, "lane-b")]
        self.assertEqual(kb.next_verify_id("lane-a", rows, self.RANGES), self.A + 8)
        self.assertEqual(kb.next_verify_id("lane-b-blind", rows, self.RANGES), self.B + 51)

    def test_two_lanes_on_two_branches_get_different_ids_from_the_same_trunk(self):
        trunk = [row(self.A, "lane-a"), row(self.B, "lane-b")]
        self.assertNotEqual(kb.next_verify_id("lane-a", trunk, self.RANGES), kb.next_verify_id("lane-b", trunk, self.RANGES))

    def test_the_owner_is_the_longest_name_up_to_a_dash(self):
        ranges = self.RANGES + [granted(120000, 129999, "lane-a-w2")]
        self.assertEqual(kb.range_owner("lane-a-w2-blind", ranges), "lane-a-w2")
        self.assertEqual(kb.range_owner("lane-a-blind", ranges), "lane-a")
        self.assertEqual(kb.range_owner("lane-a", ranges), "lane-a")
        self.assertIsNone(kb.range_owner("lane-ab", ranges))

    def test_a_full_range_moves_on_to_the_owners_next_range(self):
        ranges = [granted(self.A, self.A + 1, "lane-a"), granted(self.B, self.B + 9, "lane-a")]
        rows = [row(self.A, "lane-a"), row(self.A + 1, "lane-a")]
        self.assertEqual(kb.next_verify_id("lane-a", rows, ranges), self.B)

    def test_a_verifier_without_a_range_or_with_full_ranges_is_refused(self):
        with self.assertRaisesRegex(SystemExit, "no verify id range"):
            kb.next_verify_id("other", [], self.RANGES)
        with self.assertRaisesRegex(SystemExit, "used up"):
            kb.next_verify_id("lane-a", [row(self.A, "x"), row(self.A + 1, "x")], [granted(self.A, self.A + 1, "lane-a")])

    def test_overlapping_ranges_are_an_error(self):
        errs = kb.verify_range_errors([granted(self.A, self.A + 9999, "lane-a"), granted(self.A + 5000, self.A + 5999, "lane-b")], [])
        self.assertEqual(len(errs), 1)
        self.assertIn("overlap", errs[0])

    def test_an_id_outside_its_owners_ranges_is_an_error(self):
        rows = [row(self.B + 1, "lane-a", "Wrong"), row(self.A + 1, "lane-a", "Right"), row(self.A + 2, "nobody", "None"),
                row(20031, "legacy", "Old")]
        errs = kb.verify_range_errors(self.RANGES, rows)
        self.assertEqual([("Wrong" in e, "None" in e) for e in errs], [(True, False), (False, True)])

    def test_a_range_row_is_checked(self):
        self.assertEqual(kb.validate_row("verify_ranges", granted(self.A, self.A + 9, "lane-a")), [])
        self.assertTrue(kb.validate_row("verify_ranges", granted(20000, 20009, "lane-a")))
        self.assertTrue(kb.validate_row("verify_ranges", granted(self.A + 9, self.A, "lane-a")))
        self.assertTrue(kb.validate_row("verify_ranges", granted(self.A, self.A + 9, "two words")))

    def test_grant_range_appends_the_next_aligned_block(self):
        import tempfile
        from unittest import mock
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "verify_ranges.tsv"), "w") as f:
                f.write("first\tlast\towner\tgranted\tnotes\n100000\t109999\tlane-a\t2026-11-18\t-\n")
            args = type("A", (), {"owner": "lane-b", "size": 10000, "notes": None, "agent": "lead"})()
            with mock.patch.object(kb, "DB", d), mock.patch.object(kb, "LOCK", os.path.join(d, ".lock")), \
                    mock.patch.object(kb, "JOURNAL", os.path.join(d, "changes.log")), \
                    mock.patch("sys.stdout"):
                kb.cmd_grant_range(args)
                rows, errs = kb.read_table("verify_ranges")
        self.assertEqual(errs, [])
        self.assertEqual([(r["first"], r["last"], r["owner"]) for r in rows],
                         [("100000", "109999", "lane-a"), ("110000", "119999", "lane-b")])

    def test_an_id_another_verifier_wrote_inside_the_range_is_never_handed_out(self):
        rows = [row(self.A + 2, "lane-a"), row(self.A + 5, "lane-b"), row(self.A + 6, "legacy")]
        self.assertEqual(kb.next_verify_id("lane-a-blind", rows, self.RANGES), self.A + 7)


class LaneRanges(unittest.TestCase):
    """The ranges granted on the trunk keep every lane's ids apart: no id the allocator hands out is already in
    verify.tsv or a record, and two lanes writing rows on their own branches from the same trunk never share one."""

    @classmethod
    def setUpClass(cls):
        cls.ranges = kb.read_table("verify_ranges")[0]
        cls.rows = kb.read_table("verify")[0]
        cls.owners = sorted({r["owner"] for r in cls.ranges})

    def ids_in_files(self):
        import json
        ids = set()
        with open(os.path.join(kb.DB, "verify.tsv")) as f:
            for line in f:
                cols = line.rstrip("\n").split("\t")
                n = kb.verify_id({"notes": cols[-1]})
                if n is not None:
                    ids.add(n)
        for root, _, files in os.walk(os.path.join(kb.ROOT, "re", "bindings")):
            for name in files:
                if name.endswith(".json"):
                    with open(os.path.join(root, name)) as f:
                        ids |= {n for v in json.load(f).get("verify", []) if (n := kb.verify_id(v)) is not None}
        return ids

    def test_the_lanes_have_their_ranges(self):
        for owner in ("re-bindings", "std-libs", "native-guards", "port-live", "load-game"):
            self.assertIn(owner, self.owners)
        self.assertGreaterEqual(len(self.owners), 6)
        self.assertEqual(kb.verify_range_errors(self.ranges, self.rows), [])

    def test_no_owner_starts_another_owners_name(self):
        for a in self.owners:
            for b in self.owners:
                if a != b:
                    self.assertFalse(b.startswith(a + "-"), f"{b} draws from {a}'s range")

    def test_the_next_id_of_every_lane_is_in_no_row_of_the_checkout(self):
        taken = self.ids_in_files()
        self.assertTrue(taken)
        for owner in self.owners:
            for verifier in (owner, owner + "-blind"):
                n = kb.next_verify_id(verifier, self.rows, self.ranges)
                self.assertNotIn(n, taken, verifier)
                mine = [x for x in taken if any(int(g["first"]) <= x <= int(g["last"])
                                                for g in self.ranges if g["owner"] == owner)]
                self.assertGreater(n, max(mine, default=0), verifier)

    def test_two_lanes_on_two_branches_never_hand_out_the_same_id(self):
        handed = {}
        for owner in self.owners:
            branch = list(self.rows)
            got = []
            for i in range(25):
                n = kb.next_verify_id(owner if i % 2 else owner + "-blind", branch, self.ranges)
                got.append(n)
                branch.append(row(n, owner))
            self.assertEqual(len(set(got)), len(got), owner)
            handed[owner] = set(got)
        taken = {n for r in self.rows if (n := kb.verify_id(r)) is not None}
        for a in self.owners:
            self.assertFalse(handed[a] & taken, a)
            for b in self.owners:
                if a < b:
                    self.assertFalse(handed[a] & handed[b], f"{a} and {b}")
