//! Fixtures (docs/std/testing.md "Fixtures"): a saved game in a known state.
//!
//! ```text
//! <fixture>/
//!   fixture.json   the exe build, the addons and versions it needs, what it holds, each file's sha256
//!   save/          the game's save files, as Steam remote storage names them
//!   storage/<id>/  addon storage that lives outside the save
//! ```
//!
//! A test names a fixture by a path relative to its test file
//! (`fixtures/my-team-r3`) or by name in the fixture store
//! ([`STORE_ENV`], else `<data>/test-fixtures`). A run copies the fixture
//! into a fresh sandbox and never edits it in place.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::saves::vault::Layout;

pub const FILE: &str = "fixture.json";
pub const SAVE_DIR: &str = "save";
pub const STORAGE_DIR: &str = "storage";
pub const STORE_ENV: &str = "SWOFTY_TEST_FIXTURES";
pub const STORE_DIR: &str = "test-fixtures";
/// The profile save: every career, Grand Prix and setting. Both files go
/// together or the game reports a corrupted save.
pub const PROFILE_FILES: [&str; 2] = ["savegame@profile#NXDSMWW.BWW", "savegame@profile#QKRHMYXE"];
/// Highlight replays (13 to 15 MB each) and the game's corrupt-save bundles
/// are never part of a fixture.
const SKIPPED: [&str; 2] = ["@highlights", "corrupt_saves"];
const MAX_BYTES: u64 = 64 << 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Exe {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSum {
    pub size: u64,
    pub sha256: String,
}

/// A career the fixture holds, as its resume card shows it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Career {
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<u64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub team: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub driver: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Holds {
    #[serde(default)]
    pub careers: Vec<Career>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub schema: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub created: String,
    pub exe: Exe,
    #[serde(default)]
    pub addons: BTreeMap<String, String>,
    #[serde(default)]
    pub holds: Holds,
    pub files: BTreeMap<String, FileSum>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// The save layout of the sandbox it was captured from. The files always
    /// carry Steam's names; an EA fixture installs only on the EA copy, as
    /// the vault never exchanges the two (`Layout::exchanges_with`).
    #[serde(default = "steam_layout", skip_serializing_if = "is_steam_layout")]
    pub layout: Layout,
}

fn steam_layout() -> Layout {
    Layout::Steam
}

fn is_steam_layout(l: &Layout) -> bool {
    *l == Layout::Steam
}

fn is_ea(l: Layout) -> bool {
    matches!(l, Layout::Ea | Layout::LauncherEa)
}

fn saves_of(l: Layout) -> &'static str {
    match is_ea(l) {
        true => "EA app",
        false => "Steam",
    }
}

#[derive(Debug, Clone)]
pub struct Fixture {
    pub dir: PathBuf,
    pub manifest: Manifest,
}

pub fn store(data: &Path) -> PathBuf {
    std::env::var_os(STORE_ENV)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data.join(STORE_DIR))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// Where a test's `fixture` lives: next to its test file, else in the store
/// under the same path or its last part.
pub fn resolve(spec: &str, base: Option<&Path>, store: &Path) -> Result<PathBuf, String> {
    let rel = Path::new(spec);
    if spec.is_empty()
        || rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "fixture {spec:?}: name a folder next to the test file or a fixture in the store"
        ));
    }
    let mut tried = Vec::new();
    let candidates = base
        .map(|b| b.join(rel))
        .into_iter()
        .chain([store.join(rel)])
        .chain(rel.file_name().map(|n| store.join(n)));
    for c in candidates {
        if c.join(FILE).is_file() {
            return Ok(c);
        }
        tried.push(c.display().to_string());
    }
    Err(format!(
        "fixture {spec:?} not found (looked in {}); capture one with swofty test fixture capture",
        tried.join(", ")
    ))
}

