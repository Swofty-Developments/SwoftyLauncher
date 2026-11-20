#!/usr/bin/env python3
"""Lists every byte the core writes in the game exe, per supported build, as patches.json.

  patch_sites.py --sets DIR --out patches.json [--image EXE]...   write the list for the binding sets in DIR
  patch_sites.py --check [--sets DIR]                             fail when a line that writes code is not listed
  patch_sites.py --resolve patches.json --image EXE [--sets DIR] --out OUT
                                                                  resolve a written file's sites of the exe's build

The sites come from three places in native/src (core, loader, gamelog):
- the core's mixin tables ({"<binding>", SWOFTY_MIXIN_<KIND>, ...}) and the SDK mixin macros, which name a binding;
- tools/patch_sites.txt, which names the targets of every other line that writes code: a safetyhook create, a gated
  install (runtime/loader/hook_install.hpp), a VirtualProtect that grants write access, a patch set byte write, a
  WriteProcessMemory, and each line a "helper" pattern in the list matches;
- the binding sets, which give each binding's site on each build.

--check fails on a line that writes code and is not listed, on an entry whose line is gone, on a target that names
no signature declaration and no binding (re/bindings), and on a mixin kind used outside an entry it reads. Writing
patches.json fails on the same.
"""
import argparse, glob, json, os, re, struct, subprocess, sys, tempfile

NATIVE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SRC = os.path.join(NATIVE, 'src')
ROOT = os.path.dirname(NATIVE)
LIST = os.path.join(NATIVE, 'tools', 'patch_sites.txt')
RECORDS = os.path.join(ROOT, 're', 'bindings')
RE_TOOLS = os.path.join(ROOT, 're', 'tools')
sys.path.insert(0, RE_TOOLS)
import sigdecl  # noqa: E402
SCHEMA = 1
MODULE = 'swofty.core'
HOOK_LENGTH = 32
JUMP_SPAN = 14
ANY_CODE = '*'

SOURCES = ['core/**/*.cpp', 'core/**/*.hpp', 'core/**/*.h', 'loader/*.c', 'loader/*.h', 'gamelog/*.cpp', 'gamelog/*.hpp']
PRIMITIVES = re.compile(r'safetyhook::(?:InlineHook|MidHook)::create\(|safetyhook::create_(?:inline|mid)\('
                        r'|(?:\bloader::|(?<![\w.>:~]))install\('
                        r'|\bVirtualProtect\([^;]*PAGE_(?:EXECUTE_)?READWRITE'
                        r'|\bwrite_bytes\(|\bWriteProcessMemory\(')
WRITES = {'inline', 'mid', 'call', 'vtable', 'import'}
NO_SITE = {'via', 'outside', 'none'}
TARGET_WORDS = {'sig', 'sigs-in', 'binding', 'binding-consts', 'spec', 'rel32', 'prologue', 'import', 'tag'}

KINDS = {'WRAP': 'inline', 'SITE': 'mid', 'VTABLE': 'vtable', 'REDIRECT': 'call', 'PATCH': 'bytes'}
MIXIN_ENTRY = re.compile(r'\{\s*(?P<target>[A-Za-z_][\w:]*|"[^"]*")\s*,\s*(?:SWOFTY_MIXIN_WITH_ARG\(\s*SWOFTY_MIXIN_'
                         r'(?P<akind>WRAP|SITE|VTABLE|REDIRECT|PATCH)\s*,\s*(?P<arg>[^)]+)\)|SWOFTY_MIXIN_'
                         r'(?P<kind>WRAP|SITE|VTABLE|REDIRECT|PATCH)\b)')
MIXIN_TOKEN = re.compile(r'\bSWOFTY_MIXIN_(?:WRAP|SITE|VTABLE|REDIRECT|PATCH)\b')
COMPARE_BEFORE = re.compile(r'(?:\bcase|[=!<>]=|[<>?:]|\|\||&&|\[)$')
COMPARE_AFTER = re.compile(r'(?:[=!<>]=|[<>?:\]]|\|\||&&)')
MACRO = re.compile(r'^\s*SWOFTY_(WRAP|WRAP_PRIORITY|OVERWRITE|VTABLE|VTABLE_PRIORITY|REDIRECT|SITE|SITE_PRIORITY|PATCH)'
                   r'\(([^)]*)', re.M)
