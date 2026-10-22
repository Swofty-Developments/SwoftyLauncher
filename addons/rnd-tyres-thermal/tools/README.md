# Asset tools

These scripts generate the addon's `ui/` folder and `erp/rnd_fifth_pie.json` from the stock game archives: the 3+2 R&D > Performance overview tiles and the five-sector development-potential pie. Run them from the repository root. They import the repository's `tools/` and `tools/swofty-ui/` modules.

## Requirements

- Python 3.14, or an older Python with the `zstandard` module or the `zstd` command.
- numpy for `build_assets.py`, `build_card_art.py`, `make_bg5.py` and `render_preview.py`. `re/tools/py` runs a Python with numpy.
- `node` for BC7 decoding, and the `zstd` command for `validate.py`.
- `resvg` for `build_icons.py`, or Nix to fetch it.
- The stock UI archives. Run `swofty restore` first.

`--game` takes the F1 25 folder, or a folder holding stock copies of `flow_career_hub.erp`, `flow_team_hq.erp` and `persistent.erp`, plus `databases.erp` for `build_icons.py`. Without it, the tools use `F1_25_DIR`, then the launcher's game folder, then Steam.

## Rebuild the UI assets

```sh
re/tools/py addons/rnd-tyres-thermal/tools/build_assets.py --game "<F1 25>"
```

This builds into `addons/rnd-tyres-thermal/build/` and compares the result with the shipped `ui/` and `erp/rnd_fifth_pie.json`. It exits 1 if any file differs. Add `--write` to replace the shipped files with the build.

## Check the shipped assets

```sh
python3 addons/rnd-tyres-thermal/tools/validate.py --game "<F1 25>"
```

It prints `0 problem(s)` and exits 0 when everything passes. The two rebuilt archives go to `build/ui_package/`.

## Scripts

| script | does |
|---|---|
| `build_assets.py` | runs the steps below in order and compares the result with the shipped files |
| `make_laptop_r_and_d5.py` | adds the `focusable.tyres` tile to `screens/laptop_r_and_d.ui`, a clone of `focusable.aero` bound to `@fake` and attached to `tile_5`, and `entry_12` (bound to `pill_12`, with the per-row signals) to the three `performance_chart` lists, whose `viewHeight` becomes 13 |
| `make_rnd_overview5.py` | adds `tile_5` to `art/screens/career/rnd_overview.idf` and lays the tiles out 3+2; the shipped scene is `make_rnd_overview5.py stock out`, then `make_rnd_overview5.py out out2 --bottom fill` |
| `make_bg5.py`, `bc7enc.py`, `bc7dec.mjs` | build the 5-spoke `graph_b5` backdrop from the stock `graph_bg`, encode it as BC7 and decode BC7 with Swofty UI's `bc7.js` |
| `build_icons.py` | rasterises `../icons/tyres.svg` with `resvg`, appends the Tyres slices to `rnd_dept_icons_small`, `rnd_wheel_icon` and `rnd_department_strips` (plain and `rnd_om_xm`) in every UI archive that carries them, and writes `../icons/frames/` and `../erp/rnd_tyres_icon.json`; compares like `build_assets.py` |
| `build_card_art.py` | encodes `../art/out/` (listed by `../art/manifest.json`) with `bc7enc.encode_rgba`, appends the slices to `rnd_part_image`, `rnd_part_mask` and `rnd_category_icons` (plain and `rnd_om_xm`) in every UI archive that carries them, and writes `../art/arrays/` and `../erp/rnd_tyres_card_art.json`; compares like `build_assets.py` |
| `build_pie.py` | builds the pie scenes, the 72° meshes, the backdrop material and the `.ui` wiring, and writes `ui/` and `erp/rnd_fifth_pie.json` |
| `validate.py` | the offline checks under Validation |
| `render_preview.py` | composites the stock and five-sector pies the way the game draws them; `../preview/pie_compare.png` is its `pie_compare.png` |
| `scan_refs.py` | lists the UI resources whose bytes name a resource |
| `sites/` | the hook-site checks behind `../native/PIE_SITES.md`; run them with `EXE=<F1_25.exe> re/tools/py` |
| `common.py` | paths and stock-archive reads shared by the scripts |

