#!/usr/bin/env python3
"""Writes the ninja file native/test/run_tests.sh runs the native tests from (the list is native/test/manifest.py).

Each Unit compiles to an object with a depfile and links to one executable; its run is a target whose output is
<builddir>/<name>/ok, so ninja reruns a unit test only when its executable, a file its args name or one of its
`inputs` changed. A Script reruns every time, since ninja cannot see what it runs, after its Programs are built
into <builddir>/<name>. Objects
compile once under <builddir>/obj for every Program that lists them with the same target and flags. A Program with a
`when` condition the host does not meet (`sanitizers`: the host compiler links -fsanitize=address,undefined) is left
out, and its script runs without it. `test` is
every host test, `test-wine` every Wine test, and each test's name and each group is a target of its own.

Ninja starts first the edges with the longest chain of edges behind them. A unit's compile has two (its link and its
run), and the run of a script without programs, which takes longest, has none, so PRIORITY stamp edges follow each
such run and one follows the run of a script with programs: those scripts start first, then their programs' compiles,
then the units. A Program of kind 'obj' has a stamp edge in place of a link, so its compile is not left with the
units. Among edges with chains alike ninja starts the one written first, and the compiles are written first, largest
source first, so the longest compiles start early instead of finishing last.

With --jobs and --wine-jobs, compiles, links and host test runs share a pool of --jobs slots and Wine test runs a
pool of --wine-jobs, so a Wine test, which spends most of its time waiting on Wine, holds no slot a compile could use
when ninja runs with both counts' sum.
"""

import argparse
import glob
import hashlib
import os
import shlex
import subprocess
import sys
import tempfile

NATIVE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(NATIVE, 'test'))

import manifest  # noqa: E402

UNIT_FLAGS = ['-std=c++23', '-O2', '-Wall', '-Wextra', '-Isrc', '-I../packages/sdk/include']
ZIG_FLAGS = ['-Wno-nullability-completeness', '-Wno-missing-designated-field-initializers']
RESERVED = {'test', 'test-wine', 'always', 'build', 'include', 'src', 'third_party', 'tools', 'dist', 'obj'}
PRIORITY = 4


def quote(args):
    return ' '.join(shlex.quote(a) for a in args)