MACRO_TARGET = {'WRAP': 0, 'WRAP_PRIORITY': 1, 'OVERWRITE': 1, 'VTABLE': 0, 'VTABLE_PRIORITY': 1, 'REDIRECT': 0,
                'SITE': 0, 'SITE_PRIORITY': 1, 'PATCH': 0}
MACRO_WRITE = {'WRAP': 'inline', 'WRAP_PRIORITY': 'inline', 'OVERWRITE': 'inline', 'VTABLE': 'vtable',
               'VTABLE_PRIORITY': 'vtable', 'REDIRECT': 'call', 'SITE': 'mid', 'SITE_PRIORITY': 'mid', 'PATCH': 'bytes'}

LAYOUTS = {
    'Sig': ('what', 'pattern', 'rva', 'kind', 'insn', 'anchor', 'len', 'section'),
    'core/assets/assets.cpp:Target': ('what', 'pattern', 'rva', 'kind', 'insn', 'anchor', 'len'),
    'core/assets/items.cpp:Target': ('what', 'pattern', 'rva'),
    'core/frontend/ui.cpp:Target': ('what', 'pattern', 'rva', 'anchor'),
}


class Fail(Exception):
    pass


def rel(path):
    return os.path.relpath(path, SRC).replace(os.sep, '/')


def sources():
    out = set()
    for g in SOURCES:
        out.update(glob.glob(os.path.join(SRC, g), recursive=True))
    return sorted(rel(f) for f in out)


_text = {}


def read(path):
    if path not in _text:
        full = os.path.join(SRC, path)
        if not os.path.isfile(full):
            raise Fail(f'{path} is not a file under native/src')
        with open(full, encoding='utf-8') as f:
            _text[path] = f.read()
    return _text[path]


def line_of(text, at):
    return text.count('\n', 0, at) + 1


def includes(path):
    return [i for i in re.findall(r'#include "([^"]+)"', read(path)) if os.path.isfile(os.path.join(SRC, i))]


def near(path):
    return [path] + includes(path)


def int_consts(path):
    out = {}
    for p in near(path):
        for n, v in sigdecl.int_consts(read(p)).items():
            out.setdefault(n, v)
    return out


def string_consts(path):
    out = {}
    for p in near(path):
        for n, v in sigdecl.string_consts(read(p)).items():
            out.setdefault(n, v)
    return out


def number(v, consts):
    try:
        return sigdecl.int_arg(v, consts)
    except ValueError:
        raise Fail(f'"{v.strip()}" is not a number or a constant this tool reads') from None


def string_pattern(path, name):
    s = string_consts(path).get(name)
    if s is None:
        raise Fail(f'{path}: no string constant {name}')
    return s


_sigs = {}


def signatures(path):
    if path in _sigs:
        return _sigs[path]
    out = {}
    try:
        decls = list(sigdecl.declarations(read(path), ('Sig', 'Target')))
    except ValueError as err:
        raise Fail(f'{path}: {err}') from None
    for kind, name, line, args in decls:
        layout = LAYOUTS.get(kind) or LAYOUTS.get(f'{path}:{kind}')
        if not layout:
            continue
        decl = f'{path}:{line}'
        try:
            s = dict(sigdecl.fields(args, layout, string_consts(path), int_consts(path)), decl=decl)
            if not s.get('what') or not s.get('pattern'):
                raise ValueError('it has no what or no pattern')
            if s['kind'] not in ('addr', 'rel32', 'riprel'):
                raise ValueError(f'kind {s["kind"]} is not addr, rel32 or riprel')
        except ValueError as err:
            s = {'error': f'{decl}: {name}: {err}'}
        out[name] = s
    _sigs[path] = out
    return out


def find_signature(path, name):
    if ':' in name:
        path, name = name.split(':', 1)
    folder = sorted(rel(f) for f in glob.glob(os.path.join(SRC, os.path.dirname(path), '*.hpp')))
    for p in near(path) + folder:
        s = signatures(p).get(name)
        if s:
            if 'error' in s:
                raise Fail(s['error'])
            return path, s
    raise Fail(f'{path}: no signature {name} in it, its includes or its folder')


