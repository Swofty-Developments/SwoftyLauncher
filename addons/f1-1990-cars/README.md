# F1 1990 Cars (private)

The car models for F1 1990's `f1_1990` vehicle package. Private test content, not for release: the Codemasters classics (F1 2019/2020, from Jefry), ported with Jefry's F1modding3dtoolV1. Seven teams run a classic body recoloured in flat 1990 colour zones (base colours and the main sponsor blocks, no logos; specs in `/mnt/work/.mp45b/classic/zones/<team>.json`, built by `classic_layout.py --zones`). The archives are built locally and never committed (`erp/` is ignored).

What it adds, all under new paths, nothing replacing a game file:

- `2025_asset_groups/f1_1990_vehicle_package/teams/<folder>/wep/<folder>.erp`: the 13 cars, 13-inch rims, per-driver number plates, LOD 0–5 from the body's classic LOD 0–5, a far LOD 6 of the classic LOD 5 body plus a shadow caster cut from its classic LOD 3 (`classic_layout.py --lods 0,1,2,3,4,5,5 --shadow-lod 3`; `build_pack.py` fails on a shadow caster over 40,000 triangles), detail, wheel and steering-wheel textures of at most 1024 px, and the pod camera (T-cam) just ahead of each body's airbox with a 100° field of view (`POD_CAMERA` and `POD_FOV` in `tools/build_pack.py`; the offset and side pod cameras move with it and keep their own field of view).
- `2025_asset_groups/f1_1990_vehicle_package/teams/common.erp`: the package's shared resources, from the stock 2026 common.erp, with the 2026 tyre meshes reshaped for 13-inch rims (bead 0.175 m, outer radius 0.351 m front and 0.354 m rear, the stock 2026 radius; dry and wet, stock 2026 tyre materials and textures, no deformation data; `/mnt/work/.mp45b/porter_a/tyres_cm13.py`). The tyre set is renamed with the rest of the archive, to `eaid://f1_1990_vehicle_package/shared/tyres/`, and is the archive's only tyre set.
- `2025_asset_groups/f1_1990_vehicle_package/tyrecompounds/tyrecompounds.erp`: the 2026 compound file once, as `eaid://f1_1990_vehicle_package/tyrecompounds/f1_1990_compounds.vcmpd`, with its 16 compounds renamed from `F1_2026_*` to `F1_1990_*`. The `fom_car` handling files left in the car archives and in common.erp have `TyrePhysicsGroup` `F1_1990`.
- `2025_asset_groups/f1_1990_vehicle_b_specs/teams/<folder>/wep/<folder>.erp`: the 13 B-spec cars (the A body with gold rims, `gridc13b_*`). Their own resources carry the B package path; references to the shared resources and to the car's handling file point at the A package, so a B group lists the A common.erp and the A handling archive.
- Runtime asset groups (`assets/cars.json`): `asset_groups/f1_1990_vehicle_package/teams/<folder>/wep/<folder>.assetgroup` listing common.erp, the car archive and `<folder>_handling.erp` (shipped by `swofty.f1-1990`), and the tyre-compound group. Each takes its flags from the matching 2026 group.
- `db/ai.json` (written by `addons/f1-1990/tools/ai_data.py`): the `ai.asset_data.f1_1990_vehicle_package` rows and the switch of the `f1_1990` and `f1_1990b` packages to `ai_package_folder` `f1_1990_vehicle_package`: they read the pack's own AI archives.
- `videos/f1_1990_intro.bik`: the intro video that opens a new 1990 Driver Career (`native/f1_1990_intro.dll` in `swofty.f1-1990`, option `intro_video_1990`), from `video/f1_1990_intro.bik` (ignored, like `erp/`). Bring your own video: the build fails without that file.

