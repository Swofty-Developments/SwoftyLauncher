#!/usr/bin/env python3
"""Build the Tyres & Thermal Management upgrade tree: db/tyres_tree.json and the upgrade
names and descriptions in assets/text.json.

Every upgrade is a full stock row (all 15 keys of a stock upgrade row, with their stock
type tags) written as `{hub}.upgrade_N` into each of the 46 `trees.<team_link>.fake`
containers. Row N carries the label `tyres_thermal_N`; the native hooks find our instances
by djb2("tyres_thermal_N_255"), so a row keeps its number whatever else changes. The
branch roots hang off the hub (`upgrade_0.links`, the stock targets kept), and
`_upgrade_instancecount_` becomes 1 + the number of rows. Costs, days, weighting and
facility levels follow the stock 2026 curve per size (DESIGN.md, "Stock calibration").

It also writes db/tyres_tree_1990.json: the same rows in the 12 `upgrades.1990.trees.<token>.fake`
hubs of the F1 1990 addon's career, with the 1990 one-season economy (SIZES_1990) and the hub
links of that addon's trees (its tools/rnd.py) plus our roots. Only an F1 1990 career reads
`upgrades.1990`.

It compares the build with the shipped files and exits 1 on a difference. --write replaces
them. --no-pu-cooling leaves out branch D (rows 13-15). --game checks the hub table, the
row keys and the grid cells against a stock database.
"""
import argparse
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ADDON = os.path.dirname(HERE)
REPO = os.path.dirname(os.path.dirname(ADDON))
TREE = os.path.join(ADDON, 'db', 'tyres_tree.json')
TEXT = os.path.join(ADDON, 'assets', 'text.json')
TREE_1990 = os.path.join(ADDON, 'db', 'tyres_tree_1990.json')
F1990_TOOLS = os.path.join(os.path.dirname(ADDON), 'f1-1990', 'tools')
DB_REL = os.path.join('game_package', 'gamedatabases', 'gamedatabases.2025.bin')

STR, ENUM, INT, FLOAT, LIST = 2, 3, 0, 1, 13

SIZES = {
    'Minor': {'cost': 500, 'cost_to_develop': 1000, 'research': 20, 'develop': 25, 'weighting': '1.0', 'facility': 0},
    'Major': {'cost': 1000, 'cost_to_develop': 1000, 'research': 30, 'develop': 25, 'weighting': '2', 'facility': 0},
    'Ultimate': {'cost': 1500, 'cost_to_develop': 1000, 'research': 40, 'develop': 25, 'weighting': '3', 'facility': 3},
}

SIZES_1990 = {
    'Minor': {'cost': 400, 'cost_to_develop': 1000, 'research': 7, 'develop': 7, 'weighting': '1.0', 'facility': 0},
    'Major': {'cost': 800, 'cost_to_develop': 1000, 'research': 14, 'develop': 10, 'weighting': '2', 'facility': 0},
    'Ultimate': {'cost': 1400, 'cost_to_develop': 1000, 'research': 24, 'develop': 14, 'weighting': '3', 'facility': 0},
}

BRANCHES = {
    'A': {'type': 'Tyre Thermal', 'first_row': 1, 'from': None},
    'B': {'type': 'Brake Cooling', 'first_row': 5, 'from': None},
    'C': {'type': 'Tyre Life', 'first_row': 9, 'from': None},
    'D': {'type': 'Power Unit Cooling', 'first_row': 13, 'from': 'B2'},
}
COLUMNS = ['A', 'B', 'D', 'C']
ORIGIN_X, ORIGIN_Y = 6, 5

