import contextlib, io, json, os, shutil, struct, sys, tempfile, unittest
from unittest import mock

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import bindverify  # noqa: E402
import census  # noqa: E402
from test_bindverify import CODE, DATA, RWX, page, pe  # noqa: E402

BASE = 0x140000000
CITATIONS = census.citations
T, THUNK, DATA1_THUNK, CHAINED_THUNK = 0x1100, 0x1500, 0x2100, 0x2300


def rel(op, at, to):
    return op + struct.pack('<i', to - (at + len(op) + 4))


def image_bytes():
    """A synthetic image with each kind of reference to T:
    .xtls   A (0x1200): calls at 0x1200 and 0x1210, a jcc at 0x1220, a call to THUNK at 0x1230, a lea at 0x1240
            B (0x1300, a second chained .pdata chunk at 0x1380): a tail jmp at 0x1310, a call at 0x1390
            a leaf adapter outside .pdata ending in a jmp at 0x1403, the bare jmp thunk THUNK after int3 padding,
            a wrapper whose closing jmp lands on a 16-byte boundary (0x1600) and a thunk right after another (0x1705)
    .data1  (writable code) a call outside .pdata at 0x2010, the nop5+jmp thunk DATA1_THUNK after a junk byte,
            calls to its entry (0x2200) and to its jmp (0x2210), the thunk CHAINED_THUNK that jumps to DATA1_THUNK
            and a call to it at 0x2400
    .rdata  pointers to T and THUNK, an E8 to T that is data, unwind info and the exception directory"""
    xtls = page((0x100, b'\x48\x89\xc8\xc3'),
                (0x200, rel(b'\xe8', 0x1200, T)), (0x210, rel(b'\xe8', 0x1210, T)),
                (0x220, rel(b'\x0f\x84', 0x1220, T)), (0x230, rel(b'\xe8', 0x1230, THUNK)),
                (0x240, rel(b'\x48\x8d\x05', 0x1240, T)), (0x250, b'\xc3'),
                (0x300, b'\x48\x83\xec\x28' + b'\x90' * 12), (0x310, rel(b'\xe9', 0x1310, T)),
                (0x390, rel(b'\xe8', 0x1390, T)),
                (0x400, b'\x4d\x63\xc0' + rel(b'\xe9', 0x1403, T)),
                (0x500, rel(b'\xe9', THUNK, T)),
                (0x5fa, b'\x48\x89\xc2\x4d\x63\xc1' + rel(b'\xe9', 0x1600, T)),
                (0x700, rel(b'\xe9', 0x1700, 0x1180) + rel(b'\xe9', 0x1705, T)))
    data1 = page((0x010, rel(b'\xe8', 0x2010, T)),
                 (0x0ff, b'\x11\x0f\x1f\x44\x00\x00' + rel(b'\xe9', 0x2105, T)),
                 (0x200, rel(b'\xe8', 0x2200, DATA1_THUNK)), (0x210, rel(b'\xe8', 0x2210, DATA1_THUNK + 5)),
                 (0x300, b'\x90' + rel(b'\xe9', 0x2301, DATA1_THUNK)),
                 (0x400, rel(b'\xe8', 0x2400, CHAINED_THUNK)))
    rdata = page((0x000, struct.pack('<Q', BASE + T)), (0x010, struct.pack('<Q', BASE + THUNK)),
                 (0x020, rel(b'\xe8', 0x3020, T)),
                 (0x100, b'\x01\x00\x00\x00'),
                 (0x110, b'\x21\x00\x00\x00' + struct.pack('<III', 0x1300, 0x1320, 0x3100)),
                 (0x200, struct.pack('<9I', 0x1200, 0x1260, 0x3100, 0x1300, 0x1320, 0x3100, 0x1380, 0x13a0, 0x3110)))
    return pe({'.xtls': (0x1000, CODE, xtls), '.data1': (0x2000, RWX, data1), '.rdata': (0x3000, DATA, rdata)},
              exception=(0x3200, 36), flat=False)


class Image(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp()
        cls.path = os.path.join(cls.tmp, 'F1_25.exe')
        with open(cls.path, 'wb') as f:
            f.write(image_bytes())
        cls.img = bindverify.Image(cls.path)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp)


