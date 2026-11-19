---
title: Save format
description: The profile save's cipher and the byte layout of the game's property-bag stream, the stream inside every decoded save container, with every value type, and the Rust decoder and encoder that read and write it.
---

# Save format

F1 25's profile save is a tree of **bags**. The game writes a bag with `Game_SaveData_WriteBag` (0x1439340c0) and reads it with `Ne_BagCodec_ReadBag` (0x143934620). A `.bag` file in the plain save store is exactly one root container's bytes as that writer emits them: decoded, not compressed, not encrypted. The encrypted outer file (data file and 416-byte index) is described under [Cipher](#cipher).

All integers are little-endian. All offsets below are Steam build 1.0.141.7865 addresses.

## Stream

```text
bag      := version:u64  count:u64  entry[count]
entry    := key_len:u8  key:u8[key_len]  type_id:u32  value
list     := version:u64  count:u64  item[count]
item     := type_id:u32  value
value    := pbag -> bag | plst -> list | scalar by type id
```

- `version` is 1. The reader refuses a value above 1; the decoder keeps 0 and 1 as read.
- `count` is stored as 8 bytes. The bag and list headers hold a `u16` count, so a count above 65535 does not fit.
- A key is the entry's name. The game stores each entry under the FNV-1a64 hash of the name and writes the name back from its string table. The writer's name buffer holds 63 characters.
- A type id is four ASCII characters, read as a `u32`: the bytes in the stream spell the name (`pbag` is `70 62 61 67`).
- A bag's entries and a list's items are written in order. A list has no keys. An entry in a bag may be another bag or list; the nested bytes follow the type id with no length.
- The game treats a root container's `.bag` as an entry value: the save root writes `name`, the type id `pbag`, then these bytes.

## Value types

The type table is registered by `0x1439532f0` (30 types) and `0x14394aa50` (3 types). The wire form of each is the body of its vtable `+0x78` writer, read back by the `+0x80` reader.

| Id | Body | Size |
|---|---|---|
| `pbag` | a nested bag | variable |
| `plst` | a nested list | variable |
| `si08`, `ui08` | 1 byte | 1 |
| `si16`, `ui16` | 2 bytes | 2 |
| `si32`, `ui32`, `fp32`, `rgba` | 4 bytes (`rgba` is 4 raw bytes) | 4 |
| `si64`, `ui64`, `fp64`, `auid` | 8 bytes | 8 |
| `bool` | 1 byte, 0 or 1 | 1 |
| `vec2` | 2 x `fp32` | 8 |
| `vec3` | 3 x `fp32` | 12 |
| `vec4`, `quat` | 4 x `fp32` | 16 |
| `mat4` | 16 x `fp32` | 64 |
| `shnm`, `woid`, `matp` | 16 raw bytes | 16 |
| `st32`, `st64`, `s128` | `len:u8`, then `len` characters | 1 + len |
| `s256`, `sstr` | `len:u16`, then `len` characters | 2 + len |
| `dstr`, `uist` | `len:u32`, then `len` characters | 4 + len |
| `bffr` | `len:u32`, then `len` bytes | 4 + len |
| `blwt` | 1 x `fp32` (the weight) | 4 |
| `dbrf` | `flag:u8`; when 1, `n:u8` then `n` x `u32` | 1 or 3 + 4n |

Fixed-size types are the value's raw bytes: the writer copies the type's `vt+0x88` size from the value. The game reads `st32`, `st64`, `s128` and `s256` into a buffer of 32, 64, 128 and 256 bytes: a longer stored string is cut to the buffer less its terminator. The decoder keeps every stored byte.

`blwt` has a 16-byte in-memory value; the stream carries its first 4 bytes. `dbrf` with `flag` 0 is an absent value. The game keeps 15 ids of a `dbrf`. `uist` is read into a 1024-byte buffer.

## Proof

| Fact | How |
|---|---|
| Header, count, entry shape, nested bag and list | Disassembly of the writer 0x1439340c0, the list writer 0x143934410 and the readers 0x143934620 and 0x143935010 |
| The 33 type ids and the vtable of each | Byte scan of the exe for vtables whose slot `+0x10` is `mov eax, <id>; ret`, matched to the two registration functions |
| Fixed-size layouts | The shared writer body (`vt+0x30` value, `vt+0x88` size, stream `vt+0x48`) and the size each vtable returns |
| `st32` to `s256`, `sstr`, `dstr`, `bffr` | Disassembly of each writer and of the `st32` reader (cut at `0x1f`) |
| `blwt`, `uist`, `dbrf` | Disassembly of the writer and reader; `blwt` reaches the stream's `vt+0x98`, which is the plain write in the counting and memory streams |

The per-type vtable and function addresses are in `re/research/save-bag-types.md`; the stream functions are the records `Ne_BagCodec_WriteList` and `Ne_BagCodec_ReadList` and the existing `Game_SaveData_WriteBag`.

None of these types has been checked against bytes the game wrote. The coordinator's round trip over real dumps does that.

## Decoder and encoder

The Rust module is `swofty_core::saves::bag`.

```rust
let bag = swofty_core::saves::bag::decode(&bytes)?;
let same = swofty_core::saves::bag::encode(&bag)?;
assert_eq!(same, bytes);
```

- `decode` reads one bag and fails on unread bytes; `decode_prefix` returns how many bytes it used.
- `Limits` bounds nesting (64) and value count (8,000,000). A count that the remaining bytes cannot hold fails before any allocation.
- Errors are `DecodeError` and `EncodeError`; the decoder does not panic on any input.
- `bool` other than 0 or 1 and a `dbrf` flag other than 0 or 1 are `NonCanonical`: the game never writes them.
- `bag_to_json` and `bag_from_json` go through the same tree. Each value is `{"type": "<id>", "value": ...}`. A float that JSON cannot carry exactly is `{"bits": n}`; text that is not UTF-8 is `{"hex": "..."}`.

`swofty saves decode-bag <file.bag> [--json]` prints the tree of a file.

## Cipher

The profile save is two files, the data file and the 416-byte index. Both outer layers are decoded and the data file round-trips byte for byte. The code is in `packages/core/src/games/f1_25/saves/codec/`.

### Data file

```text
offset  size  field
0       4     magic 65 ef 00 8a
4       4     u32 LE, file size xor 0xc3615f4e
8       32    HMAC-SHA256, xored with a 12-byte cycle
40      16    IV, xored with the same cycle
56      n*16  AES-256-CBC body, PKCS7 padded
```

- The HMAC covers the unobfuscated IV followed by the ciphertext. The AES key and the HMAC key are the same 32 bytes.
- The key is a constant: the first 8 outputs, as u32 LE, of an MT19937 seeded with `0x08086283` (initialised with the `69069` multiplier, not the 1812433253 one).
- The 12-byte cycle is a constant at RVA `0x5a5db02` (Steam build `d873a1c8`).
- The IV is random per save.
- The plain body is a 16-byte stream header (`05 00 00 00 10 00 00 00 04 00 00 00 01 08 65 15`) and then one zstd frame, written without a content size or checksum (window 2 MiB).
- The decompressed stream starts with the value 1 as 8 bytes and the entry count as 8 bytes, then the first key `save_game`.
- HMAC and CBC run through the Windows CryptoAPI (`0x1439560c0`, `0x1439550f0`), so a scan for SHA or AES constants finds nothing at those sites.

Sealing a decoded frame with its original IV reproduces the original file. A re-compressed stream gives a valid file with a different frame.

### Index (`QKRHMYXE`)

- The 416 bytes are AES-256-ECB. The key is a second constant: 64 digits from an MT19937 seeded with `0x178b102d` (the same `69069` initialisation), each output taken modulo 15 (digits `0` to `E`, hex-parsed into 32 bytes).
- The constant 64-byte prefix is the ECB ciphertext of a UTF-16 display name.
- The plaintext is 416 bytes (it does not depend on the data file):
  - `+0x00`: the UTF-16 display name, up to 33 characters, NUL-terminated within 0x44 bytes.
  - `+0x44` to `+0x17f`: filler from the MSVC LCG (`x * 0x343fd + 0x269ec3`, one run of u32s over the whole buffer, from a random seed).
  - `+0x100` and `+0x104`: u16 thumbnail offset and size, 0 for a profile.
  - `+0x180`: the checksum. `+0x184`: `numFiles` (1). `+0x188`: the name hash of the data file's obfuscated name (a constant for the profile save). `+0x18c` and `+0x190`: 0. `+0x194` to `+0x19f`: a second filler run.
- The checksum is a standard CRC32 (polynomial 0xedb88320, table at RVA `0x58904d0`) over bytes `[0, 0x180)` followed by `[0x184, 0x1a0)`. Init is the complement of the constant seed `0xaf6d87d2` (zlib `crc32(data, seed)` style) and the result is complemented. It matches all 6 real indexes.
- That is why the index changes on every save: the filler seed is random. Nothing in it depends on the data file.

### Where each piece lives (Steam build `d873a1c8`)

| What | Where |
|---|---|
| Data file writer, header and MAC | `Ne_SaveSystem2Win_ProcessSerialiserActions` `0x14414cdd0`, `0x14414d483` to `0x14414db2e` |
| Data key and IV generation | `0x14414d483` (key), `0x143954ef0` (IV) |
| Index cipher objects | vtables `0x145a5f1f0` (encrypt), `0x145a5f230` (decrypt), tables `0x145a3e120` |
| Index checksum | `0x144151937` to `0x144151a2a`, table `0x1458904d0` |
| Index key | `Game_Read_GameConfig` `0x1407f9f88`, key setter `0x143f427c0`, save system constructor `0x144137460` (stores it at `+0x2d0`) |
