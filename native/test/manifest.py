"""Every native test: what it builds, how it runs and the title CI reports it under.

tools/test_graph.py turns this list into ninja targets. A Unit is one test source built into one executable, with
the host C++ compiler or with zig, that runs from native/test. A Script runs a command, mostly a shell script, on
every invocation, after the scripts its `after` names. A Script's Programs are built into its folder in the graph
first, where Programs of different scripts share every object they compile alike, and `{out}` in its command is that
folder. Objects for the target `host` build with the host C++ compiler; a Program `when='sanitizers'` builds only
when that compiler links -fsanitize=address,undefined. Wine tests start a fake F1_25.exe under Wine and run only in
CI. The Wine test's cases (the CASES line of run_wine_test.sh) are scripts of their own that share one setup and run
side by side; the group `wine` runs them all.
"""

import glob
import os
import re
from dataclasses import dataclass, field

LINUX, WINDOWS, HOST = 'x86_64-linux-musl', 'x86_64-windows-gnu', 'host'


@dataclass
class Unit:
    """A test source built into one executable. `inputs` are the files, as globs relative to native/, it reads
    beyond the files its `args` name, so a change to one runs it again."""
    name: str
    title: str
    compiler: str = 'gcc'
    args: list = field(default_factory=list)
    inputs: list = field(default_factory=list)

    @property
    def source(self):
        return f'test/{self.name}_test.cpp'


@dataclass
class Objects:
    """Sources, relative to native/, compiled with the same flags for one target: a zig target triple or HOST."""
    sources: list
    flags: list
    target: str = LINUX


@dataclass
class Program:
    """An executable, a DLL (kind 'dll') or the object of one source (kind 'obj') linked from Objects."""
    out: str
    objects: list
    kind: str = 'exe'
    ldflags: list = field(default_factory=list)
    when: str = ''


@dataclass
class Script:
    name: str
    title: str
    command: list
    wine: bool = False
    after: list = field(default_factory=list)
    group: str = ''
    programs: list = field(default_factory=list)
    cwd: str = '..'


