#!/usr/bin/env python3
"""Turn the loader.log of F1 25 runs into the loading measurements of re/research/loading_pipeline.md §6.

  python3 native/tools/loading_report.py RUN [RUN...] [--json OUT.json]

RUN is a session folder (sessions/<time>/ with loader.log, env.txt and end.json), a test run's batch folder
(swofty-test/<run>/batches/<n>/), a loader.log, or LABEL=PATH to name the arm. Without a label the arm is the
SWOFTY_LOADING_FEATURES value in the run's env.txt, else the features of the core's "[loading] game setting overrides
wanted:" line, else "off".

The times come from lines the core always writes: the boot stages, the flow states ("[career] flow"; a state the core
has no name for is logged once per process, at its first entry, as "#<djb2 of its node name>", which FLOW_NODES maps
back), the test agent's title and home lines and the main menu's. With SWOFTY_GAME_LOG=info and
SWOFTY_GAME_LOG_CHANNELS=default,SESSION_LOADER:info,FADE_REQUESTS:info,TELEMETRY:info,THUMBNAIL_SYSTEM:info the game's
own load, fade and thumbnail lines add detail. All times are seconds from the loader's first line. The exit code is 1
when any run shows a regression.
"""

import argparse
import json
import os
import re
import statistics
import sys
from datetime import datetime

FLOW_NODES = [
    'StateApp', 'StateEgoBoot', 'StatePersistent', 'StateFrontendFrameLock', 'StateFrontendWorld', 'StateIntro',
    'StateOnlineConnect', 'StateFTUE', 'StateFrontend', 'StateOnlineFrontend', 'StateLaunch', 'WeekendSessionManager',
    'StateCareerHubSelector', 'StateLoadSession', 'StateSession', 'StateUnloadSession', 'StateTeamHQ',
    'StateTeamHQToSession', 'CheckEnoughPlayers', 'StateTyreAllocationReadyMonitor', 'StateTyreAllocationReady',
    'StateTyreAllocationDataSync', 'StateSpectatorPostLoadMonitor', 'SpectatorPostLoadSync', 'SessionEnter',
    'TimeTrial', 'Race', 'PracticeAndQualifying', 'PreRaceSpectatorCheck', 'PreSessionProgressVirtualSessions',
    'ProgressVirtualSessions', 'SessionStateLoadReplaySession', 'SessionStateLoadMidSessionSave', 'IntroPlayersMonitor',
    'PreRaceCameraControlInhibit', 'SessionStateCheckCareerComponent', 'AudioInitTrackSession', 'IntroVideo',
    'IntroLockFrameRate', 'IntroCamerasLockFrameRate', 'IntroSessionStateBroadcastMode', 'IntroCameras',
    'TalkingPoints', 'GridRundownType', 'GridRunDown', 'F1Swipe', 'PreRace', 'PreRaceToRace', 'FormationLapType',
    'FormationLap', 'StartLights', 'PostLoadReadyMonitor', 'PostLoadDeclareReady', 'ZoomToMonitorCutscene',
    'BeginGameplay', 'InGame', 'PTS',
]


def djb2(text):
    h = 5381
    for c in text.encode():
        h = (h * 33 + c) & 0xffffffff
    return h


NODE_OF = {f'#{djb2(n):08x}': n for n in FLOW_NODES}

LINE = re.compile(r'^(\d{4}-\d\d-\d\d \d\d:\d\d:\d\d\.\d{3}) \[\s*(\d+)\] (.*)$')
GAME = re.compile(r'^\[game\] ([A-Za-z0-9_\-]+)(?:/[A-Z])?(?: \(\w+\))?: (.*)$')
FLOW = re.compile(r'^\[career\] flow "([^"]+)" (\w+) -> (\w+)')

