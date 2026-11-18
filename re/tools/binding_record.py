#!/usr/bin/env python3
"""One binding per file: re/bindings/<subsystem>/<Name>.json (PROTOCOL.md §11).

A record holds everything one binding needs: its functions.tsv or globals.tsv row, its signature, the verify rows
about it and the local proofs its contributors ran against their own F1_25.exe. kb.py merges records into the tables
it reads, so bindgen, kb_lint and sigcheck see one knowledge base.

Records are read from pull requests, so the parser treats every file as hostile: a size cap, UTF-8 only, strict JSON
without NaN, Infinity or duplicate keys, exact key sets and bounded strings.
"""
import copy, hashlib, json, os, re, stat

HERE = os.path.dirname(os.path.abspath(__file__))
RE_DIR = os.path.dirname(HERE)
RECORDS = os.path.join(RE_DIR, 'bindings')
CACHE = os.path.join(RE_DIR, '.cache', 'records.json')
CACHE_SCHEMA = 2

SCHEMA = 1
MAX_BYTES = 64 * 1024
MAX_TEXT = 8000
MAX_LIST = 64
RECORD_PATH_RE = re.compile(r'^re/bindings/([a-z][a-z0-9_]{0,31})/([A-Za-z][A-Za-z0-9_]{1,63})\.json$')
LOGIN_RE = re.compile(r'^[A-Za-z0-9](?:[A-Za-z0-9._-]{0,38})$')
HEX_RE = re.compile(r'^0x[0-9a-f]+$')
SHA256_RE = re.compile(r'^[0-9a-f]{64}$')
DATE_RE = re.compile(r'^\d{4}-\d{2}-\d{2}$')

TABLE_COLUMNS = {
    'functions': ('addr', 'level', 'subsystem', 'proto', 'evidence', 'notes'),
    'globals': ('addr', 'type', 'level', 'subsystem', 'evidence', 'notes'),
}
VERIFY_COLUMNS = ('date', 'verifier', 'kind', 'item', 'claimed', 'verdict', 'evidence', 'notes')
TOP_KEYS = ('schema', 'name', 'table', 'author', 'row', 'signature', 'verify', 'proofs')
SIGNATURE_KEYS = ('id', 'section', 'pattern', 'match', 'anchor', 'resolve', 'target', 'kind', 'names', 'verified',
                  'used_by')
SIGNATURE_OPTIONAL = ('expect', 'func_start', 'data1_reason', 'made_on', 'made_on_match', 'made_on_target', 'twin')
RESOLUTION_KEYS = ('section', 'pattern', 'anchor', 'resolve', 'expect', 'kind', 'match', 'target')
PROOF_KEYS = ('by', 'date', 'tool', 'exe', 'result', 'sigcheck', 'digest')
PROOF_EXE_KEYS = ('sha256', 'size', 'pe_timestamp')
PROOF_RESULT_KEYS = ('result', 'method', 'matches', 'match', 'target')
PROOF_TOOL = 'binding.py verify'


class RecordError(ValueError):
    pass


def parse_json(data):
    """Strict JSON for untrusted bytes: capped size, UTF-8, no duplicate keys, no NaN or Infinity."""
    if not isinstance(data, (bytes, bytearray)):
        raise RecordError('expected bytes')
    if len(data) > MAX_BYTES:
        raise RecordError(f'file is {len(data)} bytes, the limit is {MAX_BYTES}')
    try:
        text = bytes(data).decode('utf-8')
    except UnicodeDecodeError as e:
        raise RecordError(f'not UTF-8 ({e.reason} at byte {e.start})') from None

    def pairs(items):
        out = {}
        for k, v in items:
            if k in out:
                raise RecordError(f'duplicate key {k!r}')
            out[k] = v
        return out

    def constant(name):
        raise RecordError(f'{name} is not valid JSON')

    try:
        return json.loads(text, object_pairs_hook=pairs, parse_constant=constant)
    except RecordError:
        raise
    except RecursionError:
        raise RecordError('JSON nests too deeply') from None
    except ValueError as e:
        raise RecordError(f'invalid JSON: {e}') from None


def is_text(v, limit=MAX_TEXT):
    return isinstance(v, str) and 0 < len(v) <= limit and '\t' not in v and '\n' not in v and '\r' not in v \
        and v.isprintable()


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def number(v):
    return int(str(v), 0)


