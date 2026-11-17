#!/usr/bin/env python3
"""Report, per core feature gated on the KB build, whether everything it depends on is proven on a port build.

  portcheck.py --port EXE --build steam-26566308 [--ref KB_EXE] [--feature NAME ...] [--md OUT] [--json OUT]

A feature (FEATURES below) names the bindings it calls or hooks, the KB struct fields it reads or writes and the raw KB
RVAs it still carries. Each is checked against the KB exe and the port with the core's own rules:

  binding  resolved on both images (bindverify.py with each build's set); a function is compared instruction by
           instruction over its whole .pdata extent, a protector thunk through its relocated body; a code site through
           its containing function, at the same offset in it; data only has to resolve
  field    layoutcmp.py: the instruction at the KB xref (or any that uses the displacement) in the KB function has an
           aligned counterpart with the same displacement in the port's twin of that function
  rva      sigport.py: a pattern unique on both images that resolves to the RVA on the KB exe, and the same code or
           vtable slots on the port

The verdict per feature is `proven` when every dependency is, otherwise the list of what is not. After a game update,
add the new build's overlay, run this for it and port what fails; the per-build tables in the core take the result.
"""
import argparse, json, os, re, struct, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import bindverify as bv
import layoutcmp as lc
from sigport import KB_EXE, compare, port_one
from sigverify import STEAM, follow, md, section_map
from capstone import x86
from capstone.x86 import X86_GRP_CALL, X86_GRP_JUMP, X86_OP_IMM, X86_OP_MEM, X86_OP_REG, X86_REG_RIP

BASE = 0x140000000

# Displacements that moved on a port build, as (KB displacement, port displacement) per overlay. A function that differs
# from the KB build's only by these counts as the same code for the features below, none of which reads them. Steam
# 26566308 moved the CarErs flag and mode block by four bytes (live state selects its ERS mode offset per build itself).
MOVED = {
    'steam-26566308': {(0x1a62, 0x1a66), (0x1a64, 0x1a68), (0x1a65, 0x1a69), (0x1a68, 0x1a6c)},
}