BOOT = [
    ('game_main', re.compile(r'^trigger: KERNEL32!CreateMutexW from .*Codemasters F1 25')),
    ('window', re.compile(r'^stage: window \(')),
    ('first_frame', re.compile(r'^\[d3d12\] Present on swapchain |^\[events\] flow tick 1 observed')),
    ('title_passed', re.compile(r'^\[test\] title passed|^\[title\] engaged ')),
    ('home', re.compile(r'^\[main-menu\] home screen open|^\[test\] home view model ')),
]
BOOT_FLOW = ['StateApp', 'StateEgoBoot', 'StatePersistent', 'StateFrontendWorld', 'StateIntro', 'StateOnlineConnect',
             'StateFTUE', 'StateFrontend']
MILESTONES = ['game_main', 'window', 'first_frame'] + BOOT_FLOW[:5] + ['title_passed'] + BOOT_FLOW[5:] + ['home']
PHASES = [
    ('engine_boot', 'game_main', 'window'),
    ('device_to_first_frame', 'window', 'first_frame'),
    ('first_frame_to_title_state', 'first_frame', 'StateIntro'),
    ('title_state_to_press', 'StateIntro', 'StateOnlineConnect'),
    ('press_to_menu_flow', 'StateOnlineConnect', 'StateFrontend'),
    ('menu_flow_to_home', 'StateFrontend', 'home'),
    ('attach_to_home', None, 'home'),
]
SESSION_NODES = ['SessionEnter', 'IntroVideo', 'IntroCameras', 'TalkingPoints', 'GridRundownType', 'GridRunDown',
                 'F1Swipe', 'PreRace', 'PostLoadReadyMonitor', 'BeginGameplay', 'InGame']
# A stage ends at its node's own Exiting line when the log has one, else at the first of the nodes listed after it.
# TalkingPoints leaves by skipped, snapshot or next, and the snapshot link leads to none of the nodes listed.
STAGES = [
    ('intro_video', 'IntroVideo', ('IntroLockFrameRate', 'IntroCamerasLockFrameRate', 'IntroCameras', 'GridRundownType',
                                   'TalkingPoints', 'BeginGameplay')),
    ('talking_points', 'TalkingPoints', ('PostLoadReadyMonitor', 'F1Swipe', 'BeginGameplay')),
    ('grid_rundown', 'GridRunDown', ('F1Swipe', 'PreRace', 'PreRaceToRace')),
]

SESSION_LOAD = re.compile(r'^Session load requested for (.*)$')
TRACK_LOAD = re.compile(r'^Track load requested for (.*)$')
FADE_IN = re.compile(r'^UIModule::update - isFullyFadedOut=1, m_fullScreenFadeReasons == 0, starting fade in')
TIMES = re.compile(r'^ElasticSearchReporterModule::report_LoadingTimes(Begin|End)\((.*)\)$')
THUMB = re.compile(r'^Requesting Setting Thumbnail Generation (.*) with reason (\d+)$')
TEST_SESSION = re.compile(r'^\[test\] session\.start (\S+) (\S+)|^\[events\] session\.start \(([^,)]+)')
SETTING = re.compile(r'^\[settings\] read (bool|u32|i32|float) (\S+) = (.*?) \(status (\d+)(, forced by a loading feature)?\)$')
APPLIED = re.compile(r'^\[loading\] (\S+) read by the game .* for the (\S+) feature')
WANTED = re.compile(r'^\[loading\] game setting overrides wanted: (.*?) \(\d+ getter')
LOG_FEATURE = 'loading_log_settings'
FEATURE_SETTINGS = {
    'loading_skip_minimum_time': 'Game.IgnoreMinimumLoadingTime',
    'loading_skip_session_transition': 'Game.IgnoreMinimumLoadingTime',
    'loading_skip_session_intro_video': 'Game.SkipIntroVideo',
    'loading_skip_grid_rundown': 'Game.SkipGridRundown',
    'loading_skip_talking_points': 'Game.SkipTalkingPoints',
}
# Read only between two sessions of a weekend, which a run need not reach.
BETWEEN_SESSIONS = {'loading_skip_session_transition'}
REGRESSIONS = [
    ('crash', re.compile(r'^\[crash\] #\d+ (?!first-chance)')),
    ('crash', re.compile(r'^\[crash\] TerminateProcess')),
    ('load stall', re.compile(r'^\[stall\] load-stall report \d+')),
    ('missing PSO', re.compile(r"PSO doesn't exist")),
    ('overrides not installed', re.compile(r'^\[loading\] game setting overrides did not install')),
    ('override getter missing', re.compile(r'^\[loading\] game setting overrides: .* does not resolve')),
    ('texture preload timeout', re.compile(r'Timeout waiting for texture preload')),
    ('thumbnail system disabled', re.compile(r'^Thumbnail generation was rejected because the Thumbnail System is disabled')),
]


