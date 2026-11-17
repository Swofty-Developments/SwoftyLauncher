import argparse, contextlib, io, json, os, shutil, sys, tempfile, unittest
from unittest import mock

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
import kb


def export(path):
    with contextlib.redirect_stdout(io.StringIO()):
        kb.cmd_export(argparse.Namespace(dir=path))


def fake_tree(out):
    os.makedirs(os.path.join(out, 'structs'))
    with open(os.path.join(out, 'stamp'), 'w') as f:
        f.write(out)
    return 0


class KbExport(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.dir)
        self.link = os.path.join(self.dir, 'kb')

    def versions(self):
        return sorted(f for f in os.listdir(self.dir) if f.startswith('kb.v'))

    def test_an_export_holds_every_table_as_kb_reads_it(self):
        export(self.link)
        self.assertTrue(os.path.islink(self.link))
        for t, (cols, _) in kb.TABLES.items():
            with open(os.path.join(self.link, f'{t}.tsv')) as f:
                lines = f.read().split('\n')
            self.assertEqual(lines[0], '\t'.join(cols))
            self.assertEqual(len(lines) - 2, len(kb.read_table(t)[0]), t)
        with open(os.path.join(self.link, 'signatures.json')) as f:
            self.assertEqual(len(json.load(f)['signatures']), len(kb.load_signatures()['signatures']))
        want = sorted(f for f in os.listdir(os.path.join(kb.DB, 'structs')) if f.endswith('.h'))
        self.assertEqual(sorted(os.listdir(os.path.join(self.link, 'structs'))), want)

    def test_a_reader_that_resolved_the_link_keeps_one_whole_export(self):
        with mock.patch.object(kb, 'export_tree', side_effect=fake_tree):
            export(self.link)
            pinned = os.path.realpath(self.link)
            export(self.link)
        self.assertNotEqual(os.path.realpath(self.link), pinned)
        with open(os.path.join(pinned, 'stamp')) as f:
            self.assertEqual(f.read(), pinned)
        with open(os.path.join(self.link, 'stamp')) as f:
            self.assertEqual(f.read(), os.path.realpath(self.link))

    def test_old_exports_are_pruned_to_the_last_few(self):
        with mock.patch.object(kb, 'export_tree', side_effect=fake_tree):
            for _ in range(kb.EXPORT_KEEP + 2):
                export(self.link)
        self.assertEqual(len(self.versions()), kb.EXPORT_KEEP)
        self.assertIn(os.path.basename(os.path.realpath(self.link)), self.versions())

    def test_a_failed_export_leaves_the_last_one_in_place(self):
        with mock.patch.object(kb, 'export_tree', side_effect=fake_tree):
            export(self.link)
        before = os.path.realpath(self.link)

        def broken(out):
            fake_tree(out)
            raise OSError('disk full')
        with mock.patch.object(kb, 'export_tree', side_effect=broken), self.assertRaises(OSError):
            export(self.link)
        self.assertEqual(os.path.realpath(self.link), before)
        self.assertEqual(self.versions(), [os.path.basename(before)])

    def test_a_folder_from_an_older_export_becomes_a_link(self):
        os.makedirs(os.path.join(self.link, 'structs'))
        with mock.patch.object(kb, 'export_tree', side_effect=fake_tree):
            export(self.link + '/')
        self.assertTrue(os.path.islink(self.link))
        self.assertEqual(len(self.versions()), 2)


if __name__ == '__main__':
    unittest.main()
