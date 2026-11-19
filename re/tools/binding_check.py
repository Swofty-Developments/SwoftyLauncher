#!/usr/bin/env python3
"""Static checks for binding records: no game files needed, safe on hostile input (PROTOCOL.md §11).

check_record() checks one record against the knowledge base it would join. classify() compares the records a pull
request changes with the base branch and decides whether the change may merge without review:

  add      a new record: level L1 or L2, no verify rows, one proof by its author on a known build
  proof    an existing record that only gains a proof and an agree row from someone who is not its author
  review   anything else
"""
import json, os, re, sys
from collections import Counter
from collections.abc import Mapping

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
NO_RECORDS = 'the pull request changes no binding record'
CODE_DIRS = ('native/src', 'addons', 'packages', 'templates', 'apps')
CODE_EXTS = ('.c', '.cc', '.cpp', '.h', '.hpp', '.inc', '.rs', '.py', '.json', '.toml', '.ts', '.js', '.lua')

NEW = 'make the record again with binding.py new, which writes every signature field and the proof'
VERIFY = 'prove it again: re/tools/py re/tools/binding.py verify {name} --exe <your F1_25.exe> --by <your GitHub login>'
FIXES = [(re.compile(p), t) for p, t in (
    (r'whole E9 rel32 at the site the core hooks',
     'put the game\'s bytes back first (bindverify.py --restore LOG, or the Steam exe) and wildcard the rel32'),
    (r'invalid JSON|not valid JSON|duplicate key|a record is a JSON object|the limit is|not UTF-8|nests too deeply|'
     r'not a regular file|unknown key|^\S+: \S+ missing$|'
     r'is not a signature field|is not a (functions|globals) column|must be a one-line string',
     'the file is not what binding.py writes; ' + NEW),
    (r'must live at (re/bindings/\S+)|file must be (re/bindings/\S+)', 'move the file to {0}'),
    (r'L[23] needs (decomp: and xref:|xref:|decomp:)',
     'add the missing {0} token to --evidence (tokens are kind:value joined by ;, for example '
     '"decomp:own disasm 0x142ea5a50-0x142ea5adf;xref:0x142ea5763"), or pass --level L1'),
    (r'unknown evidence token kind|empty evidence token',
     'evidence tokens are kind:value joined by ; and the kinds are ' + ', '.join(kb.TOKEN_KINDS)),
    (r'not in subsystems\.tsv|no subsystem has the prefix',
     'pass --subsystem with an id from re/db/subsystems.tsv, or start the name with that subsystem\'s prefix'),
    (r'name \S+ (is a tool default name|does not follow)|has invalid characters|name prefix \S+ !=|global name|'
     r'global prefix \S+ !=',
     'name it <Prefix>_<Verb>_<Noun> (a function) or g_, k_, vt_ or site_<Prefix>_<Noun> (a global), with the prefix '
     'of its subsystem in re/db/subsystems.tsv (re/PROTOCOL.md §3)'),
    (r'signature\.verified needs', 'binding.py new must run on the KB build or on a build in re/db/builds; ' + NEW),
    (r'signature\.made_on \S+ is not a build', 'make the record on the KB build or on a build in re/db/builds (the '
     'current Steam build is one); ' + NEW),
    (r'signature\.(id|section|kind|pattern|match|target|names|anchor|resolve|expect|func_start|made_on|twin)|'
     r'data1_reason', NEW),
    (r'row\.addr is outside', 'use the address on the KB build (Steam 1.0.141.7865), 0x14xxxxxxx or 0x15xxxxxxx'),
    (r'digest mismatch|found match \S+ target \S+(, the record says| on the build the record was made on)',
     'the signature changed after the proof; ' + VERIFY),
    (r'did not resolve to exactly one match', 'the pattern is not unique on that exe; make the record again with '
     'binding.py new --match-from <an earlier instruction>'),
    (r'which is not a build in re/db/builds|the proof is not on a known build',
     'prove it on the KB build or on a build in re/db/builds; ' + VERIFY),
    (r'exe size or timestamp differs', 'that exe is not an unmodified KB build; ' + VERIFY),
    (r'name (\S+) is already in the knowledge base', 'pick another name: {0} already exists'),
    (r'(0x[0-9a-f]+) is already (\S+) in', 'the knowledge base already names {0} as {1}: use that binding, or open a '
     'pull request for review if it is wrong'),
    (r'signature id (\S+) is already used', 'pass --id <subsystem>.<another_snake_case_name>'),
    (r'are the same as (\S+)|already has the \S+ signature (\S+)',
     'the knowledge base already finds this place through {0}: use that binding'),
    (r'already has rows about', 'an earlier verdict exists for this name or address: pick another name, or ask for '
     'review'),
    (r'cannot verify their own|already vouched', 'the agree row must come from a second GitHub account'),
    (r'must be about (func|global) (\S+) or sig', 'a record\'s verify rows are about its own name, address or '
     'signature id; ' + NEW),
    (r'author (\S+) is not the pull request author (\S+)',
     'open the pull request from {0}, or set "author" to {1} and prove it as {1}: re/tools/py re/tools/binding.py '
     'verify {name} --exe <your F1_25.exe> --by {1}'),
    (r'needs a proof by (\S+) from', 're/tools/py re/tools/binding.py verify {name} --exe <your F1_25.exe> --by {0}'),
    (r'a new record carries no verify rows', 'remove the verify rows; a second contributor adds the agree row later '
     'with binding.py agree'),
    (r'needs review \(new records land', 'make it with --level L2 or L1; only a maintainer sets L3'),
    (r'already named in (\S+), so exporting',
     'nothing to fix: {0} already uses the name, so a maintainer reviews the pull request'),
    (r'changes an existing record|removes a record|base copy does not parse',
     'nothing to fix: changing or removing a merged record gets a review; only adding one agree row and one proof '
     'merges itself'),
    (r'exactly one verify row and one proof', 'run binding.py agree once: one pull request adds one agree row and its '
     'proof'),
    (r'verdict \S+ needs review', 'nothing to fix: only an agree row merges itself'),
    (r'must both be by the pull request author', 'run binding.py agree with --by set to the GitHub account that opens '
     'the pull request'),
    (r'needs decomp: and xref: evidence of its own',
     'pass binding.py agree --evidence "decomp:...;xref:..." from your own reading of the code'),
    (r'outside re/bindings|is not a binding record$', 'binding records are re/bindings/<subsystem>/<Name>.json; move '
     'every other change to a separate pull request'),
    (r'change what bindgen writes to', 'run python3 re/tools/bindgen.py --no-exe --json <scratch>/bindings.json '
     '--symbols <scratch>/symbols.tsv and commit native/include/swofty with the records; a maintainer merges it, '
     'and main\'s tests fail without them'),
    (r'changes the generated headers', 'nothing to fix when binding.py doctor no longer asks for them: a maintainer '
     'checks they are what bindgen writes and merges it'),
    (r'stops exporting', 'an exported binding is part of the addon API: correct the notes and add an agree row in '
     'the same pull request; only the repository owner takes one out, with an entry in re/db/export_drops.json'),
    (r'export_drops.json: the entry for \S+ no longer holds', 'remove the entry in the same pull request, or point '
     'covers at the newest partial or disagree row; the repository owner approves either'),
    (r'export_drops.json: \S+ ships in', 'set since to the next release'),
    (r'export_drops.json: drops \S+ without a released deprecation', 'set mode deprecated for the next release, or '
     'set harm when the binding crashes or corrupts state'),
    (r'which core names, without a replacement', 'add the binding that replaces it first and set replacement, then '
     'move native/src to it in the pull request that deprecates this one'),
    (r'which core names, and the pull request leaves', 'move the code under native/src off the binding in the same '
     'pull request; python3 re/tools/binding.py drops lists what it still names'),
    (r'the notes count callers by hand .*: cite callers: census (\S+) <rva>', 'take the counts from python3 '
     're/tools/census.py show {0} <address>, write callers: census {0} <rva> in their place and store the row with '
     'python3 re/tools/census.py update {0}'),
    (r'only the repository owner approves', 'nothing to fix: the bindings check passes once the repository owner '
     'approves the pull request at its head commit, after a re-run of the check'),
    (r'has no regular file there', 'commit the record as a plain file, not a symlink or a submodule'),
    (r'more than \d+ files', 'split the records over several pull requests'),
    (r'changes no binding record', 'add or change a file under re/bindings/<subsystem>/<Name>.json'),
    (r'kb_lint: ', 'python3 re/tools/kb_lint.py --structural prints the whole report'),
    (r'keyed by a retired name, so it binds nothing: (\S+) was renamed to (\S+)',
     'rename the entry\'s key from {0} to {1}; binding.py rename moved the entries that existed when it ran, so an '
     'entry a branch added after the rename is moved by hand'),
    (r'keyed by a retired name, so it binds nothing: (\S+) was renamed$',
     'rename the entry\'s key from {0} to the binding\'s current name, the record whose verify rows hold '
     'retired:{0}'),
)]