FEATURES = {
    'car_points': {
        'gate': 'native/src/core/hooks/car_live.cpp arm_car_points',
        'bindings': ['site_Car_LiveCarBuilt', 'Car_Systems_Reset_Thunk', 'Car_Upgrades_ApplyToVehicle',
                     'site_Car_SetupTuningApplied', 'Game_Pitlane_SetVehicleStatus'],
        'fields': ['CarLive.vehicle_id', 'CarLive.upgrades_allocator', 'CarComponentBase.car', 'PitLaneSystem.car_slots',
                   'PitLaneCarState.state', 'PitLaneCarState.pit_stops'],
    },
    'car_parts': {
        'gate': 'native/src/core/hooks/car_live.cpp car.parts.register and car.damage points',
        'bindings': ['Car_FrontWing_Init', 'Car_FrontWing_OnCollision', 'Car_Register_DetachablePart_Thunk',
                     'Game_Get_ModelNodeId'],
        'fields': ['CarComponentBase.car', 'CarDamageFrontWing.full_detach_scale', 'CarDamageFrontWing.left_damage',
                   'CarDamageFrontWing.right_damage'],
    },
    'car_ops': {
        'gate': 'native/src/core/hooks/car_live.cpp setup',
        'bindings': ['g_Sess_VehicleTable', 'g_Sess_VehicleCount', 'Car_Engine_ComputeDerived_Thunk',
                     'Car_Tuning_SetFinalDrive', 'Car_Tyre_SetWearFactor', 'Car_FuelEcu_SetMode_Thunk'],
        'fields': ['CarLive.vehicle_id', 'CarLive.is_ai', 'CarLive.engine', 'CarLive.gearbox', 'CarLive.fuel_tank',
                   'CarLive.fuel_ecu', 'CarLive.brake_ecu', 'CarLive.chassis', 'CarLive.steering', 'CarLive.f1_aero',
                   'CarLive.differential', 'CarLive.suspension', 'CarLive.turbo', 'CarLive.f1_wing_damage',
                   'CarLive.f1_suspension_damage', 'CarComponentBase.car', 'CarComponentBase.data_vtable',
                   'CarSteering.max_steering_lock', 'CarGearBox.final_drive_ratio', 'CarGearBox.gear_ratio1st',
                   'CarGearBox.gear_ratio9th', 'CarGearBox.max_clutch_torque', 'CarGearBox.use_setup_ratios',
                   'CarDifferential.diff_on_throttle', 'CarDifferential.diff_off_throttle', 'CarTurbo.max_boost',
                   'CarFuelTank.fuel_capacity', 'CarFuelTank.fuel_mass', 'CarDamageFrontWing.speed_threshold',
                   'CarDamageFrontWing.full_detach_scale', 'CarDamageSuspension.speed_threshold_obf',
                   'CarDamageSuspension.angular_fracture_limit', 'CarEngine.vtf_limiter_rotation_rate',
                   'CarF1Aero.lift_scale', 'CarF1Aero.drag_scale', 'CarF1Aero.aero_slip_stream_air_density_scale_front_wing',
                   'CarF1Aero.aero_slip_stream_car_width', 'CarBrakeEcu.brake_bias', 'CarBrakeEcu.brake_pressure',
                   'CarChassis.sqrt_mass', 'CarChassis.tyre_physics_group', 'CarSuspension.tyre_types',
                   'CarSuspension.axle0', 'CarSuspension.axle1', 'CarSuspAxle.local_rest_position_z',
                   'CarSuspAxle.setup_camber', 'CarSuspAxle.setup_toe', 'CarSuspAxle.setup_anti_roll',
                   'CarSuspAxle.ride_height_offset', 'CarFuelEcu.fuel_burn_scale_lean', 'CarFuelEcu.fuel_revs_scale_max',
                   'CarFuelEcu.temperature_scale_lean', 'CarFuelEcu.temperature_scale_max', 'CarFuelEcu.fuel_mode'],
        'rvas': {'g_EngineModeAvail': 0x606ec28},
    },
    'car_levers': {
        'gate': 'native/src/core/hooks/car_live.cpp car_levers_mixins',
        'bindings': ['site_Car_EngineDriveTorque', 'site_Ai_TableGripInput', 'site_Ai_BuildSystem_RecordPushed'],
        'fields': ['AiSystem.records_begin', 'AiCarRecord.vehicle_index', 'AiCarRecord.driver_config'],
    },
    'gamedb_late': {
        'gate': 'native/src/core/assets/gamedb.cpp late_inplace',
        'bindings': ['g_Gdb_Database', 'Gdb_Lookup_Key'],
        'rvas': {'vt_Gdb_IntRecord': 0x5bee788, 'vt_Gdb_FloatRecord': 0x5bee798, 'vt_Gdb_BoolRecord': 0x56e6518},
    },
    'tt_player_name': {
        'gate': 'native/src/core/frontend/player_name.cpp setup',
        'bindings': ['g_Vm_Registry', 'Vm_Set_String', 'Ne_Make_StrFromCStr'],
        'rvas': {'Game_PresenceNameCache_Lookup': 0x816b10, 'vt_Vm_ObjectNode': 0x5a70080, 'vt_Vm_ListNode': 0x5a6ff90,
                 'vt_Vm_StringLeafNode': 0x565f200, 'vt_Vm_StringBinding': 0x5660370},
        'functions': {'Vm_GetOrCreate_ChildNode': 0x42b2a50, 'Vm_Unk_140846570': 0x846570,
                      'Game_Vm_HolderGetCStrOrEmpty': 0x429db40, 'Vm_ListLookup': 0x42b1130},
    },
}


def resolved(img, build, kb_first):
    doc = bv.gen_set(build)
    return {r['id']: r for r in bv.Resolver(img, doc, None, kb_first=kb_first).run()}, \
        {b['id']: b for b in doc['bindings']}