def read_lines(path):
    out, t0 = [], None
    with open(path, encoding='utf-8', errors='replace') as f:
        for raw in f:
            m = LINE.match(raw.rstrip('\n'))
            if not m:
                continue
            t = datetime.strptime(m.group(1), '%Y-%m-%d %H:%M:%S.%f').timestamp()
            t0 = t if t0 is None else t0
            g = GAME.match(m.group(3))
            out.append((round(t - t0, 3), g.group(1) if g else '', g.group(2) if g else m.group(3)))
    return out


def find_log(path):
    if os.path.isdir(path):
        p = os.path.join(path, 'loader.log')
        if os.path.isfile(p):
            return p, path
        raise SystemExit(f'loading_report: no loader.log in {path}')
    return path, os.path.dirname(os.path.abspath(path))


def env_features(folder):
    p = os.path.join(folder, 'env.txt')
    if not os.path.isfile(p):
        return None
    for line in open(p, encoding='utf-8', errors='replace'):
        if line.startswith('SWOFTY_LOADING_FEATURES='):
            return line.split('=', 1)[1].strip()
    return ''


def crash_end(folder):
    try:
        e = json.load(open(os.path.join(folder, 'end.json')))
    except (OSError, ValueError):
        return None
    if e.get('end') != 'crash':
        return None
    return e.get('reason') or 'the game crashed'


def norm_arm(text):
    names = sorted({n.split(' (')[0].strip() for n in text.split(',')} - {'', LOG_FEATURE})
    return ','.join(names) if names else 'off'


def flow_name(raw):
    return NODE_OF.get(raw, raw)


def analyse(path, label=None):
    log, folder = find_log(path)
    lines = read_lines(log)
    run = {'path': log, 'boot': {}, 'loads': [], 'settings': {}, 'applied': {}, 'times': [], 'regressions': [],
           'wanted': None}
    for t, ch, msg in lines:
        if not ch:
            for name, rx in BOOT:
                if name not in run['boot'] and rx.search(msg):
                    run['boot'][name] = t
            m = FLOW.match(msg)
            if m and m.group(2) == 'None' and flow_name(m.group(1)) in BOOT_FLOW:
                run['boot'].setdefault(flow_name(m.group(1)), t)
            m = WANTED.match(msg)
            if m:
                run['wanted'] = m.group(1)
            m = SETTING.match(msg)
            if m:
                run['settings'][m.group(2)] = {'type': m.group(1), 'value': m.group(3), 'status': int(m.group(4)),
                                               'forced': bool(m.group(5))}
            m = APPLIED.match(msg)
            if m:
                run['applied'].setdefault(m.group(2), m.group(1))
        for kind, rx in REGRESSIONS:
            if rx.search(msg):
                run['regressions'].append({'kind': kind, 't': t, 'line': msg[:200]})
                break
    run['loads'] = cut_loads(lines)
    run['times'] = loading_times(lines)
    if label is not None:
        run['arm'] = norm_arm(label)
    else:
        env = env_features(folder)
        run['arm'] = norm_arm(env if env is not None else run['wanted'] or '')
    crash = crash_end(folder)
    if crash:
        run['regressions'].append({'kind': 'crash', 't': None, 'line': crash})
    check(run)
    return run


