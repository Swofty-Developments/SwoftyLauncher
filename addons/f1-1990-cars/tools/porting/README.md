# Car porting tools

The scripts that turn the Codemasters classic cars into the F1 25 car archives `build_pack.py` packages. They run from the working folder `/mnt/work/.mp45b`, whose paths they use, and they need inputs that are not in the repo:

- Jefry's porter, `F1modding3dtoolV1.dll`, run headless under Wine from `/mnt/work/.mp45b/porter` (`fixed/`, `dl/`, `dotnet-win/`).
- The classic cars, `1990s cars from 2019 and 2020.zip` (Jefry), extracted under `/mnt/work/.mp45b/classic`.
- The stock 2025 and 2026 car archives from a game copy, read only.

| Folder | Contents |
|---|---|
| `classic/` | `classic_layout.py` writes one glTF LOD folder per body: LOD 0–5 from the classic LODs and a LOD 6 of the LOD 5 body plus a `shadow` caster cut from LOD 3 (`--lods 0,1,2,3,4,5,5 --shadow-lod 3`). `zones.py` paints the flat colour liveries from `zones/`. `build_all.sh` builds the six teams with Codemasters liveries, `build_c7.sh` the seven zone-livery teams, and `build_lod6.sh` rebuilds all 13 layouts and 26 porter archives. `package_mix.sh` zips a mixed grid for manual installs. `cerp.py`, `gl.py`, `preview.py`, `render_hi.py` and `verify_teams.py` read, render and check the results. |
| `zones/` | The colour zones of the seven teams without a Codemasters livery. |
| `porter/` | `porter.sh` runs the porter through the harness (`export`, `patch`). `try_patch.sh`, `build_final.sh`, `package.sh` and `tex_to_dds.sh` are the single-car flow the pack started from. |
| `porter_a/` | `tryteam.sh` patches one team's 2026 archive from a glTF folder. `bspec_tex.py` makes the gold-rim B-spec wheel texture. `tyres_cm13.py` reshapes the stock 2026 tyre meshes onto the 13-inch bead at the 2026 outer radius. `donor_add.py`, `build_numbers.sh`, `try26.sh`, `package26.sh`, `package_grid.sh`, `porter_a.sh`, `porter_b2.sh` and `verify_grid.py` are the grid-wide runs and checks. |
| `harness/` | `PorterHarness`, the .NET 8 program that loads the porter DLL and calls its export and patch entry points without its window. |

`build_pack.py` rejects a car whose shadow caster has more than 40,000 triangles.
