---
title: UI and strings
description: Screens, tiles, texture slices, routes, options, HUD widgets and strings in code, with era boxes, the UI events and the view-model operations.
---

# UI and strings

With the UI system you change what the player sees in the menus and on the HUD: tiles and boxes, the images in the game's texture arrays, your own screens and routes, options, HUD widgets and text. Classic Seasons 2017, for example, adds an F1 2017 box to the Grand Prix picker and a 2017 Toro Rosso logo to the driver select screen.

A **texture array** is one of the game's image sheets, made of named **slices**. A **route** is the game's link from a press to the screen it opens. A **view model** is the data a screen binds to, by path.

::: info In 1.6.0
The header `swofty/systems/ui.hpp` has `ui::strings` (`lang.string`) and `ui::addon_options` (`addon.option`). Screens, tiles, texture slices, routes, options and HUD widgets ship later; those sections describe that design.
:::

## The header

<Split>

`swofty/systems/ui.hpp` has seven registries:

- `ui::texture_slices`: one named slice of a texture array (`ui.texture_slice`);
- `ui::tiles`: a home, career, F1 World or car-category tile (`ui.tile`);
- `ui::screens`: an addon screen (`ui.screen`);
- `ui::routes`: a front-end route (`ui.route`);
- `ui::options`: a game option and its restriction tags (`ui.option`);
- `ui::hud_widgets`: an addon HUD element (`hud.widget`, the design-only alias of `hud.element` below);
- `ui::strings`: one text key in every language (`lang.string`).

Grand Prix and Time Trial era boxes are `content::era_boxes`, in `swofty/systems/content.hpp` (see [Seasons and GP/TT content](/std/content)).

<template #example>
<Example root="swofty-sdk" code-file="include/swofty/systems/ui.hpp">
<template #code>

```cpp
#pragma once
#include <swofty/registry.hpp>
#include <swofty/events.hpp>

namespace swofty::ui {

struct Route;

struct TextureSlice {
    std::string array;                         // the texture array: "overrides.teams.driver_select_team_logo"
    std::string name;                          // the slice name the UI asks for: "cs_f1_2017"
    Path image;                                // png or svg; the core makes the BC7 slice and its mips
};
inline Registry<TextureSlice> texture_slices{"ui.texture_slice", { .applies = Applies::boot }};   // (every archive that carries the array)

enum class Place { home, career, f1_world, car_category };

struct Tile {
    Place place;
    Text title;
    Ref<TextureSlice> image;
    Ref<Route> route;                          // what a press opens
    events::Selector gate;                     // shown only where it matches; empty: always
};
inline Registry<Tile> tiles{"ui.tile", { .applies = Applies::read }};

struct Screen {
    Path file;                                 // the .ui file
    std::string flow;                          // the flow archive it joins: "flow_frontend"
    std::string anchor;                        // where it opens from
    std::string view_model;                    // the view-model path it binds
};
inline Registry<Screen> screens{"ui.screen", { .applies = Applies::boot }};

struct Route {
    std::string file;                          // routes_frontend_general.uir, ...
    std::string clone_of;                      // the route group it copies
    Ref<Screen> opens;
};
inline Registry<Route> routes{"ui.route", { .applies = Applies::boot }};

struct Option {
    std::string id;                            // options_system option
    std::vector<std::string> restriction_tags; // where it shows: "gamesetup_f1", ...
};
inline Registry<Option> options{"ui.option", { .applies = Applies::boot }};

struct HudWidget {
    std::string view_model;                    // the VM path it binds
    Path tmpl;                                 // its template
};
inline Registry<HudWidget> hud_widgets{"hud.widget", { .applies = Applies::session }};

struct String {
    std::map<std::string, std::string> text;   // per language; "*" for every language
    events::Selector within;                   // a career, a content, a mode; empty: everywhere
};
inline Registry<String> strings{"lang.string", { .applies = Applies::read }};   // the id is the lng key; read from the core's string table

namespace events {

struct EraTiles {                              // site_FeTile_EraTilesBuilt
    static constexpr const char *point = "ui.era.tiles";
    static constexpr uint32_t version = 2;     // v1 had only `list`; v1 readers still work
    uint32_t size, payload_version;
    void *list;                                // opaque, as in v1
    const SwoftyTileList *tiles_table;         // v2: the C table; tiles() wraps it, null on a v1 payload
    TileList tiles() const;                    // count(), get(i), remove(i), move(from, to), insert(at, spec), append(spec), iterable
    Result remove(size_t index);
    Result insert(size_t at, const TileSpec &spec);   // {id, title, route, image, literal}
};

struct VmResolve {                             // Vm_Resolve_RegistryNode
    static constexpr const char *point = "ui.vm.resolve";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    std::string_view path;                     // the view-model path the screen asks for
    void *node;                                // before handlers may supply their own node
};

struct LangLookup {                            // Loc_LangSet_Find
    static constexpr const char *point = "lang.lookup";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    std::string_view key, language;
    std::string_view text;                     // before handlers may supply the text
};

// also: HomeTiles (ui.home.tiles), CareerContentTiles (ui.career_content.tiles), CarCategoryTiles
// (ui.car_category.tiles): the same v2 payload as EraTiles; TrackSelectBuild (ui.track_select.build): v2 with a
// read-only `tracks()`; ScreenBuilt (ui.screen.built), RouteOpen (ui.route.open), MfdPages (hud.mfd.pages)

} // namespace events

Result set_text(std::string_view vm_path, std::string_view text);   // the Vm_Set_* family
Result set_bool(std::string_view vm_path, bool value);
Result set_ref(std::string_view vm_path, std::string_view target);

} // namespace swofty::ui
```

