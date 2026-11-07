//! The career index (docs/std/dependencies.md "Careers remember what they
//! were made with"). The core gives each career an id and a dependency
//! record, saved in its save entry and mirrored to
//! `$DATA/careers/index.json`, the launcher's data folder, which the launcher
//! names to the core in `<game>/swofty/state.json`, with the launcher profile
//! that started it. Before 1.6.0 the core wrote `<game>/swofty/careers/index.json`;
//! the launcher merges that file into the data folder's by career id and
//! removes it, so the list survives a restore of the game folder, and checks
//! each career's addons against a profile.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::range::Range;
use crate::manifest::AddonManifest;

pub const GAME_PATH: &str = "swofty/careers/index.json";
pub const DATA_PATH: &str = "careers/index.json";

/// The `why` of an addon recorded because its database rows were on when the career was saved.
pub const ROWS_WHY: &str = "database rows in the save";

/// The `why` of an addon recorded because a session of the career's save loaded one of its tracks.
pub const TRACK_WHY: &str = "a track its sessions loaded";

fn holds_rows(a: &CareerAddon) -> bool {
    matches!(a.why.as_deref(), Some(ROWS_WHY | TRACK_WHY))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CareerAddon {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub need: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Career {
    pub career: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    pub addons: Vec<CareerAddon>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// The careers a career index holds: `{ "careers": [...] }`, a list, or an object of records by
/// save key.
pub fn parse(text: &str) -> Vec<Career> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    let one = |key: Option<String>, v: Value| -> Option<Career> {
        let mut c: Career = serde_json::from_value(v).ok()?;
        if c.key.is_none() {
            c.key = key;
        }
        (!c.career.is_empty()).then_some(c)
    };
    match v {
        Value::Array(list) => list.into_iter().filter_map(|c| one(None, c)).collect(),
        Value::Object(mut map) => match map.remove("careers") {
            Some(Value::Array(list)) => list.into_iter().filter_map(|c| one(None, c)).collect(),
            Some(Value::Object(by_key)) => by_key.into_iter().filter_map(|(k, c)| one(Some(k), c)).collect(),
            _ => map
                .into_iter()
                .filter(|(_, c)| c.is_object())
                .filter_map(|(k, c)| one(Some(k), c))
                .collect(),
        },
        _ => Vec::new(),
    }
}

pub fn read(path: &Path) -> Vec<Career> {
    std::fs::read_to_string(path)
        .map(|t| parse(&t))
        .unwrap_or_default()
}

pub fn data_path(data: &Path) -> PathBuf {
    data.join(DATA_PATH)
}

fn retarget(v: &mut Value, from: &str, to: &str) -> bool {
    match v {
        Value::Object(map) => {
            let mut changed = false;
            if map.get("career").is_some() && map.get("profile").and_then(Value::as_str) == Some(from) {
                map.insert("profile".into(), Value::String(to.into()));
                changed = true;
            }
            for child in map.values_mut() {
                changed |= retarget(child, from, to);
            }
            changed
        }
        Value::Array(list) => list.iter_mut().fold(false, |c, i| retarget(i, from, to) | c),
        _ => false,
    }
}

/// Point every career of profile `from` in the data folder's index at profile `to`.
pub fn rename_profile(data: &Path, from: &str, to: &str) -> crate::Result<()> {
    let path = data_path(data);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(mut index) = serde_json::from_str::<Value>(&text) else {
        return Ok(());
    };
    if retarget(&mut index, from, to) {
        crate::write_json_shared(&path, &index)?;
    }
    Ok(())
}

/// Merge the game folder's career index, written before the core used the data folder, into the
/// launcher's copy, newest record per career id first, and return the merged list. A career the
/// game no longer lists stays in the copy. A career the core wrote without a profile gets `stamp`,
/// the profile of the session that just ran.
pub fn sync(data: &Path, game: Option<&Path>, stamp: Option<&str>) -> crate::Result<Vec<Career>> {
    let mut kept = read(&data_path(data));
    let mut changed = false;
    let fresh = game.map(|g| read(&g.join(GAME_PATH))).unwrap_or_default();
    for mut c in fresh {
        changed = true;
        match kept.iter_mut().find(|k| k.career == c.career) {
            Some(k) => {
                c.profile = c.profile.or_else(|| k.profile.clone());
                for a in k.addons.iter().filter(|a| holds_rows(a)) {
                    if !c.addons.iter().any(|o| o.id == a.id) {
                        c.addons.push(a.clone());
                    }
                }
                *k = c;
            }
            None => kept.push(c),
        }
    }
    if let Some(stamp) = stamp {
        for c in kept.iter_mut().filter(|c| c.profile.is_none()) {
            c.profile = Some(stamp.to_owned());
            changed = true;
        }
    }
    if changed {
        crate::write_json_shared(
            &data_path(data),
            &serde_json::json!({ "version": 1, "careers": kept }),
        )?;
    }
    Ok(kept)
}

/// A restore puts a snapshot's careers back as the profile's list: the careers of `profile` (and the
/// ones with no profile yet) in the index are replaced by those of `restored`, with their own details
/// and last-played times, until the core lists the profile's careers again at the next Play. Other
/// profiles' careers stay.
pub fn replace_profile(data: &Path, profile: &str, restored: &[Career]) -> crate::Result<()> {
    let mine = |c: &Career| c.profile.as_deref().is_none_or(|p| p == profile);
    let mut list: Vec<Career> = read(&data_path(data)).into_iter().filter(|c| !mine(c)).collect();
    list.extend(restored.iter().filter(|c| mine(c)).cloned());
    crate::write_json_shared(
        &data_path(data),
        &serde_json::json!({ "version": 1, "careers": list }),
    )
}

/// Where the launcher keeps the core's stat-id store: the one store of the install.
pub fn kept_ids(data: &Path) -> PathBuf {
    super::reports::path(data, super::reports::IDS)
}

/// Move what the core wrote in the game folder before it used the data folder: the career index
/// (merged), the stat-id store (the newer of the two wins) and the reports (when the data folder
/// has none). Returns one line for each thing moved.
pub fn migrate(data: &Path, game: &Path) -> crate::Result<Vec<String>> {
    use super::reports;
    let mut out = Vec::new();
    let old_index = game.join(GAME_PATH);
    if old_index.is_file() {
        sync(data, Some(game), None)?;
        std::fs::remove_file(&old_index).ok();
        out.push("career index".to_owned());
    }
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    for name in reports::SENT.iter().copied().chain([reports::IDS]) {
        let old = reports::legacy_path(game, name);
        if !old.is_file() {
            continue;
        }
        let new = reports::path(data, name);
        let take = !new.is_file() || (name == reports::IDS && modified(&old) > modified(&new));
        if take {
            let dir = new.parent().expect("has parent");
            std::fs::create_dir_all(dir).map_err(crate::io_err(dir))?;
            std::fs::copy(&old, &new).map_err(crate::io_err(&new))?;
            out.push(name.to_owned());
        }
        std::fs::remove_file(&old).ok();
    }
    Ok(out)
}

/// Before the game folder's `swofty/` goes (Restore game files): move anything the core left there
/// into the data folder, so nothing is lost. The data folder is not touched by the restore.
pub fn keep(data: &Path, game: &Path) -> crate::Result<()> {
    migrate(data, game).map(|_| ())
}

/// What a career needs that profile `order` doesn't have on: one line per addon, empty when it
/// loads in that profile.
pub fn missing(c: &Career, order: &[String], installed: &BTreeMap<String, AddonManifest>) -> Vec<String> {
    let mut out = Vec::new();
    for a in &c.addons {
        let name = installed
            .get(&a.id)
            .map_or(a.id.clone(), |m| format!("{} ({})", m.name, a.id));
        let range = a
            .need
            .as_deref()
            .and_then(|n| Range::parse(n).ok())
            .unwrap_or_else(Range::any);
        match installed.get(&a.id) {
            None => out.push(format!("needs {name}, which isn't installed")),
            Some(_) if !order.contains(&a.id) => out.push(format!("needs {name}, which is off")),
            Some(m) if !range.matches(&m.version) => {
                out.push(format!("needs {name} {range}, and {} is installed", m.version))
            }
            Some(_) => {}
        }
    }
    out
}

fn saved_ms(c: &Career) -> Option<i64> {
    let n = |name: &str| c.extra.get(name).and_then(Value::as_i64);
    n("last_saved").or_else(|| n("last_played").map(|s| s * 1000))
}

/// The careers of `list` in the save `profile` boots: the ones made under a profile that boots the
/// same save (its own launcher save, or the shared Steam save), and the ones with no profile yet.
pub fn in_save_of<'a>(list: &'a [Career], data: &Path, profile: &str) -> Vec<&'a Career> {
    let mut sources: BTreeMap<&str, Option<String>> = BTreeMap::new();
    let mine = crate::vault::Location::source_of_profile(data, profile);
    list.iter()
        .filter(|c| match c.profile.as_deref() {
            None => true,
            Some(p) => {
                *sources
                    .entry(p)
                    .or_insert_with(|| crate::vault::Location::source_of_profile(data, p))
                    == mine
            }
        })
        .collect()
}

