---
title: Game memory writes
description: Write a value into a data binding of F1 25 (a k_ or g_ table) with the core's checks and restore, and allocate through the game's own allocator, with swofty/game_mem.hpp.
---

# Game memory writes

`swofty/game_mem.hpp` writes values into the game's data bindings and allocates memory from the game's allocators, so an addon never changes page protection or calls an allocator's vtable itself. It is experimental: nothing in it has run in game yet.

```cpp [native/src/my_plugin.cpp]
#include <swofty/game.hpp>
#include <swofty/game_mem.hpp>
#include <swofty/mixin.hpp>

namespace gm = swofty::game_mem;
using swofty::game::k_Career_RnDDeptColourTable; // uint32_t[6], ABGR per R&D department

SWOFTY_ON_LOAD
{
    // colour [4] for a fifth department, only while [4] is still the stock white
    gm::Result r = gm::write(k_Career_RnDDeptColourTable, 4, 0xff40b0e0u, 0xffffffffu);
    if (r != gm::Result::ok)
        swofty::log("[my-addon] colour [4] left stock (%s)", gm::name(r));
}

SWOFTY_ADDON()
```

- **Header:** `swofty/game_mem.hpp`, namespace `swofty::game_mem`
- **Table:** the core export `SwoftyCoreGameMem` (`SwoftyGameMem`, table version 1); `SWOFTY_API_VERSION` is unchanged. Without it every call answers `Result::unavailable` and `alloc` gives `nullptr`
- **Feature:** none
- **Thread:** any

## write

```cpp
gm::write(binding, index, value);         // element index of a data binding
gm::write(binding, index, value, expect); // only while that element is expect now
gm::write_bytes(id, size, offset, bytes, n, expect); // raw form: n bytes at offset of a binding of size bytes
```

`binding` is a data binding of the [Game API](/reference/game/) (`swofty::data<"k_...", T>`). The value's type must be the element type of `T` (`uint32_t` for `uint32_t[6]`); a binding that is not an array has one element, 0. An index past the end is `Result::invalid` without a call.

The core checks, in this order, and logs the reason of a refusal to `loader.log` under `[game-mem]`:

1. `id` is a data binding of the binding set and resolves on this build (`Result::unavailable` otherwise).
2. The size the header passes (`sizeof(T)`) is the size every earlier write of `id` declared, and the bytes fit in it.
3. No other addon holds one of the bytes (the first addon to write a byte holds it until it restores).
4. The whole binding lies in one section of the game exe that is not code. Code is changed with `SWOFTY_PATCH`, never written.
5. With `expect`, the bytes there are `expect` now.

Then it makes the pages writable for the copy only and puts their protection back. On your first write of a range it keeps the bytes that were there.

## restore

```cpp
gm::restore(k_Career_RnDDeptColourTable); // the game's bytes back where this addon wrote into the binding
gm::restore_all();                        // in every binding this addon wrote
```

The owner of a write is the addon whose code called it. The core never unloads an addon, so restore when your feature turns off: when its setting is switched off, or on `career.unloaded` for a change that only belongs in a career. Writing the same range again keeps the bytes saved on the first write, so `restore` always gives the game's own bytes back.

## alloc and free

```cpp
void *buf = gm::alloc(0x80, 8);              // from the game's default allocator (g_Ne_DefaultAllocator)
auto *rows = gm::alloc_array<Row>(n, owner_alloc); // from an allocator object a game vector holds
gm::free(buf);                              // back to the default allocator
gm::free(rows, owner_alloc);
```

A game allocator object is called through its vtable: `+0x18` alloc(self, size, align) and `+0x38` free(self, p). The core first checks that the object is a game object and that the slot points into the exe image, that `size` is above 0 and at most 1 GB and that `align` is a power of two up to 4096. Use the allocator a game container already holds when you grow it, so its own grow and free paths stay consistent. Memory from `alloc` is the game's: hand it to game objects that free it themselves, or give it back with `free` on the same allocator.

## From C

```c
static const SwoftyGameMem *game_mem(void)
{
    HMODULE core = GetModuleHandleW(L"swofty_core.dll");
    SwoftyCoreGameMemFn get = core ? (SwoftyCoreGameMemFn)(void *)GetProcAddress(core, "SwoftyCoreGameMem") : NULL;
    const SwoftyGameMem *t = get ? get() : NULL;
    return t && t->version >= 1 ? t : NULL;
}
```

`write_data(id, size, offset, bytes, n, expect)`, `restore_data(id)` (`NULL`: every binding), `alloc(allocator, size, align)` and `free(allocator, p)` (`NULL` allocator: the default one) behave as above and return the shared [result codes](./#result-codes).
