// struct: PitLaneSystem
// subsystem: live
// size: ?
// notes: Registry key 0x1460fb708, type name "PitLaneSystem" (static init 0x1434e782d). Per-vehicle pit state records are inline at +0x68, stride 0x10. Linked into RelativeGapHelper at +0xd0 and into the UDP state at +0xd210.
struct PitLaneSystem {
    /* 0x068 */ PitLaneCarSlot car_slots[27];                // L2 decomp:re/decomp/142cfd6c0.c;decomp:re/decomp/140813ee0.c;xref:0x142cfd73e,0x142cfd7fa,0x142cfd85d,0x140813f4c,0x140814007,0x1408140a7 indexed by vehicle id; the game guards id < 0x1b
};
