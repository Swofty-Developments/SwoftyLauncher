# Tools

## The formats crate

`swofty-formats` (`packages/formats`) reads and writes the database.

`swofty_formats::gvdb`:

| Item | Purpose |
|---|---|
| `Gvdb::parse(&[u8]) -> Result<Gvdb>` | Parse a file. Fails on a missing terminator or trailing bytes |
| `Gvdb::to_bytes()` | Write, sorted stably by `key_hi`, re-masked. An unchanged database writes back byte for byte |
| `Gvdb::to_bytes_unsorted()` | Write in the current record order |
| `Gvdb::get(path)`, `get_key(u64)`, `get_text(path)`, `position(u64)` | Look up a record |
| `Gvdb::set(path, value, tag)` | Replace a value, or add a key when `tag` is given. Returns whether the key existed |
| `Gvdb::remove(path)` | Remove a record |
| `Gvdb::instance_count(container, row)` | Read `<container>._<row>_instancecount_` |
| `instance_count_path(container, row)` | Build that path |
| `Record { key, tag, value }`, `Record::new(path, tag, value)` | One record; `value` is the unmasked text |
| `ValueType::from_tag`, `ValueType::tag` | Tag numbers and names |
| `MASK`, `mask(plain, stream)`, `unmask(enc, stream)` | The value mask |

`swofty_formats::hash`: `fnv1a64(path)`, `fnv1a64_from(h, suffix)` (continue a hash, as in `{hash}` placeholders), `fnv1a64_unwind(h, suffix)` (peel a suffix off), `fnv1a32` (label lookups), `djb2`.

## The launcher's database stage

`swofty build` and `swofty apply` (see [the command line](/reference/cli)) write a new database when a profile has tracks or database operations:

1. Read the pristine database from the store.
2. For each enabled track, `activation::db::activate` adds the track's rows (registry: `packages/core/src/games/f1_25/activation/track_tables.json`).
3. For each addon in load order, `ops::apply_db_ops` applies its [database operations](/reference/ops/database), after `settings::render_ops` fills <span v-pre>`{{option}}`</span> placeholders and `when` conditions.
4. Write `game_package/gamedatabases/gamedatabases.2025.bin` into the build's overlay folder.

The build hash covers the pristine database, the registry and every rendered operation file. The journal installs the overlay file and restores the original.

## Python tools

`tools/` holds the research tools. They are the byte-identity oracle for the Rust code and are not shipped.

