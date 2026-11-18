import copy, json, os, sys, tempfile, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import bindgen

KB = {'exe': 'F1_25.exe', 'sha256': 'a' * 64, 'size': 100, 'pe_timestamp': 1, 'image_base': '0x140000000'}
PATTERN = '41 56 56 57 53 48 83 EC 48 48 89 CE ?? 41'


def overlay(sha='a' * 64, stamp=1, size=100, **tables):
    return {'schema': 1, 'build': {'exe': 'F1_25.exe', 'sha256': sha, 'size': size, 'pe_timestamp': stamp,
                                   'image_base': '0x140000000', 'distribution': 'Test', 'packed': sha != 'a' * 64},
            **tables}


def binding(i, **kw):
    b = {'id': i, 'kind': 'func', 'status': 'experimental', 'level': 'L2', 'evidence': 'verify:V0001',
         'subsystem': 'core', 'build': 'a' * 64, 'sig': 'core.' + i.lower(), 'section': '.xtls', 'pattern': PATTERN,
         'anchor': 0, 'resolve': {'kind': 'addr'}, 'expect': None, 'rva': '0x1000', 'match': '0x1000'}
    b.update(kw)
    return b


BINDINGS = [
    binding('Fn_A'),
    binding('Fn_B', section='.data1'),
    binding('Fn_C', string_anchor={'string': 'anchor text', 'func_rva': '0x2000', 'func_offset': '0x10',
                                   'durable': False}),
]


class OverlayErrors(unittest.TestCase):
    def test_valid_overlays(self):
        self.assertEqual(bindgen.overlay_errors('steam', overlay(), KB), [])
        ea = overlay('b' * 64, 2, sections={'.xtls': '.rdata'},
                     overrides={'Fn_A': {'pattern': PATTERN, 'note': 'drift', 'anchor': '0x3', 'expect': 'E8'}},
                     hints={'Fn_A': {'match': '0x4000'}, 'Fn_C': {'func_rva': '0x5000'}},
                     unresolvable={'Fn_B': 'packed body'})
        self.assertEqual(bindgen.overlay_errors('ea', ea, KB), [])

    def test_build_block(self):
        doc = overlay('b' * 64, 2)
        del doc['build']['packed']
        doc['build']['extra'] = 1
        errs = bindgen.overlay_errors('ea', doc, KB)
        self.assertIn('build.packed missing', errs)
        self.assertIn('build.extra is not a build field', errs)
        self.assertIn('build.packed must be true or false', errs)

    def test_kb_build_must_agree_with_signatures(self):
        errs = bindgen.overlay_errors('steam', overlay(size=5), KB)
        self.assertEqual(errs, ['build.size 5 differs from signatures.json 100'])

    def test_kb_build_takes_no_overrides(self):
        doc = overlay(overrides={'Fn_A': {'pattern': PATTERN, 'note': 'x'}})
        self.assertIn('the KB build takes no overrides: its patterns are the signatures in signatures.json',
                      bindgen.overlay_errors('steam', doc, KB))

    def test_override_pattern_rules(self):
        doc = overlay('b' * 64, 2, overrides={'Fn_A': {'pattern': '?? 41 56'}, 'Fn_B': {'pattern': 'ZZ', 'note': 'x'},
                                              'Fn_C': {'pattern': PATTERN, 'note': 'x', 'match': '0x1'}})
        errs = bindgen.overlay_errors('ea', doc, KB)
        self.assertIn('overrides.Fn_A.note missing', errs)
        self.assertIn('overrides.Fn_A.pattern is shorter than 12 bytes', errs)
        self.assertIn('overrides.Fn_A.pattern starts or ends with a wildcard', errs)
        self.assertIn("overrides.Fn_B.pattern bad pattern token 'ZZ'", errs)
        self.assertIn('overrides.Fn_C.match is not an override field', errs)

    def test_hints_and_unresolvable(self):
        doc = overlay('b' * 64, 2, hints={'Fn_A': {'match': 4096}, 'Fn_B': {'rva': '0x1'}},
                      unresolvable={'Fn_A': 'gone', 'Fn_C': ''})
        errs = bindgen.overlay_errors('ea', doc, KB)
        self.assertIn('hints.Fn_A.match must be a 0x hex string', errs)
        self.assertIn('hints.Fn_B must hold match and/or func_rva', errs)
        self.assertIn('Fn_A is unresolvable and also has an override or a hint', errs)
        self.assertIn('unresolvable.Fn_C needs a reason', errs)

    def test_unknown_key_and_name(self):
        doc = overlay('b' * 64, 2, targets={})
        self.assertEqual(bindgen.overlay_errors('EA', doc, KB),
                         ['file name must be lowercase letters, digits, - or _', "unknown key 'targets'"])


