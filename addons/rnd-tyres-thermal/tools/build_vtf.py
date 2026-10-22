"""Build the VTF upgrade blocks and the upgrade_type_metadata rows for the Tyres upgrade types.

The Tyres department mints its own upgrade types natively (ids 19-22, native/HOOKS.md v0.9.0).
The game applies an upgrade type by finding the `<Upgrade name="radUpgrade...">` block of that
type in the car's handling VTF and replaying its ops scaled by the type's progress, and it reads
each type's display values and `percentage_worth` from `upgrade_type_metadata`. This script
writes both halves from stock data:

  vtf/<resource path>.f0, .f1   each car handling VTF with the new blocks appended after the
                                stock radUpgrade* blocks (fragment 0 = BXML, 1 = TSMD stats)
  erp/rnd_upgrade_vtf.json      replace-resource ops for both fragments in every archive that
                                carries that VTF
  db/upgrade_types.json         one row per new type in every upgrade_type_metadata table
  db/upgrade_types_1990.json    the same rows in the 12 `upgrades.1990.trees.<token>` metadata
                                tables of the F1 1990 addon's career, with the tyre supplier's
                                Tyre Thermal and Tyre Life values (AFFECT_1990)

Cars: every handling VTF under f1_2026_vehicle_package and f1_2025_vehicle_package (team cars,
fom_car*, myteam_*). Each new op only targets a parameter the car's own base section has.

Power Unit Cooling (radUpgradePUCooling, type 22) scales the Engine RadiatorSurfaceArea, which the
engine temperature update 0x144689710 multiplies into its cooling term; --no-pu leaves it out.

Every build is checked before anything is compared or written: each edited VTF must be the
stock bytes plus one inserted run of the new blocks, with matching TSMD stats, and the metadata
ops are applied to a copy of the stock gamedb, which must round-trip with no stock record
changed. It then compares the build with the shipped files and exits 1 on any difference;
--write replaces them.
"""
import argparse
import hashlib
import json
import os
import shutil
import sys

import common
import gamedb
import vtf

GROUPS = '2025_asset_groups'
PACKAGES = ['f1_2026_vehicle_package', 'f1_2025_vehicle_package']
GAMEDB = 'game_package/gamedatabases/gamedatabases.2025.bin'
VTF_DIR = 'vtf'
OPS = 'erp/rnd_upgrade_vtf.json'
DB_OPS = 'db/upgrade_types.json'
DB_OPS_1990 = 'db/upgrade_types_1990.json'
F1990_TOOLS = os.path.join(os.path.dirname(common.ADDON), 'f1-1990', 'tools')
AFFECT_1990 = {'goodyear': {'Tyre Thermal': '-12', 'Tyre Life': '-14'},
               'pirelli': {'Tyre Thermal': '-20', 'Tyre Life': '-7'}}

MUL = b'*'

ROW = 'upgrade_type_metadata'
COUNT_FIELD = '_upgrade_type_metadata_instancecount_'
FIELDS = ['_metadata_', 'upgrade_type', 'upgrade_department', 'mass_adjustment_fully_upgraded',
          'bhp_adjustment_fully_upgraded', 'performance_rating_base', 'performance_rating_fully_upgraded',
          'affect_on_vehicle', 'affect_on_vehicle_unit_type', 'percentage_worth']

TYPES = [
    {'id': 19, 'type': 'Tyre Thermal', 'label': 'tyre_thermal', 'block': 'radUpgradeTyreThermal',
     'model': 'Tyre Wear', 'affect': '-15',
     'ops': [('SuspensionSystem', 'Tyre', 'UpgradeTempFactor', '0.85')]},
    {'id': 20, 'type': 'Brake Cooling', 'label': 'brake_cooling', 'block': 'radUpgradeBrakeCooling',
     'model': 'Brakes', 'affect': '25',
     'ops': [('SuspensionSystem', 'Brake', 'CoefHeatTransferConstant', '1.25'),
             ('SuspensionSystem', 'Brake', 'CoefHeatTransferLinear', '1.25')]},
    {'id': 21, 'type': 'Tyre Life', 'label': 'tyre_life', 'block': 'radUpgradeTyreLife',
     'model': 'Tyre Wear', 'affect': '-10',
     'ops': [('SuspensionSystem', 'Tyre', 'UpgradeWearFactor', '0.9')]},
]
PU = {'id': 22, 'type': 'Power Unit Cooling', 'label': 'pu_cooling', 'block': 'radUpgradePUCooling',
      'model': 'Internal Combustion Engine Durability', 'affect': '15',
      'ops': [('Engine', None, 'RadiatorSurfaceArea', '1.15')]}


