# Classic Seasons

Adds the F1 2016 to F1 2024 seasons to the Grand Prix and Time Trial pickers. Each season has its own teams, drivers, car numbers, cars, team logos and name backdrops.

Classic Seasons is ten addons in `addons/classicseasons/`. This folder is the shared addon, `swofty.classic-seasons`: the plugin that registers the season boxes, the new drivers, real driver heads, team logos, name backdrops and box art. Each season is its own addon, `swofty.classic-seasons-<year>` in `../classic-seasons-<year>/`, which requires the core and holds that season's cars, rows and text. Any set of seasons installs on top of the core, and a season that is not installed has no box.

Cars: fanfreluche (Fanfreluche aka totolab), "F1 <year> cars for F1 25", used with permission.

Code Creation and Asset Importing: Swofty. Supported by AMO Motorsport (https://discord.gg/amocommunity).

The car, suit and head archives are not in the repository. The built packages, each season's `.swoftyaddon` with its cars and the shared addon's, are on the `classic-seasons-<version>` release; install the shared addon and any seasons through the launcher.

## Layout

| Path | Holds |
|---|---|
| `data/seasons.json` | Teams, team names and line-ups per season, new drivers |
| `data/stock_looks.json` | Stock helmet, suit and glove looks used as placeholders |
| `data/heads.json` | The real heads, each with its mod zip |
| `tools/heads.py` | Copies the heads into `heads/` (not committed) and writes `assets/heads.json` |
| `tools/gen.py` | Writes the core's `db/core.json`, `lang/core.json` and `native/src/seasons.hpp`, and each season addon's `swofty.json`, `db/season.json`, `lang/season.json` and `assets/cars.json` |
| `tools/layout.py` | Season addon folders and the asset ops that ship a season's cars |
| `tools/build_cars.py` | Builds a season addon's `cars/` from the season zips, and an extra team's car (`car` in `data/seasons.json`) into `cars/extra/` |
| `tools/carperf.py` | Shadow caster rebuild and texture cap, run by `build_cars.py` |
| `tools/handling.py` | Handling files: wheel positions from the handling file, run by `build_cars.py` |
| `tools/pace.py` | Handling files: each team's pace as chassis mass, run by `build_cars.py` |
| `tools/powerunit.py` | Handling files: the stock 2026 power unit, run by `build_cars.py` |
| `tools/aislots.py` | Measures the stock teams' per-track AI data into `data/ai_slots.json` and gives each season team a slot |
| `tools/tyres.py` | `Tyres` overrides naming a season's tyre set, run by `build_cars.py` |
| `tools/build_incardriver.py` | Per-season in-car driver archives (2016-2021), in the season addon's `cars/incardriver/` |
| `tools/logos.py`, `tools/frontend.py` | Team logo arrays (`erp/logos/`) and name backdrops (`erp/frontend/`) |
| `tools/tiles.py` | Season box art (`erp/tiles/`): one `tiles_solo_6x6` slice per season from `art/tiles/src/<year>.dds` |
| `data/real_suits.json` | Real suits from mods: each one's season, team, driver, base suit and archives |
| `tools/suit_sources.py` | The suit each team-season and driver of 2016 to 2021 wears |
| `tools/suit_transfer.py` | Converts the F1 2020 suit mods of `data/real_suits.json` to F1 25 suit mods for `build_real_suits.py` |
| `art/suits/transfer/<season>-<stock team>.png` | A converted team: renders of the old and the F1 25 body, the old diffuse with its decals and the new `livery_v2_d`, for the team suit and each driver suit |
| `tools/build_real_suits.py` | The real suits from mods, relabelled, in `suits/`, and their contact sheet `art/suits/real.png` |
| `tools/build_suits.py` | Generated team suits and gloves of 2016 to 2021, in the season addon's `suits/`, and the season contact sheets |
| `art/suits/<year>.png` | Half-size contact sheet of a season's suits; the full one is the season addon's `suits/preview.png` |
| `tools/bcn.py`, `tools/suitgeo.py` | BC1, BC3, BC5 and BC7 textures and `.mipmaps` levels; mesh streams, decal slots and the suit renders, used by `build_suits.py` |
| `native/` | The plugin that adds the picker boxes |
| `../classic-seasons-<year>/cars/` | Built car archives of a season (not committed) |
| `suits/` | Built real suit archives, their `.mipmaps` sidecars, `meta.json` and `preview.png` (not committed) |
| `../classic-seasons-<year>/suits/` | Built generated suit archives of a season, their `meta.json` and `preview.png` (not committed) |
| `data/setup_preset_fields.json` | The 421 keys of a `car_setup.presets` row, copied for every season package |
| `tools/team_colours.py` | Writes `data/team_colours.json`: three team-wear colours per team, sampled from its car's paint texture (`<folder>_paint_d.tif`, or `_paint_c.tif` when `paint_d` is an 8 px placeholder), or the team's `team_wear` hex list in `data/seasons.json`; `--sheet <png>` draws a swatch sheet |

## Rows

Shared addon:

- The new drivers, their statistics holders and the head overrides (`head_overrides` in `data/seasons.json`): the driver rows' `head`, a stock generic head (`male_ls<n>`) or one of the real heads below.
- Statistics holders for the season drivers the stock game has none for (`holders_for` in `data/seasons.json`), copied from de Vries's.
- Each season driver's statistics dataset `classic_seasons` for the setup mode `swofty_f1`, a copy of his `default` dataset whose pace is flat at the median of his season ratings in `data/performance.json` (tools/ratings.py).

Each season:

- `game_setup.scenarios` row `cs_f1_<year>` (setup mode `swofty_f1`, swofty.core's host setup mode for addon boxes), entrant list `cs_f1_<year>` (a copy of `f1_2025`, 20 entrants, or of `f1_2026` with the 2025 season, leaderboard and UI path for 22), team standings `cs_f1_teams_<year>`. A 22-car season sets `swofty_f1`'s `max_entrants` to 22.
- `list-append` of `cs_f1_<year>` onto the `swofty_f1` entrant setup mode's `entrant_lists` and `selectable_entrant_lists`, and onto the 2025 and 2026 content sets.
- Teams and vehicles `cs<yy>_<key>` (`key` defaults to the team's `folder`). A team's `primary_colour` and `ui_team_colour` (the timing tower and menus) are its `colour` in `data/seasons.json`: F1's broadcast colour of that season for 2018 to 2024, and one picked from the livery for 2016 and 2017. `secondary_colour` and `tertiary_colour` are the second and third, and `team_wear_primary_colour`, `team_wear_secondary_colour` and `team_wear_tertiary_colour` all three, of its colours in `data/team_colours.json`. Vehicle package `cs_<year>`, and `cx_<year>` for a team with its own car archive.
- Each vehicle's `performance_tier` and `tyre_wear_scale` by its team's rank in the season (`aislots.ranked`): tiers 0, 1 and 2 for the first four, the next three and the rest, as the stock 2025 cars have them, and a tyre wear scale of 1.04 for the fastest team, 0.01 less per place, down to 0.95.
- A team whose AI slot (see [Performance](#performance)) isn't its own `folder`: vehicle package `cs_<year>_<key>`, whose `resource_path` names the team's car archive, and the vehicle's `asset_filename` set to the slot.
- A `car_setup.presets` row for each vehicle package (`cs_<year>_vehicle_package`, `cx_<year>_vehicle_package`, `cs_<year>_<key>_vehicle_package`), copied from `f1_2025_vehicle_package`: the game looks up the AI and player car setups by package name.
- A `pit_lane.pit_times` row per team, copied from its stock team.
- Driver looks `<year>-cs_<key>_<driver>` (stat ids from 420, numbered across every season) and pool entrants `cs<year>-<driver>`.
- 2016 to 2021: each driver look wears the suit and gloves of [Suits](#suits). A season copies the suit and gloves rows of each generated suit it ships, and the shared addon those of each real suit from a mod, with its helmet row. The copies are labelled with the suit's label and take the base suit's rows with the suit's asset group, models and `material_override`. Item ids (`tools/gen.py`): `REAL_SUIT_IDS` for the real suits, 974 up in `data/real_suits.json` order; `SUIT_IDS` for the generated ones, the next ids, one per team-season of 2016 to 2021 in season and team order; `TRANSFER_SUIT_IDS` for the real suits of converted F1 2020 mods, the ids after `SUIT_IDS`, in `data/real_suits.json` order; `HELMET_IDS`, 868 up, for the helmets. The stock ids are 448 to 973 for suits and gloves and 315 to 867 for helmets. Each suit and gloves copy sets `colour_1_default` to `colour_4_default` to `0`, a helmet copy the base helmet's values, and the file's `fields` map lists the four keys for suit, gloves and helmet rows.

New drivers (Nasr, Gutiérrez, Vandoorne, Hartley, Sirotkin, Haryanto) use driver stat ids 225 to 230.

With a season of more than ten teams, the shared addon adds a `track_data.track_layers` row for `swofty_f1` (seasons 2023 to 2025) with the `f1_26` worlds, which have an eleventh garage, and removes `swofty_f1` from the `f1` track layer.

## Performance

`data/performance.json` (tools/performance.py) holds, per season, each team's median qualifying gap to the fastest team in percent, each driver's median gap to his teammate, and each driver's pace rating on F1 25's 0 to 100 scale (10 points per 1 % of lap time, fitted to `data/stock_pace.json`, the stock ratings of the drivers who race in F1 25). The qualifying times come from the Jolpica F1 API, cached in `build/jolpica/`.

```sh
python3 tools/performance.py [--refresh]
```

Outside career, a car's pace comes from its handling file and its per-track AI data, not from the upgrade trees (`upgrades.2026.trees.*`).

The AI reads a car's per-track data from `environment_package/tracks/<track>/ai/<ai_package_folder>/<asset_filename>/`: braking, cornering and throttle tables, the lap times of simulated sessions (`.vp_times`), and tyre wear, fuel and ERS use. The tables are kept per mass. `data/ai_slots.json` (tools/aislots.py) holds each stock 2025 team's mean lap-time gap to the fastest one at 800 kg over 28 tracks, from Red Bull's (0.03 %) to Toro Rosso's (0.47 %), and the lap time a kilogram costs (0.0308 %). Each season gives its teams these slots in the order of their real gaps, and teams past ten the slowest.

```sh
python3 tools/aislots.py [--game <F1 25 folder>]
```

## Plugin

At load it registers one box per season with `swofty::era::add_box` (`swofty/era.hpp`): entrant list `cs_f1_<year>`, title `lng_cs_f1_<year>`, image `#overrides.home_screen.tiles_solo_6x6[cs_f1_<year>]`, in year order. swofty.core builds them in the Addons row of the Grand Prix and Time Trial era picker, skips a season that is not installed and narrows `swofty_f1` to the pressed season.

## Heads

| Driver | Head | Seasons | Mod |
|---|---|---|---|
| Kimi Räikkönen | `kimi_raikkonen` | 2016 to 2021 | More Driver Models |
| Sebastian Vettel | `sebastian_vettel` | 2016 to 2022 | More Driver Models |
| Romain Grosjean | `romain_grosjean` | 2016 to 2020 | More Driver Models |
| Daniil Kvyat | `daniil_kvyat` | 2016, 2017, 2019, 2020 | More Driver Models |
| Antonio Giovinazzi | `antonio_giovinazzi` | 2019 to 2021 | More Driver Models |
| Nicholas Latifi | `nicholas_latifi` | 2020 to 2022 | More Driver Models |
| Mick Schumacher | `mick_schumacher` | 2021, 2022 | More Driver Models |
| Nikita Mazepin | `nikita_mazepin` | 2021 | More Driver Models |
| Nyck de Vries | `nyck_de-vries` | 2023 | More Driver Models |
| Robert Kubica | `robert_kubica` | 2019 | F1 25 Kubica Face Mod (Cap On) |

The stock database has an `entrants.animation.driver_heads.driver_head` row for each of these heads. The row's `model_path` is `eaid://[CHARACTER_PACKAGE]/heads/male_driver/<head>/idf/<head>_rig_head_morph_v1`, and its asset group path formats give two groups named after the row's label. The stock manifest lists the groups of the stock labels, so the core never registers an addon's group under them, and a player's game folder can hold other files at their archive paths. Each head ships under its own label `cs_<head>`:

- `db/core.json` adds a `cs_<head>` row copied from the stock `<head>` row and sets the driver's `head` to it.
- `2025_asset_groups/animation_package/scenes/cinematics/drivers/heads/cs_<head>.erp`: the model, materials and textures.
- `2025_asset_groups/animation_package/scenes/cinematics/drivers/morph_poses/cs_<head>.erp`: the facial morph clips.

Kubica's head also has three `.2025.mipmaps` textures in `character_package/heads/male_driver/robert_kubica/textures/`. The More Driver Models heads hold their textures in the heads archive.

`assets/heads.json` adds each file with `file.put` and registers both `cs_<head>` groups with `group.register`, with `lando_norris`'s groups as templates. `tools/heads.py` reads the zips named in `data/heads.json` from the downloads folder. It fails when `--game` is not the F1 25 folder, when a target exists in it, or when no driver in `data/seasons.json` has the head.

```sh
python3 tools/heads.py --downloads ~/Downloads --game <F1 25>
```

More Driver Models' Sebastian Vettel, Mick Schumacher, Nicholas Latifi and Nyck de Vries archives have materials in an older format (`.sampler` resources, head textures under the driver's name). `tools/heads.py` replaces each such material with Kimi Räikkönen's of the same name, its texture folder code renamed (`repair_from` in `data/heads.json`).

More Driver Models labels its F1 25 `eye`, `head` and `head_cutscene` materials with resource version 1, and a replaced material keeps the version of the one it replaces. F1 25 parses a material of any version but 2 as the F1 23 layout. `tools/heads.py` gives every material in the F1 25 layout version 2, changing only that version.

A suit entry in `data/real_suits.json` with `"off": true` is left out. The 18 converted 2017 driver suits whose F1 25 base wears its team's glove, livery and boots textures crash the Grand Prix team picker, so those drivers wear their team's converted 2017 suit.

## Helmets

`data/helmets.json` gives drivers of a season their helmet of that season from a mod's textures: 21 drivers of 2016 from 2016 Helmets (Fernando Alonso's texture there is Jenson Button's helmet, so he keeps his stock one). F1 25's helmet shells share one texture layout, and each entry's base is an F1 25 look whose shell is the brand on the helmet.

`tools/build_helmets.py` relabels the base's helmet archive to `cs<yy>-<base look without its year>` and gives its `helmet_d`, `helmet_n` and `helmet_s` the mod's textures, every level in the archive. It writes the season addon's `helmets/` and `helmets/meta.json`; `gen.py` adds each helmet row (ids after the real suits' helmets), sets the driver's look of that season to it and writes `assets/helmets.json`, whose groups list the base helmet archive first.

```sh
python3 tools/build_helmets.py --downloads ~/Downloads --game <F1 25>
```

## Car archives

`build_cars.py`:

- Renames each archive's `eaid://f1_2025_vehicle_package/` resources to `eaid://cs_<year>_vehicle_package/` (same length). A season without its own `common.erp` (2022, 2023) renames only `teams/` and lists the stock `common.erp` in its asset groups.
- Moves the `.mipmaps` sidecars the zip ships to the season's folder.
- Rebuilds every shadow caster over 20,000 triangles from a lower LOD of the same part, and fails a car with a caster over 40,000.
- Caps every non-livery texture and shipped sidecar at 1024 px.
- Moves the cockpit camera of a season with `cockpit_camera` (`up` and `forward`, in metres) in `data/seasons.json`, and sets the TV pod cameras' near clip of a season with `pod_camera` (`near_clip`, in metres; stock 0.23 m on CamPod) (tools/cameras.py).
- Turns off `UseModelPositions` in every handling file, so the wheels sit at the handling file's positions instead of the model's (tools/handling.py).
- Sets every handling file to its team's pace (tools/pace.py): the stock 2025 teams' mean `F1Aero` wing coefficients and body drag, front and rear lift multipliers of 0.95, and `Chassis` `Mass` 798 kg plus the team's median qualifying gap to the season's fastest team (`data/performance.json`), less its AI slot's head start, at 0.0308 % per kg. `python3 tools/pace.py --years <range>` sets them on built cars.
- Gives every handling file the stock 2026 power unit (tools/powerunit.py): `Engine`, `Turbo`, `FuelTank`, `FuelECU` and `EnergyRecoverySystem`, with overtake mode and its per-track `TrackSettings`, and the R&D upgrades of those parts, all from the 2026 Ferrari, whose fuel modes add no power. Every season car has the same power unit. `python3 tools/powerunit.py --years <range>` gives it to built cars.
- Renames the tyre set (`shared/tyres/`) of a season with its own `common.erp` with the rest, and adds a `Tyres` override naming it (`tyre model`, `tyre wet model`, `material`, `wet material`) to every car and showroom model layer (tools/tyres.py). The car templates in `eaid://wote/types` name the stock tyre set.

```sh
python3 tools/build_cars.py --years 2016-2024
python3 tools/build_incardriver.py
python3 tools/gen.py
native/build.sh
```

The years build independently, so `--years` ranges can run in parallel. `build_cars.py` empties a season's `cars/`, so `build_incardriver.py` runs after it.

2016's Manor is `sauber.erp` in the downloads folder: fanfreluche's Manor car in the Sauber slot. It builds into `cx_2016_vehicle_package` under `teams/sauber/`, with the same passes and its shared references on the 2016 shared archive.

## Suits

A team-season of 2016 to 2021 wears, first found:

1. the real team suit of its own season;
2. the real team suit of the nearest season with the same team name in `data/seasons.json` (Force India and Racing Point are different teams), the earlier season on a tie;
3. its generated suit.

A real suit is a mod's suit in `data/real_suits.json` that `build_real_suits.py` has built, or a stock F1 25 suit of a season in its `stock_seasons` (2024: `2024-<stock team>` and `2024-<stock team>_<driver>`). Only team suits decide the season. Every driver of the team-season wears that season's kit: his own suit of that season and team when there is one, else that season's team suit. A mod driver suit brings its helmet. `"suit": {"source": "<label>"}` or `"source": "generated"` on a team in `data/seasons.json` sets the team's suit. The season contact sheets name the suit of every team and driver.

### Real suits

`build_real_suits.py --game <F1 25> --mods <folder holding the extracted mods>` builds each mod suit of `data/real_suits.json` into `suits/`:

- The label is `cs<yy>-<stock team>` (a team suit) or `cs<yy>-<stock team>_<driver slot>` (a driver suit), `<yy>` the suit's season. It has the length of the base label it replaces, `2025-<stock team>` or `2025-<stock team>_<driver slot>`.
- The archive holds every resource of the mod archive whose name holds the base label (a driver suit: the base team label), renamed byte for byte. The stock base archive holds the shared resources, and the asset group lists it first.
- A `.mipmaps` sidecar the mod ships, that holds the levels its texture names and that differs from the stock file goes to `suits/sidecars/` under the relabelled path. A sidecar the mod does not ship keeps its stock path when the texture's levels are the stock ones; otherwise the texture keeps only its embedded levels.
- A helmet archive takes the suit archive's copy of each resource both hold.
- `art/suits/real.png` shows each real suit's front and back render, `livery_v2_d` and `logo_01_d`.

| Mod archive | Season, team, driver | Label |
|---|---|---|
| F1 2016 - suits mod, `team_livery/2025-<team>/2025-<team>.erp` (10 teams) | 2016, the team the archive replaces | `cs16-<team>` |
| 2018 Redbull for F1 25, `team_livery/2025-redbull` | 2018 Red Bull Racing | `cs18-redbull` |
| 2018 Redbull for F1 25, `drivers/male/2025-redbull_max_verstappen` | 2018 Red Bull Racing, Max Verstappen | `cs18-redbull_max_verstappen` |
| 2018 Redbull for F1 25, `drivers/male/2025-redbull_yuki_tsunoda` | 2018 Red Bull Racing, Daniel Ricciardo | `cs18-redbull_yuki_tsunoda` |
| 2017 Suits for F1 2020, converted: `team_livery/2025-<team>` (10 teams) | 2017, the team | `cs17-<team>` |
| 2017 Suits for F1 2020, converted: `drivers/male/2025-<team>_<slot>` (20 drivers) | 2017, the team, the driver ([table](#suits-from-f1-2020-mods)) | `cs17-<team>_<slot>` |
| White Mercedes Driver Suits 1.01, converted: `team_livery/2025-mercedes` | 2019 Mercedes | `cs19-mercedes` |
| White Mercedes Driver Suits 1.01, converted: `drivers/male/2025-mercedes_<slot>` | 2019 Mercedes, Lewis Hamilton and Valtteri Bottas | `cs19-mercedes_andrea-kimi_antonelli`, `cs19-mercedes_george_russell` |

### Suits from F1 2020 mods

A mod of `data/real_suits.json` with `transfer` is an F1 2020 suit mod: `transfer` is its folder under `--mods`, and `folder` the folder `suit_transfer.py` writes its F1 25 conversion to. Each of its suits names the F1 2020 archive it comes from (`from`: `2020-<team>`, the team suit archive, or `2020-<team>_<driver>`, the driver archive under `drivers/male/`) and the F1 25 archive it becomes (`base`, `erp`). `build_real_suits.py` then builds the converted archives like any other mod.

- F1 2020's team suit archive (`2020_asset_groups/character_package/team_livery/<suit>/<suit>.erp`, zstd fragments, `.2020.mipmaps` sidecars) holds the old body `condition_scene/idf/driver_body_male.idf` (submeshes `livery`), the boots `<suit>_male.idf` (`boots`), the gloves `driver_gloves.idf` and the decal meshes `<suit>_body_logos_male.idf` and `<suit>_glove_logos_male.idf`. A driver archive holds the driver's decal meshes `<driver>_body_logos.idf` and `<driver>_glove_logos.idf`, boots `<driver>.idf`, usually its own `boots_d` and `glove_logo_d`, and copies of team textures. A texture comes from the driver archive's own, else its copy of the team's, else the team archive.
- The old and F1 25 bodies share one model space: the arm and glove vertex buffers of both games are identical.
- The old `livery_d` gets its decals baked in: each `livery_logo` triangle is drawn in its first texture coordinate pair (the body UV) with `logo_01_d` sampled at its second pair, alpha over the diffuse. `glove_d` gets the `gloves_logo` decals the same way.
- `livery_v2_d` (2048): each texel of F1 25's `livery_v2` UV layout (`driver_body_v2_male.idf` in the team suit archive) takes the 3D point and normal under it and samples the baked old diffuse (bilinear) at the nearest point of the old `livery` surface whose normal has a dot product of at least 0.2 with it. A point more than 5 cm nearer wins regardless of normal.
- A texel whose nearest point is more than 4 mm away, or whose old UV is more than 16 texels (or four times the island median) from the affine map from new to old UV fitted to its UV island's texels within 4 mm, takes that affine map, blended in up to 8 mm and twice the deviation. On these suits these are F1 25's cuffs, collar and zip. The median distance to the old surface is 1.4 mm, the 99th percentile 17 mm.
- Texels outside every UV island take the nearest covered texel.
- `boots_d` (1024 by 512): the same from the old boots to the `boots` submesh of `<base>_male.idf`, or of `<stem>.idf` for a driver suit.
- `glove_d` (1024): the baked old glove diffuse. F1 2020 and F1 25 share the glove mesh and UVs.
- A team suit's `logo_01_d` and `glove_logo_d` are transparent and its `logo_01_n` and `glove_logo_n` flat. A driver suit's own `livery_v2_d`, `boots_d` and `glove_d` carry the driver's decals; its copies of the team's `logo_01_*` and `glove_logo_*` are the converted team suit's. Every other texture, `livery_v2_n` and `livery_v2_s` included, keeps the F1 25 suit's.
- The map from new texel to old UV depends only on the two meshes. It is cached in `--cache` (default `build/suit-transfer`) by a hash of both meshes and the constants.
- A texture whose sidecar is missing from the mod uses its largest archive level. The White Mercedes mod ships no `glove_logo_d` sidecar: its glove decals come from the 64 px level.
- A driver suit's base is the F1 25 suit of the same driver at the stock team, else the team's next free F1 25 driver suit in name order.
- Of the stock F1 25 driver suits only Mercedes' have their own `livery_v2_d` and `glove_d`; the others wear the team's, and some the team's `boots_d`. A converted driver suit without its own texture gets a copy of its copy of the team texture and view under `drivers/male/<base>/textures/<stem>_<part>`, and the driver's materials (`drivers/male/<base>/materials/`) name it instead.

The 2017 mod ships its suits in F1 2020's driver slots. The driver of each slot, by the name, flag and number on the suit and the helmet:

| 2017 team | F1 2020 slot (`from`) | Driver | F1 25 base | Label |
|---|---|---|---|---|
| Mercedes | `2020-mercedes_lewis_hamilton` | Lewis Hamilton | `2025-mercedes_andrea-kimi_antonelli` | `cs17-mercedes_andrea-kimi_antonelli` |
| Mercedes | `2020-mercedes_valtteri_bottas` | Valtteri Bottas | `2025-mercedes_george_russell` | `cs17-mercedes_george_russell` |
| Red Bull Racing | `2020-redbull_alexander_albon` | Daniel Ricciardo | `2025-redbull_liam_lawson` | `cs17-redbull_liam_lawson` |
| Red Bull Racing | `2020-redbull_max_verstappen` | Max Verstappen | `2025-redbull_max_verstappen` | `cs17-redbull_max_verstappen` |
| Ferrari | `2020-ferrari_sebastian_vettel` | Sebastian Vettel | `2025-ferrari_charles_leclerc` | `cs17-ferrari_charles_leclerc` |
| Ferrari | `2020-ferrari_charles_leclerc` | Kimi Räikkönen | `2025-ferrari_lewis_hamilton` | `cs17-ferrari_lewis_hamilton` |
| Williams | `2020-williams_nicholas_latifi` | Felipe Massa | `2025-williams_alex_albon` | `cs17-williams_alex_albon` |
| Williams | `2020-williams_george_russell` | Lance Stroll | `2025-williams_carlos_sainz-jr` | `cs17-williams_carlos_sainz-jr` |
| Force India | `2020-forceindia_sergio_perez` | Sergio Pérez | `2025-forceindia_fernando_alonso` | `cs17-forceindia_fernando_alonso` |
| Force India | `2020-forceindia_lance_stroll` | Esteban Ocon | `2025-forceindia_lance_stroll` | `cs17-forceindia_lance_stroll` |
| Renault | `2020-renault_daniel_ricciardo` | Nico Hülkenberg | `2025-renault_franco_colapinto` | `cs17-renault_franco_colapinto` |
| Renault | `2020-renault_esteban_ocon` | Jolyon Palmer | `2025-renault_jack_doohan` | `cs17-renault_jack_doohan` |
| Toro Rosso | `2020-tororosso_pierre_gasly` | Carlos Sainz | `2025-tororosso_isack_hadjar` | `cs17-tororosso_isack_hadjar` |
| Toro Rosso | `2020-tororosso_daniil_kvyat` | Daniil Kvyat | `2025-tororosso_liam_lawson` | `cs17-tororosso_liam_lawson` |
| Sauber | `2020-sauber_kimi_raikkonen` | Marcus Ericsson | `2025-sauber_gabriel_bortoleto` | `cs17-sauber_gabriel_bortoleto` |
| Sauber | `2020-sauber_antonio_giovinazzi` | Pascal Wehrlein | `2025-sauber_nico_hulkenberg` | `cs17-sauber_nico_hulkenberg` |
| McLaren | `2020-mclaren_lando_norris` | Fernando Alonso | `2025-mclaren_lando_norris` | `cs17-mclaren_lando_norris` |
| McLaren | `2020-mclaren_carlos_sainz-jr` | Stoffel Vandoorne | `2025-mclaren_oscar_piastri` | `cs17-mclaren_oscar_piastri` |
| Haas | `2020-haas_romain_grosjean` | Romain Grosjean | `2025-haas_esteban_ocon` | `cs17-haas_esteban_ocon` |
| Haas | `2020-haas_kevin_magnussen` | Kevin Magnussen | `2025-haas_oliver_bearman` | `cs17-haas_oliver_bearman` |

### F1 23 suits

`build_real_suits.py --f1-23 <F1 23>` builds F1 23's own 2022 and 2023 kit into `suits/`: the 20 team suits and the 40 driver suits of `data/seasons.json`, each driver suit with its gloves and helmet. A 2022 or 2023 team-season chooses only among these, by the rules above; with none built, its looks keep their seats' stock kit, as 2024 does. The other seasons never take an F1 23 suit.

- The base is the F1 25 2024 suit of the same team (else 2025): a team suit `2024-<stock team>`, a driver suit the same driver's `2024-<stock team>_<driver>`, else a teammate's whose helmet shell is the F1 23 one. The label is `cs<yy>` and the base label from its fifth character, as for a mod suit.
- The base archive is relabelled byte for byte. Every material, bind and surface record is F1 25's.
- Each texture of a kit part (suit, boots, gloves, glove logos, logo atlas, suit attachments, harness, cap, helmet, visor) takes F1 23's texture of the same part: its mip chain, archive levels and `.mipmaps` sidecar levels, all embedded in the archive with no sidecar, in the F1 25 surface header with F1 23's format, size and level count. F1 23 packs the largest one or two sidecar levels of its bigger textures with compression 7, which no tool here reads; such a chain starts at the largest LZ4 or stored level (`livery_v2_d` 1024 instead of 2048). A texture whose F1 23 aspect differs is resized to the F1 25 size. A texture F1 23 lacks keeps F1 25's.
- A team texture a driver archive holds is the team suit's copy. A helmet archive takes the suit archive's copy of each resource both hold.
- `--logos f1-23` (the default) takes F1 23's body and glove logo meshes, renamed byte for byte to `cs<yy>-`, with the base's binds. A driver on a teammate's base gets them under `cs<yy>-<stock team>_<driver>/idf/<driver>_body_logos`, which his suit and gloves rows name. `--logos base` keeps the base's logo meshes.
- Helmet shells are byte for byte the same meshes in both games, so the helmet keeps the base's shell. 2022 Daniel Ricciardo (McLaren) and Sebastian Vettel (Aston Martin) wear Arai shells no F1 25 McLaren or Aston Martin driver has; their looks keep the stock helmet.
- `art/suits/2022.png` and `art/suits/2023.png` show each F1 23 suit's front and back render and F1 23's and the built `livery_v2_d`, `logo_01_d`, `glove_d` and `helmet_d`.

### Generated suits

`build_suits.py` builds one suit per team-season that wears no real suit, from an F1 25 team suit (the base) and the team's car:

| Field of the team's `suit` in `data/seasons.json` | Default |
|---|---|
| `base` | `2025-<stock team>`, the stock team of the team's `folder` |
| `design` | `tier01_livery76`, a customisable suit design whose panel masks paint the suit |
| `colours` | The team's `team_wear` in `data/seasons.json`, else its `team_wear` colours in `data/team_colours.json`; without them, three colours from the car's `paint_d` and opaque `decal_da` texels: the most common, then the most common that differ from those before |
| `sponsors` | The largest separate shapes of the car's `decal_da`, as `[x, y, w, h]` rectangles in pixels of the 2048 atlas |

- The suit's label is `cg<yy>-<stock team>`, or `cg<yy>-<key>` padded with `_` to the base label's length for a team with its own `key` (2016 Manor: `cg16-manor_`). It has the base label's length.
- The suit archive holds every resource of the base archive whose name holds the base label, renamed byte for byte. The shared body, glove and template resources stay in the stock base archive, which the suit's asset group lists before the suit archive.
- `livery_v2_d` (2048), `boots_d` (1024 by 512) and `glove_d` (1024) are the design's `_m` masks painted with the colours, as the game paints a customisable suit: white, then colour 1 by red, colour 2 by green and colour 3 by blue. The boots take darker colours.
- `logo_01_d` (1024) and `glove_logo_d` (512) are new decal atlases. Each decal of the base suit's logo meshes keeps its place on the body. Its atlas rectangle, the second texture coordinate pair of the decal mesh, gets the team logo (the two largest torso decals, and the largest near-square one), a sponsor, or nothing. A logo on a background of similar lightness turns white or black. The team logo is the brand art of `art/logos/<brand>/` or the stock team's mark from `teams.driver_select_team_logos`.
- `logo_01_n` and `glove_logo_n` become flat. Every other texture keeps the stock `.mipmaps` sidecar of the base suit.
- The contact sheet (`suits/preview.png`, and `art/suits/<year>.png` at half size) shows each team's front and back render (the male body, boots, gloves and decals), the painted `livery_v2_d` and the `logo_01_d` atlas with its decal rectangles: blue for the team logo, yellow for a sponsor, grey for an empty one.

```sh
python3 tools/suit_transfer.py --game <F1 25> --mods <folder holding the extracted mods> [--only <label>,...] [--cache <dir>]
python3 tools/build_real_suits.py --game <F1 25> --mods <folder holding the extracted mods> --f1-23 <F1 23>
python3 tools/build_suits.py --game <F1 25> --years 2016-2021 [--cars <folder holding classic-seasons-<year>/cars>]
python3 tools/gen.py
```

It needs numpy, Pillow, SciPy and lz4, zstandard for `suit_transfer.py`, `node` (BC7 decoding) and the season's built cars.

## Packages

```sh
for a in ../classic-seasons ../classic-seasons-20*; do python3 ../../../tools/pack_addon.py "$a" dist; done
```

The shared addon's package holds `heads/` and `native/classic_seasons.dll`: run `tools/heads.py` and `native/build.sh` before packing it.
