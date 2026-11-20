import contextlib, copy, io, json, os, shutil, subprocess, sys, tempfile, unittest
from unittest import mock

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
import binding_check as bc
import binding_record as br

SAMPLE = 'session/site_Sess_StintTyreCheckPredictCall.json'


def have_exe_modules():
    try:
        import capstone, numpy, pefile  # noqa: F401
    except ImportError:
        return False
    return True


NEEDS_EXE_MODULES = unittest.skipUnless(have_exe_modules(), 'capstone, numpy and pefile are required (re/tools/py)')
AUTHOR = 'contributor-a'


def sample_doc():
    with open(os.path.join(br.RECORDS, SAMPLE), 'rb') as f:
        return br.parse_json(f.read())


def as_new(doc, author=AUTHOR, proof=True):
    doc = copy.deepcopy(doc)
    doc['author'] = author
    doc['verify'] = []
    doc['proofs'] = [make_proof(doc, author)] if proof else []
    return doc


def knowledge():
    if not hasattr(knowledge, 'cached'):
        knowledge.cached = bc.Knowledge()
    return knowledge.cached


def make_proof(doc, by, sha=None):
    k = knowledge()
    sha = sha or k.build['sha256']
    s = doc['signature']
    return {'by': by, 'date': '2026-11-17', 'tool': br.PROOF_TOOL,
            'exe': {'sha256': sha, 'size': k.build['size'], 'pe_timestamp': k.build['pe_timestamp']},
            'result': {'result': 'resolved', 'method': 'scan', 'matches': 1, 'match': s['match'], 'target': s['target']},
            'sigcheck': 'passed', 'digest': br.resolution_digest(s, sha)}


def agree_row(doc, by):
    return {'date': '2026-11-18', 'verifier': by, 'kind': 'global', 'item': doc['name'], 'claimed': doc['row']['level'],
            'verdict': 'agree', 'evidence': 'decomp:own disasm 0x1436a3ab0;xref:0x1436a3b74', 'notes': f'gh:{by} re-derived'}


def raw(doc):
    return json.dumps(doc, indent=1).encode()


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.doc = sample_doc()
        cls.knowledge = knowledge()
        cls.empty = tempfile.mkdtemp()
        cls.fresh = cls.knowledge.without(br.Record(cls.doc, SAMPLE))

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.empty)


class HostileJson(unittest.TestCase):
    def assertRejected(self, data, fragment):
        with self.assertRaises(br.RecordError) as cm:
            br.Record.from_bytes(data)
        self.assertIn(fragment, str(cm.exception))

    def test_oversize(self):
        self.assertRejected(b'{"a": "' + b'x' * br.MAX_BYTES + b'"}', 'limit')

    def test_not_utf8(self):
        self.assertRejected(b'{"a": "\xff\xfe"}', 'UTF-8')

    def test_duplicate_keys(self):
        self.assertRejected(b'{"name": "a", "name": "b"}', 'duplicate key')

    def test_nan_and_infinity(self):
        for c in (b'NaN', b'Infinity', b'-Infinity'):
            self.assertRejected(b'{"a": ' + c + b'}', 'not valid JSON')

    def test_deep_nesting(self):
        self.assertRejected(b'[' * 30000 + b']' * 30000, 'JSON')

    def test_not_an_object(self):
        self.assertRejected(b'[1, 2]', 'JSON object')

    def test_garbage(self):
        self.assertRejected(b'{"a": ', 'invalid JSON')

    def test_no_code_paths(self):
        for data in (b'{"__class__": "x"}', b'{"schema": {"$ref": "#"}}', b'"__import__(\\"os\\")"'):
            try:
                rec = br.Record.from_bytes(data)
            except br.RecordError:
                continue
            self.assertTrue(rec.shape_errors())


class Shape(Base):
    def test_sample_is_clean(self):
        self.assertEqual(br.Record(copy.deepcopy(self.doc), SAMPLE).shape_errors(), [])

    def test_unknown_and_missing_keys(self):
        d = copy.deepcopy(self.doc)
        d['extra'] = 1
        del d['row']
        errs = br.Record(d, SAMPLE).shape_errors()
        self.assertIn("unknown key 'extra'", errs)
        self.assertIn('row missing', errs)

    def test_tabs_and_newlines(self):
        for bad in ('a\tb', 'a\nb', '', 'x' * (br.MAX_TEXT + 1), '\x00'):
            d = copy.deepcopy(self.doc)
            d['row']['notes'] = bad
            self.assertIn('row.notes must be a one-line string (use - for empty)', br.Record(d, SAMPLE).shape_errors())

    def test_wrong_types(self):
        d = copy.deepcopy(self.doc)
        d['signature']['names'] = 'site_X'
        d['verify'] = {}
        d['author'] = '../../etc'
        errs = br.Record(d, SAMPLE).shape_errors()
        self.assertIn('signature.names must be a list of strings', errs)
        self.assertIn('verify must be a list', errs)
        self.assertIn('author must be a GitHub login', errs)

    def test_file_must_match_name(self):
        errs = br.Record(copy.deepcopy(self.doc), 'session/Other.json').shape_errors()
        self.assertEqual(errs, ['file must be re/bindings/session/site_Sess_StintTyreCheckPredictCall.json'])


class CheckRecord(Base):
    def errors(self, mutate):
        d = as_new(self.doc)
        mutate(d)
        return bc.check_record(br.Record(d, SAMPLE), self.fresh, 're/bindings/' + SAMPLE)

    def test_new_record_is_clean(self):
        self.assertEqual(self.errors(lambda d: None), [])

    def test_conflicts_with_the_knowledge_base(self):
        errs = bc.check_record(br.Record(as_new(self.doc), SAMPLE), self.knowledge, 're/bindings/' + SAMPLE)
        self.assertTrue(any('already in the knowledge base' in e for e in errs), errs)
        self.assertTrue(any('signature id' in e for e in errs), errs)

    def test_existing_address(self):
        taken = next(iter(self.fresh.addrs['globals']))

        def m(d):
            d['row']['addr'] = taken
        self.assertTrue(any('is already' in e for e in self.errors(m)))

    def test_cannot_inherit_old_verify_rows(self):
        k = bc.Knowledge.__new__(bc.Knowledge)
        k.__dict__ = dict(self.fresh.__dict__)
        k.verified = {**self.fresh.verified, ('global', self.doc['name']): None}
        errs = bc.check_record(br.Record(as_new(self.doc), SAMPLE), k, 're/bindings/' + SAMPLE)
        self.assertTrue(any('cannot inherit' in e for e in errs), errs)

    def test_pattern_rules(self):
        def short(d):
            d['signature']['pattern'] = 'E8 ?? ?? ?? ??'
        def edge(d):
            d['signature']['pattern'] = '?? ' + d['signature']['pattern']
        def token(d):
            d['signature']['pattern'] = d['signature']['pattern'] + ' ZZ'
        self.assertIn('signature.pattern is shorter than 12 bytes', self.errors(short))
        self.assertIn('signature.pattern starts or ends with a wildcard', self.errors(edge))
        self.assertIn("signature.pattern bad pattern token 'ZZ'", self.errors(token))

    def test_target_must_follow_match_and_anchor(self):
        def m(d):
            d['signature']['anchor'] = '0x4'
        self.assertIn('signature.target must equal match + anchor for resolve kind addr', self.errors(m))

    def test_expect_must_agree_with_the_pattern(self):
        def m(d):
            d['signature']['expect'] = 'E9'
        self.assertIn('signature.expect byte 0 contradicts the pattern', self.errors(m))

    def test_names_and_section(self):
        def m(d):
            d['signature']['names'] = ['site_Sess_Other']
            d['signature']['section'] = '.text'
        errs = self.errors(m)
        self.assertIn("signature.names must be ['site_Sess_StintTyreCheckPredictCall']", errs)
        self.assertIn('signature.section must be one of .xtls, .data1', errs)

    def test_author_cannot_self_verify(self):
        def m(d):
            d['verify'] = [agree_row(d, AUTHOR)]
        self.assertTrue(any('cannot verify their own' in e for e in self.errors(m)))

    def test_proof_digest_binds_the_pattern(self):
        def m(d):
            d['proofs'][0]['digest'] = br.resolution_digest({**d['signature'], 'anchor': 7}, d['proofs'][0]['exe']['sha256'])
        self.assertTrue(any('digest mismatch' in e for e in self.errors(m)))

    def test_proof_on_an_unknown_build(self):
        def m(d):
            d['proofs'] = [make_proof(d, AUTHOR, sha='0' * 64)]
        self.assertTrue(any('not a build in re/db/builds' in e for e in self.errors(m)))

    def test_proof_result_must_match_the_record(self):
        def m(d):
            d['proofs'][0]['result']['target'] = '0x140001000'
        self.assertTrue(any('the record says' in e for e in self.errors(m)))


