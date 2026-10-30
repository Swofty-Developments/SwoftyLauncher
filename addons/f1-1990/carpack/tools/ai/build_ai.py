"""Build the 1990 cars' own per-track AI archives from their 1990 handling files.

For every (track, car folder) the stock `f1_2026_vehicle_package` AI archive of that track and folder is the
template. Every resource moves to `f1_1990_vehicle_package`. The five car tables come from Swofty AI's car model on the
1990 handling VTF at the template's downforce variation, on the template's axes and row order; corner speeds are the
template's copd times the model's 1990/2026 corner speed ratio. The model's drag is fitted to the 2026 cars, which open
their wings on the straights (m_aero_wing_front_drag 0.5, m_aero_wing_rear_drag 0.3 with DRS hardware); the 1990 car
has none, so its model drag is scaled by the 2026 car's model drag over its open-wing drag and by the 1990 car's
closed-wing drag over its model drag, at the variation's wing angles (closed_wing_drag). vp_times, tyre_wear and fuel_burn_granular
are simulated with those tables on the track's stock line and brake line settings. The car has no ERS, so every ERS
mode row is the no-ERS row, ers_use_granular is zero everywhere and ers_boost_data carries no ERS use. vp_chars and
brakelinesettings stay the template's.

Outputs (relative to the car pack folder `carpack/`): erp/ai/<track>/<folder>.erp, erp/ai/index.json, erp/ai/laps.json,
erp/ai/checks.json, assets/ai.json, and a copy of laps.json at addons/f1-1990/data/ai-laps-1990.json.
"""
import argparse
import ast
import copy
import json
import os
import statistics
import sys
import time
from concurrent.futures import ProcessPoolExecutor, as_completed

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.dirname(os.path.dirname(HERE))
F1990 = os.path.dirname(PACK)
REPO = os.path.dirname(os.path.dirname(F1990))
SRC = os.path.relpath(PACK, F1990).replace(os.sep, '/')

SOURCE = 'f1_2026_vehicle_package'
TARGET = 'f1_1990_vehicle_package'
FOLDERS = ('mclaren', 'ferrari', 'williams', 'lotus', 'mercedes', 'redbull', 'force_india', 'myteam_ferrari', 'sauber',
           'toro_rosso', 'gm_cadillac', 'myteam_honda', 'haas')
TABLE_EXTS = ('acpd', 'brpd', 'copd', 'crpd', 'thpd')
PLAIN_VP_PACKAGE = 'f2_2026_vehicle_package'
SANITY_RADII = (50.0, 150.0, 300.0)
SANITY_TRACK = 'melbourne'
SANITY_FOLDERS = ('mclaren', 'haas')


def calendar_tracks():
    with open(os.path.join(F1990, 'tools', 'career.py'), encoding='utf-8') as f:
        tree = ast.parse(f.read())
    for node in tree.body:
        if isinstance(node, ast.Assign) and any(getattr(t, 'id', None) == 'ROUNDS' for t in node.targets):
            return [r[1] for r in ast.literal_eval(node.value)]
    raise SystemExit('addons/f1-1990/tools/career.py has no ROUNDS')


def erp_rel(track, folder):
    return f'erp/ai/{track}/{folder}.erp'


def target_path(track, folder, package=TARGET):
    return f'2025_asset_groups/environment_package/tracks/{track}/ai/{package}/{folder}/{folder}.erp'


def group_path(track, folder, package=TARGET):
    return f'asset_groups/environment_package/tracks/{track}/ai/{package}/{folder}/{folder}.assetgroup'


def index_entry(track, folder):
    return {'track': track, 'folder': folder, 'erp': erp_rel(track, folder), 'target': target_path(track, folder),
            'group': group_path(track, folder), 'template': group_path(track, folder, SOURCE)}


def ops(tracks, folders):
    out = []
    for t in tracks:
        for f in folders:
            e = index_entry(t, f)
            out.append({'op': 'file.put', 'source': f"{SRC}/{e['erp']}", 'target': e['target']})
            out.append({'op': 'group.register', 'group': e['group'], 'erp': e['target'], 'template': e['template']})
    return {'schema_version': 1, 'ops': out}


def missing_archives():
    with open(os.path.join(PACK, 'assets', 'ai.json'), encoding='utf-8') as f:
        doc = json.load(f)
    return [o['source'] for o in doc['ops'] if o['op'] == 'file.put' and not os.path.isfile(os.path.join(F1990, o['source']))]