UPGRADES = [
    ('A1', 'tt_thermal_model', 'Minor', 'Tyre Thermal Model',
     'A carcass and surface temperature model built from track sensor data.'),
    ('A2', 'tt_rim_heat_shield', 'Minor', 'Rim Heat Shielding',
     'Shields that stop brake heat soaking into the wheel rims.'),
    ('A3', 'tt_rim_cooling', 'Major', 'Wheel Rim Cooling Channels',
     'Channels in the rim that draw heat away from the tyre bead.'),
    ('A4', 'tt_carcass_heat', 'Ultimate', 'Carcass Heat Management',
     'A full thermal package that keeps the tyres in their window for longer.'),
    ('B1', 'bc_duct_inlet', 'Minor', 'Brake Duct Inlet Revision',
     'A reshaped inlet that feeds more cool air to the brakes.'),
    ('B2', 'bc_disc_ventilation', 'Minor', 'High-Density Disc Ventilation',
     'More, finer ventilation holes through the carbon discs.'),
    ('B3', 'bc_caliper_fins', 'Major', 'Caliper Cooling Fins',
     'Finned calipers that shed heat between braking zones.'),
    ('B4', 'bc_drum_package', 'Ultimate', 'Brake Drum Airflow Package',
     'New brake drums that route air across the disc and out through the wheel.'),
    ('C1', 'tl_sidewall_load', 'Minor', 'Sidewall Load Analysis',
     'Mapping how the sidewalls load up through each corner.'),
    ('C2', 'tl_pressure_telemetry', 'Minor', 'Tyre Pressure Telemetry',
     'Live pressure and temperature data for every tyre.'),
    ('C3', 'tl_camber_wear', 'Major', 'Camber Wear Optimisation',
     'Suspension settings that spread wear across the whole tread.'),
    ('C4', 'tl_low_deg_package', 'Ultimate', 'Low-Degradation Package',
     'A complete setup package for longer stints on every compound.'),
    ('D1', 'pc_radiator_core', 'Minor', 'Radiator Core Density',
     'A denser radiator core that rejects more heat from the power unit.'),
    ('D2', 'pc_cooling_louvres', 'Major', 'Cooling Exit Louvres',
     'Bodywork louvres that pull hot air out of the sidepods.'),
    ('D3', 'pc_integrated_thermal', 'Ultimate', 'Integrated Thermal Management',
     'One cooling system for power unit, brakes and tyres.'),
]

HUB_LINKS = {
    'stock': 'rear_downforce_minor_2,front_downforce_minor_2,chassis_weight_reduction_minor_1,'
             'gearbox_durability_1,engine_power_minor_2',
    'myteam_a': 'rear_downforce_minor_1,front_downforce_minor_1,chassis_weight_redistribution_minor_1,'
                'chassis_weight_reduction_minor_1,gearbox_durability_1,engine_power_minor_1',
    'myteam_b': 'rear_downforce_minor_1,front_downforce_minor_1,chassis_weight_redistribution_minor_1,'
                'chassis_weight_reduction_minor_1,gearbox_durability_1,engine_power_minor_2',
}

HUBS = [
    ('0afa1135080ece55', 'stock', None), ('0b5a4b784f6c4e9d', 'stock', None),
    ('1193b7ff643cf967', 'stock', 'haas'), ('1580249c9b4ec8a4', 'stock', 'tororosso'),
    ('1883f115061d56b4', 'stock', 'williams'), ('1bb7e8f77b5d19a3', 'stock', None),
    ('1eac0b50e896cbe8', 'stock', None), ('292496ac6a6d26ab', 'stock', None),
    ('319f79f5a8fdc483', 'stock', None), ('32b8ae69facc5b06', 'stock', 'gm_cadillac'),
    ('37abed8029bfd862', 'stock', None), ('3fe8c41197551682', 'stock', None),
    ('4cbce10f5f4eaa07', 'stock', None), ('5927ed801bdb6765', 'myteam_a', None),
    ('75a045fd77ae52b9', 'myteam_b', None), ('7d56c5a05b55e8c8', 'stock', None),
    ('83175129da4b9b8b', 'stock', None), ('858a2fcc895ed368', 'stock', 'mclaren'),
    ('87f986819f2e06b8', 'stock', None), ('8df9b0ddffbe61f9', 'stock', None),
    ('91a2cbfeae4a4299', 'stock', 'force_india'), ('9819a8e3b6ee2fd4', 'stock', None),
    ('9efe4874756bc851', 'stock', None), ('aabe2a214535b896', 'stock', None),
    ('ae8319a01aa30478', 'myteam_a', None), ('b5a2cf765b6fec7e', 'stock', None),
    ('b6c3fc092c3d5b15', 'stock', None), ('ba66c08a0b835444', 'stock', None),
    ('be58ab684bdf8b1f', 'stock', None), ('c644b7edb00170fa', 'stock', None),
    ('c75683b72baeba97', 'stock', None), ('d561c6b980a33b77', 'stock', 'ferrari'),
    ('db1c1b1bf0d7578b', 'stock', None), ('dbe76aee5b4ad408', 'stock', None),
    ('dca3732fc20f8344', 'stock', None), ('deae0310dc31dce2', 'stock', 'mercedes'),
    ('e17f7435c3d02852', 'stock', None), ('e3e5f221c2957166', 'stock', None),
    ('e5dbd73273f0d7b6', 'stock', None), ('e7ff59df0c38751e', 'stock', None),
    ('ebf057e46896f1c9', 'stock', None), ('f493d2525e10947d', 'stock', 'renault'),
    ('f608634c30e63c96', 'stock', 'sauber'), ('f71aa592926ce9d9', 'myteam_b', 'user_defined'),
    ('f86a71cba49235ae', 'stock', 'redbull'), ('ffb3229ba590374e', 'stock', None),
]

