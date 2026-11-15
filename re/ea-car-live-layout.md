# Car live layouts on the EA App build

Static comparison of the Steam exe (sha256 d873a1c8, PE timestamp 1789118616) with the decrypted EA App memory dump `F1_25.1789128892.110548.mem` (PE timestamp 1789128892, image size 0x155dc000). No game was launched.

For every field `native/src/core/hooks/car_live.hpp` reads (`FIELDS`, the vehicle table path, the system pointers of CarLive and the suspension path), the KB struct row names the function that reads or writes it. `re/tools/carlivecmp.py` (field list `re/tools/carlivecmp.fields`) uses `re/tools/layoutcmp.py`, which finds that function on EA by a pattern of its first instructions that matches exactly once, aligns both instruction streams with relocated operands normalised, and the check below requires every Steam instruction whose memory operand has the field's displacement to have an aligned EA instruction with the same text. A VTF section reader takes the system's data object, which is the system plus 0x18, so a reader carries the field's offset minus 0x18; the constructors that place the data object at +0x18 are in the table as `data_vtable`. An obfuscated field needs both its value and its key (8 bytes on) to match.

All 93 rows match. A function listed alone is instruction-identical on both builds.

| struct.field | offset | displacement relative to | evidence function (Steam address) |
|---|---|---|---|
| CarBrakeEcu.brake_bias | 0x21d8 | system | 0x144676250 |
| CarBrakeEcu.brake_pressure | 0x21dc | system | 0x144676250 |
| CarDamageFrontWing.chassis_damage_multiple | 0x30 | system or data object | 0x14465f450, 0x14465fdc0 |
| CarDamageFrontWing.damage_scale | 0x2c | system or data object | 0x14465f450, 0x14465fdc0 |
| CarDamageFrontWing.full_detach_scale | 0x38 | system or data object | 0x144631a50, 0x14465f450 |
| CarDamageFrontWing.side_detach_scale | 0x34 | system or data object | 0x144631a50, 0x14465f450 |
| CarDamageFrontWing.speed_threshold | 0x28 | system or data object | 0x14465f450, 0x14465fdc0 |
| CarDamageSuspension.angular_fracture_limit | 0x7c | data object (+0x18) | 0x144653070, 0x1446533e0 |
| CarDamageSuspension.front_damage_scale_obf | 0x38 | system or data object | 0x144653070, 0x1446533e0 |
| CarDamageSuspension.linear_fracture_limit | 0x78 | system or data object | 0x144653070, 0x1446533e0 |
| CarDamageSuspension.parts_detach_scale_obf | 0x68 | system or data object | 0x144653070, 0x1446533e0 |
| CarDamageSuspension.rear_damage_scale_obf | 0x48 | system or data object | 0x144653070, 0x1446533e0 |
| CarDamageSuspension.speed_threshold_obf | 0x28 | system or data object | 0x144653070, 0x1446533e0 |
| CarDamageSuspension.wheel_detach_scale_obf | 0x58 | system or data object | 0x144653070, 0x1446533e0 |
| CarDifferential.diff_off_throttle | 0xec | system | 0x1553d6f70 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarDifferential.diff_on_throttle | 0xf0 | system | 0x1553d6f70 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarEngine.vtf_limiter_rotation_rate | 0x8b0 | system or data object | 0x144685680 |
| CarF1Aero.aero_slip_stream_air_density_scale_drag | 0x118 | system or data object | 0x1446c8560 |
| CarF1Aero.aero_slip_stream_air_density_scale_front_wing | 0xe8 | system or data object | 0x1446c8560 |
| CarF1Aero.aero_slip_stream_air_density_scale_lift | 0x108 | system or data object | 0x1446c8560 |
| CarF1Aero.aero_slip_stream_air_density_scale_rear_wing | 0xf8 | system or data object | 0x1446c8560 |
| CarF1Aero.aero_slip_stream_car_width | 0x128 | system or data object | 0x1446c8560 |
| CarF1Aero.drag_scale | 0x19a0 | system | 0x1446dceb0, 0x1555e9310 |
| CarF1Aero.lift_scale | 0x19a4 | system | 0x1446dceb0 |
| CarFuelEcu.fuel_burn_scale_lean | 0x28 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_burn_scale_max | 0x58 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_burn_scale_rich | 0x48 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_burn_scale_standard | 0x38 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_power_scale_lean | 0x68 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_power_scale_max | 0x98 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_power_scale_rich | 0x88 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_power_scale_standard | 0x78 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_revs_scale_lean | 0xa8 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_revs_scale_max | 0xd8 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_revs_scale_rich | 0xc8 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.fuel_revs_scale_standard | 0xb8 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.temperature_scale_lean | 0xe8 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.temperature_scale_max | 0x118 | data object (+0x18) | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.temperature_scale_rich | 0x108 | data object (+0x18) | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelEcu.temperature_scale_standard | 0xf8 | system or data object | 0x1553fb310 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelTank.fuel_mass | 0x100 | system | 0x14468edb0 |
| CarGearBox.final_drive_ratio | 0x28 | data object (+0x18) | 0x1446a39a0 |
| CarGearBox.gear_ratio1st | 0x58 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio2nd | 0x68 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio3rd | 0x78 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio4th | 0x88 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio5th | 0x98 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio6th | 0xa8 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio7th | 0xb8 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio8th | 0xc8 | system or data object | 0x1446a39a0 |
| CarGearBox.gear_ratio9th | 0xd8 | system or data object | 0x1446a39a0 |
| CarGearBox.max_clutch_torque | 0xe8 | system or data object | 0x1446a39a0 |
| CarSteering.max_steering_lock | 0x28 | system or data object | 0x142cf9820 |
| CarSuspAxle.ride_height_offset | 0x15e4 | system | 0x1446dceb0 |
| CarSuspAxle.setup_anti_roll | 0x1664 | system | 0x1446a7be0 |
| CarSuspAxle.setup_camber | 0x1650 | system | 0x1446a7be0 |
| CarSuspAxle.setup_toe | 0x1654 | system | 0x1446a7be0 |
| CarSuspTyreType.grip_multiplier | 0x2c | system or data object | 0x1446aa090 |
| CarTurbo.max_boost | 0x2d8 | data object (+0x18) | 0x15543fad0 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarLive.vehicle_id | 0xe8 | system or data object | 0x144628190 |
| CarLive.is_ai | 0x124 | system | 0x1446dc480 |
| CarComponentBase.car | 0x8 | system | 0x1447484f0 |
| CarSuspension.tyre_types | 0x38 | system or data object | 0x1446aa090 |
| CarSuspension.axle0 | 0x90 | system or data object | 0x144753050 |
| CarSuspension.axle1 | 0x98 | system or data object | 0x144753050 |
| CarSuspAxle.local_rest_position_z | 0x1598 | system | 0x1446a7be0 |
| CarSuspTyreType.name | 0x0 | system | 0x1446aa090 |
| CarLive.engine | 0x530 | system | 0x14473c2e0 |
| CarLive.gearbox | 0x538 | system | 0x14473c2e0 |
| CarLive.fuel_tank | 0x540 | system | 0x14473c2e0 |
| CarLive.fuel_ecu | 0x548 | system or data object | 0x14473c2e0 |
| CarLive.brake_ecu | 0x550 | system or data object | 0x14473c2e0 |
| CarLive.chassis | 0x558 | system or data object | 0x14473c2e0 |
| CarLive.steering | 0x560 | system or data object | 0x14473c2e0 |
| CarLive.f1_aero | 0x570 | system or data object | 0x14473c2e0 |
| CarLive.differential | 0x578 | system or data object | 0x14473c2e0 |
| CarLive.suspension | 0x580 | system or data object | 0x14473c2e0 |
| CarLive.turbo | 0x5a0 | system or data object | 0x14473c2e0 |
| CarLive.f1_wing_damage | 0x5c8 | system or data object | 0x14473c2e0 |
| CarLive.f1_suspension_damage | 0x5d0 | system or data object | 0x14473c2e0 |
| CarSteering.data_vtable | 0x18 | system or data object | 0x14474e650 |
| CarGearBox.data_vtable | 0x18 | system or data object | 0x1555c4f90 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarDifferential.data_vtable | 0x18 | system or data object | 0x1447518c0 |
| CarTurbo.data_vtable | 0x18 | system or data object | 0x1555ded40 (displacement instructions identical; the function differs elsewhere in opaque constants) |
| CarFuelTank.data_vtable | 0x18 | system or data object | 0x144749420 |
| CarEngine.data_vtable | 0x18 | data object (+0x18) | 0x14467e900 |
| CarF1Aero.data_vtable | 0x18 | system or data object | 0x1555e9310 |
| CarBrakeEcu.data_vtable | 0x18 | system or data object | 0x1555d2450 |
| CarChassis.data_vtable | 0x18 | system or data object | 0x14474c060 |
| CarFuelEcu.data_vtable | 0x18 | system or data object | 0x14468a2b0 |
| CarSuspension.data_vtable | 0x18 | system or data object | 0x144753050 |
| CarComponentBase.data_vtable | 0x18 | system or data object | 0x1447484f0 |
| CarChassis.mass (chassis.ballast_kg) | 0x28 | data object (+0x18) | 0x1553bbbc0 (value `[rsi + 0x10]`, key `[rsi + 0x18]`, identical on EA at 0x152621d30) |

Not compared, and off on EA: the calls a write re-runs (Car_Engine_ComputeDerived_Thunk, Car_Tuning_SetFinalDrive, the chassis OnReset slot, Car_FuelEcu_SetMode_Thunk, Car_Tyre_SetWearFactor), CarFuelEcu.fuel_mode (+0x128), the engine mode availability object (a Steam RVA), the car game points (car.built, car.reset, car.upgrades.replayed, car.setup.applied) and the lever sites. The table is read only on EA for that reason.