def stock_archive(game, track, folder, package=SOURCE):
    return os.path.join(game, '2025_asset_groups', 'environment_package', 'tracks', track, 'ai', package, folder,
                        f'{folder}.erp')


def handling_vtf(handling_dir, folder):
    import erpw
    path = os.path.join(handling_dir, f'{folder}_handling.erp')
    e = erpw.Erp(path)
    for r in e.res:
        if r.name.endswith('.vtf'):
            return e.frag_data(r, 0), path
    raise SystemExit(f'{path}: no handling VTF')


def compounds_1990(game, car_pack_compounds=None):
    from swofty_ai import carmodel, source
    if car_pack_compounds and os.path.exists(car_pack_compounds):
        a = source.Archive(car_pack_compounds)
        for n in a.names('.vcmpd'):
            return carmodel.load_compounds(a.read(n)), car_pack_compounds
    comp = carmodel.load_package_compounds(carmodel._stock_resolver(game), SOURCE)
    renamed = {k.replace('F1_2026_', 'F1_1990_', 1): v for k, v in comp.items()}
    return renamed, f'{SOURCE} stock compounds renamed F1_2026_ -> F1_1990_'


def wing_drag_open(game, package, folder):
    from swofty_ai import carmodel
    v = carmodel.Vtf(carmodel.load_vtf(carmodel._stock_resolver(game), package, folder)[0])
    if not v.f('F1Aero/DRSRotationAngleRadians', 0.0):
        return 1.0, 1.0
    return v.f('F1Aero/m_aero_wing_front_drag', 1.0), v.f('F1Aero/m_aero_wing_rear_drag', 1.0)


def straight_drag(cfg, cal, variation, open_wings):
    from swofty_ai import carmodel
    wf, wr = carmodel.setup_wings(cal, variation)
    body = cfg.body_drag_min + cal['body_drag'] * (cfg.body_drag_max - cfg.body_drag_min)
    front = carmodel._poly(cfg.wing_front['drag'], wf)
    rear = carmodel._poly(cfg.wing_rear['drag'], wr)
    model = body + cal.get('wing_drag', 1.0) * (front + rear)
    game = body + open_wings[0] * front + open_wings[1] * rear
    return model, game


def closed_wing_drag(game, cfg, cfg26, folder, variation):
    from swofty_ai import carmodel
    cal = carmodel.CALIBRATION[cfg26.family]
    model26, game26 = straight_drag(cfg26, cal, variation, wing_drag_open(game, SOURCE, folder))
    model90, game90 = straight_drag(cfg, cal, variation, (1.0, 1.0))
    return round(model26 / game26 * game90 / model90, 4)


def car_config(game, handling_dir, folder, car_pack_compounds=None):
    from swofty_ai import carmodel
    data, _ = handling_vtf(handling_dir, folder)
    comp, _ = compounds_1990(game, car_pack_compounds)
    cfg = carmodel.CarConfig.from_vtf(data, comp, SOURCE, folder)
    cfg.package = TARGET
    if cfg.has_ers:
        raise SystemExit(f'{folder}: the handling file has an ERS (MaxDeployPower set); expected the no-ERS 1990 car')
    if not cfg.tyre_front or not cfg.tyre_rear:
        raise SystemExit(f'{folder}: tyre compounds for TyrePhysicsGroup not found')
    return cfg


def relabel_vp_masses(blob, mid_mass, fuel):
    from swofty_ai import vp
    blocks = vp.parse(blob)
    masses = sorted({round(float(s.mass), 3) for s in blocks['track'].sets})
    if len(masses) != 3:
        return blob, None
    lo, mid, hi = masses
    new = {lo: mid_mass * lo / mid, mid: mid_mass, hi: mid_mass + fuel}
    for b in blocks.values():
        for s in b.sets:
            s.mass = new[min(new, key=lambda m: abs(m - float(s.mass)))]
    return vp.write(blocks), {str(k): round(v, 3) for k, v in new.items()}


def fit_count(blob, template):
    from swofty_ai import vp
    import numpy as np
    got, ref = vp.parse(blob), vp.parse(template)
    for name, b in got.items():
        r = ref.get(name)
        if r is None:
            continue
        for s, rs in zip(b.sets, r.sets):
            n = len(rs.times)
            t = np.asarray(s.times, float)
            if len(t) >= n:
                t = t[:n]
            else:
                step = t[-1] - t[-2] if len(t) > 1 else 0.0
                t = np.concatenate([t, t[-1] + step * np.arange(1, n - len(t) + 1)])
            s.times = t
    return vp.write(got)