class LoadOverlays(unittest.TestCase):
    def write(self, d, name, doc):
        with open(os.path.join(d, name + '.json'), 'w') as f:
            json.dump(doc, f)

    def test_needs_one_kb_overlay(self):
        with tempfile.TemporaryDirectory() as d:
            self.write(d, 'ea', overlay('b' * 64, 2))
            with self.assertRaises(SystemExit):
                bindgen.load_overlays(KB, d)
            self.write(d, 'steam', overlay())
            self.assertEqual(bindgen.load_overlays(KB, d)[0], 'steam')

    def test_distinct_timestamps(self):
        with tempfile.TemporaryDirectory() as d:
            self.write(d, 'steam', overlay())
            self.write(d, 'ea', overlay('b' * 64, 1))
            with self.assertRaises(SystemExit):
                bindgen.load_overlays(KB, d)

    def test_invalid_overlay_fails(self):
        with tempfile.TemporaryDirectory() as d:
            self.write(d, 'steam', overlay())
            self.write(d, 'ea', overlay('b' * 64, 2, hints={'Fn_A': {}}))
            with self.assertRaises(SystemExit):
                bindgen.load_overlays(KB, d)


class BuildSet(unittest.TestCase):
    def test_kb_set_keeps_every_binding_field(self):
        doc = bindgen.build_set(overlay(), copy.deepcopy(BINDINGS), True)
        self.assertEqual(doc['bindings'], BINDINGS)
        self.assertEqual(list(doc), ['schema', 'generator', 'build', 'bindings'])
        self.assertEqual(doc['build'], {**KB, 'distribution': 'Test', 'packed': False})

    def test_kb_hint_replaces_signature_match(self):
        doc = bindgen.build_set(overlay(hints={'Fn_A': {'match': '0X10A0'}}), BINDINGS, True)
        self.assertEqual(doc['bindings'][0]['match'], '0x10a0')
        self.assertEqual(doc['bindings'][0]['rva'], '0x1000')

    def test_other_build(self):
        ea = overlay('b' * 64, 2, sections={'.xtls': '.rdata'},
                     overrides={'Fn_A': {'pattern': '48 89 5C 24 08 57 48 83 EC 20 8B FA', 'note': 'drift',
                                         'anchor': '0x4', 'resolve': {'kind': 'rel32', 'insn': '0x3'},
                                         'expect': 'E8', 'section': '.text'}},
                     hints={'Fn_A': {'match': '0x4000'}, 'Fn_C': {'func_rva': '0x5000'}},
                     unresolvable={'Fn_B': 'packed body'})
        doc = bindgen.build_set(ea, BINDINGS, False)
        a, b, c = doc['bindings']
        self.assertTrue(doc['build']['packed'])
        self.assertEqual(list(doc), ['schema', 'generator', 'build', 'sections', 'bindings'])
        self.assertEqual(doc['sections'], {'.xtls': '.rdata'})
        self.assertEqual(a['pattern'], '48 89 5C 24 08 57 48 83 EC 20 8B FA')
        self.assertEqual((a['anchor'], a['resolve'], a['expect'], a['section']), (4, {'kind': 'rel32', 'insn': 3}, 'E8', '.text'))
        self.assertEqual(a['match'], '0x4000')
        self.assertNotIn('rva', a)
        self.assertEqual((b['status'], b['reason'], b['pattern'], b['section']), ('unresolvable', 'packed body', PATTERN, '.data1'))
        self.assertNotIn('match', b)
        self.assertEqual(c['section'], '.rdata')
        self.assertEqual(c['string_anchor']['func_rva'], '0x5000')
        self.assertTrue(all(e['build'] == 'b' * 64 for e in doc['bindings']))
        self.assertEqual(BINDINGS[2]['string_anchor']['func_rva'], '0x2000')

    def test_other_build_drops_kb_func_rva(self):
        doc = bindgen.build_set(overlay('b' * 64, 2), BINDINGS, False)
        self.assertNotIn('func_rva', doc['bindings'][2]['string_anchor'])
        self.assertEqual(doc['bindings'][2]['string_anchor']['func_offset'], '0x10')

    def test_a_binding_made_on_another_build_hints_only_that_builds_set(self):
        made = [dict(copy.deepcopy(BINDINGS[0]), made_on={'build': 'b' * 16, 'match': '0x4400'})]
        kb_doc = bindgen.build_set(overlay(), made, True)
        port = bindgen.build_set(overlay('b' * 64, 2), made, False)
        other = bindgen.build_set(overlay('c' * 64, 3), made, False)
        hinted = bindgen.build_set(overlay('b' * 64, 2, hints={'Fn_A': {'match': '0x4800'}}), made, False)
        self.assertEqual(kb_doc['bindings'][0]['match'], BINDINGS[0]['match'])
        self.assertEqual(port['bindings'][0]['match'], '0x4400')
        self.assertNotIn('match', other['bindings'][0])
        self.assertEqual(hinted['bindings'][0]['match'], '0x4800')
        self.assertFalse(any('made_on' in d['bindings'][0] for d in (kb_doc, port, other, hinted)))

    def test_func_rva_hint_needs_an_anchor(self):
        with self.assertRaises(SystemExit):
            bindgen.build_set(overlay('b' * 64, 2, hints={'Fn_A': {'func_rva': '0x1'}}), BINDINGS, False)

    def test_unknown_ids(self):
        ea = overlay('b' * 64, 2, hints={'Gone': {'match': '0x1'}}, unresolvable={'Fn_A': 'x', 'Old': 'y'})
        self.assertEqual(bindgen.unknown_ids(ea, BINDINGS), ['Gone', 'Old'])

    def test_set_paths(self):
        self.assertEqual(bindgen.set_path('/d/bindings.json', 'steam', True), '/d/bindings.json')
        self.assertEqual(bindgen.set_path('/d/bindings.json', 'ea', False), '/d/bindings.ea.json')


