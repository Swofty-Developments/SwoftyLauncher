#!/usr/bin/env python3
"""Contribute a binding: one file under re/bindings/, checked by CI, proven on your own F1_25.exe (PROTOCOL.md §11).

  binding.py new NAME VA --kind func|site|data --author LOGIN --exe F1_25.exe --evidence "..." [options]
                                  make the signature, write re/bindings/<subsystem>/NAME.json and prove it
  binding.py verify NAME... --exe F1_25.exe --by LOGIN
                                  resolve each record on your exe with the core's rules and record the proof
  binding.py agree NAME --exe F1_25.exe --by LOGIN --evidence "decomp:...;xref:..." [--notes TEXT]
                                  second contributor: your own proof plus an agree row (makes it experimental)
  binding.py check [FILE...]      the static checks CI runs (every record when no file is given)
  binding.py status               every record, its level, proofs, verifiers and what it still needs
  binding.py unfold [NAME...] [--table functions|globals] [--skip-unfit] --author LOGIN
                                  maintainer: move bindings from the shared tables into records; rows that would
                                  fail the record rules stay (named ones are an error unless --skip-unfit)
  binding.py ci --files FILES.json --head DIR --author LOGIN --out RESULT.json
                                  the pull request check (.github/workflows/bindings.yml)
"""
import argparse, datetime, hashlib, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import binding_check as bc
import binding_record as br
import kb

IMAGE_BASE = 0x140000000


def today():
    return datetime.date.today().isoformat()


def find(name):
    rec = br.Store.get().by_item(name)
    if not rec:
        raise SystemExit(f'binding.py: no record for {name} under re/bindings')
    return rec


def exe_identity(path):
    import bindverify
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        head = f.read(0x1000)
        h.update(head)
        for chunk in iter(lambda: f.read(1 << 24), b''):
            h.update(chunk)
    return {'sha256': h.hexdigest(), 'size': os.path.getsize(path),
            'pe_timestamp': bindverify.parse_headers(head)['stamp']}


def one_binding_set(rec, exe):
    """The set core would load for this record alone on the build of `exe`."""
    import bindgen
    kb_build = kb.load_signatures()['build']
    kb_name, overlays = bindgen.load_overlays(kb_build)
    name = next((n for n, o in overlays.items() if o['build']['sha256'] == exe['sha256']), None)
    if name is None:
        raise SystemExit(f'binding.py: {exe["sha256"][:16]} is not a build in re/db/builds; prove on the Steam or EA '
                         f'build the launcher supports')
    s, row = rec.signature, rec.table_row()
    base = int(kb_build['image_base'], 16)
    b = {'id': rec.name, 'kind': bc.binding_kind(rec.table, row, s), 'status': 'experimental', 'level': row['level'],
         'evidence': '-', 'subsystem': row['subsystem'], 'build': kb_build['sha256'], 'sig': s['id'],
         'section': s['section'], 'pattern': s['pattern'], 'anchor': br.number(s['anchor']),
         'resolve': {k: (v if k == 'kind' else br.number(v)) for k, v in s['resolve'].items()},
         'expect': s.get('expect'), 'rva': hex(int(s['target'], 16) - base), 'match': hex(int(s['match'], 16) - base)}
    return bindgen.build_set(overlays[name], [b], name == kb_name), name == kb_name


def sigcheck_result(rec, exe_path, is_kb):
    if not is_kb:
        return 'skipped (not the KB build)'
    try:
        import sigcheck
        img = sigcheck.Image(exe_path)
    except ImportError as e:
        return f'skipped ({e.name} is not installed; re/tools/py has it)'
    names = sigcheck.load_kb_names()
    names[rec.name] = (int(rec.signature['target'], 16), rec.signature['id'])
    errs, _ = sigcheck.check(img, rec.signature, kb.load_signatures()['build']['sha256'], names, True)
    return 'passed' if not errs else 'failed: ' + '; '.join(errs)[:400]