def zero_ers_use(blob):
    from pkg import Pkg, Val
    t = Pkg(blob)
    for node in t.root.get('ers_mode_list').items:
        arr = node.get('ers_use_samples')
        arr.items = [Val(arr.items[0].type if arr.items else 'fp32', 0.0) for _ in arr.items]
    return t.to_bytes()


def no_ers_boost(blob, k_time, k_fuel):
    from pkg import Arr, Pkg, Val
    t = Pkg(blob)

    def put(node, key, fn):
        v = node.get(key)
        if v is None:
            return
        if isinstance(v, Arr):
            v.items = [Val(x.type, fn(float(x.value))) for x in v.items]
        elif v.type == 'stri':
            node.set(key, Val('stri', f'{fn(float(v.value)):.6f}'))
        else:
            node.set(key, Val(v.type, fn(float(v.value))))

    put(t.root, 'baseLapTime', lambda x: x * k_time)
    put(t.root, 'baseFuelUsage', lambda x: x * k_fuel)
    put(t.root, 'baseERS', lambda x: 0.0)
    for e in t.root.get('ers_boost_list').items:
        put(e, 'lap_time_gain', lambda x: x * k_time)
        put(e, 'fuelUsage', lambda x: x * k_fuel)
        put(e, 'ers_delta_recorded', lambda x: 0.0)
        put(e, 'ers_burn_aim', lambda x: 0.0)
        put(e, 'ers_amount', lambda x: 0.0)
    return t.to_bytes()


def _sets_of(blob):
    from swofty_ai import cartables
    return cartables.Table(blob).sets


