#!/usr/bin/env python3
"""Fails the build when an object in src/core gets a dynamic initializer that tools/init_allow.txt does not list.

tools/cxx_init.sh reports every namespace-scope or static-member object that clang's -Wglobal-constructors flags.
An object that is only destroyed at exit was constant-initialized, so it reads nothing at load and is listed in the
output only. A dynamic initializer runs in link order, before or after another unit's, so each one must either be a
registration macro of the SDK or of include/swofty (it appends to a list in the SDK headers that is zero-initialized
before any code runs, or to a function-local static) or be listed in tools/init_allow.txt with the reason its initializer reads no other unit's object. The fix
for a new one is constinit or constexpr, a function-local static, or a SWOFTY_ON_LOAD handler.
"""

import argparse
import collections
import glob
import os
import re
import sys

NATIVE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SDK = os.path.normpath(os.path.join(NATIVE, '../packages/sdk/include'))
MACRO_HEADERS = [SDK, os.path.join(NATIVE, 'include')]
DIAG = re.compile(r'^(.+?):(\d+):\d+: warning: declaration requires a global (constructor|destructor)')


def sdk_macros():
    names = set()
    for path in [p for root in MACRO_HEADERS for p in glob.glob(os.path.join(root, '**/*.hpp'), recursive=True)]:
        names.update(re.findall(r'^#define (SWOFTY_[A-Z0-9_]+)', open(path).read(), re.M))
    return names


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--allow', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('reports', nargs='+')
    a = ap.parse_args()

    allowed = {}
    for n, line in enumerate(open(a.allow), 1):
        line = line.rstrip('\n')
        if line.strip() and not line.startswith('#'):
            path, sep, decl = line.partition(': ')
            if not sep:
                sys.exit(f'{a.allow}:{n}: expected "<source>: <declaration>"')
            allowed[(path, decl.strip())] = n

    macros = sdk_macros()
    found = collections.OrderedDict()
    for report in a.reports:
        for line in open(report):
            diag, _, decl = line.rstrip('\n').partition('\t')
            m = DIAG.match(diag)
            if m:
                path = os.path.relpath(os.path.normpath(os.path.join(NATIVE, m.group(1))), NATIVE)
                found.setdefault((path, int(m.group(2))), (m.group(3), decl.strip()))

    dynamic, unknown, used = [], [], set()
    for (path, line), (kind, decl) in found.items():
        if kind != 'constructor':
            continue
        dynamic.append((path, line, decl))
        macro = re.match(r'(SWOFTY_[A-Z0-9_]+)\b', decl)
        if os.path.normpath(os.path.join(NATIVE, path)).startswith(SDK + os.sep) or (macro and macro.group(1) in macros):
            continue
        if (path, decl) in allowed:
            used.add((path, decl))
            continue
        unknown.append(f'{path}:{line}: {decl}')
    stale = [f'{a.allow}:{n}: {path}: {decl}' for (path, decl), n in allowed.items() if (path, decl) not in used]

    with open(a.out, 'w') as out:
        out.write('# Objects in src/core with a dynamic initializer, then those only destroyed at exit.\n')
        for path, line, decl in dynamic:
            out.write(f'init\t{path}:{line}\t{decl}\n')
        for (path, line), (kind, decl) in found.items():
            if kind == 'destructor':
                out.write(f'exit\t{path}:{line}\t{decl}\n')
    if unknown or stale:
        if unknown:
            print('init_check.py: these objects have a dynamic initializer, which runs in link order. Make them constinit '
                  'or constexpr, a function-local static or a SWOFTY_ON_LOAD handler, or, if the initializer reads no '
                  'other unit\'s object, list them with that reason in tools/init_allow.txt:', file=sys.stderr)
            print('\n'.join(f'  {u}' for u in unknown), file=sys.stderr)
        if stale:
            print('init_check.py: tools/init_allow.txt lists objects that no longer have a dynamic initializer; remove '
                  'them:', file=sys.stderr)
            print('\n'.join(f'  {s}' for s in stale), file=sys.stderr)
        os.remove(a.out)
        sys.exit(1)


if __name__ == '__main__':
    main()
