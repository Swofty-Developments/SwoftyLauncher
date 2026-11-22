# R&D add-on: memory writes outside the mixin macros, and what each needs (2026-11-21)

Source: the season R&D add-on (`rnd_departments.dll`, source files `rnd_departments.cpp`, `rnd_capacity.cpp`,
`rnd_fifth_pie.cpp`, `rnd_dept_tags.cpp`, `rnd_upgrade_types.cpp`), inventory A1-A21 of its own memory changes. Every
code site of that add-on is already a `SWOFTY_SITE` / `SWOFTY_REDIRECT` / `SWOFTY_WRAP`; no detour, no code-byte
write. This note sorts the rest by the rule "writes to objects the handler receives are normal handler work; a
write to the game's static data or a call into game code without a binding needs an API or a binding".

## Covered by `swofty/game_mem.hpp` (this PR)

| # | What the add-on does now | With the API |
|---|---|---|
| A1 | `VirtualProtect` + store into `k_Career_RnDDeptColourTable` [4], [5] (white -> Tyres / Suspension colour) at load, only while [0..3] are stock and [i] is white | `game_mem::write(k_Career_RnDDeptColourTable, i, colour, 0xffffffffu)`; `game_mem::restore` when the feature turns off. The same call serves `k_Career_RnDCardColourTable` (`uint32_t[6]`, [4] white). Note: [5] is the stock unknown-department colour (verify V0193): three readers map an unmatched dept hash to 5 |
| A4 | Own call through the allocator vtable (+0x18 alloc, +0x38 free) of the builder's allocator (+0x278 / +0x298) and `g_Ne_DefaultAllocator` | `game_mem::alloc(size, align, allocator)` / `game_mem::free(p, allocator)`; the core checks the object and the slot |
| A2 | Card vector realloc: new buffer from the game allocator, 6 stock slots copied, begin / cap replaced | the buffer from `game_mem::alloc(.., builder allocator)`, the old one back with `game_mem::free`; the vector writes stay (object the handler receives) |
| A5 | NeStr release: atomic refcount (+4) decrement, CAS 0xffff, free through `g_Ne_DefaultAllocator` | the free through `game_mem::free(s)`; the refcount step stays (the string is one the add-on made with `Ne_Make_StrFromCStr`). Better later: a binding of the game's own NeStr release, if the KB finds one, so the refcount protocol is not copied |
| A18 | Upgrade-type metadata holder: allocates cap x 0x20 from `g_Ne_DefaultAllocator` when the game's reserve failed, writes +0x68 / +0x70 / +0x78 | `game_mem::alloc_array` / `game_mem::alloc(cap * 0x20, 4)`; the holder writes stay (object reached from the site's frame) |

## Fine as they are (handler work on what the handler receives)

All of these run inside a `SWOFTY_SITE` handler and write the frame (`rsp` + offset) or an object the hooked function
is building or was given. That is what a site handler is for; no API is needed.

| # | Write | Why it is handler work |
|---|---|---|
| A3 | card pointers into slots 6.. of the card vector, end +0x288 | the vector of the builder the site receives |
| A6 | `{stock vtable, 0, 0, 0}` handles at obj+0x80+slot*0x220 of the pie overview | initialises slots of the object under construction with the game's own vtable |
| A7 | own footer (magic `RNDDEPT6`) at obj+0xd40 | bytes past the stock size of a buffer the add-on enlarged at `site_Career_RnDPieBufAlloc` |
| A8 | tile count `[rsp+0x28]` (phase 2) | frame of the hooked function |
| A9 | Upgrades owner cache U+0x1aa8 = 0x3fff0000 | object passed to the deserialiser end site; the game's own "not cached" value |
| A10 | results `[rsp+0xdf0]`, count `[rsp+0x19f0]` (phase 2) | frame |
| A11 | compare dept byte `[rsp+0x108]` | frame |
| A12, A16, A17 | Format argument `[rsp+0x20]` (card art, wheel icon, tag frames) | frame (outgoing argument of the call at the site) |
| A13 | team modifier list entry +4 copied from dept 2/1 to 4/5 | the list `Career_Get_TeamModifiers` returned to the site |
| A14 | publisher dept list `[rsp+0xa0+n]` (cap 8) | frame |
| A15 | perf `[rsp+0x158]` / potential `[rsp+0x154]` | frame |

## Need a binding, proposed (not in this PR)

The add-on reads code bytes to decide a route. Reading is not a patch, but it re-checks what a binding signature
should guarantee, and it reads bytes the launcher may have hooked (the add-on accepts `E9 rel32 + 90` for that).
Each becomes a binding the core resolves only when the bytes match, so the add-on's check goes away:

| # | Read now | Proposed binding |
|---|---|---|
| A19 | `41 83 FD 04 0F 87` (cmp r13d,4; ja rel32) at `site_Career_RnDPiePopulateSlotCap`, gate == cap + 10 | extend the record of `site_Career_RnDPiePopulateSlotCap` (signature over these bytes, gate offset in notes), or a `SWOFTY_PATCH`-able site of the immediate if the add-on wants the cap raised |
| A20 | lea `48 8D 05 rel32` at NextCode, `4C 8D 2D` before TileCountGuards, table `00 01 02 03 04 05`, TileCountBody+0x114 `89 CA C4 E1 EA 2A C2` (phase 2) | a data binding of the stock dept code table (riprel resolve from the NextCode lea, `kind: data`, `uint8_t[6]`), and the loop sites as site bindings; the add-on stops decoding rel32 itself |
| A21 | department_performance loop head `48 8D 1C 04 48 81 C3 A0 00 00 00 48 8D AC 24 A0 00 00 00 EB` (or E9 + rel32 + 6x90), resume = LoopEnd + 0x13 (phase 2) | a jump-target site binding for the resume address (`site_Career_RnDPerfDeptLoopResume`), signature over the loop head |

Each needs the body read and the evidence written down on the Steam 26566308 exe before it is an L2 record.

## Open (for the maintainers)

- `write_data` refuses code sections, but the binding's bytes are still in the exe image. If the server's code
  challenge hashes read-only data sections too, these writes need the same skip list as the code patches
  (`tools/patch_sites.py`); the core does not add them there yet.
- The core does not know a data binding's type size; it takes the size the addon's header declares and makes every
  later write of the binding agree with it. A `size` field in `bindings.json` (from the record's `row.type`) would let
  the core check it against the KB.
