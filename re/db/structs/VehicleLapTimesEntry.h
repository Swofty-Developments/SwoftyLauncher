// struct: VehicleLapTimesEntry
// subsystem: session
// size: ?
// notes: One vehicle's entry of VehicleLapTimesSystem.cars_begin. Sess_LapTimes_GetCarData returns entry + 0x38 (add rsi, 0x38 at 0x142f0c816), the lap data the UDP lap data, car status and session history builders index.
struct VehicleLapTimesEntry {
    /* 0x038 */ VehicleLapTimesData data;                    // L2 decomp:re/decomp/142f0c7f0.c;decomp:re/decomp/142f2e520.c;xref:0x142f0c816,0x142f2e565 Sess_LapTimes_GetCarData returns entry + 0x38; Sess_LapTimes_IsNewFastestLap walks the same records from entry + 0x40 (data + 8)
};
