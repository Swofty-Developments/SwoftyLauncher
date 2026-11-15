#!/usr/bin/env python3
"""Check that every displacement the live car operations read is the same on two builds.

  carlivecmp.py <repo root> <Steam F1_25.exe> <EA dump> <fields file, one Type.field per line>

Uses layoutcmp.py to find each KB evidence function on the other build, then requires every instruction whose memory
operand has the field's displacement (as a system offset, or minus 0x18 as a data object offset) to be identical there.
"""
import sys, os, re
ROOT = sys.argv[1]
sys.path.insert(0, os.path.join(ROOT, 're', 'tools'))
import layoutcmp as lc
from capstone import x86
bv = lc.bv
steam, other = bv.Image(sys.argv[2]), bv.Image(sys.argv[3])
loc = lc.Locator(steam, other)
kb = lc.parse_structs()
cache = {}
STACK = (x86.X86_REG_RSP, x86.X86_REG_RBP)
OBF = set()
for fn in os.listdir(lc.STRUCTS):
    for line in open(os.path.join(lc.STRUCTS, fn)):
        m = re.match(r'\s*/\* 0x[0-9a-fA-F]+ \*/\s+CarObfFloat\s+(\w+);', line)
        if m: OBF.add((fn[:-2], m.group(1)))
want = [tuple(l.strip().split('.')) for l in open(sys.argv[4]) if l.strip()]
want += [(T, 'data_vtable') for T in ('CarSteering','CarGearBox','CarDifferential','CarTurbo','CarFuelTank','CarEngine','CarF1Aero','CarBrakeEcu','CarChassis','CarFuelEcu','CarSuspension','CarComponentBase')]
EXTRA = {('CarSteering', 'max_steering_lock'): [0x142cf9820]}
def hits(fr, d):
    n = same = 0; first = None
    for k, i in enumerate(fr['sins']):
        sp = lc.mem_span(i)
        if sp is None or sp[2] in STACK or sp[0] != d: continue
        n += 1
        e = fr['amap'].get(k)
        if e is None: continue
        ei = fr['eins'][e]
        if lc.norm(ei) == lc.norm(i) and lc.disp_of(ei) == d:
            same += 1
            first = first or f'{i.address+lc.BASE:#x} {i.mnemonic} {i.op_str} | EA {ei.address+lc.BASE:#x}'
    return n, same, first
summary = []
for T, f in want:
    off, lvl, xrefs, decs = kb[(T, f)]
    obf = (T, f) in OBF
    starts = []
    for x in xrefs:
        fb = lc.func_bounds(steam, x - lc.BASE) if 0 <= x - lc.BASE < steam.size else None
        if fb and fb[0] not in starts: starts.append(fb[0])
    for d in decs + EXTRA.get((T, f), []):
        if d - lc.BASE not in starts: starts.append(d - lc.BASE)
    verdict = []
    print(f'## {T}.{f} +{off:#x}{" obf" if obf else ""}')
    for st in starts:
        fr = lc.compare_function(steam, other, loc, st, cache)
        if not fr['ok']:
            print(f'  fn {st+lc.BASE:#x}: {fr["why"]}'); continue
        nd = sum(1 for k, i in enumerate(fr['sins']) if k in fr['amap'] and lc.norm(fr['eins'][fr['amap'][k]]) != lc.norm(i))
        ua = len(fr['sins']) - len(fr['amap'])
        ident = nd == 0 and ua == 0
        for space, base in (('direct', off), ('reader(-0x18)', off - 0x18)):
            if base < 0: continue
            v = hits(fr, base); kx = hits(fr, base + 8) if obf else (1, 1, '')
            full = v[0] > 0 and v[0] == v[1] and kx[0] > 0 and kx[0] == kx[1]
            if v[0] or (obf and kx[0]):
                print(f'  fn {st+lc.BASE:#x} {"IDENTICAL" if ident else "ratio %.3f diff=%d unal=%d" % (fr["ratio"], nd, ua)} {space}: value insns {v[0]} same {v[1]}' + (f', key insns {kx[0]} same {kx[1]}' if obf else '') + f'; {v[2]}')
            if full: verdict.append((space, st, ident))
    summary.append((T, f, off, verdict))
print('\n==== SUMMARY')
for T, f, off, v in summary:
    s = ', '.join(f'{sp}@{st+lc.BASE:#x}{"*" if idn else ""}' for sp, st, idn in v) or 'NONE'
    print(f'{T}.{f} +{off:#x}: {s}')
