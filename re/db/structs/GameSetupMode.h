// struct: GameSetupMode
// subsystem: session
// size: ?
// notes: one game_setup.game_setup_modes row, built once per row by Sess_Parse_GameSetupMode (0x142a14f20).
// String fields are holder pointers: the Gdb value record + 0x38, whose +0x10 is the decoded char* label;
// a missing value points at the shared empty holder 0x146cc42e8. The entrants manager's container (its
// vtable slot +0x1d0, native/src/core/era_picker.cpp) lists these objects; EntrantsManager +0x2b0 names
// the current one, and the DRS system resolves the current mode's DRS group from +0x1e0 by label.
struct GameSetupMode {
    /* 0x0d8 */ void *name;               // L2 decomp:re/decomp/142a14f20.c;xref:0x142a152aa row label holder (+0x10 char*); the era picker reads it
    /* 0x190 */ void *weekend_structures_alloc; // L2 decomp:re/decomp/142a14f20.c;decomp:re/decomp/142a2cb60.c;decomp:re/decomp/142a19f40.c vector of WeekendStructure (0x88 each) from weekend_options_0.weekend_structure[], each parsed by 0x142a23b30
    /* 0x198 */ WeekendStructure *weekend_structures_begin; // L2 decomp:re/decomp/1407c75f0.c;decomp:re/decomp/149439460.c;decomp:re/decomp/140be7260.c;decomp:re/decomp/140be9370.c;decomp:re/decomp/1421f80d0.c readers walk begin..end step 0x88; the era picker points the race sessions at a box's preset (rules.points)
    /* 0x1a0 */ WeekendStructure *weekend_structures_end; // L2 same readers
    /* 0x1c8 */ bool has_tyre_warmers;     // L1 decomp:re/decomp/142a14f20.c;xref:0x142a15cd9 (lea "has_tyre_warmers") value byte of the bool record (+8) != 0; the era picker writes it per box (rules.tyre_warmers); readers not traced
    /* 0x1c9 */ bool use_f2_trophy;        // L1 decomp:re/decomp/142a14f20.c next bool after has_tyre_warmers ("use_f2_trophy" at 0x1457b8fcf)
    /* 0x1e0 */ void *drs_allowed_lap_group; // L2 decomp:re/decomp/142a14f20.c;decomp:re/decomp/142bebd80.c;xref:0x142a16192,0x142bebf4f,0x142becd55,0x142bef907,0x142befa73 holder of the drs_allowed_lap_group label (+0x10 char*), read only through +0x10 by the four DRS readers
};