</template>
</Example>
</template>
</Split>

## An era box and a team logo

<Split>

Classic Seasons 2017 shows F1 2017 on the Grand Prix and Time Trial picker, with its own box art. It adds Toro Rosso's 2017 logo for the driver select screen, under the name its team record asks for.

Each image is one slice in a named texture array. Classic Seasons, fanfreluche's cars and ByDaniX's F1 2012 each ship a whole driver-select logo array, and whole arrays overwrite each other. As slices they all apply: each addon adds or changes its own names.

<template #example>
<Example root="swofty.classic-seasons-2017" code-file="native/src/ui.cpp" data-file="registry/ui.json" files="swofty.json, native/CMakeLists.txt, art/era/f1_2017.png, art/logos/toro_rosso.png, lang/strings.json">
<template #code>

```cpp
#include <swofty/registry.hpp>
#include <swofty/events.hpp>
#include <swofty/systems/ui.hpp>
#include <swofty/systems/content.hpp>

using namespace swofty;

SWOFTY_REGISTER
{
    ui::texture_slices.add("era_f1_2017", {
        .array = "overrides.home_screen.tiles_solo_6x6",
        .name = "cs_f1_2017",
        .image = file("art/era/f1_2017.png"),
    });

    content::era_boxes.add("f1_2017", {
        .package = content::packages["swofty.classic-seasons-2017/f1_2017"],
        .title = "lng_cs_era_2017",
        .image = "#overrides.home_screen.tiles_solo_6x6[cs_f1_2017]",   // the slice above
        .order = 2017,
    });

    ui::texture_slices.add("logo_toro_rosso", {
        .array = "overrides.teams.driver_select_team_logo",
        .name = "cs17_toro_rosso",                         // the name the 2017 Toro Rosso team record shows
        .image = file("art/logos/toro_rosso.png"),
    });

    ui::strings.add("lng_cs_era_2017", { .text = { { "*", "F1 2017" } } });
}

SWOFTY_ON_LOAD
{
    events::on<ui::events::EraTiles>().after([](const ui::events::EraTiles &, const events::Context &) {
        swofty::log("[cs-2017] era picker built");
    });
}

SWOFTY_ADDON()
```

</template>
<template #data>

