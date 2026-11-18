#!/usr/bin/env python3
"""Caller census: the direct calls, jumps and thunks that reach an address in one build, from every executable section.

  census.py show BUILD ADDR... [--image PATH] [--sites]   the census of each address (hex VA or RVA)
  census.py update BUILD [--image PATH]                   rewrite re/db/census/BUILD.tsv for the addresses the KB cites
  census.py check BUILD [--image PATH]                    exit 1 when a stored row differs from a fresh census
  census.py hand                                          the notes that count callers by hand

A note cites a stored row as `callers: census BUILD RVA`, the RVA in lowercase hex as the file holds it, in place of a
caller count of its own. BUILD names a file under re/db/builds. The image is that build's exe or flat dump, by default
the one the environment variable in IMAGE_ENV names.

The census matches rel32 byte patterns in every section the image maps executable, writable ones included; it does not
disassemble. For an address it counts:
  calls          E8 rel32 to the address
  callers        the functions those calls sit in: the .pdata entry of the call, followed through chained unwind info to
                 the function's first entry; a call outside every entry is a caller of its own
  sections       the calls per section
  thunks         stubs whose first instruction is an E9 to the address or to one of its thunks: the jmp sits outside
                 every .pdata entry or at the start of one, after nops, int3 padding, a ret on a 16-byte boundary
                 or another such stub; the stub's entry is its first nop, or the jmp
  thunk_calls    E8 rel32 to a thunk's entry or to its jmp, and thunk_callers the functions they sit in
  jumps          E9 and 0F 8x rel32 to the address or a thunk that do not start a thunk
  lea            48/4C 8D RIP-relative loads of the address or a thunk
  pointers       8-byte absolute addresses of the address or a thunk anywhere in the image
"""
import argparse, bisect, functools, json, os, re, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import bindverify  # noqa: E402

RE_DIR = os.path.dirname(HERE)
DB = os.path.join(RE_DIR, 'db')
CENSUS_DIR = os.path.join(DB, 'census')
BUILDS_DIR = os.path.join(DB, 'builds')
STRUCTS_DIR = os.path.join(DB, 'structs')
IMAGE_ENV = {'steam': 'SWOFTY_STEAM_EXE', 'steam-26566308': 'SWOFTY_STEAM_145_EXE', 'ea': 'SWOFTY_EA_DUMP',
             'ea-1-0-145': 'SWOFTY_EA_145_DUMP'}
COLUMNS = ('rva', 'calls', 'callers', 'sections', 'thunks', 'thunk_calls', 'thunk_callers', 'jumps', 'lea', 'pointers')
CITED_TABLES = ('functions', 'globals', 'db_keys', 'verify')

BRANCH = re.compile(rb'[\xe8\xe9]|\x0f[\x80-\x8f]|[\x48\x4c]\x8d[\x05\x0d\x15\x1d\x25\x2d\x35\x3d]')
KINDS = {0xe8: 'call', 0xe9: 'jmp', 0x0f: 'jcc', 0x48: 'lea', 0x4c: 'lea'}
NOPS = tuple(bytes.fromhex(h) for h in ('662e0f1f840000000000', '660f1f840000000000', '0f1f840000000000',
                                        '0f1f8000000000', '660f1f440000', '0f1f440000', '0f1f4000', '0f1f00', '6690',
                                        '90'))
I32 = struct.Struct('<i').unpack_from
Q64 = struct.Struct('<Q').unpack_from

CITE = re.compile(r'\bcensus ([a-z0-9][a-z0-9.-]*) (0x[0-9a-fA-F]+)\b')
_N = (r'(?:\d[\d,]*\+?|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|a single|single|only one|sole'
      r'|no)')
_Q = r'(?:(?:direct|distinct|other|e8|e9|static|known|code|real|more|unique|separate|external|rel32|indirect)\s+)*'
HAND = re.compile(rf'\b{_N}\s+{_Q}(?:callers?|call[- ]?sites?|calling functions|xrefs?|thunks?)\b'
                  rf'|\bcalled (?:from|by|in) {_N}\s'
                  rf'|\b\d[\d,]*\+?\s+{_Q}calls?\b(?=\s+(?:from|in)\s)'
                  rf'|\b\d[\d,]*\+?\s+direct calls?\b', re.I)