def binding_ids(folder=None):
    if folder is None:
        with tempfile.TemporaryDirectory(prefix='patch_sites.') as tmp:
            subprocess.run([sys.executable, os.path.join(RE_TOOLS, 'bindgen.py'), '--json', os.path.join(tmp, 'bindings.json'),
                            '--symbols', os.path.join(tmp, 'symbols.tsv')], check=True, stdout=subprocess.DEVNULL)
            return binding_ids(tmp)
    kb = os.path.join(folder, 'bindings.json')
    if not os.path.isfile(kb):
        raise Fail(f'{folder}: no bindings.json')
    with open(kb) as f:
        ids = {b['id']: b.get('kind', '') for b in json.load(f)['bindings']}
    for f in glob.glob(os.path.join(RECORDS, '*', '*.json')):
        ids.setdefault(os.path.basename(f)[:-5], '')
    return ids


def read_list():
    helpers, entries = [], []
    with open(LIST, encoding='utf-8') as f:
        lines = f.read().split('\n')
    for n, line in enumerate(lines, 1):
        line = line.rstrip('\n')
        if not line.strip() or line.startswith('#'):
            continue
        if line.startswith('helper '):
            scope, _, pattern = line[len('helper '):].partition(' ')
            if not re.fullmatch(r'[\w/.]+\.(?:cpp|hpp|h|c)', scope):
                scope, pattern = '', line[len('helper '):]
            helpers.append((scope, re.compile(pattern)))
            continue
        key, sep, spec = line.partition(' => ')
        src, colon, code = key.partition(': ')
        words = spec.split()
        if not sep or not colon or not words:
            raise Fail(f'patch_sites.txt:{n}: not "<source>: <line> => <write> <targets>"')
        write = words[0]
        if not (write in WRITES or write in NO_SITE or re.fullmatch(r'bytes=\d+', write)):
            raise Fail(f'patch_sites.txt:{n}: unknown write "{write}"')
        entries.append({'src': src.strip(), 'code': code.strip(), 'write': write, 'words': words[1:], 'n': n})
    return helpers, entries


def install_lines(helpers):
    out = []
    for path in sources():
        for n, line in enumerate(read(path).split('\n'), 1):
            s = line.strip()
            if s.startswith(('//', '/*', '* ')) or s == '*':
                continue
            if PRIMITIVES.search(line) or any((not scope or scope == path) and h.search(line) for scope, h in helpers):
                out.append((path, s, n))
    return out


def stray_mixin_lines():
    out = []
    for path in sources():
        text = read(path)
        entries = {line_of(text, m.start()) for m in MIXIN_ENTRY.finditer(text)}
        for n, line in enumerate(text.split('\n'), 1):
            if n in entries or line.lstrip().startswith(('//', '#define')):
                continue
            for m in MIXIN_TOKEN.finditer(line):
                before, after = line[:m.start()].rstrip(), line[m.end():].lstrip()
                if not (COMPARE_BEFORE.search(before) or COMPARE_AFTER.match(after)):
                    out.append((path, line.strip(), n))
                    break
    return out


def site(owner, write, source, name, spec, length=None, tag=None):
    return {'owner': owner, 'write': write, 'source': source, 'name': name, 'spec': spec, 'length': length, 'tag': tag}