def job(args):
    track, folder, game, handling_dir, out_root, pit_kmh, car_pack_compounds = args
    t0 = time.time()
    import erpw
    import numpy as np
    from swofty_ai import aispline, brakes, carmodel, cartables, laps, sim, source, strategy, vp

    tpl_path = stock_archive(game, track, folder)
    e = erpw.Erp(tpl_path)
    by_ext = {r.name.rsplit('.', 1)[1]: r for r in e.res}
    templates = {ext: e.frag_data(r, 0) for ext, r in by_ext.items() if len(r.frags) and ext != 'brakelinesettings'}
    variation = cartables.Table(templates['copd']).variation
    cfg = car_config(game, handling_dir, folder, car_pack_compounds)
    cfg26 = carmodel.CarConfig.from_game(game, SOURCE, folder)
    drag_k = closed_wing_drag(game, cfg, cfg26, folder, variation)
    cfg = cfg.with_overrides(drag=drag_k)
    tpl_sets = {x: _sets_of(templates[x]) for x in TABLE_EXTS}
    axes = template_axes(cfg, tpl_sets)
    rows = carmodel.build_tables(cfg, variation, axes, full=True)
    raw90 = sim.CarTables.from_sets(variation, {x: copy.deepcopy(rows[x]) for x in ('acpd', 'brpd', 'copd')}, folder)
    ea26 = {x: tpl_sets[x] for x in ('acpd', 'brpd', 'copd')}
    rows['copd'], copd_shift = corrected_copd(rows['copd'], cfg, cfg26, ea26['copd'], variation)
    rows = {x: mirror(x, rows[x], tpl_sets[x]) for x in TABLE_EXTS}
    table_bytes = {x: carmodel.serialise(templates[x], rows[x], folder, variation) for x in TABLE_EXTS}
    sets90 = {x: _sets_of(table_bytes[x]) for x in TABLE_EXTS}
    t90 = sim.CarTables.from_sets(variation, sets90, folder)
    t26 = sim.CarTables.from_sets(variation, ea26, folder)
    m26 = sim.CarTables.from_sets(variation, carmodel.build_tables(cfg26, variation), folder)

    ts = source.Game(game).track(track, cars=[(SOURCE, folder)])
    fr = ts.track_fragments(source.AISPLINE)
    spl = aispline.AiSpline(fr[0], fr[1] if len(fr) > 1 else None)
    route = spl.route('track')
    line = np.array([spl.gates[g].pos for g in route])
    bfr = ts.car_fragments(SOURCE, folder, 'brakelinesettings')
    settings = brakes.BrakeSettings(bfr[0], bfr[1] if len(bfr) > 1 else None)
    lines = laps.section(settings)
    gate_s, length = sim.stations_of(line)

    def corner_mod(grip, mass):
        mod, _, _ = laps.modifiers(spl, route, gate_s, length, True, lines, grip, laps.mass_bias_of(t90, mass))
        return mod

    lap90 = laps.flying(spl, t90, settings, grip=1.0)
    lap26 = laps.flying(spl, t26, settings, grip=1.0)
    lap90_raw = laps.flying(spl, raw90, settings, grip=1.0)['lapTime']
    lap26_model = laps.flying(spl, m26, settings, grip=1.0)['lapTime']
    plain26 = sim.simulate(line, t26, ers_mode='medium').lap_time
    plain90 = sim.simulate(line, t90, ers_mode='medium').lap_time

    out = dict(table_bytes)
    vp_tpl, vp_masses = relabel_vp_masses(templates['vp_times'], cfg.mass, cfg.fuel_capacity or 110.0)
    pit_in = vp.pit_inputs(ts)
    pit = None
    if pit_in:
        setup = laps.pit_setup(spl, lines, 1.0, 0.0, pit_kmh)
        if setup:
            n_pit = len(vp.parse(templates['vp_times'])['pit'].sets[0].times)
            pit = {'setup': setup, 'start': pit_in['start'], 'samples': n_pit}
    out['vp_times'] = fit_count(vp.build(vp_tpl, line, t90, corner_mod=corner_mod, pit=pit, package=PLAIN_VP_PACKAGE),
                                templates['vp_times'])
    fb = templates['fuel_burn_granular']
    out['fuel_burn_granular'] = strategy.build_fuel_burn(
        fb, strategy.fuel_entries(line, t90, SOURCE, strategy._template_masses(fb, 'fuel_mix_list')))
    out['tyre_wear'] = strategy.build_tyre_wear(templates['tyre_wear'], line, t90, SOURCE, {})
    out['ers_use_granular'] = zero_ers_use(templates['ers_use_granular'])
    mass_hi = t90.masses()[-1]
    r90 = sim.simulate(line, t90, mass=mass_hi, ers_mode='medium')
    r26 = sim.simulate(line, t26, mass=t26.masses()[-1], ers_mode='medium')
    k_time = r90.lap_time / r26.lap_time
    k_fuel = strategy.fuel_curve(r90, t90, SOURCE)[-1] / max(strategy.fuel_curve(r26, t26, SOURCE)[-1], 1e-9)
    out['ers_boost_data'] = no_ers_boost(templates['ers_boost_data'], k_time, k_fuel)
    out['vp_chars'] = templates['vp_chars']
    for ext in ('fuel_burn_granular', 'tyre_wear', 'ers_use_granular', 'ers_boost_data', 'vp_chars'):
        out[ext] = strategy.rename(out[ext], track, folder)

    old = f'/ai/{SOURCE}/{folder}/'
    new = f'/ai/{TARGET}/{folder}/'
    kept = []
    for r in e.res:
        ext = r.name.rsplit('.', 1)[1]
        r.name = r.name.replace(old, new)
        if ext in out:
            erpw_set(e, r, out[ext])
        else:
            kept.append(ext)
    dst = os.path.join(out_root, track, f'{folder}.erp')
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    tmp = dst + '.tmp'
    e.to_file(tmp)
    os.replace(tmp, dst)

    sanity = None
    if (track, folder) in {(SANITY_TRACK, f) for f in SANITY_FOLDERS}:
        sanity = {'1990': table_sanity(t90), '2026': table_sanity(t26), '1990_uncorrected': table_sanity(raw90),
                  '2026_model': table_sanity(m26)}
    return {'track': track, 'folder': folder, 'variation': variation, 'lap1990': lap90['lapTime'],
            'lap2026': lap26['lapTime'], 'lap1990_uncorrected': lap90_raw, 'lap2026_model': lap26_model,
            'copd_shift': copd_shift, 'lap1990_no_mods': plain90, 'lap2026_no_mods': plain26,
            'vmax1990': lap90['vmax'] * 3.6, 'vmax2026': lap26['vmax'] * 3.6, 'length': lap90['length'],
            'masses1990': t90.masses(), 'vp_masses': vp_masses, 'kept_from_template': kept,
            'k_time_heavy': k_time, 'k_fuel_heavy': k_fuel, 'bytes': os.path.getsize(dst), 'seconds': time.time() - t0,
            'sanity': sanity, 'mass_vtf': cfg.mass, 'drag_k': drag_k}