def new_load(t, how):
    return {'start': t, 'start_by': how, 'name': None, 'load_exit': None, 'session_enter': None, 'session_live': None,
            'track_request': None, 'fade_in': None, 'nodes': {}, 'exits': {}, 'thumbnails': [], 'complete': False}


def cut_loads(lines):
    loads, cur = [], None
    for t, ch, msg in lines:
        m = FLOW.match(msg) if not ch else None
        name = flow_name(m.group(1)) if m else None
        if name == 'StateLoadSession' and m.group(2) == 'None':
            if cur is None or cur['start_by'] != 'game' or cur['load_exit'] is not None:
                cur = new_load(t, 'flow')
                loads.append(cur)
            continue
        g = SESSION_LOAD.match(msg) if ch == 'SESSION_LOADER' else None
        if g:
            if cur is None or cur['load_exit'] is not None:
                cur = new_load(t, 'game')
                loads.append(cur)
            cur['name'] = g.group(1)
            continue
        if cur is None:
            continue
        if name == 'StateLoadSession' and m.group(3) == 'Exiting' and cur['load_exit'] is None:
            cur['load_exit'] = t
        elif name == 'StateSession' and m.group(2) == 'None' and cur['session_enter'] is None:
            cur['session_enter'] = t
        elif name == 'StateSession' and m.group(3) == 'Updating' and cur['session_live'] is None:
            cur['session_live'] = t
            cur['complete'] = True
        elif name and m.group(2) == 'None' and cur['session_enter'] is not None:
            cur['nodes'].setdefault(name, t)
        elif name and m.group(3) == 'Exiting' and cur['session_enter'] is not None:
            cur['exits'].setdefault(name, t)
        s = TEST_SESSION.match(msg) if not ch else None
        if s and cur['name'] is None:
            cur['name'] = ' '.join(x for x in s.groups() if x)
        if ch == 'SESSION_LOADER' and TRACK_LOAD.match(msg) and cur['track_request'] is None:
            cur['track_request'] = t
        th = THUMB.match(msg) if ch.startswith('THUMBNAIL_SYSTEM') else None
        if th:
            cur['thumbnails'].append({'t': t, 'reason': int(th.group(2))})
        if ch == 'FADE_REQUESTS' and FADE_IN.match(msg) and cur['fade_in'] is None and cur['load_exit'] is not None:
            cur['fade_in'] = t
    for ld in loads:
        derive(ld)
    return loads


def since(ld, key, t):
    return round(t - ld[key], 3) if t is not None and ld[key] is not None else None


def derive(ld):
    n = ld['nodes']
    ld['load_state_s'] = since(ld, 'start', ld['load_exit'])
    ld['to_session_s'] = since(ld, 'start', ld['session_live'])
    ld['to_fade_in_s'] = since(ld, 'start', ld['fade_in'])
    ld['track_to_fade_in_s'] = since(ld, 'track_request', ld['fade_in'])
    ld['gameplay_at'] = 'InGame' if 'InGame' in n else ('PreRace' if 'PreRace' in n else None)
    ld['to_gameplay_s'] = since(ld, 'start', n.get(ld['gameplay_at']))
    for key, node, after in STAGES:
        t0 = n.get(node)
        later = [n[a] for a in after if a in n and t0 is not None and n[a] >= t0]
        if t0 is not None and ld['exits'].get(node, -1.0) >= t0:
            later.append(ld['exits'][node])
        ld[key + '_s'] = round(min(later) - t0, 3) if later else None
    ld['grid_thumbnails'] = sum(1 for x in ld['thumbnails'] if x['reason'] == 3)


