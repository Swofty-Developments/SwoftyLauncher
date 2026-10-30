#!/usr/bin/env python3
"""Checks that every row the addon copies carries every key its stock source row has.

usage: check_fields.py <stock gamedatabases.2025.bin> <F1_25.exe> [op file ...] [--built <built .bin>] [--names <file>]

An add-row op copies only the fields its op file lists for the table; a stock key missing from the list is dropped
from the new row. The generators find a row's keys by probing the stock database with known names (the key
catalogue, tools/gamedb/names.txt, ...), so a component or field whose name no list knows is missed. This tool probes
each source row with every identifier string in the exe as well (hashed in bulk), recursing into nested tables and
counted rows, then tries each found leaf under the row's other objects of the same branch (a sibling or a nested
object), which finds keys the exe composes at run time, and prints per op the keys the stock row has that the op file
does not list. Rows past an instance count
the op sets are left out on purpose, and a child data set (parent_data_set) is checked as a tree. With --built (a
database built with the addon) it also compares the rows that set-create ops build (TREES, and the subtrees of COPIES)
with the stock rows of the same shape: a field every stock row of a shape has must be on the built row, and it lists
every stat_id / statId an added row shares with another row of its table. With --names
it writes the names it needed that tools/gamedb/names.txt lacks. Exit code 1 when a row misses a key.
"""
import argparse
import json
import re
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO = ROOT.parents[1]
sys.path.insert(0, str(HERE))
import career  # noqa: E402

COPIES = [("db/rnd.json", "upgrades.1990", "upgrades.2026")]
TREES = [
    ("track_data.game_setup.TrackDataSet[_metadata_=f1_1990]", "track_data.game_setup.TrackDataSet[_metadata_=f1_2026]", True),
    ("game_setup.game_setup_modes.game_setup_mode[_metadata_=f1_1990]",
     "game_setup.game_setup_modes.game_setup_mode[_metadata_=f1_26]", False),
    ("options_system.context.tag_folder[_metadata_=game_setup_mode].game_setup_mode_tag[_metadata_=gamesetup_f1_1990]",
     "options_system.context.tag_folder[_metadata_=game_setup_mode].game_setup_mode_tag[_metadata_=gamesetup_f1]", False),
]
OPS = ["db/career.json", "db/grid.json", "db/grid_standings.json", "db/grid_links.json", "db/rules.json",
       "db/cars.json", "db/rnd.json", "db/audio.json"]
PRIME = np.uint64(0x100000001b3)
MAX_NAME = 128
FULL_ROWS = 8


