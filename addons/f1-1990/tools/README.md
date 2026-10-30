# Tools

## season.py

Reads the 1990 season pack (`season.json`, `drivers.json`) and the addon's team slots. Python 3.10+, standard library only. The pack folder is `/mnt/work/.f1-1990/source/1990`, or `$F1_1990_SOURCE`.

```sh
python3 addons/f1-1990/tools/season.py            # prints the slots and the round 1 line-up
```

```python
import sys; sys.path.insert(0, "addons/f1-1990/tools")
from season import Season, slots, slot, grid_lineup

s = Season.load()                  # or Season.load("/path/to/1990")
s.year, s.points, s.best_results   # 1990, {1: 9, ..., 6: 1}, 11
s.races                            # [Race(round, name, short_name, date, circuit)], by round
s.race(7)                          # Race 7, French Grand Prix
s.teams["mclaren"]                 # Team(id, name, principal, reputation, colour, prequalifying, car,
                                   #      power, weight, drag, reliability, seats, malus)
s.drivers["ayrton_senna"]          # Driver(id, name, first, last, nationality, country, birth_year,
                                   #        reputation, ratings)
s.absences                         # [Absence(round, team, out, into)], chained absences flattened
s.lineup(round)                    # {team id: [Seat(driver, number, races), Seat(...)]} with that round's absences applied
s.regulars([team ids])             # {team id: [Seat, Seat]}: each seat's driver with the most starts (races = starts)
s.stand_ins([team ids])            # [(driver, team id)]: drivers who started in those teams but aren't regulars
s.substitutions([team ids])        # absences for those teams
s.normalised(driver, rating)       # rating scaled to 0..1 over the pack's 44 drivers

slots()                            # the 13 teams in the addon: [Slot(team, team_row, vehicle, asset)]
slot("benetton"), slot("renault_26")
grid_lineup()                      # [(Slot, [Seat, Seat])] for the 13 teams: season regulars
grid_lineup(round=1)               # the same with one round's line-up
```

`power`, `weight` and `drag` are the pack's `power_scalar`, `weight_scalar` and `drag_scalar` with the sign flipped (Ferrari: power 1.08, weight 0.92). `reliability` is `-vehicle_reliability` (Ferrari 0.477). `malus` holds the raw values.

`nationality` is the game's nationality label (`britain`, `northern_ireland` for Donnelly); `country` is the pack's ISO code.

### Team slots

Each 1990 team takes one 2026 team row. `asset` is the car folder the vehicle row loads (`f1_2026_vehicle_package/teams/<asset>/`); `team12_2026` and `team13_2026` are Custom Grid Size's copies of the Williams and Haas rows, so they share those folders.

| 1990 team | Team row | Vehicle | Car folder |
|---|---|---|---|
| McLaren | `mclaren_26` | `mclaren_2026` | `mclaren` |
| Ferrari | `ferrari_26` | `ferrari_2026` | `ferrari` |
| Williams | `williams_26` | `williams_2026` | `williams` |
| Benetton | `renault_26` | `lotus_2026` | `lotus` |
| Tyrrell | `mercedes_26` | `mercedes_2026` | `mercedes` |
| Lotus | `redbull_26` | `redbull_2026` | `redbull` |
| Leyton House (`march`) | `forceindia_26` | `force_india_2026` | `force_india` |
| Larrousse | `team12_26` | `team12_2026` | `williams` |
| Ligier (`ligier_prost`) | `sauber_26` | `sauber_2026` | `sauber` |
| Minardi | `tororosso_26` | `toro_rosso_2026` | `toro_rosso` |
| Brabham | `gm_cadillac_26` | `gm_cadillac_2026` | `gm_cadillac` |
| Dallara | `team13_26` | `team13_2026` | `haas` |
| Arrows | `haas_26` | `haas_2026` | `haas` |

## check_fields.py

Checks that every row the addon copies carries every key of its stock source row, probing with every identifier string
in the exe as well as the known names (a name no list knows is a key the generators drop), then each found leaf under
the row's other objects of the same branch (keys the exe composes, such as the in-or-out-lap ERS level under both ERS
modes). Run it with the pinned Python (numpy):

```sh
re/tools/py addons/f1-1990/tools/check_fields.py <stock gamedatabases.2025.bin> <F1_25.exe> --built <built .bin>
```

`--built` takes a database built with the addon (for example the runtime edits of `swofty build` applied to the stock
file) and adds the rows `set` ops create (R&D trees, the `f1_1990` data set, setup mode and option tag). `--names <file>`
writes the names to add to `tools/gamedb/names.txt`.

## grid.py