class Counts(Image):
    def test_each_kind_of_reference_lands_in_its_column(self):
        self.assertEqual(census.Census(self.img, [T]).row(T), {
            'rva': '0x1100', 'calls': 4, 'callers': 3, 'sections': '.data1:1,.xtls:3', 'thunks': 4, 'thunk_calls': 4,
            'thunk_callers': 4, 'jumps': 4, 'lea': 1, 'pointers': 2})

    def test_the_sites_behind_the_counts(self):
        d = census.Census(self.img, [T]).detail(T)
        self.assertEqual([s for s, _ in d['calls']], [0x1200, 0x1210, 0x1390, 0x2010])
        self.assertEqual(d['thunks'], [THUNK, 0x1705, DATA1_THUNK, CHAINED_THUNK])
        self.assertEqual(d['thunk_calls'], [(0x1230, THUNK), (0x2200, DATA1_THUNK), (0x2210, DATA1_THUNK + 5),
                                            (0x2400, CHAINED_THUNK)])
        self.assertEqual([s for s, _ in d['jumps']], [0x1220, 0x1310, 0x1403, 0x1600])
        self.assertEqual(d['lea'], [(0x1240, T)])
        self.assertEqual(d['pointers'], [(0x3000, T), (0x3010, THUNK)])

    def test_a_chained_chunk_counts_as_its_function(self):
        f = census.Functions(self.img)
        self.assertEqual({f.caller(s) for s in (0x1310, 0x1390)}, {0x1300})
        self.assertEqual(f.caller(0x2010), ('site', 0x2010))

    def test_writable_code_is_scanned_and_data_is_not(self):
        names = [s.name for s in census.exec_sections(self.img)]
        self.assertEqual(names, ['.xtls', '.data1'])
        self.assertNotIn(0x3020, [s for s, _ in census.Census(self.img, [T]).detail(T)['calls']])

    def test_a_thunk_cited_on_its_own_keeps_its_calls_and_still_counts_for_its_target(self):
        c = census.Census(self.img, [THUNK, T])
        self.assertEqual((c.row(THUNK)['calls'], c.row(THUNK)['pointers'], c.row(THUNK)['thunks']), (1, 1, 0))
        self.assertEqual(c.row(T), census.Census(self.img, [T]).row(T))

    def test_a_jmp_that_ends_other_code_is_a_jump(self):
        f = census.Functions(self.img)
        self.assertIsNone(census.thunk_entry(self.img, f, 0x1403))
        self.assertIsNone(census.thunk_entry(self.img, f, 0x1310))
        self.assertIsNone(census.thunk_entry(self.img, f, 0x1600))
        self.assertEqual(census.thunk_entry(self.img, f, 0x1705), 0x1705)
        self.assertEqual(census.thunk_entry(self.img, f, THUNK), THUNK)
        self.assertEqual(census.thunk_entry(self.img, f, 0x2105), DATA1_THUNK)

    def test_a_flat_dump_gives_the_same_census(self):
        flat = os.path.join(self.tmp, 'flat.mem')
        with open(flat, 'wb') as f:
            f.write(bytes(self.img.mem))
        img = bindverify.Image(flat)
        self.assertTrue(img.flat)
        self.assertEqual(census.Census(img, [T]).row(T), census.Census(self.img, [T]).row(T))