/// Records on every career in the save of `profile` saved at or after `since_ms` (or with no save time) each
/// addon of `on` that owns vehicle, engine, team, driver or head rows, since the game reads those
/// rows when it loads the save. Returns whether any career changed.
pub fn record_rows(
    list: &mut [Career],
    in_save: &dyn Fn(&Career) -> bool,
    since_ms: i64,
    on: &[String],
    owners: &BTreeSet<String>,
    installed: &BTreeMap<String, AddonManifest>,
) -> bool {
    let ids: Vec<&String> = on.iter().filter(|id| owners.contains(*id)).collect();
    record(list, in_save, since_ms, &ids, ROWS_WHY, installed)
}

/// The addons whose tracks a session loaded, from the `[apply] facts` lines of its loader.log
/// (`track <addon>/<code>`; the game's own tracks name no addon).
pub fn session_tracks(loader_log: &str) -> BTreeSet<String> {
    loader_log
        .lines()
        .filter_map(|l| {
            let rest = l.split("[apply] facts: ").nth(1)?;
            let track = rest.rsplit_once(", track ")?.1.trim();
            let (addon, _) = track.split_once('/')?;
            (addon != "game" && !addon.is_empty()).then(|| addon.to_owned())
        })
        .collect()
}

fn record(
    list: &mut [Career],
    in_save: &dyn Fn(&Career) -> bool,
    since_ms: i64,
    ids: &[&String],
    why: &str,
    installed: &BTreeMap<String, AddonManifest>,
) -> bool {
    let mut changed = false;
    for c in list.iter_mut() {
        if !in_save(c) || c.is_gone() || saved_ms(c).is_some_and(|t| t < since_ms) {
            continue;
        }
        for id in ids {
            if c.addons.iter().any(|a| &a.id == *id) {
                continue;
            }
            c.addons.push(CareerAddon {
                id: (*id).clone(),
                version: installed.get(*id).map(|m| m.version.to_string()),
                need: None,
                why: Some(why.to_owned()),
            });
            changed = true;
        }
    }
    changed
}

