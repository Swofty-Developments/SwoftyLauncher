# R&D: Tyres & Thermal Management

Adds a fifth Driver Career R&D department, Tyres & Thermal Management, alongside Aerodynamics, Chassis, Durability and Powertrain.

The addon is for Driver Career only. While it is on, the game won't start or load a My Team career: choosing My Team on the Career menu shows a "My Team not available" message and leaves you on the menu. To play My Team, turn the addon off in the launcher, or use a profile without it, and relaunch.

`swofty.rnd-tyres-thermal.zip` in this folder is the packaged addon: drop it on the launcher, or install it with `swofty addons install`. The launcher's own releases don't bundle it.

## State

Version 0.7.2 works in game:

- The Tyres department page lists its upgrades, which can be researched and developed, with Durability's research slots and R&D modifiers.
- R&D > Performance shows five department tiles (3+2) and a five-sector development-potential pie with an orange Tyres sector. The Tyres tile shows its fitted and total upgrades, highlights its sector and opens the Tyres page.
- The Develop Upgrade screen, the In Progress list, the department category rows, Team HQ popups and upgrade cards show Tyres upgrades in the Tyres orange.

Version 0.8.0 gives Tyres its own tyre icon on the R&D > Performance tile, the department box, the R&D lists and the Develop Upgrade screen, and an orange strip in the In Progress list. Without the icon arrays installed, Tyres borrows the Durability icon and strip as 0.7.2 did.

Version 0.9.0 adds:

- A full tree of 15 upgrades in four branches: Tyre Thermal, Brake Cooling, Tyre Life and Power Unit Cooling (`DESIGN.md`, "v0.9.0 — the full upgrade tree"). Costs, research and development days and facility levels follow the stock 2026 curve per size.
- Four new upgrade types, minted natively (ids 19-22), each with its own card label, `upgrade_type_metadata` row and handling block in every career car's VTF. Tyres upgrades never dilute Chassis' stock Tyre Wear or Brakes.
- Card art for every upgrade in the stock 2026 style (available, in progress, fitted and mask frames) and a category icon for each new type. Without the art arrays installed, the cards borrow stock part art and stock category icons.
- An `upgrade apply` log line per Tyres type when a car is built for a session.

Version 0.10.0 moves every engine hook onto the SDK's mixin bindings and needs Swofty Launcher 1.0.9 or later, whose core ships those bindings.

Version 0.10.1 checks game memory through the SDK's region cache. It still runs on Swofty Launcher 1.0.9 or later.

Version 0.11.0 blocks My Team. In My Team the addon's hooks made career creation and loading freeze at the season select until the game's hang watchdog closed the game. The addon now holds the core's career block on My Team (`swofty::career::block`, SDK API 7): the core cancels the Career menu route that starts or loads a My Team career and shows the game's own message popup. On a core older than API 7 the addon logs that My Team is not blocked and keeps working in Driver Career.

Version 0.12.0 adds the Tyres department to F1 1990 careers: the 15 upgrades in the 12 `upgrades.1990` trees of the F1 1990 addon, with the 1990 one-season costs and days and no facility levels, and the four type rows with the tyre supplier's Tyre Thermal and Tyre Life values (`db/tyres_tree_1990.json`, `db/upgrade_types_1990.json`). It loads after F1 1990, whose tree hubs and metadata row counts it overrides. The three team performance charts on the R&D laptop get a 13th row for the 26-car 1990 grid. 2025 and 2026 careers are unchanged.

`TESTING.md` lists what to check in game.

![Stock department icons and the Tyres icon](preview/tyres_icon_compare.png)

![Stock pie and the five-sector pie](preview/pie_compare.png)

![Stock and Tyres card art](art/preview.png)

## What it changes

