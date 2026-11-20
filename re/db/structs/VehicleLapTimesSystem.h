// struct: VehicleLapTimesSystem
// subsystem: session
// size: ?
// notes: The lap times system (registry key k_Live_VehicleLapTimesSystemKey, type name "VehicleLapTimesSystem"); the UDP telemetry resolve 0x142cf5c00 keeps it at state+0xd298 and Sess_LapTimes_GetCarData 0x142f0c7f0 takes it first. One entry pointer per vehicle id.
struct VehicleLapTimesSystem {
    /* 0x058 */ VehicleLapTimesEntry **cars_begin;           // L2 decomp:re/decomp/142f0c7f0.c;decomp:re/decomp/142f2e520.c;xref:0x142f0c7f9,0x142f2e551,0x142f2e64e,0x142f2ac53 pointer per vehicle id: Sess_LapTimes_GetCarData bounds the id by (end - begin) >> 3 and returns [begin + id*8] + 0x38; a null pointer or a bad id answers the static default
    /* 0x060 */ VehicleLapTimesEntry **cars_end;             // L2 decomp:re/decomp/142f0c7f0.c;xref:0x142f0c7fd,0x142f2ac57,0x142f2ae79
};