def fnv(data, h=career.gamedb.FNV_BASIS):
    for c in data:
        h = ((h ^ c) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h


def exe_names(exe):
    data = Path(exe).read_bytes()
    names = set(m.decode() for m in re.findall(rb"[A-Za-z_][A-Za-z0-9_]{1,%d}" % (MAX_NAME - 1), data))
    numbered = set(m.decode() for m in re.findall(rb"[A-Za-z_][A-Za-z0-9_]*%d[A-Za-z0-9_]*", data))
    names |= {n.replace("%d", str(k)) for n in numbered if n.count("%d") == 1 for k in range(16)}
    return {n for n in names if re.search(r"[a-z]", n)}


class Probe:
    def __init__(self, db, names):
        self.db = db
        self.keys = np.array(sorted(db.d), dtype=np.uint64)
        self.names = sorted((n for n in names if 0 < len(n.encode()) <= MAX_NAME), key=lambda n: (len(n.encode()), n))
        self.index = {n: i for i, n in enumerate(self.names)}
        self.buckets = []
        lens = [len(n.encode()) for n in self.names]
        start = 0
        while start < len(self.names):
            end = start
            while end < len(self.names) and lens[end] == lens[start]:
                end += 1
            raw = np.frombuffer(b"".join(n.encode() for n in self.names[start:end]), dtype=np.uint8)
            self.buckets.append((start, raw.reshape(end - start, lens[start]).astype(np.uint64)))
            start = end
        self.shapes = {}

    def _has(self, hashes):
        i = np.searchsorted(self.keys, hashes)
        i[i >= len(self.keys)] = 0
        return self.keys[i] == hashes

    def _bulk(self, prefix, suffix, subset):
        if subset is not None:
            h0 = fnv(prefix)
            return [i for i in subset if fnv(self.names[i].encode() + suffix, h0) in self.db.d]
        found = []
        h0 = np.uint64(fnv(prefix))
        with np.errstate(over="ignore"):
            for start, raw in self.buckets:
                h = np.full(raw.shape[0], h0, dtype=np.uint64)
                for j in range(raw.shape[1]):
                    h = (h ^ raw[:, j]) * PRIME
                for c in suffix:
                    h = (h ^ np.uint64(c)) * PRIME
                found.extend((start + np.nonzero(self._has(h))[0]).tolist())
        return found

    def row(self, path):
        out = set()
        sh = re.sub(r"_\d+(?=\.|$)", "_N", path)
        self._walk(path, "", out, sh)
        self._descend(path, out, sh)
        self._transplant(path, out, sh)
        return out

    def _descend(self, row, out, shape):
        done = set()
        while True:
            parents = {k.rpartition(".")[0] for k in out if "." in k} - done
            parents = {p for p in parents if not re.search(r"_\d+$", p)}
            if not parents:
                return
            for parent in sorted(parents):
                done.add(parent)
                seen = self.shapes.setdefault(f"{shape}/{re.sub(r'_[0-9]+(?=[.]|$)', '_N', parent)}#object",
                                              {"rows": 0, "names": set()})
                subset = None if seen["rows"] < FULL_ROWS else sorted(seen["names"])
                seen["rows"] += 1
                leaves = self._bulk(f"{row}.{parent}.".encode(), b"", subset)
                seen["names"].update(leaves)
                out.update(f"{parent}.{self.names[i]}" for i in leaves)

    def _transplant(self, row, out, shape):
        seen = self.shapes.setdefault(shape + "#transplant", {"rows": 0, "names": set()})
        if seen["rows"] < FULL_ROWS:
            by = {}
            for k in out:
                parent, _, leaf = k.rpartition(".")
                if not leaf.startswith("_"):
                    by.setdefault(parent, set()).add(leaf)
            cands = set()
            for p, leaves in by.items():
                for q in by:
                    if q != p and (q.startswith(p + ".") or not p or q.rpartition(".")[0] == p.rpartition(".")[0]):
                        cands.update(f"{q}.{leaf}" for leaf in leaves - by[q])
        else:
            cands = seen["names"] - out
        seen["rows"] += 1
        found = {c for c in cands - out if fnv(f"{row}.{c}".encode()) in self.db.d}
        seen["names"].update(found)
        out.update(found)

    def _walk(self, row, rel, out, shape):
        seen = self.shapes.setdefault(shape, {"rows": 0, "names": set()})
        subset = None if seen["rows"] < FULL_ROWS else sorted(seen["names"])
        seen["rows"] += 1
        pre = (f"{row}.{rel}" if rel else f"{row}.").encode()
        leaves = self._bulk(pre, b"", subset)
        tables = self._bulk(pre + b"_", b"_instancecount_", subset)
        uncounted = [i for i in self._bulk(pre, b"_0._metadata_", subset) if i not in set(tables)]
        seen["names"].update(leaves + tables + uncounted)
        out.update(rel + self.names[i] for i in leaves)
        for i in tables:
            t = self.names[i]
            out.add(f"{rel}_{t}_instancecount_")
            count = self.db.get(f"{row}.{rel}_{t}_instancecount_")
            for k in range(int(count) if count and count.lstrip("-").isdigit() else 0):
                self._walk(row, f"{rel}{t}_{k}.", out, f"{shape}/{t}")
        for i in uncounted:
            t, k = self.names[i], 0
            while k < 512 and self.db.get(f"{row}.{rel}{t}_{k}._metadata_") is not None:
                self._walk(row, f"{rel}{t}_{k}.", out, f"{shape}/{t}")
                k += 1


SEG = re.compile(r"([^.\[]+)(?:\[([^=\]]+)=([^\]]*)\])?")


def resolve(db, path):
    out = ""
    for seg in split_path(path):
        name, field, value = SEG.fullmatch(seg).groups()
        if field is None:
            out = f"{out}.{name}" if out else name
        else:
            out = f"{out}.{name}_{db.row_index(out, name, value, by=field)}"
    return out


def check(db, probe, op_file, built=None):
    doc = json.loads(Path(op_file).read_text())
    fields, rows = doc.get("fields", {}), []
    for op in doc.get("ops", []):
        if op.get("op") != "add-row":
            continue
        src_probe = probe
        try:
            src = resolve(db, op["from"])
        except KeyError:
            if built is None:
                rows.append((op["from"], None, [], [], 0))
                continue
            try:
                src, src_probe = "built:" + resolve(built[0], op["from"]), built[1]
            except KeyError:
                rows.append((op["from"], None, [], [], 0))
                continue
        last = SEG.fullmatch(split_path(op["from"])[-1])
        word = last.group(1) if last.group(2) else re.sub(r"_\d+$", "", last.group(1))
        if "parent_data_set" in op.get("set", {}):
            rows.append((op["from"], src, None, [], len(fields.get(f"{op['table']}.{word}", []))))
            continue
        listed = set(fields.get(f"{op['table']}.{word}", []))
        have = {k for k in src_probe.row(src.removeprefix("built:")) if not cut(k, op.get("set", {}))}
        rows.append((op["from"], src, sorted(have - listed), sorted(listed - have), len(listed)))
    return rows


def cut(key, values):
    for k, v in values.items():
        m = re.fullmatch(r"(.*?)_(\w+?)_instancecount_", k)
        if not m or not str(v).isdigit():
            continue
        row = re.match(rf"{re.escape(m.group(1))}{re.escape(m.group(2))}_(\d+)\.", key)
        if row and int(row.group(1)) >= int(v):
            return True
    return False


def split_path(path):
    parts, depth, cur = [], 0, ""
    for ch in path:
        if ch == "[":
            depth += 1
        elif ch == "]":
            depth -= 1
        if ch == "." and not depth:
            parts.append(cur)
            cur = ""
        else:
            cur += ch
    return parts + [cur]


def rows_of(keys):
    out = {}
    for k in keys:
        m = re.match(r"^((?:.*\.)?[^.]*_\d+)\.(.*)$", k)
        row, field = (m.group(1), m.group(2)) if m else ("", k)
        while re.search(r"_\d+\.", field):
            sub = re.match(r"^(.*?_\d+)\.(.*)$", field)
            row, field = f"{row}.{sub.group(1)}" if row else sub.group(1), sub.group(2)
        out.setdefault(row, set()).add(field)
    return out


def shape(row):
    return re.sub(r"_\d+(?=\.|$)", "_N", row)


def check_tree(stock_probe, built_probe, built_root, stock_root, sparse):
    stock_rows = rows_of(stock_probe.row(stock_root))
    built_rows = rows_of(built_probe.row(built_root))
    must = {}
    for row, fields in stock_rows.items():
        sh = shape(row)
        must[sh] = fields if sh not in must else must[sh] & fields
    gaps = []
    for row, fields in sorted(built_rows.items()):
        want = must.get(shape(row))
        if want is None:
            continue
        missing = sorted(f for f in want - fields if not (sparse and re.fullmatch(r"_\w+_instancecount_", f)))
        if missing:
            gaps.append((row, missing))
    return len(built_rows), gaps


def shared_ids(stock, built, op_files):
    tables = {}
    for op_file in op_files:
        for op in json.loads(Path(op_file).read_text()).get("ops", []):
            m = re.search(r"\.([A-Za-z_]\w*)\[[^\]]*\]$", op.get("from", "")) if op.get("op") == "add-row" else None
            if m and "[" not in op["table"]:
                tables[op["table"]] = m.group(1)
    out = []
    for table, word in sorted(tables.items()):
        stock_rows = {stock.get(f"{table}.{word}_{i}._metadata_") for i in range(stock.count(table, word) or 0)}
        for field in ("stat_id", "statId"):
            rows = {}
            for i in range(built.count(table, word) or 0):
                value = built.get(f"{table}.{word}_{i}.{field}")
                if value not in (None, ""):
                    rows.setdefault(value, []).append(built.get(f"{table}.{word}_{i}._metadata_"))
            out += [(table, field, v, names) for v, names in sorted(rows.items())
                    if len(names) > 1 and any(n not in stock_rows for n in names)]
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("stock")
    ap.add_argument("exe")
    ap.add_argument("ops", nargs="*")
    ap.add_argument("--names")
    ap.add_argument("--built", help="a built gamedatabases.2025.bin: also compare the rows set-create ops build")
    a = ap.parse_args()
    db = career.Db(a.stock)
    known = {n.decode() for n in db.vocabulary()[0]}
    vocab = exe_names(a.exe) | known
    for op_file in a.ops or [ROOT / f for f in OPS]:
        for listed in json.loads(Path(op_file).read_text()).get("fields", {}).values():
            vocab.update(listed)
    probe = Probe(db, vocab)
    built = career.Db(a.built) if a.built else None
    built_probe = Probe(built, vocab) if built else None
    bad, needed = 0, set()
    for op_file in a.ops or [ROOT / f for f in OPS]:
        rows = check(db, probe, op_file, (built, built_probe) if built else None)
        print(f"== {Path(op_file).name}: {len(rows)} add-row op(s)")
        for frm, src, missing, unknown, n in rows:
            if src is None:
                print(f"  {'added':12} {n:5} listed  {frm}  (a row this addon adds; checked as its own op)")
                continue
            if missing is None:
                print(f"  {'sparse':12} {n:5} listed  {frm}  (child of a parent data set: checked as a tree below)")
                continue
            state = f"MISSING {len(missing)}" if missing else "ok"
            print(f"  {state:12} {n:5} listed  {frm}  ({src})")
            for k in missing[:40]:
                print(f"      + {k}")
            if len(missing) > 40:
                print(f"      ... {len(missing) - 40} more")
            bad += bool(missing)
            for k in missing:
                needed.update(s for s in re.split(r"[.]", k) if s and not re.fullmatch(r".*_\d+", s))
    if built:
        trees = list(TREES)
        for op_file, prefix, source in COPIES:
            roots = set()
            for op in json.loads((ROOT / op_file).read_text())["ops"]:
                if op.get("op") == "set" and op.get("create") and op["select"].startswith(prefix + "."):
                    head = re.match(r"^(.*?)\.[^.]*?(?:_\d+|\[[^\]]*\])(?:\.|$)", op["select"] + ".")
                    roots.add(head.group(1) if head else op["select"])
            trees += [(r, source + r[len(prefix):], False) for r in sorted(roots)]
        for built_root, stock_root, sparse in trees:
            try:
                n, gaps = check_tree(probe, built_probe, resolve(built, built_root), resolve(db, stock_root), sparse)
            except KeyError as e:
                print(f"== tree {built_root}: not in the build ({e})")
                continue
            print(f"== tree {built_root} vs {stock_root}{' (sparse)' if sparse else ''}: {n} row(s), "
                  f"{len(gaps)} missing fields every stock row of their shape has")
            for row, missing in gaps[:30]:
                print(f"  MISSING {row}: {', '.join(missing[:12])}{' ...' if len(missing) > 12 else ''}")
            bad += bool(gaps)
    if built:
        clashes = shared_ids(db, built, a.ops or [ROOT / f for f in OPS])
        print(f"== stat ids: {len(clashes)} id(s) an addon row shares with another row")
        for table, field, value, names in clashes[:40]:
            print(f"  SHARED {table} {field} {value}: {', '.join(names)}")
        bad += bool(clashes)
    if a.names:
        names_txt = REPO / "tools/gamedb/names.txt"
        have = {line.strip() for line in open(names_txt, encoding="utf-8")}
        new = sorted(n.strip("_").removesuffix("_instancecount") if n.endswith("_instancecount_") else n
                     for n in needed)
        Path(a.names).write_text("".join(f"{n}\n" for n in new if n not in have))
    print(f"{bad} op(s) miss stock keys")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