def loading_times(lines):
    open_at, out = {}, []
    for t, ch, msg in lines:
        m = TIMES.match(msg) if ch == 'TELEMETRY' else None
        if not m:
            continue
        if m.group(1) == 'Begin':
            open_at[m.group(2)] = t
        elif m.group(2) in open_at:
            out.append({'name': m.group(2), 's': round(t - open_at.pop(m.group(2)), 3)})
    return out


def check(run):
    armed = set() if run['arm'] == 'off' else set(run['arm'].split(','))
    for feature, setting in FEATURE_SETTINGS.items():
        if feature in armed and feature not in run['applied'] and run['loads'] and feature not in BETWEEN_SESSIONS:
            run['regressions'].append({'kind': 'flag not applied', 't': None,
                                       'line': f'{feature} is armed but the game never read {setting}'})
        if feature not in armed and feature in run['applied']:
            run['regressions'].append({'kind': 'flag applied in the off arm', 't': None,
                                       'line': f'{feature} answered {setting} in a run whose arm does not name it'})
    for i, ld in enumerate(run['loads'], 1):
        where = f'load {i} ({ld["name"] or "session"})'
        if not ld['complete']:
            run['regressions'].append({'kind': 'load never completed', 't': ld['start'],
                                       'line': f'{where} never reached StateSession Updating'})
        if 'loading_skip_grid_rundown' in armed and ld['grid_thumbnails']:
            run['regressions'].append({'kind': 'grid thumbnails with the flag on', 't': ld['start'],
                                       'line': f'{where} still asked for {ld["grid_thumbnails"]} reason-3 thumbnail(s)'})
        if 'loading_skip_grid_rundown' in armed and (ld['grid_rundown_s'] or 0) > 1.0:
            run['regressions'].append({'kind': 'grid rundown with the flag on', 't': ld['nodes']['GridRunDown'],
                                       'line': f'{where} spent {ld["grid_rundown_s"]} s in GridRunDown'})
    if 'home' not in run['boot'] and not run['loads']:
        run['regressions'].append({'kind': 'never reached the menu', 't': None,
                                   'line': 'no home screen line and no session load in the log'})


LOAD_FIELDS = ['load_state_s', 'to_session_s', 'to_fade_in_s', 'track_to_fade_in_s', 'intro_video_s', 'talking_points_s',
               'grid_rundown_s', 'to_gameplay_s', 'grid_thumbnails']


def median(xs):
    xs = [x for x in xs if x is not None]
    return round(statistics.median(xs), 3) if xs else None


def phase(boot, a, b):
    if b not in boot or (a is not None and a not in boot):
        return None
    return round(boot[b] - (boot[a] if a else 0.0), 3)


def summarise(runs):
    arms = {}
    for r in runs:
        arms.setdefault(r['arm'], []).append(r)
    out = {}
    for arm, rs in arms.items():
        loads = {}
        for r in rs:
            for i, ld in enumerate(r['loads'], 1):
                loads.setdefault(i, []).append(ld)
        tnames = {}
        for r in rs:
            for x in r['times']:
                tnames.setdefault(x['name'], []).append(x['s'])
        out[arm] = {
            'runs': len(rs),
            'boot': {k: median([r['boot'].get(k) for r in rs]) for k in MILESTONES},
            'phases': {k: median([phase(r['boot'], a, b) for r in rs]) for k, a, b in PHASES},
            'loads': {i: dict({'runs': len(v)}, **{f: median([ld[f] for ld in v]) for f in LOAD_FIELDS})
                      for i, v in loads.items()},
            'loading_times': {k: median(v) for k, v in tnames.items()},
            'regressions': sum(len(r['regressions']) for r in rs),
        }
    return out


def fmt(x):
    return '-' if x is None else (f'{x:.3f}' if isinstance(x, float) else str(x))


def load_line(values):
    return ', '.join(f'{f} {fmt(values.get(f))}' for f in LOAD_FIELDS)