def host_has(cxx, flags):
    with tempfile.TemporaryDirectory() as d:
        try:
            return subprocess.run([cxx, '-x', 'c++', *flags, '-o', os.path.join(d, 'probe'), '-'], input=b'int main(){}',
                                  capture_output=True).returncode == 0
        except OSError:
            return False


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--builddir', required=True)
    ap.add_argument('--out', required=True)
    ap.add_argument('--host-cxx', default='c++')
    ap.add_argument('--jobs', type=int, help='slots for compiles, links and host test runs')
    ap.add_argument('--wine-jobs', type=int, help='slots for Wine test runs')
    a = ap.parse_args()
    b = a.builddir
    cpu, wine_pool = ('  pool = cpu', '  pool = wine') if a.jobs and a.wine_jobs else (None, None)

    lines = [
        'ninja_required_version = 1.10',
        f'builddir = {b}',
        f'hostcxx = {a.host_cxx}',
        'zigcxx = zig c++ -target x86_64-linux-musl',
        '',
        *(['pool cpu', f'  depth = {a.jobs}', 'pool wine', f'  depth = {a.wine_jobs}', ''] if cpu else []),
        'rule cxx_gcc',
        f'  command = $hostcxx {quote(UNIT_FLAGS)} -pthread -MD -MF $out.d -c $in -o $out',
        '  description = CXX $in',
        '  depfile = $out.d',
        '  deps = gcc',
        cpu,
        'rule link_gcc',
        '  command = $hostcxx -pthread -o $out $in',
        '  description = LINK $out',
        cpu,
        'rule cxx_zig',
        f'  command = $zigcxx {quote(UNIT_FLAGS + ZIG_FLAGS)} -MD -MF $out.d -c $in -o $out',
        '  description = CXX $in',
        '  depfile = $out.d',
        '  deps = gcc',
        cpu,
        'rule link_zig',
        '  command = $zigcxx -O2 -o $out $in',
        '  description = LINK $out',
        cpu,
        'rule cc_program',
        '  command = zig cc -target $target $flags -MD -MF $out.d -c $in -o $out',
        '  description = CC $in',
        '  depfile = $out.d',
        '  deps = gcc',
        cpu,
        'rule cxx_program',
        '  command = zig c++ -target $target $flags -MD -MF $out.d -c $in -o $out',
        '  description = CXX $in',
        '  depfile = $out.d',
        '  deps = gcc',
        cpu,
        'rule link_program',
        '  command = zig $driver -target $target $ldflags -o $out $in',
        '  description = LINK $out',
        cpu,
        'rule cxx_host',
        '  command = $hostcxx $flags -MD -MF $out.d -c $in -o $out',
        '  description = CXX $in',
        '  depfile = $out.d',
        '  deps = gcc',
        cpu,
        'rule link_host',
        '  command = $hostcxx $ldflags -o $out $in',
        '  description = LINK $out',
        cpu,
        'rule run',
        '  command = test/lib/run_one.sh $name $kind $dir $cwd $cmd',
        '  description = TEST $name',
        cpu,
        'rule run_wine',
        '  command = test/lib/run_one.sh $name $kind $dir $cwd $cmd',
        '  description = TEST $name',
        wine_pool,
        'rule stamp',
        '  command = touch $out',
        '  description = DONE $name',
        '',
        'build always: phony',
        '',
    ]

    names, host, wine, table, groups, compiled = set(), [], [], [], {}, {}
    rules, lines, compiles = [line for line in lines if line is not None], [], []

    def emit_compile(source, edge):
        compiles.append((-os.path.getsize(os.path.join(NATIVE, source)), len(compiles), edge))
    available = {'': True, 'sanitizers': host_has(a.host_cxx, ['-fsanitize=address,undefined'])}

    def claim(name):
        if name in names or name in RESERVED:
            sys.exit(f'test_graph.py: test name {name!r} is taken')
        names.add(name)

    def compile_into(path, objects, source):
        if objects.target == manifest.HOST:
            rule = 'cxx_host'
        else:
            rule = 'cxx_program' if source.endswith('.cpp') else 'cc_program'
        if not os.path.isfile(os.path.join(NATIVE, source)):
            sys.exit(f'test_graph.py: {source} is missing')
        emit_compile(source, [f'build {path}: {rule} {source}', f'  target = {objects.target}',
                              f'  flags = {quote(objects.flags)}'])

    def shared_objects(objects):
        tag = hashlib.sha1(repr((objects.target, objects.flags)).encode()).hexdigest()[:10]
        paths = []
        for source in objects.sources:
            key = (objects.target, tuple(objects.flags), source)
            if key not in compiled:
                compiled[key] = f'{b}/obj/{tag}/{os.path.splitext(source)[0]}.o'
                compile_into(compiled[key], objects, source)
            paths.append(compiled[key])
        return paths

    def program(d, p):
        out = f'{d}/{p.out}'
        if p.kind == 'obj':
            if len(p.objects) != 1 or len(p.objects[0].sources) != 1:
                sys.exit(f'test_graph.py: {out} is an object, so it compiles one source')
            compile_into(out, p.objects[0], p.objects[0].sources[0])
            lines.extend([f'build {out}.built: stamp {out}', f'  name = {p.out}'])
            return f'{out}.built'
        targets = {o.target for o in p.objects}
        if len(targets) != 1:
            sys.exit(f'test_graph.py: {out} mixes targets {sorted(targets)}')
        inputs = [path for o in p.objects for path in shared_objects(o)]
        cxx = any(source.endswith('.cpp') for o in p.objects for source in o.sources)
        target = targets.pop()
        ldflags = quote((['-shared'] if p.kind == 'dll' else []) + p.ldflags)
        lines.extend([f'build {out}: {"link_host" if target == manifest.HOST else "link_program"} {" ".join(inputs)}',
                      f'  target = {target}', f'  driver = {"c++" if cxx else "cc"}', f'  ldflags = {ldflags}'])
        return out

    def ranked(name, ok, stamps):
        last = ok
        for n in range(1, stamps + 1):
            lines.extend([f'build {ok}.{n}: stamp {last}', f'  name = {name}'])
            last = f'{ok}.{n}'
        return last

    for s in manifest.SCRIPTS:
        claim(s.name)
        d = f'{b}/{s.name}'
        dir_abs = os.path.abspath(os.path.join(NATIVE, d))
        for p in s.programs:
            if p.when not in available:
                sys.exit(f'test_graph.py: {s.name}: {p.out} builds when {p.when!r}, which is not a condition')
        built = [program(d, p) for p in s.programs if available[p.when]]
        implicit = ' | ' + ' '.join(built) if built else ''
        after = ' || ' + ' '.join(f'{b}/{x}/ok' for x in s.after) if s.after else ''
        lines.append(f'build {d}/ok: {"run_wine" if s.wine else "run"} always{implicit}{after}')
        lines += [f'  name = {s.name}', f'  kind = {"wine" if s.wine else "host"}', f'  dir = {dir_abs}', f'  cwd = {s.cwd}',
                  f'  cmd = {quote([part.replace("{out}", dir_abs) for part in s.command])}']
        lines.append(f'build {s.name}: phony {d}/ok')
        done = ranked(s.name, f'{d}/ok', 1 if s.programs else PRIORITY)
        if s.group:
            groups.setdefault(s.group, []).append(done)
        (wine if s.wine else host).append(done)
        table.append((s.name, 'wine' if s.wine else 'host', s.title))

    for u in manifest.UNITS:
        claim(u.name)
        if not os.path.isfile(os.path.join(NATIVE, u.source)):
            sys.exit(f'test_graph.py: {u.source} is missing')
        d = f'{b}/{u.name}'
        exe = f'{d}/{u.name}_test'
        emit_compile(u.source, [f'build {exe}.o: cxx_{u.compiler} {u.source}'])
        lines.append(f'build {exe}: link_{u.compiler} {exe}.o')
        inputs = [os.path.normpath(os.path.join('test', x)) for x in u.args if os.path.isfile(os.path.join(NATIVE, 'test', x))]
        inputs += sorted(os.path.relpath(path, NATIVE) for pattern in u.inputs
                         for path in glob.glob(os.path.join(NATIVE, pattern), recursive=True))
        lines.append(f'build {d}/ok: run {exe}' + (' | ' + ' '.join(inputs) if inputs else ''))
        lines += [f'  name = {u.name}', '  kind = host', f'  dir = {os.path.abspath(os.path.join(NATIVE, d))}', '  cwd = test',
                  f'  cmd = {quote([os.path.abspath(os.path.join(NATIVE, exe))] + u.args)}']
        lines.append(f'build {u.name}: phony {d}/ok')
        host.append(f'{d}/ok')
        table.append((u.name, 'host', u.title))

    for s in manifest.SCRIPTS:
        unknown = [x for x in s.after if x not in names]
        if unknown:
            sys.exit(f'test_graph.py: {s.name} runs after {unknown}, which are not tests')
    for group, done in groups.items():
        claim(group)
        lines.append(f'build {group}: phony {" ".join(done)}')
    lines = rules + [line for _, _, edge in sorted(compiles) for line in edge] + [''] + lines
    lines += ['', f'build test: phony {" ".join(host)}', f'build test-wine: phony {" ".join(wine)}', 'default test', '']
    write_if_changed(a.out, '\n'.join(lines))
    write_if_changed(os.path.join(os.path.dirname(a.out), 'tests.tsv'),
                     ''.join(f'{n}\t{kind}\t{title}\n' for n, kind, title in table))


def write_if_changed(path, text):
    try:
        if open(path).read() == text:
            return
    except OSError:
        pass
    with open(path, 'w') as f:
        f.write(text)


if __name__ == '__main__':
    main()