class Functions:
    """The exception directory: .pdata entries sorted by start, each followed to the first entry of its function."""

    def __init__(self, img):
        self.mem = img.mem
        va, size = img.exception
        rows = []
        if va and size >= 12 and img.readable(va, size - size % 12):
            rows = sorted(r for r in struct.iter_unpack('<III', bytes(img.mem[va:va + size - size % 12])) if r[0])
        self.img = img
        self.begins = [r[0] for r in rows]
        self.ends = [r[1] for r in rows]
        self.unwind = [r[2] for r in rows]
        self._roots = {}

    def index(self, rva):
        i = bisect.bisect_right(self.begins, rva) - 1
        return i if i >= 0 and rva < self.ends[i] else None

    def root(self, i):
        if i in self._roots:
            return self._roots[i]
        begin, u = self.begins[i], self.unwind[i]
        for _ in range(32):
            if u & 1:
                if not self.img.readable(u & ~1, 12):
                    break
                begin, _, u = struct.unpack_from('<III', self.mem, u & ~1)
                continue
            if not self.img.readable(u, 4) or not (self.mem[u] >> 3) & 4:
                break
            at = u + 4 + 2 * ((self.mem[u + 2] + 1) & ~1)
            if not self.img.readable(at, 12):
                break
            begin, _, u = struct.unpack_from('<III', self.mem, at)
        self._roots[i] = begin
        return begin

    def caller(self, site):
        """The function a site sits in: its first entry's start, or ('site', rva) outside every entry."""
        i = self.index(site)
        return ('site', site) if i is None else self.root(i)


def exec_sections(img):
    return [s for s in img.sections if s.flags & bindverify.SCN_EXECUTE and s.size and s.va < img.size]


def branches(img, want, sections):
    """{target: [(site, kind)]} for every rel32 in the sections that reaches an RVA in `want`; the site is the opcode's
    RVA and the kind call, jmp, jcc or lea."""
    mem, out = img.mem, {}
    for s in sections:
        end = min(s.va + s.size, img.size) - 4
        for m in BRANCH.finditer(mem, s.va, end):
            q = m.end()
            t = q + 4 + I32(mem, q)[0]
            if t in want:
                out.setdefault(t, []).append((m.start(), KINDS[mem[m.start()]]))
    return out


def pointers(img, want):
    """{target: [rva]} for every 8-byte absolute address of an RVA in `want`, at any alignment in the whole image."""
    base, out = img.image_base, {}
    lo = base & 0xffffffff
    vas = {base + t: t for t in want}
    if lo + img.size <= 1 << 32:
        pat = re.compile(b'[\\x%02x-\\x%02x]' % (lo >> 24, (lo + img.size - 1) >> 24) + re.escape(struct.pack('<I', base >> 32)))
        for m in pat.finditer(img.mem, 3):
            t = vas.get(Q64(img.mem, m.start() - 3)[0])
            if t is not None:
                out.setdefault(t, []).append(m.start() - 3)
    else:
        for va, t in vas.items():
            needle, at = struct.pack('<Q', va), img.mem.find(struct.pack('<Q', va))
            while at >= 0:
                out.setdefault(t, []).append(at)
                at = img.mem.find(needle, at + 1)
    return out


def thunk_entry(img, funcs, j, depth=0):
    """The entry of the stub whose first instruction is the E9 at j, or None when the jmp ends other code."""
    mem, e = img.mem, j
    for _ in range(8):
        n = next((n for n in NOPS if e >= len(n) and mem[e - len(n):e] == n), None)
        if n is None:
            break
        e -= len(n)
    i = funcs.index(j)
    if i is not None:
        b = funcs.begins[i]
        return b if e <= b <= j else None
    if e != j:
        return e
    if j and mem[j - 1] == 0xcc or j % 16 == 0 and mem[j - 1] == 0xc3:
        return j
    if j >= 5 and mem[j - 5] == 0xe9 and depth < 8 and thunk_entry(img, funcs, j - 5, depth + 1) is not None:
        return j
    return None