class Stored(Image):
    def setUp(self):
        self.db = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.db)
        os.makedirs(os.path.join(self.db, 'builds'))
        self.sha = 'ab' * 32
        with open(os.path.join(self.db, 'builds', 'test.json'), 'w') as f:
            json.dump({'build': {'sha256': self.sha, 'pe_timestamp': self.img.stamp, 'image_base': hex(BASE)}}, f)
        with open(os.path.join(self.db, 'signatures.json'), 'w') as f:
            json.dump({'build': {'sha256': self.sha}}, f)
        for name, value in (('DB', self.db), ('BUILDS_DIR', os.path.join(self.db, 'builds')),
                            ('CENSUS_DIR', os.path.join(self.db, 'census')),
                            ('STRUCTS_DIR', os.path.join(self.db, 'structs'))):
            p = mock.patch.object(census, name, value)
            p.start()
            self.addCleanup(p.stop)

    def notes(self, *texts):
        return {'functions': [{'name': f'F{i}', 'notes': t, 'evidence': '-'} for i, t in enumerate(texts)]}

    def update(self, tables):
        with mock.patch.object(census, 'citations', lambda *_: CITATIONS(tables)), \
                contextlib.redirect_stdout(io.StringIO()):
            census.cmd_update(type('A', (), {'build': 'test', 'image': self.path})())
        with open(census.census_path('test')) as f:
            return f.read()

    def test_update_writes_one_sorted_row_per_cited_rva_and_reads_back(self):
        text = self.update(self.notes('callers: census test 0x1500', 'x; callers: census test 0x1100.'))
        self.assertEqual(text.split('\n')[:2], [census.header('test'), '\t'.join(census.COLUMNS)])
        self.assertIn(self.sha, text.split('\n')[0])
        self.assertEqual([r.split('\t')[0] for r in text.split('\n')[2:-1]], ['0x1100', '0x1500'])
        self.assertEqual(text, self.update(self.notes('callers: census test 0x1100', 'callers: census test 0x1500')))
        rows, errs = census.load('test')
        self.assertEqual((errs, rows[T]['calls'], rows[T]['sections']), ([], 4, '.data1:1,.xtls:3'))

    def test_check_finds_a_stored_count_the_image_no_longer_gives(self):
        tables = self.notes('callers: census test 0x1100')
        text = self.update(tables)
        with mock.patch.object(census, 'citations', lambda *_: CITATIONS(tables)):
            self.assertEqual(census.check('test', self.img), [])
            with open(census.census_path('test'), 'w') as f:
                f.write(text.replace('\t4\t3\t', '\t5\t3\t'))
            errs = census.check('test', self.img)
        self.assertEqual(len(errs), 1)
        self.assertIn('stored 5 calls', errs[0])

    def test_every_citation_resolves_to_a_row(self):
        self.assertIn('does not exist', census.citation_errors(self.notes('callers: census test 0x1100'))[0][0])
        self.update(self.notes('callers: census test 0x1100', 'callers: census test 0x1500'))
        errs, warns = census.citation_errors(self.notes(
            'callers: census test 0x1100', 'callers: census test 0x140001100', 'callers: census test 0x11AB',
            'callers: census test 0x1a00', 'callers: census nosuch 0x1100'))
        self.assertEqual(len(errs), 4, errs)
        self.assertIn('is a VA; cite the RVA 0x1100', errs[0])
        self.assertIn('lowercase hex as 0x11ab', errs[1])
        self.assertIn('has no row', errs[2])
        self.assertIn('census/nosuch.tsv does not exist', errs[3])
        self.assertEqual(len(warns), 1)
        self.assertIn('0x1500', warns[0])

    def test_a_broken_census_file_is_an_error(self):
        self.update(self.notes('callers: census test 0x1100', 'callers: census test 0x1500'))
        with open(census.census_path('test')) as f:
            lines = f.read().split('\n')
        with open(census.census_path('test'), 'w') as f:
            f.write('\n'.join([lines[0], lines[1], lines[3], lines[2], '']))
        self.assertIn('sorted', ' '.join(census.load('test')[1]))
        with open(census.census_path('test'), 'w') as f:
            f.write('\n'.join(['# census of test (sha256 old)', lines[1], lines[2], '']))
        self.assertIn('the overlay changed', ' '.join(census.load('test')[1]))
        with open(os.path.join(census.CENSUS_DIR, 'gone.tsv'), 'w') as f:
            f.write('')
        self.assertIn('census/gone.tsv: no build overlay', ' '.join(census.citation_errors({})[0]))

    def test_the_kb_build_with_a_census_turns_on_the_hand_count_rule(self):
        census._kb_build.cache_clear()
        self.assertIsNone(census.active())
        self.update(self.notes('callers: census test 0x1100'))
        self.assertEqual(census.active(), 'test')

    def test_show_prints_the_counts_and_the_sites(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            census.main(['show', 'test', hex(BASE + T), '--image', self.path, '--sites'])
        text = out.getvalue()
        self.assertIn('census test 0x1100 (VA 0x140001100): 4 calls from 3 functions (.data1 1, .xtls 3); 4 thunks '
                      'with 4 calls from 4 functions; 4 jumps; 1 lea; 2 pointers', text)
        self.assertIn('  thunk_call 0x140002210 .data1 outside .pdata via 0x140002105', text)
        self.assertIn('  call 0x140001390 .xtls in 0x140001300', text)


class HandCounts(unittest.TestCase):
    def test_caller_counts_written_by_hand(self):
        for text in ('370 callers.', 'Two E8 callers (0x1, 0x2)', 'sole caller X', 'no direct callers', '88+ callers',
                     'Called from 20 task destructors', '120 calls from 105 functions', '22084 direct calls',
                     'three call sites', '16 rel32 callers', '3 xref site(s)', '12 direct call sites', 'One caller:'):
            self.assertTrue(census.hand_counts(text), text)

    def test_other_counts_and_census_citations_pass(self):
        for text in ('callers: census steam 0x53784c0', 'rc != 0 jumps straight', '14 tuning handles', 'makes 2 calls',
                     '24 slots', 'every caller passes rcx', 'callers include the cutscene parser'):
            self.assertEqual(census.hand_counts(text), [], text)


@unittest.skipUnless(os.environ.get('SWOFTY_STEAM_EXE'), 'set SWOFTY_STEAM_EXE to the KB build F1_25.exe')
class StoredCensusOfTheKbBuild(unittest.TestCase):
    def test_every_stored_row_matches_the_exe(self):
        if not os.path.isfile(census.census_path('steam')):
            self.skipTest('the steam build has no census')
        self.assertEqual(census.check('steam', census.image_for('steam', os.environ['SWOFTY_STEAM_EXE'])), [])


if __name__ == '__main__':
    unittest.main()
