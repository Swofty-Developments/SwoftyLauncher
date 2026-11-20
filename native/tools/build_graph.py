#!/usr/bin/env python3
"""Writes the ninja file native/build.sh builds the three DLLs from.

Every C++ file under src/core is a translation unit of swofty_core.dll unless CORE_EXCLUDE names it, so a new
source file needs no build change. Each translation unit compiles to its own object under <builddir>/obj with a
depfile, so ninja rebuilds exactly what an edit touches and reruns a compile whose command line changed.
swofty_core.dll links its objects in the order tools/core_link_order.txt gives; their static initializers, SDK
registrations and on-load entries run in the reverse of that order (the mingw runtime calls the .ctors list from its
end) and their static destructors in it. Every core compile also reports its objects clang finds with a dynamic
initializer (tools/cxx_init.sh), and tools/init_check.py reads the objects for the ones it misses and fails the build
on one that tools/init_allow.txt does not list, so no initializer reads another unit's object.
"""

import argparse
import os
import shlex
import sys

NATIVE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CORE_EXCLUDE = {
    # The fake core's registry engine: the CLI's addon tests and the SDK recorder link it, the core does not.
    'src/core/registry/engine.cpp',
}

# Structured exception handling (__try/__except) only parses with -fms-extensions. These compile without the core's
# defines and game headers, as they always have.
SEH_SOURCES = {
    'src/core/hooks/guard.cpp': ['-I$sdk'],
    'src/core/diagnostics/crash/stall_walk.cpp': [],
    'src/core/diagnostics/perf/pdh.cpp': ['-I$sdk'],
}

SAFETYHOOK = 'third_party/safetyhook'
ZSTD = 'third_party/zstd'
ZSTD_FLAGS = ['-O2', '-DXXH_NAMESPACE=ZSTD_', '-DZSTD_LEGACY_SUPPORT=0', '-DZSTD_DISABLE_ASM', '-DDEBUGLEVEL=0']


def sources(root, exts):
    found = []
    for dirpath, dirnames, filenames in os.walk(os.path.join(NATIVE, root)):
        dirnames.sort()
        for name in sorted(filenames):
            if name.endswith(exts):
                found.append(os.path.relpath(os.path.join(dirpath, name), NATIVE))
    return found


def link_order(objects):
    listed = [line.strip() for line in open(os.path.join(NATIVE, 'tools/core_link_order.txt'))
              if line.strip() and not line.startswith('#')]
    gone = [src for src in listed if src not in objects]
    if gone:
        sys.exit(f'build_graph.py: tools/core_link_order.txt names sources that are not built: {gone}')
    return [objects[src] for src in listed] + [objects[src] for src in sorted(set(objects) - set(listed))]


def q(args):
    return ' '.join(a if a.startswith('$') else shlex.quote(a) for a in args)


