#!/usr/bin/env python3
"""Contribute a binding: one file under re/bindings/, checked by CI, proven on your own F1_25.exe (PROTOCOL.md §11).

  binding.py new NAME VA --kind func|site|data --author LOGIN --exe F1_25.exe --evidence "..." [options]
                                  make the signature, write re/bindings/<subsystem>/NAME.json and prove it
  binding.py verify NAME... --exe F1_25.exe --by LOGIN
                                  resolve each record on your exe with the core's rules and record the proof
  binding.py agree NAME --exe F1_25.exe --by LOGIN --evidence "decomp:...;xref:..." [--notes TEXT]
                                  second contributor: your own proof plus an agree row (makes it experimental)
  binding.py doctor [--author LOGIN]
                                  what the pull request check decides for your changes, why, and how to fix it
  binding.py check [FILE...]      the static checks CI runs (every record when no file is given)
  binding.py status [NAME...]     records, their level, proofs, verifiers and what each still needs to export
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
        raise SystemExit(f'binding.py: no record for {name} under re/bindings (python3 re/tools/binding.py status '
                         f'lists them; a binding still in re/db/functions.tsv or globals.tsv has no record yet)')
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
    s = rec.signature
    if ok and is_kb and (proof['result']['match'], proof['result']['target']) != (s['match'], s['target']):
        ok = False
    if ok and s.get('made_on') and exe['sha256'].startswith(s['made_on']) and \
            (proof['result']['match'], proof['result']['target']) != (s['made_on_match'], s['made_on_target']):
        return proof, False, (f'it resolves to match {proof["result"]["match"]} target {proof["result"]["target"]}, '
                              f'not to {s["made_on_match"]} {s["made_on_target"]} where it was made')
    return proof, ok, r.get('reason', '')


NEEDS_MODULES = ('binding.py new needs Python 3.10+ with pefile, numpy and capstone: run it through re/tools/py, or '
                 'python3 -m pip install pefile numpy capstone')


def build_name(want):
    """The re/db/builds overlay name of a build that is not the KB build, else None."""
    kb_sha = kb.load_signatures()['build']['sha256']
    bdir = os.path.join(kb.DB, 'builds')
    for f in sorted(os.listdir(bdir)):
        if f.endswith('.json'):
            with open(os.path.join(bdir, f)) as fh:
                sha = json.load(fh)['build']['sha256']
            if sha == want and sha != kb_sha:
                return f[:-5]
    return None


def port_exe_or_exit(path, exe, cmd):
    """The overlay name of the exe's build when it is a supported build other than the KB build."""
    name = build_name(exe['sha256'])
    if name:
        return name
    kb_build = kb.load_signatures()['build']
    raise SystemExit(
        f'binding.py {cmd}: {path} is an unknown build (sha256 {exe["sha256"][:16]}, size {exe["size"]}, PE timestamp '
        f'{exe["pe_timestamp"]}).\n  binding.py {cmd} runs on the KB build (Steam F1 25 1.0.141.7865, '
        f'{kb_build["size"]} bytes) or on a build in re/db/builds, such as the current Steam build. Update the game '
        f'through Steam, or open an issue with the address and your evidence.')