def entry_sites(e, ids):
    src, write, words = e['src'], e['write'], e['words']
    if write in NO_SITE:
        return []
    length = int(write.split('=')[1]) if write.startswith('bytes=') else None
    write = 'bytes' if write.startswith('bytes=') else write
    if write == 'import' and words[:1] != ['import']:
        words = ['import'] + words
    tag = words[words.index('tag') + 1].replace('_', ' ') if 'tag' in words else None
    if tag is None:
        m = re.search(r'\{\s*[A-Z_][A-Z0-9_]*\s*,\s*"([^"]+)"', e['code'])
        tag = m.group(1) if m else None
    out, i = [], 0

    def names():
        nonlocal i
        word = words[i]
        got = []
        i += 1
        while i < len(words) and words[i] not in TARGET_WORDS:
            got.append(words[i])
            i += 1
        if not got:
            raise Fail(f'patch_sites.txt:{e["n"]}: "{word}" names nothing')
        return got

    def binding(owner, b):
        if b not in ids:
            raise Fail(f'patch_sites.txt:{e["n"]}: {b} is not a binding (re/bindings)')
        out.append(site(owner, write, 'binding', b, b, length, tag))

    while i < len(words):
        w = words[i]
        if w == 'sig':
            for n in names():
                owner, s = find_signature(src, n)
                out.append(site(owner, write, 'signature', n.split(':')[-1], s, length, tag))
        elif w == 'sigs-in':
            for f in names():
                sigs = signatures(f)
                for n, s in sigs.items():
                    if 'error' in s:
                        raise Fail(s['error'])
                if not sigs:
                    raise Fail(f'patch_sites.txt:{e["n"]}: {f} declares no signature')
                out += [site(f, write, 'signature', n, s, length, tag) for n, s in sorted(sigs.items())]
        elif w == 'binding':
            for n in names():
                owner, n = n.split(':', 1) if ':' in n else (src, n)
                binding(owner, string_consts(owner).get(n, n))
        elif w == 'binding-consts':
            for n in names():
                found = re.findall(r'\b%s\s*=\s*"([^"]+)"' % re.escape(n), read(src))
                if not found:
                    raise Fail(f'patch_sites.txt:{e["n"]}: no "{n} = <string>" in {src}')
                for b in found:
                    binding(src, b)
        elif w == 'spec':
            for n in names():
                m = None
                for p in near(src):
                    m = m or re.search(r'\b%s\s*\{\s*"[^"]*"\s*,\s*"([^"]+)"' % re.escape(n), read(p))
                if not m:
                    raise Fail(f'patch_sites.txt:{e["n"]}: no Spec {n}{{"<point>", "<binding>"}} near {src}')
                binding(src, m.group(1))
        elif w == 'rel32':
            sig, off = words[i + 1], words[i + 2]
            owner, s = find_signature(src, sig)
            if s['kind'] != 'addr':
                raise Fail(f'patch_sites.txt:{e["n"]}: rel32 needs {sig} to resolve by address')
            at = s['anchor'] + number(off, int_consts(src))
            s = dict(s, what=f'{s["what"]}+{off}', rva=0, kind='rel32', insn=at - 1, anchor=at, len=5)
            out.append(site(owner, write, 'signature', f'{sig}+{off}', s, length, tag))
            i += 3
        elif w == 'prologue':
            rva, pattern = words[i + 1], words[i + 2]
            i += 3
            builds = {}
            while i + 1 < len(words) and words[i] == 'on':
                stamp, at = words[i + 1].split('=')
                builds[number(stamp, int_consts(src))] = number(at, int_consts(src))
                i += 2
            s = {'what': rva, 'pattern': string_pattern(src, pattern), 'rva': number(rva, int_consts(src)), 'kind': 'addr',
                 'insn': 0, 'anchor': 0, 'len': 0, 'section': ANY_CODE, 'builds': builds, 'decl': src}
            out.append(site(src, write, 'signature', rva, s, length, tag))
        elif w == 'import':
            for n in names():
                if not re.fullmatch(r'(?:[\w.\-]+|\*)!(?:\w+|\*)', n):
                    raise Fail(f'patch_sites.txt:{e["n"]}: import "{n}" is not <dll>!<function> or <dll>!*')
                out.append(site(src, write, 'import', n, n, 8, tag))
        elif w == 'tag':
            i += 2
        else:
            raise Fail(f'patch_sites.txt:{e["n"]}: unknown target word "{w}"')
    if not out:
        raise Fail(f'patch_sites.txt:{e["n"]}: a {write} entry names no site')
    return out


def mixin_sites(ids):
    out = []
    for path in sources():
        text = read(path)
        consts = string_consts(path)
        for m in MIXIN_ENTRY.finditer(text):
            t = m['target']
            name = sigdecl.literal(t) if t.startswith('"') else consts.get(t.split('::')[-1])
            if name is None:
                raise Fail(f'{path}:{line_of(text, m.start())}: mixin target {t} is not a string this tool reads')
            kind = m['kind'] or m['akind']
            length = number(m['arg'], int_consts(path)) if kind == 'PATCH' else None
            out.append(site(path, KINDS[kind], 'binding', name, name, length))
        for m in MACRO.finditer(text):
            args = sigdecl.split_args(m.group(2))
            idx = MACRO_TARGET[m.group(1)]
            if len(args) <= idx:
                continue
            length = len(args) - 1 if m.group(1) == 'PATCH' else None
            out.append(site(path, MACRO_WRITE[m.group(1)], 'binding', args[idx], args[idx], length))
    for s in out:
        if s['name'] not in ids:
            raise Fail(f'{s["owner"]}: mixin target {s["name"]} is not a binding (re/bindings)')
    return out