class EngineBoundaries(Base):
    """The record rules an auto-merged record is held to, at the edge of what the core accepts."""

    def signature_errors(self, mutate):
        d = as_new(self.doc)
        mutate(d['signature'])
        d['row']['addr'] = d['signature']['target']
        return bc.signature_errors(d['signature'], d['row'], d['table'], d['name'], self.knowledge.build['sha256'])

    def at_anchor(self, anchor):
        def m(s):
            s['anchor'] = anchor
            s['target'] = hex(int(s['match'], 16) + br.number(anchor))
        return m

    def test_anchor_stops_at_256_bytes_after_the_match(self):
        self.assertEqual(self.signature_errors(self.at_anchor('0xff')), [])
        for bad in ('0x100', '0x28f', '0x1000', '-0x1', '0x' + 'f' * 40):
            errs = self.signature_errors(self.at_anchor(bad))
            self.assertIn('signature.anchor must be inside the first 256 bytes after the match', errs, bad)

    def test_anchor_that_is_not_a_number(self):
        def m(s):
            s['anchor'] = '1e3'
        self.assertEqual(self.signature_errors(m), ['signature.anchor and resolve offsets must be numbers'])

    def test_expect_stops_at_16_concrete_bytes(self):
        head = 'E8 11 22 33 44 C5 FA 5C FE'
        def expect(n):
            def m(s):
                s['expect'] = ' '.join([head] + ['90'] * (n - 9))
            return m
        self.assertEqual(self.signature_errors(expect(16)), [])
        self.assertIn('signature.expect must be 1 to 16 concrete bytes', self.signature_errors(expect(17)))

    def test_expect_takes_no_wildcard(self):
        def m(s):
            s['expect'] = 'E8 ??'
        self.assertIn('signature.expect must be 1 to 16 concrete bytes', self.signature_errors(m))

    def mid_site_with(self, bytes_at_anchor):
        def m(s):
            s['kind'] = 'mid'
            s['resolve'] = {'kind': 'addr'}
            s['anchor'] = '0x4'
            s['target'] = hex(int(s['match'], 16) + 4)
            s['pattern'] = '48 89 5C 24 ' + bytes_at_anchor + ' 48 8B D9 33 C0 C3'
            s.pop('expect', None)
        return m

    def test_a_mid_site_pattern_may_not_hold_the_cores_own_jump(self):
        errs = self.signature_errors(self.mid_site_with('E9 82 20 A2 14'))
        hit = [e for e in errs if 'whole E9 rel32 at the site the core hooks' in e]
        self.assertEqual(len(hit), 1, errs)
        self.assertIn('wildcard the rel32', bc.fix_for(hit[0]))

    def test_a_mid_site_pattern_with_the_game_jump_wildcarded_passes(self):
        self.assertEqual(self.signature_errors(self.mid_site_with('E9 ?? ?? ?? ??')), [])
        self.assertEqual(self.signature_errors(self.mid_site_with('F6 46 05 04 74')), [])

    def with_existing_pattern(self):
        k = bc.Knowledge.__new__(bc.Knowledge)
        k.__dict__ = dict(self.fresh.__dict__)
        k.patterns = {**self.fresh.patterns, bc.pattern_key(self.doc['signature']): 'session.existing'}
        return k

    def record_errors(self, mutate, knowledge):
        d = as_new(self.doc)
        mutate(d['signature'])
        d['row']['addr'] = d['signature']['target']
        d['proofs'] = [make_proof(d, AUTHOR)]
        return bc.check_record(br.Record(d, SAMPLE), knowledge, 're/bindings/' + SAMPLE)

    def test_a_respelled_pattern_is_still_the_same_scan(self):
        dup = 'signature.pattern, anchor and resolve are the same as session.existing'
        k = self.with_existing_pattern()
        respell = {
            'single wildcards': lambda s: s.update(pattern=s['pattern'].replace('??', '?')),
            'lowercase and spacing': lambda s: s.update(pattern='  ' + s['pattern'].lower().replace(' ', '   ') + ' '),
            'decimal anchor': lambda s: s.update(anchor='3'),
            'padded anchor': lambda s: s.update(anchor='0x003'),
        }
        for why, m in respell.items():
            errs = self.record_errors(m, k)
            self.assertIn(dup, errs, why)

    def test_the_same_pattern_with_another_anchor_is_another_binding(self):
        errs = self.record_errors(self.at_anchor('0x8'), self.with_existing_pattern())
        self.assertFalse(any('are the same as' in e for e in errs), errs)

    def test_resolve_offsets_compare_as_numbers(self):
        s = dict(self.doc['signature'], resolve={'kind': 'rel32', 'insn': '0x2'})
        for same in ('2', '0x02', 2):
            self.assertEqual(bc.pattern_key(s), bc.pattern_key(dict(s, resolve={'kind': 'rel32', 'insn': same})))
        self.assertNotEqual(bc.pattern_key(s), bc.pattern_key(dict(s, resolve={'kind': 'rel32', 'insn': '0x3'})))
        self.assertNotEqual(bc.pattern_key(s), bc.pattern_key(dict(s, kind='call')))

    def sig_row(self, item, by='contributor-b'):
        return {**agree_row(self.doc, by), 'kind': 'sig', 'item': item}

    def test_verify_rows_may_be_about_the_records_own_signature_only(self):
        own = self.doc['signature']['id']
        def ok(d):
            d['verify'] = [self.sig_row(own)]
        def other(d):
            d['verify'] = [self.sig_row('session.other_signature')]
        def selfie(d):
            d['verify'] = [self.sig_row(own, by=AUTHOR)]
        check = CheckRecord.errors
        self.assertEqual(check(self, ok), [])
        self.assertIn(f'verify[0] must be about global {self.doc["name"]} or sig {own}', check(self, other))
        self.assertTrue(any('cannot verify their own' in e for e in check(self, selfie)))

    def test_an_agree_about_the_signature_does_not_export_the_binding(self):
        import bindgen
        gate = bindgen.Gate(rows=[{'date': '2026-11-17', 'kind': 'sig', 'item': self.doc['signature']['id'],
                                   'verdict': 'agree', 'notes': 'V1 text'}], exceptions={})
        row = self.doc['row']
        status, _ = gate('L2', row['evidence'], [('global', self.doc['name']), ('global', row['addr'])])
        self.assertIsNone(status)


class Classify(Base):
    def run_classify(self, changes, author=AUTHOR, root=None):
        return bc.classify(changes, author, self.fresh, root or self.empty)

    def test_new_record_merges_itself(self):
        out = self.run_classify([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc)))])
        self.assertEqual((out['verdict'], out['errors'], out['notes']), ('auto', [], []))

    def test_other_author_needs_review(self):
        out = self.run_classify([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc)))], author='someone-else')
        self.assertEqual(out['verdict'], 'review')

    def test_without_proof_needs_review(self):
        out = self.run_classify([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc, proof=False)))])
        self.assertEqual(out['verdict'], 'review')

    def test_l3_needs_review(self):
        d = as_new(self.doc)
        d['row']['level'] = 'L3'
        d['row']['evidence'] += ';trace:T000/run1'
        d['proofs'] = [make_proof(d, AUTHOR)]
        self.assertNotEqual(self.run_classify([('re/bindings/' + SAMPLE, None, raw(d))])['verdict'], 'auto')

    def test_broken_record_fails(self):
        out = self.run_classify([('re/bindings/' + SAMPLE, None, b'{"schema": 1, "schema": 2}')])
        self.assertEqual(out['verdict'], 'fail')

    def test_other_paths_and_deletions_need_review(self):
        self.assertEqual(self.run_classify([('re/tools/bindgen.py', None, b'x')])['verdict'], 'review')
        self.assertEqual(self.run_classify([('re/bindings/' + SAMPLE, raw(self.doc), None)])['verdict'], 'review')
        self.assertEqual(self.run_classify([('re/bindings/../db/x.json', None, raw(self.doc))])['verdict'], 'review')
        self.assertEqual(self.run_classify([])['verdict'], 'review')

    def test_name_already_used_in_code_needs_review(self):
        root = tempfile.mkdtemp()
        try:
            os.makedirs(os.path.join(root, 'native', 'src'))
            with open(os.path.join(root, 'native', 'src', 'x.cpp'), 'w') as f:
                f.write(f'resolve("{self.doc["name"]}");\n')
            out = self.run_classify([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc)))], root=root)
            self.assertEqual(out['verdict'], 'review')
            self.assertTrue(any('native/src/x.cpp' in n for n in out['notes']))
        finally:
            shutil.rmtree(root)

    def second_proof(self, by, verdict='agree'):
        old = as_new(self.doc)
        new = copy.deepcopy(old)
        v = agree_row(new, by)
        v['verdict'] = verdict
        new['verify'].append(v)
        new['proofs'].append(make_proof(new, by))
        return self.run_classify([('re/bindings/' + SAMPLE, raw(old), raw(new))], author=by)

    def test_second_proof_by_someone_else_merges(self):
        out = self.second_proof('contributor-b')
        self.assertEqual((out['verdict'], out['notes']), ('auto', []))
        self.assertEqual(out['records'][0]['decision'], 'proof')

    def test_second_proof_by_the_author_needs_review(self):
        self.assertEqual(self.second_proof(AUTHOR)['verdict'], 'fail')

    def test_disagree_needs_review(self):
        self.assertEqual(self.second_proof('contributor-b', 'disagree')['verdict'], 'review')

    def test_editing_an_existing_record_needs_review(self):
        old = as_new(self.doc)
        new = copy.deepcopy(old)
        new['signature']['used_by'] = ['addons/x/native/x.cpp']
        out = self.run_classify([('re/bindings/' + SAMPLE, raw(old), raw(new))])
        self.assertEqual(out['verdict'], 'review')