fn files_under(root: &Path, rel: &str, out: &mut Vec<String>) -> Result<(), String> {
    let dir = root.join(rel);
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let child = format!("{rel}/{name}");
        let t = e
            .file_type()
            .map_err(|err| format!("{}: {err}", e.path().display()))?;
        if t.is_dir() {
            files_under(root, &child, out)?;
        } else if t.is_file() {
            out.push(child);
        }
    }
    Ok(())
}

fn sum(path: &Path) -> Result<FileSum, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    let sha256 = crate::game_folder::baseline::sha256_file(path).map_err(|e| e.to_string())?;
    Ok(FileSum { size, sha256 })
}

/// Read a fixture and check every file against `fixture.json`.
pub fn load(dir: &Path) -> Result<Fixture, String> {
    let path = dir.join(FILE);
    let text = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let manifest: Manifest = serde_json::from_slice(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if manifest.schema != 1 {
        return Err(format!(
            "{}: schema {} (this launcher reads 1)",
            path.display(),
            manifest.schema
        ));
    }
    for f in PROFILE_FILES {
        if !manifest.files.contains_key(&format!("{SAVE_DIR}/{f}")) {
            return Err(format!("{}: the fixture has no {SAVE_DIR}/{f}", path.display()));
        }
    }
    let mut present = Vec::new();
    for d in [SAVE_DIR, STORAGE_DIR] {
        if dir.join(d).is_dir() {
            files_under(dir, d, &mut present)?;
        }
    }
    for rel in &present {
        if !manifest.files.contains_key(rel) {
            return Err(format!("{}: {rel} is not listed in {FILE}", dir.display()));
        }
    }
    for (rel, want) in &manifest.files {
        if rel.split('/').any(|p| p.is_empty() || p == "..")
            || !(rel.starts_with(&format!("{SAVE_DIR}/")) || rel.starts_with(&format!("{STORAGE_DIR}/")))
        {
            return Err(format!(
                "{}: {rel} is not a file under {SAVE_DIR}/ or {STORAGE_DIR}/",
                path.display()
            ));
        }
        let got = sum(&dir.join(rel)).map_err(|e| format!("fixture {}: {e}", manifest.name))?;
        if got != *want {
            return Err(format!(
                "fixture {}: {rel} does not match {FILE} (size {} sha256 {}, expected size {} sha256 {}); a fixture is \
                 never edited in place",
                manifest.name,
                got.size,
                &got.sha256[..12],
                want.size,
                &want.sha256[..want.sha256.len().min(12)]
            ));
        }
    }
    Ok(Fixture {
        dir: dir.to_path_buf(),
        manifest,
    })
}

/// The EA layout's name for a save file of the Steam layout:
/// `savegame@<save>#<file>` is `<save>/<file>` in the EA title folder.
pub fn ea_name(steam: &str) -> Option<String> {
    let (save, file) = steam.strip_prefix("savegame@")?.split_once('#')?;
    let plain = |s: &str| !s.is_empty() && !s.starts_with('.') && !s.contains(['/', '\\', '#', ':']);
    (plain(save) && plain(file)).then(|| format!("{save}/{file}"))
}

/// The Steam layout's name for a save file of the EA title folder: `<save>/<file>` is
/// `savegame@<save>#<file>`, the inverse of [`ea_name`].
pub fn steam_name(ea: &str) -> Option<String> {
    let (save, file) = ea.split_once('/')?;
    let name = format!("savegame@{save}#{file}");
    (ea_name(&name).as_deref() == Some(ea)).then_some(name)
}

fn copy(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    std::fs::copy(from, to).map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))?;
    Ok(())
}

