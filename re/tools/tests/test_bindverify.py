import os, random, struct, sys, tempfile, unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import bindverify  # noqa: E402

CODE, RWX, DATA = 0x60000020, 0xE0000020, 0x40000040
PATTERN = '41 56 56 57 53 48 83 EC 48 48 89 CE'
FUNC = bytes.fromhex(PATTERN.replace(' ', ''))


def pe(sections, stamp=7, exception=(0, 0), flat=True):
    """A minimal PE32+ image: sections maps name -> (rva, flags, bytes), each one page long."""
    size = max(va for va, _, _ in sections.values()) + 0x1000
    head = bytearray(0x400)
    head[:2] = b'MZ'
    struct.pack_into('<I', head, 0x3c, 0x80)
    head[0x80:0x84] = b'PE\0\0'
    struct.pack_into('<HHIIIHH', head, 0x84, 0x8664, len(sections), stamp, 0, 0, 240, 0x22)
    opt = 0x98
    struct.pack_into('<H', head, opt, 0x20b)
    struct.pack_into('<QII', head, opt + 24, 0x140000000, 0x1000, 0x200)
    struct.pack_into('<II', head, opt + 56, size, 0x400)
    struct.pack_into('<I', head, opt + 108, 16)
    struct.pack_into('<II', head, opt + 112 + 8 * 3, *exception)
    out = bytearray(size) if flat else bytearray(0x400 + 0x1000 * len(sections))
    raw = 0x400
    for k, (name, (va, flags, data)) in enumerate(sections.items()):
        struct.pack_into('<8sIIIIIIHHI', head, opt + 240 + 40 * k, name.encode(), 0x1000, va, 0x1000,
                         va if flat else raw, 0, 0, 0, 0, flags)
        at = va if flat else raw
        out[at:at + len(data)] = data
        raw += 0x1000
    out[:0x400] = head
    return bytes(out)


def page(*parts):
    """One page of int3 filler with each (offset, bytes) part placed."""
    b = bytearray(b'\xCC' * 0x1000)
    for off, data in parts:
        b[off:off + len(data)] = data
    return bytes(b)


def binding(i, **kw):
    b = {'id': i, 'kind': 'func', 'status': 'experimental', 'section': '.xtls', 'pattern': PATTERN, 'anchor': 0,
         'resolve': {'kind': 'addr'}, 'expect': None}
    b.update(kw)
    return b


def doc(*bindings, stamp=7, packed=False, size=0):
    return {'build': {'pe_timestamp': stamp, 'size': size, 'packed': packed, 'distribution': 'Test',
                      'sha256': 'a' * 64}, 'bindings': list(bindings)}


