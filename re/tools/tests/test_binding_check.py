import contextlib, copy, io, json, os, shutil, subprocess, sys, tempfile, unittest
from unittest import mock

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, TOOLS)
import binding_check as bc
import binding_record as br

SAMPLE = 'session/site_Sess_StintTyreCheckPredictCall.json'
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
        gate = bindgen.Gate.__new__(bindgen.Gate)
        gate.verdicts = {('sig', self.doc['signature']['id']): ('agree', 'V1')}
        gate.passed = set()
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
            code = binding.main(['ci', '--files', os.path.join(tmp, 'files.json'), '--head', head, '--author', AUTHOR,
                                 '--out', out])
            with open(out) as f:
                result = json.load(f)
            self.assertNotEqual(result['verdict'], 'auto')
            self.assertIn(code, (0, 1))
        finally:
            shutil.rmtree(tmp)


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

            outputs, again = copy('outputs'), copy('again', ('db', 'bindings'))
            first = gen(outputs, os.path.join(tmp, 'before'))
            records = {r.name: r.relpath for r in br.Store.get().records if r.author == 'kb-import' and not r.proofs}
            before = finish(first, os.path.join(tmp, 'before'))
            to_fold, to_unfold = {}, {}
            for b in json.loads(before['bindings.json'])['bindings']:
                pick = to_fold if b['id'] in records else to_unfold
                if sum(1 for k in pick.values() if k == b['kind']) < self.PER_KIND:
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
                was, now = snapshot(os.path.join(re_dir, d)), snapshot(os.path.join(again, 're', d))
                was = {k: v for k, v in was.items() if '__pycache__' not in k}
                self.assertEqual(sorted(now), sorted(was))
                for path in was:
                    self.assertEqual(now[path], was[path], path)

            after = finish(second, os.path.join(tmp, 'after'))
            self.assertEqual(sorted(before), sorted(after))
            for k in before:
                self.assertEqual(before[k], after[k], k)
        finally:
            shutil.rmtree(tmp)

if __name__ == '__main__':
    unittest.main()