# Built with the host C++ compiler, or with zig for x86_64-linux-musl where the compiler is zig.
UNITS = [
    Unit('abi_layout', 'Live car ABI layout unit test'),
    Unit('activity_resume', 'Activity resume guard unit test'),
    Unit('ai_corner', 'AI brake line guard unit test'),
    Unit('ai_curves', 'AI team curves guard unit test'),
    Unit('ai_line_set', 'AI line set guard unit test'),
    Unit('audio_queue', 'Audio job queue fix unit test'),
    Unit('direct_start', 'Direct start unit test'),
    Unit('direct_start_tt', 'Direct start Time Trial unit test'),
    Unit('characters_logic', 'Character override rules unit test'),
    Unit('dred', 'GPU device removal report unit test'),
    Unit('report_text', 'Crash report section text unit test'),
    Unit('hook_set', 'Core mixin set state unit test'),
    Unit('binding_policy', 'Binding drop policy unit test'),
    Unit('ers_newlap', 'ERS new-lap site unit test'),
    Unit('ers_mode', 'ERS mode guard offset unit test'),
    Unit('cutscene_team', 'Cutscene team record guard unit test'),
    Unit('estimate_laptime', 'Estimate lap time guard unit test'),
    Unit('fe_tile_guard', 'Front-end tile list guard unit test'),
    Unit('game_message_log', 'Stock game message log unit test'),
    Unit('garage_crew', 'Garage crew guard unit test'),
    Unit('garage_setup', 'Garage setup guard unit test'),
    Unit('garage_slots', 'Garage slot guard unit test'),
    Unit('gp_continue', 'Grand Prix continue guard unit test'),
    Unit('gp_track', 'Grand Prix remembered-track fallback unit test'),
    Unit('gp_weekend_track', 'Weekend track guard unit test'),
    Unit('grade', 'Shader look unit test'),
    Unit('heap_sizes', 'Heap size table unit test'),
    Unit('hud_leaves', 'HUD leaf table unit test'),
    Unit('hud_osd', 'HUD runtime headers test'),
    Unit('hub_calendar', 'Career hub calendar diagnostic unit test'),
    Unit('lap_marks', 'Lap timing marks and lap host table unit test'),
    Unit('leaderboard_guard', 'Leaderboard guard unit test'),
    Unit('live_car', 'Live car signal mapping unit test'),
    Unit('load_stall', 'Load-stall watchdog trigger unit test'),
    Unit('log_csv', 'CSV recorder unit test'),
    Unit('main_menu', 'Main menu logic unit test'),
    Unit('mem_scan', 'Memory scan helpers unit test'),
    Unit('myteam_vehicle_guard', 'My Team vehicle base guard unit test'),
    Unit('myteam_vehicle_heal', 'My Team vehicle heal unit test'),
    Unit('myteam_vehicle_keep', 'My Team vehicle keep unit test'),
    Unit('pe_imports', 'Import table walk unit test (rewritten entries skipped, valid slots patched)', 'zig'),
    Unit('perf_logic', 'Profiler minute summary unit test'),
    Unit('pit_box', 'Pit box guard unit test'),
    Unit('pit_garage_state', 'Pit garage state guard unit test'),
    Unit('pit_slots', 'Pit slot guard unit test'),
    Unit('placed_record', 'placed record bound unit test'),
    Unit('guard_lines', 'Site guard log lines test',
         args=['../src/core/guards', 'guard_lines/expected.txt', 'guard_lines/added.txt', 'guard_lines/logged.txt'],
         inputs=['src/core/guards/**/*.cpp']),
    Unit('guard_registry', 'Core mixin-set registry order test', args=['../src/core'],
         inputs=['tools/core_link_order.txt', 'src/core/**/*.cpp']),
    Unit('guard_skips', 'Guard first-N log cap unit test'),
    Unit('guard_on_demand', 'On-demand guard arming unit test'),
    Unit('hashing', 'Shared hash helpers unit test'),
    Unit('steps', 'Boot step and slow addon init log line unit test'),
    Unit('report_queue', 'Registry report writer queue unit test'),
    Unit('protection_spans', 'Binding pass protection spans unit test'),
    Unit('game_mem_logic', 'Data binding write ledger unit test'),
    Unit('game_settings', 'Game setting override table unit test'),
    Unit('db_first_read', 'Database first-read probe unit test'),
    Unit('settings_logic', 'Addon settings model and requests file unit test'),
    Unit('player_name', 'Player name sanitising unit test'),
    Unit('postfx', 'Post-process pass logic unit test'),
    Unit('race_control', 'Race control events unit test', 'zig'),
    Unit('tyre_damage', 'Tyre damage events unit test', 'zig'),
    Unit('speech', 'Speech events unit test', 'zig'),
    Unit('race_retirement', 'Race retirement driver guard unit test'),
    Unit('red_flag_recompute', 'Red flag recompute guard unit test'),
    Unit('replay_grid', 'Replay recorder grid guard unit test'),
    Unit('rnd_suppliers', 'R&D supplier budget unit test'),
    Unit('rules', 'Shader rules engine unit test', args=['../../packages/shaders/tests/rules_vectors.json']),
    Unit('save_heap', 'Save temp heap size unit test'),
    Unit('sequencer_records', 'Events sequencer record guard unit test'),
    Unit('session_enter', 'Session enter guard unit test'),
    Unit('session_job_probe', 'Session job list scan probe unit test'),
    Unit('session_loader', 'Load-stall report session loader section unit test'),
    Unit('setup_vm', 'Setup screen car guard unit test'),
    Unit('srv_clamp', 'Shader resource view clamp unit test'),
    Unit('srv_hook', 'Shader resource view addon callbacks unit test'),
    Unit('rtas_guard', 'Ray tracing build guard unit test'),
    Unit('d3d12_probe', 'D3D12 probe device sharing unit test'),
    Unit('arm_gate', 'D3D12 arm gate unit test'),
    Unit('asset_template', 'Asset group template archives unit test'),
    Unit('font_upload', 'Overlay font atlas upload unit test'),
    Unit('lang_scope', 'String scope unit test'),
    Unit('stall_order', 'Load-stall report running-threads order unit test'),
    Unit('standings_drop', 'Standings dropped scores unit test'),
    Unit('standings_read', 'Season standings read unit test'),
    Unit('teamhq_track', 'Team HQ track guard unit test'),
    Unit('teammate_pairs', 'Teammate pairs guard unit test'),
    Unit('theatre_list', 'Theatre list guard unit test'),
    Unit('thrustmaster_lcd', 'Thrustmaster LCD guard unit test'),
    Unit('resume_gate', 'Resume Career gate unit test'),
    Unit('resume_press', 'Resume list row press checks unit test'),
    Unit('tile_list_select', 'Tile list select guard unit test'),
    Unit('timing_live', 'Live to timing tower adapter test'),
    Unit('toast', 'Lap toast message and state machine unit test'),
    Unit('track_acclim', 'Track acclimatisation guard decision unit test'),
    Unit('track_list_guard', 'Career calendar track list guard unit test'),
    Unit('track_record', 'Round track record guard unit test'),
    Unit('track_select', 'Track select guard confirm decision unit test'),
    Unit('build_proof', 'Per-build proof gates unit test'),
    Unit('mixin_gate', 'Core mixin set gate and commit lines unit test'),
    Unit('core_rank', 'Core mixin list rank order unit test'),
    Unit('picker_log_caps', 'Era picker log caps unit test'),
    Unit('tt_loaded', 'Time Trial loaded list board and team label unit test'),
    Unit('tyre_curve', 'Tyre curve guard unit test'),
    Unit('anim_clip_time', 'Anim clip time guard unit test'),
    Unit('marshal_lights', 'Marshal lights guard unit test'),
    Unit('anim_packet_time', 'Anim packet time guard unit test'),
    Unit('tyre_models', 'Tyre model check unit test'),
    Unit('tyre_slots', 'Tyre slot guard unit test'),
    Unit('ui_registry_walk', 'UI registry walk unit test'),
    Unit('watchdog_grace', 'Boot watchdog grace unit test'),
    Unit('weather_chain', 'Weather setup chain order rules unit test'),
    Unit('events_dispatch', 'Events dispatcher unit test', 'zig'),
    Unit('session_lifecycle', 'Session lifecycle kind unit test', 'zig'),
    Unit('career_state', 'Career and flow-state tracking unit test', 'zig'),
    Unit('career_start', 'Career start gate unit test', 'zig'),
    Unit('career_content', 'Career content library unit test', 'zig'),
    Unit('era_logic', 'Era picker logic unit test', 'zig'),
    Unit('box_grid', 'Box grid rules unit test', 'zig'),
    Unit('calendar_track', 'Career calendar track guard unit test'),
    Unit('car_category', 'Car category boxes unit test', 'zig'),
    Unit('save_splice', 'Save splice unit test', 'zig'),
    Unit('career_saves', 'Career saves index unit test', 'zig'),
    Unit('plain_saves', 'Plain save store unit test', 'zig'),
    Unit('bag_scan', 'Bag scan and read stream unit test', 'zig'),
    Unit('entitlements', 'Entitlement check unit test', 'zig'),
    Unit('room_cover', 'Loading cover unit test', 'zig'),
    Unit('session_env', 'Loader session environment file unit test', 'zig'),
    Unit('testagent', 'Test agent logic unit test', 'zig'),
    Unit('storage_logic', 'Storage library path unit test', 'zig'),
    Unit('crash_dir', 'Crash report folder unit test', 'zig'),
    Unit('exit_logic', 'Crash report exit decision unit test', 'zig'),
    Unit('hw_settings', 'Hardware settings probe unit test', 'zig'),
    Unit('pacing_logic', 'Frame pacing logic unit test'),
    Unit('state_logic', 'State folder unit test', 'zig'),
    Unit('udp_ids', 'UDP id rewrite unit test', 'zig'),
    Unit('udp_telemetry', 'UDP telemetry feed fill unit test', 'zig'),
    Unit('view_flow', 'View flow node unit test', 'zig'),
    Unit('pause_edge', 'Pause edge unit test', 'zig'),
    Unit('telemetry_feed', 'Telemetry feed unit test', 'zig'),
    Unit('save_diag', 'Save diagnostics log formatting unit test', 'zig'),
]

