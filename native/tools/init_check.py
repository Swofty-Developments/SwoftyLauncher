#!/usr/bin/env python3
"""Fails the build when an object in src/core gets a dynamic initializer that tools/init_allow.txt does not list.

tools/cxx_init.sh reports every namespace-scope or static-member object that clang's -Wglobal-constructors flags,
with the innermost macro it is spelled in. An object that is only destroyed at exit was constant-initialized, so it
reads nothing at load and is listed in the output only. A dynamic initializer runs in link order, before or after
another unit's, so each one must either be spelled in a registration macro of the SDK or of include/swofty (it
appends to a list in the SDK headers that is zero-initialized before any code runs, or to a function-local static)
or be listed in tools/init_allow.txt with the reason its initializer reads no other unit's object: as
"<source>: <declaration>" for one object, or as "<header>: #define <MACRO>" for every object spelled in a core macro.
The fix for a new one is constinit or constexpr, a function-local static, or a SWOFTY_ON_LOAD handler.

With --obj-root it also derives what the link order decides from the reports, given in link order: the units with
static objects (init: a dynamic initializer, exit: a destructor run at exit, in reverse), the SDK mixin list they
append to, the on-load entries in the order the loader runs them with the hook guard each SWOFTY_HOOK_GUARD entry
arms, and the units whose binding uses add to the SDK's wanted list. It writes that to --order-out and, with
--snapshot, fails when it differs from the snapshot or a unit with static objects is not in --link-order.
"""

import argparse
import collections
import difflib
import glob
import os
import re
import sys

NATIVE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SDK = os.path.normpath(os.path.join(NATIVE, '../packages/sdk/include'))
MACRO_HEADERS = [SDK, os.path.join(NATIVE, 'include')]
DIAG = re.compile(r'^(.+?):(\d+):\d+: warning: declaration requires a global (constructor|destructor)')
ORDER_HEADER = """# What tools/core_link_order.txt decides in swofty_core.dll, as tools/init_check.py derives it from the core's
# compile reports on every build, which fails when it differs from this file. units: the units with static objects in
# link order (init: a dynamic initializer, exit: a destructor, which runs at exit in the reverse order). mixins: the
# SDK mixin list in the order the units append to it. on-load: its on-load entries in the order the loader runs them,
# each SWOFTY_HOOK_GUARD entry arming the next hook guard in HookGuardId order. bindings: the units whose binding uses
# add to the SDK's wanted list, in order. After an intended change, copy native/build/init-order.txt here.
"""


def sdk_macros():
    names = set()
    for path in [p for root in MACRO_HEADERS for p in glob.glob(os.path.join(root, '**/*.h*'), recursive=True)]:
        names.update(re.findall(r'^\s*#\s*define\s+(\w+)', open(path).read(), re.M))
    return names


def head(decl):
    m = re.match(r'(SWOFTY_\w+)\s*(?:\(\s*([^,)]*))?', decl)
    if not m:
        return decl
    return f'{m.group(1)}({m.group(2).strip()})' if m.group(2) else m.group(1)


def hook_guard_ids():
    path = os.path.join(NATIVE, 'src/core/guards/order.hpp')
    m = re.search(r'enum class HookGuardId[^{]*\{([^}]*)\}', open(path).read()) if os.path.isfile(path) else None
    return [n for n in re.findall(r'(\w+)\s*,', m.group(1)) if n != 'count'] if m else []


def link_order_effects(reports, obj_root):
    units, mixins, onload, bindings = [], [], [], []
    hooks, armed = hook_guard_ids(), 0
    for report in reports:
        unit = os.path.relpath(report, obj_root)
        if not unit.endswith('.o.init'):
            sys.exit(f'init_check.py: {report} is not a core report under {obj_root}')
        unit = unit[:-len('.o.init')] + '.cpp'
        entries = []
        for line in open(report):
            diag, decl, macro = (line.rstrip('\n').split('\t') + ['', ''])[:3]
            m = DIAG.match(diag)
            if m:
                path = os.path.relpath(os.path.normpath(os.path.join(NATIVE, m.group(1))), NATIVE)
                entries.append((path, int(m.group(2)), m.group(3), decl.strip(), macro))
        if not entries:
            continue
        kinds = [k for k in ('init', 'exit') if any((e[2] == 'constructor') == (k == 'init') for e in entries)]
        units.append(f'{unit}  {" ".join(kinds)}')
        own = sorted((e for e in entries if e[0] == unit and e[2] == 'constructor'), key=lambda e: e[1])
        for path, line, kind, decl, macro in own:
            if macro not in ('SWOFTY_REG_', 'SWOFTY_ON_LOAD'):
                continue
            mixins.append(f'{unit}  {head(decl)}')
            if macro == 'SWOFTY_ON_LOAD':
                entry = f'{unit}  {head(decl)}'
                if decl.startswith('SWOFTY_HOOK_GUARD'):
                    entry += f', arms {hooks[armed] if armed < len(hooks) else "no hook guard"}'
                    armed += 1
                onload.append(entry)
        if any(e[2] == 'constructor' and e[0].endswith('swofty/mixin.hpp') and 'wanted(' in e[3] for e in entries):
            bindings.append(unit)
    text = ORDER_HEADER
    for name, rows in (('units', units), ('mixins', mixins), ('on-load', onload), ('bindings', bindings)):
        text += name + '\n' + ''.join(f'  {r}\n' for r in rows)
    return text, [u.split('  ')[0] for u in units]


