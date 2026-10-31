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
    /* 0x088 */ bool allow_strategy_mandatory_stop_check; // L2 decomp:re/decomp/142a14f20.c;xref:0x142a171d0 tyre_regulations.allow_strategy_mandatory_stop_check, default true
    /* 0x089 */ bool allow_strategy_valid_laps_check; // L2 decomp:re/decomp/142a14f20.c;xref:0x142a17200 tyre_regulations.allow_strategy_valid_laps_check, default true
    /* 0x08a */ bool tyre_setup_enabled;  // L2 decomp:re/decomp/142a14f20.c;xref:0x142a17230 tyre_regulations.tyre_setup_enabled, default true
    /* 0x08b */ uint8_t race_length_flags[0x27]; // L2 decomp:re/decomp/142a14f20.c;xref:0x142a16a20,0x142a17291 the race0, race1 and race2 length_requirement lists as flag bytes indexed by the length option (zeroed at 0x142a14f8a..0x142a15000 for 0x8b..0xb1)
    /* 0x1e0 */ void *drs_allowed_lap_group; // L2 decomp:re/decomp/142a14f20.c;decomp:re/decomp/142bebd80.c;xref:0x142a16192,0x142bebf4f,0x142becd55,0x142bef907,0x142befa73 holder of the drs_allowed_lap_group label (+0x10 char*), read only through +0x10 by the four DRS readers
};
