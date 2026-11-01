#!/usr/bin/env python3
"""Check packages/core/src/dbedit/modules.json against a game database.

Resolves every module's scopes, rows and field paths, prints per module the rows found,
the fields missing per row and a sample row rendered with names. Exits 1 on a schema
error or on a missing non-optional field.
"""
import argparse
import gzip
import importlib.util
import json
import os
import re
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
DEFAULT_MODULES = os.path.join(ROOT, "packages", "core", "src", "dbedit", "modules.json")

_spec = importlib.util.spec_from_file_location("gamedb_rw", os.path.join(HERE, "..", "gamedb.py"))
gamedb = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gamedb)

KINDS = {"int", "float", "text", "bool", "choice", "ref", "ref-list", "list", "color", "curve", "time", "vector"}
GROUPS = ["Drivers", "Teams & Cars", "Tracks & Calendar", "Rules & Modes"]

SCHEMA = {
    "module": ({"id", "group", "title", "icon", "summary", "rows", "columns", "fields"},
               {"scope", "filter", "label", "name", "links", "notes"}),
    "scope": ({"title", "rows"}, {"label", "default", "name", "filter"}),
    "filter": ({"field"}, {"any", "not", "prefix"}),
    "name": (set(), {"field", "template", "lng"}),
    "field": ({"id", "label", "path"},
              {"section", "kind", "min", "max", "step", "unit", "options", "refs", "help", "lng", "readonly", "optional"}),
    "option": ({"value", "label"}, set()),
    "refs": ({"rows"}, {"label", "name", "filter"}),
    "link": ({"label", "text"}, {"addon", "section", "tab", "url"}),
}


def fnv_from(h, s):
    for c in s.encode("utf-8"):
        h = ((h ^ c) * gamedb.FNV_PRIME) & gamedb.U64
    return h


class ResolveError(Exception):
    pass


class Base:
    """A path under construction: its running FNV-1a state and a readable spelling."""

    __slots__ = ("h", "text")

    def __init__(self, h, text):
        self.h = h
        self.text = text

    @staticmethod
    def parse(s):
        m = re.fullmatch(r"\{([0-9a-fA-F]{16})\}(.*)", s)
        if m:
            return Base(fnv_from(int(m.group(1), 16), m.group(2)), s)
        return Base(fnv_from(gamedb.FNV_BASIS, s), s)

    def child(self, name):
        return Base(fnv_from(self.h, "." + name), self.text + "." + name)

    def row(self, word, i):
        return self.child("%s_%d" % (word, i))


def segments(path):
    out = []
    rest = path
    while rest:
        end = len(rest)
        for i, c in enumerate(rest):
            if c in ".[":
                end = i
                break
        name, rest = rest[:end], rest[end:]
        select = None
        if rest.startswith("["):
            close = None
            for i in range(1, len(rest)):
                if rest[i] == "]" and (i + 1 == len(rest) or rest[i + 1] == "."):
                    close = i
                    break
            if close is None:
                raise ResolveError("%s: [ needs a ] at the end of its segment" % path)
            sel = rest[1:close]
            if "=" not in sel:
                raise ResolveError("%s: bad selector [%s]" % (path, sel))
            field, want = sel.split("=", 1)
            select = (field.strip(), want)
            rest = rest[close + 1:]
        if not name:
            raise ResolveError("%s: empty segment" % path)
        out.append((name, select))
        if rest.startswith("."):
            rest = rest[1:]
    return out


class Db:
    def __init__(self, path):
        data = open(path, "rb").read()
        self.records = {}
        for r in gamedb.read(data):
            try:
                v = r.value.decode("utf-8")
            except UnicodeDecodeError:
                v = r.value.decode("latin-1")
            self.records[r.key] = (r.type, v)

    def get(self, base):
        return self.records.get(base.h)

    def text(self, base):
        rec = self.records.get(base.h)
        return None if rec is None else rec[1]

    def count(self, table, word):
        v = self.text(table.child("_%s_instancecount_" % word))
        try:
            return int(v.strip()) if v is not None else None
        except ValueError:
            return None

    def rows(self, table, word):
        n = self.count(table, word)
        if n is not None:
            return [table.row(word, i) for i in range(n)]
        out = []
        for i in range(4096):
            r = table.row(word, i)
            if self.text(r.child("_metadata_")) is None:
                break
            out.append(r)
        return out

    def select(self, table, word, field, want):
        hits = []
        for r in self.rows(table, word):
            if self.text(r.child("_metadata_")) is None:
                continue
            if self.text(self.under(r, field)) == want:
                hits.append(r)
        if len(hits) != 1:
            raise ResolveError("%s.%s[%s=%s]: %s" % (table.text, word, field, want,
                                                    "no row matches" if not hits else "%d rows match" % len(hits)))
        return hits[0]

    def under(self, base, rel):
        """`rel` (dotted, may carry selectors) below `base`; `base` None makes it absolute."""
        for name, select in segments(rel):
            if select is None:
                base = Base.parse(name) if base is None else base.child(name)
            elif base is None:
                raise ResolveError("%s: a selector needs a table path before it" % rel)
            else:
                base = self.select(base, name, select[0], select[1])
        return base

    def table(self, rows_sel):
        """Split `container.word` (selector form) into the container base and the row word."""
        cut = rows_sel.rfind(".")
        if cut < 0 or "[" in rows_sel[cut:]:
            raise ResolveError("%s: rows must end in .<word>" % rows_sel)
        return self.under(None, rows_sel[:cut]), rows_sel[cut + 1:]


