// struct: StandingsTableEntry
// subsystem: career
// size: 0x8 L2 decomp:re/decomp/140b30a90.c;xref:0x140b30c90 (add r9,8 over the table session's entries)
// notes: one driver's cumulative total in a StandingsSession of F1SeasonStandings.table: a copy of the previous table session with the session's StandingsPoints added (0x140b30a90, from Career_Recompute_SeasonStandings through 0x14962fff0); a key the previous session lacks is appended whole (0x140b30c50). Raised by std-libs with the lead's authorisation.
struct StandingsTableEntry {
    /* 0x000 */ uint32_t key;                        // L2 decomp:re/decomp/140b30a90.c;decomp:re/decomp/140b317a0.c;xref:0x140b30c99,0x140b30ca0,0x140b30c53,0x140b317cc entrant key, the entrants stat id shape {u8 kind +0, u16 index +2}: the game matches byte +0 and the u16 at +2 and ignores byte +1 (0x140b30ca0, 0x140b30c99); the last compare tiebreak is the u16 at +2, lower first (0x140b31a16)
    /* 0x004 */ int32_t total;                       // L2 decomp:re/decomp/140b30a90.c;decomp:re/decomp/140b317a0.c;xref:0x140b30ca9,0x140b317ac,0x140b317b0 the cumulative points: the session's points are added at 0x140b30ca9; the compare ranks the higher total first
};
