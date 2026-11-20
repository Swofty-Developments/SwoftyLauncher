"""Callers and readers of each target, from every executable section (.xtls and .data1 on the KB build, .tls$ and
.trace on 1.0.145).

  EXE=F1_25.exe xref.py OFFSET [NAME=OFFSET ...]      file offsets in hex, as pe_index.off2rva reads them
"""
import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pe_index import BASE, off2rva, riprel_refs, func_of, sec_of
from collections import defaultdict


def xrefs(rvas):
    """{target rva: [(site rva, section, function start or None)]} over every executable section."""
    by = defaultdict(list)
    for site, t in riprel_refs(list(rvas)):
        f = func_of(site)
        by[t].append((site, sec_of(site), f[0] if f else None))
    return {t: by.get(t, []) for t in rvas}


def main(argv):
    tg = {}
    for a in argv:
        n, o = a.split('=') if '=' in a else (a, a)
        tg[off2rva(int(o, 16))] = n
    for t, sites in xrefs(tg).items():
        secs = sorted({s for _, s, _ in sites})
        fs = sorted({f for _, _, f in sites if f is not None})
        print(f'{tg[t]:40s} rva={t:08x} refs={len(sites)} sections={secs} sites={[hex(BASE + s) for s, _, _ in sites[:8]]} '
              f'funcs={[hex(BASE + f) for f in fs[:8]]}')


if __name__ == '__main__':
    main(sys.argv[1:])
