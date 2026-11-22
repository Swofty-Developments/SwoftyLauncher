---
title: Sponsor boards
description: A box's own trackside sponsor boards, as a track.sponsors record with a folder of DDS or PNG files. Experimental, off by default.
---

# Sponsor boards

Give one Grand Prix or Time Trial box its own trackside sponsor boards, for example period boards in a 1950 season, while the stock 2025 and 2026 boxes keep the stock boards. Patching the `sp_` textures inside each track's archive changes every box; this record changes only its own.

- A **box** is one year tile in Grand Prix or Time Trial. Its season build is a `content.package` record.
- A **stock sponsor texture** is a shared resource such as `eaid://environment_package/core_assets/shared_textures/sponsors/signage/sp_american_express_d.tif.image`. Every track's root archive (`tracks/<slot>/<slot>_common.erp`) carries its own copy under the same name.

## The header

<Example root="swofty-sdk" code-file="include/swofty/systems/tracks.hpp">
<template #code>

```cpp
#include <swofty/registry.hpp>

namespace swofty::tracks {

struct Sponsors {                                  // one box's trackside sponsor boards
    Ref<content::Package> package;                 // the box's content package
    Path textures;                                 // a folder of .dds and .png files
    std::string root = "eaid://environment_package/core_assets/shared_textures/sponsors/";
    bool srgb = true;                              // a .png is encoded as sRGB BC7
};

inline Registry<Sponsors> sponsors{"track.sponsors", { .applies = Applies::boot }};

}
```

</template>
</Example>

```json
// registry/sponsors.json
{
  "track.sponsors": {
    "f1_1950_boards": {
      "package": "vmknolle.season-1950/f1_1950",
      "textures": "sponsors/f1_1950"
    }
  }
}
```

The file `sponsors/f1_1950/signage/sp_american_express_d.dds` replaces `signage/sp_american_express_d.tif.image` and its view below `root`. Files are named and encoded as for [garage art](./garage): a `.dds` keeps its format and mips, a `.png` becomes BC7 with a full mip chain.

Static boards are the `sp_*_d` textures under `signage/`. Leave out the ones that are not sponsor boards or are drawn another way: `marshal*`, `anim_*` (LED boards), `speed_limit*`, `garage_board*`, `dynamic_*`, `podium*`, `brake_*` (they live under `brake/`) and `*_next` / `*_prev`.

## How it applies

It takes the [garage art](./garage) route with no team: the core packs the folder into `swofty/addons/swofty.sponsors/sponsors_<hash>.erp`, redirects `2025_asset_groups/swofty_sponsors/<record>.erp` to it and lists it first in the asset groups of **every car the package names**, so only sessions of that box load it. As with garage art, only car groups an addon declares can take the archive, not groups of the stock manifest. The record applies at every track: there is no per-track filter.

```text
[registry] track.sponsors vmknolle.season-1950/f1_1950_boards: boards of vmknolle.season-1950/f1_1950 for 9 car group(s) of 9 car(s)
```

## What is known

- **Static only.** The lowering and the archive builder run in the host tests. Nothing about this record has been seen in game.
- **Off by default:** the core leaves `track.sponsors` records unapplied and logs one line saying so. `SWOFTY_BOX_SPONSORS=1` applies them. `SWOFTY_GARAGE_DESC=0` affects these archives too.
- **The stock surfaces are streamed.** A stock `sp_*_d` signage surface has 3 fragments: the 40 byte header, an inline mip chain (1024x256, 7 levels) and a streaming descriptor for a `.tif.2025.mipmaps` file with 2 streamed levels up to 4096 (Bahrain's `bahrain_common.erp`: 44 `sp_` surfaces; `sp_american_express_d` has fragments of 40, 7391 and 128 packed bytes). That is the class of texture whose stock streamed copy crashed the texture stream for garage art in 1.7.8 to 1.7.10 (`Ne_Crt_Memcpy`, see [garage art](./garage#what-is-known)). The archive's surfaces carry the descriptor the garage builder writes, which lists no streamed level; the core ships no `.mipmaps` file, so a texture above the inline chain's size is not streamed.
- **Open: which copy wins.** Whether the car group's archive wins over the track archive's copy of the same name is not shown for sponsor boards. Garage textures also have a copy in every track's archives and the 2020 Renault garage showed in game, so it is expected to win.
- **No per-track filter:** a `tracks` field is not offered, since the archive is attached to the cars, not to a track.
