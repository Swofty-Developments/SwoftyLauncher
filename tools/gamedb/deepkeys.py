#!/usr/bin/env python3
"""Find the unnamed keys under a game-database row.

The database stores FNV-1a 64 hashes of key paths, so a key no catalogue names
is found by hashing candidate paths `<row>.<w1>[.<w2>[.<w3>]]` and testing them
against the file's key set. Each level draws words from vocabularies:

  exe            identifiers in F1_25.exe (`strings -n 5`) and the formats words
  formats        identifiers F1_25.exe builds with printf, such as colour_%u_default,
                 each integer conversion expanded over 0..31
  catalog        path segments of tools/gamedb/catalog/keys.tsv.gz, names.txt,
                 compositions.txt and packages/core/src/games/f1_25/activation/row_keys.txt.gz
  containers     the catalog segments that are not a key's last segment
  labels         identifier values of the database's id records (row labels)
  table:<path>   the labels of a table's rows, such as table:car_setup.sliders.slider
  file:<path>    one word per line
  list:a,b,c     the listed words

The prefix is hashed once and each suffix continues from it. When the last
level has at most --unwind-max words, every key is unwound by each of them
(FNV-1a is invertible) and the earlier levels are searched against those
container hashes, so a wide middle level stays cheap.
"""
import argparse
import gzip
import os
import re
import subprocess
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
sys.path.insert(0, os.path.join(HERE, '..'))
import gamedb  # noqa: E402

P = np.uint64(gamedb.FNV_PRIME)
PINV = np.uint64(gamedb.FNV_PRIME_INV)
DB_REL = os.path.join('game_package', 'gamedatabases', 'gamedatabases.2025.bin')
WORD = re.compile(rb'[A-Za-z_][A-Za-z0-9_]{1,127}')
CONVERSION = rb'%0?\d*(?:\.\d+)?(?:I64|I32|hh|ll|h|l|z|j|t)?[diuxX]'
FORMAT = re.compile(rb'(?:[A-Za-z0-9_]|' + CONVERSION + rb')+')
SPEC = re.compile(CONVERSION)
FORMAT_TOP = 32
FORMAT_MAX_SPECS = 2
IDENT = re.compile(r'^[A-Za-z_][A-Za-z0-9_]*$')
ROW_INDEX = re.compile(r'_(\d+|N)$')
SELECTOR = re.compile(r'^(.*?)\[([^=\]]+)=([^\]]*)\](.*)$')


class Db:
    def __init__(self, path):
        self.recs = gamedb.read(open(path, 'rb').read())
        self.rec = {r.key: r for r in self.recs}
        self.keys = np.unique(np.fromiter(self.rec.keys(), dtype=np.uint64))

    def text(self, path):
        r = self.rec.get(gamedb.fnv1a64(path))
        return None if r is None else r.value.decode('utf-8', 'replace')

    def rows(self, table):
        container, word = table.rsplit('.', 1)
        count = self.text('%s._%s_instancecount_' % (container, word))
        n = int(count) if count and count.strip().isdigit() else 0
        i = n
        while self.text('%s_%d._metadata_' % (table, i)) is not None:
            i += 1
        return ['%s_%d' % (table, k) for k in range(i)]

    def resolve(self, path):
        while True:
            m = SELECTOR.match(path)
            if not m:
                return path
            table, field, want, rest = m.groups()
            hits = [r for r in self.rows(table) if self.text('%s.%s' % (r, field)) == want]
            if len(hits) != 1:
                sys.exit('%s[%s=%s]: %d rows match' % (table, field, want, len(hits)))
            path = hits[0] + rest


def has(sorted_keys, h):
    i = np.searchsorted(sorted_keys, h)
    i[i >= len(sorted_keys)] = 0
    return sorted_keys[i] == h


def extend(h, word):
    h = h.copy()
    for c in word:
        h ^= np.uint64(c)
        h *= P
    return h


def unwind(h, suffix):
    h = h.copy()
    for c in reversed(suffix):
        h *= PINV
        h ^= np.uint64(c)
    return h


def trie_of(words):
    trie = {}
    for w in words:
        t = trie
        for c in w:
            t = t.setdefault(c, {})
        t[None] = w
    return trie


def probe(prefixes, words, targets):
    """Each (prefix index, word) whose hash continued from the prefix is in `targets`."""
    hits = []
    stack = [(trie_of(words), prefixes)]
    while stack:
        t, h = stack.pop()
        w = t.get(None)
        if w is not None:
            for i in np.nonzero(has(targets, h))[0]:
                hits.append((int(i), w))
        for c, sub in t.items():
            if c is not None:
                stack.append((sub, (h ^ np.uint64(c)) * P))
    return hits


def exe_strings(exe):
    try:
        return subprocess.run(['strings', '-n', '5', exe], capture_output=True, check=True).stdout
    except (OSError, subprocess.CalledProcessError):
        return open(exe, 'rb').read()