def _masses(rows):
    return sorted({float(r['mass']) for r in rows})


def template_axes(cfg, tpl_sets):
    from swofty_ai import carmodel
    tables = {x: _Sets(v) for x, v in tpl_sets.items()}
    axes = carmodel.axes_from_tables(tables)
    d = carmodel.default_axes(cfg)
    axes['mass'] = d['mass']
    same = _masses(tpl_sets['thpd']) == _masses(tpl_sets['acpd'])
    axes['thpd_mass'] = d['mass'] if same else d['thpd_mass']
    return axes


class _Sets:
    def __init__(self, sets):
        self.sets = sets

    def axes(self):
        out = {}
        for s in self.sets:
            for k, v in s.items():
                if not isinstance(v, list):
                    out.setdefault(k, set()).add(v)
        return {k: sorted(v) for k, v in out.items()}


def _key(ext, row, masses, effs):
    wet = row.get('weather_tyre', 'Dry')
    grip = round(float(row['grip']), 4)
    mi = masses.index(float(row['mass']))
    if ext == 'acpd':
        ee = row.get('engine_efficiency')
        return wet, grip, mi, row.get('fuel_map'), row.get('ers_mode'), None if ee is None else effs.index(float(ee))
    if ext == 'brpd':
        r = round(float(row.get('radius', 0.0)), 3)
        return wet, grip, mi, r, round(float(row['brakelimit']), 3) if r == 0.0 and 'brakelimit' in row else None
    if ext == 'thpd':
        return grip, mi
    return wet, grip, mi


def mirror(ext, gen_rows, tpl_rows):
    gm, tm = _masses(gen_rows), _masses(tpl_rows)
    ge = sorted({float(r['engine_efficiency']) for r in gen_rows if r.get('engine_efficiency') is not None})
    te = sorted({float(r['engine_efficiency']) for r in tpl_rows if r.get('engine_efficiency') is not None})
    index = {_key(ext, r, gm, ge): r for r in gen_rows}
    out = []
    for t in tpl_rows:
        k = _key(ext, t, tm, te)
        g = index.get(k)
        if g is None:
            raise KeyError(f'{ext}: no generated row for template row {k}')
        out.append({f: (g[f] if f in g else t[f]) for f in t})
    return out


def corrected_copd(rows90, cfg90, cfg26, ea_rows, variation, sat=129.5):
    import numpy as np
    from swofty_ai import carmodel
    ea = {}
    for s in ea_rows:
        ea.setdefault((s.get('weather_tyre', 'Dry'), round(float(s['grip']), 4)), []).append(s)
    m90 = sorted({float(r['mass']) for r in rows90})
    shifts = []
    out = []
    for row in rows90:
        wet, grip = row['weather_tyre'], float(row['grip'])
        cands = sorted(ea[(wet, round(grip, 4))], key=lambda s: s['mass'])
        er = cands[min(m90.index(float(row['mass'])), len(cands) - 1)]
        radii = np.asarray(row['distanceList'], float)
        c90 = carmodel.Car(cfg90, variation, float(row['mass']), grip, wet == 'Wet')
        c26 = carmodel.Car(cfg26, variation, float(er['mass']), grip, wet == 'Wet')
        u90 = c90.corner_speeds(radii, 1e9)
        u26 = c26.corner_speeds(radii, 1e9)
        ok = (u90 < sat) & (u26 < sat)
        ratio = np.where(ok, u90 / np.maximum(u26, 1e-6), np.nan)
        last = 1.0
        for i in range(len(ratio)):
            if np.isnan(ratio[i]):
                ratio[i] = last
            else:
                last = ratio[i]
        base = np.interp(radii, np.asarray(er['distanceList'], float), np.asarray(er['speedList'], float))
        vmax = c90.top_speed(c90.accel())
        v = np.maximum.accumulate(np.minimum(base * ratio, vmax))
        shifts.append(float(np.median(v / np.maximum(np.asarray(row['speedList'], float), 1e-6))))
        out.append(dict(row, speedList=v.tolist()))
    return out, round(float(np.median(shifts)), 4)


def erpw_set(e, r, data):
    import hashlib
    import erpw
    fr = r.frags[0]
    fr.raw = erpw.zstd_c(data) if fr.comp == erpw.ZSTD else data
    fr.size, fr.packed = len(data), len(fr.raw)
    r.hash = hashlib.md5(b''.join(e.frag_data(r, j) for j in range(len(r.frags)))).digest()