class Renames(Base):
    OLD, NEW = 'site_Sess_StintTyreCheckPredictCall', 'site_Sess_StintTyrePredictCall'

    def row(self, item, kind='global', vid='V100001', verdict='partial'):
        return {'date': '2026-11-18', 'verifier': 'other-agent', 'kind': kind, 'item': item, 'claimed': 'L2',
                'verdict': verdict, 'evidence': 'decomp:own disasm 0x1436a3ab0;xref:0x1436a3b74', 'notes': f'{vid} x'}

    def errors(self, doc):
        return [e for e in bc.check_record(br.Record(doc, SAMPLE), self.fresh) if 'must be about' in e]

    def test_a_record_keeps_the_retired_rows_of_its_own_kind(self):
        doc = copy.deepcopy(self.doc)
        doc['verify'] = [self.row('retired:Sess_Gone'), self.row(doc['row']['addr'])]
        self.assertEqual(self.errors(doc), [])
        doc['verify'] = [self.row('retired:Sess_Gone', kind='func')]
        self.assertEqual(len(self.errors(doc)), 1)
        doc['verify'] = [self.row('Sess_Gone')]
        self.assertEqual(len(self.errors(doc)), 1)

    def rename(self, old, new):
        import argparse, binding
        with contextlib.redirect_stdout(io.StringIO()) as out, \
                mock.patch.dict(os.environ, {'SWOFTY_RECORD_CACHE': '0'}):
            binding.cmd_rename(argparse.Namespace(old=old, new=new), root=self.root, db=self.db)
        br.Store.forget()
        return out.getvalue()

    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, self.tmp)
        self.root, self.db = os.path.join(self.tmp, 'bindings'), os.path.join(self.tmp, 'db')
        doc = copy.deepcopy(self.doc)
        doc['verify'] = [self.row(doc['row']['addr'], vid='V100001', verdict='agree'), self.row(self.OLD, vid='V100002')]
        os.makedirs(os.path.join(self.root, 'session'))
        os.makedirs(os.path.join(self.db, 'builds'))
        with open(os.path.join(self.root, SAMPLE), 'w') as f:
            f.write(json.dumps(doc, indent=1) + '\n')
        self.overlay = {'schema': 1, 'hints': {'site_A': {'match': '0x1'}, self.OLD: {'match': '0x2'},
                                               'site_Z': {'match': '0x3'}}}
        with open(os.path.join(self.db, 'builds', 'port.json'), 'w') as f:
            f.write(json.dumps(self.overlay, indent=1) + '\n')
        self.addCleanup(br.Store.forget)

    def test_rename_moves_the_record_and_retires_the_rows_about_its_old_name(self):
        self.rename(self.OLD, self.NEW)
        self.assertFalse(os.path.exists(os.path.join(self.root, SAMPLE)))
        with open(os.path.join(self.root, 'session', self.NEW + '.json')) as f:
            doc = json.load(f)
        self.assertEqual((doc['name'], doc['signature']['names'], doc['signature']['id']),
                         (self.NEW, [self.NEW], self.doc['signature']['id']))
        self.assertEqual([v['item'] for v in doc['verify']], [self.doc['row']['addr'], 'retired:' + self.OLD])
        self.assertEqual(doc['proofs'], self.doc['proofs'])
        self.assertEqual(bc.check_record(br.Record(doc, f'session/{self.NEW}.json'), self.fresh), [])
        with open(os.path.join(self.db, 'builds', 'port.json')) as f:
            hints = json.load(f)['hints']
        self.assertEqual(list(hints), ['site_A', self.NEW, 'site_Z'])
        self.assertEqual(hints[self.NEW], {'match': '0x2'})

    def test_rename_refuses_an_unknown_or_taken_name(self):
        for old, new in (('site_Sess_NoSuchThing', self.NEW), (self.OLD, self.OLD), (self.OLD, 'not a name'),
                         (self.OLD, 'Vm_MyTeamSetupCard_BindSponsorNodes')):
            with self.assertRaises(SystemExit, msg=(old, new)):
                self.rename(old, new)
        self.assertTrue(os.path.exists(os.path.join(self.root, SAMPLE)))

    def overlay_errors(self, rows, current, drops=None):
        return bc.overlay_name_errors(bc.retired_names(rows, current, drops or {}), os.path.join(self.db, 'builds'))

    def test_an_overlay_entry_left_under_a_retired_name_fails_the_check(self):
        rows = [dict(self.row('retired:' + self.OLD), _record=f'session/{self.NEW}.json')]
        errs = self.overlay_errors(rows, {self.NEW, 'site_A', 'site_Z'})
        self.assertEqual(len(errs), 1)
        self.assertTrue(errs[0].startswith(f're/db/builds/port.json: hints entry {self.OLD} '), errs)
        self.assertIn(f'renamed to {self.NEW}', errs[0])
        self.assertIn(f'from {self.OLD} to {self.NEW}', bc.fix_for(errs[0]))

    def test_a_retired_row_without_a_record_takes_the_new_name_from_the_drops_entry(self):
        drops = {('global', self.OLD): {'mode': 'deprecated', 'replacement': self.NEW}}
        self.assertIn(f'renamed to {self.NEW}', self.overlay_errors([self.row('retired:' + self.OLD)], set(), drops)[0])
        self.assertTrue(self.overlay_errors([self.row('retired:' + self.OLD)], set())[0].endswith('was renamed'))

    def test_moved_entries_and_reused_names_pass(self):
        rows = [dict(self.row('retired:' + self.OLD), _record=f'session/{self.NEW}.json')]
        self.assertEqual(self.overlay_errors(rows, {self.OLD}), [])
        self.rename(self.OLD, self.NEW)
        self.assertEqual(self.overlay_errors(rows, {self.NEW}), [])

    def test_the_repository_overlays_hold_no_retired_name(self):
        import bindgen, kb
        retired = bc.retired_names(kb.read_table('verify')[0], bc.Knowledge().names, bindgen.load_drops())
        self.assertEqual(bc.overlay_name_errors(retired, os.path.join(kb.DB, 'builds')), [])


class HandCallerCounts(Base):
    def classify_notes(self, old_notes, new_notes, census_build='steam'):
        import census
        new = as_new(self.doc)
        new['row']['notes'] = new_notes
        if old_notes is None:
            changes = [('re/bindings/' + SAMPLE, None, raw(new))]
        else:
            old = copy.deepcopy(new)
            old['row']['notes'] = old_notes
            changes = [('re/bindings/' + SAMPLE, raw(old), raw(new))]
        with mock.patch.object(census, 'active', return_value=census_build):
            return bc.classify(changes, AUTHOR, self.fresh, self.empty)

    def test_a_new_record_cites_the_census_for_its_callers(self):
        out = self.classify_notes(None, self.doc['row']['notes'] + ' Two callers.')
        self.assertEqual(out['verdict'], 'fail')
        self.assertIn('the notes count callers by hand (Two callers): cite callers: census steam <rva>', out['errors'][0])
        self.assertIn('census.py show steam', bc.fix_for(out['errors'][0]))
        out = self.classify_notes(None, self.doc['row']['notes'] + ' callers: census steam 0x36a3ab0.')
        self.assertEqual(out['errors'], [])

    def test_a_change_may_keep_a_count_it_found_but_not_add_one(self):
        old = self.doc['row']['notes'] + ' Two callers.'
        self.assertEqual(self.classify_notes(old, old + ' More text.')['errors'], [])
        out = self.classify_notes(old, old + ' 5 call sites.')
        self.assertEqual(out['verdict'], 'fail')
        self.assertIn('(5 call sites)', out['errors'][0])

    def test_without_a_census_of_the_kb_build_counts_pass(self):
        self.assertEqual(self.classify_notes(None, self.doc['row']['notes'] + ' Two callers.', None)['errors'], [])