class Synthetic(unittest.TestCase):
    def image(self, data):
        f = tempfile.NamedTemporaryFile(suffix='.img', delete=False)
        f.write(data)
        f.close()
        self.addCleanup(os.unlink, f.name)
        return bindverify.Image(f.name)

    def run_one(self, img, b, **kw):
        return bindverify.verify(img, doc(b, **kw))[0]

    def test_hint_then_scan(self):
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, FUNC)))}))
        r = self.run_one(img, binding('Fn', match='0x1100'))
        self.assertEqual((r['result'], r['method'], r['target']), ('resolved', 'hint', 0x1100))
        r = self.run_one(img, binding('Fn', match='0x1200'))
        self.assertEqual((r['result'], r['method'], r['matches'], r['target']), ('resolved', 'scan', 1, 0x1100))
        r = self.run_one(img, binding('Fn', match='0x1100'), stamp=8)
        self.assertEqual(r['method'], 'scan')

    def test_pe_file_maps_sections_by_rva(self):
        img = self.image(pe({'.rdata': (0x1000, DATA, page()), '.xtls': (0x2000, CODE, page((0x40, FUNC)))}, flat=False))
        self.assertFalse(img.flat)
        r = self.run_one(img, binding('Fn'))
        self.assertEqual((r['result'], r['target']), ('resolved', 0x2040))

    def test_scan_needs_exactly_one_hit_counting_overlaps(self):
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, FUNC), (0x300, FUNC)))}))
        r = self.run_one(img, binding('Fn'))
        self.assertEqual((r['result'], r['matches'], r['reason']), ('unresolved', 2, 'signature did not match exactly once'))
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, b'\x90' * 14)))}))
        r = self.run_one(img, binding('Fn', pattern='90 90 90 90 90 90 90 90 90 90 90 90'))
        self.assertEqual(r['matches'], 3)

    def test_a_hint_that_fails_apply_match_does_not_scan(self):
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, FUNC), (0x300, FUNC)))}))
        r = self.run_one(img, binding('Fn', match='0x1100', expect='E8'))
        self.assertEqual((r['result'], r['method'], r['reason']), ('unresolved', '-', 'expect bytes differ'))

    def test_a_live_verdict_stands_in_for_expect_bytes_under_the_cores_hook(self):
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, FUNC)))}))
        b = binding('Fn', match='0x1100', expect='E8')
        line = '[bind] id=Fn build=ea pass=init result=resolved method=scan matches=1 rva=0x1100 match=0x1100 verified=yes'
        proven = bindverify.live_verified([line, 'unrelated'])
        self.assertEqual(proven, {'Fn': 0x1100})
        self.assertEqual(bindverify.Resolver(img, doc(b)).run()[0]['reason'], 'expect bytes differ')
        self.assertEqual(bindverify.Resolver(img, doc(b), live_verified=proven).run()[0]['result'], 'resolved')
        other = {'Fn': 0x1200}
        self.assertEqual(bindverify.Resolver(img, doc(b), live_verified=other).run()[0]['result'], 'unresolved')

    def test_rel32_and_riprel(self):
        call = b'\xE8' + struct.pack('<i', 0x400 - 0x105) + FUNC
        lea = b'\x48\x8D\x05' + struct.pack('<i', 0x2010 - 0x1207) + FUNC
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, call), (0x200, lea))),
                             '.rdata': (0x2000, DATA, page())}))
        r = self.run_one(img, binding('Fn', pattern='E8 ?? ?? ?? ?? ' + PATTERN, anchor=1, resolve={'kind': 'rel32', 'insn': 0}))
        self.assertEqual(r['target'], 0x1400)
        r = self.run_one(img, binding('g_X', kind='data', pattern='48 8D 05 ?? ?? ?? ?? ' + PATTERN, anchor=3,
                                      resolve={'kind': 'riprel', 'insn': 0, 'len': 7}))
        self.assertEqual(r['target'], 0x2010)
        r = self.run_one(img, binding('Fn', pattern='E8 ?? ?? ?? ?? ' + PATTERN, anchor=2,
                                      resolve={'kind': 'rel32', 'insn': 0}))
        self.assertEqual(r['reason'], 'anchor is not the rel32 of a call or jump')

    def test_code_targets_need_read_only_code_unless_a_site_in_its_own_section(self):
        img = self.image(pe({'.data1': (0x1000, RWX, page((0x100, FUNC)))}))
        r = self.run_one(img, binding('Fn', section='.data1'))
        self.assertEqual(r['reason'], 'target is not in read-only code')
        r = self.run_one(img, binding('site_X', kind='site', section='.data1'))
        self.assertEqual(r['result'], 'resolved')

    def test_kb_rva_must_agree_on_the_same_build(self):
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, FUNC)))}))
        r = self.run_one(img, binding('Fn', rva='0x1104'))
        self.assertEqual(r['reason'], 'target differs from the KB build')
        r = self.run_one(img, binding('Fn', rva='0x1104'), stamp=8)
        self.assertEqual(r['result'], 'resolved')

    def test_string_anchor_fallback(self):
        lea = b'\x48\x8D\x0D' + struct.pack('<i', 0x2080 - 0x1127)
        pdata = struct.pack('<III', 0x1100, 0x1180, 0)
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x120, lea))),
                             '.rdata': (0x2000, DATA, page((0x80, b'needle text\0'))),
                             '.pdata': (0x3000, DATA, page((0, pdata)))}, exception=(0x3000, 12)))
        sa = {'string': 'needle text', 'func_rva': '0x1100', 'func_offset': '0x20', 'durable': True}
        r = self.run_one(img, binding('Fn', string_anchor=sa))
        self.assertEqual((r['result'], r['method'], r['target']), ('resolved', 'anchor-fallback', 0x1120))
        sa['func_rva'] = '0x1000'
        r = self.run_one(img, binding('Fn', string_anchor=sa))
        self.assertEqual(r['reason'], 'pattern: signature did not match exactly once; string anchor: string anchor '
                                      'function differs from the KB build')

    def test_packed_build_defers_encrypted_code(self):
        noise = random.Random(1).randbytes(0x1000)
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0, noise), (0x100, FUNC)))}))
        r = self.run_one(img, binding('Fn'), packed=True)
        self.assertEqual(r['result'], 'deferred')
        r = self.run_one(img, binding('Fn'), packed=False)
        self.assertEqual(r['result'], 'resolved')

    def test_unresolvable_and_missing_section(self):
        img = self.image(pe({'.xtls': (0x1000, CODE, page((0x100, FUNC)))}))
        r = self.run_one(img, binding('Fn', status='unresolvable', reason='packed body'))
        self.assertEqual((r['result'], r['reason']), ('unresolvable', 'unresolvable: packed body'))
        r = self.run_one(img, binding('Fn', section='.text'))
        self.assertEqual(r['reason'], 'section .text is not in this exe')

    def test_compare_flags_a_call_to_another_callee(self):
        callee = b'\x48\x83\xEC\x28\x83\x3D\x00\x00\x00\x00\x00\x75\x2D\x48\x85\xC9'
        other = b'\x48\x3B\x0D\x19\xE5\x30\x01\x75\x10\x48\xC1\xC1\x10\x66\xF7\xC1'
        ctx = b'\x49\x8B\x4C\x24\x50\x48\x8D\x15\x00\x00\x00\x00'

        def build(to):
            call = ctx + b'\xE8' + struct.pack('<i', 0x800 - 0x111) + FUNC
            return self.image(pe({'.xtls': (0x1000, CODE, page((0x100, call), (0x800, to)))}))
        b = binding('site_X', kind='call', pattern='E8 ?? ?? ?? ?? ' + PATTERN, expect='E8')
        d = doc(b)
        ref, same, moved = build(callee), build(callee), build(other)
        ref_res = bindverify.verify(ref, d)
        self.assertEqual(bindverify.compare(ref, d, ref_res, same, d, bindverify.verify(same, d)), [])
        odd = bindverify.compare(ref, d, ref_res, moved, d, bindverify.verify(moved, d))
        self.assertEqual([i for i, _ in odd], ['site_X'])

    def test_compare_reads_a_protector_thunk_by_its_relocated_body(self):
        body = b'\x48\x89\x5C\x24\x08\x57\x48\x83\xEC\x20\x48\x8B\xF9\xE8\x10\x20\x30\x40' + bytes(range(14))
        other = bytes(range(100, 132))

        def build(filler, to):
            jmp = b'\xE9' + struct.pack('<i', 0x2000 + 0x400 - (0x1000 + 0x100 + 5))
            code = page((0x0F0, filler), (0x100, jmp + FUNC), (0x100 + 5 + len(FUNC), filler))
            return self.image(pe({'.xtls': (0x1000, CODE, code), '.data1': (0x2000, RWX, page((0x400, to)))}))
        b = binding('Fn_Thunk', pattern='E9 ?? ?? ?? ?? ' + PATTERN)
        d = doc(b)
        ref = build(bytes(range(16)), body)
        moved_filler, other_body = build(bytes(range(200, 216)), body), build(bytes(range(200, 216)), other)
        ref_res = bindverify.verify(ref, d)
        self.assertEqual(bindverify.compare(ref, d, ref_res, moved_filler, d, bindverify.verify(moved_filler, d)), [])
        odd = bindverify.compare(ref, d, ref_res, other_body, d, bindverify.verify(other_body, d))
        self.assertEqual([i for i, _ in odd], ['Fn_Thunk'])
        self.assertIn('thunk body', odd[0][1])

    def test_compare_reads_an_override_at_a_caller_by_the_code_it_reaches(self):
        stub = b'\x48\x83\xEC\x28\xE8\x00\x01\x00\x00\xCC\xCC\xCC\xCC\xCC\xCC\xCC'
        other = bytes(range(16))
        caller = b'\x49\x8B\x4C\x24\x50\xE8' + struct.pack('<i', 0x800 - 0x30A) + b'\x48\x8B\x5C\x24\x30\x90'

        def build(at_stub):
            code = page((0x2F0, bytes(range(140, 156))), (0x300, caller), (0x310, bytes(range(120, 136))),
                        (0x800, at_stub))
            return self.image(pe({'.xtls': (0x1000, CODE, code)}))
        ref_d = doc(binding('Fn'))
        ref_d['bindings'][0]['pattern'] = '48 83 EC 28 E8 ?? ?? ?? ?? CC CC CC'
        ea_b = binding('Fn', pattern='49 8B 4C 24 50 E8 ?? ?? ?? ?? 48 8B 5C 24 30', anchor=6,
                       resolve={'kind': 'rel32', 'insn': 5})
        ref = self.image(pe({'.xtls': (0x1000, CODE, page((0x500, bytes(range(60, 76))), (0x510, stub),
                                                          (0x520, bytes(range(80, 96)))))}))
        ref_res = bindverify.verify(ref, ref_d)
        same, moved = build(stub), build(other)
        self.assertEqual(bindverify.compare(ref, ref_d, ref_res, same, doc(ea_b), bindverify.verify(same, doc(ea_b))), [])
        odd = bindverify.compare(ref, ref_d, ref_res, moved, doc(ea_b), bindverify.verify(moved, doc(ea_b)))
        self.assertEqual([i for i, _ in odd], ['Fn'])
        self.assertIn('code at the target', odd[0][1])