## Design

![Stock pie and the five-sector pie](../preview/pie_compare.png)

| sector | department | VM name | colour (ABGR) | span, CCW from 3 o'clock |
|---|---|---|---|---|
| sector_1 | Chassis | `chassis` | 0xff3ec58d | 18°–90° |
| sector_2 | Durability | `maintenance` | 0xff16deff | 306°–18° |
| sector_3 | Powertrain | `engine` | 0xff7a2aed | 234°–306° |
| sector_4 | Aerodynamics | `aero` | 0xffeead00 | 162°–234° |
| sector_5 | Tyres & Thermal | `tyres` | 0xff0078ff | 90°–162° |

Clockwise from 12 o'clock: Chassis, Durability, Powertrain, Aero, Tyres. The stock four keep their stock order, sector indices and colours. Tyres sits between Aero and Chassis. Each sector is offset 0.068 along its bisector (gap 0.08, the stock gap). The backdrop keeps the stock hatch and outline. Five spokes on the sector boundaries replace its 4-arm cross.

There is no icon ring around the pie. `rnd_wheel_icon` is the atlas used by the tiles' `wheel_bg_icon` (`@progress_wheel_bg_icon_tex`), with frames aero, chassis, maintenance, engine, regulation and shutdown. The 5th tile (`tile_5`) gets its icon from the populate hook, which borrows `maintenance`.

![R&D > Performance overview with five tiles and the five-sector pie, in Swofty UI](../preview/rnd_overview_5tiles.png)

## How the stock pie is wired

- Scene chain: `rnd_overview.idf` `dev_potential` → `rnd_dev_potential_full.idf` `chart_1` → `rnd_dev_potential.idf` `sector_1..4` (eXRef) → `rnd_dev_potential_sector.idf` (`comp_potential`, `current_anim/current_nub`, `potential_anim/potential_nub`).
- The engine resolves an eXRef by its `xref` id, the md5-64 of the referenced scene path, not by `fileRef`.
- `.ui` (`tab_content_rnd_upgrades_alt.dev_potential`): one UIGP per department, attached by the chain `[chart_1, sector_N]`, `dataContext = focused_team_perf_root.department_performance.<dept>`. Its `trigger` command plays `goToFraction` on three UIXA children: `current` → `current_anim` (`perf_fraction`), `potential` → `potential_anim` (`potential_perf_fraction`), `max_potential` → `comp_potential` (`max_perf_fraction`). `dept_colour` tints comp/current/potential. `highlight_department` (MVCD on `focused_department.department_name`) highlights one group.
- Data: 0x1418914f0 publishes `department_performance.<dept>` per team. `Career_Populate_RnDPieSlices` (0x141f48030) builds the tile view-model children (`aero`, `chassis`, …: name, description, icon, colour). No native code looks up `sector_N`.
- `team_hq/research/dev_potential_full.idf` (the Team HQ research overview) also uses `rnd_dev_potential.idf`, so the stock pie, sector scene, meshes and textures stay untouched.

## What is built

The Performance page gets its own chain: `rnd_dev_potential_full.idf` (used only by `rnd_overview.idf`) points `chart_1` at a new pie scene.

