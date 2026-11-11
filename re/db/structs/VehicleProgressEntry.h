// struct: VehicleProgressEntry
// subsystem: live
// size: ?
// notes: One per vehicle, [GameVehicleProgressSystem.entries_begin + id*8]. GetLapProgress returns entry+0x30, so the block fields below are entry+0x30+x. Values are XOR pairs (anti-tamper). lap_m = clamp(total - (progress_lap + delta) * track_length, 0, track_length).
struct VehicleProgressEntry {
    /* 0x038 */ uint16_t lap_a;                              // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfd88a,0x142cfd959,0x142cfda82;decomp:re/decomp/1446046a0.c lap counter = (int16)(lap_a ^ lap_b), 0 on lap 1
    /* 0x040 */ uint16_t lap_b;                              // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfd892,0x142cfda95,0x142cfdb13
    /* 0x048 */ uint16_t progress_lap_a;                     // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfd7eb,0x142cfd89e,0x142cfda4a progress lap = (int16)(a ^ b) + (int16)delta
    /* 0x050 */ uint16_t progress_lap_b;                     // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfd8a6,0x142cfda8a,0x142cfdb2a
    /* 0x058 */ int16_t progress_lap_delta;                  // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfd721,0x142cfda91,0x142cfdb2f
    /* 0x098 */ uint32_t total_a;                            // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfd73e,0x142cfd7fa,0x142cfd85d total distance m = float bits (a ^ b)
    /* 0x0a0 */ uint32_t total_b;                            // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfd709,0x142cfd861,0x142cfd8dd
};
