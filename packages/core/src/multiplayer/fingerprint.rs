//! What a profile plays, for a lobby to check that every player runs the same gameplay
//! content: the game build, each gameplay addon with its version and source, and the
//! values of their gameplay options. Cosmetic addons and options are left out.

use std::collections::BTreeMap;
use std::path::Path;

use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::asset::Source as OpSource;
use crate::baseline::{Hashes, Identity, hex, sha256_bytes};
use crate::builtin::{CORE_DB, CORE_DLL, CORE_ID, CORE_MANIFEST, loader_dir, loader_files};
use crate::catalog::{self, Addon};
use crate::manifest::{AddonManifest, Kind, Multiplayer};
use crate::profile::Profile;
use crate::store::Store;
use crate::{Result, read_json, write_json_shared};

pub const VERSION: u32 = 1;
/// Digests of the addon files [`fingerprint`] hashed, kept apart from the launch-time ones.
pub const HASHES_FILE: &str = "state/fingerprint-hashes.json";
/// What the last build of each profile made the game load, addon by addon; see [`Output`].
pub const OUTPUTS_FILE: &str = "state/fingerprint-outputs.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub version: u32,
    /// The launcher that made it, for showing; the core's `builtin` source covers it.
    pub launcher: String,
    /// The digest covers only `exe_sha256`; `build_id` is for showing.
    pub game_build: Identity,
    /// Sorted by id.
    pub addons: Vec<AddonPrint>,
    /// Addon id to option id to value.
    pub options: BTreeMap<String, BTreeMap<String, Value>>,
    /// Whether the player owns the game's DLC; `None` while the launcher can't tell.
    pub dlc: Option<bool>,
    /// [`Fingerprint::canonical_digest`] when the fingerprint was made.
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddonPrint {
    pub id: String,
    /// For showing; the digest leaves it out.
    pub name: String,
    pub version: Version,
    pub source: Source,
    /// The SHA-256 of the Addon Browser download the copy was installed from, which says what
    /// a fix installs; the digest and the comparison leave it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    /// For a built-in addon, what its part of the profile's last build made the game load, as
    /// [`Source::Built`] has it; until the profile is built, for an addon with a build hook, a
    /// SHA-256 of what the hook reads: its addon data and its state in the profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inputs: Option<String>,
    /// Its place in the load order among the gameplay addons that change game data, where
    /// a later one can override an earlier one; `None` for one that only loads code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<usize>,
    /// What its part of the last build is made of, for naming what differs from another copy;
    /// the digest leaves it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Detail>,
    /// The stat id the core's registry gave each of the addon's records, by `<registry>:<record>`,
    /// as the install's `ids.json` held them when the fingerprint was made.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stat_ids: BTreeMap<String, i64>,
}

/// The parts and files of an addon's [`Output`], each as the first 12 hex digits of its
/// SHA-256, so two copies whose outputs differ can name the part or file that does.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Detail {
    /// Part label (an operation file kind, `declared`, `picks`, `fields`, `hook` or `native`)
    /// to its hash.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parts: BTreeMap<String, String>,
    /// The hash of the read list (which files, not their contents) and its length.
    pub read_list: String,
    pub read_count: usize,
    /// One hash per read, in the read list's order; left out of a blob that would be too big.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reads: Vec<String>,
    /// The read paths, which stay on this side.
    #[serde(skip)]
    pub paths: Vec<String>,
    /// The stat id the build gave each row it added, by `<table>.<row>/<label>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub row_ids: BTreeMap<String, i64>,
}

/// What the installed copy of an addon is, as a SHA-256 in hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Its installed files: each one's path and SHA-256, in path order.
    Content(String),
    /// A DB Editor addon: the database operations and Driver Manager picks the build makes of
    /// its edits.
    Edits(String),
    /// Built into the launcher: its version and manifest, and for the core its database
    /// operations and DLL.
    Builtin(String),
    /// What its part of the profile's last build made the game load: the [`Output`] it made,
    /// with every file that output reads hashed by content. Taken in place of `Content` and
    /// `Edits` once the profile has been built.
    Built(String),
}

impl Fingerprint {
    pub fn new(
        game_build: Identity,
        mut addons: Vec<AddonPrint>,
        options: BTreeMap<String, BTreeMap<String, Value>>,
        dlc: Option<bool>,
    ) -> Self {
        addons.sort_by(|a, b| a.id.cmp(&b.id));
        let mut f = Self {
            version: VERSION,
            launcher: env!("CARGO_PKG_VERSION").into(),
            game_build,
            addons,
            options,
            dlc,
            digest: String::new(),
        };
        f.digest = f.canonical_digest();
        f
    }

    /// SHA-256 of the fingerprint as JSON with sorted keys and addons, each `order` as its rank
    /// among the placed addons, without `launcher`, `game_build.build_id`, `name`, `registry`
    /// or `digest`.
    pub fn canonical_digest(&self) -> String {
        let mut placed: Vec<(usize, &str)> = self
            .addons
            .iter()
            .filter_map(|a| Some((a.order?, a.id.as_str())))
            .collect();
        placed.sort();
        let rank = |id: &str| placed.iter().position(|(_, p)| *p == id);
        let mut addons: Vec<&AddonPrint> = self.addons.iter().collect();
        addons.sort_by(|a, b| a.id.cmp(&b.id));
        let addons: Vec<Value> = addons
            .into_iter()
            .map(|a| {
                let mut one = serde_json::json!({
                    "id": a.id,
                    "version": a.version.to_string(),
                    "source": a.source,
                    "inputs": a.inputs,
                    "order": rank(&a.id),
                });
                if !a.stat_ids.is_empty() {
                    one["stat_ids"] = serde_json::json!(a.stat_ids);
                }
                one
            })
            .collect();
        let doc = serde_json::json!({
            "version": self.version,
            "game_build": { "exe_sha256": self.game_build.exe_sha256.to_ascii_lowercase() },
            "addons": addons,
            "options": self.options,
            "dlc": self.dlc,
        });
        sha256_bytes(&serde_json::to_vec(&sorted(doc)).expect("serialisable"))
    }
}

fn sorted(v: Value) -> Value {
    match v {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        v => v,
    }
}

/// The ids `doc`, the core's `ids.json`, holds for the records of `addon`, by `<registry>:<record>`.
pub fn stat_ids_of(doc: &Value, addon: &str) -> BTreeMap<String, i64> {
    let prefix = format!("{addon}/");
    let mut out = BTreeMap::new();
    let Some(registries) = doc["ids"].as_object() else {
        return out;
    };
    for (registry, records) in registries {
        let Some(records) = records.as_object() else {
            continue;
        };
        for (record, entry) in records.iter().filter(|(r, _)| r.starts_with(&prefix)) {
            if let Some(id) = entry.as_i64().or_else(|| entry["id"].as_i64()) {
                out.insert(format!("{registry}:{record}"), id);
            }
        }
    }
    out
}

/// The fingerprint of `profile` as its build would load it, on the game `game_build`.
pub fn fingerprint(store: &Store, profile: &Profile, game_build: &Identity) -> Result<Fingerprint> {
    let registry_ids = crate::stdlib::reports::found(&store.root, None, crate::stdlib::reports::IDS)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .unwrap_or(Value::Null);
    let installed = store.addons()?;
    let load = crate::resolver::resolve(profile, &installed)?.order;
    let mut hashes = Hashes::open(store.root.join(HASHES_FILE));
    let mut outputs: BTreeMap<String, BTreeMap<String, Output>> =
        read_json(&store.root.join(OUTPUTS_FILE)).unwrap_or_default();
    let built = outputs.remove(&profile.name).unwrap_or_default();
    let mut addons = Vec::new();
    let mut options = BTreeMap::new();
    let mut place = 0;
    for m in load.iter().map(|id| &installed[id]) {
        if m.multiplayer() == Multiplayer::Cosmetic {
            continue;
        }
        let order = changes_data(m).then(|| {
            place += 1;
            place - 1
        });
        let output = built
            .get(&m.id)
            .map(|o| o.digest(store, &m.id, &mut hashes))
            .transpose()?;
        let detail = output.as_ref().map(|(_, d)| d.clone());
        let builtin = store.is_builtin(&m.id);
        let (source, inputs) = match output.map(|(digest, _)| digest) {
            Some(o) if builtin => (source(store, m, &mut hashes)?, Some(o)),
            Some(o) => (Source::Built(o), None),
            None => (
                source(store, m, &mut hashes)?,
                hook_inputs(store, m, profile, &mut hashes)?,
            ),
        };
        addons.push(AddonPrint {
            id: m.id.clone(),
            name: m.name.clone(),
            version: m.version.clone(),
            source,
            registry: (!crate::dbedit::package::is_package(store, &m.id))
                .then(|| catalog::installed_from(store, m))
                .flatten(),
            inputs,
            order,
            detail,
            stat_ids: stat_ids_of(&registry_ids, &m.id),
        });
        let values: BTreeMap<String, Value> = m
            .options
            .iter()
            .filter(|o| o.multiplayer != Some(Multiplayer::Cosmetic))
            .map(|o| {
                let v = profile.option(m, o);
                (o.id.clone(), o.normalise(&v).unwrap_or(v))
            })
            .collect();
        if !values.is_empty() {
            options.insert(m.id.clone(), values);
        }
    }
    hashes.save();
    Ok(Fingerprint::new(game_build.clone(), addons, options, None))
}

fn changes_data(m: &AddonManifest) -> bool {
    let o = &m.operations;
    let ops = [&o.database, &o.erp, &o.ui, &o.assets]
        .iter()
        .any(|l| !l.is_empty())
        || !o.files.is_empty();
    ops || m.launcher.as_ref().is_some_and(|l| l.build) || m.kind.iter().any(|k| *k != Kind::Native)
}

fn source(store: &Store, m: &AddonManifest, hashes: &mut Hashes) -> Result<Source> {
    if store.is_builtin(&m.id) {
        let mut h = Sha256::new();
        h.update(format!("launcher {}\0", env!("CARGO_PKG_VERSION")));
        if m.id == CORE_ID {
            let dll = loader_dir().map(|d| d.join(CORE_DLL)).filter(|p| p.is_file());
            let dll = dll.map(|p| hashes.sha256(&p)).transpose()?;
            h.update(format!("dll {}\0", dll.as_deref().unwrap_or("none")));
            h.update(CORE_MANIFEST);
            h.update(CORE_DB);
        } else {
            h.update(serde_json::to_vec(m).expect("serialisable"));
        }
        return Ok(Source::Builtin(hex(&h.finalize())));
    }
    if crate::dbedit::package::is_package(store, &m.id)
        && let Ok(p) = crate::dbedit::package::Package::open(store, &m.id)
    {
        let built = serde_json::json!({
            "ops": p.edits.to_ops(),
            "picks": crate::dbedit::driver_manager::picks(&p.edits),
        });
        let text = serde_json::to_vec(&sorted(built)).expect("serialisable");
        return Ok(Source::Edits(sha256_bytes(&text)));
    }
    content_hash(&store.addons_dir().join(&m.id), hashes).map(Source::Content)
}