def resolves(r, method, hinted):
    if r['id'] in hinted:
        return method == 'scan' or (r['result'], r['method']) == ('resolved', 'hint')
    return r['result'] == 'resolved'


SAME_FUNCTION_DIFFERENT_BYTES = {'Ne_Log_Thunk', 'g_Ne_LogGate', 'g_Vm_Registry', 'FeTile_ItemBase_Slot78_Thunk',
                                 'Game_HandleUserEntitlementsUpdateTask_OnChildDone'}


@unittest.skipUnless(os.environ.get('SWOFTY_STEAM_EXE'), 'set SWOFTY_STEAM_EXE to the KB build F1_25.exe')
class SteamImage(unittest.TestCase):
    def test_every_binding_resolves_by_hint_and_by_scan(self):
        img = bindverify.Image(os.environ['SWOFTY_STEAM_EXE'])
        d = bindverify.gen_set_for(img)
        hinted = {b['id'] for b in d['bindings'] if b.get('match')}
        for kb_first, method in ((True, 'hint'), (False, 'scan')):
            res = bindverify.Resolver(img, d, os.path.getsize(img.path), kb_first=kb_first).run()
            bad = [(r['id'], r['result'], r['reason']) for r in res if not resolves(r, method, hinted)]
            self.assertEqual(bad, [], method)


