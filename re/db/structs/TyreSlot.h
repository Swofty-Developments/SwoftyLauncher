// struct: TyreSlot
// subsystem: live
// size: 0x18
// notes: Slot of VehicleTyreComponent.slots.
struct TyreSlot {
    /* 0x000 */ uint32_t option;                             // L2 decomp:re/decomp/142ea2a90.c;xref:0x142ea2b70 set option id (0 reserved..)
    /* 0x008 */ uint32_t physical_compound;                  // L1 decomp:re/decomp/142ea2a90.c
    /* 0x010 */ void * visual;                               // L1 decomp:re/decomp/142ea2a90.c visual compound record; +0 byte visual id, +0x50 enum (Halo)
};