def to_kb(sig, path, build, exe):
    """The signature made on another build, with its KB addresses taken from the bindings around each address
    (bindverify.kb_twin) and the made-on fields recording where it matched and how the twin was found."""
    import bindverify
    img = bindverify.Image(path)
    marks = bindverify.anchors(img, build)
    with open(os.path.join(kb.DB, 'builds', build + '.json')) as f:
        smap = json.load(f).get('sections', {})
    out, texts, shift = dict(sig), [], None
    for k in ('target', 'match', 'func_start'):
        if not sig.get(k):
            continue
        va = int(sig[k], 16)
        kb_rva, why = bindverify.kb_twin(img, build, va - IMAGE_BASE, marks)
        if kb_rva is None:
            raise SystemExit(f'binding.py new: nothing written, the {k} 0x{va:x} has no KB twin: {why}.\n  fix: pick '
                             f'an address between two bindings that moved by the same amount, or ask a maintainer '
                             f'with the KB build to make the record')
        out[k] = hex(kb_rva + IMAGE_BASE)
        if k == 'target':
            shift = why.get('shift')
        if k == 'target' or why.get('shift') is None or why['shift'] != shift:
            texts.append(f'the {k} 0x{va:x} is KB {out[k]}: {why["text"]}')
    inverse = {v: k for k, v in smap.items()}
    out['section'] = inverse.get(sig['section'], sig['section'])
    out['made_on'] = exe['sha256'][:16]
    out['made_on_match'], out['made_on_target'] = sig['match'], sig['target']
    twin = f'made on {build}; ' + '; '.join(texts)
    ref = os.environ.get('SWOFTY_KB_EXE', '')
    if ref and os.path.isfile(ref):
        import sigport
        ok, detail = sigport.check_twin(img, bindverify.Image(ref), smap, int(sig['target'], 16) - IMAGE_BASE,
                                        int(out['target'], 16) - IMAGE_BASE)
        if not ok:
            raise SystemExit(f'binding.py new: nothing written, the KB exe in SWOFTY_KB_EXE disagrees with the twin '
                             f'{out["target"]}: {detail}')
        twin += f'; checked on the KB exe: {detail}'
    out['twin'] = twin
    return out


def call_candidate(img, path, va):
    """A call signature through the direct call site with the shortest unique pattern, for a function whose own
    bytes repeat elsewhere: the core then resolves the function from the call's rel32."""
    import bindverify, sigcheck
    refs = bindverify.Image(path).rel32_refs(va - IMAGE_BASE)
    best = None
    for r in refs[:32]:
        site = r - 1 + IMAGE_BASE
        if img.read(site, 1) != b'\xe8':
            continue
        sig = sigcheck.make(img, site, 'mid')
        if sig and (best is None or len(sig['pattern']) < len(best[1]['pattern'])):
            best = (site, sig)
    if not best:
        return None, None
    site, sig = best
    start = int(sig['match'], 16)
    sig.update(kind='call', anchor=hex(site + 1 - start), resolve={'kind': 'rel32', 'insn': hex(site - start)},
               target=hex(va))
    sig.pop('expect', None)
    return site, sig