| Tool | Use |
|---|---|
| `tools/gamedb.py` | `roundtrip <db>`, `get <db> <path>`, `set <db> <path> <value> -o <out> [--type N]`, `dump <db> [-o json] [--names tsv] [--tree]` |
| `tools/gdb_hashsim.py <db>…` | Replays the game's hash-map load and prints the missing-key sentinel |
| `tools/track_keys.py names <exe>` | Prints the 8,286 names the exe's database readers use, stack-built ones included |
| `tools/track_keys.py keys <exe> <db> <track>` | Prints the keys a track's rows hold, found with those names |
| `tools/map_check.py <overlay> --game <dir> [--maps <dir>]` | Checks a profile built with tracks against the game it was built from, using the build's `plan.json`. `--maps` is the Team Simplified library (the default) or a folder of track packages |
| `tools/gamedb/catalog.py --game <dir>` | Writes the [table and key catalogue](/reference/gamedb#table-catalogue) |
| `tools/gamedb/deepkeys.py <row> --game <dir>` | Finds keys under a row that no catalogue names; see [Finding unnamed keys](#finding-unnamed-keys) |

`track_keys.py`, `catalog.py`, `deepkeys.py` and the reader-key part of `map_check.py` need `numpy`, `pefile` and `capstone`. On NixOS:

```sh
nix-shell -p 'python3.withPackages(ps: [ps.numpy ps.pefile ps.capstone])'
```

### How `catalog.py` names keys

1. It reads the names the exe's database readers use (`track_keys.exe_names`), the seed list `tools/gamedb/names.txt` and the run-time compositions in `tools/gamedb/compositions.txt`, with each `{a,b}` group expanded.
2. It finds tables bottom-up: for every key it peels `._metadata_` and a row digit; a hash reached from both row 0 and row 1 is a table prefix.
3. It walks top-down from every name: `<node>._<word>_instancecount_` finds a table and its row count, and `<node>.<name>` finds a field. New rows are walked again, which finds nested tables.
4. It names unresolved table prefixes by trying every row word against the count key, and attaches them to a known parent where a name links them.
5. It scans the exe's code for 64-bit immediates equal to a key.
6. It gives every table and key a meaning: the most specific pattern in `tools/gamedb/meanings.tsv`, or else one built from the name, the unit suffix (`_metres`, `_deg`, `Factor`) and the values (a field whose values are all labels of one table is a reference to it). It also records each numeric key's range.

Names it cannot reach stay unnamed. Adding names to `names.txt` and rerunning names more of the file.

### Finding unnamed keys

`deepkeys.py` hashes candidate paths `<row>.<w1>[.<w2>[.<w3>]]` and prints the ones the database holds. Each level takes its words from vocabularies, joined with `+`:

| Vocabulary | Words |
|---|---|
| `exe` | Identifiers in `F1_25.exe` (`strings -n 5`, about 340,000) and the `formats` words |
| `formats` | Identifiers `F1_25.exe` prints from a format string with one or two integer conversions (`%u`, `%d`, `%i`, `%x`, `%02d`), each conversion expanded to 0..31: `colour_%u_default` gives `colour_0_default` to `colour_31_default` |
| `catalog` | Path segments of the key catalogue, `names.txt`, `compositions.txt` and `row_keys.txt.gz` |
| `containers` | The `catalog` segments that come before another segment |
| `labels` | Identifier values of the database's id records: row labels and references to them |
| `table:<path>` | The labels of a table's rows, such as `table:car_setup.sliders.slider` |
| `file:<path>`, `list:a,b` | Words from a file, one per line, or the listed words |

| `--depth` | Default levels |
|---|---|
| 1 | `exe+catalog+labels` |
| 2 | `catalog+labels`, `exe+catalog+labels` |
| 3 | `containers`, `containers`, `catalog` |

`--depth N` searches paths exactly N levels below the row. `--level N=<vocabularies>` replaces level N. The tool hashes the row path once and continues the hash per word. When the last level has at most `--unwind-max` words (5,000), it unwinds every key of the file by each of them instead, and searches the earlier levels against the result, so a wide middle level costs one pass over its words per prefix. `--braces` prints a full product as one `compositions.txt` line.

The car setup presets of a vehicle package, found from the slider labels, the catalogue's container words and the exe's identifiers in 2.5 minutes:

```sh
python3 tools/gamedb/deepkeys.py --game <F1 25 folder> \
  "car_setup.presets.vehicle_package_setups[_metadata_=f1_2025_vehicle_package]" \
  --depth 3 --level 1=containers --level 2=exe --level 3=table:car_setup.sliders.slider --braces
```

```text
level 1: containers, 1673 words
level 2: exe, 342777 words
level 3: table:car_setup.sliders.slider, 42 words
car_setup.presets.vehicle_package_setups_7: 420 key(s) found, 0 of them already in the catalogue
{ai,player}.{balanced,max_downforce,max_speed,med_downforce,med_speed}.{ballast,brake_bd_factor_front,...}
```

Without `--braces` it prints one key per line and marks the ones the catalogue already names. `--exe-cache <file>` keeps the `exe` word list between runs; a cache written before the `formats` words joined it lacks them. To catalogue the keys found, add the line to `tools/gamedb/compositions.txt` and regenerate `row_keys.txt.gz` ([Key names](/reference/ops/database#key-names)) and the catalogue.

