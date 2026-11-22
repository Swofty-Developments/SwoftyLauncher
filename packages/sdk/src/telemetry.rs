use std::ffi::{c_char, c_int};

pub const TELEMETRY_VERSION: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyCarState {
    pub version: u32,
    pub flags: u32,
    pub speed: f32,
    pub rpm: f32,
    pub max_rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steer: f32,
    pub tyre_temp: [f32; 4],
    pub tyre_wear: [f32; 4],
    pub brake_temp: [f32; 4],
    pub fuel: f32,
    pub fuel_laps: f32,
    pub position: i32,
    pub lap: u32,
    pub sector: u32,
    pub last_lap_time: f32,
    pub current_lap_time: f32,
    pub best_lap_time: f32,
    pub sector_time: [f32; 3],
    pub lap_distance: f32,
    pub world_pos: [f32; 3],
    pub world_vel: [f32; 3],
    pub tyre_compound: u8,
    pub tyre_visual: u8,
    pub tyre_age_laps: u8,
    pub pit_status: u8,
    pub result_status: u8,
    pub driver_status: u8,
    pub pit_stops: u8,
    pub warnings: u8,
    pub penalty_seconds: u8,
    pub drs: u8,
    pub grid_position: u8,
    pub ai_controlled: u8,
    pub drive_through_pens: u8,
    pub stop_go_pens: u8,
    pub corner_cut_warnings: u8,
    pub fia_flag: i8,
    pub gap_front_ms: u32,
    pub gap_leader_ms: u32,
    pub ers_store: f32,
    pub reserved: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftySessionState {
    pub version: u32,
    pub session_type: u32,
    pub track: u32,
    pub flag: u32,
    pub weather: u32,
    pub total_laps: u32,
    pub participant_count: u32,
    pub player_index: u32,
    pub session_time: f32,
    pub session_time_left: f32,
    pub track_temp: f32,
    pub air_temp: f32,
    pub safety_car_status: u32,
    pub track_length: u32,
    pub udp_format: u32,
    pub packets_seen: u32,
    pub pit_speed_limit: u32,
    pub red_flag: u32,
    pub reserved: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyParticipant {
    pub version: u32,
    pub flags: u32,
    pub name: [c_char; 48],
    pub team: u32,
    pub position: i32,
    pub lap: u32,
    pub lap_distance: f32,
    pub best_lap_time: f32,
    pub driver_id: u32,
    pub race_number: u8,
    pub ai_controlled: u8,
    pub nationality: u8,
    pub my_team: u8,
    pub reserved: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyTelemetryFeed {
    pub version: u32,
    pub size: u32,
    pub seq: u64,
    pub frame: u64,
    pub session: SwoftySessionState,
    pub cars: [SwoftyCarState; 24],
    pub participants: [SwoftyParticipant; 24],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SwoftyTelemetry {
    pub available: Option<unsafe extern "C" fn() -> c_int>,
    pub session: Option<unsafe extern "C" fn(out: *mut SwoftySessionState) -> c_int>,
    pub car: Option<unsafe extern "C" fn(index: u32, out: *mut SwoftyCarState) -> c_int>,
    pub participant: Option<unsafe extern "C" fn(index: u32, out: *mut SwoftyParticipant) -> c_int>,
    pub shared_memory: Option<unsafe extern "C" fn(name: *mut *const c_char) -> c_int>,
}

pub const EVENT_TELEMETRY_OVERTAKE: &str = "telemetry.overtake";
pub const EVENT_TELEMETRY_COLLISION: &str = "telemetry.collision";
pub const EVENT_TELEMETRY_RETIREMENT: &str = "telemetry.retirement";