def check_order(text, units, a):
    listed = {line.strip() for line in open(a.link_order) if line.strip() and not line.startswith('#')}
    unpinned = [u for u in units if u not in listed]
    if unpinned:
        print(f'init_check.py: these sources have static objects but {a.link_order} does not list them, so they link '
              'after every listed source in path order. Pin each one by adding it to that file where its initializers, '
              'mixins, on-load entries and exit-time destructors must run, then copy native/build/init-order.txt to '
              f'{a.snapshot}:', file=sys.stderr)
        print('\n'.join(f'  {u}' for u in unpinned), file=sys.stderr)
        return False
    body = lambda t: [line for line in t.splitlines() if not line.startswith('#')]
    want = body(open(a.snapshot).read()) if os.path.isfile(a.snapshot) else []
    if body(text) != want:
        print(f'init_check.py: what the link order decides differs from {a.snapshot}. A reordered '
              'tools/core_link_order.txt, or a unit that gains its first or loses its last static object, mixin, on-load '
              'entry or binding use, changes the order the core\'s initializers, registrations, on-load entries and '
              'exit-time destructors run in. If that is intended, copy native/build/init-order.txt to '
              f'native/{a.snapshot} and commit it with the change:', file=sys.stderr)
        sys.stderr.writelines(l + '\n' for l in difflib.unified_diff(want, body(text), a.snapshot, 'init-order.txt',
                                                                     n=1, lineterm=''))
        return False
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--allow', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('--obj-root')
    ap.add_argument('--order-out')
    ap.add_argument('--link-order')
    ap.add_argument('--snapshot')
    ap.add_argument('reports', nargs='+')
    a = ap.parse_args()

    allowed, allowed_macros = {}, {}
    for n, line in enumerate(open(a.allow), 1):
        line = line.rstrip('\n')
        if line.strip() and not line.startswith('#'):
            path, sep, decl = line.partition(': ')
            if not sep:
                sys.exit(f'{a.allow}:{n}: expected "<source>: <declaration>" or "<header>: #define <MACRO>"')
            m = re.fullmatch(r'#define (\w+)', decl.strip())
            if m:
                header = os.path.join(NATIVE, path)
                if not os.path.isfile(header) or not re.search(rf'^\s*#\s*define\s+{m.group(1)}\b', open(header).read(), re.M):
                    sys.exit(f'{a.allow}:{n}: {path} does not define {m.group(1)}')
                allowed_macros[m.group(1)] = (n, path)
            else:
                allowed[(path, decl.strip())] = n

    macros = sdk_macros()
    found = collections.OrderedDict()
    for report in a.reports:
        for line in open(report):
            diag, decl, macro = (line.rstrip('\n').split('\t') + ['', ''])[:3]
            m = DIAG.match(diag)
            if m:
                path = os.path.relpath(os.path.normpath(os.path.join(NATIVE, m.group(1))), NATIVE)
                found.setdefault((path, int(m.group(2)), macro), (m.group(3), decl.strip()))

    def where(path, line, macro):
        return f'{path}:{line}' + (f' (in {macro})' if macro else '')

    dynamic, unknown, used = [], [], set()
    for (path, line, macro), (kind, decl) in found.items():
        if kind != 'constructor':
            continue
        dynamic.append((path, line, macro, decl))
        if os.path.normpath(os.path.join(NATIVE, path)).startswith(SDK + os.sep) or macro in macros:
            continue
        if macro in allowed_macros:
            used.add(macro)
            continue
        if (path, decl) in allowed:
            used.add((path, decl))
            continue
        unknown.append(f'{where(path, line, macro)}: {decl}')
    stale = [f'{a.allow}:{n}: {path}: {decl}' for (path, decl), n in allowed.items() if (path, decl) not in used]
    stale += [f'{a.allow}:{n}: {path}: #define {name}' for name, (n, path) in allowed_macros.items() if name not in used]

    with open(a.out, 'w') as out:
        out.write('# Objects in src/core with a dynamic initializer, then those only destroyed at exit.\n')
        for path, line, macro, decl in dynamic:
            out.write(f'init\t{where(path, line, macro)}\t{decl}\n')
        for (path, line, macro), (kind, decl) in found.items():
            if kind == 'destructor':
                out.write(f'exit\t{where(path, line, macro)}\t{decl}\n')
    order_ok = True
    if a.obj_root:
        text, units = link_order_effects(a.reports, a.obj_root)
        with open(a.order_out, 'w') as out:
            out.write(text)
        if a.snapshot:
            order_ok = check_order(text, units, a)
    if unknown or stale or not order_ok:
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
