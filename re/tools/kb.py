#!/usr/bin/env python3
"""Shared KB access for re/db. The only sanctioned writer of db/*.tsv (see PROTOCOL.md §4, §6).

  kb.py --agent A add <table> col=value ...           add a row
  kb.py --agent A set <table> <key> col=value col+=;token ...
  kb.py get <table> <key|name>
  kb.py find <regex>                                  search names/notes/paths in every table
  kb.py --agent A claim|release <subsystem>
  kb.py --agent A note <subsystem> "text"             append to subsystems/<id>.md ## Inbox
  kb.py --agent A verify <kind> <item> agree|disagree|partial "<evidence>" ["notes"]
  kb.py --agent A test-new "<question>"               allocate re/tests/T###.md
  kb.py --agent A import <rows.tsv> <table>           add rows not yet present (ghidra_export output)
  kb.py export <dir>                                  every table with the binding records merged in, plus structs/
  kb.py function-names                                rewrite re/db/function_names.tsv (crash symbols for the apps)
Tables: functions globals db_keys strings_anchors subsystems verify
functions, globals and verify also hold the rows of the binding records under re/bindings (binding_record.py).
"""
import argparse, datetime, json, os, re, sys, contextlib
try:
    import fcntl
except ImportError:  # Windows
    fcntl = None
    import msvcrt

HERE = os.path.dirname(os.path.abspath(__file__))
RE = os.path.dirname(HERE)
ROOT = os.path.dirname(RE)
DB = os.path.join(RE, 'db')
LOCK = os.path.join(DB, '.kb.lock')
JOURNAL = os.path.join(DB, 'changes.log')

TABLES = {
    'functions': (['addr', 'name', 'sig', 'level', 'subsystem', 'proto', 'evidence', 'notes'], ('addr',)),
    'globals': (['addr', 'name', 'type', 'level', 'subsystem', 'sig', 'evidence', 'notes'], ('addr',)),
    'db_keys': (['key', 'path', 'reader', 'sites', 'level', 'subsystem', 'evidence', 'notes'], ('key', 'path', 'reader')),
    'strings_anchors': (['id', 'addr', 'enc', 'text', 'refs', 'funcs', 'level', 'notes'], ('id',)),
    'subsystems': (['id', 'prefix', 'owner', 'status', 'description'], ('id',)),
    'verify': (['date', 'verifier', 'kind', 'item', 'claimed', 'verdict', 'evidence', 'notes'], None),
    'verify_ranges': (['first', 'last', 'owner', 'granted', 'notes'], ('first',)),
}
LEVELS = ('L0', 'L1', 'L2', 'L3')
TOKEN_KINDS = ('decomp', 'xref', 'str', 'imm', 'pat', 'doc', 'kb', 'trace', 'sig')
ADDR_RE = re.compile(r'^0x1[45][0-9a-f]{7}$')
NAME_RE = re.compile(r'^[A-Za-z][A-Za-z0-9_]{1,63}$')
FUNC_NAME_RE = re.compile(r'^([A-Z][A-Za-z0-9]*)_[A-Za-z0-9]+(_[A-Za-z0-9]+)*$')
GLOBAL_NAME_RE = re.compile(r'^(g|k|vt|site)_([A-Z][A-Za-z0-9]*)(_[A-Za-z0-9]+)*$')
SIG_ID_RE = re.compile(r'^[a-z][a-z0-9_]*\.[a-z][a-z0-9_]*$')
TEST_RE = re.compile(r'^T\d{3}$')


def path(table):
    return os.path.join(DB, f'{table}.tsv')


RECORD_TABLES = ('functions', 'globals', 'verify')


def read_table(table):
    rows, errs = read_tsv(table)
    if table in RECORD_TABLES:
        import binding_record
        more, more_errs = binding_record.Store.get().table_rows(table)
        rows += more
        errs += more_errs
    return rows, errs


def load_signatures():
    with open(os.path.join(DB, 'signatures.json')) as f:
        doc = json.load(f)
    import binding_record
    doc['signatures'] += binding_record.Store.get().signatures()
    return doc


def read_tsv(table):
    cols, _ = TABLES[table]
    p = path(table)
    rows = []
    if not os.path.exists(p):
        return rows, [f'{table}.tsv missing']
    errs = []
    with open(p) as f:
        lines = f.read().split('\n')
    if lines and lines[-1] == '':
        lines.pop()
    if not lines or lines[0].split('\t') != cols:
        errs.append(f'{table}.tsv: header must be {"<TAB>".join(cols)}')
    for n, line in enumerate(lines[1:], 2):
        c = line.split('\t')
        if len(c) != len(cols):
            errs.append(f'{table}.tsv:{n}: {len(c)} columns, expected {len(cols)}')
            continue
        r = dict(zip(cols, c))
        r['_line'] = n
        rows.append(r)
    return rows, errs