def table_sanity(t):
    import numpy as np
    rad, vc = t.corner(False, 1.0, t.masses()[1])
    corner = {str(int(r)): round(float(np.interp(r, rad, vc)) * 3.6, 1) for r in SANITY_RADII}
    rows = [s for s in t.sets['acpd'] if s.get('weather_tyre', 'Dry') == 'Dry' and abs(s['grip'] - 1.0) < 1e-6
            and s.get('fuel_map') == 'standard' and s.get('ers_mode') == 'none'
            and abs(float(s.get('engine_efficiency', 1.0) or 1.0) - 1.0) < 1e-3]
    mid = t.masses()[1]
    row = min(rows, key=lambda s: abs(s['mass'] - mid))
    v = np.asarray(row['speedList']) * 3.6
    tl = np.asarray(row['timeList'])
    t200 = float(np.interp(200.0, v, tl)) if v[-1] >= 200 else None
    return {'corner_kmh': corner, 'zero_200_s': None if t200 is None else round(t200, 2),
            'top_kmh': round(float(v[-1]), 1), 'mass': round(float(row['mass']), 1)}


def check_archive(path, tpl_path, track, folder):
    import erp as erpmod
    from swofty_ai import cartables
    import numpy as np
    meta, res = erpmod.parse(path)
    _, tres = erpmod.parse(tpl_path)
    got = {n: t for n, t, *_ in res}
    want = {n.replace(f'/ai/{SOURCE}/{folder}/', f'/ai/{TARGET}/{folder}/'): t for n, t, *_ in tres}
    issues = []
    if got != want:
        issues.append(f'resource set/types differ: missing {sorted(set(want) - set(got))}, extra {sorted(set(got) - set(want))}')
    if any(f'/{SOURCE}/' in n for n in got):
        issues.append('a resource still names f1_2026_vehicle_package')
    from swofty_ai import source
    a = source.Archive(path)
    for ext in TABLE_EXTS:
        n = f'eaid://environment_package/tracks/{track}/ai/{TARGET}/{folder}/{folder}.{ext}'
        t = cartables.Table(a.read(n))
        if not t.sets:
            issues.append(f'{ext}: no sets')
        for s in t.sets:
            sp = np.asarray(s.get('speedList', []), float)
            if ext == 'copd' and np.any(np.diff(sp) < -0.05):
                issues.append(f'copd non-monotonic (grip {s.get("grip")}, mass {s.get("mass")}, {s.get("weather_tyre")})')
                break
            if ext == 'acpd' and np.any(np.diff(sp) <= 0):
                issues.append(f'acpd speed not increasing (grip {s.get("grip")}, mass {s.get("mass")})')
                break
            if ext == 'brpd' and np.any(np.diff(sp) > 1e-6):
                issues.append(f'brpd speed not decreasing (radius {s.get("radius")}, brakelimit {s.get("brakelimit")})')
                break
    acpd = cartables.Table(a.read(f'eaid://environment_package/tracks/{track}/ai/{TARGET}/{folder}/{folder}.acpd')).sets
    groups = {}
    for s in acpd:
        k = (s.get('weather_tyre'), s['grip'], s['mass'], s.get('fuel_map'), s.get('engine_efficiency'))
        groups.setdefault(k, {})[s.get('ers_mode')] = (tuple(np.round(s['speedList'], 4)), tuple(np.round(s['distanceList'], 3)))
    ers_diff = sum(1 for g in groups.values() if len(set(g.values())) > 1)
    if ers_diff:
        issues.append(f'acpd: {ers_diff} row groups where an ERS mode differs from none')
    ers_modes = sorted({s.get('ers_mode') for s in acpd})
    return {'issues': issues, 'resources': len(got), 'ers_modes': ers_modes, 'acpd_rows': len(acpd)}


def _check_job(a):
    return check_archive(*a)


