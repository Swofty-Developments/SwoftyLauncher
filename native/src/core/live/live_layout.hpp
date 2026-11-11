#pragma once
#include <cstddef>
#include <cstdint>

#include <swofty/game/car.hpp>

// Layouts of the game's objects the live state reads, build of the KB (F1_25.exe, image base 0x140000000). Every
// value is range-checked where it is read, and a failed read leaves the value's have bit clear. The CarLive fields
// that the generated game headers declare are pinned by static_asserts below; the rest are KB rows that carry their
// own evidence in re/db/structs and re/db/verify.tsv, and the reader takes them only when the group they belong to
// is ready (live_rows.hpp).

namespace swofty_core::live::layout {

namespace car {
constexpr uintptr_t lock = 0x98, lock_cs = 0xa0, systems_vt = 0x380;
constexpr uintptr_t vehicle_id = 0xe8, is_ai = 0x124;
constexpr uintptr_t throttle = 0x188, brake = 0x190, clutch = 0x18c, input_flags = 0x224;
constexpr uintptr_t engine = 0x530, gearbox = 0x538, chassis = 0x558, steering = 0x560, f1_aero = 0x570, ers = 0x5a8;
constexpr uintptr_t lo = 0xe8, hi = 0x5b0;
static_assert(lock + 8 <= hi && systems_vt + 8 <= hi && is_ai + 4 <= hi);
static_assert(offsetof(swofty::game::CarLive, vehicle_id) == vehicle_id);
static_assert(offsetof(swofty::game::CarLive, engine) == engine);
static_assert(offsetof(swofty::game::CarLive, gearbox) == gearbox);
static_assert(offsetof(swofty::game::CarLive, chassis) == chassis);
static_assert(offsetof(swofty::game::CarLive, steering) == steering);
static_assert(offsetof(swofty::game::CarLive, f1_aero) == f1_aero);
static_assert(offsetof(swofty::game::CarLive, ers) == ers);
}

constexpr uintptr_t system_car = 0x8;

namespace engine {
constexpr uintptr_t limiter = 0x2528, rate = 0x2538;
static_assert(offsetof(swofty::game::CarEngine, limiter_rotation_rate) == limiter);
static_assert(offsetof(swofty::game::CarEngine, rotation_rate) == rate);
}

namespace gearbox {
constexpr uintptr_t gear_index = 0x680;
}

namespace chassis {
constexpr uintptr_t g_lat = 0x320, g_long = 0x324, body = 0x340, body_state = 0x18, state_velocity = 0x220,
                    state_position = 0x1c0;
}

namespace steering {
constexpr uintptr_t lock = 0x28, angle = 0xe8;
}

namespace aero {
constexpr uintptr_t drs_request = 0x1988;
}

namespace ers {
constexpr uintptr_t max_deploy = 0x28, capacity = 0x60;
constexpr uintptr_t live_lo = 0x19c8, live_hi = 0x1a6c;
constexpr uintptr_t harvest_lap = 0x19c8, deploy_lap = 0x19d0, energy = 0x19f0, lap_limit = 0x19f4;
constexpr uintptr_t harvest_k = 0x1a00, harvest_h = 0x1a04, deploy = 0x1a14, mode = 0x1a68;
}

namespace progress {
constexpr uintptr_t from_manager = 0x2d0, vec_begin = 0x38, vec_end = 0x40;
constexpr uintptr_t lo = 0x28, hi = 0xa8;
constexpr uintptr_t lap_a = 0x38, lap_b = 0x40, plap_a = 0x48, plap_b = 0x50, plap_delta = 0x58;
constexpr uintptr_t total_a = 0x98, total_b = 0xa0;
}

namespace registry {
constexpr uintptr_t from_manager = 0x30, slots = 0x18, info = 0x20;
constexpr size_t max_slots = 4096;
}

namespace results {
constexpr uintptr_t vec_begin = 0x148, vec_end = 0x150;
constexpr uintptr_t vehicle = 0x0, status = 0x18, penalty_a = 0x38, penalty_b = 0x40, pos_a = 0x48, pos_b = 0x50;
constexpr uintptr_t hi = 0x58;
}

namespace timing {
constexpr uintptr_t vec_begin = 0x30, stride = 0x98;
constexpr uintptr_t leader_ms = 0x58, leader_valid = 0x68, ahead_ms = 0x70, ahead_valid = 0x80, vehicle = 0x90;
}

namespace pit {
constexpr uintptr_t records = 0x68, state = 0x4;
}

namespace penalty_log {
constexpr uintptr_t vec_begin = 0x48, stride = 0x30;
constexpr uintptr_t vehicle = 0x08, kind = 0x09, infringement = 0x0a;
constexpr uint8_t warning = 5;
constexpr uint32_t track_limits_mask = 0x3e000380;
}

namespace misconduct {
constexpr uintptr_t drive_through = 0x568, stop_go = 0x578;
constexpr uintptr_t vec_begin = 0x68;
constexpr size_t dt_stride = 0x28, sg_stride = 4;
}

namespace safety_car {
constexpr uintptr_t state = 0x108;
}

namespace marshal {
constexpr uintptr_t vec_begin = 0x88, stride = 0x10, zone = 0x8;
constexpr uintptr_t zone_flag = 0x14, zone_def = 0x0, def_start = 0x10;
constexpr size_t max_zones = 64;
}

namespace red_flag {
constexpr uintptr_t state = 0x40;
}

namespace tyres {
constexpr uintptr_t per_vehicle = 0x110;
constexpr uintptr_t lo = 0x9bc0, hi = 0x9ca0;
constexpr uintptr_t version = 0x9bc0, list = 0x9bd0, count = 0x9c48, built_for = 0x9c50, built = 0x9c58, set_begin = 0x9c90,
                    set_end = 0x9c98;
constexpr size_t desc_stride = 0x18, set_stride = 0x78;
constexpr uintptr_t visual_compound = 0x50;
}

namespace controllers {
constexpr uintptr_t sets = 0x30, count_vec = 0x90, count_lo = 0x28, count_hi = 0x30, built = 0x365;
constexpr uintptr_t set_human = 0x20, set_vehicle = 0x34;
}

namespace spectate {
constexpr uintptr_t lo = 0xd0, hi = 0xe8, ui = 0xd0, active = 0xd1, vehicle = 0xd6, gap = 0xe4;
}

namespace mode {
constexpr uintptr_t from_context = 0x88, label = 0x278;
}

namespace badge {
constexpr uintptr_t mode = 0x88, game = 0x260, entrants = 0x2e0;
constexpr size_t entrant_stride = 0x88;
constexpr uintptr_t entrant_team = 0x58, entrant_vid = 0x82, tables = 0x2c8, table_vec = 0x8, record_key = 0x4d8,
                    record_render = 0x58d;
constexpr unsigned max_kind = 2, my_team_kind = 1;
}

namespace track {
constexpr uintptr_t world_to_a = 0xd0, a_to_b = 0xe8, b_to_name = 0xa0;
constexpr uintptr_t sector_end = 0x0, sector_length = 0x58, sector_count = 0x5c;
constexpr size_t sector_block = 0x60;
}

}
