"""tools/patch_sites.py on a fixture source tree, binding sets and a synthetic exe.

The exe case needs capstone: it runs in this Python when it has capstone, else under re/tools/py, and skips when
re/tools/py finds no Python with pefile, numpy and capstone.
"""

import importlib.util
import io
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
PY = os.path.join(ROOT, 're', 'tools', 'py')
sys.path.insert(0, os.path.join(HERE, '..', 'tools'))
import patch_sites as ps  # noqa: E402

KB_SHA, PORT_SHA = 'a' * 64, 'b' * 64
KB_STAMP, PORT_STAMP = 100, 200

HOOKS = '''#include "core/a/more.hpp"

const runtime::Sig S_ONE{"One_Func", "48 89 5C 24 08 57 48 83 EC 20", 0x1000};
const runtime::Sig S_TWO{"Two_Site", "AA BB ?? CC", 0x2000, runtime::Sig::addr, 0, 2};
const runtime::Sig S_CALL{"Call_Rel", "E8 ?? ?? ?? ?? 90 90", 0x3000, runtime::Sig::rel32, 0, 1, 5};

void arm()
{
    auto a = safetyhook::InlineHook::create(reinterpret_cast<void *>(runtime::find(S_ONE)), &d);
    auto b = safetyhook::MidHook::create(reinterpret_cast<void *>(runtime::find(S_TWO)), &m);
    if (!VirtualProtect(at, 5, PAGE_EXECUTE_READWRITE, &old))
        return;
    check(arm_one(S_CALL));
}
'''

UNLISTED = '    auto c = safetyhook::MidHook::create(x, &m);\n'

MORE = '''constexpr const char *SITE = "site_Game";
constexpr uintptr_t TAIL = 0x2;
'''

TABLE = '''#include "core/a/more.hpp"

const SwoftyMixin mixins[] = {
    {"Game_Func", SWOFTY_MIXIN_WRAP, 0, (void *)&w, nullptr},
    {"vt_Game", SWOFTY_MIXIN_WITH_ARG(SWOFTY_MIXIN_VTABLE, 2), 0, (void *)&v, nullptr},
    {SITE, SWOFTY_MIXIN_SITE, 0, (void *)&s, nullptr},
    {"Game_Call", SWOFTY_MIXIN_REDIRECT, 0, (void *)&c, nullptr},
};
bool wraps(int k) { return k == SWOFTY_MIXIN_WRAP || counts[SWOFTY_MIXIN_SITE] > 0; }
'''

VERSION = '''static int set_slot(void **slot, void *value)
{
    if (!VirtualProtect(slot, sizeof *slot, PAGE_READWRITE, &old))
        return 0;
}
'''

LIST = '''# fixture
helper core/a/hooks.cpp \\barm_one\\(
core/a/hooks.cpp: auto a = safetyhook::InlineHook::create(reinterpret_cast<void *>(runtime::find(S_ONE)), &d); => inline sig S_ONE
core/a/hooks.cpp: auto b = safetyhook::MidHook::create(reinterpret_cast<void *>(runtime::find(S_TWO)), &m); => mid sig S_TWO
core/a/hooks.cpp: if (!VirtualProtect(at, 5, PAGE_EXECUTE_READWRITE, &old)) => bytes=5 sig S_TWO
core/a/hooks.cpp: check(arm_one(S_CALL)); => inline sig S_CALL rel32 S_ONE TAIL
loader/version.c: if (!VirtualProtect(slot, sizeof *slot, PAGE_READWRITE, &old)) => import KERNEL32.dll!CreateMutexW
'''


def put(path, text):
    with open(path, 'w') as f:
        f.write(text)


def binding(id_, kind, rva, pattern='11 22 33', anchor=0, resolve='addr', insn=0, ln=0, match=None, **extra):
    b = {'id': id_, 'kind': kind, 'section': '.xtls', 'pattern': pattern, 'anchor': hex(anchor),
         'resolve': {'kind': resolve, 'insn': insn, 'len': ln}}
    if rva is not None:
        b['rva'] = hex(rva)
    if match is not None:
        b['match'] = hex(match)
    b.update(extra)
    return b


