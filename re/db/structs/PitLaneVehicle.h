// struct: PitLaneVehicle
// subsystem: live
// size: ?
// notes: Per-vehicle pit lane movement record (stand-in name). The pit movement update 0x1434b83e0 sets in_lane in case 10 (0x1434b91e1) and tests it for the lane states 2,3,6,7,14,15 (0x1434b85a0). UDP lap data pit status = 0 when in_lane is 0, else 2 when PitLaneCarState.state == 4, else 1 (0x142cfe2b4-0x142cfe2ed).
struct PitLaneVehicle {
    /* 0x0e0 */ uint32_t garage_index;                       // L2 decomp:re/decomp/1434b5110.c;decomp:re/decomp/1434d1260.c;xref:0x1434b56bc,0x1434b591a garage index Game_PitLane_AssignGarageIndices writes (0x7fffffff unassigned, 2 x team garage slot + seat, or (2 x slot | 1) - seat on tracks with the other garage order, 99 the safety car); Game_PitLane_PutVehicleInPitBox reads its pit box as +0xe0 >> 1, 12 for 99. Steam 1.0.145: AssignGarageIndices 0x1434b7690 450/450 equal with +0xe0 at the same instructions (sigport --witness, re-bindings 2026-11-18)
    /* 0x103 */ uint8_t in_lane;                             // L2 decomp:re/decomp/142cfd6c0.c;decomp:re/decomp/1434b83e0.c;xref:0x142cfe2dc,0x142cfe38c,0x1434b91e1,0x1434b85a0,0x1434b88c7 nonzero while the vehicle is in the pit lane
};
