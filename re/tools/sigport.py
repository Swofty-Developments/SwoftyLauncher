#!/usr/bin/env python3
"""Turn KB build RVAs into runtime::Sig declarations that resolve once on the KB exe and on a port image.

  sigport.py --port EXE --build steam-26566308 NAME=RVA [NAME=RVA ...] [--ref KB_EXE] [--json OUT]
  sigport.py --port EXE --build steam-26566308 --compare KB_RVA PORT_RVA [LEN]

For each RVA (a function entry, a code site, a global or a vtable on the KB build) it prints the target on the port, the
same-code verdict of sigverify.py and a C++ line to paste. A function or site gets an addr pattern at the target; a
protector thunk (a jump to a body relocated out of the code section, whose filler differs per build) and anything with
no unique entry pattern get a rel32 pattern at a call site; a global or vtable gets a riprel pattern at a code site that
loads it. Every pattern matches exactly once in its section on both images and resolves to the KB RVA on the KB exe.

--compare prints how many instructions of two whole functions are equal with rip displacements and branch targets
masked (LEN defaults to the KB function's .pdata extent; a thunk is followed to its body on both images).
"""
import argparse, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from sigverify import STEAM, md, norm, follow, classify, same, section_map, riprel_refs, make, resolve
from bindverify import Image

KB_EXE = os.environ.get('SWOFTY_KB_EXE', '/mnt/work/VSC/f1-modding/work/exe/F1_25.exe')


def insn_starts(img, func, upto):
    out = []
    for i in md.disasm(img.read(func, upto - func + 16) or b'', func):
        if i.address > upto:
            break
        out.append((i.address, i.size))
    return out


def check(ref, port, smap, section, sig):
    rh, rt, _ = resolve(ref, section, sig)
    ph, pt, _ = resolve(port, smap.get(section, section), sig)
    return rh, rt, ph, pt


def at_site(ref, port, smap, section, site, need, kind, insn_of, length_of, rva):
    """A pattern starting up to 4 instructions before the instruction at `site` (its rel32/disp at `need`)."""
    func = ref.func_base_of(site)
    if func is None:
        return None
    starts = insn_starts(ref, func, site)
    idx = next((k for k, (a, n) in enumerate(starts) if a == site), None)
    if idx is None:
        return None
    ins_at, ins_len = starts[idx]
    for back in range(0, 5):
        if idx - back < 0:
            break
        st = starts[idx - back][0]
        text, a1, a2 = make(ref, port, smap, st, section, 0, 96)
        if not text:
            continue
        sig = {'pattern': text, 'kind': kind, 'insn': ins_at - st, 'anchor': need - st, 'len': ins_len}
        rh, rt, ph, pt = check(ref, port, smap, section, sig)
        if rh == 1 and ph == 1 and rt == rva:
            return sig
    return None


def port_one(ref, port, smap, name, rva):
    section = ref.section_of(rva).name if ref.section_of(rva) else '?'
    kind = classify(ref, rva)
    tries = []
    if kind in ('func', 'site') and section == '.xtls':
        text, a1, a2 = make(ref, port, smap, rva, '.xtls', 0, 160)
        if text and a1 == rva:
            tries.append({'pattern': text, 'kind': 'addr', 'insn': 0, 'anchor': 0, 'len': 0})
    if not tries:
        code = '.xtls'
        for site in riprel_refs(ref, code, rva):
            op = site - 1
            if kind in ('func', 'site') and ref.mem[op] in (0xE8, 0xE9):
                sig = at_site(ref, port, smap, code, op, site, 'rel32', 0, 5, rva)
            else:
                func = ref.func_base_of(site)
                if func is None:
                    continue
                ins = next(((a, n) for a, n in insn_starts(ref, func, site) if a < site < a + n), None)
                if not ins:
                    continue
                sig = at_site(ref, port, smap, code, ins[0], site, 'riprel', 0, 0, rva)
            if sig:
                tries.append(sig)
                break
    if not tries:
        return {'name': name, 'rva': rva, 'class': kind, 'error': 'no pattern unique on both images'}
    sig = tries[0]
    rh, rt, ph, pt = check(ref, port, smap, '.xtls', sig)
    verdict, detail = same(ref, port, rt, pt, kind)
    return dict(name=name, rva=rva, port=pt, cls=kind, verdict=verdict, detail=detail, **sig)


def cpp(r):
    tail = {'addr': '', 'rel32': f', runtime::Sig::rel32, {r["insn"]}, {r["anchor"]}, {r["len"]}',
            'riprel': f', runtime::Sig::riprel, {r["insn"]}, {r["anchor"]}, {r["len"]}'}[r['kind']]
    return f'{{"{r["name"]}", "{r["pattern"]}", {r["rva"]:#x}{tail}}}'


def compare(ref, port, a, b, n=None):
    fa, fb = follow(ref, a), follow(port, b)
    if n is None:
        starts, ends = ref.pdata()
        n = next((e - s for s, e in zip(starts, ends) if s == fa), 0x100)
    x, y = norm(ref, fa, n), norm(port, fb, n)
    equal = sum(1 for p, q in zip(x, y) if p == q)
    first = next((k for k, (p, q) in enumerate(zip(x, y)) if p != q), None)
    return fa, fb, n, equal, len(x), len(y), first


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--ref', default=KB_EXE)
    ap.add_argument('--port', default=STEAM)
    ap.add_argument('--build', required=True)
    ap.add_argument('--json')
    ap.add_argument('--compare', nargs='+')
    ap.add_argument('items', nargs='*')
    a = ap.parse_args()
    ref, port = Image(a.ref), Image(a.port)
    smap = section_map(a.build)
    if a.compare:
        x, y = int(a.compare[0], 16), int(a.compare[1], 16)
        n = int(a.compare[2], 16) if len(a.compare) > 2 else None
        fa, fb, n, eq, lx, ly, first = compare(ref, port, x, y, n)
        print(f'{fa:#x} vs {fb:#x} over {n:#x}: {eq}/{lx} instructions equal (port {ly}); first difference at index {first}')
        return 0 if eq == lx == ly else 1
    rows, bad = [], 0
    for it in a.items:
        name, rva = it.split('=')
        r = port_one(ref, port, smap, name, int(rva, 16))
        rows.append(r)
        if 'error' in r:
            bad += 1
            print(f'FAIL {name:40} {r["rva"]:#x} {r["class"]}: {r["error"]}')
            continue
        ok = r['verdict'] in ('same', 'data')
        bad += not ok
        print(f'{"ok  " if ok else "FAIL"} {name:40} {r["rva"]:#x} -> port {r["port"]:#x} {r["cls"]:6} {r["verdict"]:9} {r["detail"]}')
        print(f'     {cpp(r)}')
    if a.json:
        json.dump(rows, open(a.json, 'w'), indent=1)
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main())