class CiCommand(Base):
    def test_ci_reads_only_record_paths_from_the_head_copy(self):
        tmp = tempfile.mkdtemp()
        try:
            head = os.path.join(tmp, 'head')
            os.makedirs(os.path.join(head, 're', 'bindings', 'session'))
            files = [{'filename': 're/bindings/session/site_Sess_NewThing.json', 'status': 'added'},
                     {'filename': '.github/workflows/bindings.yml', 'status': 'modified'}]
            with open(os.path.join(tmp, 'files.json'), 'w') as f:
                json.dump(files, f)
            out = os.path.join(tmp, 'out.json')
            import binding
            with contextlib.redirect_stdout(io.StringIO()):
                code = binding.main(['ci', '--files', os.path.join(tmp, 'files.json'), '--head', head, '--author',
                                     AUTHOR, '--out', out])
            with open(out) as f:
                result = json.load(f)
            self.assertNotEqual(result['verdict'], 'auto')
            self.assertIn(code, (0, 1))
        finally:
            shutil.rmtree(tmp)


class MadeOn(Classify):
    PORT = 'steam-26566308'

    def port_sha(self):
        return next(sha for sha, name in self.knowledge.builds.items() if name == self.PORT)

    def made_on(self, drop=(), **change):
        d = as_new(self.doc, proof=False)
        s, sha = d['signature'], self.port_sha()
        s.update(made_on=sha[:16], made_on_match=hex(int(s['match'], 16) + 0x2590),
                 made_on_target=hex(int(s['target'], 16) + 0x2590),
                 twin='made on steam-26566308; the target is KB 0x1436a3b83: both neighbours sit 0x2590 above',
                 verified=[{'build': sha[:16], 'date': '2026-11-17', 'by': AUTHOR, 'how': 'static'}])
        s.update(change)
        for k in drop:
            s.pop(k)
        p = make_proof(d, AUTHOR, sha)
        p['result'].update(match=s.get('made_on_match', s['match']), target=s.get('made_on_target', s['target']))
        d['proofs'] = [p]
        return d

    def classify_doc(self, d):
        return self.run_classify([('re/bindings/' + SAMPLE, None, raw(d))])

    def test_a_record_made_on_the_current_build_merges_itself(self):
        out = self.classify_doc(self.made_on())
        self.assertEqual((out['verdict'], out['errors'], out['notes']), ('auto', [], []))

    def test_its_proof_there_must_land_where_it_was_made(self):
        d = self.made_on()
        d['proofs'][0]['result']['target'] = '0x140001000'
        out = self.classify_doc(d)
        self.assertEqual(out['verdict'], 'fail')
        self.assertTrue(any('on the build the record was made on' in e for e in out['errors']), out['errors'])

    def test_the_made_on_fields_come_together(self):
        out = self.classify_doc(self.made_on(drop=('twin',)))
        self.assertEqual(out['verdict'], 'fail')
        self.assertTrue(any('signature.twin missing' in e for e in out['errors']), out['errors'])

    def test_made_on_names_a_build_other_than_the_kb_build(self):
        kb_sha = self.knowledge.build['sha256'][:16]
        out = self.classify_doc(self.made_on(made_on=kb_sha))
        self.assertTrue(any('other than the KB build' in e for e in out['errors']), out['errors'])

    def test_without_made_on_the_kb_build_must_be_verified(self):
        out = self.classify_doc(self.made_on(drop=('made_on', 'made_on_match', 'made_on_target', 'twin')))
        self.assertTrue(any('signature.verified needs an entry for the KB build' in e for e in out['errors']),
                        out['errors'])

    def test_every_made_on_error_has_a_fix(self):
        d = self.made_on(made_on_target='0x140001000')
        d['proofs'][0]['result']['target'] = '0x140001000'
        for e in self.classify_doc(d)['errors'] + self.classify_doc(self.made_on(drop=('twin',)))['errors']:
            self.assertIsNotNone(bc.fix_for(e), e)


class Fixes(Classify):
    def messages(self, out):
        return out['errors'] + out['notes']

    def scenarios(self):
        path = 're/bindings/' + SAMPLE
        no_decomp = as_new(self.doc)
        no_decomp['row']['evidence'] = 'xref:0x1436a3b74'
        no_decomp['proofs'] = [make_proof(no_decomp, AUTHOR)]
        wild = as_new(self.doc)
        wild['signature']['pattern'] = '?? ' + wild['signature']['pattern']
        moved = as_new(self.doc)
        moved['signature']['pattern'] += ' 90'
        bad_token = as_new(self.doc)
        bad_token['row']['evidence'] += ';guess:x'
        bad_token['proofs'] = [make_proof(bad_token, AUTHOR)]
        edited = as_new(self.doc)
        edited['row']['notes'] += ' more'
        root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, root)
        os.makedirs(os.path.join(root, 'native', 'src'))
        with open(os.path.join(root, 'native', 'src', 'x.cpp'), 'w') as f:
            f.write(f'resolve("{self.doc["name"]}");\n')
        yield self.run_classify([(path, None, raw(no_decomp))])
        yield self.run_classify([(path, None, raw(wild))])
        yield self.run_classify([(path, None, raw(moved))])
        yield self.run_classify([(path, None, raw(bad_token))])
        yield self.run_classify([(path, None, raw(as_new(self.doc)))], author='someone-else')
        yield self.run_classify([(path, None, raw(as_new(self.doc, proof=False)))])
        yield self.run_classify([(path, None, b'{"schema": 1, "schema": 2}')])
        yield self.run_classify([(path, None, b'not json')])
        yield self.run_classify([('re/tools/bindgen.py', None, b'x')])
        yield self.run_classify([(path, raw(self.doc), None)])
        yield self.run_classify([(path, raw(as_new(self.doc)), raw(edited))])
        yield self.run_classify([])
        yield self.run_classify([(path, None, raw(as_new(self.doc)))], root=root)
        yield self.second_proof(AUTHOR)
        yield self.second_proof('contributor-b', 'disagree')
        yield bc.classify([(path, None, raw(as_new(self.doc)))], AUTHOR, self.knowledge, self.empty)

    def test_every_message_of_these_pull_requests_has_a_fix(self):
        seen = 0
        for out in self.scenarios():
            for msg in self.messages(out):
                seen += 1
                self.assertIsNotNone(bc.fix_for(msg), msg)
        self.assertGreater(seen, 15)

    def test_a_fix_names_the_record_and_the_command(self):
        out = self.run_classify([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc)))], author='someone-else')
        fixes = [bc.fix_for(m) for m in out['notes']]
        name = self.doc['name']
        self.assertTrue(any(f'binding.py verify {name} --exe' in f and '--by someone-else' in f for f in fixes), fixes)

    def test_the_fix_follows_its_message(self):
        msg = f're/bindings/{SAMPLE}: row: L2 needs xref:'
        self.assertEqual(bc.with_fixes([msg, 'no fix for this']),
                         [msg, '  fix: ' + bc.fix_for(msg), 'no fix for this'])
        self.assertIn('xref:', bc.fix_for(msg))
        self.assertIsNone(bc.fix_for('no fix for this'))


