# games-move

`move.py` moves the F1 25 modules of `swofty-core` under `packages/core/src/games/f1_25/` as a pure move. It lands
once, on the final 1.10 tip, after every other lane has merged.

`modules.txt` lists what moves: 16 module folders and `build_support.rs`. `games/f1_25.rs` becomes
`games/f1_25/mod.rs`, with a `pub mod` line for each module.

## What a run does

1. It refuses a tree with changes, and with `--expect <sha>` any tip but that one.
2. It rewrites the `pub(in crate::<module>)` visibility paths of the moved modules to `pub(in crate::games::f1_25::<module>)`.
3. It rewrites the `include_str!` and `include_bytes!` paths that point elsewhere once the files move. A path that
   still resolves is left alone.
4. It rewrites every `packages/core/src/<module>` path in the repository (code, docs, `native/`, `re/`, `tools/`)
   to `packages/core/src/games/f1_25/<module>`, and lists the lines that are code rather than prose.
5. It rewrites the `test(=<module>::…)` filters in `.config/nextest.toml` to `test(=games::f1_25::<module>::…)`.
6. It runs `git mv` for each module, drops their `pub mod` lines from `lib.rs`, and adds
   `pub use games::f1_25::{activation, build, …};` there, so `crate::erp` and `swofty_core::erp` still resolve.
7. It runs `cargo fmt --all`, then writes `packages/core/boundary.json` through the boundary test.
8. It stages everything. It does not commit.

A second run on a moved tree changes nothing. `--check` prints the plan, and exits 1 while anything is left to do
and 0 once the move is done.

## Run it

From `nix develop`, on the final tip:

```sh
python3 tools/games-move/move.py --check
python3 tools/games-move/move.py --expect <final tip> --verify
git diff --cached -M --stat
```

`--verify` runs `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`
and `cargo test --workspace` after the move. `--no-cargo` skips `cargo fmt` and `boundary.json`.

`git diff --cached -M --stat` shows the renames, `lib.rs`, `games/f1_25/mod.rs`, the include lines, the path
mentions and `boundary.json`, and nothing else.

## The boundary test

`packages/core/tests/boundary.rs` counts, per shell file of `swofty-core`, the `crate::<module>` paths into the
modules `modules.txt` lists, grouped `use crate::{…}` items included, against `packages/core/boundary.json`. It does
nothing until the modules are under `src/games/f1_25`. After the move, a file that names them more often fails the
test. Lower the counts after cutting an edge with:

```sh
SWOFTY_BOUNDARY_UPDATE=1 cargo test -p swofty-core --test boundary
```

## After the move

- Test names gain `games::f1_25::`, as in `games::f1_25::erp::tests::a_reused_encoder_writes_what_a_new_one_does`.
- The build hash of `swofty-core` (`build.rs`) hashes source paths, so the first Play after the update rebuilds the
  profile once, as after any release that changes the pipeline.
- Panic locations name the new paths.
