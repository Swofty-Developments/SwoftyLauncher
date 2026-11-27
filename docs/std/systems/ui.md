---
title: UI and strings
description: Screens, tiles, texture slices, routes, options, HUD widgets and strings in code, with era boxes, the UI events and the view-model operations.
---

# UI and strings

With the UI system you change what the player sees in the menus and on the HUD: tiles and boxes, the images in the game's texture arrays, your own screens and routes, options, HUD widgets and text. Classic Seasons 2017, for example, adds an F1 2017 box to the Grand Prix picker and a 2017 Toro Rosso logo to the driver select screen.

A **texture array** is one of the game's image sheets, made of named **slices**. A **route** is the game's link from a press to the screen it opens. A **view model** is the data a screen binds to, by path.

## The header

<Split>

`swofty/systems/ui.hpp` has eight registries:

- `ui::texture_slices`: one named slice of a texture array (`ui.texture_slice`);
- `ui::tiles`: a home, career, F1 World or car-category tile (`ui.tile`);
- `ui::screens`: an addon screen (`ui.screen`);
- `ui::routes`: a front-end route group, cloned from a stock one (`ui.route`);
- `ui::options`: a game option and its restriction tags (`ui.option`);
- `ui::hud_widgets`: an addon HUD template (`hud.widget`, the template-only form of `hud.element` below);
- `ui::strings`: one text key in every language (`lang.string`);
- `ui::addon_options`: the options of an addon's ADDON SETTINGS page (`addon.option`).

The six registries from `ui.texture_slice` to `hud.widget` are experimental (since 1.10.54). Each has a schema in `packages/sdk/std/registries/`, and records in data files (`registry/*.json`) or code apply the same way.

Grand Prix and Time Trial era boxes are `content::era_boxes`, in `swofty/systems/content.hpp` (see [Seasons and GP/TT content](/std/content)).

<template #example>
<Example root="swofty-sdk" code-file="include/swofty/systems/ui.hpp">
<template #code>

