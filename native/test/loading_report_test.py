import io
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'tools'))
import loading_report as lr

DAY = '2026-11-17'


def stamp(s):
    whole = int(s)
    ms = int(round((s - whole) * 1000))
    return f'{DAY} 10:{whole // 60:02d}:{whole % 60:02d}.{ms:03d}'


def line(t, msg, tid=580):
    return f'{stamp(t)} [{tid:5d}] {msg}\n'


def flow(t, name, a='None', b='Entering'):
    shown = name if name in ('StateLoadSession', 'StateSession') else f'#{lr.djb2(name):08x}'
    first = ' (first sighting)' if shown.startswith('#') else ''
    return line(t, f'[career] flow "{shown}" {a} -> {b}{first}')


def boot(press_at=20.0):
    return ''.join([
        line(0.0, 'swofty loader (version.dll proxy) attached'),
        line(0.4, "trigger: KERNEL32!CreateMutexW from 0000000140178667 (.tls$) name 'Codemasters F1 25' on thread 384"),
        line(3.5, 'stage: window (USER32!RegisterClassExW from 00000001439f2670 (.tls$)), addons were loaded at the early stage'),
        flow(5.4, 'StateApp'),
        flow(5.5, 'StateEgoBoot'),
        line(5.5, '[d3d12] Present on swapchain 000000005E575430 from sl.common.dll (00006FFFF1128ED4) on thread 384'),
        flow(7.5, 'StatePersistent'),
        flow(8.6, 'StateFrontendWorld'),
        flow(10.6, 'StateIntro'),
        line(press_at - 0.1, "[test] title passed after 3 press(es) of the virtual pad's a"),
        flow(press_at, 'StateOnlineConnect'),
        flow(press_at + 0.02, 'StateFTUE'),
        flow(press_at + 6.5, 'StateFrontend'),
        line(press_at + 8.0, '[main-menu] home screen open: state read, stock top-right strip hidden'),
    ])


def race_load(t0, video_s, rundown_s, thumbnails=2, game_lines=True):
    out = [flow(t0, 'StateLoadSession'), flow(t0 + 0.05, 'StateLoadSession', 'Entering', 'Updating')]
    if game_lines:
        out.append(line(t0 + 0.06, '[game] SESSION_LOADER: Session load requested for race'))
        out.append(line(t0 + 1.0, '[game] SESSION_LOADER: Track load requested for bahrain'))
        for i in range(thumbnails):
            out.append(line(t0 + 2.0 + i * 0.1, f'[game] THUMBNAIL_SYSTEM: Requesting Setting Thumbnail Generation d{i} with reason 3'))
    out += [
        flow(t0 + 9.0, 'StateLoadSession', 'Updating', 'Exiting'),
        flow(t0 + 9.0, 'StateLoadSession', 'Exiting', 'None'),
        flow(t0 + 9.2, 'StateSession'),
        flow(t0 + 9.7, 'StateSession', 'Entering', 'Updating'),
        line(t0 + 9.7, '[events] session.start (grand_prix, swofty.box/f1_2025): 0 handlers'),
        flow(t0 + 9.8, 'SessionEnter'),
        flow(t0 + 10.0, 'IntroVideo'),
        flow(t0 + 10.0 + video_s, 'IntroCameras'),
        flow(t0 + 10.5 + video_s, 'GridRundownType'),
        flow(t0 + 10.6 + video_s, 'GridRunDown'),
        flow(t0 + 10.6 + video_s + rundown_s, 'F1Swipe'),
        flow(t0 + 11.6 + video_s + rundown_s, 'PreRace'),
    ]
    if game_lines:
        out.append(line(t0 + 10.1, '[game] FADE_REQUESTS: UIModule::update - isFullyFadedOut=1, m_fullScreenFadeReasons == 0, '
                                   'starting fade in'))
    return ''.join(out)


def write_run(root, name, text, env=None, end=None):
    d = os.path.join(root, name)
    os.makedirs(d)
    with open(os.path.join(d, 'loader.log'), 'w') as f:
        f.write(text)
    if env is not None:
        with open(os.path.join(d, 'env.txt'), 'w') as f:
            f.write(f'SWOFTY_GAME_LOG=info\nSWOFTY_LOADING_FEATURES={env}\n')
    if end is not None:
        with open(os.path.join(d, 'end.json'), 'w') as f:
            json.dump(end, f)
    return d


ON = 'loading_skip_minimum_time,loading_skip_session_intro_video,loading_skip_grid_rundown,loading_skip_talking_points'
APPLIED = ''.join(
    line(30.0, f'[loading] {s} read by the game (it had 0, status 0): answered 1 for the {f} feature, so it is skipped')
    for f, s in lr.FEATURE_SETTINGS.items() if f in ON.split(','))