def kb_set():
    return {'build': {'sha256': KB_SHA, 'pe_timestamp': KB_STAMP, 'distribution': 'Steam'}, 'bindings': [
        binding('Game_Func', 'func', 0x5000, match=0x5000),
        binding('vt_Game', 'vtable', 0x9000, slots=4),
        binding('site_Game', 'site', 0x5100),
        binding('Game_Call', 'call', 0x5200),
    ]}


def port_set():
    return {'build': {'sha256': PORT_SHA, 'pe_timestamp': PORT_STAMP, 'distribution': 'Steam'}, 'bindings': [
        dict(binding('Game_Func', 'func', None, anchor=2, match=0x6000), section='.text'),
        dict(binding('site_Game', 'site', None, pattern='E8 ?? ?? ?? ?? 48 85 C0 74', anchor=1, resolve='rel32'),
             section='.text'),
        dict(binding('vt_Game', 'vtable', None, pattern='48 8D 05 ?? ?? ?? ?? C3', anchor=3, resolve='riprel', insn=0,
                     ln=7, slots=4), section='.text'),
        dict(binding('Game_Call', 'call', None), section='.text', status='unresolvable', reason='gone'),
    ]}


def synthetic_exe(path):
    size = 0x4000
    img = bytearray(size)
    img[0:2] = b'MZ'
    struct.pack_into('<I', img, 0x3c, 0x80)
    pe = 0x80
    img[pe:pe + 4] = b'PE\0\0'
    optsz = 240
    struct.pack_into('<HHI', img, pe + 4, 0x8664, 2, PORT_STAMP)
    struct.pack_into('<H', img, pe + 20, optsz)
    opt = pe + 24
    struct.pack_into('<H', img, opt, 0x20b)
    struct.pack_into('<QI', img, opt + 24, 0x140000000, 0x1000)
    struct.pack_into('<II', img, opt + 56, size, 0x400)
    struct.pack_into('<II', img, opt + 112 + 8, 0x2000, 40)
    sec = opt + optsz
    for i, (name, va, flags) in enumerate(((b'.text', 0x1000, 0x60000020), (b'.rdata', 0x2000, 0x40000040))):
        struct.pack_into('<8sIIIII', img, sec + 40 * i, name, 0x1000, va, 0x1000, va, 0)
        struct.pack_into('<I', img, sec + 40 * i + 36, flags)
    img[0x1000:0x2000] = b'\xcc' * 0x1000
    prologue = bytes.fromhex('48895C2408 57 4883EC20 488BD9 E800000000 90 90 90 90'.replace(' ', ''))
    img[0x1200:0x1200 + len(prologue)] = prologue
    site = bytes.fromhex('E8F0010000 4885C0 7402'.replace(' ', ''))
    img[0x1400:0x1400 + len(site)] = site
    img[0x1400 + 5 + 0x1f0:0x1400 + 5 + 0x1f0 + 16] = b'\x90' * 16
    lea = bytes.fromhex('488D05FA0F0000 C3'.replace(' ', ''))
    img[0x1700:0x1700 + len(lea)] = lea
    struct.pack_into('<IIIII', img, 0x2000, 0x2100, 0, 0, 0x2200, 0x2300)
    img[0x2200:0x2200 + 13] = b'KERNEL32.dll\0'
    struct.pack_into('<QQ', img, 0x2100, 0x2400, 0)
    struct.pack_into('<QQ', img, 0x2300, 0x2400, 0)
    img[0x2402:0x2402 + 13] = b'CreateMutexW\0'
    with open(path, 'wb') as f:
        f.write(img)