ROW_FIELDS = [
    ('_metadata_', STR), ('upgrade_name', STR), ('upgrade_type', ENUM), ('upgrade_size', ENUM),
    ('upgrade_department', ENUM), ('cost', INT), ('cost_to_develop', INT), ('weighting', FLOAT),
    ('unlock_at_facility_level', INT), ('supplier_delivery_date_index', INT), ('links', LIST),
    ('fitting_times.num_days_to_research', INT), ('fitting_times.num_days_to_develop', INT),
    ('grid_coordinates.x', FLOAT), ('grid_coordinates.y', FLOAT),
]
DEPARTMENTS = ['aerodynamics', 'chassis', 'durability', 'engine']


def label(row):
    return f'tyres_thermal_{row}'


def build_rows(pu_cooling):
    branches = [b for b in COLUMNS if pu_cooling or b != 'D']
    column = {b: ORIGIN_X + i for i, b in enumerate(branches)}
    rows = {}
    for code, name, size, display, description in UPGRADES:
        branch, depth = code[0], int(code[1:])
        if branch not in branches:
            continue
        info = BRANCHES[branch]
        rows[code] = {'code': code, 'row': info['first_row'] + depth - 1, 'name': name, 'size': size,
                      'type': info['type'], 'display': display, 'description': description,
                      'x': column[branch], 'links': []}
    for code, r in rows.items():
        branch, depth = code[0], int(code[1:])
        parent = BRANCHES[branch]['from'] if depth == 1 else f'{branch}{depth - 1}'
        r['parent'] = parent
    for code in sorted(rows, key=lambda c: rows[c]['row']):
        r = rows[code]
        r['y'] = ORIGIN_Y if r['parent'] is None else rows[r['parent']]['y'] + (1 if r['parent'][0] == code[0] else 0)
        if r['parent'] is not None:
            rows[r['parent']]['links'].append(label(r['row']))
    return sorted(rows.values(), key=lambda r: r['row'])


def roots(rows):
    return [label(r['row']) for r in rows if r['parent'] is None]


def row_values(r, sizes=SIZES):
    s = sizes[r['size']]
    return {
        '_metadata_': label(r['row']), 'upgrade_name': r['name'], 'upgrade_type': r['type'],
        'upgrade_size': r['size'], 'upgrade_department': 'Fake', 'cost': str(s['cost']),
        'cost_to_develop': str(s['cost_to_develop']), 'weighting': s['weighting'],
        'unlock_at_facility_level': str(s['facility']), 'supplier_delivery_date_index': '-1',
        'links': ','.join(r['links']), 'fitting_times.num_days_to_research': str(s['research']),
        'fitting_times.num_days_to_develop': str(s['develop']), 'grid_coordinates.x': str(r['x']),
        'grid_coordinates.y': str(r['y']),
    }


def op(select, field, value, tag):
    return {'op': 'set', 'select': select, 'field': field, 'value': value, 'create': True, 'type': tag}


