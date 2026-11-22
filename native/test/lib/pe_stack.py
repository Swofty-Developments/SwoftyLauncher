#!/usr/bin/env python3
import bisect
import os
import re
import subprocess
import sys

pid = int(sys.argv[1])
FRAME_OFF = int(os.environ.get('PE_STACK_FRAME_OFF', '0x328'), 16)
GDB = (['sudo', '-n'] if os.environ.get('PE_STACK_SUDO', '1') == '1' else []) + ['timeout', '30', os.environ.get('PE_STACK_GDB', 'gdb'), '-q', '-nx', '-p', str(pid), '-batch']


def gdb(cmds):
    args = list(GDB)
    for c in cmds:
        args += ['-ex', c]
    r = subprocess.run(args, capture_output=True, text=True)
    return r.stdout + r.stderr


def maps():
    mods = {}
    for line in open(f'/proc/{pid}/maps'):
        parts = line.split(None, 5)
        if len(parts) < 6:
            continue
        path = parts[5].strip()
        if not re.search(r'\.(dll|exe|drv|sys)$', path, re.I):
            continue
        lo, hi = (int(x, 16) for x in parts[0].split('-'))
        m = mods.setdefault(path, [lo, hi])
        m[0], m[1] = min(m[0], lo), max(m[1], hi)
    out = []
    for p, (lo, hi) in mods.items():
        try:
            with open(p, 'rb') as f:
                head = f.read(4096)
            nt = int.from_bytes(head[0x3c:0x40], 'little')
            opt = nt + 24
            if head[opt:opt + 2] == b'\x0b\x02':
                IMAGEBASE[p] = int.from_bytes(head[opt + 24:opt + 32], 'little')
                hi = max(hi, lo + int.from_bytes(head[opt + 56:opt + 60], 'little'))
        except OSError:
            pass
        out.append((lo, hi, p))
    return sorted(out)


EXPORTS = {}
IMAGEBASE = {}
MODS = maps()
STARTS = [m[0] for m in MODS]


def module_of(a):
    i = bisect.bisect_right(STARTS, a) - 1
    if i >= 0 and MODS[i][0] <= a < MODS[i][1]:
        return MODS[i]
    return None


def exports(path):
    if path in EXPORTS:
        return EXPORTS[path]
    rvas, names, base = {}, {}, 0
    try:
        out = subprocess.run(['objdump', '-p', path], capture_output=True, text=True).stdout
    except OSError:
        out = ''
    section = ''
    for line in out.splitlines():
        if 'Export Address Table' in line:
            section = 'eat'
        elif 'Ordinal/Name Pointer' in line:
            section = 'names'
        elif not line.strip():
            continue
        e = re.match(r'\s*\[\s*(\d+)\]\s+\+base\[\s*(\d+)\]\s+([0-9a-fA-F]+)\s+(\S+)', line)
        if not e:
            continue
        if section == 'eat' and e.group(4) == 'Export':
            rvas[int(e.group(2))] = int(e.group(3), 16)
        elif section == 'names':
            names[int(e.group(2))] = e.group(4)
    table = sorted((rva, names.get(o, f'#{o}')) for o, rva in rvas.items())
    EXPORTS[path] = table
    return table


def addr2line(path, va):
    try:
        r = subprocess.run(['addr2line', '-f', '-C', '-e', path, hex(va)], capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired):
        return ''
    lines = [l for l in r.stdout.splitlines() if l.strip()]
    if len(lines) >= 2 and lines[0] != '??':
        return f'{lines[0]} {os.path.basename(lines[1])}'
    return ''


def name(a):
    m = module_of(a)
    if not m:
        return None
    lo, _, path = m
    rva = a - lo
    table = exports(path)
    sym = addr2line(path, IMAGEBASE.get(path, 0) + rva) if IMAGEBASE.get(path) else ''
    if not sym and table:
        i = bisect.bisect_right([t[0] for t in table], rva) - 1
        if i >= 0:
            sym = f'{table[i][1]}+0x{rva - table[i][0]:x} (nearest export)'
    return f'{os.path.basename(path)}+0x{rva:x} {sym}'.rstrip()


def regs():
    out = gdb(['thread 1', 'printf "PC=%lx SP=%lx TEB=%lx\\n", $pc, $sp, $gs_base',
               'printf "SB=%lx SL=%lx FR=%lx\\n", *(unsigned long *)($gs_base + 8), *(unsigned long *)($gs_base + 16), '
               f'*(unsigned long *)($gs_base + {FRAME_OFF:#x})'])
    v = dict(re.findall(r'(\w+)=([0-9a-f]+)', out))
    return {k: int(x, 16) for k, x in v.items()}, out


def qwords(lo, hi):
    tmp = f'/tmp/pe_stack.{pid}.bin'
    gdb([f'dump binary memory {tmp} {lo:#x} {hi:#x}'])
    try:
        data = open(tmp, 'rb').read()
        os.unlink(tmp)
    except OSError:
        return []
    return [(lo + i, int.from_bytes(data[i:i + 8], 'little')) for i in range(0, len(data) - 7, 8)]


r, raw = regs()
if 'PC' not in r:
    print('pe_stack: gdb gave no registers:')
    print('\n'.join(raw.splitlines()[-8:]))
    sys.exit(0)
print(f"pe_stack: main thread pc {r['PC']:x} ({name(r['PC']) or 'unix'}), sp {r['SP']:x}, teb {r['TEB']:x}")
for i in range(4):
    s, _ = regs()
    if 'PC' in s:
        print(f"pe_stack: pc sample {i + 1}: {s['PC']:x} {name(s['PC']) or 'unix'}")
sp = r['SP']
frames = [r.get('FR', 0)]
if not module_of(r['PC']):
    out = gdb(['thread 1', 'printf "FR=%lx\\n", *(unsigned long *)($gs_base + 0x378)'])
    frames += [int(x, 16) for x in re.findall(r'FR=([0-9a-f]+)', out)]
for f in frames if not module_of(r['PC']) else []:
    if f < 0x10000:
        continue
    out = gdb([f'printf "RIP=%lx RSP=%lx RBP=%lx\\n", *(unsigned long *){f + 0x70:#x}, *(unsigned long *){f + 0x88:#x}, '
               f'*(unsigned long *){f + 0x98:#x}'])
    fv = {k: int(x, 16) for k, x in re.findall(r'(\w+)=([0-9a-f]+)', out)}
    if 'RIP' in fv:
        print(f"pe_stack: syscall frame {f:x}: pe rip {fv['RIP']:x} ({name(fv['RIP']) or '?'}), pe rsp {fv['RSP']:x}")
        if module_of(fv['RIP']):
            sp = fv['RSP']
            break
base = r.get('SB', 0)
if not (sp < base and base - sp < (1 << 26)):
    print(f'pe_stack: sp {sp:x} is not below the stack base {base:x}')
    sys.exit(0)
hi = min(base, sp + 0x6000)
n = 0
for at, v in qwords(sp, hi):
    s = name(v)
    if s:
        print(f'pe_stack:   [{at:x}] {v:x} {s}')
        n += 1
        if n >= 60:
            break