def fix_for(message):
    """What to run or which field to change for one check message, or None."""
    m = re.match(r'^(?:bindings/|re/bindings/)?(?:[a-z0-9_]+/)?([A-Za-z][A-Za-z0-9_]*)\.json: ', message)
    name = m.group(1) if m else '<Name>'
    for pat, text in FIXES:
        f = pat.search(message)
        if f:
            return text.format(*[g for g in f.groups() if g is not None], name=name)
    return None


def with_fixes(messages):
    out = []
    for msg in messages:
        out.append(msg)
        fix = fix_for(msg)
        if fix:
            out.append(f'  fix: {fix}')
    return out


def binding_kind(table, row, sig):
    if table == 'functions':
        return 'func'
    if row['type'] == 'code':
        return 'call' if (sig.get('expect') or '').upper().startswith('E8') else 'site'
    return 'vtable' if row['type'].startswith('vtable') else 'data'


WANT_SIG_KIND = {'func': ('func_start', 'call'), 'site': ('mid',), 'call': ('mid',), 'vtable': ('data',),
                 'data': ('data',)}


class Without(Mapping):
    """A read-only view of a dict without the entries drop(key, value) names."""

    def __init__(self, base, drop):
        self.base, self.drop = base, drop

    def __getitem__(self, key):
        v = self.base[key]
        if self.drop(key, v):
            raise KeyError(key)
        return v

    def __contains__(self, key):
        return key in self.base and not self.drop(key, self.base[key])

    def __iter__(self):
        return (key for key, v in self.base.items() if not self.drop(key, v))

    def __len__(self):
        return sum(1 for _ in self)