class Fixture(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp(prefix='patch_sites_test.')
        self.src = os.path.join(self.dir, 'src')
        self.sets = os.path.join(self.dir, 'sets')
        self.records = os.path.join(self.dir, 'records')
        self.write('core/a/hooks.cpp', HOOKS)
        self.write('core/a/more.hpp', MORE)
        self.write('core/a/table.cpp', TABLE)
        self.write('loader/version.c', VERSION)
        os.makedirs(self.sets)
        os.makedirs(os.path.join(self.records, 'game'))
        put(os.path.join(self.sets, 'bindings.json'), json.dumps(kb_set()))
        put(os.path.join(self.sets, 'bindings.port.json'), json.dumps(port_set()))
        self.list = os.path.join(self.dir, 'patch_sites.txt')
        put(self.list, LIST)
        self.saved = ps.SRC, ps.LIST, ps.RECORDS
        ps.SRC, ps.LIST, ps.RECORDS = self.src, self.list, self.records
        ps._text.clear()
        ps._sigs.clear()

    def tearDown(self):
        ps.SRC, ps.LIST, ps.RECORDS = self.saved
        ps._text.clear()
        ps._sigs.clear()
        shutil.rmtree(self.dir)

    def write(self, rel, text):
        path = os.path.join(self.src, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        put(path, text)
        ps._text.pop(rel, None)
        ps._sigs.pop(rel, None)

    def problems(self):
        return ps.problems(ps.binding_ids(self.sets))[0]

    def generate(self, images=()):
        out = os.path.join(self.dir, 'patches.json')
        with redirect_stdout(io.StringIO()):
            self.assertEqual(ps.generate(self.sets, out, list(images), '9.9.9'), 0)
        with open(out) as f:
            doc = json.load(f)
        return doc, {(r['build'], r['name'], r['write']): r for r in doc['sites']}


class Check(Fixture):
    def test_listed_tree_passes(self):
        self.assertEqual(self.problems(), [])

    def test_unlisted_hook_fails(self):
        self.write('core/a/hooks.cpp', HOOKS.replace('    check(', UNLISTED + '    check('))
        bad = self.problems()
        self.assertEqual(len(bad), 1, bad)
        self.assertIn('core/a/hooks.cpp:13: auto c = safetyhook::MidHook::create(x, &m);', bad[0])
        self.assertIn('does not list it', bad[0])

    def test_unlisted_gated_install_and_write_fail(self):
        extra = '    loader::install(h, at, "x", fn);\n    WriteProcessMemory(p, at, b, 2, &n);\n    write_bytes(at, b, 2);\n'
        self.write('core/a/hooks.cpp', HOOKS.replace('    check(', extra + '    check('))
        self.assertEqual(len(self.problems()), 3)

    def test_helper_caller_must_be_listed(self):
        self.write('core/a/hooks.cpp', HOOKS.replace('    check(', '    arm_one(S_TWO);\n    check('))
        bad = self.problems()
        self.assertEqual(len(bad), 1, bad)
        self.assertIn('arm_one(S_TWO);', bad[0])

    def test_stale_entry_fails(self):
        put(self.list, LIST + 'core/a/hooks.cpp: gone(); => inline sig S_ONE\n')
        bad = self.problems()
        self.assertEqual(len(bad), 1, bad)
        self.assertIn('is no longer in the source', bad[0])

    def test_line_listed_once_per_copy(self):
        again = '    if (!VirtualProtect(at, 5, PAGE_EXECUTE_READWRITE, &old))\n        return;\n'
        self.write('core/a/hooks.cpp', HOOKS.replace('    check(', again + '    check('))
        self.assertEqual(len(self.problems()), 1)
        put(self.list, LIST + 'core/a/hooks.cpp: if (!VirtualProtect(at, 5, PAGE_EXECUTE_READWRITE, &old)) => via\n')
        self.assertEqual(self.problems(), [])

    def test_unknown_signature_fails(self):
        put(self.list, LIST.replace('=> mid sig S_TWO', '=> mid sig S_NOPE'))
        bad = self.problems()
        self.assertTrue(any('no signature S_NOPE' in b for b in bad), bad)

    def test_unknown_binding_fails(self):
        self.write('core/a/table.cpp', TABLE.replace('"Game_Call"', '"Game_Nope"'))
        bad = self.problems()
        self.assertTrue(any('Game_Nope is not a binding' in b for b in bad), bad)

    def test_binding_record_outside_the_sets_is_known(self):
        self.write('core/a/table.cpp', TABLE.replace('"Game_Call"', '"Game_Later"'))
        put(os.path.join(self.records, 'game', 'Game_Later.json'), '{}')
        self.assertEqual(self.problems(), [])

    def test_mixin_kind_outside_an_entry_fails(self):
        self.write('core/a/table.cpp', TABLE + 'void add() { m.kind = SWOFTY_MIXIN_SITE; }\n')
        bad = self.problems()
        self.assertEqual(len(bad), 1, bad)
        self.assertIn('m.kind = SWOFTY_MIXIN_SITE;', bad[0])

    def test_unknown_write_fails(self):
        put(self.list, LIST.replace('=> mid sig S_TWO', '=> hook sig S_TWO'))
        with self.assertRaises(ps.Fail):
            self.problems()

    def test_generate_refuses_an_unlisted_tree(self):
        self.write('core/a/hooks.cpp', HOOKS.replace('    check(', UNLISTED + '    check('))
        with redirect_stdout(io.StringIO()), self.assertRaises(ps.Fail):
            ps.generate(self.sets, os.path.join(self.dir, 'patches.json'), [], '')
        self.assertFalse(os.path.exists(os.path.join(self.dir, 'patches.json')))


class Generate(Fixture):
    def test_rows(self):
        doc, rows = self.generate()
        self.assertEqual(doc['schema'], 1)
        self.assertEqual((doc['module'], doc['version']), ('swofty.core', '9.9.9'))
        self.assertEqual(doc['hook_length'], 32)
        self.assertEqual([b['set'] for b in doc['builds']], ['bindings.json', 'bindings.port.json'])
        self.assertEqual(doc['builds'][0], {'exe_sha256': KB_SHA, 'pe_timestamp': KB_STAMP, 'exe_size': 0,
                                            'distribution': 'Steam', 'packed': False, 'set': 'bindings.json',
                                            'resolved_here': False})
        self.assertEqual(rows[(KB_SHA, 'Game_Func', 'inline')]['rva'], 0x5000)
        self.assertEqual(rows[(KB_SHA, 'Game_Func', 'inline')]['length'], 32)
        self.assertFalse(rows[(KB_SHA, 'Game_Func', 'inline')]['exact'])
        self.assertEqual(rows[(KB_SHA, 'Game_Func', 'inline')]['owner'], 'core/a/table.cpp')
        vt = rows[(KB_SHA, 'vt_Game', 'vtable')]
        self.assertEqual((vt['rva'], vt['length'], vt['exact']), (0x9000, 32, True))
        self.assertEqual(rows[(KB_SHA, 'site_Game', 'mid')]['rva'], 0x5100)
        self.assertEqual(rows[(KB_SHA, 'Game_Call', 'call')]['length'], 5)

    def test_signature_sites_stay_unresolved(self):
        _, rows = self.generate()
        one = rows[(KB_SHA, 'S_ONE', 'inline')]
        self.assertEqual({k: one[k] for k in ('rva', 'pattern', 'section', 'anchor', 'resolve')},
                         {'rva': None, 'pattern': '48 89 5C 24 08 57 48 83 EC 20', 'section': '.xtls', 'anchor': 0,
                          'resolve': 'addr'})
        self.assertNotIn('next', one)
        self.assertEqual(rows[(PORT_SHA, 'S_ONE', 'inline')]['section'], '.text')
        two = rows[(KB_SHA, 'S_TWO', 'bytes')]
        self.assertEqual((two['anchor'], two['length'], two['exact']), (2, 5, True))
        call = rows[(KB_SHA, 'S_CALL', 'inline')]
        self.assertEqual((call['resolve'], call['anchor'], call['next']), ('rel32', 1, 5))
        tail = rows[(KB_SHA, 'S_ONE+TAIL', 'inline')]
        self.assertEqual((tail['resolve'], tail['anchor'], tail['next']), ('rel32', 2, 6))
        self.assertEqual(tail['owner'], 'core/a/hooks.cpp')

    def test_port_bindings(self):
        _, rows = self.generate()
        self.assertEqual(rows[(PORT_SHA, 'Game_Func', 'inline')]['rva'], 0x6002)
        site = rows[(PORT_SHA, 'site_Game', 'mid')]
        self.assertEqual((site['rva'], site['section'], site['anchor'], site['resolve'], site['next']),
                         (None, '.text', 1, 'rel32', 5))
        vt = rows[(PORT_SHA, 'vt_Game', 'vtable')]
        self.assertEqual((vt['resolve'], vt['next'], vt['length']), ('riprel', 7, 32))
        self.assertNotIn((PORT_SHA, 'Game_Call', 'call'), rows)

    def test_import_rows(self):
        _, rows = self.generate()
        imp = rows[(KB_SHA, 'KERNEL32.dll!CreateMutexW', 'import')]
        self.assertEqual((imp['rva'], imp['resolve'], imp['import'], imp['length'], imp['exact']),
                         (None, 'import', 'KERNEL32.dll!CreateMutexW', 8, True))
        self.assertEqual(imp['owner'], 'loader/version.c')


class Image(Fixture):
    def elsewhere(self, name):
        if importlib.util.find_spec('capstone'):
            return False
        probe = subprocess.run([PY, '-c', 'import pefile, numpy, capstone'], capture_output=True, text=True)
        if probe.returncode:
            self.skipTest('re/tools/py has no Python with pefile, numpy and capstone')
        run = subprocess.run([PY, '-I', os.path.abspath(__file__), f'Image.{name}'], capture_output=True, text=True)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        return True

    def test_resolves_on_the_exe(self):
        if self.elsewhere('test_resolves_on_the_exe'):
            return
        exe = os.path.join(self.dir, 'F1_25.exe')
        synthetic_exe(exe)
        doc, rows = self.generate([exe])
        self.assertEqual([b['resolved_here'] for b in doc['builds']], [False, True])
        one = rows[(PORT_SHA, 'S_ONE', 'inline')]
        self.assertEqual((one['rva'], one['length'], one['exact']), (0x1200, 18, True))
        self.assertNotIn('pattern', one)
        site = rows[(PORT_SHA, 'site_Game', 'mid')]
        self.assertEqual((site['rva'], site['length'], site['exact']), (0x1400 + 5 + 0x1f0, 14, True))
        vt = rows[(PORT_SHA, 'vt_Game', 'vtable')]
        self.assertEqual(vt['rva'], 0x1700 + 7 + 0xffa)
        self.assertEqual(rows[(PORT_SHA, 'KERNEL32.dll!CreateMutexW', 'import')]['rva'], 0x2300)
        self.assertEqual(rows[(PORT_SHA, 'S_TWO', 'mid')]['unresolved'], 'does not resolve on this exe')
        self.assertIsNone(rows[(KB_SHA, 'S_ONE', 'inline')]['rva'])

    def test_resolves_a_written_file(self):
        if self.elsewhere('test_resolves_a_written_file'):
            return
        exe = os.path.join(self.dir, 'F1_25.exe')
        synthetic_exe(exe)
        self.generate()
        out = os.path.join(self.dir, 'resolved.json')
        with redirect_stdout(io.StringIO()):
            self.assertEqual(ps.resolve_file(os.path.join(self.dir, 'patches.json'), out, [exe], self.sets), 0)
        with open(out) as f:
            doc = json.load(f)
        rows = {(r['build'], r['name'], r['write']): r for r in doc['sites']}
        _, direct = self.generate([exe])
        self.assertEqual(rows, direct)
        self.assertEqual([b['resolved_here'] for b in doc['builds']], [False, True])
        with redirect_stdout(io.StringIO()), self.assertRaises(ps.Fail):
            ps.resolve_file(os.path.join(self.dir, 'patches.json'), out, [], self.sets)


if __name__ == '__main__':
    unittest.main()