/// What one addon's part of a build made the game load: a SHA-256 of the operations, fields,
/// screens and plugin list it produced, and the files those read. A fingerprint hashes the
/// files by content, so an addon prints the same however it was installed, and files the
/// build never reads, such as logs, import reports and import history, don't count.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Output {
    pub made: String,
    pub reads: Vec<Read>,
    /// `made` split by part label, as [`Detail::parts`] carries them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub parts: BTreeMap<String, String>,
    /// The stat id the build gave each row the addon added, by `<table>.<row>/<label>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub row_ids: BTreeMap<String, i64>,
}

/// A file or folder an [`Output`] reads, as the operation names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Read {
    pub addon: String,
    pub source: OpSource,
}

impl Output {
    /// The output's digest, and its [`Detail`]; `id` is the addon the output is of.
    fn digest(&self, store: &Store, id: &str, hashes: &mut Hashes) -> Result<(String, Detail)> {
        let mut h = Sha256::new();
        let mut list = Sha256::new();
        let mut detail = Detail {
            parts: self.parts.iter().map(|(k, v)| (k.clone(), short(v))).collect(),
            read_count: self.reads.len(),
            row_ids: self.row_ids.clone(),
            ..Detail::default()
        };
        h.update(&self.made);
        if !self.row_ids.is_empty() {
            h.update(b"\nrow ids\0");
            h.update(serde_json::to_vec(&self.row_ids).expect("serialisable"));
        }
        for r in &self.reads {
            let (root, rel) = (r.source.root, &r.source.path);
            let name = format!("\n{}\0{root:?}\0{rel}", r.addon);
            h.update(&name);
            list.update(&name);
            let mut one = Sha256::new();
            let path = r
                .source
                .resolve(&crate::build::source_dir(store, &r.addon, root))?;
            let mut feed = |text: String| {
                h.update(&text);
                one.update(&text);
            };
            if path.is_dir() {
                for (rel, file) in loader_files(&path)? {
                    if !rel.split('/').any(|c| c.starts_with('.')) {
                        feed(format!("\0{rel}\0{}", hashes.sha256(&file)?));
                    }
                }
            } else if path.is_file() {
                feed(format!("\0{}", hashes.sha256(&path)?));
            } else {
                feed("\0missing".into());
            }
            detail.reads.push(short(&hex(&one.finalize())));
            detail.paths.push(match r.addon == id {
                true => rel.clone(),
                false => format!("{}:{rel}", r.addon),
            });
        }
        detail.read_list = short(&hex(&list.finalize()));
        Ok((hex(&h.finalize()), detail))
    }
}

fn short(digest: &str) -> String {
    digest.get(..12).unwrap_or(digest).to_owned()
}

/// Collects an addon's [`Output`] as the build makes it.
#[derive(Default)]
pub struct OutputBuilder {
    made: Sha256,
    parts: BTreeMap<String, Sha256>,
    reads: BTreeMap<(String, String), OpSource>,
}

impl OutputBuilder {
    pub fn made(&mut self, what: &str, value: &impl Serialize) {
        let json = serde_json::to_vec(value).expect("serialisable");
        for h in [&mut self.made, self.parts.entry(what.to_owned()).or_default()] {
            h.update(format!("{what}\0"));
            h.update(&json);
            h.update(b"\n");
        }
    }

    pub fn reads(&mut self, addon: &str, source: &OpSource) {
        let key = format!("{:?}/{}", source.root, source.path);
        self.reads.insert((addon.to_owned(), key), source.clone());
    }

    pub fn finish(self) -> Output {
        Output {
            made: hex(&self.made.finalize()),
            reads: self
                .reads
                .into_iter()
                .map(|((addon, _), source)| Read { addon, source })
                .collect(),
            parts: self
                .parts
                .into_iter()
                .map(|(label, h)| (label, hex(&h.finalize())))
                .collect(),
            row_ids: BTreeMap::new(),
        }
    }
}

/// Keeps what the build of `profile` made the game load, in place of its previous build's.
pub fn record_outputs(store: &Store, profile: &str, outputs: BTreeMap<String, Output>) -> Result<()> {
    let path = store.root.join(OUTPUTS_FILE);
    let mut all: BTreeMap<String, BTreeMap<String, Output>> = read_json(&path).unwrap_or_default();
    all.insert(profile.to_owned(), outputs);
    write_json_shared(&path, &all)
}

/// SHA-256 over each file under `dir` as `path\0sha256\n`, paths relative with `/`, in order.
pub fn content_hash(dir: &Path, hashes: &mut Hashes) -> Result<String> {
    let mut h = Sha256::new();
    for (rel, path) in loader_files(dir)? {
        h.update(format!("{rel}\0{}\n", hashes.sha256(&path)?));
    }
    Ok(hex(&h.finalize()))
}

/// The addon data files and profile state a build hook reads, leaving out hidden entries, `logs`
/// folders and `.log` and `.tmp` files. A JSON file or the state counts as [`settled`] JSON,
/// and one that holds only empty containers counts as absent.
fn hook_inputs(
    store: &Store,
    m: &AddonManifest,
    profile: &Profile,
    hashes: &mut Hashes,
) -> Result<Option<String>> {
    let builtin = store.builtins().iter().any(|b| b.addon.manifest().id == m.id);
    if !builtin && !m.launcher.as_ref().is_some_and(|l| l.build) {
        return Ok(None);
    }
    let mut h = Sha256::new();
    let dir = store.addon_data_dir(&m.id);
    let counters = m.id == crate::dbedit::driver_manager::ID;
    let volatile = |rel: &str| {
        rel.split('/').any(|c| c.starts_with('.') || c == "logs")
            || rel.ends_with(".log")
            || rel.ends_with(".tmp")
    };
    if dir.is_dir() {
        for (rel, path) in loader_files(&dir)?.into_iter().filter(|(rel, _)| !volatile(rel)) {
            let json = rel.ends_with(".json")
                && std::fs::metadata(&path).is_ok_and(|meta| meta.len() <= JSON_INPUT_LIMIT);
            let parsed = json
                .then(|| std::fs::read(&path).ok())
                .flatten()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
            let sha = match parsed {
                Some(v) => match settled(v, counters) {
                    Some(v) => sha256_bytes(&serde_json::to_vec(&v).expect("serialisable")),
                    None => continue,
                },
                None => hashes.sha256(&path)?,
            };
            h.update(format!("{rel}\0{sha}\n"));
        }
    }
    let state = profile
        .state
        .get(&m.id)
        .cloned()
        .and_then(|v| settled(v, counters));
    h.update(format!(
        "\0state\0{}",
        serde_json::to_string(&state).expect("serialisable")
    ));
    Ok(Some(hex(&h.finalize())))
}

/// JSON addon data bigger than this is hashed as bytes.
const JSON_INPUT_LIMIT: u64 = 4 * 1024 * 1024;

/// `v` with sorted keys, and with `counters` without its top-level `next_*` id counters, as
/// Driver Manager keeps them. `None` when, `schema` and `schema_version` aside, it holds only
/// empty arrays and objects.
fn settled(v: Value, counters: bool) -> Option<Value> {
    fn empty(v: &Value) -> bool {
        match v {
            Value::Array(a) => a.is_empty(),
            Value::Object(o) => o.values().all(empty),
            _ => false,
        }
    }
    let v = match v {
        Value::Object(mut o) => {
            o.retain(|k, _| !(counters && k.starts_with("next_")));
            let bare = o
                .iter()
                .all(|(k, v)| k == "schema" || k == "schema_version" || empty(v));
            if bare {
                return None;
            }
            Value::Object(o)
        }
        v if empty(&v) => return None,
        v => v,
    };
    Some(sorted(v))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mismatch {
    #[serde(flatten)]
    pub kind: MismatchKind,
    /// The launcher can fix it on its own: turn an addon off, set an option, reorder, or
    /// install the host's exact version from the Addon Browser.
    pub fixable: bool,
    /// For a copy or its data that differs, which part or file does, when both sides sent
    /// their [`Detail`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub differences: Vec<String>,
}

/// How the local fingerprint differs from the host's; `local` is this player's side.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MismatchKind {
    GameBuild {
        local: Identity,
        host: Identity,
    },
    Dlc {
        local: Option<bool>,
        host: Option<bool>,
    },
    /// A built-in addon differs, so the launchers differ: players need the same launcher.
    Launcher {
        local: String,
        host: String,
    },
    /// `registry` in these is the host's download, the one a fix installs.
    MissingAddon {
        id: String,
        name: String,
        version: Version,
        source: Source,
        registry: Option<String>,
    },
    ExtraAddon {
        id: String,
        name: String,
        version: Version,
    },
    VersionDiffers {
        id: String,
        name: String,
        local: Version,
        host: Version,
        registry: Option<String>,
    },
    SourceDiffers {
        id: String,
        name: String,
        version: Version,
        local: Source,
        host: Source,
        registry: Option<String>,
    },
    /// What the addon's build hook reads differs, such as Driver Manager's drivers.
    InputsDiffer {
        id: String,
        name: String,
    },
    /// The core's registry gave the addon's records other stat ids than the host's: the games would read
    /// the same record by different numbers.
    StatIdsDiffer {
        id: String,
        name: String,
    },
    /// The data addons both sides have, in each side's load order.
    LoadOrder {
        local: Vec<String>,
        host: Vec<String>,
    },
    OptionDiffers {
        addon: String,
        option: String,
        local: Option<Value>,
        host: Option<Value>,
    },
}

impl Mismatch {
    /// The addon this is about, when it is about one.
    pub fn name(&self) -> Option<&str> {
        match &self.kind {
            MismatchKind::MissingAddon { name, .. }
            | MismatchKind::ExtraAddon { name, .. }
            | MismatchKind::VersionDiffers { name, .. }
            | MismatchKind::SourceDiffers { name, .. }
            | MismatchKind::InputsDiffer { name, .. }
            | MismatchKind::StatIdsDiffer { name, .. } => Some(name),
            MismatchKind::OptionDiffers { addon, .. } => Some(addon),
            _ => None,
        }
    }

    /// An addon whose copies differ only in the stat ids their rows got, or in this side's game not being
    /// built with it yet: a build with the host's ids ([`row_id_overrides`]) cures it.
    pub fn cured_by_build(&self) -> bool {
        matches!(self.kind, MismatchKind::SourceDiffers { .. })
            && !self.differences.is_empty()
            && self
                .differences
                .iter()
                .all(|d| d.starts_with("row ") || d.starts_with("your game wasn't built with it"))
    }

    pub fn line(&self) -> String {
        let value = |v: &Option<Value>| v.as_ref().map_or_else(|| "unset".to_owned(), Value::to_string);
        let text = match &self.kind {
            MismatchKind::GameBuild { local, host } => {
                format!("game build {} is not the host's {}", build(local), build(host))
            }
            MismatchKind::Dlc { local, host } => {
                format!("DLC is {}; the host's is {}", owned(*local), owned(*host))
            }
            MismatchKind::Launcher { local, host } if local == host => {
                format!("this Swofty Launcher {local} is a different build from the host's {host}")
            }
            MismatchKind::Launcher { local, host } => {
                format!(
                    "Swofty Launcher {local} is not the host's {host}; play with the same launcher version"
                )
            }
            MismatchKind::MissingAddon {
                id, name, version, ..
            } => format!("{name} ({id}) {version} is missing"),
            MismatchKind::ExtraAddon { id, name, version } => {
                format!("{name} ({id}) {version} is on; the host doesn't have it")
            }
            MismatchKind::VersionDiffers {
                id,
                name,
                local,
                host,
                ..
            } => format!("{name} ({id}) is {local}; the host has {host}"),
            MismatchKind::SourceDiffers {
                id, name, version, ..
            } => format!("{name} ({id}) {version} is not the same copy as the host's"),
            MismatchKind::InputsDiffer { id, name } => {
                format!("{name} ({id}) has other data or profile state than the host's")
            }
            MismatchKind::StatIdsDiffer { id, name } => {
                format!("{name} ({id}) has other stat ids than the host's")
            }
            MismatchKind::LoadOrder { local, host } => format!(
                "load order {} is not the host's {}",
                local.join(", "),
                host.join(", ")
            ),
            MismatchKind::OptionDiffers {
                addon,
                option,
                local,
                host,
            } => format!(
                "{addon}.{option} is {}; the host has {}",
                value(local),
                value(host)
            ),
        };
        if self.fixable {
            format!("{text} (fixable)")
        } else {
            text
        }
    }
}