def all_sites(ids, entries):
    out = mixin_sites(ids)
    for e in entries:
        out += entry_sites(e, ids)
    return out


def problems(ids):
    helpers, entries = read_list()
    found = install_lines(helpers) + stray_mixin_lines()
    listed, seen = {}, {}
    for e in entries:
        listed.setdefault((e['src'], e['code']), []).append(e['n'])
    for p, s, n in found:
        seen.setdefault((p, s), []).append(n)
    bad = []
    for k, ns in sorted(seen.items()):
        have = len(listed.get(k, []))
        if have < len(ns):
            at = ','.join(map(str, ns[have:]))
            bad.append(f'{k[0]}:{at}: {k[1]}\n    writes code, and tools/patch_sites.txt does not list it')
    for k, ns in sorted(listed.items()):
        have = len(seen.get(k, []))
        if have < len(ns):
            at = ','.join(map(str, ns[have:]))
            bad.append(f'patch_sites.txt:{at}: {k[0]}: {k[1]}\n    is no longer in the source' +
                       (f' {len(ns)} times' if have else ''))
    try:
        sites = all_sites(ids, entries)
    except Fail as err:
        bad.append(str(err))
        sites = []
    return bad, found, sites


def check(folder=None):
    bad, found, sites = problems(binding_ids(folder))
    if bad:
        print('\n'.join(bad))
        print(f'patch_sites: {len(bad)} problem(s); see the top of native/tools/patch_sites.txt')
        return 1
    print(f'patch_sites: {len(found)} line(s) that write code are listed; they and the mixin tables name '
          f'{len(sites)} site(s)')
    return 0


def num(v, default=0):
    if isinstance(v, int):
        return v
    if isinstance(v, str) and v:
        return int(v, 0)
    return default


def section_alias(doc):
    given = doc.get('sections') or doc['build'].get('sections') or {}
    alias = {k: v for k, v in given.items() if isinstance(v, str)}
    uses = {}
    for b in doc['bindings']:
        uses[b.get('section')] = uses.get(b.get('section'), 0) + 1
    if not alias and uses and '.xtls' not in uses:
        alias['.xtls'] = max(uses, key=uses.get)
    return alias


def binding_where(b, kb):
    r = b.get('resolve') or {}
    kind = r.get('kind', 'addr') if isinstance(r.get('kind'), str) else 'addr'
    anchor, match = num(b.get('anchor')), num(b.get('match'), -1)
    if kb and b.get('rva') not in (None, ''):
        return {'rva': num(b['rva'])}
    if kind == 'addr' and match >= 0:
        return {'rva': match + anchor}
    out = {'rva': None, 'pattern': b.get('pattern') or '', 'section': b.get('section') or '', 'anchor': anchor,
           'resolve': kind}
    if kind != 'addr':
        out['next'] = anchor + 4 if kind == 'rel32' else num(r.get('insn')) + num(r.get('len'))
    if match >= 0:
        out['match'] = match
    if isinstance(b.get('string_anchor'), dict):
        out['string_anchor'] = b['string_anchor']
    return out


def signature_where(s, stamp, alias):
    if stamp in s.get('builds', {}):
        return {'rva': s['builds'][stamp]}
    out = {'rva': None, 'pattern': s['pattern'], 'section': alias.get(s['section'], s['section']), 'anchor': s['anchor'],
           'resolve': s['kind']}
    if s['kind'] != 'addr':
        out['next'] = s['insn'] + s['len']
    return out


def default_length(write, rec=None):
    if write == 'vtable':
        return 8 * max(1, num(rec.get('slots'), 1)) if rec else 8
    if write == 'call':
        return 5
    if write == 'import':
        return 8
    return HOOK_LENGTH