FUNCTION_NAMES = os.path.join(DB, 'function_names.tsv')


def function_names_text(rows=None):
    """addr and name of every function, tables and records together, by address: the crash symbols the launcher and
    the CLI embed. Generated; kb.py rewrites it on every functions write."""
    rows = read_table('functions')[0] if rows is None else rows
    return 'addr\tname\n' + ''.join(f'{r["addr"]}\t{r["name"]}\n' for r in sorted(rows, key=lambda r: int(r['addr'], 16)))


def write_function_names():
    text = function_names_text()
    try:
        with open(FUNCTION_NAMES) as f:
            if f.read() == text:
                return False
    except OSError:
        pass
    tmp = f'{FUNCTION_NAMES}.{os.getpid()}.tmp'
    with open(tmp, 'w') as f:
        f.write(text)
    os.replace(tmp, FUNCTION_NAMES)
    return True


def write_table(table, rows):
    """Base rows go to <table>.tsv; rows that came from a binding record go back into that record."""
    records = [r for r in rows if r.get('_record')]
    if records:
        import binding_record
        store = binding_record.Store.get()
        for r in records:
            rec = store.by_relpath(r['_record'])
            before = rec.to_json()
            rec.set_table_row(r)
            if rec.to_json() != before:
                rec.save()
    rows = [r for r in rows if not r.get('_record')]
    cols, key = TABLES[table]
    if key and key[0] in ('addr',):
        rows = sorted(rows, key=lambda r: int(r['addr'], 16))
    elif key:
        rows = sorted(rows, key=lambda r: tuple(r[k] for k in key))
    p = path(table)
    tmp = f'{p}.{os.getpid()}.tmp'
    with open(tmp, 'w') as f:
        f.write('\t'.join(cols) + '\n')
        for r in rows:
            f.write('\t'.join(r[c] for c in cols) + '\n')
    os.replace(tmp, p)
    if table == 'functions':
        write_function_names()


def row_key(table, r):
    _, key = TABLES[table]
    return tuple(r[k] for k in key) if key else None


def tokens(ev):
    out = []
    if ev in ('', '-'):
        return out
    for t in ev.split(';'):
        t = t.strip()
        if not t:
            continue
        k, _, v = t.partition(':')
        out.append((k, v))
    return out


_subsystems = (None, {})


def subsystems():
    """subsystems.tsv by id, parsed again only when the file's bytes differ from the last parse."""
    global _subsystems

    def raw():
        try:
            with open(path('subsystems'), 'rb') as f:
                return f.read()
        except OSError:
            return None
    data = raw()
    if data is None or data != _subsystems[0]:
        rows, _ = read_table('subsystems')
        parsed = {r['id']: r for r in rows}
        if data is not None and raw() == data:
            _subsystems = (data, parsed)
        return {k: dict(v) for k, v in parsed.items()}
    return {k: dict(v) for k, v in _subsystems[1].items()}


def check_level_evidence(level, ev, kind='function'):
    errs = []
    toks = tokens(ev)
    kinds = {k for k, _ in toks}
    for k, v in toks:
        if k not in TOKEN_KINDS:
            errs.append(f'unknown evidence token kind {k!r}')
        elif not v:
            errs.append(f'empty evidence token {k}:')
    if level not in LEVELS:
        errs.append(f'level {level!r} not in {LEVELS}')
        return errs
    if level == 'L1' and not toks:
        errs.append('L1 needs at least one evidence token')
    if level in ('L2', 'L3'):
        need = {'decomp', 'xref'} if kind not in ('db_key', 'code_site') else {'decomp'}
        miss = need - kinds
        if miss:
            errs.append(f'{level} needs ' + ' and '.join(f'{m}:' for m in sorted(miss)))
    if level == 'L3' and 'trace' not in kinds:
        errs.append('L3 needs a trace: token')
    return errs


