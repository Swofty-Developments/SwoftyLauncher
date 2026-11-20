// struct: VehicleLapTimesData
// subsystem: session
// size: ?
// notes: A vehicle's lap data (VehicleLapTimesEntry + 0x38, what Sess_LapTimes_GetCarData returns): 200 lap records of 0xc8 bytes from +0x8, then the current lap index. The static default for a bad id is built by 0x140987660. The lap advance itself runs in protected code and was not located; the stores below are the resets.
struct VehicleLapTimesData {
    /* 0x0008 */ LapTimeRecord laps[200];                    // L2 decomp:re/decomp/142cfe4f0.c;decomp:re/decomp/142f2e520.c;xref:0x142cfe7ce,0x142cfe7c6,0x142f2e565,0x142f2e577 the session history builder and Sess_LapTimes_IsNewFastestLap walk 200 records 0xc8 bytes apart from +0x8 (both loops end at 0x9c40 = 200 x 0xc8)
    /* 0x9c48 */ uint8_t current_lap;                        // L2 decomp:re/decomp/142cfd6c0.c;decomp:re/decomp/142cfe4f0.c;decomp:re/decomp/142d00060.c;decomp:re/decomp/142f2abd0.c;xref:0x142cfda7a,0x142cfe53a,0x142cfec39,0x142d00d29,0x142d04d37,0x142f2ac7b,0x1409878d5 the current lap's record index (laps completed): the UDP lap data reads laps[current_lap] as the current lap time and sectors 1 and 2 and laps[current_lap - 1] as the last lap time; the UDP session packet's totalLaps outside a race is current_lap + 1 (0x142d04d37); Sess_LapTimes_Reset stores 0 through the entry (0x142f2ac7b, entry + 0x9c80) and so does the data constructor 0x140987660 (0x1409878d5)
};
