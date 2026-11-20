use std::mem::{align_of, offset_of, size_of};

use swofty_sdk::*;

macro_rules! layout {
    ($t:ident, $size:literal, $align:literal, [$($field:ident: $offset:literal),* $(,)?]) => {
        assert_eq!(size_of::<$t>(), $size, "size of {}", stringify!($t));
        assert_eq!(align_of::<$t>(), $align, "alignment of {}", stringify!($t));
        $(assert_eq!(offset_of!($t, $field), $offset, "offset of {}.{}", stringify!($t), stringify!($field));)*
    };
}

#[test]
#[cfg(target_pointer_width = "64")]
fn every_c_type_keeps_its_size_alignment_and_field_offsets() {
    layout!(SwoftyMixin, 32, 8, [target: 0, kind: 8, priority: 12, handler: 16, orig: 24]);
    layout!(SwoftySettingValue, 24, 8, [kind: 0, toggle: 4, slider: 8, choice: 16]);
    layout!(SwoftyFragment, 24, 8, [file: 0, fourcc: 8, flags: 16]);
    layout!(SwoftyAssets, 64, 8, [available: 0, register_group: 8, put_resource: 16, db_set: 24, db_set_raw: 32, lang_set: 40, lang_table: 48, overlay_file: 56]);
    layout!(SwoftyEvent, 48, 8, [name: 0, frame: 8, time: 16, a: 24, b: 32, data: 40]);
    layout!(SwoftyEvents, 32, 8, [available: 0, subscribe: 8, on_frame: 16, unsubscribe: 24]);
    layout!(SwoftyPaint, 0, 1, []);
    layout!(SwoftyOverlay, 88, 8, [available: 0, on_paint: 8, remove: 16, size: 24, rect: 32, rect_outline: 40, line: 48, text: 56, measure: 64, load_image: 72, image: 80]);
    layout!(SwoftyCarState, 192, 4, [version: 0, flags: 4, speed: 8, rpm: 12, max_rpm: 16, gear: 20, throttle: 24, brake: 28, clutch: 32, steer: 36, tyre_temp: 40, tyre_wear: 56, brake_temp: 72, fuel: 88, fuel_laps: 92, position: 96, lap: 100, sector: 104, last_lap_time: 108, current_lap_time: 112, best_lap_time: 116, sector_time: 120, lap_distance: 132, world_pos: 136, world_vel: 148, tyre_compound: 160, tyre_visual: 161, tyre_age_laps: 162, pit_status: 163, result_status: 164, driver_status: 165, pit_stops: 166, warnings: 167, penalty_seconds: 168, drs: 169, grid_position: 170, ai_controlled: 171, drive_through_pens: 172, stop_go_pens: 173, corner_cut_warnings: 174, fia_flag: 175, gap_front_ms: 176, gap_leader_ms: 180, ers_store: 184, reserved: 188]);
    layout!(SwoftySessionState, 80, 4, [version: 0, session_type: 4, track: 8, flag: 12, weather: 16, total_laps: 20, participant_count: 24, player_index: 28, session_time: 32, session_time_left: 36, track_temp: 40, air_temp: 44, safety_car_status: 48, track_length: 52, udp_format: 56, packets_seen: 60, pit_speed_limit: 64, red_flag: 68, reserved: 72]);
    layout!(SwoftyParticipant, 92, 4, [version: 0, flags: 4, name: 8, team: 56, position: 60, lap: 64, lap_distance: 68, best_lap_time: 72, driver_id: 76, race_number: 80, ai_controlled: 81, nationality: 82, my_team: 83, reserved: 84]);
    layout!(SwoftyTelemetryFeed, 6920, 8, [version: 0, size: 4, seq: 8, frame: 16, session: 24, cars: 104, participants: 4712]);
    layout!(SwoftyTelemetry, 40, 8, [available: 0, session: 8, car: 16, participant: 24, shared_memory: 32]);
    layout!(SwoftyDbValue, 32, 8, [tag: 0, i: 8, f: 16, s: 24]);
    layout!(SwoftyDbCursor, 32, 8, [table: 0, index: 8, count: 16, row: 24]);
    layout!(SwoftyDb, 96, 8, [available: 0, get: 8, get_hash: 16, int_at: 24, float_at: 32, string_at: 40, bool_at: 48, open: 56, row: 64, step: 72, cell: 80, row_name: 88]);
    layout!(SwoftyText, 16, 8, [data: 0, size: 8]);
    layout!(SwoftyCareerSaves, 80, 8, [version: 0, available: 8, index: 16, current: 24, describe: 32, need: 40, data_get: 48, data_set: 56, data_remove: 64, missing: 72]);
    layout!(SwoftyStorage, 56, 8, [available: 0, read: 8, write: 16, append: 24, remove: 32, size: 40, list: 48]);
    layout!(SwoftyFlowState, 88, 8, [hash: 0, from: 4, to: 8, named: 12, transitions: 16, name: 24]);
    layout!(SwoftyCareerState, 80, 4, [loaded: 0, mode: 4, hub: 8, humans: 12, player_driver: 16, player_team: 20, has_player: 24, components: 28, component_mode: 32, team_hq_mode: 36, flow_career: 40, reserved: 44, key: 48]);
    layout!(SwoftyCareer, 80, 8, [available: 0, state: 8, flow: 16, flow_status: 24, entity: 32, component: 40, component_by_desc: 48, post: 56, block: 64, unblock: 72]);
    layout!(SwoftyCareerStart, 80, 4, [mode: 0, kind: 4, blocked: 8, reserved: 12, route: 16]);
    layout!(SwoftyPopup, 16, 8, [available: 0, show: 8]);
    layout!(SwoftyLang, 40, 8, [available: 0, override_set: 8, override_clear: 16, override_clear_all: 24, refresh: 32]);
    layout!(SwoftyCareerContentDesc, 80, 8, [size: 0, model: 4, id: 8, title: 16, description: 24, data_group: 32, suffix: 40, mode: 48, entitlement: 56, car_group: 64, badge: 68, careers: 72]);
    layout!(SwoftyCareerContentInfo, 352, 8, [index: 0, state: 4, mode_stat_id: 8, data_group_index: 16, tiles: 24, presses: 28, id: 32, data_group: 96, why: 160]);
    layout!(SwoftyCareerContent, 40, 8, [available: 0, add: 8, info: 16, data_group: 24, pressed: 32]);
    layout!(SwoftyCharacters, 64, 8, [available: 0, career_group: 8, swap: 16, mute: 24, string_set: 32, strings_json: 40, clear: 48, refresh: 56]);
    layout!(SwoftyCareerWizard, 72, 8, [available: 0, group: 8, starts_in_f1: 16, tile_route: 24, replace_step: 32, force_option: 40, confirm_route: 48, hide_driver_tab: 56, clear: 64]);
    layout!(SwoftyEraBox, 56, 8, [size: 0, entrant_list: 8, title: 16, image: 24, order: 32, time_trial_entrant_list: 40, only: 48]);
    layout!(SwoftyHud, 32, 8, [version: 0, available: 8, toast: 16, set: 24]);
    layout!(SwoftyEra, 24, 8, [available: 0, add_box: 8, count: 16]);
    layout!(SwoftyApi, 200, 8, [version: 0, log: 8, module_base: 16, scan: 24, resolve: 32, register_mixins: 40, fields: 48, setting: 56, watch_setting: 64, assets: 72, events: 80, overlay: 88, telemetry: 96, db: 104, storage: 112, career: 120, popup: 128, lang: 136, career_content: 144, characters: 152, career_wizard: 160, era: 168, registry: 176, hooks: 184, services: 192]);
}
