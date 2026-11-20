"""What a game update turns off in the core, read from the core's own tables (binding.py port).

The core keeps three gates per build: the bindings set it resolves (re/db/builds, the hint when the PE timestamp
matches, else the section scan), the layout proofs in native/src/core/runtime/build_proof.hpp (each Proof names the
builds whose struct offsets were checked, each Gate the feature that needs one), and the live state groups in
native/src/core/live/live_rows.hpp (a BuildProof per build lists the groups whose layouts were compared). This module
reads the last two from the C++ sources, so the report follows the code without a list of its own.
"""
import os, re

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
NATIVE = os.path.join(ROOT, 'native', 'src', 'core')
BUILD_PROOF = os.path.join(NATIVE, 'runtime', 'build_proof.hpp')
LIVE_ROWS = os.path.join(NATIVE, 'live', 'live_rows.hpp')
LIVE_MEM = os.path.join(NATIVE, 'live', 'live_mem.hpp')
LIVE_STATE = os.path.join(NATIVE, 'live', 'live_state.cpp')
QUALIFIERS = re.compile(r'\b(?:runtime::build_proof|swofty_core::live|bp|lv|runtime)::')
TOKEN = re.compile(r'\s*(?:(0x[0-9a-fA-F]+|\d+)[uUlL]*|([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)*(?:\.builds)?)|'
                   r'(<<|>>|[|&^~()+\-*,]))')


def read(path):
    with open(path, encoding='utf-8') as f:
        return f.read()


def rel(path):
    return os.path.relpath(path, ROOT).replace(os.sep, '/')


def line_of(text, pos):
    return text.count('\n', 0, pos) + 1


def strings(text):
    """The concatenation of the C string literals in text."""
    return ''.join(m.group(1).encode('latin-1', 'backslashreplace').decode('unicode_escape')
                   for m in re.finditer(r'"((?:[^"\\]|\\.)*)"', text))


def enum_values(text, name):
    m = re.search(r'enum(?:\s+class)?\s+' + name + r'\b[^{]*\{([^}]*)\}', text)
    if not m:
        raise ValueError(f'no enum {name}')
    return {n: i for i, n in enumerate(t.strip().split('=')[0].strip() for t in m.group(1).split(',') if t.strip())}


class Expr:
    """A C++ constant expression over integers, bit(...), names from `names` and | & ^ ~ << >> + - *."""

    def __init__(self, text, names, bit):
        self.toks, self.i = self.tokens(QUALIFIERS.sub('', text)), 0
        self.names, self.bit = names, bit
        self.used = set()

    @staticmethod
    def tokens(text):
        out, pos = [], 0
        text = text.strip()
        while pos < len(text):
            m = TOKEN.match(text, pos)
            if not m or m.end() == pos:
                raise ValueError(f'cannot read {text[pos:]!r}')
            out.append(('num', int(m.group(1), 0)) if m.group(1) else ('name', m.group(2)) if m.group(2)
                       else ('op', m.group(3)))
            pos = m.end()
        return out

    def peek(self):
        return self.toks[self.i] if self.i < len(self.toks) else (None, None)

    def take(self, op=None):
        t = self.peek()
        if op is not None and t != ('op', op):
            raise ValueError(f'expected {op!r}, got {t[1]!r}')
        self.i += 1
        return t

    def value(self):
        v = self.binary(0)
        if self.i != len(self.toks):
            raise ValueError(f'unexpected {self.peek()[1]!r}')
        return v & 0xffffffff

    LEVELS = (('|',), ('^',), ('&',), ('<<', '>>'), ('+', '-'), ('*',))

    def binary(self, level):
        if level == len(self.LEVELS):
            return self.unary()
        v = self.binary(level + 1)
        while self.peek()[0] == 'op' and self.peek()[1] in self.LEVELS[level]:
            op = self.take()[1]
            r = self.binary(level + 1)
            v = {'|': v | r, '^': v ^ r, '&': v & r, '<<': v << r, '>>': v >> r, '+': v + r, '-': v - r,
                 '*': v * r}[op]
        return v

    def unary(self):
        kind, val = self.take()
        if kind == 'num':
            return val
        if (kind, val) == ('op', '~'):
            return ~self.unary() & 0xffffffff
        if (kind, val) == ('op', '-'):
            return -self.unary()
        if (kind, val) == ('op', '('):
            v = self.binary(0)
            self.take(')')
            return v
        if kind == 'name' and val.split('::')[-1] == 'bit' and self.peek() == ('op', '('):
            self.take('(')
            arg = self.take()
            self.take(')')
            key = arg[1].split('::')[-1] if arg[0] == 'name' else None
            if key not in self.bit:
                raise ValueError(f'bit() of an unknown name {arg[1]!r}')
            return 1 << self.bit[key]
        if kind == 'name' and (val in self.names or val.split('::')[-1] in self.names):
            key = val if val in self.names else val.split('::')[-1]
            self.used.add(key)
            return self.names[key]
        raise ValueError(f'unknown name {val!r}')


