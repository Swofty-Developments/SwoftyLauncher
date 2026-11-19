# Profile save format

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