class Census:
    """The census of a set of RVAs in one image. Every thunk found leads one more scan, until none is new."""

    def __init__(self, img, rvas):
        self.img, self.funcs, self.sections = img, Functions(img), exec_sections(img)
        self.rvas = sorted(set(rvas))
        self.hits, self.stub, self.entry = {}, {}, {}
        todo = set(self.rvas)
        while todo:
            found = branches(img, todo, self.sections)
            new = set()
            for t in sorted(todo):
                self.hits[t] = sorted(found.get(t, []))
                for site, kind in self.hits[t]:
                    e = thunk_entry(img, self.funcs, site) if kind == 'jmp' else None
                    if e is None:
                        continue
                    self.entry[site] = e
                    for a in {e, site} - {t}:
                        if a not in self.stub:
                            self.stub[a] = t
                            if a not in self.hits:
                                new.add(a)
            todo = new - set(self.hits)
        self.ptrs = {t: sorted(v) for t, v in pointers(img, set(self.hits)).items()}

    def reach(self, rva):
        """rva and every thunk address whose chain of jmps ends at it."""
        out, edge = {rva}, {rva}
        while edge:
            edge = {a for a, t in self.stub.items() if t in edge and a not in out}
            out |= edge
        return out

    def detail(self, rva):
        addrs = self.reach(rva)
        d = {'calls': [], 'thunks': set(), 'thunk_calls': [], 'jumps': [], 'lea': [], 'pointers': []}
        for a in sorted(addrs):
            for site, kind in self.hits.get(a, []):
                if kind == 'call':
                    d['calls' if a == rva else 'thunk_calls'].append((site, a))
                elif kind == 'lea':
                    d['lea'].append((site, a))
                elif site in self.entry and self.entry[site] in addrs:
                    d['thunks'].add(self.entry[site])
                else:
                    d['jumps'].append((site, a))
            d['pointers'] += [(p, a) for p in self.ptrs.get(a, [])]
        d['thunks'] = sorted(d['thunks'])
        for k in ('calls', 'thunk_calls', 'jumps', 'lea', 'pointers'):
            d[k].sort()
        return d

    def section(self, rva):
        s = self.img.section_of(rva)
        return s.name if s else '?'

    def row(self, rva):
        d, f = self.detail(rva), self.funcs
        per = {}
        for site, _ in d['calls']:
            per[self.section(site)] = per.get(self.section(site), 0) + 1
        return {'rva': f'0x{rva:x}', 'calls': len(d['calls']),
                'callers': len({f.caller(s) for s, _ in d['calls']}),
                'sections': ','.join(f'{k}:{v}' for k, v in sorted(per.items())) or '-',
                'thunks': len(d['thunks']), 'thunk_calls': len(d['thunk_calls']),
                'thunk_callers': len({f.caller(s) for s, _ in d['thunk_calls']}),
                'jumps': len(d['jumps']), 'lea': len(d['lea']), 'pointers': len(d['pointers'])}


def summary(row):
    s = (f'{row["calls"]} calls from {row["callers"]} functions'
         + (f' ({row["sections"].replace(":", " ").replace(",", ", ")})' if row['sections'] != '-' else '')
         + f'; {row["thunks"]} thunks with {row["thunk_calls"]} calls from {row["thunk_callers"]} functions'
         + f'; {row["jumps"]} jumps; {row["lea"]} lea; {row["pointers"]} pointers')
    return s


def overlay(build):
    path = os.path.join(BUILDS_DIR, f'{build}.json')
    if not re.fullmatch(r'[a-z0-9][a-z0-9.-]*', build) or not os.path.isfile(path):
        raise SystemExit(f'census: no build overlay re/db/builds/{build}.json')
    with open(path) as f:
        return json.load(f)


def builds():
    return sorted(f[:-5] for f in os.listdir(BUILDS_DIR) if f.endswith('.json')) if os.path.isdir(BUILDS_DIR) else []


def kb_build():
    """The overlay that describes the build signatures.json was written against: the build the notes describe."""
    return _kb_build(BUILDS_DIR, os.path.join(DB, 'signatures.json'))


@functools.lru_cache(maxsize=None)
def _kb_build(builds_dir, signatures):
    with open(signatures) as f:
        sha = json.load(f)['build']['sha256']
    names = [b for b in builds() if overlay(b)['build']['sha256'] == sha]
    return names[0] if len(names) == 1 else None


def census_path(build):
    return os.path.join(CENSUS_DIR, f'{build}.tsv')


def header(build):
    return f'# census of {build} (sha256 {overlay(build)["build"]["sha256"]}): python3 re/tools/census.py update {build}'