```json
{
  "ui.texture_slice": {
    "era_f1_2017":     { "array": "overrides.home_screen.tiles_solo_6x6", "name": "cs_f1_2017", "image": "art/era/f1_2017.png" },
    "logo_toro_rosso": { "array": "overrides.teams.driver_select_team_logo", "name": "cs17_toro_rosso",
                         "image": "art/logos/toro_rosso.png" }
  },
  "era.box": {
    "f1_2017": { "package": "swofty.classic-seasons-2017/f1_2017", "title": "lng_cs_era_2017",
                 "image": "#overrides.home_screen.tiles_solo_6x6[cs_f1_2017]", "order": 2017 }
  },
  "lang.string": {
    "lng_cs_era_2017": { "text": { "*": "F1 2017" } }
  }
}
```

</template>
</Example>
</template>
</Split>

```text
[registry] register: swofty.classic-seasons-2017 (cs2017_ui.dll) 2 ui.texture_slice, 1 era.box, 1 lang.string
           (early stage, 0.3 ms)
[registry] applied at boot: ui.texture_slice 2 (2 arrays grown in 6 archives, as overlay archives); era.box 1 and lang.string 1 on read
[registry] ui.texture_slice overrides.teams.driver_select_team_logo: 34 slices from 3 addons (swofty.classic-seasons-2017,
           fanfreluche.cars, bydanix.f1-2012), 0 contested
[events] ui.era.tiles: 1 after handler (swofty.classic-seasons-2017)
[cs-2017] era picker built
```

### Editing the tile lists (payload version 2)

The five list events carried only an opaque `list` pointer into the core. Payload version 2 adds a versioned C table next to it (`SwoftyTileList` in `swofty_api.h`: `size`, `version`, `self`, `count`, `get`, `remove`, `move`, `insert`) with C++ helpers in `ui.hpp` (`ev.tiles()`, `ev.remove(i)`, `ev.insert(at, spec)`). `list` stays where it was, so a v1 reader still works. Read the table only when `payload_version >= 2` and `size` covers it; the helpers do that check and return `Result::unavailable` otherwise, and for any function a core leaves null.