| Team | Folder | Model | Source build |
|---|---|---|---|
| McLaren | mclaren | CM c_mclaren_90 | gridc13_mclaren |
| Ferrari | ferrari | CM c_ferrari_90 | gridc13_ferrari |
| Williams | williams | CM c_williams_92 | gridc13_williams |
| Benetton | lotus | CM c_benetton_94 | gridc13_lotus |
| Tyrrell | mercedes | CM c_jordan_91, Tyrrell colours | gridc13_mercedes |
| Lotus | redbull | CM c_mclaren_91, Lotus colours | gridc13_redbull |
| Leyton House | force_india | CM c_jordan_91, Leyton House colours | gridc13_march |
| Larrousse | myteam_ferrari | CM c_ferrari_90, Larrousse colours, on the Williams donor, stem `larrouss` | gridc13_larrousse |
| Ligier | sauber | CM c_mclaren_90, Ligier colours | gridc13_ligier_prost |
| Minardi | toro_rosso | CM c_jordan_91, Minardi colours | gridc13_minardi |
| Brabham | gm_cadillac | CM c_mclaren_91, Brabham colours | gridc13_brabham |
| Dallara | myteam_honda | CM c_williams_92, Dallara colours, on the Haas donor, stem `dall` | gridc13_dallara |
| Arrows | haas | CM c_ferrari_90, Arrows colours | gridc13_arrows |

## Building

```sh
python3 addons/f1-1990-cars/tools/build_pack.py            # all cars, common.erp, tyre compounds, assets/cars.json, assets/ai.json
python3 addons/f1-1990-cars/tools/build_pack.py --ops-only # only assets/cars.json, assets/ai.json and db/test.json
python3 addons/f1-1990-cars/tools/build_pack.py --bspec-only # only the B-spec archives
python3 addons/f1-1990-cars/tools/build_pack.py --common-only # only common.erp and the tyre compounds
```