def format_number(spec, n):
    zero, width, prec = re.match(rb'%(0?)(\d*)(?:\.(\d+))?', spec).groups()
    if width and not zero and not prec:
        return None
    text = {b'x': '%x', b'X': '%X'}.get(spec[-1:], '%d') % n
    return text.rjust(max(int(width or 0), int(prec or 0)), '0').encode()


def format_words(out):
    words = set()
    for token in FORMAT.findall(out):
        specs = SPEC.findall(token)
        if not 1 <= len(specs) <= FORMAT_MAX_SPECS or len(token) > 128:
            continue
        pieces = SPEC.split(token)
        names = [pieces[0]]
        for spec, after in zip(specs, pieces[1:]):
            choices = [format_number(spec, n) for n in range(FORMAT_TOP)]
            if None in choices:
                names = []
                break
            names = [n + c + after for n in names for c in choices]
        words.update(n for n in names if WORD.fullmatch(n))
    return words


def exe_words(exe, cache):
    if cache and os.path.exists(cache):
        return set(open(cache, 'rb').read().split())
    out = exe_strings(exe)
    words = set(WORD.findall(out)) | format_words(out)
    if cache:
        with open(cache, 'wb') as f:
            f.write(b'\n'.join(sorted(words)) + b'\n')
    return words


def catalog_words(containers_only=False):
    paths = []
    keys = os.path.join(HERE, 'catalog', 'keys.tsv.gz')
    if os.path.exists(keys):
        with gzip.open(keys, 'rt', encoding='utf-8') as f:
            next(f)
            paths += [line.split('\t', 1)[0] for line in f]
    for name in ('names.txt', 'compositions.txt'):
        p = os.path.join(HERE, name)
        if os.path.exists(p):
            for line in open(p, encoding='utf-8'):
                line = line.strip()
                if line and not line.startswith('#'):
                    paths += expand_braces(line)
    rk = os.path.join(ROOT, 'packages', 'core', 'src', 'activation', 'row_keys.txt.gz')
    if os.path.exists(rk):
        with gzip.open(rk, 'rt', encoding='utf-8') as f:
            paths += [line.strip().lstrip('+') for line in f if line[:1] not in ('#', '@', '\n')]
    out = set()
    for p in paths:
        parts = [ROW_INDEX.sub('', s) for s in p.split('.')]
        for s in parts[:-1] if containers_only else parts:
            if s and IDENT.match(s):
                out.add(s.encode())
    return out


def expand_braces(line):
    m = re.search(r'\{([^{}]*)\}', line)
    if not m:
        return [line]
    out = []
    for choice in m.group(1).split(','):
        out += expand_braces(line[:m.start()] + choice + line[m.end():])
    return out


def label_words(db):
    out = set()
    for r in db.recs:
        if r.type == 4:
            v = r.value.decode('utf-8', 'replace')
            for item in v.split(','):
                item = item.strip()
                if 2 <= len(item) <= 128 and IDENT.match(item):
                    out.add(item.encode())
    return out


class Vocab:
    def __init__(self, db, args):
        self.db = db
        self.args = args
        self.cache = {}

    def source(self, s):
        if s in self.cache:
            return self.cache[s]
        if s in ('exe', 'formats'):
            exe = self.args.exe or (os.path.join(self.args.game, 'F1_25.exe') if self.args.game else None)
            if not exe or not os.path.exists(exe):
                sys.exit('the %s vocabulary needs --exe or --game' % s)
            words = exe_words(exe, self.args.exe_cache) if s == 'exe' else format_words(exe_strings(exe))
        elif s == 'catalog':
            words = catalog_words()
        elif s == 'containers':
            words = catalog_words(containers_only=True)
        elif s == 'labels':
            words = label_words(self.db)
        elif s.startswith('table:'):
            words = set()
            for r in self.db.rows(self.db.resolve(s[6:])):
                v = self.db.text(r + '._metadata_')
                if v and IDENT.match(v):
                    words.add(v.encode())
        elif s.startswith('file:'):
            words = {w.strip().encode() for w in open(s[5:], encoding='utf-8') if w.strip()}
        elif s.startswith('list:'):
            words = {w.strip().encode() for w in s[5:].split(',') if w.strip()}
        else:
            sys.exit('unknown vocabulary %s' % s)
        self.cache[s] = words
        return words

    def level(self, spec):
        words = set()
        for s in split_sources(spec):
            words |= self.source(s)
        return sorted(words)


def split_sources(spec):
    return [s for s in spec.split('+') if s]


DEFAULTS = {
    1: ['exe+catalog+labels'],
    2: ['catalog+labels', 'exe+catalog+labels'],
    3: ['containers', 'containers', 'catalog'],
}