def resolution_digest(signature, exe_sha256):
    """Binds a proof to the signature fields that decide where a binding lands and to the exe it ran on."""
    body = {k: signature.get(k) for k in RESOLUTION_KEYS}
    body['exe'] = exe_sha256
    return hashlib.sha256(json.dumps(body, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


class Record:
    def __init__(self, doc, relpath=None):
        self.doc = doc
        self.relpath = relpath

    @classmethod
    def from_bytes(cls, data, relpath=None):
        doc = parse_json(data)
        if not isinstance(doc, dict):
            raise RecordError('a record is a JSON object')
        return cls(doc, relpath)

    @classmethod
    def load(cls, path, relpath=None):
        st = os.lstat(path)
        if not os.path.isfile(path) or os.path.islink(path):
            raise RecordError('not a regular file')
        if st.st_size > MAX_BYTES:
            raise RecordError(f'file is {st.st_size} bytes, the limit is {MAX_BYTES}')
        with open(path, 'rb') as f:
            return cls.from_bytes(f.read(MAX_BYTES + 1), relpath)

    @classmethod
    def create(cls, name, table, author, row, signature):
        doc = {'schema': SCHEMA, 'name': name, 'table': table, 'author': author,
               'row': {c: row.get(c, '-') for c in TABLE_COLUMNS[table]}, 'signature': signature,
               'verify': [], 'proofs': []}
        return cls(doc, relpath_for(row['subsystem'], name))

    @property
    def name(self):
        return self.doc.get('name')

    @property
    def table(self):
        return self.doc.get('table')

    @property
    def author(self):
        return self.doc.get('author')

    @property
    def subsystem(self):
        return (self.doc.get('row') or {}).get('subsystem')

    @property
    def signature(self):
        return self.doc.get('signature')

    @property
    def verify(self):
        return self.doc.setdefault('verify', [])

    @property
    def proofs(self):
        return self.doc.setdefault('proofs', [])

    def table_row(self):
        row = {'name': self.name, 'sig': self.signature['id'], **self.doc['row']}
        return {c: row[c] for c in table_columns(self.table)}

    def set_table_row(self, row):
        for c in TABLE_COLUMNS[self.table]:
            self.doc['row'][c] = row[c]
        if row['sig'] != self.signature['id'] or row['name'] != self.name:
            raise RecordError(f'{self.relpath}: a record keeps its name and signature id')

    def shape_errors(self):
        """Everything checkable from the file alone; cross references are binding_check's job."""
        d, errs = self.doc, []
        unknown = sorted(set(d) - set(TOP_KEYS))
        errs += [f'unknown key {k!r}' for k in unknown]
        errs += [f'{k} missing' for k in TOP_KEYS if k not in d]
        if errs:
            return errs
        if d['schema'] != SCHEMA:
            errs.append(f'schema must be {SCHEMA}')
        if d['table'] not in TABLE_COLUMNS:
            return errs + ['table must be functions or globals']
        if not is_text(d['name'], 64):
            errs.append('name must be a short string')
        if not isinstance(d['author'], str) or not LOGIN_RE.match(d['author']):
            errs.append('author must be a GitHub login')
        errs += self._row_errors()
        errs += self._signature_errors()
        errs += self._verify_errors()
        errs += self._proof_errors()
        if not errs and self.relpath and self.relpath != relpath_for(self.subsystem, self.name):
            errs.append(f'file must be re/bindings/{relpath_for(self.subsystem, self.name)}')
        return errs

    def _row_errors(self):
        row, cols = self.doc['row'], TABLE_COLUMNS[self.doc['table']]
        if not isinstance(row, dict):
            return ['row must be an object']
        errs = [f'row.{k} is not a {self.doc["table"]} column' for k in sorted(set(row) - set(cols))]
        errs += [f'row.{c} must be a one-line string (use - for empty)' for c in cols if not is_text(row.get(c))]
        return errs

    def _signature_errors(self):
        s = self.doc['signature']
        if not isinstance(s, dict):
            return ['signature must be an object']
        errs = [f'signature.{k} missing' for k in SIGNATURE_KEYS if k not in s]
        errs += [f'signature.{k} is not a signature field' for k in sorted(set(s) - set(SIGNATURE_KEYS) -
                                                                          set(SIGNATURE_OPTIONAL))]
        if errs:
            return errs
        for k in ('id', 'section', 'pattern', 'kind', 'match', 'target'):
            if not is_text(s[k], 1024 * 3):
                errs.append(f'signature.{k} must be a string')
        if not (is_int(s['anchor']) or is_text(s['anchor'], 16)):
            errs.append('signature.anchor must be a number or a 0x string')
        r = s['resolve']
        if not isinstance(r, dict) or not all(isinstance(k, str) for k in r) or len(r) > 3 or \
                not all(is_text(v, 16) or is_int(v) for v in r.values()):
            errs.append('signature.resolve must be a small object')
        for k in ('names', 'used_by'):
            if not isinstance(s[k], list) or len(s[k]) > MAX_LIST or not all(is_text(x, 256) for x in s[k]):
                errs.append(f'signature.{k} must be a list of strings')
        v = s['verified']
        if not isinstance(v, list) or not v or len(v) > MAX_LIST or not all(
                isinstance(e, dict) and set(e) <= {'build', 'date', 'by', 'how'} and
                all(is_text(x, 64) for x in e.values()) and {'build', 'date', 'by'} <= set(e) for e in v):
            errs.append('signature.verified must list {build, date, by[, how]} entries')
        for k in SIGNATURE_OPTIONAL:
            if k in s and s[k] is not None and not is_text(s[k], 1024):
                errs.append(f'signature.{k} must be a string')
        return errs

    def _verify_errors(self):
        v = self.doc['verify']
        if not isinstance(v, list) or len(v) > MAX_LIST:
            return ['verify must be a list']
        errs = []
        for i, e in enumerate(v):
            if not isinstance(e, dict) or set(e) != set(VERIFY_COLUMNS) or not all(is_text(e[c]) for c in VERIFY_COLUMNS):
                errs.append(f'verify[{i}] must hold exactly {", ".join(VERIFY_COLUMNS)} as one-line strings')
        return errs

    def _proof_errors(self):
        p = self.doc['proofs']
        if not isinstance(p, list) or len(p) > MAX_LIST:
            return ['proofs must be a list']
        errs = []
        for i, e in enumerate(p):
            where = f'proofs[{i}]'
            if not isinstance(e, dict) or set(e) != set(PROOF_KEYS):
                errs.append(f'{where} must hold exactly {", ".join(PROOF_KEYS)}')
                continue
            if not isinstance(e['by'], str) or not LOGIN_RE.match(e['by']):
                errs.append(f'{where}.by must be a GitHub login')
            if not isinstance(e['date'], str) or not DATE_RE.match(e['date']):
                errs.append(f'{where}.date must be YYYY-MM-DD')
            if e['tool'] != PROOF_TOOL:
                errs.append(f'{where}.tool must be {PROOF_TOOL!r}')
            x = e['exe']
            if not isinstance(x, dict) or set(x) != set(PROOF_EXE_KEYS) or not SHA256_RE.match(str(x.get('sha256'))) \
                    or not is_int(x.get('size')) or not is_int(x.get('pe_timestamp')):
                errs.append(f'{where}.exe must hold sha256, size and pe_timestamp')
            r = e['result']
            if not isinstance(r, dict) or set(r) != set(PROOF_RESULT_KEYS) or not is_int(r.get('matches')) or \
                    not all(is_text(r.get(k), 64) for k in ('result', 'method', 'match', 'target')):
                errs.append(f'{where}.result must hold result, method, matches, match and target')
            if not is_text(e['sigcheck'], 512):
                errs.append(f'{where}.sigcheck must be a string')
            if not isinstance(e['digest'], str) or not SHA256_RE.match(e['digest']):
                errs.append(f'{where}.digest must be a sha256')
        return errs

    def verify_rows(self):
        return [{c: e[c] for c in VERIFY_COLUMNS} for e in self.verify]

    def to_json(self):
        return json.dumps(self.doc, indent=1, ensure_ascii=False) + '\n'

    def save(self, root=RECORDS):
        path = os.path.join(root, self.relpath)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = f'{path}.{os.getpid()}.tmp'
        with open(tmp, 'w', encoding='utf-8') as f:
            f.write(self.to_json())
        os.replace(tmp, path)
        Store.forget()


def relpath_for(subsystem, name):
    return f'{subsystem}/{name}.json'


def table_columns(table):
    import kb
    return kb.TABLES[table][0]


def parser_digest():
    with open(os.path.abspath(__file__), 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def cache_path_for(root):
    """SWOFTY_RECORD_CACHE=0 turns the cache off and a path points every Store at that file; otherwise only the
    repository's own records use re/.cache/records.json."""
    env = os.environ.get('SWOFTY_RECORD_CACHE', '')
    if env == '0':
        return None
    if env not in ('', '1'):
        return env
    return CACHE if os.path.abspath(root) == os.path.abspath(RECORDS) else None


class RecordCache:
    """What the strict parser and the shape checks said about each record, kept between processes in a gitignored
    file. An entry is keyed by the record's path and the SHA-256 of its bytes and only answers for a file whose bytes
    hash the same now, so a changed record is always parsed again. A record that passed is re-read with the plain
    JSON parser, which gives the same object for bytes the strict one accepted. The parser's own source digest
    keys the whole file: a change to the rules drops every entry."""

    def __init__(self, path):
        self.path, self.code = path, parser_digest()
        self.old, self.new, self.dirty = self.read(), {}, False

    def read(self):
        try:
            with open(self.path, 'rb') as f:
                doc = json.loads(f.read())
        except (OSError, ValueError):
            return {}
        if not isinstance(doc, dict) or doc.get('schema') != CACHE_SCHEMA or doc.get('code') != self.code \
                or not isinstance(doc.get('entries'), dict):
            return {}
        return doc['entries']

    def lookup(self, rel, digest):
        e = self.old.get(rel)
        if not (isinstance(e, list) and len(e) == 2 and e[0] == digest and isinstance(e[1], list)
                and all(isinstance(x, str) for x in e[1])):
            return None
        self.new[rel] = e
        return e[1]

    def keep(self, rel, digest, errors):
        self.new[rel] = [digest, errors]
        self.dirty = True

    def save(self):
        if not self.dirty and self.new.keys() == self.old.keys():
            return
        body = json.dumps({'schema': CACHE_SCHEMA, 'code': self.code, 'entries': self.new}, separators=(',', ':'))
        tmp = f'{self.path}.{os.getpid()}'
        try:
            os.makedirs(os.path.dirname(os.path.abspath(self.path)), exist_ok=True)
            with open(tmp, 'w', encoding='utf-8') as f:
                f.write(body)
            os.replace(tmp, self.path)
        except OSError:
            try:
                os.unlink(tmp)
            except OSError:
                pass


class Store:
    """Every record under one root, loaded once per process."""
    _cache = {}

    def __init__(self, root=RECORDS, cache_path=None):
        self.root = root
        self.records, self.errors = [], []
        cache_path = cache_path or cache_path_for(root)
        cache = RecordCache(cache_path) if cache_path else None
        for dirpath, dirnames, filenames in os.walk(root) if os.path.isdir(root) else ():
            dirnames.sort()
            for f in sorted(filenames):
                path = os.path.join(dirpath, f)
                rel = os.path.relpath(path, root).replace(os.sep, '/')
                if f.endswith('.md'):
                    continue
                rec, errs = self.parse(path, rel, cache)
                if errs:
                    self.errors += [f'bindings/{rel}: {e}' for e in errs]
                else:
                    self.records.append(rec)
        if cache:
            cache.save()

    @staticmethod
    def parse(path, rel, cache):
        try:
            st = os.lstat(path) if cache else None
            if not cache or not stat.S_ISREG(st.st_mode) or st.st_size > MAX_BYTES:
                rec = Record.load(path, rel)
                return rec, rec.shape_errors()
            with open(path, 'rb') as f:
                data = f.read(MAX_BYTES + 1)
        except (RecordError, OSError) as e:
            return None, [str(e)]
        digest = hashlib.sha256(data).hexdigest()
        known = cache.lookup(rel, digest)
        if known is not None:
            return (None, list(known)) if known else (Record(json.loads(data), rel), [])
        try:
            rec = Record.from_bytes(data, rel)
            errs = rec.shape_errors()
        except RecordError as e:
            rec, errs = None, [str(e)]
        cache.keep(rel, digest, errs)
        return rec, errs

    @classmethod
    def get(cls, root=RECORDS):
        if root not in cls._cache:
            cls._cache[root] = cls(root)
        return cls._cache[root]

    @classmethod
    def forget(cls):
        cls._cache.clear()

    def overlay(self, records):
        """Add or replace records in this process only, to lint a pull request's records with the base branch's."""
        mine = {r.relpath for r in records}
        self.records = sorted([r for r in self.records if r.relpath not in mine] + list(records), key=lambda r: r.relpath)

    def table_rows(self, table):
        if table == 'verify':
            rows = [{**v, '_line': f'bindings/{r.relpath}', '_record': r.relpath} for r in self.records
                    for v in r.verify_rows()]
        else:
            rows = [{**r.table_row(), '_line': f'bindings/{r.relpath}', '_record': r.relpath} for r in self.records
                    if r.table == table]
        return rows, list(self.errors)

    def signatures(self):
        return [copy.deepcopy(r.signature) for r in self.records]

    def by_relpath(self, relpath):
        return next((r for r in self.records if r.relpath == relpath), None)

    def by_item(self, item):
        return next((r for r in self.records if item in (r.name, r.doc['row']['addr'], r.signature['id'])), None)