def load(build):
    """(rows by RVA, errors) of re/db/census/BUILD.tsv; ({}, []) when the build has no census."""
    path, rel = census_path(build), f'census/{build}.tsv'
    if not os.path.isfile(path):
        return {}, []
    with open(path, encoding='utf-8') as f:
        lines = f.read().split('\n')
    errs, rows, last = [], {}, -1
    if lines[-1:] == ['']:
        lines.pop()
    else:
        errs.append(f'{rel}: no newline at the end')
    if lines[:1] != [header(build)]:
        errs.append(f'{rel}:1: the header must read {header(build)!r}; the overlay changed, so rewrite it with '
                    f'census.py update {build}')
    if lines[1:2] != ['\t'.join(COLUMNS)]:
        errs.append(f'{rel}:2: the columns must be {" ".join(COLUMNS)}')
        return rows, errs
    for n, line in enumerate(lines[2:], 3):
        cells = line.split('\t')
        if len(cells) != len(COLUMNS):
            errs.append(f'{rel}:{n}: {len(cells)} cells, want {len(COLUMNS)}')
            continue
        r = dict(zip(COLUMNS, cells))
        if not re.fullmatch(r'0x[0-9a-f]+', r['rva']) or not all(re.fullmatch(r'0|[1-9]\d*', r[c]) for c in COLUMNS[1:]
                                                                  if c != 'sections'):
            errs.append(f'{rel}:{n}: malformed row')
            continue
        if not re.fullmatch(r'-|[^\s:,]+:[1-9]\d*(?:,[^\s:,]+:[1-9]\d*)*', r['sections']):
            errs.append(f'{rel}:{n}: sections must be name:count pairs joined by commas, or -')
            continue
        rva = int(r['rva'], 16)
        if rva <= last:
            errs.append(f'{rel}:{n}: rows must be sorted by RVA, each once')
        last = max(last, rva)
        rows[rva] = {c: (r[c] if c in ('rva', 'sections') else int(r[c])) for c in COLUMNS}
    return rows, errs


def render(build, rows):
    out = [header(build), '\t'.join(COLUMNS)]
    out += ['\t'.join(str(r[c]) for c in COLUMNS) for _, r in sorted((int(r['rva'], 16), r) for r in rows)]
    return '\n'.join(out) + '\n'


def citations(tables=None):
    """[(where, build, address text)] for every census citation in the KB tables, the binding records and structs."""
    if tables is None:
        import kb
        tables = {t: kb.read_table(t)[0] for t in CITED_TABLES}
    out = []
    for t in CITED_TABLES:
        for r in tables.get(t, []):
            who = r.get('name') or r.get('item') or r.get('key') or '?'
            for v in r.values():
                if isinstance(v, str):
                    out += [(f'{t} {who}', m.group(1), m.group(2)) for m in CITE.finditer(v)]
    for f in sorted(os.listdir(STRUCTS_DIR)) if os.path.isdir(STRUCTS_DIR) else []:
        if f.endswith('.h'):
            with open(os.path.join(STRUCTS_DIR, f), encoding='utf-8', errors='replace') as fh:
                out += [(f'structs/{f}', m.group(1), m.group(2)) for m in CITE.finditer(fh.read())]
    return out


def citation_errors(tables=None):
    """(errors, warnings) for the census files and every citation of them."""
    errs, warns, stored = [], [], {}
    for b in builds():
        rows, e = load(b)
        errs += e
        if os.path.isfile(census_path(b)):
            stored[b] = rows
    for f in sorted(os.listdir(CENSUS_DIR)) if os.path.isdir(CENSUS_DIR) else []:
        if not f.endswith('.tsv') or f[:-4] not in stored:
            errs.append(f'census/{f}: no build overlay re/db/builds/{f[:-4]}.json')
    cited = {}
    for where, b, text in citations(tables):
        rva = int(text, 16)
        cited.setdefault(b, set()).add(rva)
        if b not in stored:
            errs.append(f'{where}: cites census {b} {text}, and re/db/census/{b}.tsv does not exist '
                        f'(python3 re/tools/census.py update {b})')
        elif text != f'0x{rva:x}':
            errs.append(f'{where}: census {b} {text}: write the RVA in lowercase hex as 0x{rva:x}')
        elif rva >= int(overlay(b)['build']['image_base'], 16):
            errs.append(f'{where}: census {b} {text} is a VA; cite the RVA 0x{rva - int(overlay(b)["build"]["image_base"], 16):x}')
        elif rva not in stored[b]:
            errs.append(f'{where}: cites census {b} {text}, which re/db/census/{b}.tsv has no row for '
                        f'(python3 re/tools/census.py update {b})')
    for b, rows in sorted(stored.items()):
        loose = sorted(set(rows) - cited.get(b, set()))
        if loose:
            warns.append(f'census/{b}.tsv: {len(loose)} row(s) nothing cites ({", ".join(f"0x{r:x}" for r in loose[:4])}'
                         f'{" and more" if len(loose) > 4 else ""}): census.py update {b} drops them')
    return errs, warns


def hand_counts(text):
    return [m.group(0).strip() for m in HAND.finditer(text or '')]


