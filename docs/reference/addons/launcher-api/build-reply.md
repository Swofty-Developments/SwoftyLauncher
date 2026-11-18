---
title: The build reply
description: The asset ops a launcher plugin returns from build, with lines, warnings, items and fields.
---

# The build reply

A plugin that sets `"build": true` returns asset ops from `build`, and the launcher applies them with the profile's other ops.

## A build hook in full

The template writes the items that are on to a file in `cache_dir`, and puts it into the game:

```json [swofty.json]
"launcher": {
  "program": { "windows": "bin/example-helper.exe", "linux": "bin/example-helper" },
  "build": true
}
```

```rust [src/main.rs]
fn build(&self, host: &mut Host) -> Result<Value, String> {
    let on = self.enabled(host)?;
    let text = on.join("\n");
    let file = self.cache_dir.join("enabled.txt");
    // rewrite only on a change
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&file, &text).map_err(|e| e.to_string())?;
    }
    Ok(json!({
        "ops": [{
            "op": "file.put",
            "source": { "root": "cache", "path": "enabled.txt" },   // cache_dir/enabled.txt
            "target": "example-launcher-addon/enabled.txt"          // a path in the game folder
        }],
        "lines": [format!("{} items on", on.len())]
    }))
}
```

```text
launcher  {"id":4,"method":"build","params":{"options":{"size":24},"features":{"stable":["group","erp","db","lang","file"],"experimental":[]}}}
plugin    {"id":0,"method":"get_state","params":null}
launcher  {"id":0,"result":{"on":["okayama.exmod"]}}
plugin    {"id":4,"result":{"ops":[{"op":"file.put","source":{"root":"cache","path":"enabled.txt"},"target":"example-launcher-addon/enabled.txt"}],"lines":["1 items on"]}}
```

## The reply

```json
{
  "ops": [ { "op": "file.put", "source": { "root": "cache", "path": "enabled.txt" }, "target": "example-launcher-addon/enabled.txt" } ],
  "items": [ "okayama" ],
  "lines": ["1 item on"],
  "warnings": [],
  "fields": { "entrants.teams.team": ["_metadata_", "stat_id", "display_name_code"] }
}
```

Every key may be left out.

### ops