class CiSummary(Base):
    def test_the_summary_names_the_outside_files_and_the_fixes(self):
        import binding
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp)
        files = [{'filename': 'docs/a.md', 'status': 'modified'}, {'filename': 'native/src/b.cpp', 'status': 'added'}]
        with open(os.path.join(tmp, 'files.json'), 'w') as f:
            json.dump(files, f)
        out = os.path.join(tmp, 'out.json')
        with contextlib.redirect_stdout(io.StringIO()) as printed:
            binding.main(['ci', '--files', os.path.join(tmp, 'files.json'), '--head', tmp, '--author', AUTHOR,
                          '--out', out])
        with open(out) as f:
            result = json.load(f)
        self.assertEqual(result['verdict'], 'review')
        self.assertTrue(any('docs/a.md, native/src/b.cpp' in n for n in result['notes']), result['notes'])
        self.assertTrue(any(line.startswith('  fix: ') for line in result['summary']), result['summary'])
        self.assertIn('  fix: ', printed.getvalue())

    def test_a_record_without_a_regular_file_in_the_pull_request_needs_review(self):
        import binding
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp)
        with open(os.path.join(tmp, 'files.json'), 'w') as f:
            json.dump([{'filename': 're/bindings/session/site_Sess_NewThing.json', 'status': 'added'}], f)
        out = os.path.join(tmp, 'out.json')
        with contextlib.redirect_stdout(io.StringIO()):
            binding.main(['ci', '--files', os.path.join(tmp, 'files.json'), '--head', tmp, '--author', AUTHOR,
                          '--out', out])
        with open(out) as f:
            result = json.load(f)
        self.assertEqual(result['verdict'], 'review')
        self.assertEqual(result['notes'], ['re/bindings/session/site_Sess_NewThing.json: the pull request has no '
                                           'regular file there: review needed'])

    def test_a_list_that_is_not_a_file_list_fails_with_a_summary(self):
        import binding
        tmp = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, tmp)
        with open(os.path.join(tmp, 'files.json'), 'w') as f:
            f.write('{"filename": "x"}')
        out = os.path.join(tmp, 'out.json')
        with contextlib.redirect_stdout(io.StringIO()):
            code = binding.main(['ci', '--files', os.path.join(tmp, 'files.json'), '--head', tmp, '--author', AUTHOR,
                                 '--out', out])
        with open(out) as f:
            result = json.load(f)
        self.assertEqual((code, result['verdict']), (1, 'fail'))
        self.assertEqual(result['summary'], ['error: the file list is not a GitHub API file list'])


class HeadersFollowTheExport(Base):
    def judge(self, new, other=()):
        import binding
        path = 're/bindings/' + SAMPLE
        return binding.judge([(path, raw(self.doc), raw(new))], list(other), 'contributor-b', self.knowledge)

    def agreed(self):
        doc = copy.deepcopy(self.doc)
        doc['verify'] = doc['verify'] + [agree_row(doc, 'contributor-b')]
        doc['proofs'] = doc['proofs'] + [make_proof(doc, 'contributor-b')]
        return doc

    def test_an_agree_row_that_exports_a_binding_asks_for_the_headers(self):
        result = self.judge(self.agreed())
        self.assertEqual(result['verdict'], 'review', result['summary'])
        notes = [n for n in result['notes'] if 'change what bindgen writes' in n]
        self.assertEqual(len(notes), 1, result['notes'])
        self.assertIn('native/include/swofty/game.rs', notes[0])
        self.assertIn('native/include/swofty/game/session.hpp', notes[0])
        fix = result['summary'][result['summary'].index(notes[0]) + 1]
        self.assertIn('re/tools/bindgen.py --no-exe', fix)

    def test_carrying_the_headers_is_a_review_of_the_headers_alone(self):
        result = self.judge(self.agreed(), ['native/include/swofty/game.rs', 'native/include/swofty/game/session.hpp'])
        self.assertEqual(result['verdict'], 'review')
        self.assertFalse(any('change what bindgen writes' in n for n in result['notes']), result['notes'])
        self.assertFalse(any('outside re/bindings' in n for n in result['notes']), result['notes'])
        self.assertTrue(any('changes the generated headers' in n for n in result['notes']), result['notes'])

    def test_a_record_that_stays_unexported_changes_no_header(self):
        import binding
        self.assertEqual(binding.header_changes([('re/bindings/' + SAMPLE, raw(as_new(self.doc)))]), [])

    def test_the_store_is_put_back(self):
        import binding
        store = br.Store.get()
        before = store.records
        binding.header_changes([('re/bindings/' + SAMPLE, raw(self.agreed()))])
        self.assertIs(store.records, before)


class Doctor(Base):
    BASE = 'f' * 40

    def doctor(self, changes, other=(), *args, lost=()):
        import binding
        store = br.Store.get()
        before = store.records
        with mock.patch.object(binding, 'pick_base', return_value=self.BASE), \
                mock.patch.object(binding, 'doctor_changes', return_value=(list(changes), list(other))), \
                mock.patch.object(binding, 'lost_exports', return_value=list(lost)) as floor, \
                contextlib.redirect_stdout(io.StringIO()) as out:
            code = binding.main(['doctor', *args])
        self.assertIs(store.records, before)
        floor.assert_called_once_with(self.BASE)
        return code, out.getvalue()

    def test_a_binding_the_base_exports_that_stops_exporting_fails(self):
        code, out = self.doctor([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc)))], lost=['Dropped'])
        self.assertEqual(code, 1, out)
        self.assertIn('Dropped', out)

    def test_a_new_record_would_merge_itself(self):
        code, out = self.doctor([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc)))])
        self.assertEqual(code, 0, out)
        self.assertIn('pull request by contributor-a', out)
        self.assertIn('verdict: auto: this pull request would merge itself', out)
        self.assertIn(f'{self.doc["name"]}: ', out.split('after the merge:')[1])

    def test_a_review_says_why_and_names_the_other_files(self):
        old = as_new(self.doc)
        new = copy.deepcopy(old)
        new['row']['notes'] += ' more'
        code, out = self.doctor([('re/bindings/' + SAMPLE, raw(old), raw(new))], ['docs/a.md'], '--author', AUTHOR)
        self.assertEqual(code, 0, out)
        self.assertIn('verdict: review', out)
        self.assertIn('changes an existing record', out)
        self.assertIn('(docs/a.md)', out)
        self.assertIn('    fix: ', out)

    def test_a_failure_prints_each_error_with_its_fix(self):
        d = as_new(self.doc)
        d['row']['evidence'] = 'xref:0x1436a3b74'
        d['proofs'] = [make_proof(d, AUTHOR)]
        code, out = self.doctor([('re/bindings/' + SAMPLE, None, raw(d))])
        self.assertEqual(code, 1, out)
        self.assertIn('verdict: fail', out)
        self.assertIn(f'- error: re/bindings/{SAMPLE}: row: L2 needs decomp:', out)
        self.assertIn('    fix: add the missing decomp: token to --evidence', out)

    def test_records_by_two_authors_need_the_author(self):
        other = as_new(self.doc, author='contributor-b')
        with self.assertRaises(SystemExit) as e:
            self.doctor([('re/bindings/' + SAMPLE, None, raw(as_new(self.doc))),
                         ('re/bindings/session/site_Sess_Other.json', None, raw(other))])
        self.assertIn('pass --author', str(e.exception))
        self.assertIn('contributor-a, contributor-b', str(e.exception))

    def test_local_changes_include_commits_edits_deletions_and_new_files(self):
        import binding
        repo = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, repo)

        def run(*args):
            subprocess.run(['git', '-C', repo, '-c', 'user.email=t@t', '-c', 'user.name=t', *args], check=True,
                           capture_output=True)
        run('init', '-q')
        for name in ('kept', 'edited', 'gone', 'committed'):
            with open(os.path.join(repo, name), 'w') as f:
                f.write(name)
        run('add', '-A')
        run('commit', '-qm', 'base')
        base = subprocess.run(['git', '-C', repo, 'rev-parse', 'HEAD'], capture_output=True).stdout.decode().strip()
        with open(os.path.join(repo, 'committed'), 'a') as f:
            f.write('2')
        run('commit', '-qam', 'head')
        with open(os.path.join(repo, 'edited'), 'a') as f:
            f.write('2')
        os.remove(os.path.join(repo, 'gone'))
        with open(os.path.join(repo, 'untracked'), 'w') as f:
            f.write('new')
        with mock.patch.object(binding.kb, 'ROOT', repo):
            got = binding.local_changes(base)
        self.assertEqual(got, [('committed', 'modified'), ('edited', 'modified'), ('gone', 'removed'),
                               ('untracked', 'added')])


class CheckAndStatus(Base):
    def test_check_counts_the_files_it_was_given(self):
        import binding
        with contextlib.redirect_stdout(io.StringIO()) as out:
            code = binding.main(['check', os.path.join(br.RECORDS, SAMPLE), os.path.join(br.RECORDS, 'x', 'Nope.json')])
        self.assertEqual(code, 1)
        self.assertIn('Nope.json: no such file', out.getvalue())
        self.assertRegex(out.getvalue(), r'binding\.py check: 2 of \d+ record\(s\) checked, 1 error\(s\)')

    def test_status_takes_names(self):
        import binding
        with contextlib.redirect_stdout(io.StringIO()) as out:
            binding.main(['status', self.doc['name']])
        lines = out.getvalue().splitlines()
        self.assertEqual((len(lines), lines[-1]), (3, '1 record(s)'))
        self.assertTrue(lines[1].startswith(self.doc['name']))

    def test_status_of_an_unknown_name_says_where_records_are(self):
        import binding
        with self.assertRaises(SystemExit) as e:
            binding.main(['status', 'Nope_Not_There'])
        self.assertIn('binding.py status lists them', str(e.exception))


