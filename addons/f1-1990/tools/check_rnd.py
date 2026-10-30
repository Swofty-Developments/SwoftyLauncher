#!/usr/bin/env python3
"""Checks a build with db/rnd.json against stock and against the same build without it.

usage: check_rnd.py <stock gamedatabases.2025.bin> <db built without rnd.json> <db built with it>

Fails when rnd.json changes a stock key other than the shared component row count, when a stock R&D key
(every `upgrades.<prefix>` tree, engine supplier, schedule and challenge table of the 2025 and 2026
careers and My Team, the stock shared component sets, the 2025/2026 resource point groups, the department
entries) differs from stock in the full build, or when a 1990 R&D row is missing.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import career  # noqa: E402
import rnd  # noqa: E402

gamedb = career.gamedb
STOCK_PREFIXES = ["upgrades.2026", "upgrades.2022", "upgrades.myteam_om_xm", "upgrades.myteam_2025"]
STOCK_TOKENS = ["ferrari", "force_india", "haas", "mclaren", "mercedes", "redbull", "renault", "sauber",
                "tororosso", "user_defined", "williams", "gm_cadillac"]
TREE_TABLES = ["aerodynamics", "chassis", "durability", "engine", "fake", "upgrade_type_metadata",
               "connection_waypoint_paths"]
TREE_ROWS = [("teams_linked_to", ["_metadata_", "links"]), ("department_tweakables_modes", ["_metadata_"]),
             ("tweakables", rnd.TWEAK_FIELDS)]
ALLOWED = {gamedb.fnv1a64(f"{rnd.SHARED}._SharedComponents_instancecount_".encode())}


def h(path):
    return gamedb.fnv1a64(path.encode())


def hashed(db, path):
    tag = "{%016x}" % h(path)
    out = set()
    for key in db.cat:
        if key.startswith(tag + "."):
            out.update(f"{path}.{k}" for k in db.under(path, key[len(tag) + 1:]))
    return out


def copied(db, path, sub):
    return hashed(db, path) | {f"{path}.{k}" for p in rnd.UNCATALOGUED.get(sub, []) for k in db.under(path, p)}


def protected(db):
    keys = set()
    for prefix in STOCK_PREFIXES:
        for sub in rnd.COPIED + ["attribute_modifiers"]:
            keys |= copied(db, f"{prefix}.{sub}", sub)
        for token in STOCK_TOKENS:
            tree = f"{prefix}.trees.{token}"
            for table in TREE_TABLES:
                keys |= hashed(db, f"{tree}.{table}")
                fake = f"{tree}.fake"
                if table == "fake" and db.count(fake, "upgrade"):
                    keys.add(f"{fake}._upgrade_instancecount_")
                    keys |= {f"{fake}.upgrade_0.{f}" for f in rnd.ROW_FIELDS}
            for word, fields in TREE_ROWS:
                n = db.count(tree, word)
                if n is None:
                    continue
                keys.add(f"{tree}._{word}_instancecount_")
                keys |= {f"{tree}.{word}_{i}.{f}" for i in range(n) for f in fields}
    for i in range(db.count(rnd.SHARED, "SharedComponents")):
        row = f"{rnd.SHARED}.SharedComponents_{i}"
        keys |= {f"{row}.{f}" for f in db.probe(row, set())}
    for label in ("f1_career_2025", "f1_career_2026"):
        i = db.row_index("resource_points", "data_group", label)
        for f in db.fields("resource_points", "data_group", label, probe=False):
            keys.add(f"resource_points.data_group_{i}.{f}")
    return {h(k) for k in keys if h(k) in db.d}


def main():
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    stock = career.Db(sys.argv[1])
    base = {r.key: (r.type, r.value) for r in gamedb.read(open(sys.argv[2], "rb").read())}
    built = {r.key: (r.type, r.value) for r in gamedb.read(open(sys.argv[3], "rb").read())}
    stock_raw = {r.key: (r.type, r.value) for r in gamedb.read(open(sys.argv[1], "rb").read())}
    bad = []

    changed = [k for k in set(base) | set(built) if base.get(k) != built.get(k)]
    removed = [k for k in base if k not in built]
    touched_stock = [k for k in changed if k in stock_raw and k not in ALLOWED]
    print(f"rnd.json: {len(changed)} keys written or changed, {len(removed)} removed, "
          f"{len(touched_stock)} of them stock keys (allowed: the shared component row count)")
    bad += [f"rnd.json changes stock key {k:016x}" for k in touched_stock[:20]]
    bad += [f"rnd.json removes key {k:016x}" for k in removed[:20]]

    keys = protected(stock)
    diff = [k for k in keys if built.get(k) != stock_raw.get(k)]
    print(f"stock R&D keys: {len(keys)} protected, {len(diff)} differ in the full build")
    bad += [f"stock R&D key {k:016x} differs" for k in diff[:20]]

    def get(path):
        v = built.get(h(path))
        return v[1].decode("utf-8", "replace") if v else None

    n = int(get(f"{rnd.SHARED}._SharedComponents_instancecount_") or 0)
    row = next((f"{rnd.SHARED}.SharedComponents_{i}" for i in range(n)
                if get(f"{rnd.SHARED}.SharedComponents_{i}._metadata_") == rnd.SHARED_1990), None)
    if not row:
        bad.append(f"no {rnd.SHARED_1990} row")
    else:
        src = f"{rnd.SHARED}.SharedComponents_{stock.row_index(rnd.SHARED, 'SharedComponents', rnd.SHARED_SOURCE)}"
        fields = stock.probe(src, set())
        for f in sorted(fields):
            want = rnd.SHARED_SET.get(f, stock.get(f"{src}.{f}"))
            if get(f"{row}.{f}") != want:
                bad.append(f"{row}.{f} is {get(f'{row}.{f}')!r}, not {want!r}")
        print(f"{rnd.SHARED_1990}: {len(fields)} keys of {rnd.SHARED_SOURCE} checked")
    champ = next((f"championships.Championship_{i}" for i in range(int(get("championships._Championship_instancecount_")))
                  if get(f"championships.Championship_{i}._metadata_") == career.CHAMPIONSHIP), None)
    if not champ or get(f"{champ}.shared_components") != rnd.SHARED_LIST:
        bad.append(f"{career.CHAMPIONSHIP} does not list {rnd.SHARED_1990}")
    for token, (teams, *_rest) in rnd.TOKENS.items():
        tree = f"{rnd.PREFIX}.trees.{token}"
        links = (get(f"{tree}.teams_linked_to_0.links") or "").split(",")
        if sorted(links) != sorted(f"{t}_1990{b}" for t in teams for b in ("", "b")):
            bad.append(f"{tree}: teams_linked_to {links}")
        rows, _ = rnd.tree_rows(token)
        total = sum(int(get(f"{tree}.{d}._upgrade_instancecount_") or 0) for d in rnd.DEPARTMENT_FILES.values())
        if total != len(rows):
            bad.append(f"{tree}: {total} upgrade rows, want {len(rows)}")
        src = f"upgrades.2026.trees.{token}.department_tweakables_modes_0"
        for mode in rnd.MODES:
            for f in rnd.MODE_FIELDS:
                if stock.get(f"{src}.{mode}.{f}") is not None and get(f"{tree}.department_tweakables_modes_0.{mode}.{f}") is None:
                    bad.append(f"{tree}.department_tweakables_modes_0.{mode}.{f} missing")
        names = [get(f"{tree}.chassis.upgrade_{i}.upgrade_name") for i in range(6)]
        if not any(n and n.startswith("f1990_bspec") for n in names):
            bad.append(f"{tree}: no f1990_bspec upgrade")
    print(f"1990 trees: {len(rnd.TOKENS)} tokens checked")
    for sub in rnd.COPIED:
        src = f"{rnd.STOCK}.{sub}"
        keys = sorted(copied(stock, src, sub))
        miss = [k for k in keys if built.get(h(rnd.PREFIX + k[len(rnd.STOCK):])) != stock_raw.get(h(k))]
        bad += [f"{rnd.PREFIX + k[len(rnd.STOCK):]} is not a copy of {k}" for k in miss[:20]]
        print(f"{rnd.PREFIX}.{sub}: {len(keys) - len(miss)}/{len(keys)} keys copied from {src}")
    package = get(f"{rnd.PREFIX}.general.ui_data_0.texture_array_dictionary_path")
    if not package:
        bad.append(f"{rnd.PREFIX}.general.ui_data_0.texture_array_dictionary_path is missing: no R&D art in 1990")
    else:
        print(f"1990 R&D UI package: {package}")

    for p in bad:
        print("FAIL", p)
    if not bad:
        print("ok: 2025/2026 R&D untouched, the 1990 R&D rows are in place")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
