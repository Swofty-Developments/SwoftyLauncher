// struct: TyreStint
// subsystem: session
// size: 0x78  L2 decomp:re/decomp/142e9eda0.c (vector of 0x78-byte records)
// notes: One record of VehicleTyreComponent.set_begin..set_end: a set the car ran, in fitting order; the last record is the fitted set. Car_TyreSelection_SetCurrentTyres(selection, {compound, index}, lap) writes lap into the last record's end_lap, then appends the new set with end_lap 0xffff.
struct TyreStint {
    /* 0x00c */ uint32_t compound;                           // L2 decomp:re/decomp/142e9eda0.c;decomp:re/decomp/142d00060.c;xref:0x142e9fc1d,0x142e9fd2f,0x142e9ee1b,0x142d00d50,0x142d015c6 the set's tyre option; SetCurrentTyres stores param_2[0] and compares the last record's (set_end - 0x6c) with it
    /* 0x010 */ uint8_t set_index;                           // L2 decomp:re/decomp/142e9eda0.c;decomp:re/decomp/142d00060.c;xref:0x142e9fc22,0x142e9fd33,0x142e9ee17,0x142d00d31,0x142d015cb the set's index; SetCurrentTyres stores param_2[1] and returns early when compound and index equal the last record's; the UDP car status counts tyre age over the records with the fitted record's compound and set_index
    /* 0x074 */ uint16_t end_lap;                            // L2 decomp:re/decomp/142e9eda0.c;decomp:re/decomp/142d00060.c;decomp:re/decomp/142cfe4f0.c;xref:0x142e9fba2,0x142e9fcf6,0x142e9fdf3,0x142d015c0,0x142cfec40,0x142cfec54 the lap index the stint ended on, 0xffff while fitted (SetCurrentTyres writes the lap into the last record, then 0xffff into the new one); the UDP car status adds min(end_lap + 1, current_lap) - (previous record's end_lap + 1), floored at 0, for each matching record as tyresAgeLaps, and the session history builder reads it as the stint end
};