SDK = '-I../packages/sdk/include'
ENGINE = [f'src/core/registry/{x}.cpp' for x in ('store', 'table', 'validate', 'schema', 'engine')]
SAFETYHOOK = [Objects(['third_party/safetyhook/safetyhook.cpp'], ['-std=c++23', '-O2', '-Ithird_party/safetyhook']),
              Objects(['third_party/safetyhook/Zydis.c'], ['-O2', '-Ithird_party/safetyhook'])]
ZSTD = Objects(sorted(glob.glob('third_party/zstd/*/*.c', root_dir=os.path.dirname(os.path.dirname(os.path.abspath(__file__))))),
               ['-O2', '-DXXH_NAMESPACE=ZSTD_', '-DZSTD_LEGACY_SUPPORT=0', '-DZSTD_DISABLE_ASM', '-DDEBUGLEVEL=0',
                '-Ithird_party/zstd'])

REGISTRY_FLAGS = ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra', '-Wno-nullability-completeness',
                  '-Wno-cast-function-type-mismatch', SDK, '-DSWOFTY_FAKE_CORE_ENGINE=swofty_registry_engine']
REGISTRY_ENGINE = Objects(ENGINE, REGISTRY_FLAGS)
REGISTRY = [
    *[Program(test, [Objects([f'test/registry/{test}.cpp'] + extra, REGISTRY_FLAGS), REGISTRY_ENGINE])
      for test, extra in [('corpus_test', []), ('tolerant_edit_test', []), ('iterate_test', []), ('index_load_test', []),
                          ('declare_all_test', ['src/core/registry/older.cpp']), ('header_schemas_test', []), ('career_wait_test', []),
                          ('older_test', ['src/core/registry/older.cpp']), ('older_batch_test', ['src/core/registry/older.cpp']), ('career_scope_test', [])]],
    Program('store.windows.o', [Objects(['src/core/registry/store.cpp'], ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra', SDK], WINDOWS)], 'obj'),
    Program('pe_exports_test', [Objects(['test/registry/pe_exports_test.c'], ['-O2', '-Wall', '-Wextra', '-Isrc/loader'])]),
    Program('with_register.dll', [Objects(['test/registry/fixture_register.c'], ['-O2'], WINDOWS)], 'dll'),
    Program('without_register.dll', [Objects(['test/registry/fixture_plain.c'], ['-O2'], WINDOWS)], 'dll'),
]

CONTENT_FLAGS = ['-Isrc', '-std=c++23', '-O1', '-Wall', '-Wextra', '-Wno-nullability-completeness']
CONTENT_ENGINE = Objects(ENGINE, CONTENT_FLAGS + [SDK])
CONTENT_IDS = Objects(['src/core/apply/ids.cpp'], CONTENT_FLAGS + [SDK])
CONTENT_WINDOWS = ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra', SDK, '-Iinclude', '-Ithird_party/zstd']
CONTENT = [
    Program('lower_tool', [Objects(['test/content/lower_tool.cpp'], CONTENT_FLAGS + [SDK]), CONTENT_ENGINE]),
    Program('rows_tool', [Objects(['test/content/rows_tool.cpp', 'src/core/apply/scheduler.cpp'], CONTENT_FLAGS + [SDK]), CONTENT_IDS,
                          CONTENT_ENGINE]),
    Program('rows_test', [Objects(['test/content/rows_test.cpp', 'test/content/rows_test_packages.cpp',
                                   'test/content/rows_test_teams.cpp', 'test/content/rows_test_slots.cpp',
                                   'test/content/rows_test_career.cpp', 'test/content/rows_test_plan.cpp',
                                   'test/content/rows_test_career_rules.cpp', 'src/core/apply/scheduler.cpp'],
                                  CONTENT_FLAGS + [SDK]),
                          CONTENT_IDS, CONTENT_ENGINE]),
    *[Program(test, [Objects([f'test/content/{test}.cpp'], CONTENT_FLAGS + [SDK]), CONTENT_IDS, CONTENT_ENGINE])
      for test in ('vehicle_tower_test', 'package_id_test', 'id_pool_test', 'wear_test', 'box_world_test')],
    Program('appliers.windows.o', [Objects(['src/core/apply/content/appliers.cpp'], CONTENT_WINDOWS, WINDOWS)], 'obj'),
    Program('logos.windows.o', [Objects(['src/core/apply/content/logos.cpp'], CONTENT_WINDOWS, WINDOWS)], 'obj'),
    Program('bc7_test', [Objects(['test/content/bc7_test.cpp'], CONTENT_FLAGS + ['-ffp-contract=off'])]),
    Program('gvdb_file_test', [Objects(['test/content/gvdb_file_test.cpp'], CONTENT_FLAGS)]),
    Program('logo_cache_test', [Objects(['test/content/logo_cache_test.cpp'], CONTENT_FLAGS)]),
    Program('logo_build_test', [Objects(['test/content/logo_build_test.cpp'], CONTENT_FLAGS + ['-ffp-contract=off', '-Ithird_party/zstd']), ZSTD]),
    Program('garage_test', [Objects(['test/content/garage_test.cpp'], CONTENT_FLAGS + ['-ffp-contract=off', '-Ithird_party/zstd']), ZSTD]),
    Program('logo_deliver_test', [Objects(['test/content/logo_deliver_test.cpp'],
                                          CONTENT_FLAGS + ['-ffp-contract=off', '-Ithird_party/zstd', SDK]), ZSTD]),
    Program('bc7_test.windows.o', [Objects(['test/content/bc7_test.cpp'], ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra',
                                                                          '-Wno-nullability-completeness', '-ffp-contract=off'], WINDOWS)], 'obj'),
]

SDK_HEADERS_FLAGS = ['-std=c++23', '-O1', '-Wall', '-Wextra', '-Werror', '-Wno-nullability-completeness',
                     '-Wno-missing-designated-field-initializers', SDK, '-Itest/sdk_stub']
SDK_C_FLAGS = ['-std=c11', '-Wall', '-Wextra', '-Werror', '-pedantic', SDK]
HOOK_FLAGS = ['-Isrc', '-std=c++23', '-O1', '-Wall', '-Wextra', '-Wno-nullability-completeness', SDK]


def hook_test(name):
    return Program(f'{name}_test', [Objects([f'test/hooks/{name}_test.cpp'], HOOK_FLAGS + ['-Ithird_party/safetyhook'])] + SAFETYHOOK)


CAREER_OPS_FLAGS = ['-Isrc', '-std=c++23', '-O1', '-Wall', '-Wextra', '-Werror', '-Wno-nullability-completeness']
SDK_OPS_FLAGS = ['-std=c++23', '-O1', '-Wall', '-Wextra', '-Werror', '-Wno-nullability-completeness',
                 '-Wno-missing-designated-field-initializers', SDK]
CAREER_OPS = [Program(t, [Objects([f'test/career_ops/{t}.cpp'], CAREER_OPS_FLAGS)])
              for t in ('lineup_plan_test', 'teams_plan_test', 'ops_logic_test')] + [
    Program(f'{t}{ext}', [Objects([f'test/career_ops/{t}.cpp'], SDK_OPS_FLAGS, target)])
    for t in ('sdk_ops_test', 'sdk_ops_v4_test', 'sdk_ops_v6_test', 'sdk_ops_v7_test', 'sdk_ops_v8_test')
    for ext, target in (('', LINUX), ('.exe', WINDOWS))]

SANITIZE = ['-fsanitize=address,undefined', '-fno-sanitize-recover=all']
SCHEMA_FLAGS = ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra', '-Werror', '-Wno-nullability-completeness', SDK]
SCHEMA_SAN_FLAGS = ['-Isrc', '-std=c++23', '-O1', '-g', '-Wall', '-Wextra'] + SANITIZE + [SDK]
SCHEMA = [
    Program('schema.obj', [Objects(['src/core/registry/schema.cpp'], SCHEMA_FLAGS, WINDOWS)], 'obj'),
    *[Program(t, [Objects([f'test/schema/{t}.cpp', 'src/core/registry/schema.cpp'], SCHEMA_FLAGS)])
      for t in ('schema_test', 'schema_agree')],
    Program('schema_test_san', [Objects(['test/schema/schema_test.cpp', 'src/core/registry/schema.cpp'], SCHEMA_SAN_FLAGS, HOST)],
            ldflags=SANITIZE, when='sanitizers'),
]

JSON_FLAGS = ['-std=c++23', '-O2', '-Wall', '-Wextra', '-Werror', '-Wno-nullability-completeness', SDK]
JSON = [
    Program('json_test.obj', [Objects(['test/json/json_test.cpp'], JSON_FLAGS, WINDOWS)], 'obj'),
    Program('json_test', [Objects(['test/json/json_test.cpp'], JSON_FLAGS)]),
    Program('json_test_san', [Objects(['test/json/json_test.cpp'], ['-std=c++23', '-O1', '-g', '-Wall', '-Wextra'] + SANITIZE + [SDK], HOST)],
            ldflags=SANITIZE, when='sanitizers'),
]

TRACK_LISTS_FLAGS = ['-Isrc', '-std=c++23', '-O1', '-Wall', '-Wextra', '-Wno-nullability-completeness', SDK]
TRACK_LISTS = [Program(f'{t}_test', [Objects([f'test/tracks/{t}_test.cpp'], TRACK_LISTS_FLAGS)])
               for t in ('fits', 'list', 'per_package')] + [
    Program(f'{f}.windows.o', [Objects([f'src/core/apply/tracks/{f}.cpp'], TRACK_LISTS_FLAGS + ['-Iinclude'], WINDOWS)], 'obj')
    for f in ('list', 'per_package', 'data_session')]

APPLY_FLAGS = CONTENT_FLAGS + [SDK]
APPLY = [
    Program('apply_test', [Objects(['test/apply/apply_test.cpp', 'src/core/apply/scheduler.cpp'], APPLY_FLAGS), CONTENT_IDS,
                           CONTENT_ENGINE]),
    Program('files_test', [Objects(['test/apply/files_test.cpp'], APPLY_FLAGS)]),
    *[Program(f'{f}.windows.o', [Objects([f'src/core/apply/{f}.cpp'], APPLY_FLAGS, WINDOWS)], 'obj')
      for f in ('scheduler', 'ids', 'files')],
]

TRACKS = [
    Program('lower_test', [Objects(['test/tracks/lower_test.cpp'], APPLY_FLAGS), CONTENT_ENGINE, CONTENT_IDS]),
    Program('boot.windows.o', [Objects(['src/core/apply/tracks/boot.cpp'], ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra', SDK,
                                                                           '-Iinclude', '-Wno-nullability-completeness'], WINDOWS)], 'obj'),
]

SNAPSHOTS_FLAGS = ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra', '-Wno-nullability-completeness', SDK, '-Ithird_party/zstd']
SNAPSHOTS = [Program('snapshots_test', [
    Objects(['test/snapshots/snapshots_test.cpp', 'src/core/career/snapshots/snapshots.cpp',
             'src/core/career/snapshots/freeze.cpp', 'src/core/registry/store.cpp', 'src/core/registry/validate.cpp',
             'src/core/registry/schema.cpp'], SNAPSHOTS_FLAGS), ZSTD])]

CAR_LIVE = [Program(t, [Objects([f'test/hooks/{t}.cpp'], ['-Isrc', '-Iinclude', '-std=c++23', '-O1', '-Wall', '-Wextra', '-Werror',
                                                          '-Wno-nullability-completeness', SDK])])
            for t in ('car_live_test', 'car_profile_test', 'car_identity_test', 'ai_racecraft_test')]

FAST_FAIL = [Program('fast_fail_test', [Objects(['test/fast_fail_test.cpp'], ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra',
                                                                              '-Wno-nullability-completeness'])])]