def vrow(date, kind, item, verdict, vid):
    return {'date': date, 'kind': kind, 'item': item, 'verdict': verdict, 'notes': f'{vid} text'}


class GateOrder(unittest.TestCase):
    KEYS = [('func', 'Sess_Do_Thing'), ('func', '0x140001000')]

    def gate(self, rows, exceptions=None):
        return bindgen.Gate(rows=rows, exceptions=exceptions or {})

    def test_a_newer_partial_on_the_address_outranks_an_older_agree_on_the_name(self):
        g = self.gate([vrow('2026-10-28', 'func', 'Sess_Do_Thing', 'agree', 'V1'),
                       vrow('2026-10-31', 'func', '0x140001000', 'partial', 'V2')])
        self.assertEqual(g('L2', '-', self.KEYS), (None, 'L2 whose newest verify row is partial'))

    def test_a_newer_agree_on_either_key_exports(self):
        g = self.gate([vrow('2026-10-31', 'func', '0x140001000', 'partial', 'V2'),
                       vrow('2026-11-17', 'func', 'Sess_Do_Thing', 'agree', 'V3')])
        self.assertEqual(g('L2', '-', self.KEYS), ('experimental', 'verify:V3'))

    def test_dates_beat_file_order(self):
        g = self.gate([vrow('2026-11-01', 'func', 'Sess_Do_Thing', 'disagree', 'V9'),
                       vrow('2026-10-01', 'func', 'Sess_Do_Thing', 'agree', 'V10')])
        self.assertEqual(g('L2', '-', self.KEYS)[0], None)

    def test_on_one_date_the_later_row_wins(self):
        g = self.gate([vrow('2026-11-01', 'func', '0x140001000', 'agree', 'V4'),
                       vrow('2026-11-01', 'func', 'Sess_Do_Thing', 'partial', 'V5')])
        self.assertEqual(g('L2', '-', self.KEYS)[0], None)
        g = self.gate([vrow('2026-11-01', 'func', 'Sess_Do_Thing', 'partial', 'V5'),
                       vrow('2026-11-01', 'func', '0x140001000', 'agree', 'V6')])
        self.assertEqual(g('L2', '-', self.KEYS), ('experimental', 'verify:V6'))

    def test_no_row_and_low_levels(self):
        g = self.gate([])
        self.assertEqual(g('L2', '-', self.KEYS), (None, 'L2 without an agree verify row'))
        self.assertEqual(g('L1', '-', self.KEYS), (None, 'L1'))