/// The mismatches a room launch will not resolve by itself: with `will_build`, the ones a build with the
/// host's ids and settings (`lent`) cures are left out (an addon this player's game isn't built with yet,
/// only different stat ids, the load order, an option the host's value is lent for, and an addon whose
/// built operations differ because of such an option), since starting F1 25 for the room builds it.
pub fn unresolved_by_launch(
    mismatches: &[Mismatch],
    will_build: bool,
    lent: &crate::row_ids::Overrides,
) -> Vec<Mismatch> {
    mismatches
        .iter()
        .filter(|m| !(will_build && (m.cured_by_build() || lent_cures(m, lent))))
        .cloned()
        .collect()
}

fn lent_cures(m: &Mismatch, lent: &crate::row_ids::Overrides) -> bool {
    match &m.kind {
        MismatchKind::LoadOrder { .. } => !lent.order.is_empty(),
        MismatchKind::OptionDiffers { addon, option, .. } => {
            lent.options.get(addon).is_some_and(|o| o.contains_key(option))
        }
        MismatchKind::SourceDiffers { id, .. } => lent.options.contains_key(id) && !m.differences.is_empty(),
        MismatchKind::StatIdsDiffer { id, .. } => lent.registry.contains_key(id),
        _ => false,
    }
}

/// Files named before the rest are counted.
const NAMED_FILES: usize = 5;

/// What differs between two copies of an addon, from their [`Detail`]s: its parts, which files
/// it reads, or the contents of those files, named by path.
pub fn differences(local: &AddonPrint, host: &AddonPrint) -> Vec<String> {
    let built = |s: &Source| matches!(s, Source::Built(_) | Source::Builtin(_));
    let (l, h) = match (&local.detail, &host.detail) {
        (Some(l), Some(h)) => (l, h),
        (Some(_), None) if !built(&host.source) => {
            return vec![
                "the host's game wasn't built with it, so its files count instead; the host needs to start F1 25 again"
                    .into(),
            ];
        }
        (None, Some(_)) if !built(&local.source) => {
            return vec![
                "your game wasn't built with it, so its files count instead; start F1 25 again".into(),
            ];
        }
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    let labels: std::collections::BTreeSet<&String> = l.parts.keys().chain(h.parts.keys()).collect();
    for label in labels {
        let (a, b) = (l.parts.get(label), h.parts.get(label));
        if a != b {
            let what = part_name(label);
            out.push(match (a, b) {
                (Some(_), None) => format!("{what}: only yours has them"),
                (None, Some(_)) => format!("{what}: only the host's has them"),
                _ => format!("{what} differ"),
            });
        }
    }
    let rows: std::collections::BTreeSet<&String> = l.row_ids.keys().chain(h.row_ids.keys()).collect();
    if let Some(row) = rows.into_iter().find(|r| l.row_ids.get(*r) != h.row_ids.get(*r)) {
        let id = |ids: &BTreeMap<String, i64>| ids.get(row).map_or("none".to_owned(), i64::to_string);
        out.push(format!(
            "row {row} has stat id {} here and {} in the other copy",
            id(&l.row_ids),
            id(&h.row_ids)
        ));
    }
    if l.read_list != h.read_list || l.read_count != h.read_count {
        out.push(format!(
            "it reads other files: {} here, {} at the host",
            l.read_count, h.read_count
        ));
    } else if l.reads.len() == l.read_count && h.reads.len() == h.read_count {
        let differ: Vec<usize> = (0..l.read_count).filter(|&i| l.reads[i] != h.reads[i]).collect();
        let path = |i: usize| {
            l.paths
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("file {}", i + 1))
        };
        out.extend(
            differ
                .iter()
                .take(NAMED_FILES)
                .map(|&i| format!("{} differs", path(i))),
        );
        if differ.len() > NAMED_FILES {
            out.push(format!("and {} more file(s) differ", differ.len() - NAMED_FILES));
        }
    } else if out.is_empty() {
        out.push(format!(
            "one of its {} file(s) may differ; the per-file hashes were left out",
            l.read_count
        ));
    }
    if out.is_empty() {
        out.push("its parts and files match, but not their order".into());
    }
    out
}

fn part_name(label: &str) -> String {
    match label {
        "Database" => "its database operations".into(),
        "Erp" => "its archive operations".into(),
        "Assets" => "its asset operations".into(),
        "declared" => "its file operations".into(),
        "picks" => "its Driver Manager picks".into(),
        "fields" => "its build step's fields".into(),
        "hook" => "its build step's output".into(),
        "native" => "its plugin list".into(),
        other => format!("its {other}"),
    }
}

fn build(i: &Identity) -> String {
    format!(
        "{} (exe {})",
        i.build_id,
        i.exe_sha256.get(..12).unwrap_or(&i.exe_sha256)
    )
}

fn owned(on: Option<bool>) -> &'static str {
    match on {
        Some(true) => "owned",
        Some(false) => "not owned",
        None => "unknown",
    }
}

fn stat_id_differences(l: &AddonPrint, h: &AddonPrint) -> Vec<String> {
    let keys: std::collections::BTreeSet<&String> = l.stat_ids.keys().chain(h.stat_ids.keys()).collect();
    let id = |ids: &BTreeMap<String, i64>, key: &str| ids.get(key).map_or("none".to_owned(), i64::to_string);
    keys.into_iter()
        .filter(|key| l.stat_ids.get(*key) != h.stat_ids.get(*key))
        .map(|key| {
            let (registry, record) = key.split_once(':').unwrap_or(("", key));
            format!(
                "{registry} {record} has stat id {} here and {} at the host",
                id(&l.stat_ids, key),
                id(&h.stat_ids, key)
            )
        })
        .collect()
}

/// The host's stat ids, for each addon `local` and `host` both have whose added rows got other ids on
/// `local`: what a joiner builds one session with to match the host.
pub fn row_id_overrides(local: &Fingerprint, host: &Fingerprint) -> crate::row_ids::Overrides {
    let mut out = crate::row_ids::Overrides::default();
    for h in &host.addons {
        let Some(ids) = h
            .detail
            .as_ref()
            .map(|d| &d.row_ids)
            .filter(|ids| !ids.is_empty())
        else {
            continue;
        };
        let Some(l) = local.addons.iter().find(|a| a.id == h.id) else {
            continue;
        };
        let mine = l.detail.as_ref().map(|d| &d.row_ids);
        if mine != Some(ids) {
            out.rows.insert(h.id.clone(), ids.clone());
        }
    }
    out
}

/// The host's registry stat ids of the records of each addon `local` and `host` both have, for the
/// addons whose records `local` holds under other ids.
pub fn registry_id_overrides(
    local: &Fingerprint,
    host: &Fingerprint,
) -> BTreeMap<String, BTreeMap<String, i64>> {
    host.addons
        .iter()
        .filter_map(|h| {
            let l = local.addons.iter().find(|a| a.id == h.id)?;
            (l.stat_ids != h.stat_ids).then(|| (h.id.clone(), h.stat_ids.clone()))
        })
        .collect()
}

/// What a joiner lends one session's builds to match `host`: the host's stat ids ([`row_id_overrides`]),
/// and for the addons both have at the same version, the host's load order when it differs and the
/// host's value of each option that differs.
pub fn host_overrides(local: &Fingerprint, host: &Fingerprint) -> crate::row_ids::Overrides {
    let mut out = row_id_overrides(local, host);
    out.registry = registry_id_overrides(local, host);
    let mine: BTreeMap<&str, &AddonPrint> = local.addons.iter().map(|a| (a.id.as_str(), a)).collect();
    let shared: BTreeMap<&str, (&AddonPrint, &AddonPrint)> = host
        .addons
        .iter()
        .filter_map(|h| {
            let l = mine.get(h.id.as_str())?;
            let same =
                l.version == h.version && (l.source == h.source || !matches!(l.source, Source::Builtin(_)));
            same.then_some((h.id.as_str(), (*l, h)))
        })
        .collect();
    let placed = |pick: fn(&(&AddonPrint, &AddonPrint)) -> Option<usize>| -> Vec<String> {
        let mut ranked: Vec<(usize, &str)> = shared
            .iter()
            .filter(|(_, pair)| pair.0.order.is_some() && pair.1.order.is_some())
            .filter_map(|(id, pair)| Some((pick(pair)?, *id)))
            .collect();
        ranked.sort();
        ranked.into_iter().map(|(_, id)| id.to_owned()).collect()
    };
    let (mine_order, host_order) = (placed(|p| p.0.order), placed(|p| p.1.order));
    if mine_order != host_order {
        out.order = host_order;
    }
    for (addon, values) in host
        .options
        .iter()
        .filter(|(id, _)| shared.contains_key(id.as_str()))
    {
        let ours = local.options.get(addon);
        for (option, h) in values
            .iter()
            .filter(|(o, h)| ours.and_then(|o2| o2.get(*o)) != Some(*h))
        {
            out.options
                .entry(addon.clone())
                .or_default()
                .insert(option.clone(), h.clone());
        }
    }
    out
}