@unittest.skipUnless(os.environ.get('SWOFTY_EA_DUMP'), 'set SWOFTY_EA_DUMP to a decrypted EA F1_25 image dump')
class EaDump(unittest.TestCase):
    def test_every_binding_resolves_by_hint_and_by_scan(self):
        img = bindverify.Image(os.environ['SWOFTY_EA_DUMP'])
        d = bindverify.gen_set_for(img)
        self.assertTrue(img.flat)
        for kb_first, method in ((True, 'hint'), (False, 'scan')):
            res = bindverify.Resolver(img, d, d['build']['size'], kb_first=kb_first).run()
            hinted = {b['id'] for b in d['bindings'] if b.get('match')}
            bad = [(r['id'], r['result'], r['reason']) for r in res
                   if r['result'] != 'unresolvable' and not resolves(r, method, hinted)]
            self.assertEqual(bad, [], method)

    @unittest.skipUnless(os.environ.get('SWOFTY_STEAM_EXE'), 'set SWOFTY_STEAM_EXE to compare against the KB build')
    def test_every_binding_lands_on_the_steam_code(self):
        ea = bindverify.Image(os.environ['SWOFTY_EA_DUMP'])
        d = bindverify.gen_set_for(ea)
        steam, sd = bindverify.Image(os.environ['SWOFTY_STEAM_EXE']), bindverify.gen_set('steam')
        odd = bindverify.compare(steam, sd, bindverify.verify(steam, sd), ea, d, bindverify.verify(ea, d))
        self.assertEqual([i for i, _ in odd if i not in SAME_FUNCTION_DIFFERENT_BYTES], [])



HOOK_SPAN = 0x10000000


def hooked_at(img, rva):
    """A jmp from rva into memory past the image end, where a dump taken with the core running holds its hooks."""
    if not img.readable(rva, 5) or img.mem[rva] != 0xE9:
        return False
    return img.size <= rva + 5 + img.i32(rva + 1) < img.size + HOOK_SPAN


def same_code(a, b):
    """Equal bytes, except the rel32 of a call or jmp that both hold at the same place."""
    i = 0
    while i < len(a):
        if a[i] != b[i]:
            return False
        i += 5 if a[i] in (0xE8, 0xE9) and i + 5 <= len(a) else 1
    return True


def why_it_differs(img, ref, b, r, rr):
    """Why a binding that compare() flags still lands on the reference build's code, or None."""
    t, rt = r['target'], rr['target']
    m = r['match'] if r['match'] is not None else t
    spots = {t, m} | set(range(m, m + len(b['pattern'].split())))
    if img.readable(t, 5) and img.mem[t] == 0xE8:
        spots.add(t + 5 + img.i32(t + 1))
    if any(hooked_at(img, s) for s in spots):
        return 'a live hook of the core'
    call = ref.readable(rt, 5) and ref.mem[rt] == 0xE8
    if same_code(img.read(t - 12, 12), ref.read(rt - 12, 12)) or \
            (img.read(t + 5, 11) == ref.read(rt + 5, 11) if call else img.read(t, 16) == ref.read(rt, 16)):
        return 'the same code at the target'
    return None