def cmd_new(a):
    try:
        import sigcheck, sitereg
        import capstone, numpy, pefile  # noqa: F401
    except ImportError:
        raise SystemExit(NEEDS_MODULES)
    va = int(a.va, 16)
    if va < IMAGE_BASE:
        raise SystemExit(f'binding.py new: {a.va} is below the image base; pass the virtual address, '
                         f'0x{va + IMAGE_BASE:x} if {a.va} is an offset from the base')
    sub = a.subsystem or sitereg_subsystem(a.name)
    need_exe(a.exe, 'new')
    exe = exe_identity(a.exe)
    port = None if exe['sha256'] == kb.load_signatures()['build']['sha256'] else port_exe_or_exit(a.exe, exe, 'new')
    img = sigcheck.Image(a.exe)
    kind = {'func': 'func_start', 'site': 'mid', 'data': 'data'}[a.kind]
    via_call = None
    if a.kind == 'data':
        if not a.via or not a.type:
            raise SystemExit('binding.py new: --kind data needs --type (the C type of the global, for example '
                             '"void *") and --via (the address of an instruction that reads or writes it rip-relative)')
        sig = sitereg.data_candidate(img, va, int(a.via, 16))
    elif a.match_from:
        sig = sitereg.anchored_candidate(img, va, int(a.match_from, 16), kind)
    else:
        sig = sigcheck.make(img, va, kind, int(a.func_start, 16) if a.func_start else None)
        if not sig and a.kind == 'func':
            via_call, sig = call_candidate(img, a.exe, va)
    if not sig:
        hint = ('no direct call reaches it either, so pass --match-from <the address of an earlier instruction> to '
                'start the pattern before it' if a.kind == 'func' else
                'pass --match-from <the address of an earlier instruction in the same function> to start the pattern '
                'before it')
        raise SystemExit(f'binding.py new: the bytes at 0x{va:x} are not unique in {img.sec_of(va)} within 96 bytes '
                         f'(the same code is elsewhere in the exe); {hint}')
    if via_call:
        print(f'binding.py new: the code at 0x{va:x} repeats elsewhere in the exe, so the record finds {a.name} '
              f'through its call at 0x{via_call:x} (signature kind call, resolved from the rel32)')
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
    sig['verified'] = [{'build': exe['sha256'][:16], 'date': today(), 'by': a.author, 'how': 'static'}]
    if port:
        sig = to_kb(sig, a.exe, port, exe)
        print(f'binding.py new: {sig["twin"]}')
    sig['used_by'] = a.used_by
    sig = {k: v for k, v in sig.items() if v is not None}
    table = 'functions' if a.kind == 'func' else 'globals'
    row = {'addr': f'0x{va:x}', 'level': a.level, 'subsystem': sub, 'evidence': a.evidence, 'notes': a.notes,
           'proto': a.proto, 'type': 'code' if a.kind == 'site' else (a.type or '-')}
    rec = br.Record.create(a.name, table, a.author, row, sig)
    errs = bc.check_record(rec, bc.Knowledge())
    if errs:
        raise SystemExit('binding.py new: nothing written, the record would fail CI:\n  ' +
                         '\n  '.join(bc.with_fixes([f're/bindings/{rec.relpath}: {e}' for e in errs])))
    proof, ok, why = prove(rec, a.exe, a.author, exe)
    if not ok:
        raise SystemExit(f'binding.py new: nothing written, {a.name} does not resolve on {a.exe}: '
                         f'{why or proof["sigcheck"]}')
    rec.proofs.append(proof)
    rec.save()
    print(f'wrote re/bindings/{rec.relpath} ({sig["id"]}, match {sig["match"]}, proof on {proof["exe"]["sha256"][:16]})')
    print(f'next: python3 re/tools/binding.py doctor --author {a.author}, then commit only that file and open a pull '
          f'request')


def sitereg_subsystem(name):
    import re
    m = re.match(r'^(?:g|k|vt|site)_([A-Z][A-Za-z0-9]*)_|^([A-Z][A-Za-z0-9]*)_', name)
    prefix = m and (m.group(1) or m.group(2))
    for sid, s in kb.subsystems().items():
        if s['prefix'] == prefix:
            return sid
    raise SystemExit(f'binding.py: no subsystem has the prefix of {name}: start the name with a prefix from '
                     f're/db/subsystems.tsv (column prefix), or pass --subsystem <its id>')


def need_exe(path, cmd):
    if not os.path.isfile(path):
        raise SystemExit(f'binding.py {cmd}: --exe {path} is not a file; pass the path to your F1_25.exe')


def cmd_verify(a):
    need_exe(a.exe, 'verify')
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
    need_exe(a.exe, 'agree')
    if a.by == rec.author:
        raise SystemExit(f'binding.py agree: {a.by} wrote {rec.name}; the agree row comes from a second GitHub '
                         f'account that re-derived it (re/PROTOCOL.md §5)')
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
    wanted = {os.path.abspath(p) for p in paths}
    known = {os.path.join(br.RECORDS, rec.relpath) for rec in store.records}
    errs += [f'{os.path.relpath(p)}: ' + ('not a binding record; records are re/bindings/<subsystem>/<Name>.json'
                                          if os.path.exists(p) else 'no such file')
             for p in sorted(wanted - known)]
    for rec in store.records:
        if paths and os.path.join(br.RECORDS, rec.relpath) not in wanted:
            continue
        errs += [f're/bindings/{rec.relpath}: {e}' for e in bc.check_record(rec, knowledge.without(rec),
                                                                             're/bindings/' + rec.relpath)]
    return errs


