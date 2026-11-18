#!/usr/bin/env python3
"""Fails the build when an object in src/core gets a dynamic initializer that tools/init_allow.txt does not list.

Each core object is read for what its static constructors do (tools/coff_ctors.py): the objects they initialize and
the destructors they register. tools/cxx_init.sh adds the declarations clang's -Wglobal-constructors reports, with
the innermost macro each is spelled in. clang reports an object whose class has a non-trivial destructor only as
needing a destructor, and one whose class has a virtual destructor it defaulted not at all, so an object counts as
dynamically initialized when clang says so or when its unit's constructor initializes it; a declaration is matched
to the object by its name, by the body of the macro it is spelled in, or by the line a macro-made name ends in, and
read from the unit's source when clang reported nothing. An object only destroyed at exit was constant-initialized,
reads nothing at load and is listed in the output only. Those reads take the source as it is now, so when a unit's
source no longer has the hash its report recorded as the compile started, or a header its report names is newer than
the report, the check says that file changed during the build, removes the unit's report so the next build compiles
the unit again, and fails without checking anything else.

A dynamic initializer runs in a fixed order with every other unit's, so each one must either be spelled in a
registration macro of the SDK or of include/swofty (it appends to a list in the SDK headers that is zero-initialized
before any code runs, or to a function-local static) or be listed in tools/init_allow.txt with the reason its
initializer reads no other unit's object: as "<source>: <declaration>" for one object, as "<header>: #define <MACRO>"
for every object spelled in a core macro, or as "type: <type>" for every object of that type declared without an
initializer. The fix for a new one is constinit or constexpr, a function-local static, or a SWOFTY_ON_LOAD handler.
A static constructor that refers to another unit's object fails the build the same way unless the allow list names
that reference as "refers: <source>: <object>". The constructor of an inline variable or of a class template's static
member is the same in every unit that has a copy, so its references are listed once, under the object it initializes,
as "refers: inline <variable>: <object>".

With --obj-root it also derives what the link order decides, from the objects given in link order. lld lays out the
.ctors entries in link order (in each object, its sections in order, and a COMDAT entry only from the first object
that has it) and the mingw runtime calls them from the last to the first, so units initialize in the reverse of the
link order, and the destructors they register with atexit run in the reverse of that. It writes the units with a
static constructor of their own and the inline variables they initialize, both in the order they run, the units whose
destructors run at exit in that order, the SDK mixin list in the order it is built, the on-load entries in the order
the loader runs them with the hook guard each one spelled in a macro that expands to SWOFTY_HOOK_GUARD arms, and the
units whose copies of the SDK's binding slots add to its wanted list, in order, to --order-out, then the mixin list in
the order the core's ranks give it: each unit with a mixin or an on-load entry names its CoreRank, with
SWOFTY_CORE_RANK or a macro that expands to it, before its first registration, and rank_core_mixins orders the list by
rank before the loader reads it. It fails when a hook guard's entry arms another guard than its own, when a
registering unit has no rank, a rank of its own or a rank named before its registrations, when a rank names no
registering unit, when the ranks give another order than the run order, and with --snapshot when what it writes
differs from the snapshot or a unit with static objects is not in --link-order.
"""

import argparse
import collections
import difflib
import glob
import hashlib
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import coff_ctors  # noqa: E402