def game_root(game):
    return os.path.dirname(os.path.dirname(common.ui_package(game)))


def el(name, attrs=(), children=()):
    return vtf.Node(0, 0, name, list(attrs), list(children), end=vtf.Node(5, 0))


def leaf(name, value):
    return el(name, [(b'value', value), (b'op', MUL)])


def section(root, name):
    hits = [c for c in root.children if c.kind == 0 and c.name == name]
    return hits[0] if len(hits) == 1 else None


def make_block(root, t):
    by_comp = {}
    for comp, sub, param, value in t['ops']:
        base = section(root, comp.encode())
        if base is None:
            continue
        if sub is None:
            if not base.find(param.encode()):
                continue
            by_comp.setdefault(comp, []).append(leaf(param.encode(), value.encode()))
            continue
        for part in [c for c in base.children if c.kind == 0 and c.name == sub.encode()]:
            if not part.find(param.encode()):
                continue
            by_comp.setdefault((comp, sub), {}).setdefault(part.attr(b'name'), []).append(
                leaf(param.encode(), value.encode()))
    children = []
    done = []
    for comp, sub, _param, _value in t['ops']:
        key = comp if sub is None else (comp, sub)
        if key in done or key not in by_comp:
            continue
        done.append(key)
        if sub is None:
            children.append(el(comp.encode(), [], by_comp[key]))
            continue
        parts = by_comp[key]
        count = (b'num' + sub.encode() + b'Types', str(len(parts)).encode())
        children.append(el(comp.encode(), [count],
                           [el(sub.encode(), [(b'name', n)], kids) for n, kids in parts.items()]))
    if not children:
        return None
    return el(b'Upgrade', [(b'name', t['block'].encode())], children)


def upgrade_names(root):
    return [c.attr(b'name') for c in root.children if c.kind == 0 and c.name == b'Upgrade']


def check_builder(root):
    """The block builder must reproduce two stock blocks byte for byte."""
    stock = {c.attr(b'name'): c for c in root.children if c.kind == 0 and c.name == b'Upgrade'}
    probes = [
        (b'radUpgradeTyreWear', {'block': 'radUpgradeTyreWear',
                                 'ops': [('SuspensionSystem', 'Tyre', 'UpgradeWearFactor', None)]}),
        (b'very_high', None),
    ]
    for name, t in probes:
        s = stock.get(name)
        if s is None:
            continue
        if t is None:
            susp = section(s, b'SuspensionSystem')
            if susp is None or susp.find(b'Steering'):
                continue
            want = vtf.write(susp)
            vals = {}
            for b in susp.find(b'Brake'):
                for p in b.children:
                    vals[p.name] = p.attr(b'value')
            ops = [('SuspensionSystem', 'Brake', p.decode(), v.decode()) for p, v in vals.items()]
            got = make_block(root, {'block': 'x', 'ops': ops})
            got = section(got, b'SuspensionSystem') if got else None
            if got is None or vtf.write(got) != want:
                return f'{name.decode()} SuspensionSystem differs from the builder'
            continue
        value = None
        for tyre in section(s, b'SuspensionSystem').find(b'Tyre'):
            value = tyre.find(b'UpgradeWearFactor')[0].attr(b'value').decode()
        t = dict(t, ops=[(c, sub, p, value) for c, sub, p, _ in t['ops']])
        got = make_block(root, t)
        if got is None or vtf.write(got) != vtf.write(s):
            return f'{name.decode()} differs from the builder'
    return None


def collect(game):
    root = game_root(game)
    found = {}
    for pkg in PACKAGES:
        top = os.path.join(root, GROUPS, pkg)
        for d, _dirs, names in os.walk(top):
            for n in sorted(names):
                if not n.endswith('.erp'):
                    continue
                path = os.path.join(d, n)
                arc = common.Archive(path)
                rel = os.path.relpath(path, root).replace(os.sep, '/')
                for name, r in arc.res.items():
                    if r[1] != 'VTF' or '/handling/' not in name or not name.endswith('.vtf'):
                        continue
                    frags = (arc.read(name, 0), arc.read(name, 1))
                    found.setdefault(name, []).append((rel, frags))
    return found