/// After a session of `profile` that started at `since_ms`: records the database addons that were on
/// ([`record_rows`]) and the addons whose tracks the session loaded ([`session_tracks`]) in the
/// data folder's career index.
pub fn record_session(data: &Path, profile: &str, since_ms: i64, loader_log: &str) -> crate::Result<()> {
    let mut list = read(&data_path(data));
    if list.is_empty() {
        return Ok(());
    }
    let store = crate::store::Store::new(data);
    let (Ok(p), Ok(installed)) = (store.profile(profile), store.addons()) else {
        return Ok(());
    };
    let Ok(plan) = crate::resolver::resolve(&p, &installed) else {
        return Ok(());
    };
    let owners = crate::row_ids::save_owners(data);
    let ids: BTreeSet<String> = in_save_of(&list, data, profile)
        .into_iter()
        .map(|c| c.career.clone())
        .collect();
    let in_save = |c: &Career| ids.contains(&c.career);
    let tracks: Vec<String> = session_tracks(loader_log)
        .into_iter()
        .filter(|id| installed.contains_key(id))
        .collect();
    let mut changed = record_rows(&mut list, &in_save, since_ms, &plan.order, &owners, &installed);
    changed |= record(
        &mut list,
        &in_save,
        since_ms,
        &tracks.iter().collect::<Vec<_>>(),
        TRACK_WHY,
        &installed,
    );
    if changed {
        crate::write_json_shared(
            &data_path(data),
            &serde_json::json!({ "version": 1, "careers": list }),
        )?;
    }
    Ok(())
}

