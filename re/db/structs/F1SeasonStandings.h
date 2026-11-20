// struct: F1SeasonStandings
// subsystem: career
// size: ?
// notes: game::championship::standings::F1SeasonStandings, embedded in Season at +0x2a8 (drivers). Derived from session results; rebuilt by Career_Rebuild_SeasonStandings. Raised by ce-season with the lead's authorisation.
struct F1SeasonStandings {
    /* 0x000 */ void **vtable;                       // L2 decomp:re/decomp/140b2f860.c;decomp:re/decomp/1413cdff0.c;xref:0x1413ce049,0x140b2f87b standings vtable 0x145670be0 (slot +0x38 = points recompute 0x1413cdff0)
    /* 0x020 */ StandingsRound *points_begin;        // L2 decomp:re/decomp/1413cdff0.c;decomp:re/decomp/140b2fc20.c;xref:0x1413ce00c,0x1413ce05d points per round -> session -> StandingsPoints
    /* 0x028 */ StandingsRound *points_end;          // L2 decomp:re/decomp/1413cdff0.c;xref:0x1413ce010,0x1413ce052 end of the points rounds
    /* 0x040 */ StandingsRound *table_begin;         // L2 decomp:re/decomp/140b2fa60.c;decomp:re/decomp/14962fff0.c;xref:0x149618f68,0x149618fc3,0x149619017,0x14aab92c0 the cumulative table per round -> session -> StandingsTableEntry, every session sorted by Career_Compare_StandingsEntries; the ctor zeroes begin and end with one 16-byte store at 0x14aab92c0. Raised by std-libs with the lead's authorisation.
    /* 0x048 */ StandingsRound *table_end;           // L2 decomp:re/decomp/140b2fa60.c;xref:0x149618f64,0x149618fc7,0x14961901b,0x14aab92c0 end of the table rounds. Raised by std-libs with the lead's authorisation.
    /* 0x0a8 */ StandingsPoints *extra_points_begin; // L2 decomp:re/decomp/1413ccb90.c;decomp:re/decomp/1413cdff0.c;xref:0x1413ccba0,0x1413ce12f the extra-points list from the rebuild, added as one more points entry
    /* 0x0b0 */ StandingsPoints *extra_points_end;   // L2 decomp:re/decomp/1413ccb90.c;decomp:re/decomp/1413cdff0.c;xref:0x1413ccba7,0x1413ce106 end of the extra-points list
    /* 0x0d0 */ StandingsRound *results_begin;       // L2 decomp:re/decomp/1413cd970.c;decomp:re/decomp/1413cdff0.c;xref:0x1413cd980,0x1413ce056 results per round -> session -> StandingsResult
    /* 0x0d8 */ StandingsRound *results_end;         // L2 decomp:re/decomp/1413cd970.c;decomp:re/decomp/1413cdff0.c;xref:0x1413cd987,0x1413ce061 end of the result rounds
};