| resource | kind | change |
|---|---|---|
| `art/screens/career/rnd_overview.idf` | UILayout | `tile_5` under `transition_null_8`; top row `tile_1..3` at scale 0.65, bottom row `tile_4`/`tile_5` at 0.999096, both rows span x 0.0036..8.4961 with 0.2 gaps |
| `screens/laptop_r_and_d.ui` | UIDF | the `focusable.tyres` tile, the pie wiring (below) and a 13th performance chart row, which shows when f1-1990 at 26 cars adds `pill_12` to the chart scenes |
| `art/components/career/rnd_tree/rnd_dev_potential_full.idf` | UILayout | `chart_1` `fileRef` and `xref` → `rnd_dev_potential5.idf`, `XrefMaterialBinding` → `rnd_dev_potential5.emb` |
| `…/rnd_tree/rnd_dev_potential5.idf` | UILayout (new) | stock pie + `sector_5`, 72° rotations and offsets, sectors' `fileRef` and `xref` → `rnd_dev_potential_sect72.idf`, `bg` material → `rnd_dev_potential5.emb` |
| `…/rnd_tree/rnd_dev_potential_sect72.idf` | UILayout (new) | stock sector scene, geoms → `…_sect72.idf/<elem>?context=render`, asset = md5-64(resourceid); anims use the stock clips |
| `…_sect72.idf/<elem>?context=render?render` ×3 | ModelProxyRes (new) | stock bytes |
| `…_sect72.idf/<elem>?context=render?model` ×3 | GfxModelRes (new) | stock bytes, mesh name → `_sect72` |
| `…_sect72.idf/<elem>?context=render&lod=0&mesh` ×3 | GfxMeshRes (new) | stock bytes, vb/ib names → `_sect72` |
| `…_sect72.idf/<elem>?context=render&lod=0&{vb=0,vb=1,vb=2,ib=0}` ×12 | GfxBufferRes (new) | 72° wedge in vb0/vb2, vb1/ib stock bytes |
| `…/rnd_tree/rnd_dev_potential5.emb?context=default` | GfxMatBindRes (new) | `graph_bg` → `art/materials/career/rnd_tree/rnd_dev_potential5_bg.material` |
| `art/materials/career/rnd_tree/rnd_dev_potential5_bg.material` | GfxMaterialRes (new) | stock `dev_potential_graph_bg.material`, texture → `graph_b5.tif` |
| `art/textures/team_hq/rnd/dev_potential/graph_b5.tif` (+`.image`) | GfxSRVResource, GfxSurfaceRes (new) | 232×232 BC7 (1 mip), 5-spoke backdrop |

`<elem>` is `comp_potential`, `current_nub` or `potential_nub`. Renamed strings keep the stock length (`sector` → `sect72`, `graph_bg` → `graph_b5`), so every offset inside the mesh, model, SRV and material is unchanged.

`.ui` changes on top of the stock screen:

- `focusable.tyres` TILE, a clone of `focusable.aero` attached to `tile_5`: `dataContext` `@tyres`, `department_focused` data `tyres`; `focusable.focused_department` gets a `tyres` event.
- `focusable` `trigger` also triggers `focusable.tyres`, which refreshes its fitted/total label and progress ring.
- `dev_potential.tyres` UIGP (clone of `aero`): `dataContext = focused_team_perf_root.department_performance.tyres`, attached by `[chart_1, sector_5]`, ids and signal targets rewritten.
- `dev_potential` `trigger` also triggers `tyres`. `highlight_department` gets a `tyres` event (highlight tyres, unhighlight the other four) and `tyres.unhighlight` on every other event.

## Mesh format (GfxMeshRes + GfxBufferRes, as used by the UI quads)