impl Fixture {
    /// The addon ids whose storage the fixture carries.
    pub fn storage_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .manifest
            .files
            .keys()
            .filter_map(|rel| {
                rel.strip_prefix(&format!("{STORAGE_DIR}/"))?
                    .split('/')
                    .next()
                    .map(str::to_owned)
            })
            .collect();
        ids.dedup();
        ids
    }

    /// Copy the save files into the sandbox's empty `remote` folder and each
    /// addon's storage into the folder `storage(id)` names. Returns what was
    /// copied, for the report.
    pub fn install(&self, remote: &Path, storage: &dyn Fn(&str) -> PathBuf) -> Result<Vec<String>, String> {
        self.install_into(remote, Layout::Steam, &|name| Ok(name.to_owned()), storage)
    }

    /// [`Self::install`] for the EA copy: the save files go into the run's
    /// empty EA title folder by their EA names ([`ea_name`]).
    pub fn install_ea(&self, title: &Path, storage: &dyn Fn(&str) -> PathBuf) -> Result<Vec<String>, String> {
        self.install_into(
            title,
            Layout::Ea,
            &|name| ea_name(name).ok_or_else(|| format!("{name} has no name in the EA save layout")),
            storage,
        )
    }

    fn install_into(
        &self,
        saves: &Path,
        layout: Layout,
        name_of: &dyn Fn(&str) -> Result<String, String>,
        storage: &dyn Fn(&str) -> PathBuf,
    ) -> Result<Vec<String>, String> {
        if !self.manifest.layout.exchanges_with(layout) {
            return Err(format!(
                "fixture {} holds {} saves, and this run's game copy takes {} saves: capture a fixture from a test \
                 run on this copy",
                self.manifest.name,
                saves_of(self.manifest.layout),
                saves_of(layout)
            ));
        }
        let mut stray = Vec::new();
        let name = saves.file_name().unwrap_or_default().to_string_lossy();
        files_under(saves.parent().unwrap_or(saves), &name, &mut stray)?;
        if !stray.is_empty() {
            return Err(format!(
                "the sandbox {} is not empty: {}",
                saves.display(),
                stray.join(", ")
            ));
        }
        let mut out = Vec::new();
        for rel in self.manifest.files.keys() {
            let from = self.dir.join(rel);
            if let Some(name) = rel.strip_prefix(&format!("{SAVE_DIR}/")) {
                copy(&from, &saves.join(name_of(name)?))?;
            } else if let Some(rest) = rel.strip_prefix(&format!("{STORAGE_DIR}/")) {
                let (id, file) = rest
                    .split_once('/')
                    .ok_or_else(|| format!("{rel}: no addon id"))?;
                copy(&from, &storage(id).join(file))?;
            }
            out.push(rel.clone());
        }
        Ok(out)
    }
}

/// What a capture records besides the files.
#[derive(Debug, Clone, Default)]
pub struct Meta {
    pub description: String,
    pub created: String,
    pub exe: Exe,
    pub addons: BTreeMap<String, String>,
    pub holds: Holds,
    pub source: String,
}

/// The sandbox files a capture keeps: every save but highlights and
/// corrupt-save bundles.
pub fn kept(name: &str) -> bool {
    !SKIPPED.iter().any(|s| name.contains(s)) && !name.ends_with(".swofty-tmp")
}

/// Copy a sandbox's saves (`remote`) and addon storage (`storage/<id>/…`) to
/// `<store>/<name>/` with its `fixture.json`. An existing fixture is never
/// overwritten.
pub fn capture(
    name: &str,
    store: &Path,
    remote: &Path,
    storage: &[(String, PathBuf)],
    meta: Meta,
) -> Result<Fixture, String> {
    capture_from(name, store, Layout::Steam, remote, storage, meta)
}

/// [`capture`] from an EA run's title folder (`saves/ea`): each
/// `<save>/<file>` is kept under its Steam name ([`steam_name`]) and the
/// fixture is marked as EA saves.
pub fn capture_ea(
    name: &str,
    store: &Path,
    title: &Path,
    storage: &[(String, PathBuf)],
    meta: Meta,
) -> Result<Fixture, String> {
    capture_from(name, store, Layout::Ea, title, storage, meta)
}

fn sorted_entries(dir: &Path) -> Result<Vec<std::fs::DirEntry>, String> {
    let mut v: Vec<std::fs::DirEntry> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .collect();
    v.sort_by_key(|e| e.file_name());
    Ok(v)
}