def build_tree(rows):
    ops = []
    for h, links, _ in HUBS:
        hub = '{%s}' % h
        for r in rows:
            values = row_values(r)
            ops += [op(f'{hub}.upgrade_{r["row"]}', f, values[f], tag) for f, tag in ROW_FIELDS]
        ops.append(op(f'{hub}.upgrade_0', 'links', ','.join([HUB_LINKS[links]] + roots(rows)), LIST))
        ops.append(op(hub, '_upgrade_instancecount_', str(1 + len(rows)), INT))
    return json.dumps({'schema_version': 1, 'ops': ops}, indent=1) + '\n'


def f1990():
    if not os.path.isfile(os.path.join(F1990_TOOLS, 'rnd.py')):
        sys.exit(f'{F1990_TOOLS}/rnd.py not found: the 1990 tree needs the F1 1990 addon\'s tools next to this addon')
    sys.path.insert(0, F1990_TOOLS)
    import rnd
    return rnd


def build_tree_1990(rows, rnd):
    ops = []
    for token in rnd.TOKENS:
        hub = f'{rnd.PREFIX}.trees.{token}.fake'
        for r in rows:
            values = row_values(r, SIZES_1990)
            ops += [op(f'{hub}.upgrade_{r["row"]}', f, values[f], tag) for f, tag in ROW_FIELDS]
        ops.append(op(f'{hub}.upgrade_0', 'links', ','.join([rnd.hub_links()] + roots(rows)), LIST))
        ops.append(op(hub, '_upgrade_instancecount_', str(1 + len(rows)), INT))
    return json.dumps({'schema_version': 1, 'ops': ops}, indent=1) + '\n'


def check_1990(rows, rnd):
    taken = {tuple(float(v) for v in u.xy): u.label for u in rnd.UPGRADES}
    taken[(0.0, 0.0)] = 'default'
    return [f'1990 grid cell {r["x"]},{r["y"]} ({r["code"]}) is taken by {taken[(float(r["x"]), float(r["y"]))]}'
            for r in rows if (float(r['x']), float(r['y'])) in taken]


def is_upgrade_key(key, names):
    return any(key in (f'lng_rnd_{n}', f'lng_rnd_{n}_description') for n in names)


def build_text(rows, current):
    names = {n for _, n, *_ in UPGRADES} | {'tyres_wear_reduction', 'tyres_brake_cooling', 'tyres_thermal_reliability'}
    ops = [o for o in current['ops'] if not is_upgrade_key(o.get('key', ''), names)]
    for r in rows:
        ops.append({'op': 'lang.set', 'language': '*', 'key': f'lng_rnd_{r["name"]}', 'value': r['display']})
        ops.append({'op': 'lang.set', 'language': '*', 'key': f'lng_rnd_{r["name"]}_description',
                    'value': r['description']})
    lang_w = max(len(f'"language": {json.dumps(o["language"])},') for o in ops)
    key_w = max(len(f'"key": {json.dumps(o["key"])},') for o in ops)
    lines = []
    for o in ops:
        lang = f'"language": {json.dumps(o["language"])},'.ljust(lang_w)
        key = f'"key": {json.dumps(o["key"])},'.ljust(key_w)
        lines.append(f'    {{ "op": "lang.set", {lang} {key} "value": {json.dumps(o["value"], ensure_ascii=False)} }}')
    return '{\n  "schema_version": 1,\n  "ops": [\n' + ',\n'.join(lines) + '\n  ]\n}\n'


def load_db(path):
    sys.path.insert(0, os.path.join(REPO, 'tools'))
    import gamedb
    with open(path, 'rb') as f:
        recs = gamedb.read(f.read())
    return gamedb, {r.key: r.value.decode('utf-8', 'replace') for r in recs}