class Knowledge:
    """The base branch's knowledge base: names, addresses, signatures, builds and the subsystem table."""

    def __init__(self):
        self.subsystems = kb.subsystems()
        sigdoc = kb.load_signatures()
        self.build = sigdoc['build']
        self.signatures = {s['id']: s for s in sigdoc['signatures']}
        self.patterns = {pattern_key(s): s['id'] for s in sigdoc['signatures']}
        self.targets = {}
        for s in sigdoc['signatures']:
            self.targets.setdefault((s['kind'], str(s['target']).lower()), s['id'])
        self.names, self.addrs = {}, {'functions': {}, 'globals': {}}
        for t in ('functions', 'globals'):
            for r in kb.read_table(t)[0]:
                self.names[r['name']] = (t, r.get('_record'))
                self.addrs[t][r['addr']] = (r['name'], r.get('_record'))
        self.verified = {}
        for v in kb.read_table('verify')[0]:
            self.verified.setdefault((v['kind'], v['item']), v.get('_record'))
        self.builds = {self.build['sha256']: 'KB build'}
        bdir = os.path.join(kb.DB, 'builds')
        for f in sorted(os.listdir(bdir)) if os.path.isdir(bdir) else ():
            if f.endswith('.json'):
                with open(os.path.join(bdir, f)) as fh:
                    doc = json.load(fh)
                sha = (doc.get('build') or {}).get('sha256')
                if sha:
                    self.builds.setdefault(sha, f[:-5])

    @classmethod
    def alone(cls):
        """Only the subsystems and builds: checks a record's own rules, with no knowledge-base conflicts."""
        k = cls.__new__(cls)
        k.subsystems = kb.subsystems()
        with open(os.path.join(kb.DB, 'signatures.json')) as f:
            k.build = json.load(f)['build']
        k.signatures, k.patterns, k.targets, k.names, k.verified = {}, {}, {}, {}, {}
        k.addrs = {'functions': {}, 'globals': {}}
        k.builds = {k.build['sha256']: 'KB build'}
        return k

    def without(self, record):
        """Forget the record's own entries, so a record is never its own conflict. The views cost nothing to make."""
        if record is None:
            return self
        k = Knowledge.__new__(Knowledge)
        k.__dict__ = dict(self.__dict__)
        mine = record.relpath
        sid = record.signature['id'] if isinstance(record.signature, dict) else None
        k.names = Without(self.names, lambda key, v: v[1] == mine)
        k.verified = Without(self.verified, lambda key, v: v == mine)
        k.addrs = {t: Without(m, lambda key, v: v[1] == mine) for t, m in self.addrs.items()}
        k.signatures = Without(self.signatures, lambda key, v: key == sid)
        k.patterns = Without(self.patterns, lambda key, v: v == sid)
        k.targets = Without(self.targets, lambda key, v: v == sid)
        return k


