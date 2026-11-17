#!/usr/bin/env python3
"""Writes expected.txt: the off, needs and settle lines every SiteGuard logged before the guard registry, taken from
the guard sources at the commit the registry started from (git show 2a13cf459:<path>).

Each guard's lines are the literal format strings of its old offer and commit functions with every compile-time
constant substituted; the environment variable's value stays %s.
"""

import re
import subprocess
import sys
from pathlib import Path

BASE = '2a13cf459'
HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]

GUARDS = {
    'ai_corner_guard': 'ai/ai_corner_guard.cpp',
    'ai_line_set': 'ai/ai_line_set_guard.cpp',
    'chassis_registry': 'cars/chassis_registry.cpp',
    'ers_mode': 'cars/ers_mode.cpp',
    'ers_newlap': 'cars/ers_newlap.cpp',
    'tyre_slots': 'cars/tyre_slots.cpp',
    'standings_guard': 'career/standings_guard.cpp',
    'myteam_vehicle_guard': 'career/myteam_vehicle_guard.cpp',
    'audio_queue': 'engine/audio_queue.cpp',
    'watchdog_grace': 'engine/watchdog_grace.cpp',
    'fe_tile_guard': 'frontend/fe_tile_guard.cpp',
    'garage_crew': 'garage/garage_crew.cpp',
    'garage_setup': 'garage/garage_setup.cpp',
    'garage_slots': 'garage/garage_slots.cpp',
    'pit_slots': 'garage/pit_slots.cpp',
    'estimate_laptime': 'session/estimate_laptime_guard.cpp',
    'red_flag_recompute': 'session/red_flag_recompute_guard.cpp',
    'replay_grid_guard': 'session/replay_grid_guard.cpp',
    'session_enter': 'session/session_enter.cpp',
    'team_slots': 'session/team_slots.cpp',
    'thrustmaster_lcd': 'session/thrustmaster_lcd.cpp',
}

TOKEN = re.compile(r'\s+|//[^\n]*|"(?:[^"\\]|\\.)*"|[A-Za-z_][\w:]*|.', re.S)


def tokens(text):
    return [t for t in TOKEN.findall(text) if t.strip() and not t.startswith('//')]


def function(text, header):
    a = text.index(header)
    b = text.index('\n}\n', a)
    return text[a:b]


def log_calls(body):
    toks = tokens(body)
    calls = []
    for i, t in enumerate(toks):
        if t != 'swofty::log':
            continue
        depth, args, cur = 0, [], []
        for t2 in toks[i + 1:]:
            if t2 == '(':
                depth += 1
                if depth == 1:
                    continue
            elif t2 == ')':
                depth -= 1
                if depth == 0:
                    args.append(cur)
                    break
            if t2 == ',' and depth == 1:
                args.append(cur)
                cur = []
            else:
                cur.append(t2)
        calls.append(args)
    return calls


def literal(toks):
    return ''.join(bytes(t[1:-1], 'utf-8').decode('unicode_escape') for t in toks)


def value(toks, constants):
    if all(t.startswith('"') for t in toks):
        return literal(toks)
    if len(toks) == 1 and toks[0] in constants:
        return constants[toks[0]]
    if len(toks) == 1 and toks[0] == 'v':
        return '%s'
    raise ValueError(f'cannot evaluate {toks}')


def render(args, constants, choose=None):
    fmt = literal(args[0])
    vals = []
    for a in args[1:]:
        if '?' in a:
            q, c = a.index('?'), a.index(':')
            vals.append(value(a[q + 1:c] if choose == 'armed' else a[c + 1:], constants))
        else:
            vals.append(value(a, constants))
    parts = fmt.split('%s')
    assert len(parts) == len(vals) + 1, (fmt, vals)
    return ''.join(p + (vals[i] if i < len(vals) else '') for i, p in enumerate(parts))


def main():
    out = []
    for stem, rel in GUARDS.items():
        path = f'native/src/core/guards/{rel}'
        text = subprocess.run(['git', '-C', str(ROOT), 'show', f'{BASE}:{path}'], check=True, capture_output=True,
                              text=True).stdout
        constants = {m[0]: m[1] for m in re.findall(r'constexpr const char \*(\w+) = ("(?:[^"\\]|\\.)*");', text)}
        constants = {k: literal([v]) for k, v in constants.items()}
        offer = log_calls(function(text, f'std::vector<CoreMixins> {stem}_mixins()'))
        commit = log_calls(function(text, f'void {stem}_commit()'))
        off = render(offer[0], constants)
        needs = render(offer[1], constants)
        armed = render(commit[0], constants, 'armed')
        not_installed = render(commit[0], constants, 'not_installed')
        assert ' off (' in off and 'does not resolve' in needs, stem
        out += [f'[{stem}]', f'off {off}', f'unresolved {needs}', f'armed {armed}', f'not_installed {not_installed}', '']
    (HERE / 'expected.txt').write_text('\n'.join(out))
    print(f'wrote {len(GUARDS)} guards to {HERE / "expected.txt"}')


if __name__ == '__main__':
    sys.exit(main())
