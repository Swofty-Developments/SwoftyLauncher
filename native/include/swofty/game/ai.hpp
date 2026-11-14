#pragma once
#include <swofty/game/base.hpp>

namespace swofty::game {

struct AiDriverConfig {
    uint8_t _pad_0x0[0x20];
    void **alongside_speed_difference_for_limited_zone_yielding; // L2 experimental
    void **alongside_yield_overtake_rate; // L2 experimental
    void **alongside_min_presence_within_buffer; // L2 experimental
    uint8_t _pad_0x38[0x8];
    void **blocking_start_blocking_intersection_time; // L2 experimental
    void **blocking_start_blocking_maximum_time_to_corner; // L2 experimental
    void **blocking_start_blocking_minimum_time_to_corner; // L2 experimental
    void **blocking_time_to_corner_for_end_merge_rate; // L2 experimental
    void **blocking_time_to_corner_for_start_merge_rate; // L2 experimental
    void **blocking_end_blocking_time_to_corner_vs_offset; // L2 experimental
    void **blocking_blocking_offset_from_car_behind; // L2 experimental
    void **blocking_blocking_speed_differential; // L2 experimental
    void **blocking_blocking_threat_time_modifier; // L2 experimental
    void **blocking_blocking_variable_offset_fraction; // L2 experimental
    void **blocking_fraction_of_follow_speed; // L2 experimental
    void **blocking_fraction_of_follow_speed_at_race_start; // L2 experimental
    void **blocking_max_blocking_distance; // L2 experimental
    void **blocking_max_blocking_distance_at_race_start; // L2 experimental
    void **blocking_avoidance_decrease_rate; // L2 experimental
    void **blocking_merge_back_to_racing_line; // L2 experimental
    uint8_t _pad_0xc0[0x30];
    void **divebomb_defense_max_presence_alongside; // L2 experimental
    void **divebomb_defense_max_time_to_ignore; // L2 experimental
    void **divebomb_defense_max_track_offset; // L2 experimental
    void **divebomb_defense_max_track_radius; // L2 experimental
    uint8_t _pad_0x110[0x10];
    void **offset_variation_max_offset; // L2 experimental
    void **offset_variation_offset_limit_at_corner_start; // L2 experimental
    void **offset_variation_offset_limit_blend_distance; // L2 experimental
    uint8_t _pad_0x138[0x10];
    void **offset_variation_half_width_scaling_graph; // L2 experimental
    void **offset_variation_radius_scalar_graph; // L2 experimental
    void **overtake_attacker_close_enough_car_lengths; // L2 experimental
    void **overtake_braking_zone_abort_overtake_time; // L2 experimental
    void **overtake_close_range_car_lengths_to_continue_overtake; // L2 experimental
    void **overtake_close_range_car_lengths_to_start_overtake; // L2 experimental
    void **overtake_oppertunist_overtake_time; // L2 experimental
    void **overtake_oppertunist_overtake_track_speed_difference; // L2 experimental
    void **overtake_danger_overtake_continue_time_adjustment; // L2 experimental
    void **overtake_danger_overtake_full_heat_time_adjustment; // L2 experimental
    void **overtake_danger_overtake_initiate_time_adjustment; // L2 experimental
    void **overtake_danger_overtake_pull_out_time; // L2 experimental
    uint8_t _pad_0x1a8[0x8];
    void **overtake_regular_overtake_continue_time_adjustment; // L2 experimental
    void **overtake_regular_overtake_full_heat_time_adjustment; // L2 experimental
    void **overtake_regular_overtake_initiate_time_adjustment; // L2 experimental
    void **overtake_regular_overtake_pull_out_time; // L2 experimental
    void **overtake_overlap_percentage; // L2 experimental
    void **overtake_concede_speed_diff; // L2 experimental
    void **overtake_concede_slowdown_speed_diff; // L2 experimental
    void **overtake_concede_time_vs_behind; // L2 experimental
    uint8_t _pad_0x1f0[0x48];
    void **space_seeking_added_buffer; // L2 experimental
    void **space_seeking_speed_below_follow; // L2 experimental
    void **space_seeking_first_speed_below_follow; // L2 experimental
    void **space_seeking_car_in_front_needed; // L2 experimental
    void **space_seeking_remember_crowed; // L2 experimental
    void **space_seeking_concede_overtake_blocked_by_crowed_time; // L2 experimental
    void **space_seeking_total_queue_needed; // L2 experimental
    void **space_seeking_distance_braking_point; // L2 experimental
    void **space_seeking_time_to_switch_sides; // L2 experimental
    void **space_seeking_track_distance_needed; // L2 experimental
    void **space_seeking_track_abort_distance; // L2 experimental
    void **space_seeking_cars_moving_away_speed_to_ignore; // L2 experimental
    void **space_seeking_can_start_track_radius; // L2 experimental
};
static_assert(offsetof(AiDriverConfig, alongside_speed_difference_for_limited_zone_yielding) == 0x20);
static_assert(offsetof(AiDriverConfig, alongside_yield_overtake_rate) == 0x28);
static_assert(offsetof(AiDriverConfig, alongside_min_presence_within_buffer) == 0x30);
static_assert(offsetof(AiDriverConfig, blocking_start_blocking_intersection_time) == 0x40);
static_assert(offsetof(AiDriverConfig, blocking_start_blocking_maximum_time_to_corner) == 0x48);
static_assert(offsetof(AiDriverConfig, blocking_start_blocking_minimum_time_to_corner) == 0x50);
static_assert(offsetof(AiDriverConfig, blocking_time_to_corner_for_end_merge_rate) == 0x58);
static_assert(offsetof(AiDriverConfig, blocking_time_to_corner_for_start_merge_rate) == 0x60);
static_assert(offsetof(AiDriverConfig, blocking_end_blocking_time_to_corner_vs_offset) == 0x68);
static_assert(offsetof(AiDriverConfig, blocking_blocking_offset_from_car_behind) == 0x70);
static_assert(offsetof(AiDriverConfig, blocking_blocking_speed_differential) == 0x78);
static_assert(offsetof(AiDriverConfig, blocking_blocking_threat_time_modifier) == 0x80);
static_assert(offsetof(AiDriverConfig, blocking_blocking_variable_offset_fraction) == 0x88);
static_assert(offsetof(AiDriverConfig, blocking_fraction_of_follow_speed) == 0x90);
static_assert(offsetof(AiDriverConfig, blocking_fraction_of_follow_speed_at_race_start) == 0x98);
static_assert(offsetof(AiDriverConfig, blocking_max_blocking_distance) == 0xa0);
static_assert(offsetof(AiDriverConfig, blocking_max_blocking_distance_at_race_start) == 0xa8);
static_assert(offsetof(AiDriverConfig, blocking_avoidance_decrease_rate) == 0xb0);
static_assert(offsetof(AiDriverConfig, blocking_merge_back_to_racing_line) == 0xb8);
static_assert(offsetof(AiDriverConfig, divebomb_defense_max_presence_alongside) == 0xf0);
static_assert(offsetof(AiDriverConfig, divebomb_defense_max_time_to_ignore) == 0xf8);
static_assert(offsetof(AiDriverConfig, divebomb_defense_max_track_offset) == 0x100);
static_assert(offsetof(AiDriverConfig, divebomb_defense_max_track_radius) == 0x108);
static_assert(offsetof(AiDriverConfig, offset_variation_max_offset) == 0x120);
static_assert(offsetof(AiDriverConfig, offset_variation_offset_limit_at_corner_start) == 0x128);
static_assert(offsetof(AiDriverConfig, offset_variation_offset_limit_blend_distance) == 0x130);
static_assert(offsetof(AiDriverConfig, offset_variation_half_width_scaling_graph) == 0x148);
static_assert(offsetof(AiDriverConfig, offset_variation_radius_scalar_graph) == 0x150);
static_assert(offsetof(AiDriverConfig, overtake_attacker_close_enough_car_lengths) == 0x158);
static_assert(offsetof(AiDriverConfig, overtake_braking_zone_abort_overtake_time) == 0x160);
static_assert(offsetof(AiDriverConfig, overtake_close_range_car_lengths_to_continue_overtake) == 0x168);
static_assert(offsetof(AiDriverConfig, overtake_close_range_car_lengths_to_start_overtake) == 0x170);
static_assert(offsetof(AiDriverConfig, overtake_oppertunist_overtake_time) == 0x178);
static_assert(offsetof(AiDriverConfig, overtake_oppertunist_overtake_track_speed_difference) == 0x180);
static_assert(offsetof(AiDriverConfig, overtake_danger_overtake_continue_time_adjustment) == 0x188);
static_assert(offsetof(AiDriverConfig, overtake_danger_overtake_full_heat_time_adjustment) == 0x190);
static_assert(offsetof(AiDriverConfig, overtake_danger_overtake_initiate_time_adjustment) == 0x198);
static_assert(offsetof(AiDriverConfig, overtake_danger_overtake_pull_out_time) == 0x1a0);
static_assert(offsetof(AiDriverConfig, overtake_regular_overtake_continue_time_adjustment) == 0x1b0);
static_assert(offsetof(AiDriverConfig, overtake_regular_overtake_full_heat_time_adjustment) == 0x1b8);
static_assert(offsetof(AiDriverConfig, overtake_regular_overtake_initiate_time_adjustment) == 0x1c0);
static_assert(offsetof(AiDriverConfig, overtake_regular_overtake_pull_out_time) == 0x1c8);
static_assert(offsetof(AiDriverConfig, overtake_overlap_percentage) == 0x1d0);
static_assert(offsetof(AiDriverConfig, overtake_concede_speed_diff) == 0x1d8);
static_assert(offsetof(AiDriverConfig, overtake_concede_slowdown_speed_diff) == 0x1e0);
static_assert(offsetof(AiDriverConfig, overtake_concede_time_vs_behind) == 0x1e8);
static_assert(offsetof(AiDriverConfig, space_seeking_added_buffer) == 0x238);
static_assert(offsetof(AiDriverConfig, space_seeking_speed_below_follow) == 0x240);
static_assert(offsetof(AiDriverConfig, space_seeking_first_speed_below_follow) == 0x248);
static_assert(offsetof(AiDriverConfig, space_seeking_car_in_front_needed) == 0x250);
static_assert(offsetof(AiDriverConfig, space_seeking_remember_crowed) == 0x258);
static_assert(offsetof(AiDriverConfig, space_seeking_concede_overtake_blocked_by_crowed_time) == 0x260);
static_assert(offsetof(AiDriverConfig, space_seeking_total_queue_needed) == 0x268);
static_assert(offsetof(AiDriverConfig, space_seeking_distance_braking_point) == 0x270);
static_assert(offsetof(AiDriverConfig, space_seeking_time_to_switch_sides) == 0x278);
static_assert(offsetof(AiDriverConfig, space_seeking_track_distance_needed) == 0x280);
static_assert(offsetof(AiDriverConfig, space_seeking_track_abort_distance) == 0x288);
static_assert(offsetof(AiDriverConfig, space_seeking_cars_moving_away_speed_to_ignore) == 0x290);
static_assert(offsetof(AiDriverConfig, space_seeking_can_start_track_radius) == 0x298);

struct AiCarRecord {
    uint8_t vehicle_index; // L2 experimental
    uint8_t _pad_0x1[0x22f];
    void *handling_config; // L2 experimental
    AiDriverConfig driver_config; // L2 experimental
};
static_assert(offsetof(AiCarRecord, vehicle_index) == 0x0);
static_assert(offsetof(AiCarRecord, handling_config) == 0x230);
static_assert(offsetof(AiCarRecord, driver_config) == 0x238);

struct AiSystem {
    uint8_t _pad_0x0[0xa8];
    AiCarRecord *records_begin; // L2 experimental
};
static_assert(offsetof(AiSystem, records_begin) == 0xa8);

inline constexpr address<"Ai_BehaviourTestRetire_Update"> Ai_BehaviourTestRetire_Update{}; // L2 experimental
inline constexpr address<"Ai_Calc_GripLimitScaleA"> Ai_Calc_GripLimitScaleA{}; // L2 experimental
inline constexpr address<"Ai_Calc_GripLimitScaleB"> Ai_Calc_GripLimitScaleB{}; // L2 experimental
inline constexpr address<"Ai_DriverVariationInRaceSystem_Setup"> Ai_DriverVariationInRaceSystem_Setup{}; // L2 experimental
inline constexpr address<"Ai_Job_UpdateCarsParallel"> Ai_Job_UpdateCarsParallel{}; // L2 experimental
inline constexpr address<"Ai_LineCache_UpdateIfChanged"> Ai_LineCache_UpdateIfChanged{}; // L2 experimental
inline constexpr address<"Ai_Select_ErsDeployMode"> Ai_Select_ErsDeployMode{}; // L2 experimental
inline constexpr address<"Ai_SplineCursor_Ctor"> Ai_SplineCursor_Ctor{}; // L2 experimental
inline constexpr address<"Ai_Update_CompoundPaceCurves"> Ai_Update_CompoundPaceCurves{}; // L2 experimental
inline constexpr site<"site_Ai_BuildSystem_RecordPushed"> site_Ai_BuildSystem_RecordPushed{}; // L2 experimental
inline constexpr site<"site_Ai_TableGripInput"> site_Ai_TableGripInput{}; // L2 experimental
inline constexpr site<"site_Ai_UpdateCars_CornerLineLoad"> site_Ai_UpdateCars_CornerLineLoad{}; // L2 experimental
inline constexpr site<"site_Ai_UpdateCars_NoCornerLine"> site_Ai_UpdateCars_NoCornerLine{}; // L2 experimental

}
