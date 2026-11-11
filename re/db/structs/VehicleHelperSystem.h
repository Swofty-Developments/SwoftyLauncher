// struct: VehicleHelperSystem
// subsystem: live
// size: ?
// notes: Registry key 0x146096ac8, type name "VehicleHelperSystem". The rule manager keeps it at +0x1c30 as the participants object. Halo's "session component S1". Misconduct objects hang off it.
struct VehicleHelperSystem {
    /* 0x568 */ DriveThroughObject * drive_through;          // L2 decomp:re/decomp/142cf5c00.c;xref:0x142cf60a1 UDP state +0xd2e0 = [this+0x568]
    /* 0x578 */ StopGoObject * stop_go;                      // L2 decomp:re/decomp/142cf5c00.c;xref:0x142cf6093 UDP state +0xd2d8 = [this+0x578]
};