- Write each op exactly as you would in an [op file](../asset-api#op-files): set the `op` field to `<kind>.<name>`, such as `file.put`, `db.set`, `erp.patch-pkg`, `lang.set` or `warn`.
- For a `source` inside the addon folder, use a relative path (`"art/okayama.dds"`). Use `{ "root": "data" | "cache", "path": "…" }` to read from `data_dir` or `cache_dir`.
- The launcher tags every op with this addon's id as its origin, routes them like declared ops, and reports conflicts by origin ([Asset API](../asset-api#deliveries)).
- The build calls `build` for each enabled addon in load order, after that addon's declared ops.
- How these ops reach the game while it boots: [Changes at runtime](../runtime-assets#from-a-build-hook).

### features

`build.params.features` lists the op kinds the installed loader supports, `stable` and `experimental`: `group`, `erp`, `db`, `lang`, `file`. Check them before returning ops, and drop any op the loader cannot apply.

```json
{ "options": { "size": 24 }, "features": { "stable": ["file"], "experimental": [] } }
```

`options` holds the profile's value for every option your addon declares ([Settings](../settings)).

### lines and warnings

The build prints `lines` and `warnings` with the addon id as a prefix.

### items

When your addon loads several items, such as the mods in a library, `items` names the item each op comes from, by position in `ops`, with `null` for an op that belongs to no item:

```json
{ "ops": [ { "op": "file.put", … }, { "op": "file.put", … }, { "op": "lang.set", … } ],
  "items": [ "okayama", "okayama", null ] }
```

- The launcher adds the item to the op's origin, so each item gets its own runtime overlay archive and conflicts name it.
- Leave `items` out, or shorter than `ops`, and the remaining ops have no item.

### fields

*Launcher 1.0.12.* The field names a `db.add-row` copies, in the same shape as an [op file's `fields`](/reference/ops/database): an object keyed by `<table>.<row>`, each holding a list of field names.

```json
{
  "ops": [ { "op": "db.add-row", "table": "entrants.teams",
             "from": "entrants.teams.team[_metadata_=mclaren_26]",
             "set": { "_metadata_": "team12_26" } } ],
  "fields": { "entrants.teams.team": ["_metadata_", "stat_id", "display_name_code"] }
}
```

- `fields` applies to every `db.add-row` of your addon, from the reply and from your op files.
- An op file's own `fields` takes precedence for the tables it lists.
- An `add-row` whose table is in neither fails with `field names unknown`.
- A launcher before 1.0.12 ignores `fields`: set `"compat": { "launcher": ">=1.0.12" }` when you return `add-row` ops.

### reads

*Launcher 1.10.* The files and folders this build read. A launcher whose `hello` lists the `build.reads` capability keeps the reply and answers the next build with it, without starting your plugin, while nothing the reply depends on has changed:

```json
{
  "ops": [ { "op": "file.put", "source": { "root": "cache", "path": "grid-editor/build/grid-editor.json" },
             "target": "swofty/addons/teamsimplified.mods/grid-editor.json" } ],
  "lines": ["Grid Editor: 1 change(s), 0 name(s)"],
  "reads": [
    "C:/Users/me/AppData/Roaming/swofty/baseline/objects/2d/2db2c235…",
    "C:/Users/me/AppData/Roaming/swofty/addons/kizi.ai-driver-behaviour/db/racecraft.json",
    "C:/Users/me/AppData/Roaming/swofty/addons/swofty.swofty-seasons-2013-real/registry",
    "C:/Program Files (x86)/Steam/steamapps/common/F1 25/2025_asset_groups/ui_package/flow_race.erp"
  ]
}
```

```text
launcher  {"id":4,"method":"build","params":{"options":{…},"features":{…}}}
plugin    {"id":0,"method":"addons","params":null}
launcher  {"id":0,"result":[…]}
plugin    {"id":4,"result":{"ops":[…],"lines":[…],"reads":[…]}}
          … the next Play, with nothing changed: no session, no build call; the launcher applies the kept reply
```

The launcher reuses the kept reply when all of these are unchanged since the build that sent it:

- every path in `reads`: a file's size, modified time and change time; a folder's whole tree, every name in it and each file's size, modified time and change time; a path that did not exist still does not;
- the answer to every host call the plugin sent during that build, asked again with the same params;
- your addon's folder and its `data_dir`, file by file, as for a folder in `reads`;
- the `build` params, the `hello` the session started with, and the launcher's version.

Anything else runs `build` as usual: a changed or unreadable path, a reply without `reads`, a path changed less than 2 seconds before the build, or a build that sent `set_state`, `install_addon`, `set_addon_enabled`, `remove_addon` or `move_addon`. The kept reply is applied exactly as a fresh one: its ops, `items`, `lines`, `warnings` and `fields`, and the sources its ops name are read again.

- List absolute paths, each file and folder the build opened or listed, including a path it looked for and did not find. A path in `game_dir`, `data_dir`, `cache_dir`, another addon's folder or a `game_original` result counts the same.
- `progress`, `tasks` and `invalidate` notifications have no effect on reuse.
- A build that sends `reads` writes nothing outside `cache_dir`: on a reused build it does not run.
- Leave `reads` out and the launcher calls `build` on every Play. A launcher without `build.reads` ignores `reads`, so send it whatever `capabilities` lists.
- Each Play logs what the launcher did, and Send Logs carries the line: `[play] hook <addon> reused: <why>` when the kept reply answered the build, `[play] hook <addon> ran: <why>` when `build` ran, such as `ran: C:/…/racecraft.json changed` or `ran: its last reply listed no reads`.
- A Rust helper on `run_helper` sets `reads: Some(paths)` in the `AssetBatch` its `build` returns.
