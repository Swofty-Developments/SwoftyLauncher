#!/usr/bin/env python3
"""Static checks for binding records: no game files needed, safe on hostile input (PROTOCOL.md §11).

check_record() checks one record against the knowledge base it would join. classify() compares the records a pull
request changes with the base branch and decides whether the change may merge without review:

  add      a new record: level L1 or L2, no verify rows, one proof by its author on a known build
  proof    an existing record that only gains a proof and an agree row from someone who is not its author
  review   anything else
"""
import json, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import binding_record as br
import kb
from sigcheck import MIN_LEN, parse_pattern

IMAGE_BASE = 0x140000000
SECTIONS = ('.xtls', '.data1')
SIG_KINDS = {'func_start', 'mid', 'call', 'data'}
AUTO_LEVELS = ('L1', 'L2')
MAX_AUTO_RECORDS = 25
CODE_DIRS = ('native/src', 'addons', 'packages', 'templates', 'apps')
CODE_EXTS = ('.c', '.cc', '.cpp', '.h', '.hpp', '.rs', '.py', '.json', '.toml', '.ts', '.js', '.lua')


def binding_kind(table, row, sig):
    if table == 'functions':
        return 'func'
    if row['type'] == 'code':
        return 'call' if (sig.get('expect') or '').upper().startswith('E8') else 'site'
    return 'vtable' if row['type'].startswith('vtable') else 'data'


WANT_SIG_KIND = {'func': ('func_start', 'call'), 'site': ('mid',), 'call': ('mid',), 'vtable': ('data',),
                 'data': ('data',)}


class Knowledge:
    """The base branch's knowledge base: names, addresses, signatures, builds and the subsystem table."""

    def __init__(self):
        self.subsystems = kb.subsystems()
        sigdoc = kb.load_signatures()
        self.build = sigdoc['build']
        self.signatures = {s['id']: s for s in sigdoc['signatures']}
        self.patterns = {(s['section'], s['pattern']): s['id'] for s in sigdoc['signatures']}
        self.targets = {}
        for s in sigdoc['signatures']:
            self.targets.setdefault((s['kind'], str(s['target']).lower()), s['id'])
        self.names, self.addrs = {}, {'functions': {}, 'globals': {}}
        for t in ('functions', 'globals'):
            for r in kb.read_table(t)[0]:
                self.names[r['name']] = (t, r.get('_record'))
                self.addrs[t][r['addr']] = (r['name'], r.get('_record'))
        self.builds = {self.build['sha256']: 'KB build'}
        bdir = os.path.join(kb.DB, 'builds')
        for f in sorted(os.listdir(bdir)) if os.path.isdir(bdir) else ():
            if f.endswith('.json'):
                with open(os.path.join(bdir, f)) as fh:
                    doc = json.load(fh)
                sha = (doc.get('build') or {}).get('sha256')
                if sha:
                    self.builds.setdefault(sha, f[:-5])

    def without(self, record):
        """Forget the record's own entries, so a record is never its own conflict."""
        if record is None:
            return self
        k = Knowledge.__new__(Knowledge)
        k.__dict__ = dict(self.__dict__)
        mine = record.relpath
        k.names = {n: v for n, v in self.names.items() if v[1] != mine}
        k.addrs = {t: {a: v for a, v in m.items() if v[1] != mine} for t, m in self.addrs.items()}
        sid = record.signature['id'] if isinstance(record.signature, dict) else None
        k.signatures = {i: s for i, s in self.signatures.items() if i != sid}
        k.patterns = {p: i for p, i in self.patterns.items() if i != sid}
        k.targets = {t: i for t, i in self.targets.items() if i != sid}
        return k


def pattern_errors(text):
    try:
        pat = parse_pattern(text)
    except ValueError as e:
        return [str(e)], []
    errs = []
    if len(pat) < MIN_LEN:
        errs.append(f'is shorter than {MIN_LEN} bytes')
    if len(pat) > 256:
        errs.append('is longer than 256 bytes')
    if pat and (pat[0] is None or pat[-1] is None):
        errs.append('starts or ends with a wildcard')
    return errs, pat