def prove(rec, exe_path, by, exe=None):
    import bindverify
    exe = exe or exe_identity(exe_path)
    doc, is_kb = one_binding_set(rec, exe)
    img = bindverify.Image(exe_path)
    r = bindverify.Resolver(img, doc, None if img.flat else exe['size'], kb_first=False).run()[0]
    va = lambda v: hex(v + IMAGE_BASE) if isinstance(v, int) else '-'
    proof = {'by': by, 'date': today(), 'tool': br.PROOF_TOOL, 'exe': exe,
             'result': {'result': r['result'], 'method': r['method'], 'matches': r.get('matches') or 0,
                        'match': va(r.get('match')), 'target': va(r.get('target'))},
             'sigcheck': sigcheck_result(rec, exe_path, is_kb),
             'digest': br.resolution_digest(rec.signature, exe['sha256'])}
    ok = r['result'] == 'resolved' and not proof['sigcheck'].startswith('failed')
    if ok and is_kb and (proof['result']['match'], proof['result']['target']) != (rec.signature['match'],
                                                                                    rec.signature['target']):
        ok = False
    return proof, ok, r.get('reason', '')


def cmd_new(a):
    import sigcheck, sitereg
    va = int(a.va, 16)
    sub = a.subsystem or sitereg_subsystem(a.name)
    img = sigcheck.Image(a.exe)
    if sigcheck.sha256_file(a.exe) != kb.load_signatures()['build']['sha256']:
        raise SystemExit('binding.py new: --exe must be the KB build (PROTOCOL.md header); other builds get overlay hints')
    kind = {'func': 'func_start', 'site': 'mid', 'data': 'data'}[a.kind]
    if a.kind == 'data':
        if not a.via or not a.type:
            raise SystemExit('binding.py new: data needs --type and --via')
        sig = sitereg.data_candidate(img, va, int(a.via, 16))
    elif a.match_from:
        sig = sitereg.anchored_candidate(img, va, int(a.match_from, 16), kind)
    else:
        sig = sigcheck.make(img, va, kind, int(a.func_start, 16) if a.func_start else None)
    if not sig:
        raise SystemExit(f'binding.py new: no unique pattern near 0x{va:x}; try --match-from')
    if a.kind == 'site':
        sig['expect'] = sitereg.expect_bytes(img, va, a.expect_len)
    else:
        sig.pop('expect', None)
    sig['id'] = a.id or f'{sub}.{sitereg.snake(a.name)}'
    if a.func_start:
        sig['func_start'] = hex(int(a.func_start, 16))
    if a.data1_reason:
        sig['data1_reason'] = a.data1_reason
    sig['names'] = [a.name]
    sig['verified'] = [{'build': kb.load_signatures()['build']['sha256'][:16], 'date': today(), 'by': a.author,
                        'how': 'static'}]
    sig['used_by'] = a.used_by
    sig = {k: v for k, v in sig.items() if v is not None}
    table = 'functions' if a.kind == 'func' else 'globals'
    row = {'addr': f'0x{va:x}', 'level': a.level, 'subsystem': sub, 'evidence': a.evidence, 'notes': a.notes,
           'proto': a.proto, 'type': 'code' if a.kind == 'site' else (a.type or '-')}
    rec = br.Record.create(a.name, table, a.author, row, sig)
    errs = bc.check_record(rec, bc.Knowledge())
    if errs:
        raise SystemExit('binding.py new: the record would fail CI:\n  ' + '\n  '.join(errs))
    proof, ok, why = prove(rec, a.exe, a.author)
    if not ok:
        raise SystemExit(f'binding.py new: {a.name} does not resolve on {a.exe}: {why or proof["sigcheck"]}')
    rec.proofs.append(proof)
    rec.save()
    print(f'wrote re/bindings/{rec.relpath} ({sig["id"]}, match {sig["match"]}, proof on {proof["exe"]["sha256"][:16]})')
    print('next: python3 re/tools/binding.py check, then open a pull request with only that file')


def sitereg_subsystem(name):
    import re
    m = re.match(r'^(?:g|k|vt|site)_([A-Z][A-Za-z0-9]*)_|^([A-Z][A-Za-z0-9]*)_', name)
    prefix = m and (m.group(1) or m.group(2))
    for sid, s in kb.subsystems().items():
        if s['prefix'] == prefix:
            return sid
    raise SystemExit(f'binding.py: no subsystem has the prefix of {name}; pass --subsystem')


