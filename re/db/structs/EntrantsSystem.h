// struct: EntrantsSystem
// subsystem: live
// size: ?
// notes: Registry key 0x14606ecd8 (vtable 0x14578af00, slot 0 0x1424cb490 returns the key). Holds the team tables: system+0x2c0+set*0x10 = {allocator, TeamSlots *} (struct TeamRecord's header describes set 0 = the database teams, set 1 = My Team additions). The HUD view-model init 0x141759a40 stores registry(key) at its +0x28 and 0x141760020 indexes +0x2c8 by the entrant's team table byte (0..2).
struct EntrantsSystem {
    /* 0x2c0 */ TeamSetRef team_sets[3];                     // L2 decomp:re/decomp/141760020.c;decomp:re/decomp/141759a40.c;xref:0x1417600c6,0x141759ab8 table pointer at +0x2c8+set*0x10, the team table byte is checked <= 2 first
};