def signature_errors(sig, row, table, name, kb_build):
    errs = []
    sub = row['subsystem']
    if not kb.SIG_ID_RE.match(sig['id']) or sig['id'].split('.')[0] != sub:
        errs.append(f'signature.id must be {sub}.<snake_case_name>')
    if sig['section'] not in SECTIONS:
        errs.append(f'signature.section must be one of {", ".join(SECTIONS)}')
    if sig['section'] == '.data1' and not sig.get('data1_reason'):
        errs.append('a .data1 signature needs data1_reason (PROTOCOL.md §2)')
    if sig['kind'] not in SIG_KINDS:
        errs.append(f'signature.kind must be one of {", ".join(sorted(SIG_KINDS))}')
    perrs, pat = pattern_errors(sig['pattern'])
    errs += [f'signature.pattern {e}' for e in perrs]
    for k in ('match', 'target'):
        if not kb.ADDR_RE.match(sig[k]):
            errs.append(f'signature.{k} must be a lowercase 0x14xxxxxxx address')
    if sig['target'] != row['addr']:
        errs.append(f'signature.target {sig["target"]} is not row.addr {row["addr"]}')
    if sig['names'] != [name]:
        errs.append(f'signature.names must be [{name!r}]')
    try:
        anchor = br.number(sig['anchor'])
        res = {k: (v if k == 'kind' else br.number(v)) for k, v in sig['resolve'].items()}
    except (ValueError, TypeError):
        return errs + ['signature.anchor and resolve offsets must be numbers']
    if not 0 <= anchor < 256:
        errs.append('signature.anchor must be inside the first 256 bytes of the match')
    kind = res.get('kind')
    if kind not in ('addr', 'rel32', 'riprel') or set(res) - {'kind', 'insn', 'len'}:
        errs.append('signature.resolve.kind must be addr, rel32 or riprel with only insn and len')
    elif not errs and kind == 'addr':
        if set(res) != {'kind'}:
            errs.append('signature.resolve addr takes no insn or len')
        elif int(sig['match'], 16) + anchor != int(sig['target'], 16):
            errs.append('signature.target must equal match + anchor for resolve kind addr')
    elif not errs:
        insn = res.get('insn', anchor - 1 if kind == 'rel32' else anchor)
        if not 0 <= insn <= anchor:
            errs.append('signature.resolve.insn must come before the anchor')
        elif kind == 'rel32':
            if anchor - insn not in (1, 2):
                errs.append('signature.resolve rel32 needs the anchor 1 (call, jmp) or 2 (jcc) bytes after insn')
            if pat[anchor:anchor + 4] != [None] * 4 and len(pat) >= anchor + 4:
                errs.append('signature.pattern must wildcard the rel32 operand')
        elif kind == 'riprel':
            n = res.get('len')
            if n is None or not anchor + 4 <= insn + n <= insn + 15:
                errs.append('signature.resolve riprel needs len covering the disp32')
            if pat[anchor:anchor + 4] != [None] * 4 and len(pat) >= anchor + 4:
                errs.append('signature.pattern must wildcard the disp32')
    if sig.get('expect') is not None and not errs:
        try:
            want = parse_pattern(sig['expect'])
        except ValueError as e:
            want = None
            errs.append(f'signature.expect {e}')
        if want is not None and (None in want or not 0 < len(want) <= 16):
            errs.append('signature.expect must be 1 to 16 concrete bytes')
        elif want:
            for i, b in enumerate(want):
                p = pat[anchor + i] if anchor + i < len(pat) else None
                if p is not None and p != b:
                    errs.append(f'signature.expect byte {i} contradicts the pattern')
                    break
    for k in ('func_start',):
        if sig.get(k) is not None and not kb.ADDR_RE.match(sig[k]):
            errs.append(f'signature.{k} must be an address')
    if not any(isinstance(v, dict) and kb_build.startswith(v.get('build', '-') or '-') and len(v.get('build', '')) >= 16
               for v in sig['verified']):
        errs.append(f'signature.verified needs an entry for the KB build {kb_build[:16]}')
    return errs


def proof_errors(record, known_builds, kb_build):
    errs = []
    sig = record.signature
    for i, p in enumerate(record.proofs):
        where = f'proofs[{i}]'
        sha = p['exe']['sha256']
        if sha not in known_builds:
            errs.append(f'{where} ran on {sha[:16]}, which is not a build in re/db/builds')
            continue
        if p['digest'] != br.resolution_digest(sig, sha):
            errs.append(f'{where} is for a different signature or exe (digest mismatch); rerun binding.py verify')
        if p['result']['result'] != 'resolved' or p['result']['matches'] != 1:
            errs.append(f'{where} did not resolve to exactly one match')
        if sha == kb_build['sha256']:
            if p['exe']['size'] != kb_build['size'] or p['exe']['pe_timestamp'] != kb_build['pe_timestamp']:
                errs.append(f'{where} exe size or timestamp differs from the KB build')
            if p['result']['match'] != sig['match'] or p['result']['target'] != sig['target']:
                errs.append(f'{where} found match {p["result"]["match"]} target {p["result"]["target"]}, the record '
                            f'says {sig["match"]} {sig["target"]}')
    return errs