class ExportsDoNotShrink(Base):
    @classmethod
    def setUpClass(cls):
        super().setUpClass()
        import binding, bindgen
        gate = bindgen.Gate()
        cls.exported = next(r for r in br.Store.get().records
                            if r.table == 'functions' and binding.export_state(gate, r)[0] == 'experimental')

    def change(self, verdict):
        doc = copy.deepcopy(self.exported.doc)
        row = agree_row(doc, 'contributor-b')
        row.update(kind='func', verdict=verdict, date=max(v['date'] for v in doc['verify']))
        doc['verify'].append(row)
        doc['proofs'].append(make_proof(doc, 'contributor-b'))
        return doc

    def judge(self, new):
        import binding
        path = 're/bindings/' + self.exported.relpath
        return binding.judge([(path, raw(self.exported.doc), new and raw(new))], [], 'contributor-b', self.knowledge)

    def test_a_newer_partial_row_on_an_exported_binding_fails(self):
        result = self.judge(self.change('partial'))
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        errors = [e for e in result['errors'] if 'stops exporting' in e]
        self.assertEqual(len(errors), 1, result['errors'])
        self.assertIn(self.exported.name, errors[0])
        fix = result['summary'][result['summary'].index('error: ' + errors[0]) + 1]
        self.assertIn('addon API', fix)

    def test_removing_an_exported_record_fails(self):
        result = self.judge(None)
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any('removes ' + self.exported.name in e for e in result['errors']), result['errors'])

    def test_a_newer_agree_row_keeps_it_exported(self):
        result = self.judge(self.change('agree'))
        self.assertFalse(any('stops exporting' in e for e in result['errors']), result['errors'])

    def test_lost_exports_lists_what_the_base_exports_and_the_checkout_does_not(self):
        import binding
        with mock.patch.object(binding, 'bindgen_ids', side_effect=[{'Kept', 'Dropped'}, {'Kept', 'Added'}]):
            self.assertEqual(binding.lost_exports('HEAD'), ['Dropped'])

    def test_the_checkout_exports_everything_its_commit_does(self):
        import binding
        self.assertEqual(binding.lost_exports('HEAD'), [])


class ExportDropsNeedTheOwner(ExportsDoNotShrink):
    PATH = 're/db/export_drops.json'

    def setUp(self):
        import bindgen
        at = bindgen.workspace_version()
        self.next = f'{at[0]}.{at[1]}.{at[2] + 1}'
        self.shipped = '.'.join(map(str, at))
        self.key = f'func:{self.exported.name}'

    def ddoc(self, entries):
        return json.dumps({'schema': 1, 'note': 'why', 'entries': entries}).encode()

    def entry(self, mode='deprecated', covers='V109991', **kw):
        return {'mode': mode, 'since': self.next, 'covers': covers, 'reason': 'its proto is wrong', **kw}

    def partial(self, vid='V109991'):
        doc = self.change('partial')
        doc['verify'][-1].update(verifier='re-bindings-blind', notes=f'{vid} Blind: partial, the fourth argument is a u8')
        return doc

    def judge_drops(self, new, entries, old_entries=None, core=(), other=()):
        import binding
        from unittest import mock
        path = 're/bindings/' + self.exported.relpath
        pair = (self.ddoc(old_entries or {}), self.ddoc(entries))
        core = set(core)
        with mock.patch.object(binding, 'core_names', side_effect=lambda n, root=None: 'native/src/core/x.cpp'
                               if n in core else None):
            return binding.judge([(path, raw(self.exported.doc), new and raw(new))], [self.PATH, *other],
                                 'contributor-b', self.knowledge, None, pair)

    def test_a_binding_core_names_needs_a_replacement_to_be_deprecated(self):
        core = {self.exported.name}
        result = self.judge_drops(self.partial(), {self.key: self.entry()}, core=core,
                                  other=['native/src/core/x.cpp'])
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any('without a replacement' in e for e in result['errors']), result['errors'])

    def test_core_moves_off_a_binding_in_the_change_that_takes_it_out(self):
        core = {self.exported.name}
        moved = self.entry(replacement='Example_Other')
        result = self.judge_drops(self.partial(), {self.key: moved}, core=core)
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any('core moves off it to Example_Other in the same change' in e for e in result['errors']),
                        result['errors'])
        crash = self.judge_drops(self.partial(), {self.key: self.entry('dropped', harm='crash')}, core=core)
        self.assertTrue(any('core moves off it in the same change' in e for e in crash['errors']), crash['errors'])
        both = self.judge_drops(self.partial(), {self.key: moved}, core=core, other=['native/src/core/x.cpp'])
        self.assertEqual(both['verdict'], 'review', both['summary'])
        self.assertFalse(any('core names' in e for e in both['errors']), both['errors'])

    def test_core_names_skips_comments_and_reads_resolve_strings_and_types(self):
        import binding, tempfile
        with tempfile.TemporaryDirectory() as d:
            os.makedirs(os.path.join(d, 'native', 'src', 'core'))
            with open(os.path.join(d, 'native', 'src', 'core', 'a.cpp'), 'w') as f:
                f.write('// Example_InLineComment\n/* Example_InBlock\n spans */ int x;\n'
                        'auto f = api->resolve("Example_Resolved"); game::Example_Typed_t g; '
                        'auto h = game::Example_Constant; // trailing Example_After\n'
                        'const char *u = "http://example//Example_InUrl";\n')
            with open(os.path.join(d, 'native', 'src', 'core', 'b.md'), 'w') as f:
                f.write('Example_NotSource')
            for n in ('Example_Resolved', 'Example_Typed', 'Example_Constant', 'Example_InUrl'):
                self.assertEqual(binding.core_names(n, d), os.path.join('native', 'src', 'core', 'a.cpp'), n)
            for n in ('Example_InLineComment', 'Example_InBlock', 'Example_After', 'Example_NotSource', 'Example'):
                self.assertIsNone(binding.core_names(n, d), n)
            self.assertTrue(bc.referenced_in_code('Example_After', d, ('native/src',)))

    def test_a_deprecated_entry_keeps_the_export_and_needs_the_owner(self):
        result = self.judge_drops(self.partial(), {self.key: self.entry()})
        self.assertEqual(result['verdict'], 'review', result['summary'])
        self.assertTrue(result['owner_review'])
        self.assertTrue(any('only the repository owner approves' in n for n in result['notes']), result['notes'])
        self.assertTrue(any(f'deprecates {self.key} in {self.next}' in n for n in result['notes']), result['notes'])

    def test_a_dropped_entry_with_harm_takes_it_out_at_once(self):
        result = self.judge_drops(self.partial(), {self.key: self.entry('dropped', harm='crash')})
        self.assertEqual(result['verdict'], 'review', result['summary'])
        self.assertTrue(any('leaves the addon API' in n for n in result['notes']), result['notes'])
        self.assertFalse(any('stops exporting' in e for e in result['errors']), result['errors'])

    def test_a_drop_without_harm_or_a_released_deprecation_fails(self):
        result = self.judge_drops(self.partial(), {self.key: self.entry('dropped', deprecated=self.shipped)})
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any('without a released deprecation' in e for e in result['errors']), result['errors'])

    def test_a_drop_after_a_released_deprecation_passes(self):
        import binding
        old = self.ddoc({self.key: dict(self.entry(), since=self.shipped)})
        self.assertEqual(binding.drops_review(old, {('func', self.exported.name): self.entry(
            'dropped', deprecated=self.shipped)}), [])
        unreleased = self.ddoc({self.key: self.entry()})
        later = dict(self.entry('dropped', deprecated=self.next), since=f'{self.next[:-1]}9')
        self.assertTrue(binding.drops_review(unreleased, {('func', self.exported.name): later}))

    def test_an_entry_must_ship_in_a_later_release(self):
        result = self.judge_drops(self.partial(), {self.key: dict(self.entry(), since=self.shipped)})
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any(f'ships in {self.shipped}' in e for e in result['errors']), result['errors'])

    def test_an_entry_that_does_not_cover_the_newest_row_fails(self):
        result = self.judge_drops(self.partial('V109992'), {self.key: self.entry()})
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any('no longer holds' in e for e in result['errors']), result['errors'])
        self.assertTrue(any('stops exporting' in e for e in result['errors']), result['errors'])

    def test_a_malformed_file_fails(self):
        result = self.judge_drops(self.partial(), {self.key: self.entry('gone')})
        self.assertEqual(result['verdict'], 'fail', result['summary'])

    def test_a_record_change_on_a_listed_binding_keeps_its_entry_holding(self):
        import binding, bindgen
        listed = {('func', self.exported.name): self.entry()}
        with mock.patch.object(bindgen, 'load_drops', return_value=listed):
            result = self.judge(self.partial('V109993'))
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any('no longer holds' in e for e in result['errors']), result['errors'])
        self.assertFalse(result['owner_review'])

    def test_ci_flags_the_owner_review(self):
        import binding
        tmp = tempfile.mkdtemp()
        try:
            head = os.path.join(tmp, 'head')
            os.makedirs(os.path.join(head, 're', 'db'))
            with open(os.path.join(head, self.PATH), 'wb') as f:
                f.write(self.ddoc({}))
            with open(os.path.join(tmp, 'files.json'), 'w') as f:
                json.dump([{'filename': self.PATH, 'status': 'modified'}], f)
            out = os.path.join(tmp, 'out.json')
            with contextlib.redirect_stdout(io.StringIO()):
                binding.main(['ci', '--files', os.path.join(tmp, 'files.json'), '--head', head, '--author', AUTHOR,
                              '--out', out])
            with open(out) as f:
                result = json.load(f)
        finally:
            shutil.rmtree(tmp)
        self.assertEqual(result['verdict'], 'review', result['summary'])
        self.assertTrue(result['owner_review'])

    def test_lost_exports_spares_the_held_drops(self):
        import binding
        with mock.patch.object(binding, 'bindgen_ids', side_effect=[{'Kept', 'Dropped', 'Gone'}, {'Kept'}]), \
                mock.patch.object(binding, 'held_drops', return_value={'Dropped'}):
            self.assertEqual(binding.lost_exports('HEAD'), ['Gone'])

    def test_release_lines(self):
        import binding
        self.assertEqual(binding.release_line('Fn_A', self.entry(replacement='Fn_B')),
                         '- The addon binding `Fn_A` is deprecated and leaves the addon API in the next release: its '
                         'proto is wrong. Use `Fn_B` instead.')
        self.assertEqual(binding.release_line('Fn_A', self.entry('dropped', harm='crash')),
                         '- The addon binding `Fn_A` is removed from the addon API: its proto is wrong.')