def active():
    """The KB build's name when it has a census, so notes cite it in place of their own caller counts."""
    b = kb_build()
    return b if b and os.path.isfile(census_path(b)) else None


def hand_rows(tables):
    out = []
    for t in ('functions', 'globals', 'db_keys'):
        for r in tables.get(t, []):
            found = hand_counts(r.get('notes', ''))
            if found:
                out.append((r.get('name') or r.get('key'), found))
    return out


def image_for(build, path):
    doc = overlay(build)
    path = path or os.environ.get(IMAGE_ENV.get(build, ''), '')
    if not path:
        raise SystemExit(f'census: pass --image, or set {IMAGE_ENV.get(build, "--image")} to the {build} image')
    img = bindverify.Image(path)
    if img.stamp != doc['build']['pe_timestamp']:
        raise SystemExit(f'census: {path} has PE timestamp {img.stamp}, and {build} is {doc["build"]["pe_timestamp"]}')
    return img


def parse_addr(text, base):
    v = int(text, 16)
    return v - base if v >= base else v


def cited_rvas(build):
    base = int(overlay(build)['build']['image_base'], 16)
    return sorted({int(t, 16) for _, b, t in citations() if b == build and int(t, 16) < base})


def cmd_show(a):
    img = image_for(a.build, a.image)
    rvas = [parse_addr(x, img.image_base) for x in a.addr]
    c = Census(img, rvas)
    for rva in rvas:
        row = c.row(rva)
        print(f'census {a.build} 0x{rva:x} (VA 0x{img.image_base + rva:x}): {summary(row)}')
        if a.sites:
            d, base = c.detail(rva), img.image_base
            for k in ('calls', 'thunk_calls', 'jumps', 'lea', 'pointers'):
                for site, via in d[k]:
                    who = c.funcs.caller(site) if k not in ('pointers',) else None
                    where = '' if who is None else (' outside .pdata' if isinstance(who, tuple) else f' in 0x{base + who:x}')
                    print(f'  {k[:-1] if k != "lea" else "lea"} 0x{base + site:x} {c.section(site)}{where}'
                          + (f' via 0x{base + via:x}' if via != rva else ''))
            for e in d['thunks']:
                print(f'  thunk 0x{base + e:x} {c.section(e)}')
    return 0


def cmd_update(a):
    img = image_for(a.build, a.image)
    rvas = cited_rvas(a.build)
    c = Census(img, rvas)
    os.makedirs(CENSUS_DIR, exist_ok=True)
    text = render(a.build, [c.row(r) for r in rvas])
    with open(census_path(a.build) + '.tmp', 'w', encoding='utf-8') as f:
        f.write(text)
    os.replace(census_path(a.build) + '.tmp', census_path(a.build))
    print(f'census.py: {len(rvas)} row(s) in re/db/census/{a.build}.tsv')
    return 0


def check(build, img):
    """Every difference between the stored census of a build and a fresh one, plus cited RVAs it lacks."""
    rows, errs = load(build)
    c = Census(img, sorted(rows))
    for rva, r in sorted(rows.items()):
        now = c.row(rva)
        if now != r:
            errs.append(f'census/{build}.tsv 0x{rva:x}: stored {summary(r)}; the image gives {summary(now)}')
    errs += [f'census/{build}.tsv: no row for the cited 0x{r:x}' for r in cited_rvas(build) if r not in rows]
    return errs


def cmd_check(a):
    errs = check(a.build, image_for(a.build, a.image))
    for e in errs:
        print('ERROR', e)
    print(f'census.py check: {len(errs)} error(s)')
    return 1 if errs else 0


def cmd_hand(a):
    import kb
    rows = hand_rows({t: kb.read_table(t)[0] for t in ('functions', 'globals', 'db_keys')})
    for name, found in rows:
        print(f'{name}\t{"; ".join(found)}')
    print(f'census.py hand: {len(rows)} note(s) count callers by hand')
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest='cmd', required=True)
    p = sp.add_parser('show')
    p.add_argument('build')
    p.add_argument('addr', nargs='+')
    p.add_argument('--image')
    p.add_argument('--sites', action='store_true')
    for n in ('update', 'check'):
        p = sp.add_parser(n)
        p.add_argument('build')
        p.add_argument('--image')
    sp.add_parser('hand')
    a = ap.parse_args(argv)
    return {'show': cmd_show, 'update': cmd_update, 'check': cmd_check, 'hand': cmd_hand}[a.cmd](a)


if __name__ == '__main__':
    sys.exit(main())