def write_json(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    tmp = path + '.tmp'
    with open(tmp, 'w', encoding='utf-8') as f:
        f.write(json.dumps(obj, indent=1) + '\n')
    os.replace(tmp, path)


def laps_doc(results, tracks, folders):
    doc = {'description': 'Simulated flying laps (s) on the stock AI line with the stock brake line settings: grip 1.0, '
                          "the tables' middle mass, difficulty 1.0, standard fuel map, medium ERS (the 1990 car has "
                          'none). lap1990: the generated 1990 tables (handling removed_2026_1990). lap2026: the stock '
                          'tables of the same folder at that track. ratio = lap1990 / lap2026. model_ratio: both cars '
                          'from the car model (1990 before the copd correction against the 2026 car), for comparison.',
           'handling': 'removed_2026_1990', 'tracks': {}}
    for t in tracks:
        cars = {f: results[(t, f)] for f in folders if (t, f) in results}
        if not cars:
            continue
        ratios = [c['lap1990'] / c['lap2026'] for c in cars.values()]
        mratios = [c['lap1990_uncorrected'] / c['lap2026_model'] for c in cars.values()]
        fastest = min(cars.items(), key=lambda kv: kv[1]['lap1990'])
        first = next(iter(cars.values()))
        doc['tracks'][t] = {
            'cars': {f: {'lap1990': round(c['lap1990'], 3), 'lap2026': round(c['lap2026'], 3),
                         'ratio': round(c['lap1990'] / c['lap2026'], 5),
                         'model_ratio': round(c['lap1990_uncorrected'] / c['lap2026_model'], 5)}
                     for f, c in cars.items()},
            'summary': {'cars': len(cars), 'median_ratio': round(statistics.median(ratios), 5),
                        'median_model_ratio': round(statistics.median(mratios), 5),
                        'fastest_1990': {'folder': fastest[0], 'lap': round(fastest[1]['lap1990'], 3)},
                        'fastest_2026': round(min(c['lap2026'] for c in cars.values()), 3),
                        'variation': first['variation'], 'length_m': round(first['length'], 1)}}
    return doc


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    ap.add_argument('--game', default='/mnt/gaming/SteamLibrary/steamapps/common/F1 25',
                    help='F1 25 folder (read only) with the stock track AI archives')
    ap.add_argument('--swofty-ai', default=os.path.join(REPO, 'tools', 'swofty-ai'),
                    help='the tools/swofty-ai folder (feat/swofty-ai)')
    ap.add_argument('--handling', default=os.path.join(F1990, 'cars', 'handling', 'removed_2026_1990'),
                    help='folder with the 1990 <folder>_handling.erp files')
    ap.add_argument('--tracks', help='comma-separated tracks (default: the 16 calendar tracks)')
    ap.add_argument('--folders', help='comma-separated car folders (default: the 13)')
    ap.add_argument('--out', default=os.path.join(PACK, 'erp', 'ai'))
    ap.add_argument('--compounds', default=os.path.join(F1990, 'tyres', 'historical', 'tyrecompounds.erp'),
                    help="the 1990 tyre compounds (F1_1990 Goodyear and F1_1990P Pirelli, tools/tyres.py); the stock "
                         "2026 ones renamed F1_1990 are used when the file is missing")
    ap.add_argument('--workers', type=int, default=12)
    ap.add_argument('--ops-only', action='store_true', help='write assets/ai.json and stop')
    ap.add_argument('--report-only', action='store_true',
                    help='rebuild index.json, laps.json and checks.json from erp/ai/results.json without generating')
    args = ap.parse_args()

    all_tracks = calendar_tracks()
    tracks = args.tracks.split(',') if args.tracks else all_tracks
    folders = args.folders.split(',') if args.folders else list(FOLDERS)
    write_json(os.path.join(PACK, 'assets', 'ai.json'), ops(all_tracks, FOLDERS))
    if args.ops_only:
        return 0

    sa = os.path.abspath(args.swofty_ai)
    if not os.path.isdir(os.path.join(sa, 'swofty_ai')):
        raise SystemExit(f'{sa}: not a tools/swofty-ai folder')
    sys.path.insert(0, sa)
    import swofty_ai  # noqa: F401
    from swofty_ai import source, trackdb

    missing = [(t, f) for t in tracks for f in folders if not os.path.exists(stock_archive(args.game, t, f))]
    if missing:
        raise SystemExit(f'no stock {SOURCE} AI archive for {missing[:5]} in {args.game}')
    for f in folders:
        if not os.path.exists(os.path.join(args.handling, f'{f}_handling.erp')):
            raise SystemExit(f'{args.handling}: no {f}_handling.erp')
    try:
        db = trackdb.TrackDB.from_game(args.game)
    except OSError:
        db = None
    pit = {t: trackdb.pit_speed_limit(db, t, SOURCE)[0] for t in tracks}

    out_root = os.path.abspath(args.out)
    index_path = os.path.join(out_root, 'index.json')
    laps_path = os.path.join(out_root, 'laps.json')
    results = {}
    if os.path.exists(os.path.join(out_root, 'results.json')):
        with open(os.path.join(out_root, 'results.json')) as f:
            for r in json.load(f):
                if os.path.exists(os.path.join(out_root, r['track'], f'{r["folder"]}.erp')):
                    results[(r['track'], r['folder'])] = r
    jobs = [(t, f, args.game, os.path.abspath(args.handling), out_root, pit[t], args.compounds)
            for t in tracks for f in folders if not args.report_only]
    order = {(t, f): i for i, (t, f) in enumerate((t, f) for t in all_tracks for f in FOLDERS)}

    def flush():
        done = sorted(results, key=lambda k: order.get(k, 1e9))
        write_json(index_path, [index_entry(t, f) for t, f in done])
        doc = laps_doc(results, all_tracks, FOLDERS)
        write_json(laps_path, doc)
        write_json(os.path.join(F1990, 'data', 'ai-laps-1990.json'), doc)
        write_json(os.path.join(out_root, 'results.json'), [results[k] for k in done])

    t0 = time.time()
    failed = []
    if args.report_only:
        if not results:
            raise SystemExit(f'{out_root}: no results.json with existing archives; run without --report-only first')
        flush()
    with ProcessPoolExecutor(max(1, min(args.workers, len(jobs))), initializer=_init, initargs=(sa,)) as pool:
        futs = {pool.submit(job, j): (j[0], j[1]) for j in jobs}
        for fut in as_completed(futs):
            key = futs[fut]
            try:
                r = fut.result()
            except Exception as ex:
                failed.append((key, f'{type(ex).__name__}: {ex}'))
                print(f'{key[0]:18} {key[1]:15} FAILED {type(ex).__name__}: {ex}', flush=True)
                continue
            results[key] = r
            flush()
            print(f'{r["track"]:18} {r["folder"]:15} {r["variation"]:9} 1990 {r["lap1990"]:7.3f}  2026 {r["lap2026"]:7.3f}  '
                  f'x{r["lap1990"] / r["lap2026"]:.4f}  {r["bytes"] / 1e6:.1f} MB  {r["seconds"]:.0f} s  '
                  f'[{len(results)}/{len(jobs)} {time.time() - t0:.0f} s]', flush=True)

    checks = {'archives': {}, 'issues': {}}
    keys = sorted(results, key=lambda k: order.get(k, 1e9))
    with ProcessPoolExecutor(max(1, min(args.workers, len(keys))), initializer=_init, initargs=(sa,)) as pool:
        checked = list(pool.map(_check_job, [(os.path.join(out_root, t, f'{f}.erp'), stock_archive(args.game, t, f), t, f)
                                             for t, f in keys]))
    for (t, f), c in zip(keys, checked):
        checks['archives'][f'{t}/{f}'] = c
        if c['issues']:
            checks['issues'][f'{t}/{f}'] = c['issues']
        r = results[(t, f)]
        if r['lap1990'] <= r['lap2026']:
            checks['issues'].setdefault(f'{t}/{f}', []).append(
                f'1990 lap {r["lap1990"]:.3f} is not slower than the 2026 lap {r["lap2026"]:.3f}')
    checks['sanity'] = {f'{k[0]}/{k[1]}': results[k]['sanity'] for k in results if results[k].get('sanity')}
    checks['bytes'] = sum(os.path.getsize(os.path.join(out_root, t, f'{f}.erp')) for t, f in results)
    checks['archives_written'] = len(results)
    checks['failed'] = [{'track': k[0], 'folder': k[1], 'error': e} for k, e in failed]
    write_json(os.path.join(out_root, 'checks.json'), checks)
    print(json.dumps({'archives': len(results), 'bytes': checks['bytes'], 'issues': checks['issues'],
                      'failed': checks['failed']}, indent=1))
    return 1 if failed or checks['issues'] else 0


def _init(sa):
    if sa not in sys.path:
        sys.path.insert(0, sa)
    import swofty_ai  # noqa: F401
    import erpw
    try:
        from compression import zstd
        erpw.zstd_d = zstd.decompress
        erpw.zstd_c = lambda b, level=19: zstd.compress(b, level)
    except ImportError:
        pass


if __name__ == '__main__':
    sys.exit(main())