def edit(f0, f1, types):
    root = vtf.parse(f0)
    if vtf.write(root) != f0:
        raise ValueError('BXML does not round-trip')
    version = int.from_bytes(f1[4:8], 'little')
    if f1[:4] != b'TSMD' or vtf.tsmd(root, version) != f1:
        raise ValueError('TSMD stats do not match the stock BXML')
    names = upgrade_names(root)
    rad = [i for i, c in enumerate(root.children) if c.kind == 0 and c.name == b'Upgrade'
           and (c.attr(b'name') or b'').startswith(b'radUpgrade')]
    if not rad:
        return None, 'no radUpgrade blocks'
    err = check_builder(root)
    if err:
        raise ValueError(err)
    blocks, skipped = [], []
    for t in types:
        if t['block'].encode() in names:
            raise ValueError(f'{t["block"]} already exists')
        b = make_block(root, t)
        if b is None:
            skipped.append(t['block'])
        else:
            blocks.append(b)
    if not blocks:
        return None, 'no target parameters'
    at = rad[-1] + 1
    root.children[at:at] = blocks
    out0 = vtf.write(root)
    return (out0, vtf.tsmd(root, version), [b.attr(b'name').decode() for b in blocks], skipped), None


def short(name):
    return name.split('eaid://', 1)[1]


