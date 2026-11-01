---
title: Garage art
description: A team's own garage and pit lane textures for one Grand Prix or Time Trial box, as a team.garage record with a folder of DDS or PNG files.
---

# Garage art

Give a team its own garage and pit lane art in one box. Classic Seasons 2020, for example, shows the Renault garage in 2020 Renault colours, while Alpine 2021 to 2024 and the stock 2025 box keep their garages.

A few terms used on this page:

- A **box** is one of the year tiles in Grand Prix or Time Trial. Its season build is a `content.package` record.
- A **garage record** is a `teams::Garage`: the box, the team it dresses and a folder of textures.
- A **stock texture** is a game resource such as `eaid://environment_package/core_assets/objects/textures/garage_liveries/garage_livery_renault_a_01_da.tif.image`. The game carries one copy in every track's archives.

## The header

<Example root="swofty-sdk" code-file="include/swofty/systems/teams.hpp">
<template #code>

```cpp
#include <swofty/registry.hpp>

namespace swofty::teams {

struct Garage {                                    // one team's garage and pit lane art in one box
    Ref<content::Package> package;                 // the box's content package
    Ref<Identity> team;                            // the team identity the package's team entry names
    std::string entry;                             // or the key of the package's team entry, when it names no identity
    Path textures;                                 // a folder of .dds and .png files
    std::string root = "eaid://environment_package/core_assets/objects/textures/";
    bool srgb = true;                              // a .png is encoded as sRGB BC7
};

inline Registry<Garage> garages{"team.garage", { .applies = Applies::boot }};

}
```

</template>
</Example>

A file's path below the folder, without its extension, is appended to `root`. The file `garage_liveries/garage_livery_renault_a_01_da.dds` replaces `garage_livery_renault_a_01_da.tif.image` and its view `garage_livery_renault_a_01_da.tif`. A `.dds` keeps its own format (BC1, BC3, BC5 or BC7) and every mip level it holds. A `.png` is encoded to BC7 with a mip chain down to 1x1.

## The Renault 2020 garage, in code

<Split>

Classic Seasons 2020 keeps its Renault under the package team entry `lotus`.

- **The record names the box.** The art reaches only the asset groups of the cars that package gives the team, so a session of any other box never loads it.
- **The folder mirrors the stock names.** The addon ships `garage/renault_2020/garage_liveries/garage_livery_renault_a_01_da.dds` and the other textures it replaces. The team slot token (`renault_a`) is part of each stock name.
- **The core builds one archive.** The textures are packed into `swofty/addons/swofty.garage/garage_<hash>.erp`, named by a hash of the record, the files and the root. An unchanged boot reuses it.
- **The game opens it as an archive of its own.** The path `2025_asset_groups/swofty_garage/<record>.erp` is redirected to the cached file, and the car's asset groups list it first.

<template #example>
<Example root="swofty.classic-seasons-2020" code-file="native/src/garage.cpp" data-file="registry/garage.json" files="swofty.json, garage/renault_2020/garage_liveries/garage_livery_renault_a_01_da.dds, garage/renault_2020/pitlane/pitstop_gantry_renault_a_d.dds">
<template #code>

```cpp
#include <swofty/registry.hpp>
#include <swofty/systems/content.hpp>
#include <swofty/systems/teams.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    teams::garages.add("renault_2020", {
        .package = content::packages["swofty.classic-seasons-2020/f1_2020"],
        .entry = "lotus",
        .textures = file("garage/renault_2020"),
    });
}

SWOFTY_ADDON()
```

</template>
<template #data>

```json
// registry/garage.json
{
  "team.garage": {
    "renault_2020": {
      "package": "swofty.classic-seasons-2020/f1_2020",
      "entry": "lotus",
      "textures": "garage/renault_2020"
    }
  }
}
```

</template>
</Example>
</template>
</Split>

```text
[registry] team.garage swofty.classic-seasons-2020/renault_2020: art of lotus in swofty.classic-seasons-2020/f1_2020 for 1 car group(s): asset_groups/cs_2020_vehicle_package/teams/lotus/wep/lotus.assetgroup
[registry] team.garage swofty.classic-seasons-2020/renault_2020: asset_groups/cs_2020_vehicle_package/teams/lotus/wep/lotus.assetgroup lists 2025_asset_groups/swofty_garage/swofty_classic_seasons_2020_renault_2020.erp
[registry] team.garage swofty.classic-seasons-2020/renault_2020: 53 texture file(s) of garage/renault_2020/ in garage_0f3a9c41d27b6e55.erp (112 MB, built in 9120 ms), 2025_asset_groups/swofty_garage/swofty_classic_seasons_2020_renault_2020.erp -> swofty/addons/swofty.garage/garage_0f3a9c41d27b6e55.erp, 1 of 1 group(s) extended
[assets] group asset_groups/cs_2020_vehicle_package/teams/lotus/wep/lotus.assetgroup: 2025_asset_groups/swofty_garage/swofty_classic_seasons_2020_renault_2020.erp added to its ERP list (now 3)
```

## Naming the team

A package team entry names a team in one of two ways. Set `team` when the entry's `team` is a `teams::Identity` record, and `entry` when the entry has only a `key`, as Classic Seasons' entries do. A package with no entry for the team refuses the record, and so does a team whose cars have no asset group.

## Extracting the stock files

`tools/garage_textures.py` writes the textures of an archive as `.dds` files named like the stock resources:

```sh
python3 tools/garage_textures.py "shader_package_2025/pack.erp" garage/renault_2020 --check
```

`--match` keeps the resources whose name contains a text, and `--root` changes the root. `--check` compares each texture's header and view with the ones the core builds.

## What is known

- **Built and tested:** the archive builder (names, headers, views, mip chains, the cache key) and the lowering to the cars' asset groups run in the host tests.
- **Not run in game:** whether the archive loads before the track's own copy. The resource map keeps the first copy that is loaded (`re/db/subsystems/assets.md`). The sizes and times in the log above are examples of its format.
- **Groups the core cannot extend:** a car group of the stock manifest. Only groups an addon declares, as Classic Seasons' cars do, take the archive.