def cmd_verify(a):
    exe = exe_identity(a.exe)
    bad = 0
    for name in a.names:
        rec = find(name)
        proof, ok, why = prove(rec, a.exe, a.by, exe)
        print(f'{rec.name:48s} {"ok" if ok else "FAILED"}  {proof["result"]["method"]} matches={proof["result"]["matches"]} '
              f'target={proof["result"]["target"]} sigcheck {proof["sigcheck"]} {why}')
        if not ok:
            bad += 1
            continue
        rec.doc['proofs'] = [p for p in rec.proofs if not (p['by'] == a.by and p['exe']['sha256'] == exe['sha256'])]
        rec.proofs.append(proof)
        rec.save()
    return 1 if bad else 0


def cmd_agree(a):
    rec = find(a.name)
    if a.by == rec.author:
        raise SystemExit('binding.py agree: the author cannot verify their own binding (PROTOCOL.md §5)')
    proof, ok, why = prove(rec, a.exe, a.by)
    if not ok:
        raise SystemExit(f'binding.py agree: {rec.name} does not resolve on your exe: {why or proof["sigcheck"]}')
    row = rec.table_row()
    v = {'date': today(), 'verifier': a.by, 'kind': 'func' if rec.table == 'functions' else 'global',
         'item': rec.name, 'claimed': row['level'], 'verdict': 'agree', 'evidence': a.evidence,
         'notes': f'gh:{a.by} {a.notes}'.strip()}
    errs = kb.validate_row('verify', v)
    if errs:
        raise SystemExit('binding.py agree: ' + '; '.join(errs))
    rec.verify.append(v)
    rec.proofs.append(proof)
    rec.save()
    print(f'{rec.name}: agree row and proof by {a.by} added; with level L2 it exports as experimental')


def check_paths(paths):
    store = br.Store.get()
    knowledge = bc.Knowledge()
    errs = list(store.errors)
    for rec in store.records:
        if paths and not any(os.path.abspath(p) == os.path.join(br.RECORDS, rec.relpath) for p in paths):
            continue
        errs += [f'bindings/{rec.relpath}: {e}' for e in bc.check_record(rec, knowledge.without(rec),
                                                                          're/bindings/' + rec.relpath)]
    return errs


def cmd_check(a):
    errs = check_paths(a.files)
    for e in errs:
        print('ERROR', e)
    n = len(br.Store.get().records)
    print(f'binding.py check: {n} record(s), {len(errs)} error(s)')
    return 1 if errs else 0


def cmd_status(a):
    import bindgen
    gate = bindgen.Gate()
    rows = []
    for rec in br.Store.get().records:
        row = rec.table_row()
        vkind = 'func' if rec.table == 'functions' else 'global'
        status, why = gate(row['level'], row['evidence'], [(vkind, rec.name), (vkind, row['addr'])])
        need = {None: 'an agree row from a second contributor (binding.py agree)' if row['level'] in ('L2', 'L3')
                else 'L2 evidence (decomp: and xref:) from the subsystem owner'}.get(status, '-')
        rows.append((rec.name, row['level'], status or 'not exported', len(rec.proofs),
                     ','.join(v['verifier'] for v in rec.verify if v['verdict'] == 'agree') or '-', need))
    w = max([len(r[0]) for r in rows] + [4])
    print(f'{"name":{w}s}  level  status        proofs  verified by       needs')
    for r in rows:
        print(f'{r[0]:{w}s}  {r[1]:5s}  {r[2]:12s}  {r[3]:6d}  {r[4]:16s}  {r[5]}')
    print(f'{len(rows)} record(s)')


def remove_lines(path, keep):
    with open(path) as f:
        lines = f.read().split('\n')
    out = [lines[0]] + [ln for ln in lines[1:] if ln == '' or keep(ln.split('\t'))]
    with open(path, 'w') as f:
        f.write('\n'.join(out))