class Images:
    def __init__(self, paths):
        self.by_stamp, self._resolvers = {}, {}
        if not paths:
            return
        import bindverify
        from capstone import Cs, CS_ARCH_X86, CS_MODE_64
        self.bv, self.cs = bindverify, Cs(CS_ARCH_X86, CS_MODE_64)
        for p in paths:
            img = bindverify.Image(p)
            self.by_stamp[img.stamp] = img

    def scan(self, img, section, pattern):
        if section == ANY_CODE:
            names = sorted({s.name for s in img.sections if s.flags & self.bv.SCN_EXECUTE})
        else:
            names = [section]
        hits, hit = 0, None
        for n in names:
            h, _, first = img.scan(n, pattern)
            hits += h
            hit = first if h else hit
        return hit if hits == 1 else None

    def imports(self, img, name):
        dll, fn = name.split('!')
        pe = struct.unpack_from('<I', img.mem, 0x3c)[0]
        at = struct.unpack_from('<I', img.mem, pe + 24 + 112 + 8)[0]
        out = []
        while at and img.readable(at, 20):
            oft, _, _, dname, ft = struct.unpack_from('<IIIII', img.mem, at)
            if not dname:
                break
            if dll == '*' or (img.read(dname, len(dll) + 1) or b'').decode('latin-1').lower() == dll.lower() + '\0':
                names = oft or ft
                for k in range(65536):
                    thunk = struct.unpack_from('<Q', img.mem, names + 8 * k)[0]
                    if not thunk:
                        break
                    named = ''
                    if not thunk >> 63:
                        start = (thunk & 0x7fffffff) + 2
                        named = bytes(img.mem[start:img.mem.find(b'\0', start)]).decode('latin-1')
                    if fn == '*' or named == fn:
                        out.append(ft + 8 * k)
            at += 20
        return out

    def resolve(self, img, row, rec, doc):
        if row['rva'] is None and row.get('resolve') == 'import':
            slots = self.imports(img, row['import'])
            base = {k: v for k, v in row.items() if k not in ('resolve', 'import')}
            return [dict(base, rva=at, length=8, exact=True) for at in slots] or [
                dict(row, unresolved='the exe does not import it')]
        self.place(img, row, rec, doc)
        return [row]

    def place(self, img, row, rec, doc):
        if row['rva'] is None:
            if rec is not None:
                key = (id(img), id(doc))
                if key not in self._resolvers:
                    self._resolvers[key] = self.bv.Resolver(img, doc)
                row['rva'] = self._resolvers[key].resolve_binding(self.bv.Binding(rec))['target']
            else:
                hit = self.scan(img, row['section'], row['pattern'])
                if hit is not None:
                    at = hit + row['anchor']
                    row['rva'] = at if row['resolve'] == 'addr' else hit + row['next'] + img.i32(at)
            if row['rva'] is None:
                row['unresolved'] = 'does not resolve on this exe'
                return
            for k in ('pattern', 'section', 'anchor', 'resolve', 'next', 'match', 'string_anchor', 'import'):
                row.pop(k, None)
        if row['write'] in ('inline', 'mid') and not row['exact']:
            n = 0
            for i in self.cs.disasm(img.read(row['rva'], 64) or b'', row['rva']):
                n += i.size
                if n >= JUMP_SPAN:
                    break
            if n >= JUMP_SPAN:
                row['length'], row['exact'] = n, True