def cmd_check(a):
    errs = check_paths(a.files)
    for e in bc.with_fixes(errs):
        print(e if e.startswith('  fix: ') else 'ERROR ' + e)
    n = len(br.Store.get().records)
    which = f'{len(a.files)} of {n}' if a.files else f'{n}'
    print(f'binding.py check: {which} record(s) checked, {len(errs)} error(s)')
    return 1 if errs else 0


def export_state(gate, rec):
    """(status, what it still needs) of one record under bindgen's gate."""
    row = rec.table_row()
    vkind = 'func' if rec.table == 'functions' else 'global'
    status, why = gate(row['level'], row['evidence'], [(vkind, rec.name), (vkind, row['addr'])])
    if status:
        return status, '-'
    if row['level'] not in ('L2', 'L3'):
        return 'not exported', 'L2: decomp: and xref: evidence, then an agree row'
    newest = gate.newest([(vkind, rec.name), (vkind, row['addr'])])
    if newest:
        return 'not exported', (f'its newest verify row {newest[1]} is {newest[0]}: a new agree row after the notes '
                                f'are fixed (binding.py agree)')
    return 'not exported', 'an agree row from a second contributor (binding.py agree)'


def cmd_status(a):
    import bindgen
    gate = bindgen.Gate()
    store = br.Store.get()
    recs = store.records
    if a.names:
        recs = [find(n) for n in a.names]
    rows = []
    for rec in recs:
        status, need = export_state(gate, rec)
        rows.append((rec.name, rec.table_row()['level'], status, len(rec.proofs),
                     ','.join(v['verifier'] for v in rec.verify if v['verdict'] == 'agree') or '-', need))
    w = max([len(r[0]) for r in rows] + [4])
    try:
        print(f'{"name":{w}s}  level  status        proofs  verified by       needs')
        for r in rows:
            print(f'{r[0]:{w}s}  {r[1]:5s}  {r[2]:12s}  {r[3]:6d}  {r[4]:16s}  {r[5]}')
        print(f'{len(rows)} record(s)')
    except BrokenPipeError:
        sys.stdout = None


def git(*args, check=True):
    import subprocess
    r = subprocess.run(['git', '-C', kb.ROOT, *args], capture_output=True)
    if check and r.returncode:
        raise SystemExit(f'binding.py doctor: git {" ".join(args)} failed: {r.stderr.decode(errors="replace").strip()}')
    return r


def local_changes(base):
    """[(path, git status)] of everything this checkout changes against `base`, committed or not, untracked included."""
    out = {}
    for line in git('diff', '--name-status', '--no-renames', base, '--').stdout.decode().splitlines():
        status, path = line.split('\t', 1)
        out[path] = {'A': 'added', 'D': 'removed'}.get(status[0], 'modified')
    for path in git('ls-files', '--others', '--exclude-standard').stdout.decode().splitlines():
        out[path] = 'added'
    return sorted(out.items())


def pick_base(base):
    if base:
        return base
    for ref in ('origin/main', 'upstream/main', 'main'):
        if git('rev-parse', '--verify', '--quiet', ref, check=False).returncode == 0:
            return git('merge-base', 'HEAD', ref).stdout.decode().strip()
    raise SystemExit('binding.py doctor: no main branch to compare with; pass --base <the commit your branch starts from>')


def judge(changes, other, author, knowledge):
    """What the bindings workflow decides for these record changes plus `other` paths, with a printable summary."""
    result = bc.classify(changes, author, knowledge, kb.ROOT)
    if result['verdict'] != 'fail':
        lint_errors = lint_with([(path, new) for path, _, new in changes if new is not None])
        if lint_errors:
            result['verdict'] = 'fail'
            result['errors'] += lint_errors
    if other:
        result['verdict'] = 'fail' if result['verdict'] == 'fail' else 'review'
        shown = ', '.join(other[:5]) + (f' and {len(other) - 5} more' if len(other) > 5 else '')
        result['notes'].append(f'{len(other)} file(s) outside re/bindings/*/*.json: review needed ({shown})')
    result['summary'] = summary(result)
    return result


