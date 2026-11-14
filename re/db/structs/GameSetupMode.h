// struct: GameSetupMode
// subsystem: session
// size: ?
// notes: one game_setup.game_setup_modes row, built once per row by Sess_Parse_GameSetupMode (0x142a14f20).
// String fields are holder pointers: the Gdb value record + 0x38, whose +0x10 is the decoded char* label;
// a missing value points at the shared empty holder 0x146cc42e8. The entrants manager's container (its
// vtable slot +0x1d0, native/src/core/era_picker.cpp) lists these objects; EntrantsManager +0x2b0 names
// the current one, and the DRS system resolves the current mode's DRS group from +0x1e0 by label.
struct GameSetupMode {
    /* 0x084 */ int32_t tyre_pitstop_threshold;   // L1 decomp:re/decomp/142a14f20.c;decomp:re/decomp/14357aed0.c tyre_regulations.pitstop_threshold, scaled by the race length into TyreRegulations +0xa4
    /* 0x08a */ bool tyre_setup_enabled;           // L1 decomp:re/decomp/142a14f20.c tyre_regulations.tyre_setup_enabled
    /* 0x08b */ uint8_t race0_length_requirement[13]; // L1 decomp:re/decomp/142a14f20.c;decomp:re/decomp/14358d1e0.c;decomp:re/decomp/14357aed0.c;decomp:re/decomp/143695ff0.c flags by race length (enum 0x14568e5d8); race session r (15/16/17) reads mode+0x8b+r*13+length at race start (0x142f29820 -> rule query 0x14358d1e0 case 0x13 TyreRegulations); the era picker writes all 39 per box (rules.mandatory_tyre_change)
    /* 0x098 */ uint8_t race1_length_requirement[13]; // L1 same readers
    /* 0x0a5 */ uint8_t race2_length_requirement[13]; // L1 same readers
    /* 0x0d8 */ void *name;               // L2 decomp:re/decomp/142a14f20.c;xref:0x142a152aa row label holder (+0x10 char*); the era picker reads it
    /* 0x1c8 */ bool has_tyre_warmers;     // L1 decomp:re/decomp/142a14f20.c;xref:0x142a15cd9 (lea "has_tyre_warmers") value byte of the bool record (+8) != 0; the era picker writes it per box (rules.tyre_warmers); readers not traced
    /* 0x1c9 */ bool use_f2_trophy;        // L1 decomp:re/decomp/142a14f20.c next bool after has_tyre_warmers ("use_f2_trophy" at 0x1457b8fcf)
    /* 0x1e0 */ void *drs_allowed_lap_group; // L2 decomp:re/decomp/142a14f20.c;decomp:re/decomp/142bebd80.c;xref:0x142a16192,0x142bebf4f,0x142becd55,0x142bef907,0x142befa73 holder of the drs_allowed_lap_group label (+0x10 char*), read only through +0x10 by the four DRS readers
};