def check_record(record, knowledge, relpath=None):
    """Every static error for one record joining `knowledge` (which must not already contain it)."""
    errs = record.shape_errors()
    if errs:
        return errs
    if relpath is not None:
        m = br.RECORD_PATH_RE.match(relpath)
        if not m or relpath != 're/bindings/' + br.relpath_for(record.subsystem, record.name):
            return [f'must live at re/bindings/{br.relpath_for(record.subsystem, record.name)}']
    row = record.table_row()
    errs += [f'row: {e}' for e in kb.validate_row(record.table, row, knowledge.subsystems)]
    if row['subsystem'] not in knowledge.subsystems:
        return errs
    lo, hi = IMAGE_BASE, IMAGE_BASE + knowledge.build['size']
    if kb.ADDR_RE.match(row['addr']) and not lo <= int(row['addr'], 16) < hi:
        errs.append('row.addr is outside the KB build image')
    sig = record.signature
    errs += signature_errors(sig, row, record.table, record.name, knowledge.build['sha256'])
    if not errs:
        kind = binding_kind(record.table, row, sig)
        if sig['kind'] not in WANT_SIG_KIND[kind]:
            errs.append(f'signature.kind {sig["kind"]} cannot back a {kind} binding')
    if record.name in knowledge.names:
        errs.append(f'name {record.name} is already in the knowledge base')
    if row['addr'] in knowledge.addrs[record.table]:
        errs.append(f'{row["addr"]} is already {knowledge.addrs[record.table][row["addr"]][0]} in {record.table}')
    if sig['id'] in knowledge.signatures:
        errs.append(f'signature id {sig["id"]} is already used')
    if (sig['section'], sig['pattern']) in knowledge.patterns:
        errs.append(f'signature.pattern is the same as {knowledge.patterns[(sig["section"], sig["pattern"])]}')
    if (sig['kind'], sig['target']) in knowledge.targets:
        errs.append(f'{sig["target"]} already has the {sig["kind"]} signature {knowledge.targets[(sig["kind"], sig["target"])]}')
    vkind = 'func' if record.table == 'functions' else 'global'
    for i, v in enumerate(record.verify):
        errs += [f'verify[{i}]: {e}' for e in kb.validate_row('verify', v)]
        if v['kind'] != vkind or v['item'] not in (record.name, row['addr']):
            errs.append(f'verify[{i}] must be about {vkind} {record.name}')
        if v['verdict'] == 'agree' and v['verifier'] == record.author:
            errs.append(f'verify[{i}]: the author cannot verify their own binding (PROTOCOL.md §5)')
    errs += proof_errors(record, knowledge.builds, knowledge.build)
    return errs


def referenced_in_code(name, root):
    """In-repo code that already names the binding would start using it the moment it is exported."""
    pat = re.compile(rb'(?<![A-Za-z0-9_])' + re.escape(name.encode()) + rb'(?![A-Za-z0-9_])')
    for d in CODE_DIRS:
        for dirpath, dirnames, filenames in os.walk(os.path.join(root, d)):
            dirnames[:] = [x for x in dirnames if x not in ('node_modules', 'target', 'dist', 'build', '.git')]
            for f in filenames:
                if f.endswith(CODE_EXTS):
                    p = os.path.join(dirpath, f)
                    try:
                        if os.path.getsize(p) <= 4 << 20 and pat.search(open(p, 'rb').read()):
                            return os.path.relpath(p, root)
                    except OSError:
                        continue
    return None


def is_append(old, new):
    return isinstance(new, list) and len(new) >= len(old) and new[:len(old)] == old


