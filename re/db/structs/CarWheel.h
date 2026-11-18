// struct: CarWheel
// subsystem: car
// size: 0x480  L2 decomp:re/decomp/144753050.c (alloc 0x480, 0x10 aligned)
// notes: one of the four wheels at CarSuspension+0x70+i*8 (i: 0 RL, 1 RR, 2 FL, 3 FR). Fields below +0x3b0 from
// Car_Tyre_GripScale; the +0x3b0.. block is the telemetry row order (older L1 note) and needs a trace.
// The tyre-set apply 0x142e88840 takes wheel 0 as [[car+0x580]+0x70] (0x142e8885c) and writes +0x88 (wear) and +0x94 on
// it (0x142e888a2, 0x142e888b0): the same offsets as CarTyreState, so the two files very likely describe one object.
// The wheel's index into a TyreSet's wheel array is (tyre_set_side == 2) + (tyre_set_axle >= 2 ? 2 : 0) (0x142e8886f..0x142e88884).
struct CarWheel {
    /* 0x0020 */ uint8_t tyre_flags;                           // L2 decomp:own capstone disasm 0x142e88840-0x142e88c2a,0x155444020-0x155444374,0x1446b5720-0x1446b5f57;xref:0x142e88894,0x1554441ff,0x15544424d,0x155444337,0x15544433f,0x142e888b8,0x142e88902,0x142e8892f,0x1446b5baf,0x1446b5c72,0x1446b5d6c,0x1446b5d72 puncture state: bit 1 punctured, set right after Car_Puncture_Add 0x14464e200 succeeds (0x142e88902, 0x1446b5baf, 0x1446b5ca5, 0x1446b5d3a, 0x1446b5dff, 0x1446b5ee5) and cleared with 0xfd after 0x14464e3a0 (0x142e8892f); bit 0 burst, set only while bit 1 is set (test al,2 / or al,1 at 0x1446b5d6c/0x1446b5d70); flags & 3: 0 none, 2 punctured, 3 burst (0x1446b5c72 tests & 3). Bits 2 and 3: pending hits, inference. The tyre-set apply 0x142e88840 first resets each wheel through 0x1446b6d10(wheel+8, wheel, 0x1002) (0x142e88894, body 0x155444020): it clears bit 0 (0x1554441ff), removes a puncture and clears bit 1 (0x15544424d) and stores {0, 0, 1.0, 0} at +0x88..+0x94 (0x155444337) and 0 at +0x98 (0x15544433f), so its burst test at 0x142e888bc never fires, and it then adds a puncture only where the fitted set's per-wheel byte is set (0x142e88902). The flags before a tyre change hold the damage the car came in with; after it they hold only the punctures the new set's per-wheel bytes add.
    /* 0x0098 */ float deflation;                              // L2 decomp:own capstone disasm 0x142e88840-0x142e88c2a,0x1446b5720-0x1446b5f57;xref:0x142e8890e,0x142e88933,0x1446b5bb3,0x1446b5cef,0x1446b5cff,0x1446b5eaa deflation level: max(1.0, x) when a puncture is added (0x142e8890e, 0x1446b5bb3), 0 when it is removed (0x142e88933), grown by the tyre update 0x1446b5720 (vaddss at 0x1446b5c4f, 0x1446b5cef, 0x1446b5e9a), compared with the constant at 0x1453ca8c8 (0x1446b5cff, 0x1446b5eaa) before the burst bit is set
    /* 0x00f8 */ CarObfFloat max_brake_torque;                // L2 decomp:re/decomp/144676250.c;xref:0x140619eb9 written by Car_BrakeEcu_ApplyBias: axle torque (bias blend) x brake pressure
    /* 0x0130 */ float duct_factor;                           // L1 decomp:re/decomp/1446dceb0.c per-wheel factor multiplied into BrakeDuct*CoeffLift/Drag (mean of the axle pair); meaning needs a trace
    /* 0x0138 */ uint8_t brake_locked_out;                    // L1 decomp:re/decomp/144676250.c non-zero = ApplyBias skips this wheel (failed/disabled brake?)
    /* 0x0188 */ float spin_rate;                             // L2 decomp:re/decomp/1446b7240.c;xref:0x1446df89a,0x1446df8ce,0x1446df901,0x1446df949 wheel angular velocity (abs taken), rad/s
    /* 0x02b4 */ uint32_t tyre_set_side;                       // L1 decomp:own capstone disasm 0x142e88840-0x142e88c2a;xref:0x142e8886f 2 adds 1 to the wheel's TyreSet index
    /* 0x02b8 */ uint32_t tyre_set_axle;                       // L1 decomp:own capstone disasm 0x142e88840-0x142e88c2a;xref:0x142e8887d 2 or more adds 2 to the wheel's TyreSet index
    /* 0x03a8 */ float contact_vel_x;                         // L2 decomp:re/decomp/1446b7240.c;xref:0x1446df89a,0x1446df8ce,0x1446df901,0x1446df949 contact-patch velocity x; |(x,z)|/TyreRadius vs SpeedDownslopeRotRate
    /* 0x03ac */ float contact_vel_z;                         // L2 decomp:re/decomp/1446b7240.c;xref:0x1446df89a,0x1446df8ce,0x1446df901,0x1446df949 contact-patch velocity z
    /* 0x03b0 */ float slip_angle;                            // L1 decomp:re/decomp/1446e4330.c published by Car_Ers_IntegrateAndPublishWheels
    /* 0x03b4 */ float slip_ratio;                            // L1 decomp:re/decomp/1446e4330.c <- solver +0x760
    /* 0x03c0 */ float lateral_force;                         // L1 decomp:re/decomp/1446e4330.c
    /* 0x03c4 */ float longitudinal_force;                    // L1 decomp:re/decomp/1446e4330.c <- solver +0x7e0
};