MAX_ANCHOR = 0x100
MAX_EXPECT = 16
OVERLAY_PARTS = ('overrides', 'hints', 'unresolvable')


def retired_names(rows, current, drops):
    """{old name: its new name, or None when no record or drops entry names it} for every retired:<old> verify row
    whose old name no binding holds now."""
    out = {}
    for v in rows:
        old = v['item'][len('retired:'):] if v['item'].startswith('retired:') else None
        if not old or old in current:
            continue
        rec = v.get('_record')
        new = os.path.basename(rec)[:-len('.json')] if rec else (drops.get((v['kind'], old)) or {}).get('replacement')
        if new or old not in out:
            out[old] = new
    return out


def overlay_name_errors(retired, bdir):
    """Overlay entries still keyed by a retired name. The core looks an entry up by the binding's current name, so
    an entry under the old one binds nothing; a branch that adds one after binding.py rename moved the rest leaves
    it behind."""
    errs = []
    for f in sorted(os.listdir(bdir)) if os.path.isdir(bdir) else ():
        if not f.endswith('.json'):
            continue
        with open(os.path.join(bdir, f), encoding='utf-8') as fh:
            doc = json.load(fh)
        for part in OVERLAY_PARTS:
            for key in sorted(k for k in (doc.get(part) or {}) if k in retired):
                new = retired[key]
                errs.append(f're/db/builds/{f}: {part} entry {key} is keyed by a retired name, so it binds nothing: '
                            + (f'{key} was renamed to {new}' if new else f'{key} was renamed'))
    return errs


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