class GateExceptions(unittest.TestCase):
    ROWS = [vrow('2026-10-28', 'func', 'Sess_Do_Thing', 'agree', 'V1'),
            vrow('2026-10-31', 'func', '0x140001000', 'partial', 'V2')]
    KEYS = GateOrder.KEYS

    def entry(self, covers='V2'):
        return {('func', 'Sess_Do_Thing'): {'kind': 'func', 'covers': covers, 'reason': 'native hook; offsets agree'}}

    def test_a_listed_exception_exports_and_names_the_row_it_covers(self):
        g = bindgen.Gate(rows=self.ROWS, exceptions=self.entry())
        self.assertEqual(g('L2', '-', self.KEYS), ('experimental', 'exception:V2'))
        self.assertEqual(g.excepted, {('func', 'Sess_Do_Thing')})

    def test_a_newer_row_ends_the_exception(self):
        rows = self.ROWS + [vrow('2026-11-17', 'func', 'Sess_Do_Thing', 'disagree', 'V3')]
        g = bindgen.Gate(rows=rows, exceptions=self.entry())
        self.assertEqual(g('L2', '-', self.KEYS)[0], None)
        self.assertEqual(g.excepted, set())

    def test_an_exception_does_not_lift_low_levels(self):
        g = bindgen.Gate(rows=self.ROWS, exceptions=self.entry())
        self.assertEqual(g('L1', '-', self.KEYS), (None, 'L1'))

    def write(self, doc):
        d = tempfile.mkdtemp()
        p = os.path.join(d, 'export_exceptions.json')
        with open(p, 'w') as f:
            json.dump(doc, f)
        return p

    def test_the_file_format(self):
        ok = {'kind': 'func', 'covers': 'V2', 'reason': 'why'}
        p = self.write({'schema': 1, 'exceptions': {'Sess_Do_Thing': ok}})
        self.assertEqual(bindgen.load_exceptions(p), {('func', 'Sess_Do_Thing'): ok})
        self.assertEqual(bindgen.load_exceptions(os.path.join(os.path.dirname(p), 'absent.json')), {})
        for bad in ({'schema': 2, 'exceptions': {}}, {'schema': 1},
                    {'schema': 1, 'exceptions': {'X_Y': {**ok, 'reason': ' '}}},
                    {'schema': 1, 'exceptions': {'X_Y': {**ok, 'covers': '2'}}},
                    {'schema': 1, 'exceptions': {'X_Y': {**ok, 'kind': 'field'}}},
                    {'schema': 1, 'exceptions': {'X_Y': {**ok, 'extra': 1}}}):
            with self.assertRaises(SystemExit, msg=bad):
                bindgen.load_exceptions(self.write(bad))