def report(runs, summary, w=None):
    w = w or sys.stdout.write
    w('# Loading report\n\n')
    for n, r in enumerate(runs, 1):
        w(f'## Run {n}: arm {r["arm"]}\n{r["path"]}\n')
        w('boot: ' + ', '.join(f'{k} {fmt(r["boot"].get(k))}' for k in MILESTONES) + '\n')
        w('phases: ' + ', '.join(f'{k} {fmt(phase(r["boot"], a, b))}' for k, a, b in PHASES) + '\n')
        if r['applied']:
            w('flags the game read: ' + ', '.join(f'{f} ({s})' for f, s in sorted(r['applied'].items())) + '\n')
        for i, ld in enumerate(r['loads'], 1):
            w(f'load {i} {ld["name"] or "session"} at {fmt(ld["start"])} (from the {ld["start_by"]} line): '
              f'{load_line(ld)}\n')
            chain = sorted((t, k) for k, t in ld['nodes'].items() if k in SESSION_NODES)
            w('  states: ' + ', '.join(f'{k} +{fmt(round(t - ld["start"], 3))}' for t, k in chain) + '\n')
        for x in r['times']:
            w(f'game loading time {x["name"]}: {fmt(x["s"])} s\n')
        for g in r['regressions']:
            w(f'REGRESSION {g["kind"]}: {g["line"]}' + (f' (t {fmt(g["t"])})' if g['t'] is not None else '') + '\n')
        w('\n')
    w('## Medians per arm\n\n')
    for arm, s in sorted(summary.items()):
        w(f'### {arm}: {s["runs"]} run(s), {s["regressions"]} regression(s)\n')
        w('M0 boot: ' + ', '.join(f'{k} {fmt(v)}' for k, v in s['boot'].items()) + '\n')
        w('M0 phases: ' + ', '.join(f'{k} {fmt(v)}' for k, v in s['phases'].items()) + '\n')
        for i, v in s['loads'].items():
            w(f'load {i} ({v["runs"]} run(s)): {load_line(v)}\n')
        for k, v in s['loading_times'].items():
            w(f'game loading time {k}: {fmt(v)} s\n')
        w('\n')
    if 'off' in summary:
        base = summary['off']
        for arm, s in sorted(summary.items()):
            if arm == 'off':
                continue
            w(f'## {arm} against off (median difference, negative is faster)\n')
            for k, v in s['phases'].items():
                if v is not None and base['phases'][k] is not None:
                    w(f'{k}: {v - base["phases"][k]:+.3f} s\n')
            for i, v in s['loads'].items():
                b = base['loads'].get(i)
                for f in LOAD_FIELDS[:-1]:
                    if b and v[f] is not None and b[f] is not None:
                        w(f'load {i} {f}: {v[f] - b[f]:+.3f} s\n')
            w('\n')
    settings = {}
    for r in runs:
        for k, v in r['settings'].items():
            settings.setdefault(k, set()).add((v['type'], v['value'], v['status'], v['forced']))
    if settings:
        w('## Settings the game read (loading_log_settings)\n\n')
        for k in sorted(settings):
            for typ, val, status, forced in sorted(settings[k]):
                where = {0: 'set in a config file', 2: 'code default'}.get(status, f'status {status}')
                w(f'{k}\t{typ}\t{val}\t{where}{" (forced)" if forced else ""}\n')
        w('\n')


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('runs', nargs='+', metavar='RUN', help='a session or batch folder, a loader.log, or LABEL=PATH')
    ap.add_argument('--json', metavar='OUT', help='also write every run and the medians as JSON')
    a = ap.parse_args(argv)
    runs = []
    for spec in a.runs:
        label, path = spec.split('=', 1) if '=' in spec and not os.path.exists(spec) else (None, spec)
        runs.append(analyse(path, label))
    summary = summarise(runs)
    report(runs, summary)
    if a.json:
        with open(a.json, 'w') as f:
            json.dump({'runs': runs, 'arms': summary}, f, indent=1)
    return 1 if any(r['regressions'] for r in runs) else 0


if __name__ == '__main__':
    sys.exit(main())
