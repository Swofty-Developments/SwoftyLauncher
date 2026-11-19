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

  sigport.py --port EXE --build B --witness KB_RVA OFF[,OFF...] [--witness ...]

  sigport.py --port EXE --build B --to-kb PORT_RVA [PORT_RVA ...]

--to-kb goes the other way, for someone who only has the port build: the KB twin of a port RVA is the RVA minus the
shift that both bindings around it (the nearest one below and the nearest one above, resolved on the port image, in
the same section) share. Code moves between builds in blocks, so two neighbours with the same shift put everything
between them at that shift; neighbours that disagree mean code was added or removed between them and give no twin.
When the KB exe (--ref) is also there, the twin is checked the sigport way: a pattern unique on both images must land
on it and the code must be the same.

--witness proves struct offsets: it finds the port's twin of the KB function (an entry pattern unique on both, else
through a call site), compares the two whole functions, and lists per offset the instructions that use it as a
displacement on each side. The two instruction streams are aligned with rip displacements and branch targets masked, in
matching runs of at least 3 instructions, so a protector rewrite elsewhere in the body (a constant split into a mov and
an add) does not hide a use both builds share. An offset is witnessed when both functions use it the same number of
times and every KB use aligns with a port use; when the functions are equal, at the same instruction index.
"""
import argparse, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from sigverify import STEAM, md, norm, follow, classify, same, section_map, riprel_refs, make, resolve, protector_thunk, aligned
from bindverify import Image, anchors, kb_twin
from capstone.x86 import X86_GRP_CALL, X86_GRP_JUMP, X86_OP_IMM, X86_OP_MEM, X86_REG_RIP, X86_REG_RSP

KB_EXE = os.environ.get('SWOFTY_KB_EXE', '/mnt/work/VSC/f1-modding/work/exe/F1_25.exe')
RUN = 3


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


def port_one(ref, port, smap, name, rva, code='.xtls'):
    """code is the ref image's code section (.xtls on the KB build, .tls$ on 1.0.145 when it is the ref)."""
    section = ref.section_of(rva).name if ref.section_of(rva) else '?'
    kind = classify(ref, rva)
    tries = []
    if kind in ('func', 'site') and section == code and not protector_thunk(ref, rva):
        text, a1, a2 = make(ref, port, smap, rva, code, 0, 160)
        if text and a1 == rva:
            tries.append({'pattern': text, 'kind': 'addr', 'insn': 0, 'anchor': 0, 'len': 0})
    if not tries:
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
    rh, rt, ph, pt = check(ref, port, smap, code, sig)
    verdict, detail = same(ref, port, rt, pt, kind)
    return dict(name=name, rva=rva, port=pt, cls=kind, verdict=verdict, detail=detail, **sig)


def check_twin(port, ref, smap, rva, kb_rva):
    """Confirm a twin with both images: a pattern from the port bytes unique on both lands on kb_rva, same code."""
    inverse = {v: k for k, v in smap.items()}
    r = port_one(port, ref, inverse, 'twin', rva, smap.get('.xtls', '.xtls'))
    if 'error' in r:
        return False, r['error']
    if r['port'] != kb_rva:
        return False, f'a pattern unique on both images lands on KB {r["port"]:#x}, not {kb_rva:#x}'
    return r['verdict'] in ('same', 'likely', 'data'), f'{r["verdict"]}: {r["detail"]}'


def cpp(r):
    tail = {'addr': '', 'rel32': f', runtime::Sig::rel32, {r["insn"]}, {r["anchor"]}, {r["len"]}',
            'riprel': f', runtime::Sig::riprel, {r["insn"]}, {r["anchor"]}, {r["len"]}'}[r['kind']]
    return f'{{"{r["name"]}", "{r["pattern"]}", {r["rva"]:#x}{tail}}}'


def extent(img, rva, default=0x100):
    starts, ends = img.pdata()
    return next((e - s for s, e in zip(starts, ends) if s == rva), default)


def compare(ref, port, a, b, n=None):
    fa, fb = follow(ref, a), follow(port, b)
    if n is None:
        n = extent(ref, fa)
    x, y = norm(ref, fa, n), norm(port, fb, n)
    equal = sum(1 for p, q in zip(x, y) if p == q)
    first = next((k for k, (p, q) in enumerate(zip(x, y)) if p != q), None)
    return fa, fb, n, equal, len(x), len(y), first