impl Fingerprint {
    /// This fingerprint as it is once a session's builds use `lent`: the shared addons in the host's load
    /// order and the host's option values.
    pub fn lent(&self, lent: &crate::row_ids::Overrides) -> Fingerprint {
        let mut ranked: Vec<&AddonPrint> = self.addons.iter().filter(|a| a.order.is_some()).collect();
        ranked.sort_by_key(|a| a.order);
        let current: Vec<String> = ranked.iter().map(|a| a.id.clone()).collect();
        let arranged = lent.arranged(&current);
        let mut addons = self.addons.clone();
        for a in &mut addons {
            if let Some(rank) = arranged.iter().position(|id| *id == a.id) {
                a.order = Some(rank);
            }
        }
        for a in &mut addons {
            if let Some(ids) = lent.registry.get(&a.id) {
                a.stat_ids = ids.clone();
            }
        }
        let mut options = self.options.clone();
        for (addon, values) in &lent.options {
            if let Some(mine) = options.get_mut(addon) {
                mine.extend(values.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
        Fingerprint::new(self.game_build.clone(), addons, options, self.dlc)
    }
}

/// What `local` must change to match `host`, the game build and DLC first, then addons,
/// the load order and options. `catalog` is the Addon Browser's listing.
pub fn compare(local: &Fingerprint, host: &Fingerprint, catalog: &[Addon]) -> Vec<Mismatch> {
    let launcher = crate::update::current_version();
    let listed = |a: &AddonPrint| {
        a.registry.as_ref().is_some_and(|sha| {
            catalog
                .iter()
                .find(|c| c.id == a.id)
                .and_then(|c| c.listed().into_iter().find(|v| v.version == a.version))
                .is_some_and(|v| v.sha256.trim().eq_ignore_ascii_case(sha) && v.accepts(&launcher))
        })
    };
    let mut out = Vec::new();
    let mut push = |kind, fixable| {
        out.push(Mismatch {
            kind,
            fixable,
            differences: Vec::new(),
        })
    };
    if !local
        .game_build
        .exe_sha256
        .eq_ignore_ascii_case(&host.game_build.exe_sha256)
    {
        push(
            MismatchKind::GameBuild {
                local: local.game_build.clone(),
                host: host.game_build.clone(),
            },
            false,
        );
    }
    if local.dlc != host.dlc {
        push(
            MismatchKind::Dlc {
                local: local.dlc,
                host: host.dlc,
            },
            false,
        );
    }
    let mine: BTreeMap<&str, &AddonPrint> = local.addons.iter().map(|a| (a.id.as_str(), a)).collect();
    let theirs: BTreeMap<&str, &AddonPrint> = host.addons.iter().map(|a| (a.id.as_str(), a)).collect();
    for h in theirs.values() {
        let (id, name) = (h.id.clone(), h.name.clone());
        match mine.get(h.id.as_str()) {
            Some(l)
                if (l.version != h.version || l.source != h.source)
                    && [&l.source, &h.source]
                        .iter()
                        .any(|s| matches!(s, Source::Builtin(_))) =>
            {
                push(
                    MismatchKind::Launcher {
                        local: local.launcher.clone(),
                        host: host.launcher.clone(),
                    },
                    false,
                )
            }
            None => push(
                MismatchKind::MissingAddon {
                    id,
                    name,
                    version: h.version.clone(),
                    source: h.source.clone(),
                    registry: h.registry.clone(),
                },
                listed(h),
            ),
            Some(l) if l.version != h.version => push(
                MismatchKind::VersionDiffers {
                    id,
                    name,
                    local: l.version.clone(),
                    host: h.version.clone(),
                    registry: h.registry.clone(),
                },
                listed(h),
            ),
            Some(l) if l.source != h.source => push(
                MismatchKind::SourceDiffers {
                    id,
                    name,
                    version: h.version.clone(),
                    local: l.source.clone(),
                    host: h.source.clone(),
                    registry: h.registry.clone(),
                },
                listed(h),
            ),
            Some(l) if l.stat_ids != h.stat_ids => push(MismatchKind::StatIdsDiffer { id, name }, false),
            Some(_) => {}
        }
        if let Some(l) = mine.get(h.id.as_str())
            && l.inputs != h.inputs
        {
            push(
                MismatchKind::InputsDiffer {
                    id: h.id.clone(),
                    name: h.name.clone(),
                },
                false,
            );
        }
    }
    for l in mine.values().filter(|l| !theirs.contains_key(l.id.as_str())) {
        push(
            MismatchKind::ExtraAddon {
                id: l.id.clone(),
                name: l.name.clone(),
                version: l.version.clone(),
            },
            !matches!(l.source, Source::Builtin(_)),
        );
    }
    let shared_order = |f: &Fingerprint, other: &BTreeMap<&str, &AddonPrint>| -> Vec<String> {
        let mut placed: Vec<(usize, &str)> = f
            .addons
            .iter()
            .filter(|a| other.get(a.id.as_str()).is_some_and(|o| o.order.is_some()))
            .filter_map(|a| Some((a.order?, a.id.as_str())))
            .collect();
        placed.sort();
        placed.into_iter().map(|(_, id)| id.to_owned()).collect()
    };
    let (mine_order, their_order) = (shared_order(local, &theirs), shared_order(host, &mine));
    let placed_alike = mine.iter().all(|(id, l)| {
        theirs
            .get(id)
            .is_none_or(|h| l.order.is_some() == h.order.is_some())
    });
    if mine_order != their_order || !placed_alike {
        push(
            MismatchKind::LoadOrder {
                local: mine_order,
                host: their_order,
            },
            true,
        );
    }
    for (addon, values) in host
        .options
        .iter()
        .filter(|(id, _)| mine.contains_key(id.as_str()))
    {
        let ours = local.options.get(addon);
        for (option, h) in values {
            let l = ours.and_then(|o| o.get(option));
            if l != Some(h) {
                push(
                    MismatchKind::OptionDiffers {
                        addon: addon.clone(),
                        option: option.clone(),
                        local: l.cloned(),
                        host: Some(h.clone()),
                    },
                    true,
                );
            }
        }
    }
    for (addon, values) in local
        .options
        .iter()
        .filter(|(id, _)| theirs.contains_key(id.as_str()))
    {
        let hosts = host.options.get(addon);
        for (option, l) in values
            .iter()
            .filter(|(o, _)| hosts.is_none_or(|h| !h.contains_key(*o)))
        {
            push(
                MismatchKind::OptionDiffers {
                    addon: addon.clone(),
                    option: option.clone(),
                    local: Some(l.clone()),
                    host: None,
                },
                false,
            );
        }
    }
    for m in &mut out {
        if let MismatchKind::SourceDiffers { id, .. } | MismatchKind::InputsDiffer { id, .. } = &m.kind
            && let (Some(l), Some(h)) = (mine.get(id.as_str()), theirs.get(id.as_str()))
        {
            m.differences = differences(l, h);
        }
        if let MismatchKind::StatIdsDiffer { id, .. } = &m.kind
            && let (Some(l), Some(h)) = (mine.get(id.as_str()), theirs.get(id.as_str()))
        {
            m.differences = stat_id_differences(l, h);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn store(name: &str) -> Store {
        let root = std::env::temp_dir().join(format!("swofty-fingerprint-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        Store::new(root)
    }

    fn put(store: &Store, manifest: Value, files: &[(&str, &str)]) {
        let dir = store.addons_dir().join(manifest["id"].as_str().unwrap());
        write(&dir, manifest, files);
    }

    fn write(dir: &Path, manifest: Value, files: &[(&str, &str)]) {
        std::fs::create_dir_all(dir).unwrap();
        let mut m = json!({
            "schema_version": 1,
            "name": manifest["id"],
            "version": "1.0.0",
            "author": "a",
            "compat": { "game": "f1_25" },
        });
        m.as_object_mut()
            .unwrap()
            .extend(manifest.as_object().unwrap().clone());
        std::fs::write(dir.join("swofty.json"), m.to_string()).unwrap();
        for (rel, text) in files {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
    }

    fn data(id: &str) -> Value {
        json!({ "id": id, "kind": ["database"], "operations": { "database": ["db/ops.json"] } })
    }

    fn code(id: &str) -> Value {
        json!({ "id": id, "kind": ["native"], "operations": { "native": [{ "dll": "native/x.dll" }] } })
    }

    fn profile(ids: &[&str]) -> Profile {
        let mut p = Profile::new("mp");
        for id in ids {
            p.set_enabled(id, true).unwrap();
        }
        p
    }

    fn game() -> Identity {
        Identity {
            build_id: "25251167".into(),
            exe_sha256: "a".repeat(64),
        }
    }

    fn ids(f: &Fingerprint) -> Vec<&str> {
        f.addons.iter().map(|a| a.id.as_str()).collect()
    }

    fn print(id: &str, version: &str, source: Source, order: Option<usize>) -> AddonPrint {
        AddonPrint {
            id: id.into(),
            name: format!("{id} name"),
            version: Version::parse(version).unwrap(),
            source,
            registry: None,
            inputs: None,
            order,
            detail: None,
            stat_ids: BTreeMap::new(),
        }
    }

    fn content(sha: &str) -> Source {
        Source::Content(sha.repeat(64))
    }

    fn installed_from(mut a: AddonPrint, sha: &str) -> AddonPrint {
        a.registry = Some(sha.repeat(64));
        a
    }

    fn options(entries: &[(&str, &str, Value)]) -> BTreeMap<String, BTreeMap<String, Value>> {
        let mut out: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        for (addon, option, v) in entries {
            out.entry((*addon).into())
                .or_default()
                .insert((*option).into(), v.clone());
        }
        out
    }

    fn host() -> Fingerprint {
        Fingerprint::new(
            game(),
            vec![
                installed_from(print("t.a", "1.0.0", content("a"), Some(0)), "1"),
                installed_from(print("t.b", "2.0.0", content("b"), Some(1)), "2"),
                print("t.code", "1.0.0", content("c"), None),
            ],
            options(&[("t.a", "laps", json!(5)), ("t.a", "mode", json!("full"))]),
            None,
        )
    }

    fn listing(id: &str, versions: &[(&str, &str)]) -> Addon {
        let versions: Vec<Value> = versions
            .iter()
            .map(|(v, sha)| json!({ "version": v, "size": 1, "sha256": sha.repeat(64), "url": "u" }))
            .collect();
        serde_json::from_value(json!({ "id": id, "name": id, "versions": versions })).unwrap()
    }

    fn kinds(found: &[Mismatch]) -> Vec<(String, bool)> {
        found
            .iter()
            .map(|m| {
                let v = serde_json::to_value(m).unwrap();
                (v["kind"].as_str().unwrap().to_owned(), m.fixable)
            })
            .collect()
    }

    #[test]
    fn digest_is_stable_and_ignores_names_and_listing_order() {
        let f = host();
        assert_eq!(
            f.digest,
            "9547802c5a3ed7f1f2ea3d12b22a21b6f96dbf61fe0f29b65f61fb860a96bbd9"
        );
        let mut shuffled = f.addons.clone();
        shuffled.reverse();
        shuffled[0].name = "Another name".into();
        let again = Fingerprint::new(game(), shuffled, f.options.clone(), None);
        assert_eq!(again.digest, f.digest);
        assert_eq!(ids(&again), ["t.a", "t.b", "t.code"]);

        let text = serde_json::to_string(&f).unwrap();
        let read: Fingerprint = serde_json::from_str(&text).unwrap();
        assert_eq!(read, f);
        assert_eq!(read.canonical_digest(), f.digest);

        let changed = |edit: &dyn Fn(&mut Fingerprint)| {
            let mut c = host();
            edit(&mut c);
            c.canonical_digest()
        };
        for d in [
            changed(&|c| c.addons[0].version = Version::new(1, 0, 1)),
            changed(&|c| c.addons[1].source = content("d")),
            changed(&|c| c.addons[1].inputs = Some("d".repeat(64))),
            changed(&|c| c.addons[0].order = Some(2)),
            changed(&|c| {
                c.options.get_mut("t.a").unwrap().insert("laps".into(), json!(6));
            }),
            changed(&|c| c.game_build.exe_sha256 = "b".repeat(64)),
            changed(&|c| c.dlc = Some(true)),
        ] {
            assert_ne!(d, f.digest);
        }
        assert_eq!(changed(&|c| c.addons[0].registry = None), f.digest);
    }

    #[test]
    fn fingerprints_a_profile_the_same_way_twice() {
        let store = store("stable");
        put(&store, data("t.data1"), &[("db/ops.json", "1")]);
        put(&store, data("t.data2"), &[("db/ops.json", "2")]);
        put(&store, code("t.code1"), &[("native/x.dll", "c1")]);
        put(&store, code("t.code2"), &[("native/x.dll", "c2")]);
        let p = profile(&["t.data1", "t.code1", "t.data2", "t.code2"]);
        let first = fingerprint(&store, &p, &game()).unwrap();
        assert_eq!(
            ids(&first),
            ["swofty.core", "t.code1", "t.code2", "t.data1", "t.data2"]
        );
        let order: Vec<(&str, Option<usize>)> =
            first.addons.iter().map(|a| (a.id.as_str(), a.order)).collect();
        assert_eq!(
            order,
            [
                ("swofty.core", Some(0)),
                ("t.code1", None),
                ("t.code2", None),
                ("t.data1", Some(1)),
                ("t.data2", Some(2)),
            ]
        );
        assert!(store.root.join(HASHES_FILE).is_file());
        assert_eq!(fingerprint(&store, &p, &game()).unwrap(), first);

        let code_swapped = profile(&["t.data1", "t.code2", "t.data2", "t.code1"]);
        assert_eq!(
            fingerprint(&store, &code_swapped, &game()).unwrap().digest,
            first.digest
        );
        let data_swapped = profile(&["t.data2", "t.code1", "t.data1", "t.code2"]);
        assert_ne!(
            fingerprint(&store, &data_swapped, &game()).unwrap().digest,
            first.digest
        );

        std::fs::write(store.addons_dir().join("t.code1/native/x.dll"), "c1 changed").unwrap();
        let edited = fingerprint(&store, &p, &game()).unwrap();
        assert_ne!(edited.addons[1].source, first.addons[1].source);
        assert_eq!(edited.addons[2].source, first.addons[2].source);
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn leaves_out_cosmetic_addons_and_options() {
        let store = store("cosmetic");
        let look = json!({
            "id": "t.look", "experimental_features": ["shaders"], "shader": { "lut": "look.cube" },
        });
        let passes = json!({
            "id": "t.passes", "experimental_features": ["postfx"], "postfx": [{ "shader": "fx/a.hlsl" }],
        });
        let mut helmets = data("t.helmets");
        helmets["category"] = json!("helmets");
        let mut shaders = data("t.shaders");
        shaders["category"] = json!("shaders");
        let mut kept_helmets = data("t.kept-helmets");
        kept_helmets["category"] = json!("helmets");
        kept_helmets["multiplayer"] = json!("gameplay");
        let mut quiet = data("t.quiet");
        quiet["multiplayer"] = json!("cosmetic");
        let mut cars = data("t.cars");
        cars["category"] = json!("cars");
        cars["options"] = json!([
            { "id": "laps", "type": "slider", "min": 1, "max": 10, "step": 1, "default": 5, "binds": "db" },
            { "id": "hud", "type": "toggle", "default": true, "binds": "native", "multiplayer": "cosmetic" },
        ]);
        for m in [look, passes, helmets, shaders, kept_helmets, quiet, cars] {
            put(&store, m, &[]);
        }
        let all = [
            "t.look",
            "t.passes",
            "t.helmets",
            "t.shaders",
            "t.kept-helmets",
            "t.quiet",
            "t.cars",
        ];
        let mut p = profile(&all);
        let installed = store.addons().unwrap();
        p.set_option(&installed["t.cars"], "laps", json!(7)).unwrap();
        let f = fingerprint(&store, &p, &game()).unwrap();
        assert_eq!(ids(&f), ["swofty.core", "t.cars", "t.kept-helmets"]);
        assert_eq!(f.options, options(&[("t.cars", "laps", json!(7))]));

        let without = fingerprint(&store, &profile(&["t.kept-helmets", "t.cars"]), &game()).unwrap();
        let mut seven = profile(&["t.kept-helmets", "t.cars"]);
        seven.set_option(&installed["t.cars"], "laps", json!(7)).unwrap();
        assert_ne!(without.digest, f.digest);
        assert_eq!(fingerprint(&store, &seven, &game()).unwrap().digest, f.digest);
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn manifest_multiplayer_defaults() {
        let m = |extra: Value| {
            let mut doc = json!({
                "schema_version": 1, "id": "t.m", "name": "m", "version": "1.0.0", "author": "a",
                "compat": { "game": "f1_25" },
            });
            doc.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            AddonManifest::parse(&doc.to_string()).unwrap().multiplayer()
        };
        use Multiplayer::{Cosmetic, Gameplay};
        assert_eq!(m(json!({})), Gameplay);
        assert_eq!(m(json!({ "category": "helmets" })), Cosmetic);
        assert_eq!(m(json!({ "category": "shaders" })), Cosmetic);
        assert_eq!(m(json!({ "category": "performance" })), Cosmetic);
        let declared = r#"{
            "schema_version": 1,
            "id": "xanpavle.re-optimization",
            "name": "F1 25 RE-Optimization",
            "author": "xanpavle",
            "version": "0.2.5",
            "kind": ["native"],
            "compat": { "game": "f1_25", "game_builds": ["=25251167"], "seasons": [] },
            "multiplayer": "cosmetic"
        }"#;
        assert_eq!(AddonManifest::parse(declared).unwrap().multiplayer(), Cosmetic);
        assert_eq!(m(json!({ "category": "cars" })), Gameplay);
        assert_eq!(
            m(json!({ "category": "helmets", "multiplayer": "gameplay" })),
            Gameplay
        );
        assert_eq!(m(json!({ "multiplayer": "cosmetic" })), Cosmetic);
        let look = json!({ "experimental_features": ["shaders"], "shader": { "lut": "l.cube" } });
        assert_eq!(m(look.clone()), Cosmetic);
        let mut with_data = look;
        with_data["operations"] = json!({ "database": ["db/x.json"] });
        assert_eq!(m(with_data), Gameplay);
        assert!(AddonManifest::parse(r#"{"schema_version":1,"id":"t.m","name":"m","version":"1.0.0","author":"a","compat":{"game":"f1_25"},"multiplayer":"sometimes"}"#).is_err());
    }

    #[test]
    fn a_registry_install_is_recorded_beside_the_content_hash() {
        let store = store("registry");
        put(&store, data("t.a"), &[("db/ops.json", "1")]);
        let p = profile(&["t.a"]);
        let print = |store: &Store| fingerprint(store, &p, &game()).unwrap().addons[1].clone();
        let before = print(&store);
        assert!(matches!(before.source, Source::Content(_)));
        assert_eq!(before.registry, None);

        let mut v: catalog::Version = serde_json::from_value(json!({
            "version": "1.0.0", "size": 1, "sha256": "AB".repeat(32), "url": "u",
        }))
        .unwrap();
        catalog::record_install(&store, "t.a", &v).unwrap();
        let recorded = print(&store);
        assert_eq!(recorded.registry, Some("ab".repeat(32)));
        assert_eq!(recorded.source, before.source);

        v.version = Version::new(0, 9, 0);
        catalog::record_install(&store, "t.a", &v).unwrap();
        assert_eq!(print(&store).registry, None);

        v.version = Version::new(1, 0, 0);
        catalog::record_install(&store, "t.a", &v).unwrap();
        let copy = store.root.join("copy");
        write(&copy, data("t.a"), &[("db/ops.json", "1")]);
        store.install_addon(&copy).unwrap();
        assert!(!store.registry_install_path("t.a").exists());
        assert_eq!(print(&store), before);
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn the_same_files_match_however_they_were_installed() {
        let (dropped, downloaded) = (store("dropped"), store("downloaded"));
        for s in [&dropped, &downloaded] {
            put(s, data("t.a"), &[("db/ops.json", "1")]);
        }
        let v: catalog::Version = serde_json::from_value(json!({
            "version": "1.0.0", "size": 1, "sha256": "ab".repeat(32), "url": "u",
        }))
        .unwrap();
        catalog::record_install(&downloaded, "t.a", &v).unwrap();
        let p = profile(&["t.a"]);
        let (local, host) = (
            fingerprint(&dropped, &p, &game()).unwrap(),
            fingerprint(&downloaded, &p, &game()).unwrap(),
        );
        assert_eq!(host.addons[1].registry, Some("ab".repeat(32)));
        assert_eq!(local.digest, host.digest);
        assert!(compare(&local, &host, &[]).is_empty());
        for s in [dropped, downloaded] {
            std::fs::remove_dir_all(&s.root).ok();
        }
    }

    #[test]
    fn a_build_hook_prints_its_addon_data_and_profile_state() {
        let store = store("hook");
        let hook = json!({
            "id": "t.hook", "kind": ["database"],
            "launcher": { "program": { "linux": "bin/hook", "windows": "bin/hook.exe" }, "build": true },
        });
        put(&store, hook, &[]);
        put(&store, data("t.plain"), &[("db/ops.json", "1")]);
        let data_dir = store.addon_data_dir("t.hook");
        std::fs::create_dir_all(data_dir.join("logs")).unwrap();
        std::fs::write(data_dir.join("drivers.json"), r#"{"drivers":["a"]}"#).unwrap();
        let mut p = profile(&["t.hook", "t.plain"]);
        let inputs = |p: &Profile| {
            let f = fingerprint(&store, p, &game()).unwrap();
            assert_eq!(f.addons[2].inputs, None);
            f.addons[1].inputs.clone().unwrap()
        };
        let first = inputs(&p);

        std::fs::write(data_dir.join("logs/build.log"), "noise").unwrap();
        std::fs::write(data_dir.join("run.log"), "noise").unwrap();
        std::fs::create_dir_all(data_dir.join(".cache")).unwrap();
        std::fs::write(data_dir.join(".cache/view.json"), "noise").unwrap();
        assert_eq!(inputs(&p), first);

        std::fs::write(data_dir.join("drivers.json"), r#"{"drivers":["b"]}"#).unwrap();
        let other_drivers = inputs(&p);
        assert_ne!(other_drivers, first);

        std::fs::write(data_dir.join("drivers.json"), r#"{"drivers":["a"]}"#).unwrap();
        p.set_state("t.hook", json!({ "mods": ["x"] })).unwrap();
        let other_state = inputs(&p);
        assert_ne!(other_state, first);
        p.set_state("t.hook", json!({ "mods": ["y"] })).unwrap();
        assert_ne!(inputs(&p), other_state);
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn import_records_that_differ_only_in_their_hidden_sidecar_print_the_same() {
        let hook = json!({
            "id": "t.importer", "kind": ["database"],
            "launcher": { "program": { "linux": "bin/hook", "windows": "bin/hook.exe" }, "build": true },
        });
        let record = r#"{"mods":{"one":{"name":"One","converter":3,"made":{"id":"mod.one","name":"One"}}}}"#;
        let p = profile(&["t.importer"]);
        let prints: Vec<Fingerprint> = [(1792000000, "C:/Mods/one.zip"), (1795000000, "/home/b/one.zip")]
            .into_iter()
            .enumerate()
            .map(|(i, (at, file))| {
                let store = store(&format!("importer{i}"));
                put(&store, hook.clone(), &[]);
                let data = store.addon_data_dir("t.importer");
                std::fs::create_dir_all(&data).unwrap();
                std::fs::write(data.join("importer.json"), record).unwrap();
                let local = json!({ "mods": { "one": { "at": at } }, "last": { "at": at, "files": [file], "mods": ["one"] } });
                std::fs::write(data.join(".importer-local.json"), local.to_string()).unwrap();
                let f = fingerprint(&store, &p, &game()).unwrap();
                std::fs::remove_dir_all(&store.root).ok();
                f
            })
            .collect();
        assert!(prints[0].addons[1].inputs.is_some());
        assert_eq!(prints[0].digest, prints[1].digest);
    }

    fn detailed(id: &str, digest: &str, parts: &[(&str, &str)], reads: &[(&str, &str)]) -> AddonPrint {
        let mut a = print(id, "1.0.0", Source::Built(digest.repeat(32)), Some(0));
        let list: String = reads.iter().map(|(p, _)| *p).collect::<Vec<_>>().join("\n");
        a.detail = Some(Detail {
            parts: parts.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect(),
            read_list: short(&sha256_bytes(list.as_bytes())),
            read_count: reads.len(),
            reads: reads.iter().map(|(_, h)| (*h).into()).collect(),
            paths: reads.iter().map(|(p, _)| (*p).into()).collect(),
            row_ids: BTreeMap::new(),
        });
        a
    }

    #[test]
    fn a_row_with_another_stat_id_counts_and_is_named() {
        let store = Store::new(std::env::temp_dir().join(format!("swofty-fp-rows-{}", std::process::id())));
        let mut hashes = Hashes::open(store.root.join(HASHES_FILE));
        let output = |id: i64| Output {
            made: "m".into(),
            row_ids: BTreeMap::from([
                ("entrants.vehicles.vehicle/car_a".to_owned(), 512),
                ("entrants.vehicles.vehicle/car_b".to_owned(), id),
            ]),
            ..Output::default()
        };
        let (mine, detail) = output(513).digest(&store, "t.cars", &mut hashes).unwrap();
        let (theirs, other) = output(600).digest(&store, "t.cars", &mut hashes).unwrap();
        assert_ne!(mine, theirs);
        let mut local = detailed("t.cars", "aa", &[("Database", "d1")], &[]);
        local.detail.as_mut().unwrap().row_ids = detail.row_ids;
        let mut host = detailed("t.cars", "bb", &[("Database", "d1")], &[]);
        host.detail.as_mut().unwrap().row_ids = other.row_ids;
        let found = compare(&with(vec![local]), &with(vec![host]), &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(
            found[0].differences,
            ["row entrants.vehicles.vehicle/car_b has stat id 513 here and 600 in the other copy"]
        );
    }

    #[test]
    fn a_room_launch_hides_only_what_its_build_cures() {
        let source = |differences: &[&str]| Mismatch {
            kind: MismatchKind::SourceDiffers {
                id: "a.b".into(),
                name: "A".into(),
                version: Version::new(1, 0, 0),
                local: Source::Content("x".into()),
                host: Source::Built("y".into()),
                registry: None,
            },
            fixable: false,
            differences: differences.iter().map(|d| (*d).to_owned()).collect(),
        };
        let unbuilt =
            source(&["your game wasn't built with it, so its files count instead; start F1 25 again"]);
        let ids = source(&["row t.r/x has stat id 269 here and 297 in the other copy"]);
        let files = source(&["it reads other files: 0 here, 2 at the host"]);
        let missing = Mismatch {
            kind: MismatchKind::MissingAddon {
                id: "c.d".into(),
                name: "C".into(),
                version: Version::new(1, 0, 0),
                source: Source::Content("z".into()),
                registry: None,
            },
            fixable: true,
            differences: Vec::new(),
        };
        let all = vec![unbuilt, ids, files.clone(), missing.clone()];
        assert_eq!(
            unresolved_by_launch(&all, true, &Default::default()),
            vec![files, missing]
        );
        assert_eq!(unresolved_by_launch(&all, false, &Default::default()), all);
    }

    #[test]
    fn building_with_the_hosts_ids_makes_the_fingerprints_match() {
        let store = Store::new(std::env::temp_dir().join(format!("swofty-fp-lent-{}", std::process::id())));
        let mut hashes = Hashes::open(store.root.join(HASHES_FILE));
        let output = |a: i64, b: i64| Output {
            made: "m".into(),
            row_ids: BTreeMap::from([
                ("entrants.vehicles.vehicle/car_a".to_owned(), a),
                ("entrants.vehicles.vehicle/car_b".to_owned(), b),
            ]),
            ..Output::default()
        };
        let mut print_of = |o: Output| {
            let (digest, detail) = o.digest(&store, "t.cars", &mut hashes).unwrap();
            let mut a = detailed("t.cars", "aa", &[("Database", "d1")], &[]);
            a.source = Source::Built(digest);
            a.detail = Some(Detail {
                row_ids: detail.row_ids,
                ..a.detail.unwrap()
            });
            a
        };
        let host = with(vec![print_of(output(512, 513))]);
        let local = with(vec![print_of(output(513, 512))]);
        assert_ne!(local.digest, host.digest);
        let found = compare(&local, &host, &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].cured_by_build(), "{found:?}");
        let ids = row_id_overrides(&local, &host);
        assert_eq!(ids.rows["t.cars"]["entrants.vehicles.vehicle/car_a"], 512);
        let lent = with(vec![print_of(Output {
            row_ids: ids.rows["t.cars"].clone(),
            ..output(0, 0)
        })]);
        assert_eq!(lent.digest, host.digest);
        assert!(compare(&lent, &host, &[]).is_empty());
        assert!(row_id_overrides(&lent, &host).is_empty());
    }

    fn with(addons: Vec<AddonPrint>) -> Fingerprint {
        Fingerprint::new(game(), addons, BTreeMap::new(), None)
    }

    #[test]
    fn a_different_copy_names_the_part_and_the_files_that_differ() {
        let reads = [("a.erp", "aaaa"), ("b.erp", "bbbb"), ("c.erp", "cccc")];
        let local = with(vec![detailed(
            "t.cars",
            "aa",
            &[("Database", "d1"), ("native", "n1")],
            &reads,
        )]);
        let mut theirs = reads;
        theirs[1].1 = "beef";
        let host = with(vec![detailed(
            "t.cars",
            "bb",
            &[("Database", "d2"), ("native", "n1")],
            &theirs,
        )]);
        let found = compare(&local, &host, &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(matches!(found[0].kind, MismatchKind::SourceDiffers { .. }));
        assert_eq!(
            found[0].differences,
            ["its database operations differ", "b.erp differs"]
        );
        let blob = serde_json::to_value(&host).unwrap();
        assert!(
            blob.pointer("/addons/0/detail/paths").is_none(),
            "paths stay local"
        );
        assert_eq!(
            serde_json::from_value::<Fingerprint>(blob).unwrap().digest,
            host.digest
        );
    }

    #[test]
    fn a_different_read_list_is_named_with_both_counts() {
        let local = with(vec![detailed(
            "t.cars",
            "aa",
            &[],
            &[("a.erp", "aaaa"), ("b.erp", "bbbb")],
        )]);
        let host = with(vec![detailed("t.cars", "bb", &[], &[("a.erp", "aaaa")])]);
        assert_eq!(
            compare(&local, &host, &[])[0].differences,
            ["it reads other files: 2 here, 1 at the host"]
        );
        let many: Vec<(String, String)> = (0..8).map(|i| (format!("f{i}.erp"), format!("{i:04}"))).collect();
        let theirs: Vec<(String, String)> = many.iter().map(|(p, _)| (p.clone(), "ffff".into())).collect();
        fn refs(v: &[(String, String)]) -> Vec<(&str, &str)> {
            v.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect()
        }
        let found = compare(
            &with(vec![detailed("t.cars", "aa", &[], &refs(&many))]),
            &with(vec![detailed("t.cars", "bb", &[], &refs(&theirs))]),
            &[],
        );
        assert_eq!(found[0].differences.len(), NAMED_FILES + 1);
        assert_eq!(found[0].differences.last().unwrap(), "and 3 more file(s) differ");
    }

    #[test]
    fn a_host_without_a_build_record_is_told_to_restart_and_an_old_peer_names_nothing() {
        let local = with(vec![detailed("t.cars", "aa", &[("Database", "d1")], &[])]);
        let unbuilt = with(vec![print("t.cars", "1.0.0", content("cc"), Some(0))]);
        assert_eq!(
            compare(&local, &unbuilt, &[])[0].differences,
            [
                "the host's game wasn't built with it, so its files count instead; the host needs to start F1 25 again"
            ]
        );
        let old = with(vec![print(
            "t.cars",
            "1.0.0",
            Source::Built("dd".repeat(32)),
            Some(0),
        )]);
        let found = compare(&local, &old, &[]);
        assert!(matches!(found[0].kind, MismatchKind::SourceDiffers { .. }));
        assert!(found[0].differences.is_empty());
        let json = serde_json::to_value(&found[0]).unwrap();
        assert!(json.get("differences").is_none());
    }

    #[test]
    fn different_build_hook_inputs_are_not_fixable() {
        let mut local = host();
        local.addons[1].inputs = Some("1".repeat(64));
        let mut h = host();
        h.addons[1].inputs = Some("2".repeat(64));
        let found = compare(&local, &h, &[]);
        assert_eq!(kinds(&found), [("inputs_differ".into(), false)]);
        assert_eq!(
            found[0].line(),
            "t.b name (t.b) has other data or profile state than the host's"
        );
    }

    #[test]
    fn matching_fingerprints_have_nothing_to_fix() {
        assert!(compare(&host(), &host(), &[]).is_empty());
    }

    #[test]
    fn game_build_and_dlc_differences_are_not_fixable() {
        let mut local = host();
        local.game_build.exe_sha256 = "b".repeat(64);
        assert_eq!(
            kinds(&compare(&local, &host(), &[])),
            [("game_build".into(), false)]
        );

        let mut local = host();
        local.game_build.build_id = format!("exe-{}", "a".repeat(12));
        assert!(compare(&local, &host(), &[]).is_empty());
        assert_eq!(local.canonical_digest(), host().digest);
        local.game_build.build_id = "25300000".into();
        local.game_build.exe_sha256 = "A".repeat(64);
        assert!(compare(&local, &host(), &[]).is_empty());
        assert_eq!(local.canonical_digest(), host().digest);

        let (mut local, mut h) = (host(), host());
        local.dlc = Some(false);
        assert_eq!(kinds(&compare(&local, &h, &[])), [("dlc".into(), false)]);
        assert_eq!(
            compare(&local, &h, &[])[0].line(),
            "DLC is not owned; the host's is unknown"
        );
        h.dlc = Some(false);
        assert!(compare(&local, &h, &[]).is_empty());
        h.dlc = Some(true);
        assert_eq!(kinds(&compare(&local, &h, &[])), [("dlc".into(), false)]);
    }

    #[test]
    fn a_different_launcher_is_reported_as_one() {
        let core = |f: &mut Fingerprint, sha: &str| {
            f.addons
                .push(print(CORE_ID, "0.2.13", Source::Builtin(sha.repeat(64)), Some(0)));
            f.addons.sort_by(|a, b| a.id.cmp(&b.id));
        };
        let (mut local, mut h) = (host(), host());
        core(&mut local, "1");
        core(&mut h, "2");
        local.launcher = "1.5.12".into();
        h.launcher = "1.5.13".into();
        let found = compare(&local, &h, &[]);
        assert_eq!(kinds(&found), [("launcher".into(), false)]);
        assert_eq!(
            found[0].line(),
            "Swofty Launcher 1.5.12 is not the host's 1.5.13; play with the same launcher version"
        );
        h.launcher = "1.5.12".into();
        assert_eq!(
            compare(&local, &h, &[])[0].line(),
            "this Swofty Launcher 1.5.12 is a different build from the host's 1.5.12"
        );
        let mut newer_core = local.clone();
        newer_core.addons[0].version = Version::new(0, 2, 14);
        assert_eq!(
            kinds(&compare(&local, &newer_core, &[])),
            [("launcher".into(), false)]
        );
    }

    #[test]
    fn the_core_print_covers_the_launcher_and_its_database_operations() {
        let store = store("core");
        let f = fingerprint(&store, &profile(&[]), &game()).unwrap();
        let Source::Builtin(sha) = &f.addons[0].source else {
            panic!("{:?}", f.addons[0].source);
        };
        let mut h = Sha256::new();
        h.update(format!("launcher {}\0", env!("CARGO_PKG_VERSION")));
        let dll = loader_dir().map(|d| d.join(CORE_DLL)).filter(|p| p.is_file());
        let dll = dll.map(|p| crate::baseline::sha256_file(&p).unwrap());
        h.update(format!("dll {}\0", dll.as_deref().unwrap_or("none")));
        h.update(CORE_MANIFEST);
        h.update(CORE_DB);
        assert_eq!(*sha, hex(&h.finalize()));
        assert_eq!(f.launcher, env!("CARGO_PKG_VERSION"));
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn a_missing_addon_is_fixable_when_the_registry_lists_that_download() {
        let mut local = host();
        local.addons.retain(|a| a.id != "t.b");
        let listed = [listing("t.b", &[("2.0.0", "2"), ("1.0.0", "e")])];
        let found = compare(&local, &host(), &listed);
        assert_eq!(
            found[0].kind,
            MismatchKind::MissingAddon {
                id: "t.b".into(),
                name: "t.b name".into(),
                version: Version::new(2, 0, 0),
                source: content("b"),
                registry: Some("2".repeat(64)),
            }
        );
        assert!(found[0].fixable);
        assert_eq!(found[0].line(), "t.b name (t.b) 2.0.0 is missing (fixable)");
        assert!(!compare(&local, &host(), &[listing("t.b", &[("2.0.0", "f")])])[0].fixable);
        assert!(!compare(&local, &host(), &[listing("t.b", &[("1.0.0", "2")])])[0].fixable);
        assert!(!compare(&local, &host(), &[])[0].fixable);

        let mut code_host = host();
        let mut code_local = host();
        code_local.addons.retain(|a| a.id != "t.code");
        let missing = compare(&code_local, &code_host, &[listing("t.code", &[("1.0.0", "c")])]);
        assert_eq!(kinds(&missing)[0], ("missing_addon".into(), false));
        code_host.addons.clear();
        assert!(
            kinds(&compare(&code_local, &code_host, &[]))
                .iter()
                .all(|(k, _)| k == "extra_addon")
        );
    }

    #[test]
    fn an_extra_addon_is_fixable_by_turning_it_off() {
        let mut local = host();
        local.addons.push(print(
            "t.extra",
            "1.0.0",
            Source::Content("e".repeat(64)),
            Some(2),
        ));
        local
            .options
            .insert("t.extra".into(), BTreeMap::from([("x".into(), json!(true))]));
        let found = compare(&local, &host(), &[]);
        assert_eq!(kinds(&found), [("extra_addon".into(), true)]);
        assert_eq!(
            found[0].line(),
            "t.extra name (t.extra) 1.0.0 is on; the host doesn't have it (fixable)"
        );
        let core = |f: &mut Fingerprint| {
            f.addons
                .push(print(CORE_ID, "0.2.13", Source::Builtin("1".repeat(64)), None))
        };
        let mut local = host();
        core(&mut local);
        assert_eq!(
            kinds(&compare(&local, &host(), &[])),
            [("extra_addon".into(), false)]
        );
    }

    #[test]
    fn a_version_difference_is_fixable_when_the_registry_has_the_hosts_version() {
        let mut local = host();
        local.addons[1].version = Version::new(1, 0, 0);
        local.addons[1].source = content("e");
        let found = compare(
            &local,
            &host(),
            &[listing("t.b", &[("2.0.0", "2"), ("1.0.0", "e")])],
        );
        assert_eq!(
            found[0].kind,
            MismatchKind::VersionDiffers {
                id: "t.b".into(),
                name: "t.b name".into(),
                local: Version::new(1, 0, 0),
                host: Version::new(2, 0, 0),
                registry: Some("2".repeat(64)),
            }
        );
        assert_eq!(kinds(&found), [("version_differs".into(), true)]);
        assert_eq!(
            kinds(&compare(&local, &host(), &[])),
            [("version_differs".into(), false)]
        );
    }

    #[test]
    fn a_source_difference_is_fixable_by_installing_the_hosts_download() {
        let mut local = host();
        local.addons[0].source = content("9");
        let listed = [listing("t.a", &[("1.0.0", "1")])];
        let found = compare(&local, &host(), &listed);
        assert_eq!(
            found[0].kind,
            MismatchKind::SourceDiffers {
                id: "t.a".into(),
                name: "t.a name".into(),
                version: Version::new(1, 0, 0),
                local: content("9"),
                host: content("a"),
                registry: Some("1".repeat(64)),
            }
        );
        assert_eq!(kinds(&found), [("source_differs".into(), true)]);
        let mut h = host();
        h.addons[0].registry = None;
        assert_eq!(
            kinds(&compare(&local, &h, &listed)),
            [("source_differs".into(), false)]
        );
    }

    #[test]
    fn option_differences_are_fixable() {
        let mut local = host();
        local.options = options(&[("t.a", "laps", json!(3))]);
        let found = compare(&local, &host(), &[]);
        assert_eq!(
            kinds(&found),
            [("option_differs".into(), true), ("option_differs".into(), true)]
        );
        assert_eq!(
            found[0].kind,
            MismatchKind::OptionDiffers {
                addon: "t.a".into(),
                option: "laps".into(),
                local: Some(json!(3)),
                host: Some(json!(5)),
            }
        );
        assert_eq!(found[0].line(), "t.a.laps is 3; the host has 5 (fixable)");
        assert_eq!(
            found[1].line(),
            "t.a.mode is unset; the host has \"full\" (fixable)"
        );

        let mut local = host();
        local.addons.retain(|a| a.id != "t.a");
        local.options.clear();
        assert_eq!(
            kinds(&compare(&local, &host(), &[])),
            [("missing_addon".into(), false)]
        );
    }

    #[test]
    fn a_load_order_difference_among_shared_data_addons_is_fixable() {
        let mut local = host();
        local.addons[0].order = Some(1);
        local.addons[1].order = Some(0);
        let found = compare(&local, &host(), &[]);
        assert_eq!(
            found[0].kind,
            MismatchKind::LoadOrder {
                local: vec!["t.b".into(), "t.a".into()],
                host: vec!["t.a".into(), "t.b".into()],
            }
        );
        assert_eq!(kinds(&found), [("load_order".into(), true)]);

        let mut local = host();
        local.addons.push(print(
            "t.extra",
            "1.0.0",
            Source::Content("e".repeat(64)),
            Some(0),
        ));
        local.addons[0].order = Some(1);
        local.addons[1].order = Some(2);
        assert_eq!(
            kinds(&compare(&local, &host(), &[])),
            [("extra_addon".into(), true)]
        );
    }

    fn differing_local() -> Fingerprint {
        let mut local = host();
        local.addons[0].order = Some(1);
        local.addons[1].order = Some(0);
        local.options = options(&[("t.a", "laps", json!(3)), ("t.a", "mode", json!("full"))]);
        Fingerprint::new(local.game_build.clone(), local.addons, local.options, None)
    }

    fn with_ids(mut f: Fingerprint, addon: &str, ids: &[(&str, i64)]) -> Fingerprint {
        for a in &mut f.addons {
            if a.id == addon {
                a.stat_ids = ids.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect();
            }
        }
        Fingerprint::new(f.game_build.clone(), f.addons, f.options, None)
    }

    const PACKAGE: &str = "content.package:t.a/season";
    const SCENARIO: &str = "content.package.scenario:t.a/season";

    #[test]
    fn the_core_registry_ids_an_addon_holds_are_read_from_ids_json() {
        let doc = json!({
            "ids": {
                "content.package": {
                    "t.a/season": { "id": 116, "since": "2026-11-08" },
                    "t.ab/other": 9,
                    "game/stock": { "id": 1 }
                },
                "content.package.scenario": { "t.a/season": { "id": 26 } }
            }
        });
        assert_eq!(
            stat_ids_of(&doc, "t.a"),
            BTreeMap::from([(PACKAGE.to_owned(), 116), (SCENARIO.to_owned(), 26)])
        );
        assert!(stat_ids_of(&Value::Null, "t.a").is_empty());
    }

    #[test]
    fn registry_ids_are_part_of_the_digest_and_name_the_record_that_differs() {
        let hosted = with_ids(host(), "t.a", &[(PACKAGE, 116), (SCENARIO, 26)]);
        let local = with_ids(host(), "t.a", &[(PACKAGE, 115), (SCENARIO, 25)]);
        assert_ne!(hosted.digest, local.digest);
        assert_ne!(hosted.digest, host().digest);
        let found = compare(&local, &hosted, &[]);
        assert_eq!(kinds(&found), [("stat_ids_differ".into(), false)]);
        assert_eq!(
            found[0].differences,
            [
                "content.package.scenario t.a/season has stat id 25 here and 26 at the host",
                "content.package t.a/season has stat id 115 here and 116 at the host"
            ]
        );
        assert!(compare(&hosted, &hosted, &[]).is_empty());
    }

    #[test]
    fn the_hosts_registry_ids_are_lent_and_make_the_fingerprint_the_hosts() {
        let hosted = with_ids(host(), "t.a", &[(PACKAGE, 116), (SCENARIO, 26)]);
        let local = with_ids(
            host(),
            "t.a",
            &[(PACKAGE, 115), (SCENARIO, 25), ("vehicle:t.a/car", 7)],
        );
        let lent = host_overrides(&local, &hosted);
        assert_eq!(
            lent.registry,
            BTreeMap::from([(
                "t.a".to_owned(),
                BTreeMap::from([(PACKAGE.to_owned(), 116), (SCENARIO.to_owned(), 26)])
            )])
        );
        assert!(lent.rows.is_empty() && lent.order.is_empty() && lent.options.is_empty());
        assert_eq!(local.lent(&lent).digest, hosted.digest);
        assert!(compare(&local.lent(&lent), &hosted, &[]).is_empty());
        assert!(host_overrides(&hosted, &hosted).is_empty());
        let found = compare(&local, &hosted, &[]);
        assert!(unresolved_by_launch(&found, true, &lent).is_empty());
        assert_eq!(unresolved_by_launch(&found, false, &lent).len(), 1);
    }

    #[test]
    fn registry_ids_are_lent_only_for_addons_the_local_side_has() {
        let hosted = with_ids(host(), "t.b", &[("vehicle:t.b/car", 7)]);
        let mut local = host();
        local.addons.retain(|a| a.id != "t.b");
        let local = Fingerprint::new(game(), local.addons, local.options, None);
        assert!(registry_id_overrides(&local, &hosted).is_empty());
    }

    #[test]
    fn a_host_with_no_registry_ids_for_an_addon_clears_the_joiners_so_the_prints_match() {
        let local = with_ids(host(), "t.a", &[(PACKAGE, 115)]);
        let lent = host_overrides(&local, &host());
        assert_eq!(lent.registry["t.a"], BTreeMap::new());
        assert!(!lent.is_empty());
        assert_eq!(local.lent(&lent).digest, host().digest);
        assert!(lent.room_stat_ids(&["t.a".to_owned()]).is_none());
    }

    #[test]
    fn the_hosts_order_and_differing_options_are_lent_for_the_addons_both_have() {
        let lent = host_overrides(&differing_local(), &host());
        assert_eq!(lent.order, ["t.a", "t.b"]);
        assert_eq!(lent.options, options(&[("t.a", "laps", json!(5))]));
        assert!(host_overrides(&host(), &host()).is_empty());
    }

    #[test]
    fn an_addon_at_another_version_or_missing_is_not_lent_anything() {
        let mut local = differing_local();
        local.addons[0].version = Version::parse("1.1.0").unwrap();
        let lent = host_overrides(&local, &host());
        assert!(!lent.order.contains(&"t.a".to_owned()), "{lent:?}");
        assert!(lent.options.is_empty(), "{lent:?}");
        let mut local = differing_local();
        local.addons.retain(|a| a.id != "t.b");
        assert!(host_overrides(&local, &host()).order.is_empty());
    }

    #[test]
    fn a_fingerprint_built_with_the_lent_settings_is_the_hosts() {
        let local = differing_local();
        assert_ne!(local.digest, host().digest);
        let lent = host_overrides(&local, &host());
        assert_eq!(local.lent(&lent).digest, host().digest);
        assert!(compare(&local.lent(&lent), &host(), &[]).is_empty());
    }

    #[test]
    fn the_launch_resolves_load_order_and_option_mismatches_but_not_the_rest() {
        let local = differing_local();
        let lent = host_overrides(&local, &host());
        let mut found = compare(&local, &host(), &[]);
        found.extend(compare(
            &Fingerprint::new(game(), Vec::new(), BTreeMap::new(), None),
            &host(),
            &[],
        ));
        let left: Vec<String> = kinds(&unresolved_by_launch(&found, true, &lent))
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert!(left.iter().all(|k| k == "missing_addon"), "{left:?}");
        assert!(!left.is_empty());
        assert_eq!(unresolved_by_launch(&found, false, &lent).len(), found.len());
        let source = Mismatch {
            kind: MismatchKind::SourceDiffers {
                id: "t.a".into(),
                name: "t.a name".into(),
                version: Version::parse("1.0.0").unwrap(),
                local: Source::Built("1".repeat(64)),
                host: Source::Built("2".repeat(64)),
                registry: None,
            },
            fixable: false,
            differences: vec!["its database operations differ".into()],
        };
        assert!(unresolved_by_launch(std::slice::from_ref(&source), true, &lent).is_empty());
        assert_eq!(
            unresolved_by_launch(&[source], true, &Default::default()).len(),
            1,
            "without a lent option the difference is real"
        );
    }

    #[test]
    fn db_editor_addons_print_the_operations_their_edits_build() {
        let package = |name: &str, tool: &str, edit: Value| {
            let s = store(name);
            let mut m = data("dbedit.grid");
            m["origin"] = json!({ "tool": "db-editor", "tool_version": tool });
            let marker = json!({
                "format": "swofty-db-package", "format_version": 1, "game_build": "25251167", "edits": [edit],
            })
            .to_string();
            let readme = format!("made with {tool}");
            put(
                &s,
                m,
                &[
                    ("dbedit.json", &marker),
                    ("database.json", tool),
                    ("README.txt", &readme),
                ],
            );
            let f = fingerprint(&s, &profile(&["dbedit.grid"]), &game()).unwrap();
            std::fs::remove_dir_all(&s.root).ok();
            f
        };
        let edit = |value: &str, stock: &str, path: &str| json!({ "key": "0x0000000000000001", "type": 0, "value": value, "stock": stock, "path": path });
        let older = package(
            "dbedit-older",
            "1.5.10",
            edit("33", "1", "entrants.drivers.driver_1.car_number"),
        );
        let newer = package("dbedit-newer", "1.5.12", edit("33", "2", "Car number"));
        assert_eq!(older.addons[0].id, "dbedit.grid");
        assert!(matches!(older.addons[0].source, Source::Edits(_)));
        assert_eq!(older.digest, newer.digest);
        assert!(compare(&older, &newer, &[]).is_empty());
        let other = package("dbedit-other", "1.5.12", edit("34", "1", "Car number"));
        assert_eq!(
            kinds(&compare(&older, &other, &[])),
            [("source_differs".into(), false)]
        );

        let copied = "entrants.drivers.driver[driver=x].car_number";
        let key = format!("0x{:016x}", swofty_formats::hash::fnv1a64(copied));
        let in_row = json!({ "key": key, "type": 0, "value": "33", "stock": null, "path": copied });
        let mut loose = in_row.clone();
        loose["path"] = Value::Null;
        let in_row = package("dbedit-in-row", "1.5.12", in_row);
        let loose = package("dbedit-loose", "1.5.12", loose);
        assert_ne!(in_row.digest, loose.digest);
    }

    #[test]
    fn driver_manager_inputs_ignore_id_counters_and_empty_files() {
        use crate::dbedit::driver_manager::ID as DM;
        let store = store("settled");
        let hook = json!({
            "id": DM, "kind": ["database"],
            "launcher": { "program": { "linux": "bin/hook", "windows": "bin/hook.exe" }, "build": true },
        });
        put(&store, hook, &[]);
        let mut p = profile(&[DM]);
        let inputs = |p: &Profile| {
            fingerprint(&store, p, &game()).unwrap().addons[1]
                .inputs
                .clone()
                .unwrap()
        };
        let none = inputs(&p);
        let data_dir = store.addon_data_dir(DM);
        std::fs::create_dir_all(&data_dir).unwrap();
        let roster = |next: u32, drivers: Value| {
            json!({ "schema": 1, "next_driver": next, "next_preset": next + 158, "drivers": drivers })
                .to_string()
        };
        std::fs::write(data_dir.join("drivers.json"), roster(209, json!([]))).unwrap();
        std::fs::write(data_dir.join("stock.json"), r#"{"schema":1,"drivers":{}}"#).unwrap();
        p.set_state(DM, json!({ "mods": [] })).unwrap();
        assert_eq!(inputs(&p), none);

        std::fs::write(
            data_dir.join("drivers.json"),
            roster(210, json!([{ "stat_id": 209 }])),
        )
        .unwrap();
        let one = inputs(&p);
        assert_ne!(one, none);
        std::fs::write(
            data_dir.join("drivers.json"),
            roster(300, json!([{ "stat_id": 209 }])),
        )
        .unwrap();
        assert_eq!(inputs(&p), one);
        std::fs::write(
            data_dir.join("stock.json"),
            r#"{"schema":1,"drivers":{"max":{"head":"h"}}}"#,
        )
        .unwrap();
        assert_ne!(inputs(&p), one);
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn other_build_hooks_keep_false_zero_empty_text_and_counters() {
        let store = store("generic-hook");
        let hook = json!({
            "id": "t.hook", "kind": ["database"],
            "launcher": { "program": { "linux": "bin/hook", "windows": "bin/hook.exe" }, "build": true },
        });
        put(&store, hook, &[]);
        let p = profile(&["t.hook"]);
        let data_dir = store.addon_data_dir("t.hook");
        std::fs::create_dir_all(&data_dir).unwrap();
        let inputs = || {
            fingerprint(&store, &p, &game()).unwrap().addons[1]
                .inputs
                .clone()
                .unwrap()
        };
        let none = inputs();
        std::fs::write(
            data_dir.join("items.json"),
            r#"{"schema":1,"items":[],"groups":{"a":[]}}"#,
        )
        .unwrap();
        assert_eq!(inputs(), none);
        for text in [
            r#"{"enabled":false}"#,
            r#"{"count":0}"#,
            r#"{"name":""}"#,
            r#"{"next_id":4}"#,
            r#"{"value":null}"#,
        ] {
            std::fs::write(data_dir.join("items.json"), text).unwrap();
            assert_ne!(inputs(), none, "{text}");
        }
        std::fs::write(data_dir.join("items.json"), r#"{"next_id":4}"#).unwrap();
        let four = inputs();
        std::fs::write(data_dir.join("items.json"), r#"{"next_id":5}"#).unwrap();
        assert_ne!(inputs(), four);
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn equal_digests_and_an_empty_comparison_always_agree() {
        type Edit = Box<dyn Fn(&mut Fingerprint)>;
        let core = |sha: &'static str| -> Edit {
            Box::new(move |c| {
                c.addons
                    .push(print(CORE_ID, "0.2.13", Source::Builtin(sha.repeat(64)), None))
            })
        };
        let edits: Vec<Edit> = vec![
            Box::new(|c| c.addons[0].name = "Renamed".into()),
            Box::new(|c| c.addons[0].registry = None),
            Box::new(|c| c.launcher = "0.0.1".into()),
            Box::new(|c| c.game_build.build_id = format!("exe-{}", "a".repeat(12))),
            Box::new(|c| c.game_build.exe_sha256 = "A".repeat(64)),
            Box::new(|c| c.game_build.exe_sha256 = "b".repeat(64)),
            Box::new(|c| c.dlc = Some(true)),
            Box::new(|c| c.dlc = Some(false)),
            Box::new(|c| c.addons[0].version = Version::new(1, 0, 1)),
            Box::new(|c| c.addons[1].source = content("d")),
            Box::new(|c| c.addons[1].source = Source::Edits("b".repeat(64))),
            Box::new(|c| c.addons[1].inputs = Some("d".repeat(64))),
            Box::new(|c| {
                c.addons[0].order = Some(1);
                c.addons[1].order = Some(0);
            }),
            Box::new(|c| {
                c.addons[0].order = Some(5);
                c.addons[1].order = Some(9);
            }),
            Box::new(|c| c.addons[2].order = Some(2)),
            Box::new(|c| c.addons.retain(|a| a.id != "t.b")),
            Box::new(|c| c.addons.push(print("t.extra", "1.0.0", content("e"), None))),
            Box::new(|c| {
                c.options.get_mut("t.a").unwrap().insert("laps".into(), json!(6));
            }),
            Box::new(|c| {
                c.options
                    .get_mut("t.a")
                    .unwrap()
                    .insert("extra".into(), json!(true));
            }),
            Box::new(|c| {
                c.options.remove("t.a");
            }),
            core("1"),
            core("2"),
            Box::new(move |c| {
                core("1")(c);
                c.launcher = "1.5.13".into();
            }),
        ];
        let mut fixtures = vec![host()];
        for edit in &edits {
            let mut f = host();
            edit(&mut f);
            f.digest = f.canonical_digest();
            fixtures.push(f);
        }
        for a in &fixtures {
            for b in &fixtures {
                let found = compare(a, b, &[]);
                assert_eq!(a.digest == b.digest, found.is_empty(), "{a:?}\n{b:?}\n{found:?}");
            }
        }
    }
}