def pattern_key(sig):
    """Two signatures of one kind that match the same bytes and resolve the same way find the same address. The core
    scans once per section and pattern and applies each binding's anchor, resolve and kind to that one match, so the
    key compares parsed bytes (a wildcard is one token however many '?' spell it) and numeric offsets."""
    def num(v):
        try:
            return br.number(v)
        except (ValueError, TypeError):
            return v
    try:
        pattern = tuple(parse_pattern(sig['pattern']))
    except ValueError:
        pattern = ' '.join(sig['pattern'].upper().split())
    resolve = tuple(sorted((k, v if k == 'kind' else num(v)) for k, v in sig['resolve'].items())) \
        if isinstance(sig['resolve'], dict) else ()
    return sig['section'], pattern, num(sig['anchor']), resolve, sig['kind']


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
    if not 0 <= anchor < MAX_ANCHOR:
        errs.append(f'signature.anchor must be inside the first {MAX_ANCHOR} bytes after the match')
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
    if sig['kind'] == 'mid' and not errs and anchor + 5 <= len(pat) and pat[anchor] == 0xE9 \
            and None not in pat[anchor + 1:anchor + 5]:
        errs.append('signature.pattern fixes a whole E9 rel32 at the site the core hooks; that is the core\'s own '
                    'patch, so the pattern was read from a dump with the core running')
    if sig.get('expect') is not None and not errs:
        try:
            want = parse_pattern(sig['expect'])
        except ValueError as e:
            want = None
            errs.append(f'signature.expect {e}')
        if want is not None and (None in want or not 0 < len(want) <= MAX_EXPECT):
            errs.append(f'signature.expect must be 1 to {MAX_EXPECT} concrete bytes')
        elif want:
            for i, b in enumerate(want):
                p = pat[anchor + i] if anchor + i < len(pat) else None
                if p is not None and p != b:
                    errs.append(f'signature.expect byte {i} contradicts the pattern')
                    break
    for k in ('func_start', 'made_on_match', 'made_on_target'):
        if sig.get(k) is not None and not kb.ADDR_RE.match(sig[k]):
            errs.append(f'signature.{k} must be an address')
    errs += made_on_errors(sig)
    on = [kb_build] + ([sig['made_on']] if sig.get('made_on') else [])
    if not any(isinstance(v, dict) and len(v.get('build', '')) >= 16 and any(b.startswith(v['build']) for b in on)
               for v in sig['verified']):
        errs.append(f'signature.verified needs an entry for the KB build {kb_build[:16]}' +
                    (f' or for {sig["made_on"]}, the build it was made on' if sig.get('made_on') else ''))
    return errs


MADE_ON = ('made_on', 'made_on_match', 'made_on_target', 'twin')


def made_on_errors(sig):
    """A record made on another build than the KB build says which build, where it matched there and how its KB
    addresses were found."""
    have = [k for k in MADE_ON if sig.get(k) is not None]
    if not have:
        return []
    if len(have) != len(MADE_ON):
        return [f'signature.{k} missing: a record made on another build needs made_on, made_on_match, '
                f'made_on_target and twin' for k in MADE_ON if k not in have]
    errs = []
    if not re.match(r'^[0-9a-f]{16,64}$', sig['made_on']):
        errs.append('signature.made_on must be the sha256 (or its first 16 hex digits) of the build it was made on')
    if not sig['twin'].strip():
        errs.append('signature.twin must say how the KB addresses were found')
    try:
        anchor = br.number(sig['anchor'])
    except (ValueError, TypeError):
        return errs
    if sig['resolve'].get('kind') == 'addr' and kb.ADDR_RE.match(sig['made_on_match']) and \
            kb.ADDR_RE.match(sig['made_on_target']) and \
            int(sig['made_on_match'], 16) + anchor != int(sig['made_on_target'], 16):
        errs.append('signature.made_on_target must equal made_on_match + anchor for resolve kind addr')
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
        if sig.get('made_on') and sha.startswith(sig['made_on']) and \
                (p['result']['match'], p['result']['target']) != (sig.get('made_on_match'), sig.get('made_on_target')):
            errs.append(f'{where} found match {p["result"]["match"]} target {p["result"]["target"]} on the build the '
                        f'record was made on, which says {sig.get("made_on_match")} {sig.get("made_on_target")}')
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
    if pattern_key(sig) in knowledge.patterns:
        errs.append(f'signature.pattern, anchor and resolve are the same as {knowledge.patterns[pattern_key(sig)]}')
    if (sig['kind'], sig['target']) in knowledge.targets:
        errs.append(f'{sig["target"]} already has the {sig["kind"]} signature {knowledge.targets[(sig["kind"], sig["target"])]}')
    vkind = 'func' if record.table == 'functions' else 'global'
    for key in ((vkind, record.name), (vkind, row['addr'])):
        if key in knowledge.verified:
            errs.append(f'verify.tsv already has rows about {key[0]} {key[1]}; a new record cannot inherit them')
    for i, v in enumerate(record.verify):
        errs += [f'verify[{i}]: {e}' for e in kb.validate_row('verify', v)]
        if (v['kind'], v['item']) not in ((vkind, record.name), (vkind, row['addr']), ('sig', sig['id'])) and not (
                v['kind'] == vkind and v['item'].startswith('retired:')):
            errs.append(f'verify[{i}] must be about {vkind} {record.name} or sig {sig["id"]}')
        if v['verdict'] == 'agree' and v['verifier'] == record.author:
            errs.append(f'verify[{i}]: the author cannot verify their own binding (PROTOCOL.md §5)')
    if sig.get('made_on'):
        on = [b for b in knowledge.builds if b.startswith(sig['made_on'])]
        if not on or on == [knowledge.build['sha256']]:
            errs.append(f'signature.made_on {sig["made_on"]} is not a build in re/db/builds other than the KB build')
    errs += proof_errors(record, knowledge.builds, knowledge.build)
    return errs