def validate_row(table, r, subs=None):
    if subs is None and table in ('functions', 'globals', 'db_keys'):
        subs = subsystems()
    errs = []
    cols, _ = TABLES[table]
    for c in cols:
        v = r.get(c, '')
        if v == '':
            errs.append(f'{c} is empty (use -)')
        if '\t' in v or '\n' in v:
            errs.append(f'{c} contains a tab or newline')
    if errs:
        return errs
    if table in ('functions', 'globals'):
        if not ADDR_RE.match(r['addr']):
            errs.append(f'addr {r["addr"]!r} is not 0x14xxxxxxx/0x15xxxxxxx lowercase')
        if not NAME_RE.match(r['name']):
            errs.append(f'name {r["name"]!r} has invalid characters or length')
        if r['name'].startswith(('FUN_', 'DAT_', 'LAB_', 'sub_', 'PTR_', 'UNK_')):
            errs.append(f'name {r["name"]} is a tool default name')
        sub = subs.get(r['subsystem'])
        if not sub:
            errs.append(f'subsystem {r["subsystem"]!r} not in subsystems.tsv')
        if table == 'functions':
            m = FUNC_NAME_RE.match(r['name'])
            if not m:
                errs.append(f'name {r["name"]} does not follow <Prefix>_<Verb>_<Noun>')
            elif sub and m.group(1) != sub['prefix']:
                errs.append(f'name prefix {m.group(1)} != subsystem {r["subsystem"]} prefix {sub["prefix"]}')
            unk = f'{sub["prefix"] if sub else "?"}_Unk_{r["addr"][2:]}'
            if r['level'] == 'L0' and r['name'] != unk:
                errs.append(f'L0 function must be named {unk}')
            if '_Unk_' in r['name'] and r['name'] != unk:
                errs.append(f'unknown-name must be exactly {unk}')
        else:
            m = GLOBAL_NAME_RE.match(r['name'])
            if not m:
                errs.append(f'global name {r["name"]} must be g_/k_/vt_/site_<Prefix>_...')
            elif sub and m.group(2) != sub['prefix']:
                errs.append(f'global prefix {m.group(2)} != subsystem prefix {sub["prefix"]}')
        if r['sig'] != '-' and not SIG_ID_RE.match(r['sig']):
            errs.append(f'sig id {r["sig"]!r} malformed')
        errs += check_level_evidence(r['level'], r['evidence'], 'code_site' if r.get('type') == 'code' else 'function')
    elif table == 'db_keys':
        if r['key'] != '-' and not re.match(r'^0x[0-9a-f]{16}$', r['key']):
            errs.append(f'key {r["key"]!r} must be 0x + 16 hex')
        if r['key'] != '-' and r['path'] != '?' and '{' not in r['path'] and f'0x{fnv1a64(r["path"]):016x}' != r['key']:
            errs.append(f'fnv1a64({r["path"]}) != {r["key"]}')
        if not ADDR_RE.match(r['reader']):
            errs.append(f'reader {r["reader"]!r} is not an address')
        if r['subsystem'] not in subs:
            errs.append(f'subsystem {r["subsystem"]!r} not in subsystems.tsv')
        errs += check_level_evidence(r['level'], r['evidence'], 'db_key')
    elif table == 'strings_anchors':
        if not re.match(r'^[shw]_[A-Za-z0-9_]+$', r['id']):
            errs.append(f'anchor id {r["id"]!r} must be s_/h_/w_<name>')
        if r['addr'] != '-' and not ADDR_RE.match(r['addr']):
            errs.append(f'addr {r["addr"]!r} malformed')
        if r['enc'].startswith('djb2:'):
            if f'djb2:0x{djb2(r["text"]):08x}' != r['enc']:
                errs.append(f'djb2({r["text"]}) != {r["enc"]}')
        elif r['enc'] not in ('ascii', 'utf16'):
            errs.append(f'enc {r["enc"]!r} must be ascii, utf16 or djb2:0x........')
        if r['level'] not in LEVELS:
            errs.append(f'level {r["level"]!r}')
        if r['level'] in ('L2', 'L3') and r['refs'] == '-':
            errs.append('L2 anchor needs refs')
    elif table == 'subsystems':
        if not re.match(r'^[a-z][a-z0-9_]*$', r['id']):
            errs.append('subsystem id must be snake_case')
        if not re.match(r'^[A-Z][A-Za-z0-9]*$', r['prefix']):
            errs.append('prefix must be PascalCase')
    elif table == 'verify_ranges':
        if not (re.match(r'^[1-9][0-9]*$', r['first']) and re.match(r'^[1-9][0-9]*$', r['last'])):
            errs.append('first and last must be decimal verify ids')
        elif int(r['first']) < RANGED_VERIFY_IDS_FROM or int(r['last']) < int(r['first']):
            errs.append(f'a range runs from first to last, both at or above {RANGED_VERIFY_IDS_FROM}')
        if not OWNER_RE.match(r['owner']):
            errs.append(f'owner {r["owner"]!r} must be a verifier name or the start of one')
        if not re.match(r'^\d{4}-\d{2}-\d{2}$', r['granted']):
            errs.append(f'granted {r["granted"]!r} must be a date')
    elif table == 'verify':
        if r['verdict'] not in ('agree', 'disagree', 'partial'):
            errs.append(f'verdict {r["verdict"]!r}')
        if r['kind'] not in ('func', 'global', 'struct', 'field', 'sig', 'db_key', 'anchor'):
            errs.append(f'kind {r["kind"]!r}')
        errs += [e for e in check_level_evidence('L1', r['evidence'])]
    return errs