class LevelBaselineOnlyShrinks(Base):
    PATH = 're/db/level_baseline.json'

    def bdoc(self, entries):
        return json.dumps({'schema': 1, 'note': 'why', 'entries': entries}).encode()

    def test_adding_or_moving_an_entry_is_an_error_and_removing_one_is_not(self):
        import binding
        old = self.bdoc({'func:A': 'V1', 'func:B': 'V2'})
        self.assertEqual(binding.baseline_growth(old, self.bdoc({'func:A': 'V1'})), [])
        self.assertEqual(binding.baseline_growth(old, None), [])
        grown = binding.baseline_growth(old, self.bdoc({'func:A': 'V1', 'func:B': 'V2', 'field:T.x': 'V3'}))
        self.assertEqual(len(grown), 1, grown)
        self.assertIn('adds field:T.x', grown[0])
        moved = binding.baseline_growth(old, self.bdoc({'func:A': 'V7', 'func:B': 'V2'}))
        self.assertEqual(len(moved), 1, moved)
        self.assertIn('moves func:A from V1 to V7', moved[0])
        self.assertEqual(len(binding.baseline_growth(None, self.bdoc({'func:A': 'V1'}))), 1)
        self.assertEqual(len(binding.baseline_growth(old, b'{"schema": 2}')), 1)

    def test_judge_fails_a_pull_request_that_grows_the_baseline(self):
        import binding
        old = self.bdoc({'func:A': 'V1'})
        result = binding.judge([], [self.PATH], AUTHOR, self.knowledge, (old, self.bdoc({'func:A': 'V1', 'func:B': 'V2'})))
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        result = binding.judge([], [self.PATH], AUTHOR, self.knowledge, (old, self.bdoc({})))
        self.assertEqual(result['verdict'], 'review', result['summary'])

    def ci(self, head_doc):
        import binding
        tmp = tempfile.mkdtemp()
        try:
            head = os.path.join(tmp, 'head')
            os.makedirs(os.path.join(head, 're', 'db'))
            with open(os.path.join(head, self.PATH), 'wb') as f:
                f.write(head_doc)
            with open(os.path.join(tmp, 'files.json'), 'w') as f:
                json.dump([{'filename': self.PATH, 'status': 'modified'}], f)
            out = os.path.join(tmp, 'out.json')
            with contextlib.redirect_stdout(io.StringIO()):
                binding.main(['ci', '--files', os.path.join(tmp, 'files.json'), '--head', head, '--author', AUTHOR,
                              '--out', out])
            with open(out) as f:
                return json.load(f)
        finally:
            shutil.rmtree(tmp)

    def test_ci_fails_a_grown_baseline_and_reviews_a_shrunk_one(self):
        import binding
        with open(os.path.join(binding.kb.ROOT, self.PATH)) as f:
            doc = json.load(f)
        grown = copy.deepcopy(doc)
        grown['entries']['func:Sess_Not_Listed_Before'] = 'V1'
        result = self.ci(json.dumps(grown).encode())
        self.assertEqual(result['verdict'], 'fail', result['summary'])
        self.assertTrue(any('adds func:Sess_Not_Listed_Before' in e for e in result['errors']), result['errors'])
        shrunk = copy.deepcopy(doc)
        shrunk['entries'].pop(next(iter(shrunk['entries'])))
        self.assertEqual(self.ci(json.dumps(shrunk).encode())['verdict'], 'review')


@NEEDS_EXE_MODULES
@unittest.skipUnless(os.environ.get('SWOFTY_STEAM_EXE'), 'set SWOFTY_STEAM_EXE to the KB build F1_25.exe')
class NewOnTheKbBuild(unittest.TestCase):
    TWIN = 0x142e9dc20

    def test_a_function_whose_bytes_repeat_gets_a_call_signature(self):
        import binding, sigcheck
        exe = os.environ['SWOFTY_STEAM_EXE']
        img = sigcheck.Image(exe)
        self.assertIsNone(sigcheck.make(img, self.TWIN, 'func_start', None))
        site, sig = binding.call_candidate(img, exe, self.TWIN)
        self.assertEqual((sig['kind'], sig['resolve']['kind'], sig['target']), ('call', 'rel32', hex(self.TWIN)))
        start = int(sig['match'], 16)
        self.assertEqual(start + int(sig['resolve']['insn'], 16), site)
        self.assertEqual(img.read(site, 1), b'\xe8')