class Names(unittest.TestCase):
    def test_flow_hashes_match_the_core(self):
        self.assertEqual(lr.NODE_OF['#ba16bc16'], 'InGame')
        self.assertEqual(lr.NODE_OF['#c8a977c8'], 'IntroVideo')
        self.assertEqual(lr.NODE_OF['#a4d3a4b2'], 'StateIntro')
        self.assertEqual(lr.NODE_OF['#e11edb87'], 'StateApp')

    def test_arm_names_leave_out_the_logging_feature(self):
        self.assertEqual(lr.norm_arm('loading_skip_grid_rundown (SWOFTY_LOADING_FEATURES), loading_log_settings'),
                         'loading_skip_grid_rundown')
        self.assertEqual(lr.norm_arm(''), 'off')
        self.assertEqual(lr.norm_arm('loading_log_settings'), 'off')


class Report(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name

    def tearDown(self):
        self.tmp.cleanup()

    def test_boot_and_race_load_numbers(self):
        r = lr.analyse(write_run(self.root, 'off1', boot() + race_load(40.0, 2.0, 6.0), env=''))
        b = r['boot']
        self.assertEqual(r['arm'], 'off')
        self.assertAlmostEqual(b['window'] - b['game_main'], 3.1, places=3)
        self.assertAlmostEqual(lr.phase(b, 'StateIntro', 'StateOnlineConnect'), 9.4, places=3)
        self.assertAlmostEqual(lr.phase(b, 'StateOnlineConnect', 'StateFrontend'), 6.5, places=3)
        self.assertAlmostEqual(lr.phase(b, None, 'home'), 28.0, places=3)
        self.assertEqual(len(r['loads']), 1)
        ld = r['loads'][0]
        self.assertEqual(ld['name'], 'race')
        self.assertEqual(ld['start_by'], 'flow')
        self.assertAlmostEqual(ld['load_state_s'], 9.0, places=3)
        self.assertAlmostEqual(ld['to_session_s'], 9.7, places=3)
        self.assertAlmostEqual(ld['intro_video_s'], 2.0, places=3)
        self.assertAlmostEqual(ld['grid_rundown_s'], 6.0, places=3)
        self.assertEqual(ld['gameplay_at'], 'PreRace')
        self.assertAlmostEqual(ld['to_gameplay_s'], 19.6, places=3)
        self.assertAlmostEqual(ld['to_fade_in_s'], 10.1, places=3)
        self.assertAlmostEqual(ld['track_to_fade_in_s'], 9.1, places=3)
        self.assertEqual(ld['grid_thumbnails'], 2)
        self.assertEqual(r['regressions'], [])

    def test_talking_points_end_at_their_own_exit(self):
        t0 = 40.0
        text = boot() + race_load(t0, 2.0, 6.0) + ''.join([
            flow(t0 + 30.0, 'TalkingPoints'),
            flow(t0 + 30.05, 'TalkingPoints', 'Entering', 'Updating'),
            flow(t0 + 30.1, 'TalkingPoints', 'Updating', 'Exiting'),
            flow(t0 + 30.1, 'SnapshotTarget'),
            flow(t0 + 35.0, 'BeginGameplay'),
        ])
        ld = lr.analyse(write_run(self.root, 'tp', text, env='loading_skip_talking_points'))['loads'][0]
        self.assertAlmostEqual(ld['talking_points_s'], 0.1, places=3)

    def test_ab_medians_and_difference(self):
        runs = []
        for i, (video, rundown) in enumerate([(2.0, 6.0), (2.2, 6.4), (1.8, 5.8)]):
            runs.append(lr.analyse(write_run(self.root, f'off{i}', boot() + race_load(40.0, video, rundown), env='')))
        for i in range(3):
            text = boot() + APPLIED + race_load(40.0, 0.1, 0.2, thumbnails=0)
            runs.append(lr.analyse(write_run(self.root, f'on{i}', text, env=ON)))
        s = lr.summarise(runs)
        on = lr.norm_arm(ON)
        self.assertEqual(set(s), {'off', on})
        self.assertEqual(s['off']['loads'][1]['intro_video_s'], 2.0)
        self.assertEqual(s['off']['loads'][1]['grid_rundown_s'], 6.0)
        self.assertEqual(s[on]['loads'][1]['grid_rundown_s'], 0.2)
        self.assertEqual(s[on]['regressions'], 0)
        out = io.StringIO()
        lr.report(runs, s, out.write)
        text = out.getvalue()
        self.assertIn(f'## {on} against off', text)
        self.assertIn('load 1 grid_rundown_s: -5.800 s', text)
        self.assertIn('load 1 intro_video_s: -1.900 s', text)

    def test_arm_from_the_core_line_when_env_is_missing(self):
        text = line(0.1, '[loading] game setting overrides wanted: loading_skip_minimum_time (SWOFTY_LOADING_FEATURES), '
                         'loading_log_settings (2 getter(s))') + boot()
        r = lr.analyse(write_run(self.root, 'batch', text))
        self.assertEqual(r['arm'], 'loading_skip_minimum_time')
        self.assertEqual(r['regressions'], [], 'a run with no load has nothing to read the setting')
        r = lr.analyse(os.path.join(self.root, 'batch'), label='off')
        self.assertEqual(r['arm'], 'off')
        r = lr.analyse(write_run(self.root, 'loaded', text + race_load(40.0, 2.0, 6.0)))
        self.assertEqual([g['kind'] for g in r['regressions']], ['flag not applied'])

    def test_the_default_arm_needs_no_session_transition(self):
        wanted = line(0.1, '[loading] game setting overrides wanted: loading_skip_minimum_time, '
                           'loading_skip_session_transition (1 getter(s))')
        minimum = line(41.0, '[loading] Game.IgnoreMinimumLoadingTime read by the game (it had 0 (0x0), status 0): '
                             'answered 1 (0x1) for the loading_skip_minimum_time feature, so the 3 s minimum is dropped')
        transition = line(60.0, '[loading] Game.IgnoreMinimumLoadingTime read by the game (it had 0 (0x0), status 0): '
                                'answered 1 (0x1) for the loading_skip_session_transition feature, so it is dropped')
        r = lr.analyse(write_run(self.root, 'default', wanted + boot() + race_load(40.0, 2.0, 6.0) + minimum))
        self.assertEqual(r['arm'], 'loading_skip_minimum_time,loading_skip_session_transition')
        self.assertEqual(r['regressions'], [])
        self.assertEqual(sorted(r['applied']), ['loading_skip_minimum_time'])
        r = lr.analyse(write_run(self.root, 'opted_out', boot() + race_load(40.0, 2.0, 6.0) + transition))
        self.assertEqual(r['arm'], 'off')
        self.assertEqual([g['kind'] for g in r['regressions']], ['flag applied in the off arm'])

    def test_regressions(self):
        text = boot() + race_load(40.0, 2.0, 6.0) + flow(80.0, 'StateLoadSession') + line(
            90.0, '[stall] load-stall report 1 of at most 2: the load has not moved for 45.9 s')
        r = lr.analyse(write_run(self.root, 'bad', text, env='loading_skip_grid_rundown',
                                 end={'end': 'crash', 'exit_code': 3221225477, 'reason': 'ACCESS_VIOLATION'}))
        kinds = sorted(g['kind'] for g in r['regressions'])
        self.assertEqual(kinds, ['crash', 'flag not applied', 'grid rundown with the flag on',
                                 'grid thumbnails with the flag on', 'load never completed', 'load stall'])
        r = lr.analyse(write_run(self.root, 'menuless', line(0.0, 'swofty loader (version.dll proxy) attached'), env=''))
        self.assertEqual([g['kind'] for g in r['regressions']], ['never reached the menu'])

    def test_quit_is_not_a_crash(self):
        r = lr.analyse(write_run(self.root, 'quit', boot(), env='', end={'end': 'unknown', 'exit_code': 1}))
        self.assertEqual(r['regressions'], [])

    def test_settings_lines(self):
        text = boot() + ''.join([
            line(6.0, '[settings] read u32 ErpLoader.readBufferSize = 4194304 (0x400000) (status 0)'),
            line(6.0, '[settings] read bool Game.SkipIntroVideo = 1 (status 0, forced by a loading feature)'),
            line(6.1, '[settings] read i32 Renderer.Gfx.maxPsoCreationThreads = 8 (status 2)'),
        ])
        r = lr.analyse(write_run(self.root, 'm0', text, env='loading_log_settings'))
        self.assertEqual(r['arm'], 'off')
        self.assertEqual(r['settings']['ErpLoader.readBufferSize']['value'], '4194304 (0x400000)')
        self.assertTrue(r['settings']['Game.SkipIntroVideo']['forced'])
        self.assertEqual(r['settings']['Renderer.Gfx.maxPsoCreationThreads']['status'], 2)
        out = io.StringIO()
        lr.report([r], lr.summarise([r]), out.write)
        self.assertIn('Renderer.Gfx.maxPsoCreationThreads\ti32\t8\tcode default', out.getvalue())

    def test_main_exit_code_and_json(self):
        good = write_run(self.root, 'g', boot() + race_load(40.0, 2.0, 6.0), env='')
        path = os.path.join(self.root, 'out.json')
        saved = sys.stdout
        sys.stdout = io.StringIO()
        try:
            self.assertEqual(lr.main([good, '--json', path]), 0)
            bad = write_run(self.root, 'b', boot() + flow(40.0, 'StateLoadSession'), env='')
            self.assertEqual(lr.main([f'off={bad}']), 1)
        finally:
            sys.stdout = saved
        with open(path) as f:
            data = json.load(f)
        self.assertEqual(data['arms']['off']['loads']['1']['grid_rundown_s'], 6.0)


if __name__ == '__main__':
    unittest.main()