/// The Play stop for careers whose saved database rows come from an addon that is off or not
/// installed in profile `order`: the game reads every career in the save when it starts, so it
/// crashes whichever career is picked. `None` when no such addon is missing.
pub fn stop_line(
    list: &[Career],
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
) -> Option<String> {
    let mut addons: BTreeMap<&str, String> = BTreeMap::new();
    let mut careers: Vec<&str> = Vec::new();
    for c in list.iter().filter(|c| !c.is_gone()) {
        let gone: Vec<&CareerAddon> = c
            .addons
            .iter()
            .filter(|a| holds_rows(a))
            .filter(|a| !order.contains(&a.id) || !installed.contains_key(&a.id))
            .collect();
        if gone.is_empty() {
            continue;
        }
        for a in gone {
            let name = installed.get(&a.id).map_or(a.id.clone(), |m| m.name.clone());
            let state = if installed.contains_key(&a.id) {
                "off"
            } else {
                "not installed"
            };
            addons.insert(&a.id, format!("{name} ({state})"));
        }
        careers.push(if c.label.is_empty() { &c.career } else { &c.label });
    }
    if addons.is_empty() {
        return None;
    }
    let names = addons.values().cloned().collect::<Vec<_>>().join(", ");
    let shown = careers.iter().take(3).copied().collect::<Vec<_>>().join("; ");
    let more = careers.len().saturating_sub(3);
    let more = if more > 0 {
        format!("; {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "The game won't start with {names}: {} in your save use its database rows ({shown}{more}). \
         Turn {} back on and press Play, or restore a save from before in Profiles > Saves.",
        match careers.len() {
            1 => "1 career".to_owned(),
            n => format!("{n} careers"),
        },
        if addons.len() == 1 { "it" } else { "them" }
    ))
}

/// The careers in `list` that list `addon` among the addons they need and are still in the save:
/// the careers turning `addon` off would stop from loading.
pub fn needing<'a>(list: &'a [Career], addon: &str) -> Vec<&'a Career> {
    list.iter()
        .filter(|c| !c.is_gone())
        .filter(|c| c.addons.iter().any(|a| a.id == addon))
        .collect()
}

/// The one Play line for the careers that won't load in profile `order`: how many, and the first
/// few with the addon each needs. `None` when every career loads.
pub fn play_line(
    list: &[Career],
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
) -> Option<String> {
    let short: Vec<(&Career, Vec<String>)> = list
        .iter()
        .filter(|c| !c.is_gone())
        .map(|c| (c, missing(c, order, installed)))
        .filter(|(_, m)| !m.is_empty())
        .collect();
    if short.is_empty() {
        return None;
    }
    let shown = short
        .iter()
        .take(3)
        .map(|(c, m)| {
            let name = if c.label.is_empty() {
                c.career.as_str()
            } else {
                c.label.as_str()
            };
            format!("{name} {}", m[0])
        })
        .collect::<Vec<_>>()
        .join("; ");
    let more = short.len().saturating_sub(3);
    let more = if more > 0 {
        format!("; {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "{} won't load in this profile: {shown}{more}",
        match short.len() {
            1 => "1 career".to_owned(),
            n => format!("{n} careers"),
        }
    ))
}

impl Career {
    /// Whether the career is gone from the save: missing from the last profile load or deleted in game.
    pub fn is_gone(&self) -> bool {
        matches!(self.status(), "missing" | "deleted")
    }

    /// `present` unless the core marked the career missing from the last profile load.
    pub fn status(&self) -> &str {
        self.extra
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("present")
    }