class NewOnAnotherBuild(unittest.TestCase):
    def test_an_unknown_build_is_refused_with_the_builds_that_work(self):
        import binding
        with self.assertRaises(SystemExit) as e:
            binding.port_exe_or_exit('F1_25.exe', {'sha256': 'f' * 64, 'size': 1, 'pe_timestamp': 2}, 'new')
        self.assertIn('1.0.141.7865', str(e.exception))
        self.assertIn('re/db/builds', str(e.exception))

    def test_only_builds_other_than_the_kb_build_are_ports(self):
        import binding, kb
        self.assertIsNone(binding.build_name(kb.load_signatures()['build']['sha256']))
        sha = next(sha for sha, name in bc.Knowledge().builds.items() if name == 'steam-26566308')
        self.assertEqual(binding.build_name(sha), 'steam-26566308')

    @NEEDS_EXE_MODULES
    @unittest.skipUnless(os.environ.get('SWOFTY_STEAM_145_EXE'), 'set SWOFTY_STEAM_145_EXE to the Steam 1.0.145 exe')
    def test_the_current_build_maps_an_address_to_its_kb_twin(self):
        import binding, sigcheck
        exe = os.environ['SWOFTY_STEAM_145_EXE']
        ident = binding.exe_identity(exe)
        self.assertEqual(binding.port_exe_or_exit(exe, ident, 'new'), 'steam-26566308')
        sig = sigcheck.make(sigcheck.Image(exe), 0x142e91ae0, 'func_start')
        with mock.patch.dict(os.environ, {'SWOFTY_KB_EXE': os.environ.get('SWOFTY_STEAM_EXE', '')}):
            out = binding.to_kb(sig, exe, 'steam-26566308', ident)
        self.assertEqual((out['target'], out['match'], out['section']), ('0x142e8f550', '0x142e8f550', '.xtls'))
        self.assertEqual((out['made_on'], out['made_on_match'], out['made_on_target']),
                         (ident['sha256'][:16], '0x142e91ae0', '0x142e91ae0'))
        self.assertIn('0x2590', out['twin'])
        if os.environ.get('SWOFTY_STEAM_EXE'):
            self.assertIn('checked on the KB exe: same', out['twin'])


    @NEEDS_EXE_MODULES
    @unittest.skipUnless(os.environ.get('SWOFTY_STEAM_145_EXE'), 'set SWOFTY_STEAM_145_EXE to the Steam 1.0.145 exe')
    def test_a_record_made_on_the_current_build_holds_kb_addresses(self):
        import binding
        saved = []
        with mock.patch.object(br.Record, 'save', lambda rec, *a: saved.append(rec)), \
                mock.patch.dict(os.environ, {'SWOFTY_KB_EXE': ''}), contextlib.redirect_stdout(io.StringIO()):
            binding.main(['new', 'Sess_Tyre_ProbeFunctionA', '0x142e91ae0', '--kind', 'func', '--author',
                          'contributor-a', '--exe', os.environ['SWOFTY_STEAM_145_EXE'], '--evidence',
                          'decomp:own disasm 0x142e91ae0;xref:0x142e91ae0', '--notes', 'probe'])
        rec = saved[0]
        self.assertEqual((rec.table_row()['addr'], rec.signature['target']), ('0x142e8f550', '0x142e8f550'))
        self.assertEqual(rec.signature['made_on_target'], '0x142e91ae0')
        self.assertEqual(rec.proofs[0]['result']['target'], '0x142e91ae0')

class CiLint(Base):
    def test_ci_lints_the_pull_requests_records_with_the_base(self):
        import binding
        d = as_new(self.doc)
        d['row']['evidence'] = 'xref:0x1436a3b74 decomp:x'
        errs = binding.lint_with([('re/bindings/' + SAMPLE, raw(d))])
        self.assertTrue(any('malformed' in e for e in errs), errs)
        self.assertTrue(all(SAMPLE in e for e in errs), errs)


class UnfoldKeepsBindgenOutput(unittest.TestCase):
    """A binding gives the same bindgen output from a record as from the shared tables, for every binding kind, and
    unfold writes back exactly the record and tables that folding it took apart."""

    PER_KIND = 2

    def test_round_trip(self):
        import argparse, binding, kb
        re_dir = os.path.dirname(TOOLS)
        tmp = tempfile.mkdtemp()
        cache = os.path.join(tmp, 'records-cache.json')
        if os.path.exists(br.CACHE):
            shutil.copy(br.CACHE, cache)
        try:
            def gen(root, out):
                tools = os.path.join(root, 're', 'tools')
                return subprocess.Popen([sys.executable, os.path.join(tools, 'bindgen.py'), '--no-exe', '--json',
                                         os.path.join(out, 'bindings.json'), '--symbols', os.path.join(out, 'symbols.tsv'),
                                         '--include', os.path.join(out, 'include'), '--rust', os.path.join(out, 'game.rs')],
                                        stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                        env={**os.environ, 'SWOFTY_RECORD_CACHE': cache})

            def finish(proc, out):
                _, err = proc.communicate()
                self.assertEqual(proc.returncode, 0, err.decode())
                return snapshot(out)

            def snapshot(top):
                files = {}
                for dp, _, fs in os.walk(top):
                    for f in fs:
                        with open(os.path.join(dp, f), 'rb') as fh:
                            files[os.path.relpath(os.path.join(dp, f), top)] = fh.read()
                return files

            def listing(top):
                """{path: (size, mtime)} under top. copy2 keeps both, so a file whose pair still matches its source
                was never rewritten; every other file is compared byte for byte."""
                out = {}
                for dp, dirs, fs in os.walk(top):
                    dirs[:] = [x for x in dirs if x != '__pycache__']
                    for f in fs:
                        st = os.stat(os.path.join(dp, f))
                        out[os.path.relpath(os.path.join(dp, f), top)] = (st.st_size, st.st_mtime_ns)
                return out

            def copy(name, dirs=('db', 'tools', 'tests', 'bindings')):
                root = os.path.join(tmp, name)
                for d in dirs:
                    shutil.copytree(os.path.join(re_dir, d), os.path.join(root, 're', d),
                                    ignore=shutil.ignore_patterns('__pycache__'))
                rustfmt = os.path.join(os.path.dirname(re_dir), 'rustfmt.toml')
                if os.path.exists(rustfmt):
                    shutil.copy(rustfmt, root)
                return root

            def unfold(root, names):
                with contextlib.redirect_stdout(io.StringIO()), mock.patch.dict(os.environ, {'SWOFTY_RECORD_CACHE': cache}):
                    binding.cmd_unfold(argparse.Namespace(names=names, author='kb-import', skip_unfit=True),
                                       db=os.path.join(root, 're', 'db'), root=os.path.join(root, 're', 'bindings'))

            def fold(root, names):
                db = os.path.join(root, 're', 'db')
                docs = []
                for n in names:
                    path = os.path.join(root, 're', 'bindings', records[n])
                    with open(path) as f:
                        docs.append(json.load(f))
                    os.remove(path)
                for table in ('functions', 'globals'):
                    with open(os.path.join(db, table + '.tsv'), 'a') as f:
                        for doc in (d for d in docs if d['table'] == table):
                            row = {**doc['row'], 'name': doc['name'], 'sig': doc['signature']['id']}
                            f.write('\t'.join(row[c] for c in kb.TABLES[table][0]) + '\n')
                with open(os.path.join(db, 'verify.tsv'), 'a') as f:
                    for v in (v for d in docs for v in d['verify']):
                        f.write('\t'.join(v[c] for c in kb.TABLES['verify'][0]) + '\n')
                with open(os.path.join(db, 'signatures.json')) as f:
                    sigs = json.load(f)
                sigs['signatures'] += [d['signature'] for d in docs]
                with open(os.path.join(db, 'signatures.json'), 'w') as f:
                    f.write(json.dumps(sigs, indent=1) + '\n')

            first = gen(os.path.dirname(re_dir), os.path.join(tmp, 'before'))
            outputs, again = copy('outputs'), copy('again', ('db', 'bindings'))
            records = {r.name: r.relpath for r in br.Store.get().records if r.author == 'kb-import' and not r.proofs}
            before = finish(first, os.path.join(tmp, 'before'))
            to_fold, to_unfold = {}, {}
            tabled = {r['name'] for t in ('functions', 'globals') for r in kb.read_table(t)[0] if not r.get('_record')}
            for b in json.loads(before['bindings.json'])['bindings']:
                pick = to_fold if b['id'] in records else to_unfold if b['id'] in tabled else None
                if pick is not None and sum(1 for k in pick.values() if k == b['kind']) < self.PER_KIND:
                    pick[b['id']] = b['kind']
            subsystem = {r['name']: r['subsystem'] for t in ('functions', 'globals') for r in kb.read_table(t)[0]}
            fold(outputs, list(to_fold))
            if to_unfold:
                unfold(outputs, list(to_unfold))
            second = gen(outputs, os.path.join(tmp, 'after'))
            moved = {n for n in to_unfold if os.path.exists(os.path.join(
                outputs, 're', 'bindings', br.relpath_for(subsystem[n], n)))}
            self.assertEqual({to_fold[n] for n in to_fold} | {to_unfold[n] for n in moved},
                             {'func', 'site', 'call', 'vtable', 'data'})

            fold(again, list(to_fold))
            if to_fold:
                unfold(again, list(to_fold))
            for d in ('db', 'bindings'):
                was, now = listing(os.path.join(re_dir, d)), listing(os.path.join(again, 're', d))
                self.assertEqual(sorted(now), sorted(was))
                for path, st in was.items():
                    if now[path] != st:
                        with open(os.path.join(re_dir, d, path), 'rb') as a, \
                                open(os.path.join(again, 're', d, path), 'rb') as b:
                            self.assertEqual(b.read(), a.read(), path)

            after = finish(second, os.path.join(tmp, 'after'))
            self.assertEqual(sorted(before), sorted(after))
            for k in before:
                self.assertEqual(before[k], after[k], k)
        finally:
            shutil.rmtree(tmp)

if __name__ == '__main__':
    unittest.main()