def ea_145_image():
    """The EA 1.0.145 dump and the bindings its run verified live, with the core's patches put back from that run's
    loader.log when SWOFTY_EA_145_LOG names it, as bindverify.py --restore does."""
    img = bindverify.Image(os.environ['SWOFTY_EA_145_DUMP'])
    log = os.environ.get('SWOFTY_EA_145_LOG')
    if not log:
        return img, {}
    import sigverify
    with open(log, errors='replace') as f:
        sigverify.restore(img, f)
    with open(log, errors='replace') as f:
        return img, bindverify.live_verified(f)


@unittest.skipUnless(os.environ.get('SWOFTY_EA_145_DUMP'), 'set SWOFTY_EA_145_DUMP to a decrypted EA 1.0.145 image dump')
class Ea145Dump(unittest.TestCase):
    """The EA 1.0.145 dump was taken with the core running, so some code holds its hooks. With SWOFTY_EA_145_LOG set to
    that run's loader.log the resolve test puts the original bytes back first; without it the hooked hints fail."""

    def test_every_binding_resolves_by_hint_and_by_scan(self):
        img, proven = ea_145_image()
        d = bindverify.gen_set_for(img)
        self.assertTrue(img.flat)
        self.assertEqual(d['build']['pe_timestamp'], img.stamp)
        hinted = {b['id'] for b in d['bindings'] if b.get('match')}
        for kb_first, method in ((True, 'hint'), (False, 'scan')):
            res = bindverify.Resolver(img, d, d['build']['size'], kb_first=kb_first, live_verified=proven).run()
            bad = [(r['id'], r['result'], r['reason']) for r in res
                   if r['result'] != 'unresolvable' and not resolves(r, method, hinted)]
            self.assertEqual(bad, [], method)

    @unittest.skipUnless(os.environ.get('SWOFTY_STEAM_145_EXE'), 'set SWOFTY_STEAM_145_EXE to compare with Steam 1.0.145')
    def test_every_binding_lands_on_the_steam_1_0_145_code(self):
        ea = bindverify.Image(os.environ['SWOFTY_EA_145_DUMP'])
        d = bindverify.gen_set_for(ea)
        steam = bindverify.Image(os.environ['SWOFTY_STEAM_145_EXE'])
        sd = bindverify.gen_set_for(steam)
        er, sr = bindverify.verify(ea, d), bindverify.verify(steam, sd)
        mine = {b['id']: (b, r) for b, r in zip(d['bindings'], er)}
        ref = {b['id']: r for b, r in zip(sd['bindings'], sr)}
        odd = [(i, why) for i, why in bindverify.compare(steam, sd, sr, ea, d, er)
               if i not in SAME_FUNCTION_DIFFERENT_BYTES and why_it_differs(ea, steam, *mine[i], ref[i]) is None]
        self.assertEqual(odd, [])


class FakeSection:
    def __init__(self, name):
        self.name = name


class FakePort:
    def section_of(self, rva):
        return FakeSection('.tls$') if rva < 0x100000 else FakeSection('.trace') if rva < 0x200000 else None


MARKS = [(0x1000, 0x900, 'A', '.tls$'), (0x3000, 0x2900, 'B', '.tls$'), (0x5000, 0x4800, 'C', '.tls$'),
         (0x101000, 0x100000, 'D', '.trace'), (0x103000, 0x102000, 'E', '.trace')]


class KbTwin(unittest.TestCase):
    def twin(self, rva):
        return bindverify.kb_twin(FakePort(), 'port', rva, MARKS)

    def test_between_two_bindings_that_moved_alike(self):
        kb, why = self.twin(0x2000)
        self.assertEqual((kb, why['how'], why['shift'], why['below'], why['above']), (0x1900, 'neighbours', 0x700, 'A', 'B'))

    def test_a_bound_address_is_its_binding(self):
        self.assertEqual(self.twin(0x3000)[0], 0x2900)

    def test_neighbours_that_moved_differently_give_no_twin(self):
        kb, why = self.twin(0x4000)
        self.assertIsNone(kb)
        self.assertIn('moved by different amounts', why)

    def test_neighbours_come_from_the_same_section(self):
        self.assertEqual(self.twin(0x102000)[0], 0x101000)
        kb, why = self.twin(0x6000)
        self.assertIsNone(kb)
        self.assertIn('no binding above', why)
        self.assertIn('no section', self.twin(0x300000)[1])


if __name__ == '__main__':
    unittest.main()