def check_db(path, rows):
    gamedb, db = load_db(path)
    problems = []
    fields = [f for f, _ in ROW_FIELDS]
    for h, links, team in HUBS:
        hub = int(h, 16)
        get = lambda suffix: db.get(gamedb.fnv1a64(suffix, hub))
        if team and gamedb.fnv1a64(f'upgrades.2026.trees.{team}.fake') != hub:
            problems.append(f'{h}: not upgrades.2026.trees.{team}.fake')
        if get('._upgrade_instancecount_') != '1':
            problems.append(f'{h}: _upgrade_instancecount_ is {get("._upgrade_instancecount_")!r}, not 1')
        if get('.upgrade_0._metadata_') != 'default':
            problems.append(f'{h}: upgrade_0 is not the default hub')
        if get('.upgrade_0.links') != HUB_LINKS[links]:
            problems.append(f'{h}: stock hub links are {get(".upgrade_0.links")!r}')
        for r in rows:
            for f in fields:
                if get(f'.upgrade_{r["row"]}.{f}') is not None:
                    problems.append(f'{h}: upgrade_{r["row"]}.{f} already exists')
        parent = gamedb.fnv1a64_unwind(hub, '.fake')
        taken = set()
        for dept in DEPARTMENTS:
            c = gamedb.fnv1a64('.' + dept, parent)
            n = db.get(gamedb.fnv1a64('._upgrade_instancecount_', c))
            if n is None:
                problems.append(f'{h}: no sibling {dept} tree')
                continue
            for i in range(int(n)):
                x = db.get(gamedb.fnv1a64(f'.upgrade_{i}.grid_coordinates.x', c))
                y = db.get(gamedb.fnv1a64(f'.upgrade_{i}.grid_coordinates.y', c))
                taken.add((float(x), float(y)))
        for r in rows:
            if (float(r['x']), float(r['y'])) in taken:
                problems.append(f'{h}: grid cell {r["x"]},{r["y"]} ({r["code"]}) is taken by a stock upgrade')
    return problems


def sketch(rows):
    cells = {(r['x'], r['y']): r['code'] for r in rows}
    xs = sorted({r['x'] for r in rows})
    ys = sorted({r['y'] for r in rows})
    out = ['       ' + ' '.join(f'x={x:<3}' for x in xs)]
    for y in ys:
        out.append(f'  y={y:<2} ' + ' '.join(f'{cells.get((x, y), "."):<5}' for x in xs))
    return '\n'.join(out)


def compare(path, text, write):
    old = None
    if os.path.isfile(path):
        with open(path, encoding='utf-8') as f:
            old = f.read()
    if old == text:
        return True
    if write:
        with open(path, 'w', encoding='utf-8') as f:
            f.write(text)
    return False


def main():
    ap = argparse.ArgumentParser(description='Build db/tyres_tree.json and the upgrade text in assets/text.json.')
    ap.add_argument('--write', action='store_true', help='replace the shipped files')
    ap.add_argument('--no-pu-cooling', action='store_true', help='leave out branch D (Power Unit Cooling)')
    ap.add_argument('--game', help='F1 25 folder or a stock gamedatabases.2025.bin to check against')
    args = ap.parse_args()

    rows = build_rows(not args.no_pu_cooling)
    if args.game:
        path = args.game if os.path.isfile(args.game) else os.path.join(args.game, DB_REL)
        problems = check_db(path, rows)
        for p in problems:
            print(p)
        if problems:
            return 1
        print(f'stock database: {len(HUBS)} hubs, {len(rows)} rows each, no key or grid collisions')

    rnd = f1990()
    problems = check_1990(rows, rnd)
    for p in problems:
        print(p)
    if problems:
        return 1

    with open(TEXT, encoding='utf-8') as f:
        text_now = json.load(f)
    outputs = [(TREE, build_tree(rows)), (TEXT, build_text(rows, text_now)), (TREE_1990, build_tree_1990(rows, rnd))]
    differ = [p for p, t in outputs if not compare(p, t, args.write)]
    print(f'{len(rows)} upgrades in {len(HUBS)} hubs, roots {", ".join(roots(rows))}')
    print(f'1990: {len(rows)} upgrades in {len(rnd.TOKENS)} {rnd.PREFIX} hubs, hub links {rnd.hub_links()} + our roots')
    print(sketch(rows))
    for p in differ:
        print(('wrote ' if args.write else 'differs: ') + os.path.relpath(p, REPO))
    return 0 if args.write or not differ else 1


if __name__ == '__main__':
    sys.exit(main())