def fnv1a64(s):
    h = 0xcbf29ce484222325
    for c in s.encode():
        h = ((h ^ c) * 0x100000001b3) & 0xffffffffffffffff
    return h


def djb2(s):
    h = 5381
    for c in s.encode():
        h = (h * 33 + c) & 0xffffffff
    return h


@contextlib.contextmanager
def locked():
    os.makedirs(DB, exist_ok=True)
    with open(LOCK, 'a') as lf:
        if fcntl is None:
            # Windows: lock the first byte, blocking until it is free.
            lf.seek(0)
            msvcrt.locking(lf.fileno(), msvcrt.LK_LOCK, 1)
            try:
                yield
            finally:
                lf.seek(0)
                msvcrt.locking(lf.fileno(), msvcrt.LK_UNLCK, 1)
            return
        fcntl.flock(lf, fcntl.LOCK_EX)
        try:
            yield
        finally:
            fcntl.flock(lf, fcntl.LOCK_UN)


def journal(agent, op, table, key, before, after):
    with open(JOURNAL, 'a') as f:
        f.write(json.dumps({'ts': datetime.datetime.now().isoformat(timespec='seconds'), 'agent': agent, 'op': op,
                            'table': table, 'key': key, 'before': before, 'after': after}) + '\n')


def raiser(table, key):
    who = None
    if not os.path.exists(JOURNAL):
        return None
    for line in open(JOURNAL):
        try:
            j = json.loads(line)
        except Exception:
            continue
        if j.get('table') == table and j.get('key') == key:
            b, a = j.get('before') or {}, j.get('after') or {}
            if a.get('level') and a.get('level') != b.get('level'):
                who = j['agent']
    return who


def owner_gate(agent, sub, new_level, force):
    s = subsystems().get(sub)
    if s and s['owner'] not in ('-', agent) and new_level in ('L2', 'L3') and not force:
        raise SystemExit(f'refused: subsystem {sub} is owned by {s["owner"]}; only the owner raises rows to {new_level} (use kb.py note {sub} "...")')


def parse_assign(items, cols):
    sets, appends = {}, {}
    for it in items:
        m = re.match(r'^([a-z_]+)(\+?)=(.*)$', it, re.S)
        if not m or m.group(1) not in cols:
            raise SystemExit(f'bad assignment {it!r}; columns: {cols}')
        (appends if m.group(2) else sets)[m.group(1)] = m.group(3)
    return sets, appends


def find_row(table, rows, key):
    _, kcols = TABLES[table]
    for r in rows:
        if kcols and len(kcols) == 1 and r[kcols[0]] == key:
            return r
        if r.get('name') == key or r.get('id') == key:
            return r
        if kcols and len(kcols) > 1 and '|'.join(r[k] for k in kcols) == key:
            return r
    return None


def uniqueness(table, rows, r):
    errs = []
    k = row_key(table, r)
    if k and any(row_key(table, x) == k for x in rows if x is not r):
        errs.append(f'duplicate key {k}')
    if table in ('functions', 'globals'):
        names = set()
        for t in ('functions', 'globals'):
            other = rows if t == table else read_table(t)[0]
            names |= {x['name'] for x in other if x is not r}
        if r['name'] in names:
            errs.append(f'name {r["name"]} already used')
    return errs