NATIVE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SDK = os.path.normpath(os.path.join(NATIVE, '../packages/sdk/include'))
MACRO_HEADERS = [SDK, os.path.join(NATIVE, 'include')]
DIAG = re.compile(r'^(.+?):(\d+):\d+: warning: declaration requires a global (constructor|destructor)')
RANK_HEADER = 'src/core/runtime/loader/core_rank.hpp'
BINDING = re.compile(r'^_ZN6swofty6detail5boundI.*4addrE$')
DEFAULT_DECL = re.compile(r'(?:(?:static|inline|thread_local)\s+)*((?:\w+::)*\w+)(?:\s*<.*>)?\s+[\w\s,*&\[\]]+;')
ORDER_HEADER = """# What tools/core_link_order.txt decides in swofty_core.dll, as tools/init_check.py derives it from the core's
# objects on every build, which fails when it differs from this file. The mingw runtime runs the .ctors entries lld
# lays out in link order from the last to the first, so units initialize in the reverse of the link order. init: the
# units with a static constructor of their own, in the order they run. inline: the inline variables and static members
# of class templates with a dynamic initializer, other than the SDK's binding slots, under the unit whose copy the
# linker keeps, in the order they run. exit: the units whose destructors run at exit, in that order (the reverse of
# the order they registered them). mixins: the SDK mixin list in the order it is built. on-load: its on-load entries
# in the order the loader runs them, each hook guard's entry arming the next hook guard in HookGuardId order.
# bindings: the units whose copies of the SDK's binding slots add to its wanted list, in order. explicit: the mixin
# list in the order the core's ranks (CoreRank in src/core/runtime/loader/core_rank.hpp) put it in before the loader
# reads it, under each unit's rank; the build fails unless it is the order of the mixins and on-load sections. After
# an intended change, copy native/build/init-order.txt here.
"""

Entry = collections.namedtuple('Entry', 'path line kind decl macro')


def headers(roots):
    return [p for root in roots for p in glob.glob(os.path.join(root, '**/*.h*'), recursive=True)]


def sdk_macros():
    names = set()
    for path in headers(MACRO_HEADERS):
        names.update(re.findall(r'^\s*#\s*define\s+(\w+)', open(path).read(), re.M))
    return names


def macro_bodies():
    bodies = {}
    for path in headers(MACRO_HEADERS + [os.path.join(NATIVE, 'src/core')]):
        text = open(path, errors='replace').read().replace('\\\n', ' ')
        for name, body in re.findall(r'^\s*#\s*define\s+(\w+)(.*)$', text, re.M):
            bodies[name] = bodies.get(name, '') + body
    return bodies


def head(decl):
    m = re.match(r'(SWOFTY_\w+)\s*(?:\(\s*([^,)]*))?', decl)
    if not m:
        return decl
    return f'{m.group(1)}({m.group(2).strip()})' if m.group(2) else m.group(1)


def expands_to(name, target, bodies, seen=None):
    if name == target:
        return True
    seen = seen if seen is not None else set()
    if name in seen:
        return False
    seen.add(name)
    return any(expands_to(m, target, bodies, seen) for m in re.findall(r'\bSWOFTY_\w+', bodies.get(name, '')))


def enum_names(path, enum):
    path = os.path.join(NATIVE, path)
    m = re.search(rf'enum class {enum}[^{{]*\{{([^}}]*)\}}', open(path).read()) if os.path.isfile(path) else None
    return [n for n in re.findall(r'(\w+)\s*,', m.group(1)) if n != 'count'] if m else []


def hook_guard_ids():
    return enum_names('src/core/guards/order.hpp', 'HookGuardId')


def core_ranks():
    return enum_names(RANK_HEADER, 'CoreRank')


def rank_marks(unit, bodies):
    marks = []
    for n, text in enumerate(source_lines(unit), 1):
        m = re.match(r'\s*(SWOFTY_\w+)\s*\(\s*(\w+)', text)
        if m and expands_to(m.group(1), 'SWOFTY_CORE_RANK', bodies):
            marks.append((n, m.group(2)))
    return marks


def source_lines(path, cache={}):
    if path not in cache:
        full = os.path.join(NATIVE, path)
        cache[path] = open(full, errors='replace').read().split('\n') if os.path.isfile(full) else []
    return cache[path]


def statement(path, line):
    lines = source_lines(path)
    if not 0 < line <= len(lines):
        return ''
    start = line - 1
    while start > 0 and not re.search(r'[;{}:]\s*$|^\s*$|^\s*(//|#)', lines[start - 1]):
        start -= 1
    end = line - 1
    while end < len(lines) - 1 and ';' not in lines[end]:
        end += 1
    return re.sub(r'\s+', ' ', ' '.join(re.sub(r'//.*', '', l) for l in lines[start:end + 1])).strip()


def default_type(path, line):
    m = DEFAULT_DECL.fullmatch(statement(path, line))
    return m.group(1) if m else None