def canonical(img, rva, n):
    """The instructions of [rva, rva+n) as text with rip displacements and branch targets masked, alignment nops and
    int3 dropped, and the protector's hidden constants folded: `mov r32, K; sub r32, [rip+X]; op a, r32` is `op a, K-[X]`
    with [X] read from the image. Returns (tokens, folds)."""
    out, folds, pend = [], 0, None
    for i in md.disasm(img.read(rva, n + 16) or b'', rva):
        if i.address - rva >= n:
            break
        if i.mnemonic.startswith('nop') or i.mnemonic == 'int3':
            continue
        text = f'{i.mnemonic} {i.op_str}'
        rip = [o for o in i.operands if o.type == X86_OP_MEM and o.mem.base == X86_REG_RIP]
        if i.mnemonic == 'mov' and len(i.operands) == 2 and i.operands[0].type == X86_OP_REG and \
                i.operands[1].type == X86_OP_IMM and i.operands[0].size == 4:
            pend = (i.reg_name(i.operands[0].reg), i.operands[1].imm & 0xffffffff, None, text)
            continue
        if pend and pend[2] is None and i.mnemonic == 'sub' and rip and i.op_str.startswith(pend[0] + ',') and \
                img.readable(i.address + i.size + i.disp, 4):
            hidden = struct.unpack_from('<I', img.mem, i.address + i.size + i.disp)[0]
            pend = (pend[0], (pend[1] - hidden) & 0xffffffff, True, pend[3])
            continue
        if pend and pend[2] and text.endswith(', ' + pend[0]):
            v = pend[1]
            text = text[:-len(pend[0])] + (str(v) if v < 10 else hex(v))
            folds += 1
            pend = None
        elif pend:
            out.append(pend[3])
            pend = None
        if i.group(X86_GRP_JUMP) or i.group(X86_GRP_CALL):
            text = i.mnemonic + ' T'
        elif rip:
            text = re.sub(r'rip [+-] 0x[0-9a-f]+', 'rip+R', text)
        out.append(text)
    return out, folds


BUILD = [None]


def displaced(x, y):
    """The (KB, port) displacement pairs when the two token lists differ only in the displacement of a memory operand."""
    if len(x) != len(y):
        return None
    pairs = set()
    for p, q in zip(x, y):
        if p == q:
            continue
        mp, mq = re.fullmatch(r'(.*\[\w+ \+ )0x([0-9a-f]+)(\].*)', p), re.fullmatch(r'(.*\[\w+ \+ )0x([0-9a-f]+)(\].*)', q)
        if not mp or not mq or mp.group(1) != mq.group(1) or mp.group(3) != mq.group(3):
            return None
        pairs.add((int(mp.group(2), 16), int(mq.group(2), 16)))
    return pairs


def whole(ref, port, a, b):
    fa, fb, n, eq, lx, ly, first = compare(ref, port, a, b)
    ok = eq == lx == ly and lx > 0
    why = f'{fa + BASE:#x} vs port {fb + BASE:#x}: {eq}/{lx} instructions equal over {n:#x}'
    if ok:
        return ok, why
    x, fx = canonical(ref, fa, n)
    y, fy = canonical(port, fb, n + 0x80)
    if x and y[:len(x)] == x:
        return True, why + f'; equal after folding {fx}+{fy} protector constant(s) and dropping alignment'
    moved = displaced(x, y[:len(x)])
    if moved is not None and moved <= MOVED.get(BUILD[0], set()):
        return True, why + '; the rest differ only by displacements that moved on this build and no gated feature reads (' + \
            ', '.join(f'+{a:#x} -> +{b:#x}' for a, b in sorted(moved)) + ')'
    k = next((k for k, (p, q) in enumerate(zip(x, y)) if p != q), None)
    return False, why + f', first difference at {first}; still differs after folding ({x[k] if k is not None else "-"} / ' \
                        f'{y[k] if k is not None else "-"})'


def check_binding(ref, port, kb, pt, kinds, name):
    k, p = kb.get(name), pt.get(name)
    if not k or not p or k.get('target') is None or p.get('target') is None:
        return False, 'does not resolve on both builds'
    a, b = (int(x['target'], 16) if isinstance(x['target'], str) else x['target'] for x in (k, p))
    kind = kinds.get(name, {}).get('kind', '')
    if kind == 'data':
        return True, f'data {a + BASE:#x} -> port {b + BASE:#x}'
    if kind == 'func':
        return whole(ref, port, a, b)
    fa, fb = ref.func_base_of(a), port.func_base_of(b)
    if fa is None or fb is None:
        return False, 'site outside any .pdata function'
    if a - fa != b - fb:
        return False, f'site at +{a - fa:#x} in the KB function, +{b - fb:#x} on the port'
    ok, why = whole(ref, port, fa, fb)
    return ok, f'site +{a - fa:#x} in ' + why


def check_rva(ref, port, smap, name, rva):
    r = port_one(ref, port, smap, name, rva)
    if 'error' in r:
        return False, r['error']
    ok = r['verdict'] in ('same', 'data')
    return ok, f'{rva + BASE:#x} -> port {r["port"] + BASE:#x} {r["verdict"]} ({r["detail"]})'


def check_fields(ref, port, build, names):
    fields = lc.parse_structs()
    loc = lc.Locator(ref, port, build)
    cache, out = {}, {}
    for name in names:
        T, f = name.split('.')
        if (T, f) not in fields:
            out[name] = (False, 'no KB row')
            continue
        ok, why = prove_field(ref, port, loc, cache, fields, T, f)
        out[name] = (ok, why)
    return out