def cmd_add(a):
    cols, _ = TABLES[a.table]
    sets, appends = parse_assign(a.assign, cols)
    r = {c: sets.get(c, '-') for c in cols}
    if a.table in ('functions', 'globals') and r['addr'] != '-':
        r['addr'] = '0x' + r['addr'].lower().removeprefix('0x')
    with locked():
        rows, errs = read_table(a.table)
        if errs:
            raise SystemExit('\n'.join(errs))
        if 'subsystem' in r:
            owner_gate(a.agent, r['subsystem'], r.get('level'), a.force)
        e = validate_row(a.table, r) + uniqueness(a.table, rows, r)
        if e:
            raise SystemExit('refused:\n  ' + '\n  '.join(e))
        rows.append(r)
        write_table(a.table, rows)
        journal(a.agent, 'add', a.table, '|'.join(row_key(a.table, r) or ()), None, {c: r[c] for c in cols})
    print(f'added {a.table} {row_key(a.table, r)}')


def cmd_set(a):
    cols, _ = TABLES[a.table]
    sets, appends = parse_assign(a.assign, cols)
    with locked():
        rows, errs = read_table(a.table)
        r = find_row(a.table, rows, a.key)
        if not r:
            raise SystemExit(f'no {a.table} row {a.key}')
        before = {c: r[c] for c in cols}
        for c, v in sets.items():
            r[c] = v
        for c, v in appends.items():
            r[c] = v if r[c] in ('', '-') else r[c] + (';' if c == 'evidence' and not v.startswith(';') else ' ') + v.lstrip(';')
        if 'subsystem' in r:
            owner_gate(a.agent, r['subsystem'], r.get('level'), a.force)
        e = validate_row(a.table, r) + uniqueness(a.table, rows, r)
        if e:
            raise SystemExit('refused:\n  ' + '\n  '.join(e))
        write_table(a.table, rows)
        journal(a.agent, 'set', a.table, '|'.join(row_key(a.table, r) or ()), before, {c: r[c] for c in cols})
    print(f'updated {a.table} {row_key(a.table, r)}')


def cmd_get(a):
    rows, _ = read_table(a.table)
    r = find_row(a.table, rows, a.key)
    if not r:
        raise SystemExit(f'no {a.table} row {a.key}')
    for c in TABLES[a.table][0]:
        print(f'{c:10s} {r[c]}')


def cmd_find(a):
    rx = re.compile(a.regex, re.I)
    for t in TABLES:
        rows, _ = read_table(t)
        for r in rows:
            line = '\t'.join(r[c] for c in TABLES[t][0])
            if rx.search(line):
                print(f'{t}:{r["_line"]}: {line[:220]}')


def cmd_claim(a, release=False):
    with locked():
        rows, _ = read_table('subsystems')
        r = find_row('subsystems', rows, a.subsystem)
        if not r:
            raise SystemExit(f'no subsystem {a.subsystem}')
        before = dict(r)
        if release:
            if r['owner'] != a.agent and not a.force:
                raise SystemExit(f'{a.subsystem} is owned by {r["owner"]}, not {a.agent}')
            r['owner'] = '-'
        else:
            if r['owner'] not in ('-', a.agent) and not a.force:
                raise SystemExit(f'{a.subsystem} is already owned by {r["owner"]}')
            r['owner'] = a.agent
        write_table('subsystems', rows)
        journal(a.agent, 'release' if release else 'claim', 'subsystems', a.subsystem, {'owner': before['owner']}, {'owner': r['owner']})
    print(f'{a.subsystem}: owner {r["owner"]}')


def cmd_note(a):
    p = os.path.join(DB, 'subsystems', f'{a.subsystem}.md')
    if not os.path.exists(p):
        raise SystemExit(f'no {p}')
    with locked():
        txt = open(p).read()
        if '\n## Inbox' not in txt:
            txt = txt.rstrip('\n') + '\n\n## Inbox\n'
        stamp = datetime.datetime.now().strftime('%Y-%m-%d %H:%M')
        txt = txt.rstrip('\n') + f'\n- {stamp} {a.agent}: {a.text}\n'
        tmp = p + f'.{os.getpid()}.tmp'
        open(tmp, 'w').write(txt)
        os.replace(tmp, p)
        journal(a.agent, 'note', 'subsystems', a.subsystem, None, {'text': a.text})
    print(f'noted in {p}')


SHARED_VERIFY_IDS_UP_TO = 10540


def colliding_verify_ids(rows):
    """{n: rows} for each verify id above SHARED_VERIFY_IDS_UP_TO that more than one row carries. The ids up to that
    bound were handed out twice on parallel branches before kb_lint checked them, and stay as they are."""
    out = {}
    for r in rows:
        m = re.match(r'V(\d+) ', r.get('notes') or '')
        if m and int(m.group(1)) > SHARED_VERIFY_IDS_UP_TO:
            out.setdefault(int(m.group(1)), []).append(r)
    return {n: rs for n, rs in out.items() if len(rs) > 1}