class Graph:
    def __init__(self, builddir):
        self.builddir = builddir
        self.lines = []

    def var(self, name, value):
        self.lines.append(f'{name} = {value}')

    def rule(self, name, command, description, depfile=False):
        self.lines.append(f'rule {name}')
        self.lines.append(f'  command = {command}')
        self.lines.append(f'  description = {description}')
        if depfile:
            self.lines.append('  depfile = $out.d')
            self.lines.append('  deps = gcc')
        self.lines.append('')

    def build(self, outputs, rule, inputs, flags=None):
        line = f'build {" ".join(outputs)}: {rule} {" ".join(inputs)}'
        self.lines.append(line)
        if flags is not None:
            self.lines.append(f'  flags = {q(flags)}')

    def obj(self, source):
        return f'{self.builddir}/obj/{os.path.splitext(source)[0]}.o'

    def compile(self, rule, source, flags, report=False):
        out = self.obj(source)
        self.build([out] + ([f'| {out}.init'] if report else []), rule, [source] + (['| tools/cxx_init.sh'] if report else []),
                   flags)
        return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--builddir', required=True)
    ap.add_argument('--out', required=True, help='where the ninja file goes')
    ap.add_argument('--cc', required=True)
    ap.add_argument('--cxx', required=True)
    ap.add_argument('--sdk', required=True)
    ap.add_argument('--define', action='append', default=[])
    ap.add_argument('--game-include', default='')
    ap.add_argument('--strip', default='-s')
    ap.add_argument('--order-snapshot', help='the link order effects the core build must keep (tools/init_check.py)')
    a = ap.parse_args()

    g = Graph(a.builddir)
    g.var('ninja_required_version', '1.10')
    g.var('builddir', a.builddir)
    g.var('cc', a.cc)
    g.var('cxx', a.cxx)
    g.var('sdk', a.sdk)
    g.var('strip', a.strip)
    g.lines.append('')
    g.rule('cc', '$cc $flags -MD -MF $out.d -c $in -o $out', 'CC $in', depfile=True)
    g.rule('cxx', '$cxx $flags -MD -MF $out.d -c $in -o $out', 'CXX $in', depfile=True)
    g.rule('cxx_core', 'sh tools/cxx_init.sh $out.init $in $cxx $flags -MD -MF $out.d -c $in -o $out', 'CXX $in', depfile=True)
    g.rule('init_check', 'python3 tools/init_check.py --allow tools/init_allow.txt $flags $in', 'CHECK static initializers')
    g.rule('link_c', '$cc $flags -shared -o $out $in $strip', 'LINK $out')
    g.rule('link_cxx', '$cxx $flags -shared -o $out $in $strip', 'LINK $out')

    warn = ['-O2', '-Wall', '-Wextra']
    game = [f'-I{a.game_include}'] if a.game_include else []

    # Objects of a stripped DLL compile without debug info ($strip), which keeps their code what the single-command
    # build made. The SEH sources and third_party never took it.
    loader_flags = warn + ['-I$sdk', '-Iinclude', f'-I{a.builddir}', '-D__USE_MINGW_ANSI_STDIO=1', '$strip']
    loader = [g.compile('cc', 'src/loader/version.c', loader_flags)]
    g.build([f'{a.builddir}/version.dll'], 'link_c', loader + ['src/loader/version.def'], warn)

    core_flags = warn + a.define + ['-Isrc', '-I$sdk'] + game + ['-Iinclude', f'-I{SAFETYHOOK}', f'-I{ZSTD}', '$strip']
    objects = {f'{SAFETYHOOK}/safetyhook.cpp': g.compile('cxx', f'{SAFETYHOOK}/safetyhook.cpp', ['-O2', f'-I{SAFETYHOOK}'])}
    objects[f'{SAFETYHOOK}/Zydis.c'] = g.compile('cc', f'{SAFETYHOOK}/Zydis.c', ['-O2', f'-I{SAFETYHOOK}'])
    for sub in ('common', 'compress', 'decompress'):
        for src in sources(f'{ZSTD}/{sub}', ('.c',)):
            objects[src] = g.compile('cc', src, ZSTD_FLAGS + [f'-I{ZSTD}'])
    core_sources = sources('src/core', ('.cpp',))
    core = set()
    for src in core_sources:
        if src in CORE_EXCLUDE:
            continue
        if src in SEH_SOURCES:
            objects[src] = g.compile('cxx_core', src, warn + ['-fms-extensions', '-Isrc'] + SEH_SOURCES[src], True)
        else:
            objects[src] = g.compile('cxx_core', src, core_flags, True)
        core.add(objects[src])
    missing = (CORE_EXCLUDE | set(SEH_SOURCES)) - set(core_sources)
    if missing:
        sys.exit(f'build_graph.py: listed sources that no longer exist: {sorted(missing)}')
    g.build([f'{a.builddir}/swofty_core.dll'], 'link_cxx', link_order(objects), warn)
    reports = [f'{o}.init' for o in link_order(objects) if o in core]
    check = ['--out', f'{a.builddir}/static-init.txt', '--obj-root', f'{a.builddir}/obj', '--order-out',
             f'{a.builddir}/init-order.txt', '--link-order', 'tools/core_link_order.txt']
    checked = ['tools/init_check.py', 'tools/coff_ctors.py', 'tools/init_allow.txt', 'tools/core_link_order.txt',
               'src/core/guards/order.hpp', 'src/core/runtime/loader/core_rank.hpp']
    if a.order_snapshot:
        check += ['--snapshot', a.order_snapshot]
        checked.append(a.order_snapshot)
    g.build([f'{a.builddir}/static-init.txt', f'{a.builddir}/init-order.txt'], 'init_check',
            reports + ['| ' + ' '.join(checked)], check)

    gamelog = [g.compile('cxx', 'src/gamelog/gamelog.cpp', warn + ['-fms-extensions', '-Isrc', '-I$sdk', '-Iinclude', '$strip'])]
    g.build([f'{a.builddir}/swofty_gamelog.dll'], 'link_cxx', gamelog, warn)
    g.lines.append('')

    text = '\n'.join(g.lines)
    try:
        if open(a.out).read() == text:
            return
    except OSError:
        pass
    with open(a.out, 'w') as f:
        f.write(text)


if __name__ == '__main__':
    main()