    /// The text field the core wrote beside the dependency record, such as `mode` or `driver`.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.extra
            .get(name)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECORD: &str = r#"{
      "career": "6f3e9a",
      "profile": "progressive",
      "label": "Progressive Career: Max Verstappen, 2016, season 1",
      "match": { "driver": 4103, "team": 2, "season_index": 0 },
      "made_with": { "launcher": "1.6.0", "core": "0.3.0", "game_build": 25251167 },
      "addons": [
        { "id": "swofty.progressive", "version": "1.3.0", "need": "^1.3.0", "why": "career content f1_career_progressive" },
        { "id": "mod.sochi", "version": "1.1.0", "need": "^1.1.0", "why": "calendar round 4: track mod.sochi/sochi" }
      ],
      "records": [ { "ref": "track:venue/sochi", "resolved": "mod.sochi/sochi", "stat_id": 18 } ]
    }"#;

    fn manifest(id: &str, version: &str) -> AddonManifest {
        serde_json::from_str(&format!(
            r#"{{ "schema_version": 1, "id": "{id}", "name": "N {id}", "version": "{version}", "author": "a",
                 "compat": {{ "game": "f1_25" }} }}"#
        ))
        .unwrap()
    }

    #[test]
    fn reads_every_shape_of_the_index() {
        let by_key = format!(r#"{{ "f1_career_progressive/pg-1849223-max_verstappen": {RECORD} }}"#);
        let c = parse(&by_key);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].career, "6f3e9a");
        assert_eq!(c[0].profile.as_deref(), Some("progressive"));
        assert_eq!(
            c[0].key.as_deref(),
            Some("f1_career_progressive/pg-1849223-max_verstappen")
        );
        assert_eq!(c[0].addons[1].need.as_deref(), Some("^1.1.0"));
        assert!(c[0].extra.contains_key("made_with"));
        assert_eq!(parse(&format!(r#"{{ "careers": [{RECORD}] }}"#)).len(), 1);
        assert_eq!(parse(&format!("[{RECORD}]")).len(), 1);
        assert!(parse("garbage").is_empty());
        assert!(parse(r#"{ "version": 1 }"#).is_empty());
    }

    #[test]
    fn names_what_a_career_needs_in_a_profile() {
        let c = &parse(&format!("[{RECORD}]"))[0];
        let installed: BTreeMap<String, AddonManifest> = [
            ("swofty.progressive", manifest("swofty.progressive", "1.3.2")),
            ("mod.sochi", manifest("mod.sochi", "1.0.0")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let both: Vec<String> = vec!["swofty.progressive".into(), "mod.sochi".into()];
        assert_eq!(
            missing(c, &both, &installed),
            ["needs N mod.sochi (mod.sochi) ^1.1.0, and 1.0.0 is installed"]
        );
        assert_eq!(
            missing(c, &both[..1], &installed),
            ["needs N mod.sochi (mod.sochi), which is off"]
        );
        assert_eq!(
            missing(c, &both, &BTreeMap::new()),
            [
                "needs swofty.progressive, which isn't installed",
                "needs mod.sochi, which isn't installed"
            ]
        );
    }

    #[test]
    fn says_which_careers_an_addon_would_drop_and_one_play_line() {
        let record = |id: &str, status: &str, addon: &str| {
            format!(
                r#"{{"career":"{id}","label":"L {id}","status":"{status}","mode":"driver_career","addons":[{{"id":"{addon}"}}]}}"#
            )
        };
        let list = parse(&format!(
            "[{}, {}, {}]",
            record("a", "present", "x.one"),
            record("b", "missing", "x.one"),
            record("c", "present", "x.two")
        ));
        let names = |v: Vec<&Career>| v.iter().map(|c| c.career.clone()).collect::<Vec<_>>();
        assert_eq!(names(needing(&list, "x.one")), ["a"]);
        assert_eq!(names(needing(&list, "x.two")), ["c"]);
        assert!(needing(&list, "x.none").is_empty());
        assert_eq!(list[0].field("mode"), Some("driver_career"));
        assert_eq!(list[0].field("driver"), None);
        assert_eq!(list[1].status(), "missing");
        let installed: BTreeMap<String, AddonManifest> = [
            ("x.one", manifest("x.one", "1.0.0")),
            ("x.two", manifest("x.two", "1.0.0")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let both: Vec<String> = vec!["x.one".into(), "x.two".into()];
        assert_eq!(play_line(&list, &both, &installed), None);
        let one: Vec<String> = vec!["x.one".into()];
        assert_eq!(
            play_line(&list, &one, &installed).as_deref(),
            Some("1 career won't load in this profile: L c needs N x.two (x.two), which is off")
        );
        assert_eq!(
            play_line(&list, &[], &installed).as_deref(),
            Some(
                "2 careers won't load in this profile: L a needs N x.one (x.one), which is off; L c needs N x.two (x.two), which is off"
            )
        );
    }

    #[test]
    fn careers_deleted_in_game_are_left_out_of_the_play_line_and_stop_line() {
        let record = |id: &str, status: &str| {
            format!(
                r#"{{"career":"{id}","label":"L {id}","status":"{status}","addons":[{{"id":"x.one","why":"{ROWS_WHY}"}}]}}"#
            )
        };
        let installed = installed_of(&["x.one"]);
        let list_of = |status: &str| parse(&format!("[{}]", record("a", status)));

        assert!(play_line(&list_of("present"), &[], &installed).is_some());
        assert!(needing(&list_of("present"), "x.one").len() == 1);

        let deleted = list_of("deleted");
        assert_eq!(play_line(&deleted, &[], &installed), None);
        assert_eq!(stop_line(&deleted, &[], &installed), None);
        assert!(needing(&deleted, "x.one").is_empty());

        let missing = list_of("missing");
        assert_eq!(play_line(&missing, &[], &installed), None);
        assert_eq!(stop_line(&missing, &[], &installed), None);
        assert!(needing(&missing, "x.one").is_empty());
    }

    #[test]
    fn deleted_careers_do_not_record_addons() {
        let installed = installed_of(&["x.one"]);
        let mut list =
            parse(r#"[{"career":"a","profile":"p","status":"deleted","last_saved":2000,"addons":[]}]"#);
        let ids = ["x.one".to_owned()];
        let ids: Vec<&String> = ids.iter().collect();
        assert!(!record(&mut list, &|_| true, 0, &ids, "why", &installed));
        assert!(list[0].addons.is_empty());
    }

    fn installed_of(ids: &[&str]) -> BTreeMap<String, AddonManifest> {
        ids.iter()
            .map(|id| ((*id).to_owned(), manifest(id, "1.0.0")))
            .collect()
    }

    fn my_team(saved: i64) -> Vec<Career> {
        parse(&format!(
            r#"[{{"career":"mt-1","profile":"p","label":"My Team","mode":"my_team","last_saved":{saved},"addons":[]}},
               {{"career":"mt-2","profile":"other","label":"Elsewhere","last_saved":{saved},"addons":[]}},
               {{"career":"mt-3","profile":"p","label":"Old","status":"missing","last_saved":{saved},"addons":[]}}]"#
        ))
    }

    #[test]
    fn records_the_row_owning_addons_that_were_on_when_a_career_was_saved() {
        let installed = installed_of(&["x.engines", "x.skin", "x.liveries"]);
        let owners: BTreeSet<String> = ["x.engines".to_owned(), "x.liveries".to_owned()].into();
        let on: Vec<String> = vec!["x.engines".into(), "x.skin".into()];
        let mut list = my_team(2_000);
        assert!(record_rows(
            &mut list,
            &|c| c.profile.as_deref() == Some("p"),
            1_000,
            &on,
            &owners,
            &installed
        ));
        assert_eq!(list[0].addons.len(), 1);
        assert_eq!(list[0].addons[0].id, "x.engines");
        assert_eq!(list[0].addons[0].version.as_deref(), Some("1.0.0"));
        assert_eq!(list[0].addons[0].why.as_deref(), Some(ROWS_WHY));
        assert!(list[1].addons.is_empty(), "another profile's career");
        assert!(list[2].addons.is_empty(), "a career gone from the save");
        assert!(!record_rows(
            &mut list,
            &|c| c.profile.as_deref() == Some("p"),
            1_000,
            &on,
            &owners,
            &installed
        ));
        let mut older = my_team(500);
        assert!(!record_rows(
            &mut older,
            &|c| c.profile.as_deref() == Some("p"),
            1_000,
            &on,
            &owners,
            &installed
        ));
        assert!(older[0].addons.is_empty());
    }

    #[test]
    fn a_profile_boots_only_the_careers_of_its_own_save() {
        let root = std::env::temp_dir().join(format!("swofty-careers-scope-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        for (name, launcher_saves) in [("a", true), ("b", true), ("s1", false), ("s2", false)] {
            let dir = root.join("profiles");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(format!("{name}.json")),
                format!(r#"{{"name":"{name}","launcher_saves":{launcher_saves}}}"#),
            )
            .ok();
        }
        let list = parse(
            r#"[{"career":"1","profile":"a"},{"career":"2","profile":"b"},{"career":"3"},
                {"career":"4","profile":"s1"},{"career":"5","profile":"s2"}]"#,
        );
        let ids = |p: &str| -> Vec<String> {
            in_save_of(&list, &root, p)
                .iter()
                .map(|c| c.career.clone())
                .collect()
        };
        assert_eq!(ids("a"), ["1", "3"]);
        assert_eq!(ids("b"), ["2", "3"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn stops_play_when_a_saved_rows_addon_is_off_or_gone_in_any_career() {
        let installed = installed_of(&["x.engines"]);
        let owners: BTreeSet<String> = ["x.engines".to_owned()].into();
        let mut list = my_team(2_000);
        let on: Vec<String> = vec!["x.engines".into()];
        record_rows(
            &mut list,
            &|c| c.profile.as_deref() == Some("p"),
            0,
            &on,
            &owners,
            &installed,
        );
        assert_eq!(stop_line(&list, &on, &installed), None);
        let stop = stop_line(&list, &[], &installed).unwrap();
        assert!(stop.contains("N x.engines (off)"), "{stop}");
        assert!(
            stop.contains("1 career in your save use its database rows (My Team)"),
            "{stop}"
        );
        assert!(stop.contains("Turn it back on"), "{stop}");
        let gone = stop_line(&list, &on, &BTreeMap::new()).unwrap();
        assert!(gone.contains("x.engines (not installed)"), "{gone}");
        list[0]
            .extra
            .insert("status".into(), Value::String("missing".into()));
        assert_eq!(stop_line(&list, &[], &installed), None);
    }

    #[test]
    fn a_career_records_only_the_track_addons_its_sessions_loaded() {
        let log = "[apply] facts: mode grand_prix, session race, track track.mza26/mza26\n\
                   [apply] facts: mode grand_prix, session race, track game/Bahrain\n\
                   [apply] facts: mode unknown, session none, track none\n";
        assert_eq!(
            session_tracks(log).into_iter().collect::<Vec<_>>(),
            ["track.mza26"]
        );
        let installed = installed_of(&["track.mza26", "track.other"]);
        let mut list = parse(
            r#"[{"career":"gp-1","profile":"p","label":"Grand Prix","mode":"grand_prix","last_saved":2000,"addons":[]}]"#,
        );
        let tracks: Vec<String> = session_tracks(log).into_iter().collect();
        let ids: Vec<&String> = tracks.iter().collect();
        assert!(record(
            &mut list,
            &|c| c.profile.as_deref() == Some("p"),
            0,
            &ids,
            TRACK_WHY,
            &installed
        ));
        assert_eq!(list[0].addons.len(), 1);
        assert_eq!(list[0].addons[0].id, "track.mza26");
        let on: Vec<String> = vec!["track.mza26".into()];
        assert_eq!(stop_line(&list, &on, &installed), None);
        let unrelated: Vec<String> = vec!["track.mza26".into(), "track.other".into()];
        assert_eq!(stop_line(&list, &unrelated, &installed), None);
        let off = stop_line(&list, &["track.other".into()], &installed).unwrap();
        assert!(off.contains("N track.mza26 (off)"), "{off}");
        assert!(off.contains("(Grand Prix)"), "{off}");
        let gone = stop_line(&list, &on, &BTreeMap::new()).unwrap();
        assert!(gone.contains("track.mza26 (not installed)"), "{gone}");
    }

    #[test]
    fn a_content_addon_is_not_a_play_stop_and_sync_keeps_the_recorded_rows() {
        let list = parse(&format!("[{RECORD}]"));
        assert_eq!(stop_line(&list, &[], &BTreeMap::new()), None);
        let root = std::env::temp_dir().join(format!("swofty-careers-rows-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let (data, game) = (root.join("data"), root.join("game"));
        let mut mine = my_team(2_000);
        mine[0].addons.push(CareerAddon {
            id: "x.engines".into(),
            why: Some(ROWS_WHY.into()),
            ..CareerAddon::default()
        });
        std::fs::create_dir_all(data.join("careers")).unwrap();
        std::fs::write(
            data_path(&data),
            serde_json::json!({ "version": 1, "careers": mine }).to_string(),
        )
        .unwrap();
        std::fs::create_dir_all(game.join("swofty/careers")).unwrap();
        std::fs::write(
            game.join(GAME_PATH),
            r#"[{"career":"mt-1","profile":"p","label":"My Team","addons":[]}]"#,
        )
        .unwrap();
        let merged = sync(&data, Some(&game), None).unwrap();
        let kept = merged.iter().find(|c| c.career == "mt-1").unwrap();
        assert_eq!(kept.addons.len(), 1);
        assert_eq!(kept.addons[0].id, "x.engines");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_launchers_copy_keeps_careers_and_their_profile() {
        let root = std::env::temp_dir().join(format!("swofty-careers-{}", std::process::id()));
        let (data, game) = (root.join("data"), root.join("game"));
        std::fs::create_dir_all(game.join("swofty/careers")).unwrap();
        std::fs::write(game.join(GAME_PATH), format!("[{RECORD}]")).unwrap();
        let first = sync(&data, Some(&game), Some("progressive")).unwrap();
        assert_eq!(first.len(), 1);
        let later = RECORD
            .replace("\"profile\": \"progressive\",", "")
            .replace("season 1", "season 2");
        let other = RECORD
            .replace("6f3e9a", "77aa01")
            .replace("progressive\",", "vanilla-ish\",");
        std::fs::write(game.join(GAME_PATH), format!("[{later}, {other}]")).unwrap();
        let merged = sync(&data, Some(&game), Some("other")).unwrap();
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].profile.as_deref(), Some("progressive"));
        assert!(merged[0].label.ends_with("season 2"));
        std::fs::remove_file(game.join(GAME_PATH)).unwrap();
        assert_eq!(sync(&data, Some(&game), None).unwrap().len(), 2);
        assert_eq!(sync(&data, None, None).unwrap().len(), 2);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_career_without_a_profile_gets_the_sessions_and_the_ids_survive_a_restore() {
        let root = std::env::temp_dir().join(format!("swofty-careers-keep-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let (data, game) = (root.join("data"), root.join("game"));
        let unstamped = RECORD.replace("\"profile\": \"progressive\",", "");
        std::fs::create_dir_all(game.join("swofty/careers")).unwrap();
        std::fs::write(game.join(GAME_PATH), format!("[{unstamped}]")).unwrap();
        let synced = sync(&data, Some(&game), Some("tails")).unwrap();
        assert_eq!(synced[0].profile.as_deref(), Some("tails"));
        let ids = crate::stdlib::reports::legacy_path(&game, crate::stdlib::reports::IDS);
        std::fs::create_dir_all(ids.parent().unwrap()).unwrap();
        std::fs::write(&ids, "{\"track\":{\"mod.sochi/sochi\":18}}").unwrap();
        keep(&data, &game).unwrap();
        std::fs::remove_dir_all(game.join("swofty")).unwrap();
        assert_eq!(read(&data_path(&data)).len(), 1);
        assert_eq!(
            std::fs::read_to_string(kept_ids(&data)).unwrap(),
            "{\"track\":{\"mod.sochi/sochi\":18}}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn migrating_moves_the_old_files_once_and_the_newer_ids_win() {
        let root = std::env::temp_dir().join(format!("swofty-careers-migrate-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let (data, game) = (root.join("d\u{e4}ta \u{30b9}"), root.join("game"));
        let put = |p: &Path, t: &str| {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, t).unwrap();
        };
        use crate::stdlib::reports::{IDS, ROOM, legacy_path, path};
        put(&legacy_path(&game, IDS), "{\"old\":1}");
        put(&legacy_path(&game, ROOM), "{\"presize\":\"applied\"}");
        put(&game.join(GAME_PATH), &format!("[{RECORD}]"));
        put(&path(&data, ROOM), "{\"presize\":\"kept\"}");
        let moved = migrate(&data, &game).unwrap();
        assert_eq!(moved, ["career index", "ids.json"]);
        assert_eq!(std::fs::read_to_string(path(&data, IDS)).unwrap(), "{\"old\":1}");
        assert_eq!(
            std::fs::read_to_string(path(&data, ROOM)).unwrap(),
            "{\"presize\":\"kept\"}"
        );
        assert_eq!(read(&data_path(&data)).len(), 1);
        for gone in [
            legacy_path(&game, IDS),
            legacy_path(&game, ROOM),
            game.join(GAME_PATH),
        ] {
            assert!(!gone.exists(), "{}", gone.display());
        }
        assert!(migrate(&data, &game).unwrap().is_empty());
        put(&legacy_path(&game, IDS), "{\"stale\":1}");
        std::fs::File::options()
            .write(true)
            .open(legacy_path(&game, IDS))
            .unwrap()
            .set_modified(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
        assert!(migrate(&data, &game).unwrap().is_empty());
        assert_eq!(std::fs::read_to_string(path(&data, IDS)).unwrap(), "{\"old\":1}");
        assert!(!legacy_path(&game, IDS).exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