def classify_record(relpath, old_bytes, new_bytes, author, knowledge, repo_root):
    """-> (decision, errors, notes) for one changed path. old_bytes/new_bytes are None for an absent side."""
    if not br.RECORD_PATH_RE.match(relpath):
        return 'review', [], [f'{relpath} is not a binding record']
    if new_bytes is None:
        return 'review', [], [f'{relpath} removes a record']
    try:
        new = br.Record.from_bytes(new_bytes, relpath[len('re/bindings/'):])
    except br.RecordError as e:
        return 'fail', [f'{relpath}: {e}'], []
    if old_bytes is None:
        errs = [f'{relpath}: {e}' for e in check_record(new, knowledge, relpath)]
        if errs:
            return 'fail', errs, []
        notes = []
        if new.author != author:
            notes.append(f'{relpath}: author {new.author} is not the pull request author {author}')
        if new.table_row()['level'] not in AUTO_LEVELS:
            notes.append(f'{relpath}: level {new.table_row()["level"]} needs review (new records land at L1 or L2)')
        if new.verify:
            notes.append(f'{relpath}: a new record carries no verify rows; a second contributor adds the agree row')
        if not any(p['by'] == author and p['exe']['sha256'] in knowledge.builds for p in new.proofs):
            notes.append(f'{relpath}: needs a proof by {author} from binding.py verify')
        used = referenced_in_code(new.name, repo_root)
        if used:
            notes.append(f'{relpath}: {new.name} is already named in {used}, so exporting it changes shipped code')
        return ('review' if notes else 'add'), [], notes
    try:
        old = br.Record.from_bytes(old_bytes, relpath[len('re/bindings/'):])
    except br.RecordError:
        return 'review', [], [f'{relpath}: the base copy does not parse']
    errs = [f'{relpath}: {e}' for e in check_record(new, knowledge.without(old), relpath)]
    if errs:
        return 'fail', errs, []
    same = {k: v for k, v in old.doc.items() if k not in ('verify', 'proofs')} == \
        {k: v for k, v in new.doc.items() if k not in ('verify', 'proofs')}
    if not same or not is_append(old.verify, new.verify) or not is_append(old.proofs, new.proofs):
        return 'review', [], [f'{relpath}: changes an existing record']
    added_v, added_p = new.verify[len(old.verify):], new.proofs[len(old.proofs):]
    notes = []
    if len(added_v) != 1 or len(added_p) != 1:
        notes.append(f'{relpath}: a second proof adds exactly one verify row and one proof')
    else:
        v, p = added_v[0], added_p[0]
        earlier = {x['verifier'] for x in old.verify} | {x['by'] for x in old.proofs}
        if v['verdict'] != 'agree':
            notes.append(f'{relpath}: verdict {v["verdict"]} needs review')
        if not (v['verifier'] == p['by'] == author):
            notes.append(f'{relpath}: the verify row and proof must both be by the pull request author {author}')
        if author == new.author or author in earlier:
            notes.append(f'{relpath}: {author} already vouched for this binding; the second proof needs someone else')
        if p['exe']['sha256'] not in knowledge.builds:
            notes.append(f'{relpath}: the proof is not on a known build')
        if {k for k, _ in kb.tokens(v['evidence'])} < {'decomp', 'xref'}:
            notes.append(f'{relpath}: an agree row needs decomp: and xref: evidence of its own')
        used = referenced_in_code(new.name, repo_root)
        if used:
            notes.append(f'{relpath}: {new.name} is already named in {used}, so exporting it changes shipped code')
    return ('review' if notes else 'proof'), [], notes


def classify(changes, author, knowledge, repo_root):
    """changes: [(relpath, old_bytes or None, new_bytes or None)] -> {'verdict', 'records', 'errors', 'notes'}.

    verdict is auto (every change is an add or a proof), review (a human looks) or fail (a check failed)."""
    out = {'verdict': 'auto', 'records': [], 'errors': [], 'notes': []}
    if not changes:
        out['verdict'] = 'review'
        out['notes'].append('the pull request changes no binding record')
    if len(changes) > MAX_AUTO_RECORDS:
        out['notes'].append(f'more than {MAX_AUTO_RECORDS} files: split it or ask for review')
    seen = knowledge
    for relpath, old, new in changes:
        decision, errs, notes = classify_record(relpath, old, new, author, seen, repo_root)
        out['records'].append({'path': relpath, 'decision': decision})
        out['errors'] += errs
        out['notes'] += notes
        if decision == 'add' and not errs:
            seen = seen_with(seen, br.Record.from_bytes(new, relpath[len('re/bindings/'):]))
    decisions = {r['decision'] for r in out['records']}
    if out['errors'] or 'fail' in decisions:
        out['verdict'] = 'fail'
    elif 'review' in decisions or out['notes']:
        out['verdict'] = 'review'
    return out


def seen_with(knowledge, record):
    k = Knowledge.__new__(Knowledge)
    k.__dict__ = dict(knowledge.__dict__)
    row, sig = record.table_row(), record.signature
    k.names = {**knowledge.names, record.name: (record.table, record.relpath)}
    k.addrs = {t: dict(m) for t, m in knowledge.addrs.items()}
    k.addrs[record.table][row['addr']] = (record.name, record.relpath)
    k.signatures = {**knowledge.signatures, sig['id']: sig}
    k.patterns = {**knowledge.patterns, (sig['section'], sig['pattern']): sig['id']}
    k.targets = {**knowledge.targets, (sig['kind'], sig['target']): sig['id']}
    return k