```cpp
#pragma once
#include <swofty/registry.hpp>
#include <swofty/events.hpp>

namespace swofty::ui {

struct TextureSlice {
    std::string array;                         // "overrides.teams.driver_select_team_logos"
    std::string name;                          // the slice name the UI asks for: "cs_f1_2017"
    Path image;                                // a png or svg file of the addon
};
inline Registry<TextureSlice> texture_slices{"ui.texture_slice", {.applies = Applies::boot}};

struct Screen {
    Path file;                                 // the .ui file, in the addon's folder
    std::string flow;                          // "flow_options": the flow the build lists the screen in (see Screens and routes)
    std::string anchor;                        // the route group it opens from; empty: "settings_container"
    std::string view_model;                    // non-empty: the screen gets the addon view model
};
inline Registry<Screen> screens{"ui.screen", {.applies = Applies::boot}};

struct Route {
    std::string file;                          // "routes_frontend_general.uir"
    std::string clone_of;                      // the route group it copies
    LooseRef<Screen> opens;                    // a ui.screen record id or its bare key
};
inline Registry<Route> routes{"ui.route", {.applies = Applies::boot}};

enum class Place { home, career, f1_world, car_category };

struct Tile {
    Place place = Place::home;
    Text title;                                // a lang key, or { "literal" } in data files
    LooseRef<TextureSlice> image;              // a ui.texture_slice record id, or "#array[name]"
    LooseRef<Route> route;                     // what a press runs: a ui.route id or key, or a stock route name
    json::Value gate;                          // a selector object or a list of them; empty: always
};
inline Registry<Tile> tiles{"ui.tile", {.applies = Applies::read}};

struct Option {
    std::string id;                            // "game/parc_ferme_rules" or "parc_ferme_rules"
    std::vector<std::string> restriction_tags;
};
inline Registry<Option> options{"ui.option", {.applies = Applies::boot}};

struct HudWidget {
    std::string view_model;                    // the VM path it binds
    Path tmpl;                                 // a .ui file of the addon
};
inline Registry<HudWidget> hud_widgets{"hud.widget", {.applies = Applies::session}};

struct String {
    std::map<std::string, std::string> text;   // per language; "*" for every language
    json::Value within;                        // a career, a content, a mode; empty: everywhere
};
inline Registry<String> strings{"lang.string", {.applies = Applies::read}};

struct TileItem { std::string_view id, title, route, image, place; bool literal; };
struct TileSpec { std::string_view id, title, route, image; bool literal = false; };

class TileList {                               // payload version 2 of the tile events
public:
    size_t count() const;
    std::optional<TileItem> get(size_t index) const;
    Result remove(size_t index);
    Result move(size_t from, size_t to);
    Result insert(size_t at, const TileSpec &spec);
    Result append(const TileSpec &spec);
};

class TrackList {                              // payload version 2 of ui.track_select.build, read-only
public:
    size_t count() const;
    std::optional<std::string_view> get(size_t index) const;   // always nullopt on this core
};

namespace events {

struct EraTiles {                              // ui.era.tiles
    static constexpr uint32_t version = 2;     // v1 had only `list`; v1 readers still work
    uint32_t size, payload_version;
    void *list;                                // opaque, as in v1
    const SwoftyTileList *tiles_table;
    TileList tiles() const;                    // empty on a v1 payload
    Result remove(size_t index);
    Result insert(size_t at, const TileSpec &spec);
};
// HomeTiles (ui.home.tiles), CareerContentTiles (ui.career_content.tiles) and CarCategoryTiles (ui.car_category.tiles)
// have the same fields and helpers.

struct TrackSelectBuild {                      // ui.track_select.build, v2
    uint32_t size, payload_version;
    void *list;                                // the screen's view model
    const SwoftyTrackList *tracks_table;
    TrackList tracks() const;
};

struct VmResolve {                             // ui.vm.resolve, v2, hot
    uint32_t size, payload_version;
    const char *path_str;
    void *node;                                // a handler may set a node handle for the path
    std::string_view path() const;
    Result use_node_of(std::string_view other);  // v2: set node to the game's own node for another path
};

struct LangLookup {                            // lang.lookup, v1, hot
    uint32_t size, payload_version;
    const char *key_str, *language_str, *text_str;
    std::string_view key() const;
    std::string_view language() const;
    std::string_view text() const;             // the text that would show
    Result set_text(std::string_view text);    // supply another
};

struct ScreenBuilt {                           // ui.screen.built, v1, notify
    uint32_t size, payload_version;
    const char *screen_str;
    void *view;
    std::string_view screen() const;
};

struct RouteOpen {                             // ui.route.open, v1, cancel
    uint32_t size, payload_version;
    const char *route_str, *screen_str;
    std::string_view route() const;
    std::string_view screen() const;
};

struct MfdPage { const char *name; uint8_t hide; uint8_t reserved[7]; };

struct MfdPages {                              // hud.mfd.pages, v1
    uint32_t size, payload_version;
    void *pages;
    int32_t count, capacity;                   // 6 and 8
    MfdPage *page(int index) const;
};

} // namespace events

Result set_text(std::string_view vm_path, std::string_view text);
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
        .array = "overrides.teams.driver_select_team_logos",
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
    "logo_toro_rosso": { "array": "overrides.teams.driver_select_team_logos", "name": "cs17_toro_rosso",
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
[registry] ui.texture_slice home_screen.tiles_solo_6x6: 2 slices from 2 addons (swofty.core, swofty.classic-seasons-2017), 0 contested
[registry] ui.texture_slice teams.driver_select_team_logos: 34 slices from 3 addons (swofty.classic-seasons-2017,
           fanfreluche.cars, bydanix.f1-2012), 0 contested
[apply] ui.texture_slice boot: 2 applied, 0 put back, 0 refused
[registry] applied at boot: ..., ui.texture_slice 2 (2 arrays grown in 35 archives)
[era] box cs_f1_2017 from swofty.classic-seasons-2017 registered (title lng_cs_era_2017, image #overrides.home_screen.tiles_solo_6x6[cs_f1_2017], order 2017): queued for the next mixin commit
[cs-2017] era picker built
```