SCAN = [Program('scan_test', [Objects(['test/scan_test.cpp'], ['-Isrc', '-std=c++23', '-O2', '-Wall', '-Wextra',
                                                               '-Wno-nullability-completeness'])])]


def each(*programs):
    return ['lib/each.sh'] + [f'{{out}}/{p}' for p in programs]


WINE_CASES = re.search(r'^CASES=\((.*)\)$', open(os.path.join(os.path.dirname(__file__), 'run_wine_test.sh')).read(),
                       re.M).group(1).split()

SCRIPTS = [
    Script('wine_setup', 'Wine test: setup', ['bash', 'native/test/run_wine_test.sh', '--setup'], wine=True, group='wine'),
    *[Script(f'wine_{case}', f'Wine test: {case}', ['bash', 'native/test/run_wine_test.sh', '--case', case], wine=True,
             after=['wine_setup'], group='wine') for case in WINE_CASES],
    Script('registry', 'Registry core (shared corpus, Python reference, random stacks, read benchmark)',
           ['bash', 'native/test/run_registry_test.sh', '{out}'], programs=REGISTRY),
    Script('sdk_headers', 'Standard library 1.6 headers compile test', ['{out}/sdk_headers_test'], cwd='test', programs=[
        Program('sdk_headers_test.exe', [Objects(['test/sdk_headers_test.cpp'], SDK_HEADERS_FLAGS, WINDOWS)]),
        Program('sdk_headers_test', [Objects(['test/sdk_headers_test.cpp'], SDK_HEADERS_FLAGS)])]),
    Script('sdk_api_c', 'Standard library C API compile test', ['{out}/sdk_api_c_test'], cwd='test', programs=[
        Program('sdk_api_c_test.exe', [Objects(['test/sdk_api_c_test.c'], SDK_C_FLAGS, WINDOWS)]),
        Program('sdk_api_c_test', [Objects(['test/sdk_api_c_test.c'], SDK_C_FLAGS)])]),
    Script('schema', 'Schema check unit and agreement test', ['bash', 'native/test/run_schema_test.sh', '{out}'], programs=SCHEMA),
    Script('testagent_wine', 'Test agent Wine test', ['bash', 'native/test/run_testagent_test.sh'], wine=True),
    Script('content', 'Content registries (row engine, Classic Seasons lowering, variants, stat-id pools)',
           ['bash', 'native/test/run_content_test.sh', '{out}'], programs=CONTENT),
    Script('bind', 'Binding resolver Wine test', ['bash', 'native/test/run_bind_test.sh'], wine=True),
    Script('career_ops', 'Career operations planner and SDK calls host test', each(
        'lineup_plan_test', 'teams_plan_test', 'sdk_ops_test', 'ops_logic_test', 'sdk_ops_v4_test', 'sdk_ops_v6_test',
        'sdk_ops_v7_test', 'sdk_ops_v8_test'), cwd='test', programs=CAREER_OPS),
    Script('json', 'swofty::json unit test', ['bash', 'native/test/run_json_test.sh', '{out}'], programs=JSON),
    Script('build_gate', 'Loader build gate unit test', ['bash', 'native/test/run_build_gate_test.sh']),
    Script('apply', 'Live apply paths (scheduler, put-back, context, budgets, sizing, stat ids, reports)',
           ['bash', 'native/test/run_apply_test.sh', '{out}'], programs=APPLY),
    Script('tracks', "Track registries (seeds, record lowering, parity with the launcher's track activation)",
           ['bash', 'native/test/run_tracks_test.sh', '{out}'], programs=TRACKS),
    Script('save_sandbox', 'Save sandbox host and Wine test', ['bash', 'native/test/run_save_sandbox_test.sh'], wine=True),
    Script('runtime_assets', 'Runtime assets Wine test', ['bash', 'native/test/run_runtime_assets_test.sh'], wine=True),
    Script('track_lists', 'Box track lists (fit rules, calendar and More tracks, per-box entries, fuel plan, guard decision)',
           each('fits_test', 'list_test', 'per_package_test'), cwd='test', programs=TRACK_LISTS),
    Script('snapshots', 'Career snapshots (swofty_registry entry, freezes, budgets, migration, fixtures)',
           ['bash', 'native/test/run_snapshots_test.sh', '{out}'], programs=SNAPSHOTS),
    Script('net_trace', 'Socket trace Wine test', ['bash', 'native/test/run_net_trace_test.sh'], wine=True),
    Script('career_ui', 'Career UI Wine test', ['bash', 'native/test/run_career_ui_test.sh'], wine=True),
    Script('fast_fail', 'Fast-fail crash reports (fast-fail codes, reasons, exit statuses, byte patterns, cached prologue addresses)', ['{out}/fast_fail_test'],
           cwd='test', programs=FAST_FAIL),
    Script('log_fallback', 'Loader log fallback Wine test', ['bash', 'native/test/run_log_fallback_test.sh'], wine=True),
    Script('session_env_wine', 'Loader session environment Wine test', ['bash', 'native/test/run_session_env_test.sh'],
           wine=True),
    Script('lang_override', 'String override Wine test', ['bash', 'native/test/run_lang_override_test.sh'], wine=True),
    Script('toolkit', 'Overlay toolkit Wine test', ['bash', 'native/test/run_toolkit_test.sh'], wine=True),
    Script('hud', 'Overlay draw-list unit test', ['bash', 'native/test/run_hud_test.sh']),
    Script('hud_slots', 'HUD slot reader and placement host test', ['bash', 'native/test/run_hud_slots_test.sh']),
    Script('hud_dial_template', 'HUD dial template host test', ['bash', 'templates/hud-dial/native/test/run_test.sh']),
    Script('guard', 'Handler guard Wine test', ['bash', 'native/test/run_guard_test.sh'], wine=True),
    Script('pdh', 'GPU Engine counter calls Wine test (pinned pdh.dll, faults kept)', ['bash', 'native/test/run_pdh_test.sh'], wine=True),
    Script('mem_pages', 'Memory page checks Wine test', ['bash', 'native/test/run_mem_pages_test.sh'], wine=True),
    Script('pe_imports_wine', 'Import table walk Wine test (rewritten entries skipped, valid slots patched)',
           ['bash', 'native/test/run_pe_imports_test.sh'], wine=True),
    Script('settings_get', 'Settings read memo Wine test', ['bash', 'native/test/run_settings_get_test.sh'], wine=True),
    Script('career_list', 'Career list Wine test', ['bash', 'native/test/run_career_list_test.sh'], wine=True),
    Script('hooks', 'Hook points, events and services host test', ['{out}/hooks_test'], cwd='test', programs=[hook_test('hooks')]),
    Script('game_points', 'Game hook points host test', ['{out}/game_points_test'], cwd='test', programs=[hook_test('game_points')]),
    Script('session_points', 'Session control, AI controller and spectator points host test', ['{out}/session_points_test'],
           cwd='test', programs=[hook_test('session_points')]),
    Script('car_live', 'Live car operations host test', each('car_live_test', 'car_profile_test', 'car_identity_test', 'ai_racecraft_test'), cwd='test',
           programs=CAR_LIVE),
    Script('w5_review', 'Hook points review test', ['{out}/w5_review_test'], cwd='test',
           programs=[Program('w5_review_test', [Objects(['test/hooks/w5_review_test.cpp'], HOOK_FLAGS)])]),
    Script('live', 'Live state memory reads, derivations and telemetry fill', ['bash', 'native/test/run_live_test.sh']),
    Script('tyre_images', 'Live tyre image catalogues, mod routes and archive worker',
           ['bash', 'native/test/run_tyre_images_test.sh'], wine=True),
    Script('game_assets', 'Game assets library (archive index, overlay archives, texture decode, UI catalogues, real game files)',
           ['{out}/game_assets_test'], cwd='test',
           programs=[Program('game_assets_test', [Objects(['test/game_assets_test.cpp'], CONTENT_FLAGS + [SDK, '-Ithird_party/zstd']),
                                                  ZSTD])]),
    Script('core_sites_exported', 'Core site names exported (host-only)', ['bash', 'native/test/run_core_sites_exported_test.sh']),
    Script('garage_placement', 'Garage placement and crew poses', ['bash', 'native/test/run_garage_placement_test.sh']),
    Script('garage_crew_sites', 'Garage crew guard sites export test', ['bash', 'native/test/run_garage_crew_sites_test.sh']),
    Script('scan', 'Pattern scan and scan cache unit test', ['bash', 'native/test/run_scan_test.sh', '{out}'], programs=SCAN),
    Script('pin', 'CPU pin plan (CI, taskset, affinity, job count)', ['bash', 'native/test/pin_test.sh']),
    Script('wineserver', 'Wine test wineserver lookup (beside wine, lib folders, versions)',
           ['bash', 'native/test/wineserver_test.sh']),
    Script('std_templates', 'Standard library templates', ['python3', 'tools/std/templates.py']),
    Script('loading_report', 'Loading report on synthetic loader.log runs', ['python3', '-I', 'native/test/loading_report_test.py']),
    Script('sigverify', 'Core signatures on the KB build and Steam 1.0.145 (skipped without both executables)',
           ['python3', '-I', 'native/test/sigverify_test.py']),
    Script('init_check', 'Static initializer check: run order, inline variables and hook guard arming',
           ['python3', '-I', 'native/test/init_check_test.py']),
    Script('ninja_deps', 'Ninja deps log check (damage from two writers, removal, incremental again)',
           ['python3', '-I', 'native/test/ninja_deps_test.py']),
    Script('patch_sites', 'Every line that writes game code is in tools/patch_sites.txt',
           ['python3', '-I', 'native/tools/patch_sites.py', '--check']),
    Script('patch_sites_tool', 'Patch site list and patches.json on a fixture tree and a synthetic exe',
           ['python3', '-I', 'native/test/patch_sites_test.py']),
]