COMMENT_RE = re.compile(rb'"(?:\\.|[^"\\\n])*"|//[^\n]*|/\*.*?\*/', re.S)


def referenced_in_code(name, root, dirs=CODE_DIRS, comments=True):
    """In-repo code that already names the binding, or its header `_t` type, would start using it the moment it is
    exported. With comments=False a name inside a // or /* */ comment does not count; string contents still do."""
    pat = re.compile(rb'(?<![A-Za-z0-9_])' + re.escape(name.encode()) + rb'(?:_t)?(?![A-Za-z0-9_])')
    for d in dirs:
        for dirpath, dirnames, filenames in os.walk(os.path.join(root, d)):
            dirnames[:] = [x for x in dirnames if x not in ('node_modules', 'target', 'dist', 'build', '.git')]
            for f in filenames:
                if f.endswith(CODE_EXTS):
                    p = os.path.join(dirpath, f)
                    try:
                        if os.path.getsize(p) > 4 << 20:
                            continue
                        with open(p, 'rb') as fh:
                            data = fh.read()
                        if not comments:
                            data = COMMENT_RE.sub(lambda m: m.group() if m.group()[:1] == b'"' else b' ', data)
                        if pat.search(data):
                            return os.path.relpath(p, root)
                    except OSError:
                        continue
    return None


def is_append(old, new):
    return isinstance(new, list) and len(new) >= len(old) and new[:len(old)] == old


def hand_count_errors(relpath, old, new):
    """Once the KB build has a caller census, the notes a pull request adds or changes cite it for every caller count
    they add."""
    import census
    build = census.active()
    if not build:
        return []
    added = Counter(census.hand_counts(new.table_row()['notes']))
    if old is not None:
        added -= Counter(census.hand_counts(old.table_row()['notes']))
    if not added:
        return []
    return [f'{relpath}: the notes count callers by hand ({"; ".join(sorted(added))}): cite callers: census {build} '
            f'<rva> instead']


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
        errs += hand_count_errors(relpath, None, new)
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
    errs += hand_count_errors(relpath, old, new)
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
        out['notes'].append(NO_RECORDS)
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
    k.patterns = {**knowledge.patterns, pattern_key(sig): sig['id']}
    k.targets = {**knowledge.targets, (sig['kind'], sig['target']): sig['id']}
    return k