def load_lng(path):
    data = open(path, "rb").read()
    if data[:4] != b"LNGT":
        raise SystemExit("%s is not a language file" % path)
    be = lambda off: struct.unpack_from(">I", data, off)[0]
    pos = 8
    chunks = {}
    for tag in ("HSHS", "HSHT", "SIDA", "SIDB", "LNGB"):
        if data[pos:pos + 4] != tag.encode():
            raise SystemExit("%s: expected %s at %d" % (path, tag, pos))
        size = be(pos + 4)
        pos += 8
        if tag == "SIDA":
            pos += 4
        chunks[tag] = data[pos:pos + size]
        pos += size
    sida, sidb, lngb = chunks["SIDA"], chunks["SIDB"], chunks["LNGB"]

    def cstr(blob, off):
        end = blob.index(b"\0", off)
        return blob[off:end].decode("utf-8", "replace")

    out = {}
    for i in range(len(sida) // 8):
        k, v = struct.unpack_from(">II", sida, i * 8)
        out[cstr(sidb, k)] = cstr(lngb, v)
    return out


def check_schema(doc):
    errs = []

    def obj(o, kind, where):
        if not isinstance(o, dict):
            errs.append("%s: expected an object" % where)
            return False
        req, opt = SCHEMA[kind]
        for k in req - o.keys():
            errs.append("%s: missing %s" % (where, k))
        for k in o.keys() - req - opt:
            errs.append("%s: unknown field %s" % (where, k))
        return True

    def filters(fs, where):
        for i, f in enumerate(fs or []):
            obj(f, "filter", "%s.filter[%d]" % (where, i))

    def name(n, where):
        if n is not None and obj(n, "name", where + ".name"):
            if not n.get("field") and not n.get("template"):
                errs.append("%s.name: needs field or template" % where)

    if set(doc.keys()) != {"modules"}:
        errs.append("top level must be {\"modules\": [...]}")
        return errs
    ids = set()
    for m in doc["modules"]:
        where = "module %s" % m.get("id", "?")
        if not obj(m, "module", where):
            continue
        if m["id"] in ids:
            errs.append("%s: duplicate id" % where)
        ids.add(m["id"])
        if m["group"] not in GROUPS:
            errs.append("%s: unknown group %s" % (where, m["group"]))
        if m.get("scope") is not None and obj(m["scope"], "scope", where + ".scope"):
            filters(m["scope"].get("filter"), where + ".scope")
            name(m["scope"].get("name"), where + ".scope")
        filters(m.get("filter"), where)
        name(m.get("name"), where)
        fids = set()
        for f in m["fields"]:
            fw = "%s.%s" % (where, f.get("id", "?"))
            if not obj(f, "field", fw):
                continue
            if f["id"] in fids:
                errs.append("%s: duplicate field id" % fw)
            fids.add(f["id"])
            kind = f.get("kind")
            if kind is not None and kind not in KINDS:
                errs.append("%s: unknown kind %s" % (fw, kind))
            for i, o in enumerate(f.get("options", [])):
                obj(o, "option", "%s.options[%d]" % (fw, i))
            if kind == "choice" and not f.get("options"):
                errs.append("%s: choice without options" % fw)
            if kind in ("ref", "ref-list"):
                if f.get("refs") is None:
                    errs.append("%s: %s without refs" % (fw, kind))
            if f.get("refs") is not None and obj(f["refs"], "refs", fw + ".refs"):
                filters(f["refs"].get("filter"), fw + ".refs")
                name(f["refs"].get("name"), fw + ".refs")
            for k in ("min", "max", "step"):
                if k in f and not isinstance(f[k], (int, float)):
                    errs.append("%s: %s must be a number" % (fw, k))
        for c in m["columns"]:
            if c not in fids:
                errs.append("%s: column %s is not a field" % (where, c))
        for i, l in enumerate(m.get("links", [])):
            obj(l, "link", "%s.links[%d]" % (where, i))
    return errs


class Checker:
    def __init__(self, db, lang):
        self.db = db
        self.lang = lang
        self.ref_cache = {}

    def english(self, v):
        return self.lang.get(v) if self.lang and v else None

    def keep(self, row, filters):
        for f in filters or []:
            try:
                v = self.db.text(self.db.under(row, f["field"]))
            except ResolveError:
                v = None
            if f.get("any") and v not in f["any"]:
                return False
            if v is not None and v in f.get("not", []):
                return False
            if f.get("prefix") is not None and not (v or "").startswith(f["prefix"]):
                return False
        return True

    def label(self, row, field):
        try:
            return self.db.text(self.db.under(row, field))
        except ResolveError:
            return None

    def name(self, row, spec, label):
        if not spec:
            return label or ""
        if spec.get("template"):
            def sub(m):
                v = self.label(row, m.group(1)) or ""
                return (self.english(v) or v) if spec.get("lng") else v
            text = re.sub(r"\{([^}]+)\}", sub, spec["template"]).strip()
            return text or label or ""
        v = self.label(row, spec["field"]) or ""
        if spec.get("lng"):
            v = self.english(v) or v
        return v or label or ""

    def table_rows(self, rows_sel, filters, scope=None):
        if scope is not None:
            rows_sel = rows_sel.replace("{scope}", scope)
        table, word = self.db.table(rows_sel)
        return [r for r in self.db.rows(table, word) if self.keep(r, filters)]

    def refs(self, spec, scope):
        key = (json.dumps(spec, sort_keys=True), scope)
        if key not in self.ref_cache:
            out = {}
            try:
                for r in self.table_rows(spec["rows"], spec.get("filter"), scope):
                    lab = self.label(r, spec.get("label", "_metadata_"))
                    if lab is not None:
                        out[lab] = self.name(r, spec.get("name"), lab)
            except ResolveError as e:
                out = {"__error__": str(e)}
            self.ref_cache[key] = out
        return self.ref_cache[key]

    def field_base(self, row, label, scope, path, fields=()):
        if "{" not in path:
            return self.db.under(row, path)

        def sub(m):
            k = m.group(1)
            if k == "label":
                return label or ""
            if k == "scope":
                if scope is None:
                    raise ResolveError("%s: {scope} in a module without a scope" % path)
                return scope
            if k == "season":
                parts = (scope or "").split("_")
                if len(parts) > 1 and parts[0].lower() in ("f1", "f2") and re.fullmatch(r"\d{4}", parts[1]):
                    return "%s_%s" % (parts[0].lower(), parts[1])
                raise ResolveError("%s: %s has no season" % (path, scope))
            if k.startswith("ref."):
                fid, rel = k[4:].split(".", 1)
                f = next((f for f in fields if f["id"] == fid), None)
                if f is None or not f.get("refs"):
                    raise ResolveError("%s: no ref field %s" % (path, fid))
                want = self.label(row, f["path"])
                spec = f["refs"]
                for r in self.table_rows(spec["rows"], spec.get("filter"), scope):
                    if self.label(r, spec.get("label", "_metadata_")) == want:
                        v = self.label(r, rel)
                        if v is not None:
                            return v
                raise ResolveError("%s: no %s under %s" % (path, rel, want))
            if k.startswith("row."):
                v = self.label(row, k[4:])
                if v is None:
                    raise ResolveError("%s: row has no %s" % (path, k[4:]))
                return v
            if re.fullmatch(r"[0-9a-fA-F]{16}", k):
                return m.group(0)
            raise ResolveError("%s: unknown placeholder {%s}" % (path, k))

        return self.db.under(None, re.sub(r"\{([^}]+)\}", sub, path))

    def render(self, f, value, scope):
        kind = f.get("kind")
        if f.get("lng"):
            t = self.english(value)
            return "%s (%s)" % (value, t) if t else value
        if kind in ("ref", "ref-list") and f.get("refs"):
            names = self.refs(f["refs"], scope)
            parts = value.split(",") if kind == "ref-list" else [value]
            return ",".join("%s (%s)" % (p, names[p]) if names.get(p) and names[p] != p else p for p in parts)
        if kind == "choice":
            opts = {o["value"]: o["label"] for o in f.get("options", [])}
            return "%s (%s)" % (value, opts[value]) if value in opts else value
        return value

    def warn_value(self, f, value, scope):
        kind = f.get("kind")
        if kind == "choice":
            if value not in {o["value"] for o in f["options"]}:
                return "value %r is not an option" % value
        if kind in ("ref", "ref-list") and f.get("refs"):
            names = self.refs(f["refs"], scope)
            if "__error__" in names:
                return "refs: " + names["__error__"]
            parts = [p for p in value.split(",") if p] if kind == "ref-list" else ([value] if value else [])
            bad = [p for p in parts if p not in names]
            if bad:
                return "unknown refs %s" % ",".join(sorted(set(bad))[:4])
        if f.get("lng") and self.lang and value.startswith("lng_") and value not in self.lang:
            return "no English text for %s" % value
        return None

    def check(self, m, verbose):
        fails = 0
        scopes = [None]
        sc = m.get("scope")
        print("== %s (%s)" % (m["id"], m["title"]))
        if sc:
            try:
                srows = self.table_rows(sc["rows"], sc.get("filter"))
            except ResolveError as e:
                print("  FAIL scope: %s" % e)
                return 1
            scopes = []
            for r in srows:
                lab = self.label(r, sc.get("label", "_metadata_"))
                if lab is not None:
                    scopes.append(lab)
            if not scopes:
                print("  FAIL scope %s: no rows" % sc["rows"])
                return 1
            if sc.get("default") and sc["default"] not in scopes:
                print("  FAIL scope default %s is not a scope label" % sc["default"])
                fails += 1
        fields = m["fields"]
        sample = None
        sample_scope = sc.get("default") if sc and sc.get("default") in scopes else scopes[0]
        per_scope = []
        missing = {}
        optional_hits = {}
        warnings = {}
        for scope in scopes:
            try:
                rows = self.table_rows(m["rows"], m.get("filter"), scope)
            except ResolveError as e:
                print("  FAIL rows (%s): %s" % (scope, e))
                fails += 1
                continue
            per_scope.append((scope, len(rows)))
            if not rows:
                print("  FAIL rows (%s): table is empty" % scope)
                fails += 1
            for row in rows:
                label = self.label(row, m.get("label", "_metadata_"))
                cells = []
                for f in fields:
                    value = None
                    try:
                        b = self.field_base(row, label, scope, f["path"], m["fields"])
                        value = self.db.text(b)
                    except ResolveError:
                        value = None
                    if value is None:
                        if not f.get("optional"):
                            missing.setdefault(f["id"], []).append("%s%s" % ("" if scope is None else scope + ":", label or row.text))
                    else:
                        if f.get("optional"):
                            optional_hits[f["id"]] = optional_hits.get(f["id"], 0) + 1
                        w = self.warn_value(f, value, scope)
                        if w:
                            warnings.setdefault(f["id"], []).append("%s: %s" % (label or row.text, w))
                    cells.append(value)
                if sample is None and scope == sample_scope and label is not None:
                    sample = (scope, row, label, cells)
        total = sum(n for _, n in per_scope)
        if sc:
            shown = ", ".join("%s=%d" % (s, n) for s, n in per_scope)
            print("  rows: %d over %d scopes (%s)" % (total, len(per_scope), shown if len(shown) < 400 else shown[:400] + "…"))
        else:
            print("  rows: %d" % total)
        for fid, where in missing.items():
            fails += 1
            print("  MISSING %s on %d rows: %s" % (fid, len(where), ", ".join(where[:6]) + (" …" if len(where) > 6 else "")))
        for f in fields:
            if f.get("optional"):
                n = optional_hits.get(f["id"], 0)
                note = "  optional %s on %d of %d rows" % (f["id"], n, total)
                if n == 0:
                    fails += 1
                    print(note.replace("  optional", "  FAIL optional") + " (never present)")
                elif verbose:
                    print(note)
        for fid, ws in warnings.items():
            print("  warn %s on %d rows: %s" % (fid, len(ws), "; ".join(ws[:3])))
        if sample:
            scope, row, label, cells = sample
            print("  sample %s%s = %s" % ("" if scope is None else "[%s] " % scope, row.text,
                                          self.name(row, m.get("name"), label)))
            for f, v in zip(fields, cells):
                if v is None:
                    continue
                if verbose or f["id"] in m["columns"]:
                    print("    %-28s %s" % (f["label"][:28], self.render(f, v, scope)))
        return fails


def main():
    ap = argparse.ArgumentParser(description="Check the DB Editor module specs against a game database")
    ap.add_argument("db", help="gamedatabases.2025.bin")
    ap.add_argument("--lang", help="language_eng.lng (English text for lng_* keys)")
    ap.add_argument("--modules", default=DEFAULT_MODULES)
    ap.add_argument("--only", help="comma-separated module ids")
    ap.add_argument("-v", "--verbose", action="store_true", help="print every field of the sample row")
    args = ap.parse_args()
    doc = json.load(open(args.modules, encoding="utf-8"))
    errs = check_schema(doc)
    for e in errs:
        print("SCHEMA %s" % e)
    if errs:
        return 1
    checker = Checker(Db(args.db), load_lng(args.lang) if args.lang else None)
    only = set(args.only.split(",")) if args.only else None
    fails = 0
    for m in doc["modules"]:
        if only and m["id"] not in only:
            continue
        fails += checker.check(m, args.verbose)
    print("%d modules, %d failures" % (len(doc["modules"]), fails))
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
