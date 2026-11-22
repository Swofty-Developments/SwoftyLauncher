# Save bag value types

The property-bag stream inside a decoded save container, read from the Steam 1.0.141.7865 exe (KB build, `d873a1c8`) by disassembly. Wire layout: `docs/reference/save-format.md`. Decoder and encoder: `packages/core/src/games/f1_25/saves/bag/`.

## Stream functions

| Address | Role |
|---|---|
| 0x1439340c0 | `Game_SaveData_WriteBag`, codec vt+0x8 |
| 0x143934410 | `Ne_BagCodec_WriteList` (record `Ne_BagCodec_WriteList`) |
| 0x143934620 | `Ne_BagCodec_ReadBag`, codec vt+0x10 |
| 0x143935010 | `Ne_BagCodec_ReadList` (record `Ne_BagCodec_ReadList`) |
| 0x143933320 | `Ne_Bag_ReadValue`, leaf read |
| 0x14568e060 | `k_Ne_BagCodecVtbl`; vt+0x18 returns the stream version 1 |
| 0x1439532f0 | registers 30 types: pbag, plst and 28 value types |
| 0x14394aa50 | registers blwt, uist, dbrf |

## Type table

A type object's vtable: +0x10 id (`mov eax, <fourcc>; ret`), +0x20 value offset, +0x30 alignment, +0x78 stream writer, +0x80 stream reader, +0x88 value size in bytes. The id spells its name in stream byte order. Found by scanning the exe for vtables whose slot +0x10 is `mov eax, <four printable bytes>; ret`, then reading the slots.

| Id | Vtable | Writer (+0x78) | Reader (+0x80) | Value size (vt+0x88) | Stream body |
|---|---|---|---|---|---|
| pbag | 0x145890c80 | 0x143930dc0 | 0x143930f30 | - | nested bag |
| plst | 0x145890d30 | 0x143939970 | 0x143939ae0 | - | nested list |
| si08 | 0x145890ec0 | 0x143941e50 | 0x143941ea0 | 1 | 1 raw bytes |
| ui08 | 0x145890f70 | 0x1439421a0 | 0x1439421f0 | 1 | 1 raw bytes |
| si16 | 0x145891020 | 0x1439424f0 | 0x143942540 | 2 | 2 raw bytes |
| ui16 | 0x1458910d0 | 0x143942860 | 0x1439428b0 | 2 | 2 raw bytes |
| si32 | 0x145891180 | 0x143942d20 | 0x143942d70 | 4 | 4 raw bytes |
| ui32 | 0x145891230 | 0x143943070 | 0x1439430c0 | 4 | 4 raw bytes |
| si64 | 0x1458912e0 | 0x143943520 | 0x143943570 | 8 | 8 raw bytes |
| ui64 | 0x145891390 | 0x1439439e0 | 0x143943a30 | 8 | 8 raw bytes |
| bool | 0x145891440 | 0x143943d40 | 0x143943d90 | 1 | 1 raw bytes |
| fp32 | 0x1458914f0 | 0x1439440a0 | 0x1439440f0 | 4 | 4 raw bytes |
| fp64 | 0x1458915a0 | 0x1439443f0 | 0x143944440 | 8 | 8 raw bytes |
| rgba | 0x145891650 | 0x1439448c0 | 0x143944910 | 4 | 4 raw bytes |
| vec2 | 0x145891700 | 0x143944d00 | 0x143944d50 | 8 | 8 raw bytes |
| vec3 | 0x1458917b0 | 0x143945220 | 0x143945270 | 12 | 12 raw bytes |
| vec4 | 0x145891860 | 0x1439457d0 | 0x143945820 | 16 | 16 raw bytes |
| mat4 | 0x145891910 | 0x1439463f0 | 0x143946440 | 64 | 64 raw bytes |
| quat | 0x1458919c0 | 0x143945c50 | 0x143945ca0 | 16 | 16 raw bytes |
| shnm | 0x145891a80 | 0x143946910 | 0x143946960 | 16 | 16 raw bytes |
| st32 | 0x145891b30 | 0x143946f80 | 0x143946fe0 | - | u8 len + chars |
| st64 | 0x145891be0 | 0x1439477a0 | 0x143947800 | - | u8 len + chars |
| s128 | 0x145891c90 | 0x143947d40 | 0x143947da0 | - | u8 len + chars |
| s256 | 0x145891d40 | 0x143948300 | 0x143948360 | - | u16 len + chars |
| auid | 0x145891df0 | 0x143948980 | 0x1439489d0 | 8 | 8 raw bytes |
| woid | 0x145891eb0 | 0x143949030 | 0x143949080 | 16 | 16 raw bytes |
| sstr | 0x145891f60 | 0x1439499e0 | 0x143949a50 | - | u16 len + chars |
| matp | 0x145892010 | 0x143949620 | 0x143949670 | 16 | 16 raw bytes |
| dstr | 0x1458920c0 | 0x14394a0c0 | 0x14394a130 | - | u32 len + chars |
| bffr | 0x145892170 | 0x14394a640 | 0x14394a6b0 | - | u32 len + bytes |
| blwt | 0x145859840 | 0x143689240 | 0x1436892c0 | 16 | 4 bytes (first float of the 16-byte value) |
| uist | 0x145a73480 | 0x144197310 | 0x1441973c0 | - | u32 len + chars |
| dbrf | 0x145a74400 | 0x1442bafc0 | 0x1442bb180 | - | u8 flag; if 1: u8 n + n x u32 |

## Facts read from the code

- A bag entry is `key_len:u8, key, type_id:u32, value`; the key goes through the `st64` writer, from a 0x40-byte buffer. A list item is `type_id:u32, value`. WriteBag and the list writer compare the id with `pbag` and `plst` first and recurse; every other id goes through the type's +0x78 writer.
- The fixed-size writers (si08 to quat, shnm, woid, matp, auid, bool, rgba) share one body: value pointer from property vt+0x30, size from type vt+0x88, one stream vt+0x48 write.
- The `st32` reader reads the length byte, then at most 0x1f characters; a longer stored string is skipped past the buffer (`stream vt+0x58`).
- `sstr` writes the strlen as u16, then the characters; `bffr` writes `end - begin` as u32, then the bytes.
- `blwt` writes 4 bytes through stream vt+0x98. It writes a fifth byte only when that call returns 0x24, which neither the counting stream nor the memory stream does. The reader reads 4 bytes through vt+0x88 with the same 0x24 branch.
- `dbrf` writes `0` when its pointer is null, else `1`, a count byte and the ids as u32. The reader keeps at most 15 ids and reads all `n`.
- `uist` writes the 20-bit length (`[obj+6] & 0xfffff`) as u32, then the characters. The reader reads into a 0x400-byte stack buffer.
- The header version is read as u64 and refused above 1; the count is a u64 stored into a u16.

## Not checked

No bytes written by the game were decoded: the save containers are encrypted offline. The list in the table comes from one exe; the EA build was not compared. `.data1` code (packed on EA) was not scanned for types registered elsewhere; an unknown id decodes to an error.
