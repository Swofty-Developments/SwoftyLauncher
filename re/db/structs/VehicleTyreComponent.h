// struct: VehicleTyreComponent
// subsystem: live
// size: ?
// notes: Per-vehicle tyre component (TyreSystem +0x110 vector, Halo V). Slot table and fitted set as read by Sess_Tyre_ShowFittedSet. Compound visual ids: Halo reads the visual record behind the slot (+0x10).
struct VehicleTyreComponent {
    /* 0x9bc0 */ void * compound_class;                      // L2 decomp:re/decomp/142ea2a90.c;xref:0x142ea2acb,0x142ea2add,0x142ea2b85 the cached class the slot table was built for
    /* 0x9bd0 */ TyreSlot slots[5];                          // L2 decomp:re/decomp/142ea2a90.c;xref:0x142ea2aba stride 0x18: u32 option, +8 physical, +0x10 visual compound record
    /* 0x9c48 */ uint8_t slot_count;                         // L2 decomp:re/decomp/142ea2a90.c;xref:0x142ea2aec at most 5
    /* 0x9c50 */ void * built_for;                           // L2 decomp:re/decomp/142ea2a90.c;xref:0x142ea2ad2
    /* 0x9c58 */ uint8_t built;                              // L2 decomp:re/decomp/142ea2a90.c;xref:0x142ea2ac1 0 means the table is rebuilt first (0x142e9e6c0 per the caller)
    /* 0x9c90 */ uint8_t * set_begin;                        // L1 decomp:re/decomp/142ea2a90.c 0x78-byte set records
    /* 0x9c98 */ uint8_t * set_end;                          // L2 decomp:re/decomp/142ea2a90.c;xref:0x142ea2af3 the fitted set option is the dword at set_end - 0x6c
};