def summary(result):
    """Errors and notes, each followed by its fix: what the workflow prints on the pull request."""
    out = []
    for prefix, msgs in (('error: ', result['errors']), ('', result['notes'])):
        for m in msgs:
            out.append(prefix + m)
            fix = bc.fix_for(m)
            if fix:
                out.append('  fix: ' + fix)
    return out


def base_knowledge(changes):
    """The knowledge base as the base branch has it: the changed records as they were there, or absent."""
    store = br.Store.get()
    mine = {path[len('re/bindings/'):] for path, _, _ in changes}
    before = []
    for path, old, _ in changes:
        if old is not None:
            try:
                before.append(br.Record.from_bytes(old, path[len('re/bindings/'):]))
            except br.RecordError:
                pass
    keep = store.records
    store.records = sorted([r for r in keep if r.relpath not in mine] + before, key=lambda r: r.relpath)
    try:
        return bc.Knowledge(), keep
    except BaseException:
        store.records = keep
        raise


def doctor_changes(base):
    """([(path, base bytes or None, checkout bytes or None)], [other paths]) of this checkout against `base`."""
    changes, other = [], []
    for path, status in local_changes(base):
        p = os.path.join(kb.ROOT, path)
        if not br.RECORD_PATH_RE.match(path) or (status != 'removed' and (os.path.islink(p) or not os.path.isfile(p))):
            other.append(path)
            continue
        new = None
        if status != 'removed':
            with open(p, 'rb') as f:
                new = f.read(br.MAX_BYTES + 1)
        got = git('show', f'{base}:{path}', check=False)
        changes.append((path, got.stdout if got.returncode == 0 else None, new))
    return changes, other


DOCTOR_VERDICT = {
    'auto': 'would merge itself: every changed file is a binding record that passes the checks',
    'review': 'would pass the checks and wait for a maintainer\'s review, because:',
    'fail': 'would fail the binding check, because:',
}