/// The save files of a sandbox folder a capture keeps, as (path, Steam name):
/// the files of a Steam `remote` folder, or each `<save>/<file>` of an EA
/// title folder.
fn saves_in(dir: &Path, layout: Layout) -> Result<Vec<(PathBuf, String)>, String> {
    let file = |e: &std::fs::DirEntry| e.file_type().is_ok_and(|t| t.is_file());
    let mut out = Vec::new();
    for e in sorted_entries(dir)? {
        let save = e.file_name().to_string_lossy().into_owned();
        if !is_ea(layout) {
            if file(&e) {
                out.push((e.path(), save));
            }
            continue;
        }
        if !e.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        for f in sorted_entries(&e.path())?.into_iter().filter(file) {
            if let Some(n) = steam_name(&format!("{save}/{}", f.file_name().to_string_lossy())) {
                out.push((f.path(), n));
            }
        }
    }
    out.retain(|(_, n)| kept(n));
    Ok(out)
}

fn capture_from(
    name: &str,
    store: &Path,
    layout: Layout,
    remote: &Path,
    storage: &[(String, PathBuf)],
    meta: Meta,
) -> Result<Fixture, String> {
    if !valid_name(name) {
        return Err(format!("{name:?} is not a fixture name (letters, digits, - _ .)"));
    }
    let dest = store.join(name);
    if dest.exists() {
        return Err(format!(
            "{} exists; fixtures are never overwritten: pick another name or remove it",
            dest.display()
        ));
    }
    for f in PROFILE_FILES {
        let shown = match is_ea(layout) {
            true => ea_name(f).expect("a profile file has an EA name"),
            false => f.to_owned(),
        };
        if !remote.join(&shown).is_file() {
            return Err(format!(
                "{} has no {shown}: the game has not saved its profile in this sandbox (return to the main menu or \
                 reach Team HQ, which save it)",
                remote.display()
            ));
        }
    }
    let tmp = store.join(format!(".{name}.capturing"));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?;
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    let mut copies: Vec<(PathBuf, String)> = saves_in(remote, layout)?
        .into_iter()
        .map(|(path, n)| (path, format!("{SAVE_DIR}/{n}")))
        .collect();
    for (id, dir) in storage {
        if !valid_name(id) || !dir.is_dir() {
            continue;
        }
        let mut rels = Vec::new();
        files_under(dir, ".", &mut rels)?;
        for r in rels {
            let r = r.trim_start_matches("./").to_owned();
            if r.starts_with('.') || r.contains("/.") {
                continue;
            }
            copies.push((dir.join(&r), format!("{STORAGE_DIR}/{id}/{r}")));
        }
    }
    for (from, rel) in &copies {
        let s = sum(from)?;
        total += s.size;
        if total > MAX_BYTES {
            std::fs::remove_dir_all(&tmp).ok();
            return Err(format!(
                "the capture is over {} MB; a fixture is about 1 MB",
                MAX_BYTES >> 20
            ));
        }
        copy(from, &tmp.join(rel))?;
        let again = sum(&tmp.join(rel))?;
        if again != s {
            std::fs::remove_dir_all(&tmp).ok();
            return Err(format!(
                "{rel} changed while it was copied; capture again once the game is idle"
            ));
        }
        files.insert(rel.clone(), s);
    }
    let manifest = Manifest {
        schema: 1,
        name: name.to_owned(),
        description: meta.description,
        created: meta.created,
        exe: meta.exe,
        addons: meta.addons,
        holds: meta.holds,
        files,
        source: meta.source,
        layout,
    };
    let text = serde_json::to_string_pretty(&manifest).expect("serialisable") + "\n";
    std::fs::write(tmp.join(FILE), text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    load(&dest)
}

/// The fixtures in the store, by name.
pub fn list(store: &Path) -> Vec<Result<Fixture, String>> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(store)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(FILE).is_file())
        .collect();
    dirs.sort();
    dirs.iter().map(|d| load(d)).collect()
}
