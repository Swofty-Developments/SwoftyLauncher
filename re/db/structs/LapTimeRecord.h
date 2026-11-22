// struct: LapTimeRecord
// subsystem: session
// size: 0xc8  L2 decomp:re/decomp/142cfe4f0.c (records 0xc8 bytes apart)
// notes: One lap of VehicleLapTimesData.laps. Times are seconds as float bits split into an XOR pair (value = a ^ b). The valid flag is a pair too: valid when (b & 1) == a. Sess_LapRecords_SortByTime orders records by time (+0xa8 ^ +0xb0); Sess_LapTimes_IsNewFastestLap sorts the records whose three sectors are non-zero and whose valid pair holds, and takes the first as the best lap.
struct LapTimeRecord {
    /* 0x000 */ uint8_t lap;                                 // L2 decomp:re/decomp/142f2e520.c;xref:0x142f2e61a,0x142f2e61d 0-based lap number: Sess_LapTimes_IsNewFastestLap reads it from the best record and treats 0xff (the static empty record 0x14604f2f8) as no best lap
    /* 0x008 */ uint32_t sector1_a;                          // L2 decomp:re/decomp/142cfd6c0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfdb13,0x142cfdb5f,0x142f2e580 sector 1 seconds = float bits (a ^ b); the UDP lap data sends it as sector1TimeMSPart/MinutesPart for the current lap
    /* 0x010 */ uint32_t sector1_b;                          // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfdb18,0x142cfdb64,0x142f2e585
    /* 0x018 */ uint32_t sector2_a;                          // L2 decomp:re/decomp/142cfd6c0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfdb2a,0x142cfdb9c,0x142f2e593 sector 2 seconds = float bits (a ^ b)
    /* 0x020 */ uint32_t sector2_b;                          // L2 decomp:re/decomp/142cfd6c0.c;xref:0x142cfdb2f,0x142cfdba1,0x142f2e598
    /* 0x028 */ uint32_t sector3_a;                          // L2 decomp:re/decomp/142cfe4f0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfe7ee,0x142cfe990,0x142f2e5a6 sector 3 seconds = float bits (a ^ b); a best lap needs it non-zero
    /* 0x030 */ uint32_t sector3_b;                          // L2 decomp:re/decomp/142cfe4f0.c;xref:0x142cfe7f2,0x142cfe995,0x142f2e5ab
    /* 0x098 */ uint8_t valid_a;                             // L2 decomp:re/decomp/142cfe4f0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfe7fe,0x142f2e5b9,0x142f2e631 lap valid when (valid_b & 1) == valid_a; the best lap takes only valid records and Sess_LapTimes_IsNewFastestLap returns the comparison for the best record
    /* 0x0a0 */ uint64_t valid_b;                            // L2 decomp:re/decomp/142cfe4f0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfe806,0x142f2e5c2,0x142f2e638
    /* 0x0a8 */ uint32_t time_a;                             // L2 decomp:re/decomp/142cfd6c0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfdaab,0x142cfdae4,0x142f2e621,0x142f2e661 lap seconds = float bits (a ^ b): the UDP lap data sends 1000 x it as currentLapTimeInMS for laps[current_lap] and lastLapTimeInMS for laps[current_lap - 1]; Sess_LapRecords_SortByTime sorts by it
    /* 0x0b0 */ uint32_t time_b;                             // L2 decomp:re/decomp/142cfd6c0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfdab3,0x142cfdaec,0x142f2e627,0x142f2e669
};