def generate(folder, out, images, version):
    bad, _, sites = problems(binding_ids(folder))
    if bad:
        print('\n'.join(bad))
        raise Fail(f'{len(bad)} problem(s), so no {os.path.basename(out)}; see the top of native/tools/patch_sites.txt')
    imgs = Images(images)
    builds, rows = [], []
    for path in sorted(glob.glob(os.path.join(folder, 'bindings*.json'))):
        name = os.path.basename(path)
        with open(path) as f:
            doc = json.load(f)
        b = doc['build']
        kb = name == 'bindings.json'
        sha, stamp = b.get('sha256', ''), num(b.get('pe_timestamp'))
        img = imgs.by_stamp.get(stamp)
        builds.append({'exe_sha256': sha, 'pe_timestamp': stamp, 'exe_size': num(b.get('size')),
                       'distribution': b.get('distribution', ''), 'packed': bool(b.get('packed')), 'set': name,
                       'resolved_here': img is not None})
        alias = section_alias(doc)
        records = {r['id']: r for r in doc['bindings']}
        done = set()
        for s in sites:
            key = (s['owner'], s['write'], s['source'], s['name'])
            if key in done:
                continue
            done.add(key)
            rec = None
            if s['source'] == 'binding':
                rec = records.get(s['spec'])
                if rec is None or rec.get('status') == 'unresolvable':
                    continue
                where = binding_where(rec, kb)
                length = s['length'] or default_length(s['write'], rec)
            elif s['source'] == 'import':
                where = {'rva': None, 'resolve': 'import', 'import': s['name']}
                length = 8
            else:
                where = signature_where(s['spec'], stamp, alias)
                length = s['length'] or default_length(s['write'])
            row = {'build': sha, 'owner': s['owner'], 'name': s['name'], 'source': s['source'], 'write': s['write'],
                   **where, 'length': length, 'exact': s['write'] not in ('inline', 'mid')}
            if s['source'] == 'import' and '*' in s['name']:
                row['exact'] = False
            if s['tag']:
                row['tag'] = s['tag']
            rows += imgs.resolve(img, row, rec, doc) if img is not None else [row]
    doc = {'schema': SCHEMA, 'module': MODULE, 'version': version, 'hook_length': HOOK_LENGTH, 'jump_span': JUMP_SPAN,
           'builds': builds, 'sites': rows}
    with open(out, 'w') as f:
        json.dump(doc, f, indent=1)
        f.write('\n')
    left = sum(1 for r in rows if r['rva'] is None)
    print(f'patch_sites: {os.path.basename(out)}: {len(rows)} site(s) on {len(builds)} build(s), {left} left for the exe '
          f'to resolve')
    return 0


def resolve_file(path, out, images, folder):
    with open(path) as f:
        doc = json.load(f)
    imgs = Images(images)
    sets = {}
    for p in sorted(glob.glob(os.path.join(folder, 'bindings*.json'))) if folder else []:
        with open(p) as f:
            d = json.load(f)
        sets[d['build'].get('sha256', '')] = (d, {r['id']: r for r in d['bindings']})
    on = {}
    for b in doc['builds']:
        img = imgs.by_stamp.get(b['pe_timestamp'])
        if img is not None:
            on[b['exe_sha256']] = img
            b['resolved_here'] = True
    if not on:
        raise Fail('no --image has the PE timestamp of a build in ' + os.path.basename(path))
    rows = []
    for row in doc['sites']:
        img = on.get(row['build'])
        if img is None:
            rows.append(row)
            continue
        setdoc, records = sets.get(row['build'], (None, {}))
        rec = records.get(row['name']) if row['source'] == 'binding' else None
        rows += imgs.resolve(img, row, rec, setdoc)
    doc['sites'] = rows
    with open(out, 'w') as f:
        json.dump(doc, f, indent=1)
        f.write('\n')
    left = sum(1 for r in rows if r['build'] in on and r['rva'] is None)
    print(f'patch_sites: {os.path.basename(out)}: {sum(1 for r in rows if r["build"] in on)} site(s) on '
          f'{len(on)} build(s) resolved here, {left} not found on the exe')
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--check', action='store_true', help='fail when a line that writes code is not listed')
    ap.add_argument('--sets', help='the folder bindgen writes bindings*.json to')
    ap.add_argument('--out', help='the patches.json to write')
    ap.add_argument('--image', action='append', default=[], help='an exe or dump to resolve its build\'s sites on')
    ap.add_argument('--resolve', help='a patches.json to resolve on --image')
    ap.add_argument('--core-version', default='')
    a = ap.parse_args(argv)
    try:
        if a.check:
            return check(a.sets)
        if a.resolve:
            if not a.image or not a.out:
                ap.error('--resolve needs --image and --out')
            return resolve_file(a.resolve, a.out, a.image, a.sets)
        if not a.sets or not a.out:
            ap.error('give --sets and --out, or --check')
        return generate(a.sets, a.out, a.image, a.core_version)
    except Fail as err:
        print(f'patch_sites: {err}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