`build_pack.py` renames each porter output from `eaid://f1_2026_vehicle_package/` to `eaid://f1_1990_vehicle_package/` byte for byte (same length), leaves the bare `.mipmaps` sidecar paths on the stock 2026 files, drops the car's own handling file (the f1-1990 addon ships it per option), and sets the AudioF1Car `switchName` in each carmodel layer to the car's 1990 engine audio label (`addons/f1-1990/tools/audio.py`: the donor's label in other letter case, such as `F1_M_mclaren_26`, which keeps the donor's Wwise switch state and names the car's own `audio.audio_car_data` row). Larrousse and Dallara sit on My Team folders for their AI data but were built on team donors: their resources carry a stem of the donor's length, and their World, event graph and layers are also stored under the folder name. Every instance of each carmodel and carshowroommodel layer, A and B-spec, gets a `Tyres` override naming the package's tyre set (`tyre model`, `tyre wet model`, `material` and `wet material`, `TYRE_PATHS` in `tools/build_pack.py`); the car templates (`F1CarModern2026`, `F1ShowRoomCarModern2026`) name the stock 2026 set. The build fails if any `eaid://f1_2026_vehicle_package/` reference or `f1_2026_vehicle_package/shared/tyres/` resource survives. Sources default to `/mnt/work/.mp45b/porter_a/tries` (`--tries`, `--common`, `--game`).

The intro video:

```sh
nix shell nixpkgs#ffmpeg nixpkgs#wine64 nixpkgs#xvfb-run nixpkgs#p7zip -c \
    addons/f1-1990-cars/tools/intro_video.sh <input video>   # writes video/f1_1990_intro.bik
```

`intro_video.sh` scales the input to 1280x720 (letterboxed; `WIDTH`, `HEIGHT`), resamples it to 29.97 fps and 48 kHz stereo, and encodes it as Bink 1 at `RATE` bytes per second (1500000) with `radvideo64.exe binkc` and `binkmix` under Wine. The game plays a `.bik` as Bink 1; Bink 2 encoding needs a licence. `RADVIDEO` points at an installed `radvideo64.exe`; otherwise the script downloads the free RAD Video Tools from radgametools.com into `~/.cache/f1-1990-intro`. RAD Video Tools are for non-commercial use only.

## AI data

`tools/ai/build_ai.py` writes the 1990 cars' per-track AI archives, `erp/ai/<track>/<folder>.erp`, for the 16 calendar tracks (`addons/f1-1990/tools/career.py` ROUNDS) and the 13 car folders. `assets/ai.json` registers them: per archive a `file.put` to `2025_asset_groups/environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.erp` and a `group.register` of `asset_groups/environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.assetgroup` with the flags of the matching `f1_2026_vehicle_package` group. A full `build_pack.py` run fails at the end when one of those archives is not built.

```sh
nix shell --impure --expr 'with import <nixpkgs> {}; python314.withPackages (p: [p.numpy p.scipy])' -c \
  taskset -c 2-7,10,11,14-31 python3 addons/f1-1990-cars/tools/ai/build_ai.py \
    --game "<F1 25 folder>" --swofty-ai <feat/swofty-ai checkout>/tools/swofty-ai
python3 addons/f1-1990-cars/tools/ai/build_ai.py --ops-only      # only assets/ai.json
python3 addons/f1-1990-cars/tools/ai/build_ai.py --report-only   # index.json, laps.json, checks.json from results.json
```

| Flag | Default |
|---|---|
| `--game` | `/mnt/gaming/SteamLibrary/steamapps/common/F1 25`, read only, with the stock `f1_2026_vehicle_package` track AI archives |
| `--swofty-ai` | `tools/swofty-ai` of this checkout (Swofty AI, branch `feat/swofty-ai`; Python 3.14, numpy, scipy) |
| `--handling` | `addons/f1-1990/cars/handling/removed_2026_1990` |
| `--tracks`, `--folders` | The 16 tracks, the 13 folders (comma-separated) |
| `--compounds` | `erp/tyrecompounds/tyrecompounds.erp`; when it is not built, the stock 2026 compounds renamed `F1_2026_*` to `F1_1990_*` |
| `--workers` | 12 |

Each archive is the stock 2026 archive of the same track and folder with every resource under `eaid://environment_package/tracks/<track>/ai/f1_1990_vehicle_package/<folder>/<folder>.<ext>`, in the stock order with the stock resource types:

| Resource | Contents |
|---|---|
| `.acpd .brpd .copd .crpd .thpd` | Swofty AI's car model (`carmodel.build_tables`, every axis) on the folder's 1990 handling VTF at the stock archive's downforce variation, in the stock tables' row order and axes (engine efficiencies, `x_mode`, copd and brake radii). Masses are the 1990 car's: light, VTF mass + 12 kg, + 110 kg fuel. Every `ers_mode` and `x_mode` row is the no-ERS row. Corner speeds are the stock 2026 copd of the folder × the model's 1990/2026 corner speed ratio at that radius, grip, mass row and tyre, the ratio held from the last radius where neither model car reaches its top speed, capped at the 1990 top speed. |
| `.vp_times` | Every set simulated with the 1990 tables on the track's stock AI line with the folder's stock brake line settings (`vp.build` with the plain set map Swofty AI uses for F2), the pit block on the stock pit route. Set masses are the 1990 car's (light × 1990/2026 mass ratio, VTF mass, VTF mass + 110 kg). Sample counts and the block hash are the stock file's. |
| `.fuel_burn_granular`, `.tyre_wear` | `strategy.build_fuel_burn` and `strategy.build_tyre_wear` with the 1990 tables on the stock line, F1 2026 fuel and wear coefficients. |
| `.ers_use_granular` | The stock layout, every sample 0. |
| `.ers_boost_data` | The stock entries with `baseERS`, `ers_delta_recorded`, `ers_amount` and `ers_burn_aim` 0, and `baseLapTime`, `lap_time_gain`, `baseFuelUsage` and `fuelUsage` scaled by the 1990/2026 lap and fuel ratio at full fuel. |
| `.vp_chars`, `.brakelinesettings` | The stock folder's. |

Outputs in `erp/ai/`, next to the archives:

- `index.json`: `track`, `folder`, `erp`, `target`, `group` and `template` of every archive written.
- `laps.json`, also written to `addons/f1-1990/data/ai-laps-1990.json`: per track and folder the simulated flying lap with the 1990 tables (`lap1990`) and with the stock 2026 tables (`lap2026`) on the stock line and brake line settings, grip 1.0, the tables' middle mass, difficulty 1.0, and `model_ratio`, both cars from the car model; per track the median 1990/2026 ratio and the fastest 1990 car.
- `checks.json`: resource set and types against the stock archive, table decoding and monotonicity, ERS rows, 1990 laps slower than 2026, corner speeds at 50/150/300 m and 0–200 km/h for McLaren and Haas at Melbourne, and the total size.
- `results.json`: the raw per-archive results `--report-only` reads.

The tool exits non-zero when a job fails or a check reports an issue.

## Test option

`test_grand_prix` (default `off`) puts 1990 cars on the F1 2026 Grand Prix grid for testing before the 1990 career exists: `ferrari` (the two Ferrari seats), `all` (all 22 seats, each on the 1990 car of the same AI folder), `myteam` (Larrousse on the Williams seats, Dallara on the Haas seats) or `bspec` (all 22 seats on the B-spec cars). Off leaves Grand Prix stock.