def constants(text, names, bit, uses=None):
    """`constexpr uint32_t NAME = expr;` definitions in text, evaluated in order on top of names. uses, when given,
    gets the names each definition reads."""
    out = dict(names)
    for m in re.finditer(r'constexpr\s+uint32_t\s+([A-Z][A-Z0-9_]*)\s*=\s*([^;]+);', text):
        try:
            e = Expr(m.group(2), out, bit)
            out[m.group(1)] = e.value()
        except ValueError:
            continue
        if uses is not None:
            uses[m.group(1)] = e.used
    return out


def taken_from(used, uses):
    """The proofs whose `.builds` the names in used read, directly or through constants and other proofs, nearest
    first."""
    out, todo, seen = [], sorted(used), set()
    while todo:
        n = todo.pop(0)
        if n in seen:
            continue
        seen.add(n)
        if n.endswith('.builds'):
            out.append(n[:-len('.builds')])
        todo.extend(sorted(uses.get(n, ())))
    return out


def known_builds(text=None):
    """Build enum order and the KNOWN table of build_proof.hpp: [{build, name, pe_timestamp, size_of_image, larger}]."""
    text = read(BUILD_PROOF) if text is None else text
    order = enum_values(text, 'Build')
    body = re.search(r'KNOWN\[\]\s*=\s*\{(.*?)\n\};', text, re.S).group(1)
    rows = []
    for m in re.finditer(r'\{\s*Build::(\w+)\s*,\s*"([^"]*)"\s*,\s*(\w+?)u?\s*,\s*(\w+?)u?\s*,\s*(true|false)\s*\}', body):
        rows.append({'build': m.group(1), 'name': m.group(2), 'pe_timestamp': int(m.group(3), 0),
                     'size_of_image': int(m.group(4), 0), 'larger': m.group(5) == 'true'})
    return order, rows


def identify(rows, pe_timestamp, size_of_image):
    for k in rows:
        if pe_timestamp == k['pe_timestamp'] and (size_of_image > k['size_of_image'] if k['larger']
                                                  else size_of_image == k['size_of_image']):
            return k['build']
    return None


def native_sources(root=NATIVE):
    for d, dirs, files in os.walk(root):
        dirs.sort()
        for f in sorted(files):
            if f.endswith(('.cpp', '.hpp')):
                yield os.path.join(d, f)


PROOF = re.compile(r'(?:constexpr\s+)?(?:[A-Za-z_:]*::)?Proof\s+([A-Z][A-Z0-9_]*)\s*\{')
GATE = re.compile(r'(?:constexpr\s+)?(?:[A-Za-z_:]*::)?Gate\s+([A-Z][A-Z0-9_]*)\s*\{\s*"([^"]*)"\s*,\s*&\s*'
                  r'(?:[A-Za-z_:]*::)?([A-Z][A-Z0-9_]*)\s*(?:,\s*([^}]*))?\}')