def find_decl(unit, name):
    decl = re.compile(rf'\s*(?:(?:static|inline|const|constinit|thread_local)\s+)*[\w:<>, ]*[\w>]\s*[*&]*\s*'
                      rf'{re.escape(name)}\s*(\{{|\(|=|;|\[)')
    for n, text in enumerate(source_lines(unit), 1):
        if decl.match(text) and not re.match(r'\s*(return|if|for|while|else|case)\b', text):
            return n, text.strip()
    return 0, ''


def report_entries(report):
    entries = []
    for line in open(report):
        diag, decl, macro = (line.rstrip('\n').split('\t') + ['', ''])[:3]
        m = DIAG.match(diag)
        if m:
            path = os.path.relpath(os.path.normpath(os.path.join(NATIVE, m.group(1))), NATIVE)
            entries.append(Entry(path, int(m.group(2)), m.group(3), decl.strip(), macro))
    return entries


def compiled_hash(report):
    with open(report) as f:
        first = f.readline().split()
    return first[2] if first[:2] == ['#', 'sha256'] and len(first) == 3 else None


def source_hash(path):
    full = os.path.join(NATIVE, path)
    if not os.path.isfile(full):
        return ''
    with open(full, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def changed_during_build(unit):
    changed = [unit.name] if unit.compiled is not None and source_hash(unit.name) != unit.compiled else []
    for path in dict.fromkeys(e.path for e in unit.entries):
        full = os.path.join(NATIVE, path)
        if path != unit.name and os.path.isfile(full) and os.path.getmtime(full) > unit.built:
            changed.append(path)
    return changed


def locate(symbol, unit, entries, bodies, taken):
    name = coff_ctors.leaf(symbol)
    word = re.compile(rf'(?<!\w){re.escape(name)}(?!\w)')
    hits = [e for e in entries if word.search(e.decl)]
    if not hits:
        num = re.search(r'_(\d+)$', name)
        hits = [e for e in entries if e.macro and (word.search(bodies.get(e.macro, '')) or
                                                    num and int(num.group(1)) == e.line)]
    hits.sort(key=lambda e: ((e.path, e.line, e.macro, name) in taken, e.kind != 'constructor'))
    if hits:
        taken.add((hits[0].path, hits[0].line, hits[0].macro, name))
        return hits[0]
    line, decl = find_decl(unit, name)
    return Entry(unit, line, 'constructor', decl or coff_ctors.readable(symbol), '')


class Unit:
    def __init__(self, report, obj_root, bodies):
        name = os.path.relpath(report, obj_root)
        if not name.endswith('.o.init'):
            sys.exit(f'init_check.py: {report} is not a core report under {obj_root}')
        self.name = name[:-len('.o.init')] + '.cpp'
        self.report = report
        self.built = os.path.getmtime(report)
        self.compiled = compiled_hash(report)
        self.entries = report_entries(report)
        self.ctors = coff_ctors.ctors(report[:-len('.init')])
        taken = set()
        self.objects = {}
        for c in self.ctors:
            for symbol in c.initialized:
                self.objects[symbol] = locate(symbol, self.name, self.entries, bodies, taken)


def linked(units):
    listed, kept = [], set()
    for u in units:
        for c in u.ctors:
            key = tuple(c.initialized) or c.function
            if c.comdat:
                if key in kept:
                    continue
                kept.add(key)
            listed.append((u, c))
    return listed


def registered_lines(unit):
    return sorted(e.line for e in unit.entries
                  if e.path == unit.name and e.kind == 'constructor' and e.macro in ('SWOFTY_REG_', 'SWOFTY_ON_LOAD'))


def registrations(names, by_name, hooks, bodies):
    mixins, onload, rows, armed, misarmed = [], [], [], 0, []
    for name in names:
        own = sorted((e for e in by_name[name].entries if e.path == name and e.kind == 'constructor'),
                     key=lambda e: e.line)
        for e in own:
            if e.macro not in ('SWOFTY_REG_', 'SWOFTY_ON_LOAD'):
                continue
            entry = f'{name}  {head(e.decl)}'
            mixins.append(entry)
            if e.macro == 'SWOFTY_ON_LOAD':
                spelled = re.match(r'\s*(SWOFTY_\w+)\s*(?:\(\s*(\w+))?', e.decl)
                if spelled and expands_to(spelled.group(1), 'SWOFTY_HOOK_GUARD', bodies):
                    arms = hooks[armed] if armed < len(hooks) else 'no hook guard'
                    armed += 1
                    if arms != spelled.group(2):
                        misarmed.append(f'{entry} arms {arms}')
                    entry += f', arms {arms}'
                onload.append(entry)
            rows.append(entry)
    return mixins, onload, rows, misarmed


def explicit_order(units, hooks, bodies, ranks, marks_of=rank_marks):
    by_name = {u.name: u for u in units}
    run = list(dict.fromkeys(u.name for u, c in reversed(linked(units)) if not c.comdat))
    owner, problems = {}, []
    for name in run:
        lines = registered_lines(by_name[name])
        if not lines:
            continue
        marks = marks_of(name, bodies)
        if len(marks) != 1:
            problems.append(f'{name}: ' + ('registers mixins or on-load entries but names no rank with SWOFTY_CORE_RANK'
                                           if not marks else 'names more than one rank'))
            continue
        line, rank = marks[0]
        if rank not in ranks:
            problems.append(f'{name}:{line}: CoreRank in {RANK_HEADER} has no {rank}')
            continue
        if line > lines[0]:
            problems.append(f'{name}:{line}: the rank is named after the unit\'s first registration (line {lines[0]})')
        if rank in owner:
            problems.append(f'{name}:{line}: {owner[rank]} names the same rank, {rank}')
            continue
        owner[rank] = name
    problems += [f'{RANK_HEADER}: CoreRank::{r} is no registering unit\'s rank' for r in ranks if r not in owner]
    rank_of = {name: rank for rank, name in owner.items()}
    order = [owner[r] for r in ranks if r in owner]
    mixins, onload, rows, _ = registrations(order, by_name, hooks, bodies)
    run_mixins, run_onload, _, _ = registrations([n for n in run if n in rank_of], by_name, hooks, bodies)
    if not problems and (mixins, onload) != (run_mixins, run_onload):
        problems.append('the ranks order the mixins and on-load entries otherwise than the run order:')
        problems += [l for l in difflib.unified_diff(run_mixins + ['on-load'] + run_onload, mixins + ['on-load'] + onload,
                                                    'run order', 'rank order', lineterm='', n=1)]
    text = 'explicit\n' + ''.join(f'  {rank_of[row.split("  ")[0]]}  {row}\n' for row in rows)
    return text, problems


def order_effects(units, hooks, bodies):
    run = list(reversed(linked(units)))
    init, inline, bindings, registered = [], [], [], []
    for u, c in run:
        if not c.comdat and u.name not in init:
            init.append(u.name)
        for symbol in c.initialized if c.comdat else []:
            if BINDING.match(symbol):
                if u.name not in bindings:
                    bindings.append(u.name)
            else:
                inline.append(f'{u.name}  {coff_ctors.readable(symbol)}')
        registered += [u.name] * len(c.destroyed)
    exit_order = []
    for name in reversed(registered):
        if name not in exit_order:
            exit_order.append(name)
    mixins, onload, _, misarmed = registrations(init, {u.name: u for u in units}, hooks, bodies)
    text = ORDER_HEADER
    for title, rows in (('init', init), ('inline', inline), ('exit', exit_order), ('mixins', mixins),
                        ('on-load', onload), ('bindings', bindings)):
        text += title + '\n' + ''.join(f'  {r}\n' for r in rows)
    involved = [u.name for u in units if u.name in set(init) | set(exit_order) | set(bindings) |
                {r.split('  ')[0] for r in inline}]
    return text, involved, misarmed


def references(units):
    refs = []
    for u, c in linked(units):
        source = f'inline {coff_ctors.readable((c.initialized or [c.function])[0])}' if c.comdat else u.name
        for symbol in c.external:
            if (source, coff_ctors.readable(symbol)) not in refs:
                refs.append((source, coff_ctors.readable(symbol)))
    return refs


def unlisted_references(refs, allowed_refs):
    return [r for r in refs if r not in allowed_refs], [r for r in allowed_refs if r not in refs]


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
              'tools/core_link_order.txt, or a unit that gains its first or loses its last static object, inline '
              'variable, mixin, on-load entry or binding use, changes the order the core\'s initializers, registrations, '
              'on-load entries and exit-time destructors run in. If that is intended, copy native/build/init-order.txt '
              f'to native/{a.snapshot} and commit it with the change:', file=sys.stderr)
        sys.stderr.writelines(l + '\n' for l in difflib.unified_diff(want, body(text), a.snapshot, 'init-order.txt',
                                                                     n=1, lineterm=''))
        return False
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--allow', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('--obj-root', required=True)
    ap.add_argument('--order-out')
    ap.add_argument('--link-order')
    ap.add_argument('--snapshot')
    ap.add_argument('reports', nargs='+')
    a = ap.parse_args()

    allowed, allowed_macros, allowed_types, allowed_refs = {}, {}, {}, {}
    for n, line in enumerate(open(a.allow), 1):
        line = line.rstrip('\n')
        if line.strip() and not line.startswith('#'):
            path, sep, decl = line.partition(': ')
            if not sep:
                sys.exit(f'{a.allow}:{n}: expected "<source>: <declaration>", "<header>: #define <MACRO>", '
                         '"type: <type>" or "refers: <source>: <object>"')
            m = re.fullmatch(r'#define (\w+)', decl.strip())
            if path == 'type':
                allowed_types[decl.strip()] = n
            elif path == 'refers':
                source, sep, obj = decl.strip().partition(': ')
                if not sep:
                    sys.exit(f'{a.allow}:{n}: expected "refers: <source>: <object>"')
                allowed_refs[(source, obj)] = n
            elif m:
                header = os.path.join(NATIVE, path)
                if not os.path.isfile(header) or not re.search(rf'^\s*#\s*define\s+{m.group(1)}\b', open(header).read(), re.M):
                    sys.exit(f'{a.allow}:{n}: {path} does not define {m.group(1)}')
                allowed_macros[m.group(1)] = (n, path)
            else:
                allowed[(path, decl.strip())] = n

    macros, bodies = sdk_macros(), macro_bodies()
    units = [Unit(report, a.obj_root, bodies) for report in a.reports]
    changed = {u.report: changed_during_build(u) for u in units}
    if any(changed.values()):
        for report, paths in changed.items():
            if paths:
                os.remove(report)
        for path in dict.fromkeys(p for paths in changed.values() for p in paths):
            print(f'init_check.py: {path} changed during the build; rebuild', file=sys.stderr)
        if os.path.exists(a.out):
            os.remove(a.out)
        sys.exit(1)
    found = collections.OrderedDict()
    for u in units:
        for e in u.entries:
            found.setdefault((e.path, e.line, e.macro), [e.kind, e.decl, 'clang'])
        for e in u.objects.values():
            got = found.setdefault((e.path, e.line, e.macro), ['constructor', e.decl, 'object'])
            if got[0] == 'destructor':
                got[0], got[2] = 'constructor', 'object'

    def where(path, line, macro):
        return f'{path}:{line}' + (f' (in {macro})' if macro else '')

    dynamic, unknown, used = [], [], set()
    for (path, line, macro), (kind, decl, origin) in found.items():
        if kind != 'constructor':
            continue
        dynamic.append((path, line, macro, decl, origin))
        if os.path.normpath(os.path.join(NATIVE, path)).startswith(SDK + os.sep) or macro in macros:
            continue
        if macro in allowed_macros:
            used.add(macro)
            continue
        if (path, decl) in allowed:
            used.add((path, decl))
            continue
        typ = default_type(path, line) if not macro else None
        if typ in allowed_types:
            used.add(('type', typ))
            continue
        unknown.append(f'{where(path, line, macro)}: {decl}')
    stale = [f'{a.allow}:{n}: {path}: {decl}' for (path, decl), n in allowed.items() if (path, decl) not in used]
    stale += [f'{a.allow}:{n}: {path}: #define {name}' for name, (n, path) in allowed_macros.items() if name not in used]
    stale += [f'{a.allow}:{n}: type: {typ}' for typ, n in allowed_types.items() if ('type', typ) not in used]
    refs = references(units)
    new_refs, gone_refs = unlisted_references(refs, allowed_refs)
    stale += [f'{a.allow}:{allowed_refs[r]}: refers: {r[0]}: {r[1]}' for r in gone_refs]

    with open(a.out, 'w') as out:
        out.write('# Objects in src/core with a dynamic initializer, then those only destroyed at exit, then the other '
                  'units\' objects\n# a unit\'s constructor refers to. "object" marks one clang does not report as '
                  'dynamically initialized,\n# found in its unit\'s constructor.\n')
        for path, line, macro, decl, origin in dynamic:
            out.write(f'init\t{where(path, line, macro)}\t{decl}' + ('\tobject\n' if origin == 'object' else '\n'))
        for (path, line, macro), (kind, decl, origin) in found.items():
            if kind == 'destructor':
                out.write(f'exit\t{where(path, line, macro)}\t{decl}\n')
        for source, obj in refs:
            out.write(f'refers\t{source}\t{obj}\n')
    order_ok = True
    if a.order_out:
        text, involved, misarmed = order_effects(units, hook_guard_ids(), bodies)
        explicit, rank_problems = explicit_order(units, hook_guard_ids(), bodies, core_ranks())
        text += explicit
        with open(a.order_out, 'w') as out:
            out.write(text)
        if misarmed:
            print('init_check.py: these hook guards\' on-load entries arm another hook guard than their own. Each arms the '
                  'next one in HookGuardId order, so src/core/guards/order.hpp must list the hook guards in the order '
                  'their on-load entries run (the on-load section of native/build/init-order.txt):', file=sys.stderr)
            print('\n'.join(f'  {m}' for m in misarmed), file=sys.stderr)
            order_ok = False
        if rank_problems:
            print(f'init_check.py: the core\'s ranks do not order its SDK registrations. Every unit with a mixin or an '
                  f'on-load entry names its rank in CoreRank ({RANK_HEADER}) with SWOFTY_CORE_RANK before its first '
                  'registration, or gets it from SWOFTY_HOOK_GUARD, and CoreRank lists those units in the order the '
                  'mixins section of native/build/init-order.txt runs them in:', file=sys.stderr)
            print('\n'.join(f'  {p}' for p in rank_problems), file=sys.stderr)
            order_ok = False
        if a.snapshot:
            order_ok = check_order(text, involved, a) and order_ok
    if unknown or stale or new_refs or not order_ok:
        if new_refs:
            print('init_check.py: these units\' static constructors refer to another unit\'s object, whose own '
                  'initializer may not have run yet. Read it from a function-local static or a SWOFTY_ON_LOAD handler '
                  'instead, or, if the constructor only takes its address, list it with that reason in '
                  'tools/init_allow.txt as "refers: <source>: <object>", or "refers: inline <variable>: <object>" for '
                  'an inline variable\'s constructor:', file=sys.stderr)
            print('\n'.join(f'  {source}: {obj}' for source, obj in new_refs), file=sys.stderr)
        if unknown:
            print('init_check.py: these objects have a dynamic initializer, which runs in a fixed order with every other '
                  'unit\'s. Make them constinit or constexpr, a function-local static or a SWOFTY_ON_LOAD handler, or, '
                  'if the initializer reads no other unit\'s object, list them with that reason in tools/init_allow.txt:',
                  file=sys.stderr)
            print('\n'.join(f'  {u}' for u in unknown), file=sys.stderr)
        if stale:
            print('init_check.py: tools/init_allow.txt lists objects that no longer have a dynamic initializer, or '
                  'references no constructor makes any more; remove them:', file=sys.stderr)
            print('\n'.join(f'  {s}' for s in stale), file=sys.stderr)
        os.remove(a.out)
        sys.exit(1)


if __name__ == '__main__':
    main()