RANGED_VERIFY_IDS_FROM = 100000
RANGE_SIZE = 10000
OWNER_RE = re.compile(r'^[A-Za-z0-9][A-Za-z0-9_.-]{0,63}$')


def verify_id(r):
    m = re.match(r'V(\d+) ', r.get('notes') or '')
    return int(m.group(1)) if m else None


def range_owner(verifier, ranges):
    """The owner whose ranges a verifier's ids come from: the longest owner that is the verifier's name or the start
    of it up to a '-' (re-bindings owns re-bindings and re-bindings-blind)."""
    owners = {r['owner'] for r in ranges if verifier == r['owner'] or verifier.startswith(r['owner'] + '-')}
    return max(owners, key=len) if owners else None


def next_verify_id(verifier, rows, ranges):
    """The next free id in the ranges of the verifier's owner. Ranges are granted on the trunk and never overlap, so
    rows written on different branches by different owners cannot get the same id. `rows` are every verify row of
    the checkout, verify.tsv's and the records'; the id handed out lies above every id of theirs in its range, whoever
    wrote it, so it is never one of theirs."""
    owner = range_owner(verifier, ranges)
    if not owner:
        raise SystemExit(f'refused: {verifier} has no verify id range in re/db/verify_ranges.tsv; a lead grants one '
                         f'on the trunk with kb.py --agent LEAD grant-range OWNER')
    taken = {n for x in rows if (n := verify_id(x)) is not None}
    for g in sorted((x for x in ranges if x['owner'] == owner), key=lambda x: int(x['first'])):
        first, last = int(g['first']), int(g['last'])
        used = [n for n in taken if first <= n <= last]
        n = max(used) + 1 if used else first
        if n <= last:
            return n
    raise SystemExit(f'refused: the verify id ranges of {owner} are used up; a lead grants another on the trunk')


def verify_range_errors(ranges, rows):
    """Overlapping ranges, and ids at or above RANGED_VERIFY_IDS_FROM outside every range of their verifier's
    owner."""
    errs = []
    spans = sorted((int(r['first']), int(r['last']), r['owner']) for r in ranges
                   if not validate_row('verify_ranges', r))
    for (f1, l1, o1), (f2, l2, o2) in zip(spans, spans[1:]):
        if f2 <= l1:
            errs.append(f'verify_ranges: {o1} {f1}..{l1} and {o2} {f2}..{l2} overlap; grant ranges on the trunk only')
    for r in rows:
        n = verify_id(r)
        if n is None or n < RANGED_VERIFY_IDS_FROM:
            continue
        owner = range_owner(r['verifier'], ranges)
        if not any(x['owner'] == owner and int(x['first']) <= n <= int(x['last']) for x in ranges):
            errs.append(f'verify: V{n} by {r["verifier"]} ({r["kind"]} {r["item"]}) lies outside every range of '
                        f'{owner or "its owner, which has none"} in verify_ranges.tsv')
    return errs