def uses(img, rva, n, offs):
    """Per offset, (instruction index, text) of the instructions using it as a displacement off a register other than
    rip and rsp (stack slots are not struct fields), or as an immediate (a stride or a size)."""
    out = {o: [] for o in offs}
    for k, i in enumerate(md.disasm(img.read(rva, n + 16) or b'', rva)):
        if i.address - rva >= n:
            break
        for o in i.operands:
            if o.type == X86_OP_MEM and o.mem.base not in (X86_REG_RIP, X86_REG_RSP) and o.mem.disp in out:
                out[o.mem.disp].append((k, i.mnemonic + ' ' + i.op_str))
            elif o.type == X86_OP_IMM and o.imm in out and not (i.group(X86_GRP_JUMP) or i.group(X86_GRP_CALL)):
                out[o.imm].append((k, i.mnemonic + ' ' + i.op_str))
    return out


def witness(ref, port, smap, rva, offs):
    r = port_one(ref, port, smap, 'w', rva)
    if 'error' in r:
        return {'rva': rva, 'error': r['error']}
    fa, fb, n, eq, lx, ly, first = compare(ref, port, rva, r['port'])
    m = extent(port, fb, n)
    amap = aligned(norm(ref, fa, n), norm(port, fb, m), RUN)
    ua, ub = uses(ref, fa, n, offs), uses(port, fb, m, offs)
    identical = eq == lx == ly
    per = {}
    for o in offs:
        a, b = ua[o], ub[o]
        at = {k for k, _ in b}
        same_place = bool(a) and a == b
        paired = same_place if identical else bool(a) and len(a) == len(b) and all(amap.get(k) in at for k, _ in a)
        per[o] = {'kb': len(a), 'port': len(b), 'same_place': same_place, 'aligned': paired,
                  'first': a[0][1] if a else None}
    return {'rva': rva, 'port': r['port'], 'body': fa, 'port_body': fb, 'len': n, 'equal': eq, 'kb_insns': lx,
            'port_insns': ly, 'aligned': len(amap), 'identical': identical, 'offsets': per}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--ref', default=KB_EXE)
    ap.add_argument('--port', default=STEAM)
    ap.add_argument('--build', required=True)
    ap.add_argument('--json')
    ap.add_argument('--compare', nargs='+')
    ap.add_argument('--witness', nargs=2, action='append', metavar=('KB_RVA', 'OFFS'))
    ap.add_argument('--to-kb', nargs='+', metavar='PORT_RVA')
    ap.add_argument('items', nargs='*')
    a = ap.parse_args()
    if a.to_kb:
        port, smap = Image(a.port), section_map(a.build)
        ref = Image(a.ref) if os.path.isfile(a.ref) else None
        marks, bad = anchors(port, a.build), 0
        for x in a.to_kb:
            rva = int(x, 16)
            kb_rva, why = kb_twin(port, a.build, rva, marks)
            if kb_rva is None:
                bad += 1
                print(f'FAIL {rva:#x}: {why}')
                continue
            line = f'ok   {rva:#x} -> KB {kb_rva:#x}: {why["text"]}'
            if ref is not None:
                ok, detail = check_twin(port, ref, smap, rva, kb_rva)
                bad += not ok
                line += f'; with the KB exe: {detail}'
            print(line)
        return 1 if bad else 0
    ref, port = Image(a.ref), Image(a.port)
    smap = section_map(a.build)
    if a.witness:
        rows, bad = [], 0
        for rva, offs in a.witness:
            w = witness(ref, port, smap, int(rva, 16), [int(x, 16) for x in offs.split(',')])
            rows.append(w)
            if 'error' in w:
                bad += 1
                print(f'FAIL {w["rva"]:#x}: {w["error"]}')
                continue
            print(f'{"same" if w["identical"] else "DIFF"} {w["rva"]:#x} -> {w["port"]:#x} (bodies {w["body"]:#x} / '
                  f'{w["port_body"]:#x}): {w["equal"]}/{w["kb_insns"]} equal in place, {w["aligned"]}/{w["kb_insns"]} '
                  f'aligned in runs of {RUN}+, port {w["port_insns"]}')
            for o, p in w['offsets'].items():
                ok = p['aligned']
                bad += not ok
                print(f'  {"ok  " if ok else "----"} +{o:#x}: kb {p["kb"]} port {p["port"]} use(s), '
                      f'{"aligned" if ok else "not aligned"}{"  e.g. " + p["first"] if p["first"] else ""}')
        if a.json:
            json.dump(rows, open(a.json, 'w'), indent=1, default=str)
        return 1 if bad else 0
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
