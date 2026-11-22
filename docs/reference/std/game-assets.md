---
title: Game assets
description: Read the game's own resources from its archives with swofty/game_assets.hpp - bytes by eaid:// URI, textures decoded to RGBA8 and UI image catalogue entries.
---

# Game assets

`swofty/game_assets.hpp` reads the game's own resources from the archives in the player's game folder: a resource's bytes by its `eaid://` URI, a texture or texture-array layer decoded to RGBA8, and the texture array and layer of a UI image catalogue entry. An addon builds what it needs from the player's own install at runtime, so its package carries no game files.

```cpp [native/src/my_plugin.cpp]
#include <thread>

#include <swofty/events.hpp>
#include <swofty/game_assets.hpp>
#include <swofty/overlay.hpp>
#include <swofty/storage.hpp>

namespace ga = swofty::game_assets;
namespace ov = swofty::overlay;

constexpr const char *LOGOS = "overrides.team_logos.driver_career_2024_team_select";

std::atomic<bool> ready{false};
ga::Image logo;

void build() // a thread of its own: every call blocks on file reads
{
    auto slot = ga::find(LOGOS, "redbull"); // the texture array and layer of the catalogue entry
    if (!slot) {
        swofty::log("[my-addon] no redbull logo in this game");
        return;
    }
    ga::Result r = ga::rgba(slot->texture, slot->layer, logo); // 148x148 straight RGBA8
    swofty::log("[my-addon] redbull logo: %s (%ux%u)", ga::name(r), logo.width, logo.height);
    ready = r == ga::Result::ok;
}

void on_frame(double, void *)
{
    static bool loaded = false;
    if (ready && !loaded && ov::available())
        loaded = ov::load_image_rgba("logo", logo.width, logo.height, logo.rgba.data()) == ov::Result::ok;
}

SWOFTY_ON_LOAD
{
    if (!ga::available()) {
        swofty::log("[my-addon] this core has no game assets library");
        return;
    }
    std::thread(build).detach();
    swofty::events::on_frame(on_frame);
}

SWOFTY_ADDON()
```

```text [swofty/loader.log]
[game-assets] 2025_asset_groups/ui_package/: 3193 archive(s), 117947 resource(s) indexed in 734 ms
[my-addon] redbull logo: ok (148x148)
[overlay] api: author.my-addon load_image_rgba logo (148x148) -> ok (slot 2)
```

- **Header:** `swofty/game_assets.hpp`, namespace `swofty::game_assets`
- **Table:** the core export `SwoftyCoreGameAssets` (`SwoftyGameAssets`), table version 1; not a `SwoftyApi` member, so any API version
- **Feature:** none
- **Thread:** a thread of your own; any thread works, and every call blocks on file reads

Every call returns a `swofty::game_assets::Result`: `ok`, `late`, `unavailable` or `invalid` (see [result codes](./#result-codes)), and `name(r)` turns it into a string. On a core without the export every call returns `unavailable` and `available()` returns `false`.

## Where it reads

A URI's package, `eaid://<package>/...`, names the folder `2025_asset_groups/<package>/` in the game folder. The first call for a package opens the index of every `.erp` archive in that folder and its subfolders and keeps which archive carries each URI, so later calls open one archive. The `ui_package` folder holds 3193 archives; indexing them took 0.5 to 0.7 s from a warm disk cache in the host test on Linux. An overlay archive the launcher layers on a game archive (the `resource_erps` of `swofty/assets.json`) wins over it.

```cpp
std::string font;
ga::read("eaid://ui_package/art/fonts/formula1-tv-600_bold_web.font", font); // first call: indexes ui_package
ga::read("eaid://ui_package/databases/overrides.uidb", font);              // one archive open
```

## available

```cpp
if (!ga::available())
    return; // a core without the library, or the game folder is unknown
```

## read and size

```cpp
std::string bytes;
ga::read("eaid://ui_package/art/fonts/formula1-tv-600_bold_web.font", bytes);    // every fragment, one after another
ga::read(slot->texture, bytes, 0);                                               // fragment 0 only
size_t n = 0;
ga::size("eaid://ui_package/databases/overrides.uidb", &n);                      // the size, nothing copied
```

`read` returns the decompressed bytes of one fragment, or of all of them with `all_fragments` (the default). A URI no archive carries, or a fragment past the last, returns `invalid`. The C call takes a buffer and returns `SWOFTY_TOO_SMALL` with `*len` set when it is NULL or too small; the wrapper asks for the size first.

## texture and rgba

```cpp
ga::Texture t{};
ga::texture(slot->texture, &t);  // t.format 70, t.width 148, t.height 148, t.levels 1, t.layers 69

ga::Image im;
ga::rgba(slot->texture, 12, im);     // layer 12, top level
ga::rgba(slot->texture, 12, im, 1);  // layer 12, level 1: 74x74
```

A texture resource's first fragment is its header (format at byte 8, width at 12, height at 16, level count at 24, layer count at 28) and its second the pixels, every level of layer 0, then every level of layer 1. `rgba` decodes one level of one layer to straight RGBA8 rows, top row first. It reads these engine formats:

| Format | Block |
|---|---|
| 52, 54 | BC1 |
| 55, 57 | BC3 |
| 65 | BC5: red and green, blue 0, alpha 255 |
| 69, 70 | BC7 |

Any other format, a layer or level past the last, or a pixel fragment shorter than the header says returns `invalid`.

## find and names

```cpp
auto slot = ga::find("overrides.team_logos.driver_career_2024_team_select", "redbull");
// slot->texture: eaid://ui_package/art/texture_arrays/overrides.uidb/overrides.team_logos.driver_career_2024_team_select.texarray.image
// slot->layer:   the entry's index in the catalogue

std::vector<std::string> all;
ga::names("overrides.team_logos.driver_career_2024_team_select", all); // every entry's Name, in layer order
```

A catalogue is the dotted name of a list in a UI database. Its first part names the database `eaid://ui_package/databases/<part>.uidb`, and each later part a list under `Contents`. The list's `TextureArrayName` gives the texture array and each entry's `Name` its layer. A catalogue the database lacks, a list without a texture array, or a name the list lacks returns `invalid` (`std::nullopt` from `find`). An entry without a `Name` gives an empty string in `names`.

## What the core logs

The core logs the index of each package it reads and every refused call, up to 32 refusals per session.

```text
[game-assets] 2025_asset_groups/ui_package/: 3193 archive(s), 117947 resource(s) indexed in 734 ms
[game-assets] catalogue_find overrides.team_logos.driver_career_2024_team_select nobody -> invalid: the catalogue has no entry of that name
[game-assets] read eaid://ui_package/art/fonts/missing.font -> invalid: eaid://ui_package/art/fonts/missing.font is in no archive of 2025_asset_groups/ui_package/
```

## Host tests

`detail::set_table(t)` makes the wrapper answer from a `SwoftyGameAssets` table of your own, so a host test can run the code that calls the library. `native/test/game_assets_test.cpp` runs the core's reader against synthetic archives and, with `F1_25_DIR` set, against the game files.

```sh
F1_25_DIR="/path/to/F1 25" native/test/run_tests.sh game_assets
```