| part | files | change |
|---|---|---|
| Database | `db/departments.json`, `db/tyres_tree.json`, `db/upgrade_types.json` | an `UpgradeDepartmentEntry_4` row (`department = Fake`, department code 4) cloned from Durability; the 15 upgrades in all 46 `trees.<team>.fake` containers, the branch roots linked from the tree hub; `upgrade_type_metadata_19..22` in all 46 metadata tables |
| F1 1990 | `db/tyres_tree_1990.json`, `db/upgrade_types_1990.json` | the same 15 upgrades in the 12 `upgrades.1990.trees.<token>.fake` hubs, linked after F1 1990's five roots; `upgrade_type_metadata_19..22` in the 12 1990 metadata tables. Only an F1 1990 career reads `upgrades.1990` |
| Text | `assets/text.json` | the department name and description, the four type labels, and the upgrade names and descriptions |
| UI | `erp/rnd_fifth_pie.json`, `ui/` | the five-tile R&D > Performance overview and the five-sector pie, in `flow_career_hub.erp` and `flow_team_hq.erp` |
| Icons | `erp/rnd_tyres_icon.json`, `icons/` | a `tyres` frame in `rnd_dept_icons_small` and `rnd_wheel_icon` and a `rnd_tyres_strip` frame in `rnd_department_strips`: one more slice in each texture array (`flow_career_hub.erp`, `flow_team_hq.erp`, `flow_r_and_d.erp`) and one more `overrides.uidb` entry (`databases.erp`) |
| Card art | `erp/rnd_tyres_card_art.json`, `art/arrays/` | 45 slices in `rnd_part_image`, 15 in `rnd_part_mask` and 4 in `rnd_category_icons`, plain and `rnd_om_xm`, in every UI archive that carries them (`common_flow_misc.erp`, `flow_career_hub.erp`, `flow_r_and_d.erp`, `flow_team_hq.erp`), and the matching `overrides.uidb` entries |
| Cars | `erp/rnd_upgrade_vtf.json`, `vtf/` | the four `radUpgrade*` blocks for the new types in 44 career handling VTFs (94 copies in 65 archives) |
| Engine | `native/rnd_departments.dll` | reads the fifth department row, gives the Tyres upgrades R&D states, mints the four upgrade types, names, colours, counts and routes the fifth tile and pie sector, picks the Tyres card art for each card state, and blocks My Team careers |

The engine hooks are mixins on knowledge-base bindings for the Steam build 25251167 (`F1_25.exe` d873a1c8…). If any binding doesn't resolve, the addon installs nothing.

## Settings

The addon has no settings. Turn it on or off per profile on the Addons page.

## Build the DLL

```sh
addons/rnd-tyres-thermal/native/build.sh
```

This writes `native/rnd_departments.dll`. It needs zig 0.16, or Nix to fetch it, and compiles against `packages/sdk/include` and the generated game bindings in `native/include` (`re/tools/bindgen.py`).

## Regenerate the UI assets

`ui/` and `erp/rnd_fifth_pie.json` are generated from the stock game archives. With the game restored to stock (`swofty restore`):

```sh
re/tools/py addons/rnd-tyres-thermal/tools/build_assets.py --game "<F1 25>"
python3 addons/rnd-tyres-thermal/tools/validate.py --game "<F1 25>"
```

`build_assets.py` rebuilds every file into `build/` and compares it with the shipped one. `--write` replaces the shipped files. See `tools/README.md` for each script, the pie design and the 72° wedge mesh format.

## Regenerate the icon frames

`icons/frames/` and `erp/rnd_tyres_icon.json` are built from `icons/tyres.svg` and the stock `databases.erp`, `flow_career_hub.erp` and `flow_team_hq.erp`. It needs `resvg` on `PATH`, or Nix to fetch it:

```sh
re/tools/py addons/rnd-tyres-thermal/tools/build_icons.py --game "<F1 25>"
```

It compares the build with the shipped files and exits 1 on a difference. `--write` replaces them.

## Regenerate the card art arrays

`art/arrays/` and `erp/rnd_tyres_card_art.json` are built from `art/out/` (see `art/STYLE.md`), `art/manifest.json` and the stock UI archives. It needs numpy and `node`:

```sh
re/tools/py addons/rnd-tyres-thermal/tools/build_card_art.py --game "<F1 25>"
```

It compares like `build_icons.py`; `--write` replaces the shipped files.

## Package

Build the DLL first, then:

```sh
python3 tools/pack_addon.py --zip addons/rnd-tyres-thermal addons/rnd-tyres-thermal
```

This writes `swofty.rnd-tyres-thermal.zip` with the manifest, the DLL and every file the operations name.

## Install into a profile

```sh
swofty addons install addons/rnd-tyres-thermal/swofty.rnd-tyres-thermal.zip
swofty --profile <profile> addons enable swofty.rnd-tyres-thermal
```

Use a modded profile and launcher 1.0.9 or later. The launcher applies the addon when you press Play.

The Tyres upgrades' progress lives in the profile's career saves. A save written while the addon is off drops that progress, so play a Tyres career only in a profile that keeps the addon on.

## Details

- `native/HOOKS.md`: every hook site and address, the department-byte sweep and the expected log lines for each version.
- `DESIGN.md`: the department design, the database containers, the upgrade tree, the card art and the version history.
- `TESTING.md`: the in-game test checklist and the log lines that prove each part.
- `native/PIE_INTEGRATION.md`, `native/PIE_SITES.md`: the pie's native half and its hook-site proofs.
- `tools/README.md`: the asset tools and the UI resources they build.