def build_vtf(game, out, types):
    found = collect(game)
    ops, report, files = [], [], {}
    for name in sorted(found):
        copies = found[name]
        variants = {}
        for rel, frags in copies:
            variants.setdefault(frags, []).append(rel)
        for frags, archives in variants.items():
            res, why = edit(frags[0], frags[1], types)
            if res is None:
                report.append(f'skip {short(name)} ({why}) in {len(archives)} archive(s)')
                continue
            f0, f1, added, skipped = res
            stem = short(name)
            if len(variants) > 1:
                stem += '.' + hashlib.sha1(frags[0]).hexdigest()[:8]
            for i, blob in enumerate((f0, f1)):
                rel_src = f'{VTF_DIR}/{stem}.f{i}'
                files[rel_src] = blob
                for erp in sorted(archives):
                    ops.append({'op': 'replace-resource', 'erp': erp, 'resource': name, 'fragment': i,
                                'source': rel_src})
            report.append(f'{short(name)}: +{",".join(added)}'
                          + (f' (no target for {",".join(skipped)})' if skipped else '')
                          + f'; {len(frags[0])} -> {len(f0)} bytes; {len(archives)} archive(s)')
    ops.sort(key=lambda o: (o['erp'], o['resource'], o['fragment']))
    for rel, blob in files.items():
        dst = os.path.join(out, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        with open(dst, 'wb') as f:
            f.write(blob)
    with open(os.path.join(out, OPS), 'w') as f:
        json.dump({'schema_version': 1, 'ops': ops}, f, indent=1)
        f.write('\n')
    return report, found


def one_insertion(old, new, ins):
    if len(new) != len(old) + len(ins):
        return False
    pre = 0
    while pre < len(old) and old[pre] == new[pre]:
        pre += 1
    suf = 0
    while suf < len(old) and old[len(old) - 1 - suf] == new[len(new) - 1 - suf]:
        suf += 1
    if pre + suf < len(old):
        return False
    return any(new[p:p + len(ins)] == ins for p in range(len(old) - suf, pre + 1))


def verify(game, out, found, types):
    """Apply every op to the stock fragments and re-read the result."""
    with open(os.path.join(out, OPS)) as f:
        ops = json.load(f)['ops']
    by = {}
    for o in ops:
        by.setdefault((o['erp'], o['resource']), {})[o['fragment']] = o['source']
    want = {t['block'].encode(): t for t in types}
    problems, archives, checked = [], set(), 0
    for name, copies in found.items():
        for erp, (s0, s1) in copies:
            src = by.get((erp, name))
            if src is None:
                problems.append(f'{erp}: {short(name)} has no replacement')
                continue
            if set(src) != {0, 1}:
                problems.append(f'{erp}: {short(name)} replaces fragments {sorted(src)}')
                continue
            archives.add(erp)
            n0 = open(os.path.join(out, src[0]), 'rb').read()
            n1 = open(os.path.join(out, src[1]), 'rb').read()
            stock, new = vtf.parse(s0), vtf.parse(n0)
            if vtf.write(new) != n0 or vtf.tsmd(new, int.from_bytes(s1[4:8], 'little')) != n1:
                problems.append(f'{erp}: {short(name)} rebuilt BXML/TSMD mismatch')
            added = [c for c in new.children if c.kind == 0 and c.name == b'Upgrade' and c.attr(b'name') in want]
            kept = [c for c in new.children if not (c.kind == 0 and c.name == b'Upgrade' and c.attr(b'name') in want)]
            if [vtf.write(c) for c in kept] != [vtf.write(c) for c in stock.children] or new.attrs != stock.attrs:
                problems.append(f'{erp}: {short(name)} stock content changed')
            if not one_insertion(s0, n0, b''.join(vtf.write(c)[5:] for c in added)):
                problems.append(f'{erp}: {short(name)} is not the stock bytes plus one inserted run of the new blocks')
            names = upgrade_names(new)
            last_rad = max(i for i, n in enumerate(names) if n.startswith(b'radUpgrade') and n not in want)
            if any(names.index(c.attr(b'name')) <= last_rad for c in added):
                problems.append(f'{erp}: {short(name)} new block before a stock radUpgrade block')
            for c in added:
                t = want[c.attr(b'name')]
                for comp, sub, param, value in t['ops']:
                    base = section(stock, comp.encode())
                    targets = [base] if sub is None else base.find(sub.encode()) if base else []
                    for tg in targets:
                        if not tg.find(param.encode()):
                            continue
                        blk = section(c, comp.encode())
                        hit = blk if sub is None else next(
                            (x for x in blk.find(sub.encode()) if x.attr(b'name') == tg.attr(b'name')), None)
                        lf = hit.find(param.encode()) if hit else []
                        if len(lf) != 1 or lf[0].attrs != [(b'value', value.encode()), (b'op', MUL)]:
                            problems.append(f'{erp}: {short(name)} {c.attr(b"name").decode()} misses '
                                            f'{comp}/{sub}[{tg.attr(b"name")}]/{param}')
            checked += 1
    return problems, archives, checked


def gamedb_path(game):
    return os.path.join(game_root(game), GAMEDB)


def metadata_containers():
    """The 46 `<tree>.upgrade_type_metadata` prefixes, one per career upgrade tree."""
    path = os.path.join(common.REPO, 'tools', 'gamedb', 'catalog', 'tables.tsv')
    out = []
    with open(path) as f:
        for line in f:
            name = line.split('\t', 1)[0]
            if name.startswith('{') and name.endswith('}.upgrade_type_metadata_N'):
                out.append(int(name[1:17], 16))
    return sorted(out)


def build_db(game, out, types):
    import gamedb
    data = open(gamedb_path(game), 'rb').read()
    recs = gamedb.read(data)
    if gamedb.write(recs) != data:
        sys.exit('the stock gamedb does not round-trip')
    by = {r.key: r for r in recs}

    def get(prefix, suffix):
        r = by.get(gamedb.fnv1a64(suffix, prefix))
        return r and (r.type, r.value.decode())

    ops, report, added = [], [], 0
    containers = metadata_containers()
    for prefix in containers:
        count = int(get(prefix, '.' + COUNT_FIELD)[1])
        rows = []
        for i in range(count):
            row = {f: get(prefix, f'.{ROW}_{i}.{f}') for f in FIELDS}
            if None in row.values():
                sys.exit(f'{{{prefix:016x}}}.{ROW}_{i} misses a field: {row}')
            rows.append(row)
        if get(prefix, f'.{ROW}_{count}._metadata_'):
            sys.exit(f'{{{prefix:016x}}} has a row past its count {count}')
        models = {r['upgrade_type'][1]: r for r in rows}
        labels = {r['_metadata_'][1] for r in rows}
        for k, t in enumerate(types):
            if t['label'] in labels or t['type'] in models:
                sys.exit(f'{{{prefix:016x}}} already has {t["type"]}')
            model = models[t['model']]
            select = f'{{{prefix:016x}}}.{ROW}_{count + k}'
            for field in FIELDS:
                tag, value = model[field]
                value = {
                    '_metadata_': t['label'],
                    'upgrade_type': t['type'],
                    'upgrade_department': 'Fake',
                    'mass_adjustment_fully_upgraded': '0',
                    'bhp_adjustment_fully_upgraded': '0',
                    'affect_on_vehicle': t['affect'],
                    'percentage_worth': '1',
                }.get(field, value)
                ops.append({'op': 'set', 'select': select, 'field': field, 'value': value, 'create': True,
                            'type': tag})
                added += 1
        ops.append({'op': 'set', 'select': f'{{{prefix:016x}}}', 'field': COUNT_FIELD,
                    'value': str(count + len(types)), 'create': True, 'type': 0})
    report.append(f'upgrade_type_metadata: {len(containers)} tables, rows 19 -> {19 + len(types)} '
                  f'({", ".join(t["type"] for t in types)}), {added} fields')
    with open(os.path.join(out, DB_OPS), 'w') as f:
        json.dump({'schema_version': 1, 'ops': ops}, f, indent=1)
        f.write('\n')
    report += verify_db(data, recs, ops, containers, types)
    report += build_db_1990(get, out, types)
    return report


def f1990():
    if not os.path.isfile(os.path.join(F1990_TOOLS, 'rnd.py')):
        sys.exit(f'{F1990_TOOLS}/rnd.py not found: the 1990 rows need the F1 1990 addon\'s tools next to this addon')
    sys.path.insert(0, F1990_TOOLS)
    import rnd
    return rnd


def build_db_1990(get, out, types):
    import gamedb
    rnd = f1990()
    template = gamedb.fnv1a64(f'{rnd.STOCK}.trees.{rnd.TEMPLATE_TOKEN}.upgrade_type_metadata')
    count = int(get(template, '.' + COUNT_FIELD)[1])
    if count != 19:
        sys.exit(f'{rnd.STOCK}.trees.{rnd.TEMPLATE_TOKEN}: {count} metadata rows, not 19')
    models = {}
    for i in range(count):
        row = {f: get(template, f'.{ROW}_{i}.{f}') for f in FIELDS}
        models[row['upgrade_type'][1]] = row
    ops = []
    for token, (_teams, _supplier, _works, tyres) in rnd.TOKENS.items():
        table = f'{rnd.PREFIX}.trees.{token}.upgrade_type_metadata'
        for k, t in enumerate(types):
            model = models[t['model']]
            for field in FIELDS:
                tag, value = model[field]
                value = {
                    '_metadata_': t['label'],
                    'upgrade_type': t['type'],
                    'upgrade_department': 'Fake',
                    'mass_adjustment_fully_upgraded': '0',
                    'bhp_adjustment_fully_upgraded': '0',
                    'affect_on_vehicle': AFFECT_1990[tyres].get(t['type'], t['affect']),
                    'percentage_worth': '1',
                }.get(field, value)
                ops.append({'op': 'set', 'select': f'{table}.{ROW}_{count + k}', 'field': field, 'value': value,
                            'create': True, 'type': tag})
        ops.append({'op': 'set', 'select': table, 'field': COUNT_FIELD, 'value': str(count + len(types)),
                    'create': True, 'type': 0})
    with open(os.path.join(out, DB_OPS_1990), 'w') as f:
        json.dump({'schema_version': 1, 'ops': ops}, f, indent=1)
        f.write('\n')
    return [f'upgrade_type_metadata 1990: {len(rnd.TOKENS)} {rnd.PREFIX} tables, rows {count} -> {count + len(types)}, '
            f'{len(ops)} ops']


def op_key(op):
    import gamedb
    sel = op['select']
    prefix = int(sel[1:17], 16)
    return gamedb.fnv1a64(sel[18:] + '.' + op['field'], prefix)


def verify_db(data, recs, ops, containers, types):
    """Apply the ops to a copy of the stock gamedb, write it, read it back and check every row."""
    import gamedb
    stock = {r.key: (r.type, r.value) for r in recs}
    new = {r.key: gamedb.Record(r.key, r.type, r.value) for r in recs}
    for op in ops:
        k = op_key(op)
        if k in new:
            if op['field'] != COUNT_FIELD:
                return [f'verify: {op["select"]}.{op["field"]} collides with a stock key']
            new[k].value = op['value'].encode()
        else:
            new[k] = gamedb.Record(k, op['type'], op['value'].encode())
    blob = gamedb.write(list(new.values()))
    back = gamedb.read(blob)
    if gamedb.write(back) != blob:
        return ['verify: the patched gamedb does not round-trip']
    got = {r.key: (r.type, r.value) for r in back}
    changed = [k for k, v in stock.items() if got.get(k) != v]
    counts = {gamedb.fnv1a64('.' + COUNT_FIELD, p) for p in containers}
    problems = [f'verify: stock key {k:016x} changed' for k in changed if k not in counts]
    for p in containers:
        n = int(got[gamedb.fnv1a64('.' + COUNT_FIELD, p)][1])
        if n != 19 + len(types):
            problems.append(f'verify: {{{p:016x}}} count {n}')
        for k, t in enumerate(types):
            row = {f: got.get(gamedb.fnv1a64(f'.{ROW}_{19 + k}.{f}', p)) for f in FIELDS}
            if None in row.values() or row['upgrade_type'][1].decode() != t['type']:
                problems.append(f'verify: {{{p:016x}}}.{ROW}_{19 + k} incomplete')
    extra = len(got) - len(stock)
    want = len(containers) * len(types) * len(FIELDS)
    if extra != want:
        problems.append(f'verify: {extra} new records, expected {want}')
    return problems or [f'verify: patched gamedb round-trips; {extra} new records, {len(containers)} counts '
                        f'19 -> {19 + len(types)}, no other stock record changed']


def files(root):
    found = set()
    for rel in (OPS, DB_OPS, DB_OPS_1990):
        if os.path.isfile(os.path.join(root, rel)):
            found.add(rel)
    for d, _dirs, names in os.walk(os.path.join(root, VTF_DIR)):
        found |= {os.path.relpath(os.path.join(d, n), root).replace(os.sep, '/') for n in names}
    return found


def main():
    ap = argparse.ArgumentParser(description='Build the Tyres upgrade-type VTF blocks and metadata rows.')
    common.add_game_arg(ap)
    ap.add_argument('--no-pu', action='store_true', help='leave out Power Unit Cooling (type 22)')
    ap.add_argument('--work', default=os.path.join(common.BUILD, 'vtf'), help='work folder (default: build/vtf)')
    ap.add_argument('--write', action='store_true', help='replace the shipped vtf/, erp/rnd_upgrade_vtf.json, '
                    'db/upgrade_types.json and db/upgrade_types_1990.json')
    a = ap.parse_args()
    types = TYPES + ([] if a.no_pu else [PU])
    shutil.rmtree(a.work, ignore_errors=True)
    os.makedirs(os.path.join(a.work, 'erp'))
    os.makedirs(os.path.join(a.work, 'db'))
    report, found = build_vtf(a.game, a.work, types)
    for line in report:
        print(line)
    problems, archives, checked = verify(a.game, a.work, found, types)
    print(f'verify: {checked} VTF copies in {len(archives)} archives re-read; {len(problems)} problem(s)')
    for p in problems:
        print('  ', p)
    if problems:
        return 1
    db_report = build_db(a.game, a.work, types)
    for line in db_report:
        print(line)
    if any(line.startswith('verify: ') and 'round-trips' not in line for line in db_report):
        return 1
    if a.write:
        shutil.rmtree(os.path.join(common.ADDON, VTF_DIR), ignore_errors=True)
        shutil.copytree(os.path.join(a.work, VTF_DIR), os.path.join(common.ADDON, VTF_DIR))
        for rel in (OPS, DB_OPS, DB_OPS_1990):
            shutil.copyfile(os.path.join(a.work, rel), os.path.join(common.ADDON, rel))
        print(f'wrote {VTF_DIR}/, {OPS}, {DB_OPS} and {DB_OPS_1990}')
        return 0
    built, shipped = files(a.work), files(common.ADDON)
    differ = [r for r in sorted(built & shipped)
              if open(os.path.join(a.work, r), 'rb').read() != open(os.path.join(common.ADDON, r), 'rb').read()]
    missing, extra = sorted(built - shipped), sorted(shipped - built)
    print(f'{len(built & shipped) - len(differ)} same, {len(differ)} differ, {len(missing)} not shipped, '
          f'{len(extra)} only shipped')
    for r in differ + missing + extra:
        print('  ', r)
    return 1 if differ or missing or extra else 0


if __name__ == '__main__':
    sys.exit(main())