- GfxMeshRes (one fragment): dwords `1, 3 (vertex streams), 1, 1, 1`, then string offsets relative to 0x4c: vb0 name (0x58), ib name, slot name, end of slot name; 0x50 vertex count (4), 0x5c index count (6); 0x64 bounding sphere `(0.56, -0.0001, -0.56, r 0.792)`. Strings from 0xa4: the vb0, vb1, vb2, ib resource names, then the material slot name (`rnd_comp_potential`, `rnd_dev_current`, `rnd_dev_potential`), then a 3-entry vertex-element table (streams 0/1/2).
- GfxBufferRes: fragment 0 is 16 bytes `kind (0x109 vertex, 0x10a index), stride, count, flags (0x27 vertex, 0x13 index)`; fragment 1 is the raw data.
- vb0 POSITION float3, stride 12: the quad lies in the element's XZ plane at y = -0.0001; the element rotation (+90° about X) maps (x, z) to screen (x, -z). vb1 NORMAL float3 (0,1,0). vb2 TEXCOORD float2, stride 8: `u = x/1.12`, `v = 1 + z/1.12`. ib u16 `0 1 2 3 2 1`.
- The three wedge textures are quarter-disc masks (white, alpha) centred at UV (0,1), the sector origin; the colour is the runtime tint. The stock 90° wedge is the full 1.12 quad.
- GfxModelRes: bounds, node transform, the mesh resource name, the node name. ModelProxyRes: node transform and node name only. A geom's `asset` is md5-64 (big-endian) of its `resourceid` (`<scene>.idf/<elem>?context=render`).

72° wedge: only vertex 0 changes, position `(1.12·tan18°, -0.0001, -1.12)` = `(0.363910, …)`, UV `(tan18°, 0)` = `(0.324920, 0)`. The polygon is C(0,0), A(1.12,0), B(1.12,1.12), D(0.364,1.12) in screen space: the stock quad cut at 72°. The quarter-disc mask inside it is an exact 72° slice with the stock radius, UV convention and tint. Strides, counts, normals, index buffer and bounds are the stock ones.

## ERP operations

`erp/rnd_fifth_pie.json` holds 60 operations with sources relative to the addon folder. For each of `flow_career_hub.erp` and `flow_team_hq.erp`: 3 `replace-resource` (`rnd_overview.idf`, `laptop_r_and_d.ui`, `rnd_dev_potential_full.idf`) and 27 `add-resource`. All three replacements change size, so the launcher rewrites both archives.

## Validation

`validate.py` checks:

- Scenes: each re-parses and round-trips; `elementId == djb2(path)`; `nodeidlookup == sorted djb2(leaf names)`; `componentId == md5-64(path)`; geom assets match; every eXRef `xref` id is the md5-64 of its `fileRef`; every fileRef, resourceid, materialid and anim resolves. Three stock material bindings are missing in stock too and stay unchanged.
- Meshes: each new mesh is parsed like Swofty UI does, rasterised and sampled through its stock mask: 0.799 of the stock 90° area (ideal 0.800), angle span 0°–72°, radius 1.120 (stock 1.120).
- `.ui`: `uicheck` (ids, nameIds, layout nodeIds, event, command and signal hashes, targets).
- Archives: both archives rebuilt with `erpw`, 27 resources added, every untouched resource byte-identical.

The BC7 backdrop round-trips through `bc7.js` with max error 10/255, mean 0.21.

## Limits

- The Tyres values are derived (`../native/PIE_INTEGRATION.md`). The Tyres wedge is smaller than a full department's, and its attributes are also counted under Durability.
- `lng_rnd_dept_tyres_overview_description` exists in English only (`../assets/text.json`).
- `rnd_goto_tyres_department` (`dept_colour_anim`) has no stock animation.
- The Team HQ research overview keeps the stock four-sector pie.

## In-game check

1. Install and enable the addon (`../README.md`) and press Play. The launcher rewrites `flow_career_hub.erp` and `flow_team_hq.erp`.
2. Career → R&D → Performance shows 5 tiles (3 small and 2 wide, both rows the same width) and a 5-sector pie: Chassis top-right, Durability right, Powertrain bottom, Aero left, Tyres (orange) top-left, 0.08 gaps, backdrop spokes in the gaps, no cross.
3. The log has the `[rnd-tyres] pie5` lines listed in `../native/PIE_INTEGRATION.md` and `../native/HOOKS.md`.
4. Focusing each tile highlights the matching wedge. The Tyres tile highlights the orange wedge and shows the Tyres name and description.
5. Completing or progressing a Tyres upgrade grows the orange current wedge. The stock four are unchanged.
6. After the game exits, `swofty verify` reports no drift in either archive.