Writes `db/grid.json`, `db/grid_standings.json` (the driver standings) and `lang/grid.json` from the season pack and the stock database. Every op adds a row or sets a field of an added row; no stock row changes, only the row counts of the tables it adds to.

```sh
python3 addons/f1-1990/tools/grid.py <stock gamedatabases.2025.bin> -v
```

| Part | Rows |
|---|---|
| 13 teams | `<team>_1990` (`mclaren`, `ferrari`, `williams`, `benetton`, `tyrrell`, `lotus`, `leytonhouse`, `larrousse`, `ligier`, `minardi`, `brabham`, `dallara`, `arrows`), stat_id 502–514: every field of the 2026 donor (Larrousse from Williams, Dallara from Haas) plus names, code, colours, base, nationality, story text, `livery_name` and `timing_tower_texture_path` = the row label, and a boss row `f1990_boss_<team>` |
| 31 drivers | `f1990_<driver>`, stat ids from the free low block and 235–253 (Driver Manager keeps 208–234): every field of a donor row (Senna, Prost and Mansell from their stock rows, the rest from 2026 and old F2 drivers) plus names, code, number, nationality, head, birth date (36 years later with `career_year` 2026), seasons, voice and AI traits; a statistics holder (one `default` data set), a look `f1990-<team>_<driver>` in the official folder (the team's suit and gloves from `db/kit.json`), and an entrant `f1990-<driver>` |
| Lists | `f1_1990` (the grid: 22 seats, 26 with `cars` = 26, `team`/`vehicle` = `<team>_1990`), `f1_1990_driver_pool` (the five stand-ins), `f1_career_1990` (references both); entrant setup mode `f1_1990` (`max_entrants` = `cars`); standings `f1_teams_1989` and `f1_drivers_1989` |
| Kits | `2026-f1990_<team>` in the 2026 team-customisation year, `linked_team` the 1990 row, the team's looks |
| Career data group | In the `f1_career_1990` rows the career lane clones: the team rows of accolades, primary sponsors, recognition and resource points move from `<team>_26` to `<team>_1990`, plus Larrousse and Dallara |

Field lists: the gamedb catalog plus every known field name present on the row (names.txt, the catalog, Driver Manager's driver fields). The catalog alone has 29 of a driver row's 129 fields.

`check_grid.py` checks a build against stock and a build without the grid ops.

### Ratings

The pack's 14 ratings are scaled to 0..1 across its 44 drivers (`Season.normalised`), then:

| F1 25 | From |
|---|---|
| experience | 0.62 + 0.33 × min(seasons before 1990 / 10, 1) |
| racecraft | 0.70 + 0.27 × mean(race_skill, defending, aggression) |
| awareness | 0.70 + 0.27 × mean(avoidance_of_mistakes, avoidance_of_forced_mistakes, blue_flag_conceding) |
| pace | 0.70 + 0.27 × mean(qualifying_skill, race_skill) |
| focus | 0.70 + 0.27 × mean(stamina, tyre_management, fuel_management, weather_tyre_changes) |
| trait aggression | 0.55 + 0.45 × aggression |
| trait control | 0.55 + 0.45 × mean(race_skill, wet_skill) |
| trait mistakes | 0.55 + 0.45 × mean(avoidance_of_mistakes, avoidance_of_forced_mistakes) |
| trait reaction_speed | 0.55 + 0.45 × start_reactions |
| trait speed_in | 0.55 + 0.45 × qualifying_skill |
| trait speed_out | 0.55 + 0.45 × mean(qualifying_skill, race_skill) |

Each rating curve is flat at its value. `consistency` is not used.

## kit.py

Writes the 1990 team suits and gloves: `kit/<label>.erp` (build output, not committed), `registry/kit.json` and
`db/kit.json`, and a preview sheet (front, back, `livery_v2_d` and `glove_d` of every team, `kit/preview.png` or
`--sheet`). Needs Pillow, numpy, scipy, lz4 and zstd.

```sh
python3 addons/f1-1990/tools/kit.py --game <F1 25> [--sheet <png>] [--teams mclaren,...]
```

| Part | What |
|---|---|
| Archive | `kit/f90-<team>.erp` (label padded with `_` to 13 characters: `f90-mclaren__`, `f90-ligier___`, `f90-march____`): every resource of the stock `2026-williams` suit that holds its label, renamed byte for byte. `livery_v2_d` holds the team's colour blocks (`DESIGNS`), painted per texel from the body's position; `glove_d` is the stock glove with its light cloth in the team's glove and cuff colours; `boots_d`, `cap_d` and `suit_attachments_d` are flat; `logo_01_d` has the team lockup on the two largest chest decals and the mark on the next, `glove_logo_d` the mark; the logo normals are flat. Replaced textures hold every level in the archive; normals, specular maps and the harness keep the `2026-williams` sidecars |
| `registry/kit.json` | a `suit` and a `gloves` record per team (`kit_<team>`): `label` the archive label, `archive`, `from` `game/2026-williams`, `team` the 2026 donor team, four colours. The core copies the `2026-williams` rows with ids above the stock ones and gives both one asset group, `team_livery/<label>/<label>`, listing the `2026-williams` archive first |
| `db/kit.json` | `suit` and `gloves` of every 1990 look `f1990-<team>_<driver>` (31) and team kit `2026-f1990_<team>` (13; Larrousse and Dallara with `cars` = 26) set to the team's label |

## faces.py

Turns face zips into driver heads and rebuilds every face of `faces/faces.json`. A zip holds `<driver>/faces_entry.json` (`source`, `mipmaps`, `base`, `code`; `<...>` placeholders are allowed) and `<driver>_head_d.tga.mipmaps` (BC1 levels 2048 to 256). Needs Pillow, numpy and zstd; uses Classic Seasons' `heads.py`, `bcn.py` and `head_sheet.py`.

```sh
python3 addons/f1-1990/tools/faces.py ~/Downloads/<driver>.zip [--base <head>] [--game <F1 25>]
python3 addons/f1-1990/tools/faces.py              # rebuild every face from faces/src/
python3 addons/f1-1990/tools/faces.py --bases      # scanned heads whose hair is painted from the face map
```

| Part | What |
|---|---|
| `faces/faces.json` | per driver: `source` (the zip), `mipmaps` (`src/<driver>_head_d.tga.mipmaps`), `base`, `code`, `label` (`f90_<driver>`). `base` is `--base`, else the zip's, else the recorded one, else `felipe_massa`; it must be a scanned head under `heads/male_driver/`. `code` is `f90_<driver>` cut or padded with `_` to the base's length |
| `faces/src/` | the zip's face map and preview, and `<driver>_sheet.png` (base face map, new face map, zip preview); not committed |
| `faces/heads/<label>.erp`, `faces/morph_poses/<label>.erp` | both archives of the base, `base` renamed byte for byte to `code`, the head_d surface replaced by the face map with all 10 levels and no sidecar; not committed |
| `faces/assets.json` | `file.put` of both archives and `group.register` of their groups, with `lando_norris`'s as templates |
| `db/faces.json` | an add-row of `<label>` from `male_ls222` (the archive's `model_path`, `has_morph_data` True, `available_for_customisation` False) and `head` of `f1990_<driver>` set to `<label>`, with the driver row's `when` gate. grid.py takes the same label from `faces/faces.json` |

A base without a `<head>_hair.idf` hair mesh paints its hair from the face map (its hair cap uses head_d). One with a hair mesh keeps its own hairstyle and hair colour.

## frontend.py

Writes `erp/frontend.json` and `erp/frontend/`: the Driver Select and Team Select backdrop names for the 1990 rows, added to `main_front_end.erp` as new resources. The backdrop is a texture picked by material context, the driver or team row label (`mfe_2025_main_menu_driver_select_name.emb?context=<label>`, `…vehicle_select_name.emb?context=<label>`). Per row: a texture `f1_drivername_f1990_d_NNN` / `f1_teamname_f1990_NN` (BC7, the template's size and mip layout, loose top mips under `names/f1990_drv_names~…` / `names/f1990_tmnames~…`), its SRV, a material and the context, each a copy of a stock one (`sonny_hayes`, `mercedes`) with the name swapped for one of the same length. Needs Pillow and lz4 (`nix-shell -p "python3.withPackages (ps: [ ps.pillow ps.lz4 ps.numpy ])"`) and rnd-tyres-thermal's `bc7enc.py`.

```sh
python3 addons/f1-1990/tools/frontend.py --game <game folder> [--baseline <launcher baseline>/<build>] [--preview <dir>]
```

## badges.py

Writes `erp/badges.json` and `erp/badges/badges.erpk` (the repo ignores `*.erp`): one slice per 1990 team appended to 22 team texture arrays, with one `overrides.uidb` entry per new slice. Keys: the team row `<team>_1990` (also its `livery_name`, `expression_label` and `timing_tower_texture_path`), `team_select_<team>_1990` in the career small logos and `laptop_<team>_1990_logo` in the laptop header. Each array has a style in `ARRAYS`, drawn by `logo_surfaces.py` from the art in `art/logos/` and the stock slices of the array: `card` (the logo on the team's card colour), `mp`, `fade` and `holding` (the stock gradient, profile-averaged without its logo and tinted), `room` (the stock fan zone room with its logo blurred out), `contract`, `menu` (the full logo across the slice, as a menu logo: Driver Select, Team HQ, R&D and the career team logos), `lockup` and `mark` (sized to the stock logos; in slices of 32 px and below Larrousse, Ligier and Dallara use the crop of their mark in `SMALL`: globe, French flag, BMS), `tint` and `pill` (stock backgrounds in the team colour) and `blank` (a copy of a stock F1 team's slice, for the arrays stock leaves blank). Card colours and logo variants are `CARD` in `logo_surfaces.py`. Stock slices stay as they are; every carrier archive grows, so the build writes them into the game files.

```sh
python3 addons/f1-1990/tools/badges.py --game <game folder> [--baseline <launcher baseline>/<build>] [--sheet badges.png]
```

`--sheet` writes a contact sheet: per array a stock slice, then the 13 encoded 1990 slices decoded back.

## car_numbers.py

Writes `erp/numbers.json` and `erp/numbers/numbers.erpk`: one slice in `overrides.drivers.driver_numbers` per 1990
car number (regular seats and stand-ins, from `grid.lineup_of`) that has no stock `nr_<number>` slice, with one
`overrides.uidb` entry per new slice. It exits when a car number is on two teams or a 1990 driver's number still
resolves to no slice. See README.md "Car numbers".

```sh
python3 addons/f1-1990/tools/car_numbers.py --game <game folder> [--baseline <launcher baseline>/<build>] [--sheet numbers.png]
```

`--sheet` writes every 1990 driver's card number as the built array resolves it, marked stock slice or 1990 slice.

## logo_art.py

Writes `art/logos/<team>/lockup_light.png`, `lockup_dark.png` and `mark.png` from `art/logos/src/`: the full logo for light and team-colour cards, the same with black or navy lettering turned white for the dark UI, and the compact symbol for square and small badges (the timing towers, broadcast lists and grid rundown). A team whose 1990 logo has no separate symbol uses its full logo as the mark: Williams the Canon Williams plate, Tyrrell its wordmark. Lotus's full logo is the Team Lotus roundel with CAMEL / TEAM LOTUS set in DejaVu Sans Bold on Camel yellow. Arrows keeps its navy and red: its dark full logo and its mark sit on a white plate. Ferrari's full logo and mark are the classic rectangular badge built from the shield's horse and the Ferrari script. Raster sources are traced per colour with potrace. Needs Pillow, numpy, resvg and potrace. `--check` exits 1 when a shipped file differs from a fresh build.

```sh
python3 addons/f1-1990/tools/logo_art.py [--check]
```

## hq_logos.py

Writes `erp/hq_logos.json` and `erp/hq_logos/`: per 1990 team and per HQ prop (`raceday_r_n_d_folder_a`, `raceday_r_n_d_notebook_a`), a diffuse texture (BC1, the stock unbranded texture's size and mip layout, top mips in `mipmaps/` for `add-mipmaps`), its SRV, a copy of the unbranded material pointing at it, and a copy of the `default` material context named `2026-<team>_1990`, all new resources in `f1_driver_hq.erp`. The texture is the unbranded one, recoloured in the team's primary colour where the stock Williams texture differs from it, with the team logo at the Williams logo (mirrored on the notebook). Needs Pillow, numpy and lz4.

```sh
python3 addons/f1-1990/tools/hq_logos.py --game <game folder> [--baseline <launcher baseline>/<build>] [--preview hq.png]
```

## tracks.py

Writes `erp/tracks.json` and `erp/tracks/`: a 1990 world `<track>_f1_1990.wep` in each track's `<track>_f1_26_myteam.erp`, which `grid.py`'s track layer `f1_1990` loads with the stock My Team asset group. The world is the My Team world listing its own copies of the layers Custom Grid Size widens (`…_f1990.layer`); with `cars` at 26 the copies get Custom Grid Size's 26-car edits. The stock worlds and layers stay as they are.

```sh
python3 addons/f1-1990/tools/tracks.py --game <game folder> [--baseline <launcher baseline>/<build>]
```

## check_grid.py

```sh
python3 addons/f1-1990/tools/check_grid.py <stock db> <db built without grid.json> <db built with it> --cars 22|26 [--lists <driver-manager cache>/game-lists.json]
```

Fails when a grid op changes a stock record other than a row count, or when a seat, entrant, driver, holder, look, list, setup mode, standings or data-group team row is missing or doubled. Lists the `<team>_1990` vehicle rows the build lacks (cars lane).

## build_cars.py

Writes `cars/handling/<ers>_<tyre_size>/<folder>_handling.erp` (one archive per 1990 car and option pair, holding its handling file under `eaid://f1_1990_vehicle_package/teams/<stem>/handling/<stem>.vtf`, and also under the folder name when the stem differs), `cars/handling.json` (the `file.put` ops keyed by `ers` and `tyre_size`) and `db/cars.json` (the `f1_1990` package row and the 13 vehicle rows). Reads the car table from `cars.py`, the team scalars through `season.py`, the vehicle field list from Custom Grid Size's `db/grid.json`, and the stock 2026 handling files from `--game`. Without `--write` it builds into `build/` and exits 1 when that differs from the shipped files. Tuning constants: `BASE_HP`, `BASE_MASS`, `SPREAD`, `FAMILIES`, `TYRES_13`, `BEAD_13`, `ALLOY_13`.

## tyres.py

`tyres.py --game <F1 25> [--check]` writes `tyres/<supplier>/tyrecompounds.erp` for `historical`, `goodyear` and `pirelli` and `tyres/compounds.json` (the `file.put` ops keyed by `tyre_supplier`). Each archive is the stock 2026 compound archive with one compound file, its 16 compounds written twice, as `F1_1990_*` (Goodyear teams) and `F1_1990P_*` (Pirelli teams), and the uso, sup and sof codes scaled per supplier (`GOODYEAR`, `PIRELLI`). With `--check` it exits 1 when that differs from the shipped files. The handling side, `TyrePhysicsGroup` per team, is `PIRELLI_TEAMS` in `build_cars.py`.

## cars.py

The 1990 car table: team, vehicle row (`<token>_1990`, the grid lane's tokens), performance tier, car folder (asset_filename and AI data), donor 2026 folder, resource stem, chassis, engine. Shared by `build_cars.py` and `carpack/tools/build_pack.py`.

## build_audio.py, audio.py

`build_audio.py --game <F1 25> [--write]` writes `db/audio.json`: four `audio.audio_bundle_settings` rows for the 1990 bundles and one `audio.audio_car_data.<label>` row per 1990 car, every field copied from its donor's stock row except `_metadata_` and `bundle_name`. It reads the donor labels from the stock 2026 carmodel layers and fails if any key it writes is already in the stock database. Without `--write` it builds into `build/` and exits 1 when that differs from the shipped file. `audio.py` holds the label rule (`label_1990`: the donor's label in other letter case, checked to keep its Wwise ID, `wwise_id`), the `switchName` reader and rewriter the car pack's `build_pack.py` uses, and the engine-to-bundle table.

## sessions.py

Writes `data/session-strings.json`, the 1990 session names per language (`FROM_STOCK` takes each language's stock
`lng_qualifying_1` / `lng_qualifying_2` texts, `FIXED` the two results headers), from the stock language tables.

```sh
python3 addons/f1-1990/tools/sessions.py <dir of language_<code>.tsv>
```

## rnd.py

Writes `db/rnd.json`, `db/rnd-lang.json`, `data/rnd-strings.json` and `data/rnd-art.json` from the stock database, the
season pack and the tables at the top of the file (`TOKENS`, `UPGRADES`, `SIZES`, `SUPPLIER_BHP`, `BUDGET`). `--check`
exits 1 when a shipped file differs from a fresh build.

Each tree's `performance_rating_base` and `performance_rating_fully_upgraded` are the 2026 McLaren tree's, scaled per
tree from 0.78 (slowest) to 1.05 (fastest) by the season pack's pace: the reputation tier (`TIER`) plus
power / weight / sqrt(drag), averaged over the tree's teams. The Vehicle Performance Comparison chart sums them per team:
Ferrari, McLaren, Benetton, Williams, Lotus and Larrousse (one tree), Tyrrell, Minardi, Leyton House, Arrows, Brabham,
Ligier, Dallara.

```sh
python3 addons/f1-1990/tools/rnd.py <stock gamedatabases.2025.bin> [--check]
```

## check_rnd.py

```sh
python3 addons/f1-1990/tools/check_rnd.py <stock db> <db built without rnd.json> <db built with it>
```

Fails when `rnd.json` changes a stock key other than the shared component row count, when a stock R&D key of the
2026, 2022 and My Team prefixes, the stock shared component sets or the 2025/2026 resource-point groups differs from
stock in the full build, when a key of the stock `shared_driver_career_26_components` row (row counts included) is
missing or different in `shared_driver_career_1990_components`, or when a 1990 tree, its vehicle list or its B-spec
upgrade is missing.