def cmd_unfold(a, db=None, root=None):
    """Move base rows into records with the exact same content, so bindgen's output does not change."""
    db, root = db or kb.DB, root or br.RECORDS
    cols = {t: kb.TABLES[t][0] for t in ('functions', 'globals', 'verify')}
    tsv = {t: kb.read_tsv(t)[0] if db == kb.DB else read_tsv_at(db, t) for t in cols}
    with open(os.path.join(db, 'signatures.json')) as f:
        sigdoc = json.load(f)
    sigs = {s['id']: s for s in sigdoc['signatures']}
    wanted = set(a.names)
    whole = set()
    for t in getattr(a, 'table', None) or ():
        whole |= {r['name'] for r in tsv[t] if sigs.get(r['sig'], {}).get('names') == [r['name']]} - wanted
    wanted |= whole
    alone = bc.Knowledge.alone()
    kept = []
    verify_at = {}
    for i, v in enumerate(tsv['verify']):
        verify_at.setdefault((v['kind'], v['item']), []).append(i)
    moved = {'functions': set(), 'globals': set()}
    verify_keys, sig_ids, records, errs = set(), set(), [], []
    for t in ('functions', 'globals'):
        for r in tsv[t]:
            if r['name'] not in wanted:
                continue
            wanted.discard(r['name'])
            s = sigs.get(r['sig'])
            if s is None or s.get('names') != [r['name']]:
                errs.append(f'{r["name"]} has no signature of its own')
                continue
            vk = 'func' if t == 'functions' else 'global'
            keys = {(vk, r['name']), (vk, r['addr']), ('sig', r['sig'])}
            rec = br.Record.create(r['name'], t, a.author or s['verified'][0]['by'], r, s)
            rows = sorted(i for k in keys for i in verify_at.get(k, ()))
            rec.doc['verify'] = [{c: tsv['verify'][i][c] for c in cols['verify']} for i in rows]
            bad = bc.check_record(rec, alone)
            if bad:
                (kept.append((r['name'], bad[0])) if r['name'] in whole or getattr(a, 'skip_unfit', False) else
                 errs.extend(f'{r["name"]}: {e}' for e in bad))
                continue
            records.append(rec)
            moved[t].add(r['addr'])
            verify_keys |= keys
            sig_ids.add(r['sig'])
    errs += [f'{n} is not in the shared tables' for n in sorted(wanted)]
    if errs:
        raise SystemExit('binding.py unfold: nothing moved\n  ' + '\n  '.join(errs))
    for rec in records:
        rec.save(root)
    for t in ('functions', 'globals'):
        remove_lines(os.path.join(db, f'{t}.tsv'), lambda c, t=t: c[0] not in moved[t])
    remove_lines(os.path.join(db, 'verify.tsv'), lambda c: (c[2], c[3]) not in verify_keys)
    sigdoc['signatures'] = [s for s in sigdoc['signatures'] if s['id'] not in sig_ids]
    with open(os.path.join(db, 'signatures.json'), 'w') as f:
        f.write(json.dumps(sigdoc, indent=1) + '\n')
    br.Store.forget()
    print(f'binding.py unfold: {sum(len(m) for m in moved.values())} binding(s) moved into {os.path.relpath(root)}')
    for name, why in kept:
        print(f'kept {name} in the tables: {why}')


def read_tsv_at(db, table):
    cols = kb.TABLES[table][0]
    with open(os.path.join(db, f'{table}.tsv')) as f:
        lines = [ln for ln in f.read().split('\n')[1:] if ln]
    return [dict(zip(cols, ln.split('\t'))) for ln in lines]


def lint_with(changes):
    """kb_lint's structural rules over the base branch plus these record files."""
    import kb_lint
    records = []
    for path, data in changes:
        try:
            records.append(br.Record.from_bytes(data, path[len('re/bindings/'):]))
        except br.RecordError:
            continue
    store = br.Store.get()
    base = store.records
    store.overlay(records)
    try:
        return [f'kb_lint: {e}' for e in kb_lint.KbLint(evidence_files=False, exe=False).run().E]
    finally:
        store.records = base