def search(db, row, levels, unwind_max, max_prefixes):
    base = np.array([gamedb.fnv1a64(row + '.')], dtype=np.uint64)
    if len(levels[-1]) <= unwind_max and len(levels) > 1:
        leaves = levels[-1]
        parts = [unwind(db.keys, b'.' + leaf) for leaf in leaves]
        owner = np.concatenate([np.full(len(p), i, dtype=np.int32) for i, p in enumerate(parts)])
        flat = np.concatenate(parts)
        order = np.argsort(flat, kind='stable')
        targets, owner = flat[order], owner[order]
        walk = levels[:-1]
    else:
        targets, owner, leaves, walk = db.keys, None, None, levels
    prefixes, names = base, [b'']
    for words in walk[:-1]:
        if len(prefixes) * len(words) > max_prefixes:
            sys.exit('%d x %d prefixes exceed --max-prefixes %d; narrow a level' % (
                len(prefixes), len(words), max_prefixes))
        nxt, nn = [], []
        for w in words:
            nxt.append(extend(prefixes, w + b'.'))
            nn += [n + w + b'.' for n in names]
        prefixes, names = np.concatenate(nxt), nn
    found = set()
    for i, w in probe(prefixes, walk[-1], targets):
        path = names[i] + w
        if owner is None:
            found.add(path.decode())
            continue
        h = np.array([gamedb.fnv1a64(row + '.' + path.decode())], dtype=np.uint64)
        lo = np.searchsorted(targets, h[0], 'left')
        hi = np.searchsorted(targets, h[0], 'right')
        for j in range(lo, hi):
            found.add((path + b'.' + leaves[owner[j]]).decode())
    keyset = set(db.rec)
    return sorted(p for p in found if gamedb.fnv1a64(row + '.' + p) in keyset)


def braces(paths):
    groups = {}
    for p in paths:
        groups.setdefault(p.count('.'), []).append(p.split('.'))
    out = []
    for depth in sorted(groups):
        rows = groups[depth]
        levels = [sorted({r[i] for r in rows}, key=lambda w: next(j for j, r in enumerate(rows) if r[i] == w))
                  for i in range(depth + 1)]
        total = 1
        for lv in levels:
            total *= len(lv)
        if total == len(rows):
            out.append('.'.join(lv[0] if len(lv) == 1 else '{%s}' % ','.join(lv) for lv in levels))
        else:
            out += ['.'.join(r) for r in rows]
    return out


def catalogued(row, paths):
    shape = '.'.join(ROW_INDEX.sub('_N', s) if ROW_INDEX.search(s) else s for s in row.split('.'))
    keys = os.path.join(HERE, 'catalog', 'keys.tsv.gz')
    known = set()
    if os.path.exists(keys):
        with gzip.open(keys, 'rt', encoding='utf-8') as f:
            prefix = shape + '.'
            for line in f:
                k = line.split('\t', 1)[0]
                if k.startswith(prefix):
                    known.add(k[len(prefix):])
    return {p for p in paths if ROW_INDEX.sub('_N', p) in known or p in known}


def main():
    ap = argparse.ArgumentParser(description='Find unnamed keys under a game-database row by hashing candidate paths')
    ap.add_argument('row', help='a dotted row path; selectors such as table[_metadata_=x] are resolved')
    ap.add_argument('--game', help='F1 25 folder (or a pristine copy) holding F1_25.exe and game_package/')
    ap.add_argument('--db', help='gamedatabases.2025.bin, overriding --game')
    ap.add_argument('--exe', help='F1_25.exe, overriding --game')
    ap.add_argument('--exe-cache', help='read (or write) the exe word list here')
    ap.add_argument('--depth', type=int, default=1, choices=(1, 2, 3), help='levels below the row (default 1)')
    ap.add_argument('--level', action='append', default=[], metavar='N=SOURCES',
                    help='vocabulary of level N, sources joined with +, such as 2=exe or 3=table:car_setup.sliders.slider')
    ap.add_argument('--unwind-max', type=int, default=5000,
                    help='unwind the keys by the last level when it has at most this many words (default 5000)')
    ap.add_argument('--max-prefixes', type=int, default=20_000_000)
    ap.add_argument('--braces', action='store_true', help='print full products as {a,b}.{c,d} lines')
    args = ap.parse_args()
    db_path = args.db or (os.path.join(args.game, DB_REL) if args.game else None)
    if not db_path:
        ap.error('pass --game or --db')
    db = Db(db_path)
    row = db.resolve(args.row)
    if db.text(row + '._metadata_') is None:
        print('%s: no _metadata_ key; searching under it anyway' % row, file=sys.stderr)
    specs = list(DEFAULTS[args.depth])
    for lv in args.level:
        n, _, spec = lv.partition('=')
        if not n.isdigit() or not 1 <= int(n) <= args.depth:
            ap.error('--level %s: N must be 1 to --depth' % lv)
        specs[int(n) - 1] = spec
    vocab = Vocab(db, args)
    levels = [vocab.level(s) for s in specs]
    for i, (s, words) in enumerate(zip(specs, levels), 1):
        print('level %d: %s, %d words' % (i, s, len(words)), file=sys.stderr)
    found = search(db, row, levels, args.unwind_max, args.max_prefixes)
    known = catalogued(row, found)
    print('%s: %d key(s) found, %d of them already in the catalogue' % (row, len(found), len(known)), file=sys.stderr)
    for line in braces(found) if args.braces else found:
        print(line if args.braces or line not in known else line + '\t(catalogued)')


if __name__ == '__main__':
    main()