def braced(text, start):
    """The text between the brace at start and its match, string literals skipped."""
    depth, i = 0, start
    while i < len(text):
        c = text[i]
        if c == '"':
            i += 1
            while text[i] != '"':
                i += 2 if text[i] == '\\' else 1
        elif c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
            if depth == 0:
                return text[start + 1:i]
        i += 1
    raise ValueError('unclosed brace')


def split_top(text):
    """text split at the commas outside parentheses and string literals."""
    parts, depth, cur, i = [], 0, '', 0
    while i < len(text):
        c = text[i]
        if c == '"':
            j = i + 1
            while text[j] != '"':
                j += 2 if text[j] == '\\' else 1
            cur += text[i:j + 1]
            i = j + 1
            continue
        if c in '([{':
            depth += 1
        elif c in ')]}':
            depth -= 1
        if c == ',' and depth == 0:
            parts.append(cur)
            cur = ''
        else:
            cur += c
        i += 1
    parts.append(cur)
    return [p.strip() for p in parts]


def proofs(order, files=None):
    """Every Proof the core declares, with the Gates (features) that need it. A proof whose builds are another
    proof's `.builds` (and more) names it in derived_from, and the other proof also lists its features, as
    "<feature> (through <proof>)": extending the other proof's builds turns them on too.

    [{name, what, builds: [build ids], file, line, features: [feature], bindings: [text], derived_from: [proof]}]"""
    bit = dict(order)
    uses = {}
    shared = constants(read(BUILD_PROOF), {}, bit, uses)
    out = []
    sources = list(native_sources() if files is None else files)
    sources.sort(key=lambda s: os.path.abspath(s) != os.path.abspath(BUILD_PROOF))
    for path in sources:
        text = read(path)
        if 'Proof' not in text:
            continue
        names = constants(text, shared, bit, uses)
        found = {}
        for m in PROOF.finditer(text):
            body = braced(text, m.end() - 1)
            parts = split_top(body)
            if len(parts) != 2:
                continue
            e = Expr(parts[1], names, bit)
            builds = e.value()
            p = {'name': m.group(1), 'what': strings(parts[0]),
                 'builds': sorted((b for b, i in order.items() if builds >> i & 1), key=order.get),
                 'file': rel(path), 'line': line_of(text, m.start()), 'features': [], 'bindings': [],
                 'derived_from': []}
            uses[p['name'] + '.builds'] = e.used
            found[p['name']] = p
            shared[p['name'] + '.builds'] = names[p['name'] + '.builds'] = builds
            out.append(p)
        for m in GATE.finditer(text):
            target = found.get(m.group(3)) or next((p for p in out if p['name'] == m.group(3)), None)
            if target:
                target['features'].append(m.group(2))
                if m.group(4) and m.group(4).strip():
                    target['bindings'].append(m.group(4).strip())
    users = sorted(rel(p) for p in sources
                   if 'verified_build()' in read(p) and not p.endswith(('build.hpp', 'stage.cpp')))
    for p in out:
        if p['name'] == 'VERIFIED_LAYOUT':
            p['features'] += [f'the code that calls verified_build() in {u}' for u in users]
    own = {p['name']: list(p['features']) for p in out}
    by_name = {p['name']: p for p in out}
    for p in out:
        p['derived_from'] = [n for n in taken_from(uses[p['name'] + '.builds'], uses) if n in by_name]
        for n in p['derived_from']:
            by_name[n]['features'] += [f'{f} (through {p["name"]})' for f in own[p['name']]]
    return out


def entries(text, head):
    """The top-level brace entries of the initializer that follows head in text, each split at its commas."""
    start = text.index('{', text.index(head) + len(head))
    return [split_top(braced(e, 0)) for e in split_top(braced(text, start)) if e.startswith('{')]