def cmd_ci(a):
    """Classify a pull request. --files is the GitHub API file list; --head holds the PR's copies of the binding
    records it touches (written by the workflow from git blobs, never executed). Everything else comes from this
    checkout, which is the base branch."""
    with open(a.files, 'rb') as f:
        files = br.parse_json(f.read(4 << 20)) if os.path.getsize(a.files) <= 4 << 20 else None
    result = {'verdict': 'review', 'records': [], 'errors': [], 'notes': []}
    if not isinstance(files, list) or not all(isinstance(x, dict) and isinstance(x.get('filename'), str) for x in files):
        result['verdict'] = 'fail'
        result['errors'].append('the file list is not a GitHub API file list')
    else:
        changes, other = [], []
        for x in files:
            path, status = x['filename'], x.get('status')
            if not br.RECORD_PATH_RE.match(path) or status not in ('added', 'modified'):
                other.append(path)
                continue
            head = os.path.join(a.head, path)
            new = open(head, 'rb').read(br.MAX_BYTES + 1) if os.path.isfile(head) and not os.path.islink(head) else None
            base = os.path.join(kb.ROOT, path)
            old = open(base, 'rb').read(br.MAX_BYTES + 1) if os.path.isfile(base) and not os.path.islink(base) else None
            changes.append((path, old, new))
        result = bc.classify(changes, a.author, bc.Knowledge(), kb.ROOT)
        if result['verdict'] != 'fail':
            lint_errors = lint_with([(path, new) for path, _, new in changes if new is not None])
            if lint_errors:
                result['verdict'] = 'fail'
                result['errors'] += lint_errors
        if other:
            result['verdict'] = 'fail' if result['verdict'] == 'fail' else 'review'
            result['notes'].append(f'{len(other)} file(s) outside re/bindings/*/*.json: review needed')
    with open(a.out, 'w') as f:
        json.dump(result, f, indent=1)
    print(f'binding.py ci: {result["verdict"]}')
    for e in result['errors']:
        print('ERROR', e)
    for n in result['notes']:
        print('NOTE ', n)
    return 1 if result['verdict'] == 'fail' else 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest='cmd', required=True)
    p = sp.add_parser('new')
    p.add_argument('name')
    p.add_argument('va')
    p.add_argument('--kind', required=True, choices=('func', 'site', 'data'))
    p.add_argument('--author', required=True)
    p.add_argument('--exe', required=True)
    p.add_argument('--evidence', required=True)
    p.add_argument('--notes', default='-')
    p.add_argument('--level', default='L2', choices=('L1', 'L2'))
    p.add_argument('--subsystem')
    p.add_argument('--proto', default='-')
    p.add_argument('--type')
    p.add_argument('--via')
    p.add_argument('--id')
    p.add_argument('--match-from')
    p.add_argument('--expect-len', type=int)
    p.add_argument('--func-start')
    p.add_argument('--data1-reason')
    p.add_argument('--used-by', action='append', default=[])
    p = sp.add_parser('verify')
    p.add_argument('names', nargs='+')
    p.add_argument('--exe', required=True)
    p.add_argument('--by', required=True)
    p = sp.add_parser('agree')
    p.add_argument('name')
    p.add_argument('--exe', required=True)
    p.add_argument('--by', required=True)
    p.add_argument('--evidence', required=True)
    p.add_argument('--notes', default='')
    p = sp.add_parser('check')
    p.add_argument('files', nargs='*')
    sp.add_parser('status')
    p = sp.add_parser('unfold')
    p.add_argument('names', nargs='*')
    p.add_argument('--table', action='append', choices=['functions', 'globals'],
                   help='also move every row of this table that has a signature of its own')
    p.add_argument('--skip-unfit', action='store_true', help='keep named rows that fail the record rules in the tables')
    p.add_argument('--author')
    p = sp.add_parser('ci')
    p.add_argument('--files', required=True)
    p.add_argument('--head', required=True)
    p.add_argument('--author', required=True)
    p.add_argument('--out', required=True)
    a = ap.parse_args(argv)
    return {'new': cmd_new, 'verify': cmd_verify, 'agree': cmd_agree, 'check': cmd_check, 'status': cmd_status,
            'unfold': cmd_unfold, 'ci': cmd_ci}[a.cmd](a) or 0


if __name__ == '__main__':
    sys.exit(main())