def cmd_doctor(a):
    """The pull request check run on this checkout: the records it adds or changes against the main branch."""
    base = pick_base(a.base)
    changes, other = doctor_changes(base)
    if a.author:
        author = a.author
    else:
        authors = set()
        for path, old, new in changes:
            try:
                if old is None and new is not None:
                    authors.add(br.Record.from_bytes(new, path[len('re/bindings/'):]).author)
            except br.RecordError:
                pass
        if len(authors) != 1:
            found = f' (the new records name {", ".join(sorted(authors))})' if authors else ''
            raise SystemExit(f'binding.py doctor: pass --author <the GitHub login that opens the pull request>{found}')
        author = authors.pop()
    store = br.Store.get()
    knowledge, keep = base_knowledge(changes)
    try:
        result = judge(changes, other, author, knowledge)
    finally:
        store.records = keep
    print(f'binding.py doctor: {len(changes)} record file(s) and {len(other)} other file(s) changed against '
          f'{base[:12]}, pull request by {author}')
    for path, _, _ in changes:
        print(f'  {path}')
    print(f'verdict: {result["verdict"]}: this pull request {DOCTOR_VERDICT[result["verdict"]]}')
    for line in result['summary']:
        print(('    ' if line.startswith('  fix: ') else '  - ') + line.strip())
    if result['verdict'] != 'fail' and changes:
        import bindgen
        gate = bindgen.Gate()
        print('after the merge:')
        for path, _, new in changes:
            rec = store.by_relpath(path[len('re/bindings/'):])
            if rec:
                status, need = export_state(gate, rec)
                print(f'  {rec.name}: {status}' + (f'; needs {need}' if need != '-' else ''))
    return 1 if result['verdict'] == 'fail' else 0


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
    if os.path.abspath(root) == os.path.abspath(br.RECORDS):
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
        result['summary'] = summary(result)
    else:
        changes, other, missing = [], [], []
        for x in files:
            path, status = x['filename'], x.get('status')
            if not br.RECORD_PATH_RE.match(path) or status not in ('added', 'modified', 'removed'):
                other.append(path)
                continue
            base = os.path.join(kb.ROOT, path)
            old = open(base, 'rb').read(br.MAX_BYTES + 1) if os.path.isfile(base) and not os.path.islink(base) else None
            new = None
            if status != 'removed':
                head = os.path.join(a.head, path)
                if not os.path.isfile(head) or os.path.islink(head):
                    missing.append(path)
                    continue
                with open(head, 'rb') as f:
                    new = f.read(br.MAX_BYTES + 1)
            changes.append((path, old, new))
        result = judge(changes, other, a.author, bc.Knowledge())
        if missing:
            result['verdict'] = 'fail' if result['verdict'] == 'fail' else 'review'
            result['notes'] = [n for n in result['notes'] if n != bc.NO_RECORDS] + [
                f'{p}: the pull request has no regular file there: review needed' for p in missing]
            result['summary'] = summary(result)
    with open(a.out, 'w') as f:
        json.dump(result, f, indent=1)
    print(f'binding.py ci: {result["verdict"]}')
    for line in result['summary']:
        print(line if line.startswith('  fix: ') else ('ERROR ' + line[7:] if line.startswith('error: ') else 'NOTE  ' + line))
    return 1 if result['verdict'] == 'fail' else 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest='cmd', required=True, metavar='command')
    exe_help = 'your F1_25.exe'
    p = sp.add_parser('new', help='make a record and prove it on your exe',
                      description='Write re/bindings/<subsystem>/NAME.json: the row, a signature made from the bytes '
                                  'at VA, and a proof that the core resolves it on your exe. Your exe is the current '
                                  'Steam build or any build in re/db/builds. Records hold addresses on the KB build '
                                  '(Steam 1.0.141.7865); on another build the KB addresses come from the bindings on '
                                  'either side, which must have moved by the same amount, and the record keeps where '
                                  'it was made and how its KB twin was found. Nothing is written when a check fails. '
                                  'Needs pefile, numpy and capstone (re/tools/py has them).')
    p.add_argument('name', help='the binding name, <Prefix>_<Verb>_<Noun> for a function or g_/k_/vt_/site_<Prefix>_'
                                '<Noun> for a global (re/PROTOCOL.md §3); the prefix picks the subsystem')
    p.add_argument('va', help='its virtual address on your exe, for example 0x142ea5a50')
    p.add_argument('--kind', required=True, choices=('func', 'site', 'data'),
                   help='func: a function you wrap; site: an instruction you run code at; data: a global')
    p.add_argument('--author', required=True, help='your GitHub login, the account that opens the pull request')
    p.add_argument('--exe', required=True, help=exe_help + ': the current Steam build or another build in re/db/builds; '
                                                         'VA and every address option are on this exe')
    p.add_argument('--evidence', required=True,
                   help='kind:value tokens joined by ;, for example "decomp:own disasm 0x142ea5a50-0x142ea5adf;'
                        'xref:0x142ea5763"; L2 needs decomp: and xref:')
    p.add_argument('--notes', default='-', help='what it does, in one line: arguments, fields, callers')
    p.add_argument('--level', default='L2', choices=('L1', 'L2'),
                   help='L2 (default) when you read the code, L1 for a pattern match without a read decompile')
    p.add_argument('--subsystem', help='the subsystem id from re/db/subsystems.tsv, when the name prefix is ambiguous')
    p.add_argument('--proto', default='-', help='the C prototype of a func, for example "void Game_X_Y(void *self)"')
    p.add_argument('--type', help='the C type of a data global, for example "void *"')
    p.add_argument('--via', help='data: the address of an instruction that reads or writes the global rip-relative')
    p.add_argument('--id', help='the signature id, <subsystem>.<snake_case_name> (default: from the name)')
    p.add_argument('--match-from', metavar='VA', help='start the pattern at this earlier instruction when the bytes at '
                                                      'the address are not unique')
    p.add_argument('--expect-len', type=int, metavar='N', help='site: how many bytes at the address to expect '
                                                               '(default: whole instructions covering 5 bytes)')
    p.add_argument('--func-start', metavar='VA', help='a .data1 site: the start of its function when .pdata lacks it')
    p.add_argument('--data1-reason', help='a .data1 site: why no .xtls site works (re/PROTOCOL.md §2)')
    p.add_argument('--used-by', action='append', default=[], metavar='PATH', help='code that uses the binding')
    p = sp.add_parser('doctor', help='what the pull request check decides for your changes, and how to fix them',
                      description='Compares this checkout (commits, edits and new files) with the main branch and '
                                  'runs the bindings workflow\'s check on it: whether the pull request would merge '
                                  'itself, need a review or fail, why, and the fix for each problem.')
    p.add_argument('--author', help='the GitHub login that opens the pull request (default: the records\' author)')
    p.add_argument('--base', help='the commit to compare with (default: where HEAD leaves origin/main or main)')
    p = sp.add_parser('verify', help='prove existing records again on your exe',
                      description='Resolve each record on your exe with the core\'s rules and store the proof, '
                                  'replacing your earlier proof on the same exe. Any build in re/db/builds works.')
    p.add_argument('names', nargs='+', help='record names')
    p.add_argument('--exe', required=True, help=exe_help)
    p.add_argument('--by', required=True, help='your GitHub login')
    p = sp.add_parser('agree', help='second contributor: add your proof and an agree row',
                      description='Re-derive the binding from the code yourself first (re/PROTOCOL.md §5). Adds your '
                                  'proof and an agree row; with L2 the next build exports it as experimental.')
    p.add_argument('name', help='the record name')
    p.add_argument('--exe', required=True, help=exe_help)
    p.add_argument('--by', required=True, help='your GitHub login; not the record\'s author')
    p.add_argument('--evidence', required=True, help='your own decomp: and xref: tokens')
    p.add_argument('--notes', default='', help='what you checked')
    p = sp.add_parser('check', help='the static checks CI runs, on every record or the given files')
    p.add_argument('files', nargs='*', help='record files (default: every record)')
    p = sp.add_parser('status', help='whether records export, and what each still needs')
    p.add_argument('names', nargs='*', help='record names (default: every record)')
    p = sp.add_parser('unfold', help='maintainer: move bindings from the shared tables into records')
    p.add_argument('names', nargs='*', help='binding names')
    p.add_argument('--table', action='append', choices=['functions', 'globals'],
                   help='also move every row of this table that has a signature of its own')
    p.add_argument('--skip-unfit', action='store_true', help='keep named rows that fail the record rules in the tables')
    p.add_argument('--author', help='the author to write (default: the signature\'s first verifier)')
    p = sp.add_parser('ci', help='the pull request check (.github/workflows/bindings.yml); doctor runs it locally')
    p.add_argument('--files', required=True, help='the GitHub API file list of the pull request, as JSON')
    p.add_argument('--head', required=True, help='a folder with the pull request\'s copies of its record files')
    p.add_argument('--author', required=True, help='the pull request author\'s login')
    p.add_argument('--out', required=True, help='where to write the result JSON')
    a = ap.parse_args(argv)
    return {'new': cmd_new, 'doctor': cmd_doctor, 'verify': cmd_verify, 'agree': cmd_agree, 'check': cmd_check,
            'status': cmd_status, 'unfold': cmd_unfold, 'ci': cmd_ci}[a.cmd](a) or 0


if __name__ == '__main__':
    sys.exit(main())