class RepoData(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.gate = bindgen.Gate()
        cls.build, cls.bindings, cls.structs, _ = bindgen.load(cls.gate)
        cls.kb_name, cls.overlays = bindgen.load_overlays(cls.build)

    def test_every_export_exception_is_in_use_and_named_by_native_code(self):
        import binding_check
        for key, e in self.gate.exceptions.items():
            self.assertIn(key, self.gate.excepted, f'{key[1]}: its newest verify row is no longer {e["covers"]} or it '
                                                   'is agree now; remove it from re/db/export_exceptions.json')
            self.assertTrue(binding_check.referenced_in_code(key[1], bindgen.ROOT, ('native/src',)),
                            f'{key[1]}: only bindings native/src names may be listed')

    REGENERATE = ('stale; run python3 re/tools/bindgen.py --no-exe --json <scratch>/bindings.json '
                  '--symbols <scratch>/symbols.tsv and commit native/include')

    def stale(self, outputs, extra=()):
        def text(p):
            if not os.path.exists(p):
                return None
            with open(p, encoding='utf-8') as f:
                return f.read()
        return sorted(os.path.relpath(p, bindgen.ROOT) for p, want in outputs.items() if text(p) != want) + \
            sorted(os.path.relpath(p, bindgen.ROOT) for p in extra)

    def test_the_tracked_headers_are_what_bindgen_writes(self):
        outputs = {os.path.normpath(os.path.join(bindgen.OUT_INC, k)): v
                   for k, v in bindgen.gen_cpp(self.build, self.bindings, self.structs).items()}
        gdir = os.path.join(bindgen.OUT_INC, 'game')
        extra = {os.path.normpath(os.path.join(gdir, f)) for f in os.listdir(gdir) if f.endswith('.hpp')} - set(outputs)
        self.assertEqual(self.stale(outputs, extra), [], self.REGENERATE)

    def test_the_tracked_game_rs_is_what_bindgen_writes(self):
        rust, formatted = bindgen.rustfmt(bindgen.gen_rust(self.build, self.bindings, self.structs))
        if not formatted:
            self.skipTest('rustfmt is not installed; bindgen writes game.rs through it')
        self.assertEqual(self.stale({bindgen.OUT_RS: rust}), [], self.REGENERATE)

    def test_every_set_has_a_pattern_for_every_binding(self):
        for name, ov in self.overlays.items():
            doc = bindgen.build_set(ov, self.bindings, name == self.kb_name)
            self.assertEqual(len(doc['bindings']), len(self.bindings))
            for e in doc['bindings']:
                self.assertTrue(e['pattern'], f'{name}: {e["id"]}')
                self.assertTrue(e['section'].startswith('.'), f'{name}: {e["id"]}')
                smap = ov.get('sections', {})
                if e['section'] in smap:
                    self.assertIn(e['section'], smap.values(), f'{name}: {e["id"]} names a KB section')
                self.assertIn(e['status'], ('stable', 'experimental', 'unresolvable'))
                self.assertEqual(e['status'] == 'unresolvable', 'reason' in e)

    def test_kb_set_is_the_gated_bindings(self):
        doc = bindgen.build_set(self.overlays[self.kb_name], self.bindings, True)
        self.assertEqual(doc['bindings'], self.bindings)
        self.assertFalse(doc['build']['packed'])

    def test_other_sets_carry_no_kb_addresses(self):
        for name, ov in self.overlays.items():
            if name == self.kb_name:
                continue
            doc = bindgen.build_set(ov, self.bindings, False)
            hinted = ov.get('hints', {})
            for e in doc['bindings']:
                self.assertNotIn('rva', e)
                self.assertEqual('match' in e, 'match' in hinted.get(e['id'], {}), e['id'])

    def test_f2_revamp_bindings(self):
        by_id = {b['id']: b for b in self.bindings}
        want = {
            'k_Career_DriverAcademiesComponentDesc': 'data',
            'k_Career_RivalManagerComponentDesc': 'data',
            'Career_DriverAcademies_MapFindOrAdd': 'func',
            'Game_DriverCareerTeamSelect_OnMessage': 'func',
        }
        for name, kind in want.items():
            self.assertIn(name, by_id)
            self.assertEqual(by_id[name]['kind'], kind, name)
            self.assertEqual(by_id[name]['status'], 'experimental', name)
        self.assertEqual(by_id['Career_DriverAcademies_MapFindOrAdd']['resolve']['kind'], 'rel32')
        fields = {
            ('DriverAcademiesComponent', 'academies_begin'): 0x298,
            ('DriverAcademiesComponent', 'academies_end'): 0x2a0,
            ('DriverAcademiesComponent', 'driver_academy_entries'): 0x2c8,
            ('RivalManagerComponent', 'managers_begin'): 0xbd8,
            ('RivalManagerComponent', 'managers_end'): 0xbe0,
            ('RivalManager', 'current_rivalries_begin'): 0x18,
            ('RivalManager', 'current_rivalries_end'): 0x20,
            ('DriverCareerTeamSelectViewModel', 'selected_team_key'): 0xf20,
        }
        text = ''
        for name in ('career.hpp', 'game.hpp'):
            with open(os.path.join(bindgen.OUT_INC, 'game', name)) as f:
                text += f.read()
        for (struct, field), off in fields.items():
            self.assertIn(f'static_assert(offsetof({struct}, {field}) == {off:#x});', text)

    def test_steam_26566308_hints_the_f2_revamp_bindings(self):
        hints = self.overlays['steam-26566308']['hints']
        self.assertEqual(hints['k_Career_DriverAcademiesComponentDesc']['match'], '0x1a7328d')
        self.assertEqual(hints['k_Career_RivalManagerComponentDesc']['match'], '0x15cd07c')
        self.assertEqual(hints['Career_DriverAcademies_MapFindOrAdd']['match'], '0xd6f9af')
        self.assertEqual(hints['Game_DriverCareerTeamSelect_OnMessage']['match'], '0x1a6b230')

    def test_ea_set_is_packed(self):
        self.assertIn('ea', self.overlays)
        self.assertTrue(self.overlays['ea']['build']['packed'])


@unittest.skipUnless(os.environ.get('SWOFTY_STEAM_EXE'), 'set SWOFTY_STEAM_EXE to the KB build F1_25.exe')
class SteamExe(unittest.TestCase):
    def test_every_pattern_matches_once(self):
        import bindverify
        if not bindverify.is_kb_build(os.environ['SWOFTY_STEAM_EXE']):
            self.skipTest('SWOFTY_STEAM_EXE is not the KB build')
        img = bindverify.Image(os.environ['SWOFTY_STEAM_EXE'])
        _, bindings, _, _ = bindgen.load()
        for b in bindings:
            hits, at, _ = img.scan(b['section'], b['pattern'])
            self.assertEqual((hits, at and hex(at)), (1, b['match']), b['id'])


if __name__ == '__main__':
    unittest.main()