def cmd_grant_range(a):
    if not OWNER_RE.match(a.owner):
        raise SystemExit(f'refused: owner {a.owner!r} must be a verifier name or the start of one')
    with locked():
        rows, errs = read_table('verify_ranges')
        if errs:
            raise SystemExit('\n'.join(errs))
        top = max([RANGED_VERIFY_IDS_FROM - 1, *(int(r['last']) for r in rows)])
        first = (top // a.size + 1) * a.size
        r = {'first': str(first), 'last': str(first + a.size - 1), 'owner': a.owner,
             'granted': datetime.date.today().isoformat(), 'notes': a.notes or '-'}
        e = validate_row('verify_ranges', r) + uniqueness('verify_ranges', rows, r)
        if e:
            raise SystemExit('refused:\n  ' + '\n  '.join(e))
        rows.append(r)
        write_table('verify_ranges', rows)
        journal(a.agent, 'add', 'verify_ranges', r['first'], None, r)
    print(f'granted V{r["first"]}..V{r["last"]} to {a.owner}; commit it on the trunk before the lane writes verify rows')


def struct_level(kind, item):
    """The level a field or struct row claims: the field's level or the struct's size level in re/db/structs, which
    the export gate compares with the level at export time; '-' when the struct file has no such item."""
    from kb_lint import parse_struct
    sname, _, fname = item.partition('.')
    path = os.path.join(DB, 'structs', sname + '.h')
    if not os.path.isfile(path) or (kind == 'field') != bool(fname):
        return '-'
    s = parse_struct(path)
    if kind == 'struct':
        return s['size_level'] or '-'
    return next((f['level'] for f in s['fields'] if f['name'] == fname), '-')


def cmd_verify(a):
    table = {'func': 'functions', 'global': 'globals', 'db_key': 'db_keys', 'anchor': 'strings_anchors'}.get(a.kind)
    claimed = '-'
    if table:
        rows, _ = read_table(table)
        r = find_row(table, rows, a.item)
        if not r:
            raise SystemExit(f'no {table} row {a.item}')
        claimed = r.get('level', '-')
        who = raiser(table, '|'.join(row_key(table, r) or ()))
        if who == a.agent:
            raise SystemExit(f'refused: {a.agent} raised this item to {claimed}; verification must come from another agent')
    elif a.kind in ('field', 'struct'):
        claimed = struct_level(a.kind, a.item)
    r = {'date': datetime.date.today().isoformat(), 'verifier': a.agent, 'kind': a.kind, 'item': a.item, 'claimed': claimed,
         'verdict': a.verdict, 'evidence': a.evidence, 'notes': a.notes or '-'}
    e = validate_row('verify', r)
    if e:
        raise SystemExit('refused:\n  ' + '\n  '.join(e))
    with locked():
        rows, _ = read_table('verify')
        n = next_verify_id(a.agent, rows, read_table('verify_ranges')[0])
        r['notes'] = f'V{n:04d} ' + r['notes']
        import binding_record
        rec = binding_record.Store.get().by_item(a.item)
        if rec:
            rec.verify.append(r)
            rec.save()
            journal(a.agent, 'verify', a.kind, a.item, None, r)
            print(f'V{n:04d} recorded in re/bindings/{rec.relpath} ({a.verdict})')
            return
        rows = [x for x in rows if not x.get('_record')]
        rows.append(r)
        cols = TABLES['verify'][0]
        p = path('verify')
        tmp = f'{p}.{os.getpid()}.tmp'
        with open(tmp, 'w') as f:
            f.write('\t'.join(cols) + '\n')
            for x in rows:
                f.write('\t'.join(x[c] for c in cols) + '\n')
        os.replace(tmp, p)
        journal(a.agent, 'verify', a.kind, a.item, None, {c: r[c] for c in cols})
    print(f'V{n:04d} recorded ({a.verdict}); on disagree, demote the item and add DISPUTE(V{n:04d}) to its notes')


def cmd_test_new(a):
    tdir = os.path.join(RE, 'tests')
    with locked():
        nums = [int(f[1:4]) for f in os.listdir(tdir) if re.match(r'^T\d{3}\.md$', f)]
        n = max(nums, default=-1) + 1
        tid = f'T{n:03d}'
        tpl = open(os.path.join(tdir, 'TEMPLATE.md')).read()
        txt = tpl.replace('T###', tid).replace('<question>', a.question).replace('<agent>', a.agent).replace('<date>', datetime.date.today().isoformat())
        p = os.path.join(tdir, f'{tid}.md')
        open(p, 'x').write(txt)
        journal(a.agent, 'test-new', 'tests', tid, None, {'question': a.question})
    print(p)


def cmd_import(a):
    cols, _ = TABLES[a.table]
    added = skipped = 0
    with locked():
        rows, _ = read_table(a.table)
        for line in open(a.file):
            c = line.rstrip('\n').split('\t')
            if c == cols or len(c) != len(cols):
                continue
            r = dict(zip(cols, c))
            if validate_row(a.table, r) or uniqueness(a.table, rows, r):
                skipped += 1
                continue
            rows.append(r)
            added += 1
            journal(a.agent, 'import', a.table, '|'.join(row_key(a.table, r) or ()), None, {k: r[k] for k in cols})
        write_table(a.table, rows)
    print(f'imported {added}, skipped {skipped} (invalid or already present; fix and add them by hand)')


EXPORT_KEEP = 3


def export_tree(out):
    """Every table as kb.py reads it (re/db plus re/bindings), signatures.json and the structs, written into `out`."""
    import shutil
    os.makedirs(os.path.join(out, 'structs'))
    for t, (cols, key) in TABLES.items():
        rows, _ = read_table(t)
        if key and key[0] == 'addr':
            rows = sorted(rows, key=lambda r: int(r['addr'], 16))
        elif key:
            rows = sorted(rows, key=lambda r: tuple(r[k] for k in key))
        with open(os.path.join(out, f'{t}.tsv'), 'w') as f:
            f.write('\t'.join(cols) + '\n' + ''.join('\t'.join(r[c] for c in cols) + '\n' for r in rows))
    with open(os.path.join(out, 'signatures.json'), 'w') as f:
        f.write(json.dumps(load_signatures(), indent=1) + '\n')
    src = os.path.join(DB, 'structs')
    structs = sorted(f for f in os.listdir(src) if f.endswith('.h'))
    for f in structs:
        shutil.copyfile(os.path.join(src, f), os.path.join(out, 'structs', f))
    return len(structs)


def cmd_export(a):
    """One folder for tools outside Python such as the Ghidra scripts. Each export is written whole into a new
    sibling folder and `dir` becomes a symlink to it in one rename, so a reader that resolves `dir` once sees one
    export, never files from two. The last EXPORT_KEEP exports stay for readers still holding an older one."""
    import shutil, time
    link = os.path.abspath(a.dir.rstrip('/' + os.sep))
    parent, base = os.path.dirname(link), os.path.basename(link)
    os.makedirs(parent, exist_ok=True)
    version = f'{base}.v{time.time_ns()}.{os.getpid()}'
    out = os.path.join(parent, version)
    try:
        n = export_tree(out)
    except BaseException:
        shutil.rmtree(out, ignore_errors=True)
        raise
    if os.path.isdir(link) and not os.path.islink(link):
        os.rename(link, os.path.join(parent, f'{base}.v0.{os.getpid()}'))
    tmp = os.path.join(parent, f'.{base}.{os.getpid()}.link')
    os.symlink(version, tmp)
    os.replace(tmp, link)
    mine = re.compile(r'^' + re.escape(base) + r'\.v(\d+)\.(\d+)$')
    old = sorted((int(m.group(1)), f) for f in os.listdir(parent) if (m := mine.match(f)) and f != version)
    for _, f in old[:max(0, len(old) - (EXPORT_KEEP - 1))]:
        shutil.rmtree(os.path.join(parent, f), ignore_errors=True)
    print(f'exported {len(TABLES)} tables, signatures.json and {n} structs to {a.dir} ({version})')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--agent', default=os.environ.get('RE_AGENT'))
    ap.add_argument('--force', action='store_true', help='lead only: bypass ownership checks')
    sp = ap.add_subparsers(dest='cmd', required=True)
    p = sp.add_parser('add'); p.add_argument('table', choices=[t for t in TABLES if t != 'verify']); p.add_argument('assign', nargs='+')
    p = sp.add_parser('set'); p.add_argument('table', choices=[t for t in TABLES if t != 'verify']); p.add_argument('key'); p.add_argument('assign', nargs='+')
    p = sp.add_parser('get'); p.add_argument('table', choices=list(TABLES)); p.add_argument('key')
    p = sp.add_parser('find'); p.add_argument('regex')
    p = sp.add_parser('claim'); p.add_argument('subsystem')
    p = sp.add_parser('release'); p.add_argument('subsystem')
    p = sp.add_parser('note'); p.add_argument('subsystem'); p.add_argument('text')
    p = sp.add_parser('verify'); p.add_argument('kind'); p.add_argument('item'); p.add_argument('verdict'); p.add_argument('evidence'); p.add_argument('notes', nargs='?')
    p = sp.add_parser('test-new'); p.add_argument('question')
    p = sp.add_parser('import'); p.add_argument('file'); p.add_argument('table', choices=['functions', 'globals'])
    p = sp.add_parser('export'); p.add_argument('dir')
    sp.add_parser('function-names')
    p = sp.add_parser('grant-range'); p.add_argument('owner'); p.add_argument('--size', type=int, default=RANGE_SIZE); p.add_argument('--notes')
    a = ap.parse_args()
    if a.cmd not in ('get', 'find', 'export', 'function-names') and not a.agent:
        raise SystemExit('--agent NAME (or RE_AGENT) is required for writes')
    {'add': cmd_add, 'set': cmd_set, 'get': cmd_get, 'find': cmd_find, 'claim': cmd_claim,
     'release': lambda x: cmd_claim(x, True), 'note': cmd_note, 'verify': cmd_verify, 'test-new': cmd_test_new,
     'import': cmd_import, 'export': cmd_export, 'grant-range': cmd_grant_range,
     'function-names': lambda a: print('function_names.tsv ' + ('rewritten' if write_function_names() else 'unchanged'))}[a.cmd](a)


if __name__ == '__main__':
    main()