def live_groups(order, rows=None):
    """The live state groups and the builds that read each: {groups, needs, sites, builds: [{build, name, symbol,
    groups, field_proofs, line}]}."""
    mem, rows_text, state = read(LIVE_MEM), read(LIVE_ROWS) if rows is None else rows, read(LIVE_STATE)
    groups = enum_values(mem, 'Group')
    count = groups.pop('G_COUNT')
    bit = dict(groups, **order)
    names = constants(rows_text, {'G_COUNT': count}, bit)
    needs = {e[0]: strings(e[3]) for e in entries(rows_text, 'ROWS[G_COUNT]') if len(e) == 4}
    site_names = {m.group(2): m.group(1)
                  for m in re.finditer(r'\{"([A-Za-z0-9_]+)",\s*&lv::Sites::(\w+)\}', state)}
    sites = {e[0]: [site_names.get(x, x) for x in re.findall(r'&Sites::(\w+)', e[1])]
             for e in entries(rows_text, 'NEEDS[]') if len(e) == 2}
    builds = []
    for m in re.finditer(r'BuildProof\s+([A-Z][A-Z0-9_]*)\s*=\s*\{', rows_text):
        parts = split_top(braced(rows_text, m.end() - 1))
        b = re.search(r'Build::(\w+)', parts[0])
        mask = Expr(parts[2], names, bit).value()
        field = Expr(parts[5], names, bit).value() if len(parts) > 5 else 0
        builds.append({'build': b.group(1) if b else None, 'name': strings(parts[1]), 'symbol': m.group(1),
                       'groups': [g for g, i in groups.items() if mask >> i & 1],
                       'field_proofs': [g for g, i in groups.items() if field >> i & 1],
                       'line': line_of(rows_text, m.start())})
    return {'groups': list(groups), 'needs': needs, 'sites': sites, 'builds': builds}


def layout_report(build, resolves=None):
    """What stays off on `build` (a Build id from build_proof.hpp, or None for an exe the table does not know), and the
    maintainer's step for each. resolves(name) -> bool says whether a binding resolves on that exe (None: not asked)."""
    order, known = known_builds()
    out = {'build': build, 'known': [k['build'] for k in known], 'proofs': [], 'unused': [], 'live': []}
    for p in proofs(order):
        if build in p['builds']:
            continue
        if not p['features']:
            out['unused'].append(p['name'])
            continue
        where = f'{p["file"]}:{p["line"]}'
        do = (f'check {p["what"]} on the new build (sigport.py --witness of each function that uses the offsets, or '
              f'layoutcmp.py), then add bit(Build::{build or "<new>"}) to {p["name"]} at {where}')
        if p['derived_from']:
            do += f', or to {p["derived_from"][0]}, whose builds it takes'
        out['proofs'].append({'proof': p['name'], 'what': p['what'], 'where': where, 'proven_on': p['builds'],
                              'off': p['features'], 'do': do})
    live = live_groups(order)
    entry = next((b for b in live['builds'] if b['build'] == build), None) if build else None
    for g in live['groups']:
        missing = [s for s in live['sites'].get(g, []) if resolves and not resolves(s)]
        if entry and g in entry['groups'] and not missing:
            continue
        why, do = [], []
        if not entry or g not in entry['groups']:
            why.append('its layouts are not compared on this build')
            target = (f'{entry["symbol"]} ({rel(LIVE_ROWS)}:{entry["line"]})' if entry else
                      f'a new BuildProof in PROVEN_BUILDS ({rel(LIVE_ROWS)})')
            do.append(f'compare {live["needs"].get(g, g)} with re/tools/layoutcmp.py, then add bit({g}) to the groups '
                      f'of {target}')
        if missing:
            why.append(f'{", ".join(missing)} does not resolve')
            do.append(f'bind {", ".join(missing)} on the new build (sigport.py --port, then a hint in its overlay)')
        out['live'].append({'group': g, 'needs': live['needs'].get(g, ''), 'why': why, 'do': do})
    if build is None:
        out['first'] = (f'add the build to enum Build and KNOWN in {rel(BUILD_PROOF)} (its PE timestamp and '
                        f'SizeOfImage), and a BuildProof for it to PROVEN_BUILDS in {rel(LIVE_ROWS)}; until then no '
                        f'proof and no live group holds on it')
    return out