| Event | The table's list | `get` | `remove`, `move`, `insert` |
|---|---|---|---|
| `ui.home.tiles` | the `ui.tile` records of place `home` about to be added (cap: 255 minus the tiles already on the screen) | yes | yes |
| `ui.career_content.tiles` | the records of place `career` (cap: the slots the content tiles leave) | yes | yes |
| `ui.car_category.tiles` | the records of place `car_category` (cap: the tab's free slots) | yes | yes |
| `ui.era.tiles` | the era picker's tile range | no: count only | no |
| `ui.track_select.build` | `SwoftyTrackList` | not on this core: every function is null | no |

An edit reaches the step that adds the tiles: home, career content and the car category tab fire the event after reading the records and before adding any, so a removed record tile is not added and an inserted one is. The core then settles the list as before, one tile per id and no more than the cap, and logs what it dropped. Each function returns `Result::invalid` for an index out of range, a spec with no id or title (or no route on these three places), an id the list already holds, or an insert into a full list. Strings from `get` are valid only during the handler call and until the next edit; copy what you keep. An inserted tile has no gate and its title is a lang key unless `literal` is set.

The home list holds record tiles only. The stock tiles the game builds (Career, Grand Prix, the settings row) are game objects with button indices and view-model nodes behind them, so they are neither listed nor removable. The era picker's entries are game handles whose layout is not proven, so that table counts and nothing more. `ui.track_select.build` fires after the game has filled the screen's list, whose element layout is not proven either, so `tracks()` is empty on this core and remove and insert are not part of the table; a core that proves the layout fills `count` and `get` (track ids as the track registry names them) in the same table version.

```cpp
events::on<ui::events::HomeTiles>().before([](ui::events::HomeTiles &e, const events::Context &) {
    if (e.tiles().count() > 0)
        e.remove(0);
    e.insert(0, {.id = "my.tile", .title = "my_tile_title", .route = "my.route"});
});
```

The core builds the slices into overlay archives that it adds to the game's asset manifest at boot. The data version is the same records. Only the after handler needs code.

## Tiles, screens and routes

```cpp
auto settings = ui::screens.add("settings", {
    .file = file("ui/settings.ui"), .flow = "flow_frontend", .anchor = "addon_settings", .view_model = "mod.settings",
});
auto open = ui::routes.add("open_settings", { .file = "routes_frontend_general.uir", .clone_of = "options", .opens = settings });

ui::tiles.add("settings_tile", {
    .place = ui::Place::home,
    .title = "lng_mod_settings",
    .image = ui::texture_slices["mod.settings/tile"],
    .route = open,
    .gate = events::mode(events::Mode::driver_career),
});
```

## Options and HUD widgets

```cpp
// show the option in an F1 1990 weekend, and drop it from the multiplayer lobby
ui::options.layer(ui::options["game/parc_ferme_rules"], [](ui::Option &o) {
    o.restriction_tags.push_back("gamesetup_f1_1990");
    std::erase(o.restriction_tags, "Unranked");
});

ui::hud_widgets.add("tyre_letters", { .view_model = "hud.f1990.tyre_letter", .tmpl = file("ui/tyre_letter.ui") });
```

Every option of the game's `options_system` (212 of them, in the `game`, `career`, `ea_trax` and `multiplayer` sections) has a stock record, `game/<option name>`, whose `id` is the option and whose `restriction_tags` are the option's tags as the game holds them. An option shows where the running context's tag of the same kind is in its list. A tag belongs to one kind, the folder of `options_system.context` it is listed in: `game_setup_mode` (`gamesetup_f1`, `gamesetup_f2`, `gamesetup_f2_21`, `gamesetup_f1_esports`, `gamesetup_f1w`, `gamesetup_classic`, `gamesetup_multiclass`), `game_state` (`FrontEnd`, `MainMenu`, `PreSession`, `InSession`, `InLobby` and their `_Locked` forms), `multiplayer_context` (`Unranked`, `UnrankedTemplate`, `UnrankedLAN`, `Ranked`, `Event`, `OnlineC`, `None`), `game_mode` and the general state folders (`splitscreen_visible`, `hide_for_product_pro`, ...). At boot the core compares each layered stock record with the game's list and writes the difference into the option's tag rows in the game database: the tags a layer added are appended to the row of their kind, and the tags it removed are taken out. Layers apply in load order, so a later layer sees what an earlier one did. The boot line is `[registry] applied at boot: ui.option 1 (1 tag added)`.

- An addon record of its own (`mine/parc`, with `id` set to a stock option) only adds its tags.
- `id` is the option name, `parc_ferme_rules`, or `<section>/<name>`, `game/parc_ferme_rules`. The names are unique across the sections.
- A tag has to be a value of a context folder. A new setup mode tag, `gamesetup_f1_1990` in the example, comes from the addon's own database rows (a `game_setup_mode_tag` row of the `game_setup_mode` context folder), as the F1 1990 addon does. An unknown tag refuses the record and names the setup mode tags it could have used.
- The option needs a tag row of the tag's kind. A tag of a kind the option has no row for is refused with the kinds it has: the core adds no option rows.
- An unknown option is refused with the closest names. A record cannot declare a new option: an option is a database row the game's options store reads by name, and the core adds no logic for one.

## HUD elements

`hud.element` records put sprites, texts, level bars, gauges and sweeps into the game's HUD dials. They are data files (`operations.hud`), composed at build time across every enabled addon into one grown scene per dial. A record binds your own `view_model_data` leaves (written with `hud::set`) or the dial's own values through a typed table, and the build checks the tag of every binding against the stock template:

```json [registry/hud.json]
{
  "hud.element": {
    "dial_a": {
      "dial": "f1_26",
      "sprites": { "base": { "png": "art/base.png", "bounds": [-100, -48, 110, 38] } },
      "elements": [
        { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 },
        { "kind": "text", "id": "speed", "style": "OSD_24_bold", "align": "centre", "box": 40,
          "cap_height": 9.5, "at": [0, -14.9], "value": { "game": "speed_str" } }
      ]
    }
  }
}
```

`hud.widget` stays as the name of the template-only form and does nothing the records don't. The full format, with every element kind, binding, decision node and the HUD editor preview, is [HUD elements](/reference/ops/hud). To build one end to end, from the record and sprites to the plugin that writes its leaves and the host test, follow [Build a HUD element](/make/hud-element). The element templates and binding rules follow Project Halo by nyannoying. Uses work from Project Halo by nyannoying, <https://github.com/nyannoying1337/f1-25-project-halo>.

## Strings

A string can apply in one career or one box only. Stock keys change there and nowhere else:

```cpp
// F1 1990 careers call the first one-shot session "Qualifying 1"
ui::strings.add("lng_sprintshootout_title", {
    .text = { { "*", "Qualifying 1" } },
    .within = events::content("swofty.f1-1990/f1_career_1990"),
});

// a 2012 box shows the team's 2012 name, and every other box keeps the stock one
ui::strings.add("lng_team_alpine", {
    .text = { { "*", "Lotus" } },
    .within = events::content_exact("bydanix.f1-2012/f1_2012"),
});
```

The game looks text up every frame. The core keeps a table of every `lang.string` record, and each lookup checks the record's `within` against the current content, mode and career. The last record in load order that applies wins; when none applies, the game's own text shows. `lang.lookup` runs after that table: a `before` handler sees the key, the language and the text that would show, and `set_text` supplies another. The core runs the handlers once per key, the first time the game asks for it, and keeps the answer until the content, the language or the view models change; a key no handler takes keeps its table text or the game's own. With no subscriber the lookup is the same code as without the hook. `ui.vm.resolve` works the same way for view-model paths: a handler sets `node` to a node handle it has for the path, and the core hands it to the game in place of the game's own node. A handler on either point runs once per entry when the table is built, so it should only read a table of its own.

## Events and operations

```cpp
// Progressive's view-model hook: its own team names on the hub
events::on<ui::events::VmResolve>().in(events::content("swofty.progressive/f1_career_progressive"))
    .before([](ui::events::VmResolve &r, const events::Context &) {
        if (r.path.starts_with("hud_progressive_"))
            r.node = progressive::vm_node(r.path);
    });

ui::set_text("career.hub.title", "Season 1990");
ui::set_bool("ui_menu_data.my_addon_visible", true);
swofty::popup::show("Tyres", "Goodyear brings B, C, D and E this weekend.");
swofty::hud::toast("Driver change", "Gasly replaces Kvyat");
```

`set_text` and `set_bool` write a leaf of the view-model registry. The last dot of the path splits the leaf from its parent node, and the parent resolves from the registry root, so `ui_menu_data.my_addon_visible` writes `my_addon_visible` under `ui_menu_data`. The write is queued and applied on the game's flow thread, and a later write to the same path replaces a queued one. They return `unavailable` on a core without the UI table, and `invalid` for a path with no parent, a space or a bracket. `set_ref` returns `unavailable`: what a node reference targets is not proven yet.

## Readiness and limits

- **Ready:** tiles, era boxes, view models, routes, popups and strings. The front-end and view-model lanes are among the most L2 and L3 code outside the career.
- **Partial:** the HUD (`hud.element` records grow the three dial scenes, F1 26, F1 25 and F2, and their spectator dials; other HUD scenes are not composed), `.idf` layout write-back (shapes are baked meshes).
- **Fixed counts in native screens:** era tiles, four R&D departments, eight MFD pages, Team HQ tabs. Data-bound lists (R&D upgrade tiles, settings rows) take new rows with no UI patch.
- **A rejected node ends the whole screen build:** the screen comes up blank.
- **Growing a texture array** changes the archive layout. The core grows every archive that carries the array.
- **Renames** leave stale hashes behind. Third-party whole-file `screens/*.erp` replacements still overwrite each other.
- **Strings:** the in-game loader is required (no strings without it). Speech can't be edited. Text with counts in it ("tenth and final season") is hard-coded.