def prove_field(steam, other, loc, cache, fields, T, f):
    off, lvl, xrefs, decs = fields[(T, f)]
    starts = []
    for x in xrefs:
        rva = x - BASE
        fb = lc.func_bounds(steam, rva) if 0 <= rva < steam.size else None
        st = fb[0] if fb else next((d - BASE for d in decs if 0 <= rva - (d - BASE) < 0x3000), None)
        if st is not None and st not in starts:
            starts.append(st)
    for d in decs:
        if 0 <= d - BASE < steam.size and d - BASE not in starts:
            starts.append(d - BASE)
    xr = {x - BASE for x in xrefs}
    why = []
    for st in starts:
        fr = lc.compare_function(steam, other, loc, st, cache)
        if not fr['ok']:
            why.append(f'{st + BASE:#x}: {fr["why"]}')
            continue
        field = [k for k, i in enumerate(fr['sins']) if lc.covers(i, off) and lc.mem_span(i)[2] != x86.X86_REG_RSP]
        hits = [k for k in field if fr['sins'][k].address in xr] or field
        if not hits:
            why.append(f'{st + BASE:#x}: no instruction with displacement {off:#x}')
            continue
        bad = 0
        for k in hits:
            e = fr['amap'].get(k)
            si = fr['sins'][k]
            if e is None or not (lc.same_slot(si, fr['eins'][e]) and lc.covers(fr['eins'][e], off)):
                bad += 1
        if bad:
            why.append(f'{st + BASE:#x}: {bad}/{len(hits)} instruction(s) differ (ratio {fr["ratio"]:.2f})')
            continue
        si = fr['sins'][hits[0]]
        ei = fr['eins'][fr['amap'][hits[0]]]
        return True, f'{off:#x}: {si.address + BASE:#x} `{si.mnemonic} {si.op_str}` = port {ei.address + BASE:#x} ' \
                     f'(fn ratio {fr["ratio"]:.2f}, {len(hits)} insn)'
    return False, f'{off:#x} not proven: ' + '; '.join(why[:3])


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--ref', default=KB_EXE)
    ap.add_argument('--port', default=STEAM)
    ap.add_argument('--build', required=True, help='the port\'s overlay in re/db/builds')
    ap.add_argument('--feature', action='append', choices=sorted(FEATURES))
    ap.add_argument('--md')
    ap.add_argument('--json')
    a = ap.parse_args(argv)
    ref, port = bv.Image(a.ref), bv.Image(a.port)
    smap = section_map(a.build)
    BUILD[0] = a.build
    want = a.feature or sorted(FEATURES)
    kb, kinds = resolved(ref, 'steam', True)
    pt, _ = resolved(port, a.build, True)
    report, lines, failed = {}, [], 0
    for name in want:
        feat = FEATURES[name]
        items = []
        for b in feat.get('bindings', []):
            items.append(('binding', b) + check_binding(ref, port, kb, pt, kinds, b))
        for n, rva in feat.get('rvas', {}).items():
            items.append(('rva', n) + check_rva(ref, port, smap, n, rva))
        for n, rva in feat.get('functions', {}).items():
            r = port_one(ref, port, smap, n, rva)
            items.append(('function', n) + (whole(ref, port, rva, r['port']) if 'error' not in r else (False, r['error'])))
        for n, (ok, why) in check_fields(ref, port, a.build, feat.get('fields', [])).items():
            items.append(('field', n, ok, why))
        missing = [n for _, n, ok, _ in items if not ok]
        failed += bool(missing)
        report[name] = {'gate': feat['gate'], 'proven': not missing, 'missing': missing,
                        'items': [{'kind': k, 'name': n, 'ok': ok, 'why': w} for k, n, ok, w in items]}
        lines += [f'## {name}: {"proven" if not missing else "NOT proven (" + ", ".join(missing) + ")"}', '',
                  f'Gate: `{feat["gate"]}`', '', '| kind | name | ok | evidence |', '|---|---|---|---|']
        lines += [f'| {k} | {n} | {"yes" if ok else "NO"} | {w} |' for k, n, ok, w in items]
        lines.append('')
    text = '\n'.join(lines)
    print(text)
    if a.md:
        open(a.md, 'w').write(text + '\n')
    if a.json:
        json.dump(report, open(a.json, 'w'), indent=1)
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