The core builds the slices into overlay archives that it adds to the game's asset manifest at boot. The data version is the same records. Only the after handler needs code.

A slice that does not apply is listed under the boot line, as `[apply] ui.texture_slice boot:   ui.texture_slice <id>: not applied: <why>` (for example an `image` that is not a file of the addon's folder).

### Texture slice limits

- The core grows eight arrays: `teams.driver_select_team_logos`, `team_logos.driver_career_2024_team_select`, `teams.mp_team_select_logos`, `osd.timing_tower.team_badges_hud`, `osd.timing_tower.team_badges_broadcast`, `teams.badges_broadcast`, `broadcast_ui_generic.grid_team_logos` and `home_screen.tiles_solo_6x6`. The `overrides.` prefix is optional, and the old singular `driver_select_team_logo` is read as the plural. Any other array refuses the record and the log names these.
- `image` is a `.png` or `.svg` file in the addon's folder. `name` has no space, `#` or bracket.
- Two addons that name the same slice of one array are contested: the later one in load order wins, and the log says so.
- An `era.box` or `ui.tile` image `#<array>[<name>]` that no enabled record provides logs a warning (`image slice <name> of <array> is not provided by any enabled ui.texture_slice record`) and still shows if the game ships that slice; otherwise the tile is blank. The record is kept.

### Era box limits

- **One box per entrant list.** An `era.box` whose package has a box already is refused, and neither box is replaced: `[era] box <entrant list> from swofty.core refused: a box for this entrant list is already registered by <owner>`, then `[apply] era.box read:   era.box <record id>: <entrant list> not registered (a box for it exists or the picker is off)`. Every package a season addon ships registers its own box, so an `era.box` record on one of those packages never shows. To add a box for a season that has one, give it a package of its own that starts from the original (a `content.package` with `extends` and its own `entrant_list`, as in [Seasons and GP/TT content](/std/content)) and point the `era.box` at that; its box sits next to the original.

## Tiles, screens and routes

```cpp
auto settings = ui::screens.add("settings", {
    .file = file("ui/settings.ui"), .flow = "flow_options", .anchor = "settings_container", .view_model = "mod.settings",
});
auto open = ui::routes.add("open_settings", { .file = "routes_frontend_general.uir", .clone_of = "mail_container", .opens = settings });

ui::tiles.add("settings_tile", {
    .place = ui::Place::home,
    .title = "lng_mod_settings",
    .image = ui::texture_slices["mod.settings/tile"],
    .route = open,
    .gate = events::mode(events::Mode::driver_career),
});
```

The same records in a data file:

```json [registry/ui.json]
{
  "ui.screen": {
    "settings": { "file": "ui/settings.ui", "flow": "flow_options", "anchor": "settings_container", "view_model": "mod.settings" }
  },
  "ui.route": {
    "open_settings": { "file": "routes_frontend_general.uir", "clone_of": "mail_container", "opens": "mod.settings/settings" }
  },
  "ui.tile": {
    "settings_tile": { "place": "home", "title": "lng_mod_settings", "image": "mod.settings/tile",
                       "route": "mod.settings/open_settings", "gate": { "mode": "driver_career" } }
  }
}
```

### Screens and routes

The launcher's build lowers every enabled `ui.screen` and `ui.route` record into the same pipeline as `operations.screens` in `swofty.json`, so a screen can come from either; the same name in both is refused.

- **Screens:** `file` is a `.ui` file in the addon's folder (a stock or archive path is refused). `flow` is one of `flow_frontend`, `flow_options`, `flow_race`, `flow_career_hub`, `flow_f1_world`, and only `flow_options` is built into the game's flows: the build writes the flow entry (`enter_<screen>`), the region entry and the anchor route for those screens alone. A `ui.route` can open a `flow_options` screen only; a route to a screen in another flow is refused with `the build writes flow entries only for flow_options`. A screen in another flow is registered with the game's screen loader and nothing opens it. `anchor` defaults to `settings_container`. `view_model` is read as set or not: any non-empty path gives the screen the addon view model, and the path itself is not used. `extends` is refused on a screen.
- **Routes:** `file` must be `routes_frontend_general.uir`. `clone_of` is required and must name a route group in that file; the build copies it, renames the group to the record key, and swaps every string equal to `clone_of` for the screen. `opens` must be a `ui.screen` of an enabled addon. A group of that name that exists already, or an unknown `clone_of` (the error lists the groups the file has), is an error. `extends` is refused on a route.
- A screen is reachable only through a route and a flow entry. A rejected node blanks the screen (see Readiness and limits).

#### Making an addon screen

The game opens a screen by the name of its root node, and a screen name is global: two screens of one name cannot both register (the game's build result is `0x55`, `a same-name screen is already registered`, and the screen never builds). The build handles the name for you.

1. Start from any `.ui` file with exactly one root screen, such as a copy of one of the launcher's ADDON SETTINGS pages, and put it in the addon's folder (`ui/stdui.ui`).
2. Declare it as a `ui.screen` record. The record key is the screen's name, so pick one no other screen has: not a stock screen, not another addon's key, and not `swofty_settings`, `swofty_resume_list` or `swofty_addon_<n>`, which the build generates (a reserved name stops the build).
3. You do not edit the file's root name. When the root node is named something else, the build writes a copy of the file with the root, and every reference inside it that starts with the old name, renamed to the record key, and places that copy in the game. The copy in your addon is not changed. A file with more than one root screen is used as written, so its root must already carry the key.
4. Add a `ui.route` with `clone_of` set to a group of `routes_frontend_general.uir` (`mail_container` works) and `opens` set to the screen, and a `ui.tile` whose `route` is the route record's id or key.
5. A screen with a `view_model` needs a view model the core can attach by screen name; one it cannot attach logs `needs a view model and its view model is not attached yet; not registered, so routes to it are cancelled` when it registers, and a press on its route leaves the front end where it is.

A build log line `ui screen <key> joins flow_options at settings_container` confirms the record, and `[ui] screen <key> (flow_options): root '<key>' built with result 0x0 (ok, ...)` confirms the game built it.

### Tiles

A `ui.tile` record adds a tile to one of four screens, after the stock tiles:

| `place` | Screen | Room | Image |
|---|---|---|---|
| `home` | the main menu | 255 minus the tiles already on it | yes |
| `f1_world` | the F1 World menu | 255 minus the stock tiles | no |
| `career` | the career content picker (the Addons row) | 8 minus the career content tiles | yes |
| `car_category` | the LAN Car Category tab | 4 minus the era boxes on it | yes |

- Every place needs a `route`; a record with no route, no title, or an unknown `place` is refused and logged. `title` is a lang key, or `{ "literal": "text" }` in a data file.
- Records apply in load order, one per id: a repeated id keeps the first position and the later record's content. Records over the room are not shown and the log counts them.
- `gate` is a selector over mode, career and content, the same as `lang.string` `within`. It is read at build time; a gated tile does not show where the facts are not known yet.
- `route` names a `ui.route` record or a stock route; see Route names below for how it reaches the game. In code `route`, `image` and a route's `opens` take a `Ref` (`open`, `ui::texture_slices["..."]`) or a plain name through `LooseRef<T>("mod_menu")`; neither field is resolved when the record is checked, so a name that nothing provides is a log line, not a refused record.
- Images: `image` is a `ui.texture_slice` record id or a `#array[name]` string. A `#array[name]` that no enabled record provides logs `[ui.tile] <place>: <tile id>: image slice <name> of <array> is not provided by any enabled ui.texture_slice record (a slice the game ships in that array still shows)` once, and the tile is kept; a record id that no enabled `ui.texture_slice` provides logs `[ui.tile] <place>: <tile id>: image slice <id> is not provided by any enabled ui.texture_slice record; the tile shows without an image`, and the tile is kept without an image. Neither drops the tile. Home and career tiles take it through an image template that the launcher's UI build adds only when an enabled record names an image; without it the log says so and the tile shows no image. The car category tab sets it through the tab's own tile fill, and a tile without a usable image shows the F1 2026 tile's image. F1 World tiles have no image.
- Home, career and car category tiles fire their list event before they are added; F1 World tiles have no event.
- The stock tiles the game builds itself (Career, Grand Prix, the settings row, F1 2025 and F1 2026 on the car category tab, the content tiles) are not records and cannot be changed through `ui.tile`.

### Editing the tile lists (payload version 2)

The four tile events and `ui.track_select.build` carried only an opaque `list` pointer into the core. Payload version 2 adds a versioned C table next to it (`SwoftyTileList` and `SwoftyTrackList` in `swofty_api.h`: `size`, `version`, `self`, then the functions) with C++ helpers in `ui.hpp` (`ev.tiles()`, `ev.tracks()`, `ev.remove(i)`, `ev.insert(at, spec)`). `list` stays where it was, so a v1 reader still works. Read the table only when `payload_version >= 2` and `size` covers it; the helpers do that check and return `Result::unavailable` otherwise, and for any function a core leaves null.

| Event | The table's list | `get` | `remove`, `move` | `insert` |
|---|---|---|---|---|
| `ui.home.tiles` | the `ui.tile` records of place `home` about to be added (cap: 255 minus the tiles already on the screen) | yes | yes | yes |
| `ui.career_content.tiles` | the records of place `career` (cap: 8 minus the content tiles) | yes | yes | yes |
| `ui.car_category.tiles` | the records of place `car_category` (cap: 4 minus the era boxes) | yes | yes | yes |
| `ui.era.tiles` | the addon boxes (`era.box` records) the picker is about to add, as shown on that picker | yes | yes | no: unavailable; add a box with an `era.box` record |
| `ui.track_select.build` | `SwoftyTrackList`: the rows the Grand Prix and championship settings screens just filled | no | no | no |

An edit reaches the step that adds the tiles: home, career content and the car category tab fire the event after reading the records and before adding any, so a removed record tile is not added and an inserted one is. The core then settles the list as before, one tile per id and no more than the cap, and logs what it dropped. Each function returns `Result::invalid` for an index out of range, a spec with no id or title or no route, an id the list already holds, or an insert into a full list. Strings from `get` are valid only during the handler call and until the next edit; copy what you keep. An inserted tile has no gate and its title is a lang key unless `literal` is set. `get` gives the record's id (`<addon id>/<key>`) as `id`, so a handler can find a tile another addon added.

```cpp
events::on<ui::events::HomeTiles>().before([](ui::events::HomeTiles &e, const events::Context &) {
    auto tiles = e.tiles();
    for (size_t i = tiles.count(); i-- > 0;)
        if (auto t = tiles.get(i); t && t->id == "mod.a/old_tile")
            tiles.remove(i);
    tiles.insert(0, {.id = "my.tile", .title = "lng_my_tile", .route = "my.route"});
});
```

Limits of the lists:

- **Home, career and car category:** the lists hold record tiles only. The stock tiles are game objects with button indices and view-model nodes behind them, so they are neither listed nor removable.
- **Era tiles:** `get`, `remove` and `move` cover add-on boxes only; stock tiles are not listed. The stock boxes are game objects (a 16-byte allocator and tile pair per entry, with the title and id in view-model nodes). `ui.era.tiles` fires before the addon boxes are added, so a removed or moved box is added in that order or not at all. Indices count the boxes the picker shows. `get` gives the box's entrant list as `id`, its lang key as `title`, the entrant list its tile selects as `route`, and `place` is `era`.
- **Track select:** count only. `ui.track_select.build` fires once per rebuild of the Grand Prix and the championship settings track lists, after the game has filled them, and `tracks().count()` is the number of rows, one per displayed track. A row holds node references, not the track id, so `get` returns nothing and there is no remove or insert. Time Trial's carousel is not covered: its list builder is not found. The point needs the two `Trk_*RefreshTrackList` bindings to export; where they do not resolve, the log says `[hooks] ui.track_select.build unavailable` and handlers do not run.

### Route names

One name is used everywhere: the key of the `ui.route` record. The build clones the route group `clone_of` under the record's key (`open_settings` in the example, not `mod.settings/open_settings`), and that is the route name the game runs. A route key is global across addons, so two addons cannot both declare `open_settings`; the build refuses the second.

- **A tile's `route`** names a route record by its id (`mod.settings/open_settings`) or by its bare key (`open_settings`), or is a stock route name (`mod_menu`, `career_hub`). The core turns a record id into the key before it gives the route to the game; a bare key and a stock name pass through unchanged. A name with a `/` is a record id, and one that no enabled record has is logged (`[ui.tile] ui.tile <tile id>: route <route> names no enabled ui.route record; the game gets it as written`) and passed as written, which opens nothing. A name with no `/` is not checked, since it may be a stock route; one that matches nothing the game has opens nothing and is not logged. The same rule covers a route a handler inserts into `ui.home.tiles`, `ui.career_content.tiles` or `ui.car_category.tiles`; the line is logged once per tile, and the log shows the first few of a screen's lines.
- **A route's `opens`** names a `ui.screen` record by its id or by its bare key; the screen's registered name is the key.
- **`ui.route.open`** fires with the route name the game runs (the key) as `route()`. `screen()` is the id of the `ui.screen` record the route opens (`mod.settings/settings`), or empty for a route no record declares (a stock route).
- **`ui.screen.built`** fires for every screen that builds without error. `screen()` is the id of the addon's `ui.screen` record for an addon screen (`mod.settings/settings`), and the name of the screen's root node for every other screen: stock screens, screens an addon declares in `swofty.json` only, and the options pages the launcher generates.

## Other UI events

All of these are `Fired::game`: the core installs the hook only once an addon subscribes.

- **`ui.screen.built`** (notify, v1): after the game builds a screen without error, stock or addon. `screen()` is the `ui.screen` record id for an addon screen and the root name for any other (see Route names), and `view` is the token of that view-model node, or null.
- **`ui.route.open`** (cancel, v1): before the game runs a route (`Vm_Run_Route`). `route()` is the route name the game runs. `screen()` is the `ui.screen` record id the route opens, or empty for a stock route (see Route names). A `replace` handler that returns true stops the route.
- **`hud.mfd.pages`** (modify, v1): the race HUD's MFD pages. `pages` holds `count` entries (6: `car_settings`, `pit_settings`, `car_damage`, `car_engine`, `car_temp`, `mods`) and `capacity` is 8. A handler can only hide a page, by setting `hide` to 1; the core then writes that page's `<name>_enabled` false after the game's own availability rules, so a handler never shows a page the game disabled. Reordering, adding and removing are refused and logged. The point needs the `Hud_MfdVm_RefreshPanelAvailability` binding to export.

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

`hud.widget` is the template-only form of a `hud.element` record, and does nothing a `hud.element` record doesn't. A record `{ "view_model", "tmpl" }` is lowered before composing to the `hud.element` record `{ "template": { "file", "view_model" } }`, so one composer builds both. `tmpl` is a `.ui` file in the addon's folder (a stock or archive path is refused), and `view_model` is required. A widget and an element with the same id in one file is an error. The full format, with every element kind, binding, decision node and the HUD editor preview, is [HUD elements](/reference/ops/hud). To build one end to end, from the record and sprites to the plugin that writes its leaves and the host test, follow [Build a HUD element](/make/hud-element). The element templates and binding rules follow Project Halo by nyannoying. Uses work from Project Halo by nyannoying, <https://github.com/nyannoying1337/f1-25-project-halo>.

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

The game looks text up every frame. The core keeps a table of every `lang.string` record, and each lookup checks the record's `within` against the current content, mode and career. The last record in load order that applies wins; when none applies, the game's own text shows. `lang.lookup` runs after that table: a `before` handler sees the key, the language and the text that would show, and the payload's `set_text` supplies another (4096 bytes or more is refused with `Result::invalid`). The core runs the handlers once per key, the first time the game asks for it, and keeps the answer until the content, the language or the view models change; a key no handler takes keeps its table text or the game's own. With no subscriber the lookup is the same code as without the hook. `ui.vm.resolve` works the same way for view-model paths that the game looks up: a handler sets `node` to a node handle, and the core hands it to the game in place of the game's own node. The way to get a handle is `use_node_of(path)` (payload version 2): the core resolves `path` with the game's own lookup, handlers not run for it, and sets `node` to the handle it got. It returns `Result::unavailable` when `path` has no node, or on a core that is older than payload version 2, and leaves `node` as it was. A handler can also set `node` to a handle of its own: a pointer to the node pointer the game's lookup returns. A lookup that creates a node is not offered to handlers, and a handle that does not point at a readable node is ignored. The table is reset when the content or the view models change, and the log says `[hooks] ui.vm.resolve (hot): table reset`. A handler on either point runs once per entry when the table is built, so it should only read a table of its own.

## Events and operations

```cpp
// Progressive's view-model hook: its team title leaf reads the hub's title node
events::on<ui::events::VmResolve>().in(events::content("swofty.progressive/f1_career_progressive"))
    .before([](ui::events::VmResolve &r, const events::Context &) {
        if (r.path() == "hud_progressive_title")
            r.use_node_of("career.hub.title");
    });

ui::set_text("career.hub.title", "Season 1990");
ui::set_bool("ui_menu_data.my_addon_visible", true);
swofty::popup::show("Tyres", "Goodyear brings B, C, D and E this weekend.");
swofty::hud::toast("Driver change", "Gasly replaces Kvyat");
```

`set_text`, `set_bool` and `set_ref` write a leaf of the view-model registry. The last dot of the path splits the leaf from its parent node, and the parent resolves from the registry root, so `ui_menu_data.my_addon_visible` writes `my_addon_visible` under `ui_menu_data`. A leading `#` on the path is dropped. `set_ref` binds the leaf to another node: `target` is a node path from the registry root, or a `#`-prefixed path as the game writes them. If the leaf exists and is not a reference, the write is refused.

A call returns `ok` once the write is queued. The write is applied on the game's flow thread, and a later write to the same path replaces a queued one. The queue holds 512 writes: past that the oldest is dropped. Text is cut at 200 characters. A call returns `invalid` for a path with no parent, a space, a bracket, an empty `set_ref` target, or a path that a hud leaf holds with another type. A parent that does not resolve, a target that resolves to no node, or a leaf that is not a reference is not a return code: it shows in the log as `[hud] set failed` or `[ui] set_ref ... refused`. The calls return `unavailable` on a core without the UI table.

## Readiness and limits

- **Ready:** tiles (home, F1 World, career, car category), era boxes, texture slices, screens, routes, options, `hud.widget`, view models, popups and strings. The front-end and view-model lanes are among the most L2 and L3 code outside the career.
- **Experimental:** the six registries `ui.texture_slice`, `ui.tile`, `ui.screen`, `ui.route`, `ui.option` and `hud.widget`.
- **Bindings:** a point whose game binding does not resolve on a build logs `unavailable` and runs no handler. A binding exports only after an independent verify row (`re/tools/binding.py status` lists the ones that wait).
- **Partial:** the HUD (`hud.element` records grow the three dial scenes, F1 26, F1 25 and F2, and their spectator dials; other HUD scenes are not composed), `.idf` layout write-back (shapes are baked meshes).
- **Fixed counts in native screens:** era tiles (255 boxes), four R&D departments, six MFD pages (capacity 8, hide only), Team HQ tabs, the car category tab (4 addon slots), the career content row (8). Data-bound lists (R&D upgrade tiles, settings rows) take new rows with no UI patch.
- **Not covered:** stock tiles in the tile lists, Time Trial's track carousel, the track id in `ui.track_select.build`, images on F1 World tiles, `extends` on screens and routes.
- **A rejected node ends the whole screen build:** the screen comes up blank.
- **Growing a texture array** changes the archive layout. The core grows every archive that carries the array, and only the eight arrays above.
- **Renames** leave stale hashes behind. Third-party whole-file `screens/*.erp` replacements still overwrite each other.
- **Strings:** the in-game loader is required (no strings without it). Speech can't be edited. Text with counts in it ("tenth and final season") is hard-coded.
