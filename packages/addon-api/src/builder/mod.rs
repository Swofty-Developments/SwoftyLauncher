//! Writes a data addon folder: a manifest, its operation files and the
//! source files they name. Tools that turn mods or edits into addons use it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use semver::{Version, VersionReq};

use crate::asset::{AssetOp, LangOp, Source, TrackOp};
use crate::manifest::{AddonManifest, Category, FileEntry, MANIFEST_FILE, Origin, relative_inside, valid_id};
use crate::ops::{DbOp, DbOps, ErpOp};
use crate::{Error, Result};

pub const DATABASE_FILE: &str = "database.json";
pub const ERP_FILE: &str = "erp.json";
pub const TEXT_FILE: &str = "text.json";
pub const TRACKS_FILE: &str = "tracks.json";
const CODE: [&str; 6] = ["dll", "exe", "asi", "so", "dylib", "bat"];
/// `swofty.` for the launcher's own addons, `dbedit.` for the DB Editor's.
const RESERVED: [&str; 2] = ["swofty.", "dbedit."];
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone)]
pub struct AddonBuilder {
    pub id: String,
    pub name: String,
    pub version: Version,
    pub author: String,
    pub description: Option<String>,
    pub category: Option<Category>,
    pub origin: Option<Origin>,
    pub game_builds: Vec<VersionReq>,
    database: DbOps,
    erp: Vec<ErpOp>,
    text: Vec<LangOp>,
    tracks: Vec<String>,
    files: Vec<FileEntry>,
    sources: BTreeMap<String, PathBuf>,
}

/// The oldest launcher a generated addon accepts: 1.3.0, the first that reads
/// `category` and `origin`, or this one when it is older.
pub fn launcher_floor() -> Version {
    let v = Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver");
    let v = Version {
        pre: v.pre,
        ..Version::new(v.major, v.minor, v.patch)
    };
    v.min(Version::new(1, 3, 0))
}

impl AddonBuilder {
    pub fn new(id: impl Into<String>, name: impl Into<String>, author: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: Version::new(1, 0, 0),
            author: author.into(),
            description: None,
            category: None,
            origin: None,
            game_builds: Vec::new(),
            database: DbOps {
                schema_version: 1,
                fields: BTreeMap::new(),
                strict: false,
                side_data: Default::default(),
                ops: Vec::new(),
            },
            erp: Vec::new(),
            text: Vec::new(),
            tracks: Vec::new(),
            files: Vec::new(),
            sources: BTreeMap::new(),
        }
    }

    pub fn db(&mut self, op: DbOp) -> &mut Self {
        self.database.ops.push(op);
        self
    }

    /// Adds a database op file's ops and field names.
    pub fn db_ops(&mut self, ops: DbOps) -> &mut Self {
        self.database.fields.extend(ops.fields);
        self.database.strict |= ops.strict;
        self.database.ops.extend(ops.ops);
        self
    }

    /// An ERP op; name its source files with [`source`](Self::source).
    pub fn erp(&mut self, op: ErpOp) -> &mut Self {
        self.erp.push(op);
        self
    }

    /// Sets a language string; `language` is a code such as `eng`, or `*` for all.
    pub fn text(&mut self, language: &str, key: &str, value: &str) -> &mut Self {
        self.lang(LangOp::Set {
            language: language.into(),
            key: key.into(),
            value: value.into(),
        })
    }

    pub fn lang(&mut self, op: LangOp) -> &mut Self {
        self.text.push(op);
        self
    }

    /// Copies `from` into the addon and puts it at `target` in the game folder.
    pub fn file(&mut self, from: impl Into<PathBuf>, target: &str) -> &mut Self {
        let rel = format!("files/{target}");
        self.files.push(FileEntry {
            source: rel.clone(),
            target: target.into(),
        });
        self.source(&rel, from)
    }

    /// Copies the track package folder `from` into `tracks/<name>` and adds it.
    pub fn track(&mut self, from: impl Into<PathBuf>, name: &str) -> &mut Self {
        let rel = format!("tracks/{name}");
        self.tracks.push(rel.clone());
        self.source(&rel, from)
    }

    /// Copies the file or folder `from` to `rel` inside the addon, for an op that names it.
    pub fn source(&mut self, rel: &str, from: impl Into<PathBuf>) -> &mut Self {
        self.sources.insert(rel.into(), from.into());
        self
    }

    pub fn manifest(&self) -> Result<AddonManifest> {
        let mut ops = serde_json::Map::new();
        let mut kind = Vec::new();
        if !self.database.ops.is_empty() {
            ops.insert("database".into(), serde_json::json!([DATABASE_FILE]));
            kind.push("database");
        }
        if !self.erp.is_empty() {
            ops.insert("erp".into(), serde_json::json!([ERP_FILE]));
            kind.push("erp");
        }
        if !self.text.is_empty() {
            kind.push("text");
        }
        let assets: Vec<&str> = [
            (TEXT_FILE, self.text.is_empty()),
            (TRACKS_FILE, self.tracks.is_empty()),
        ]
        .into_iter()
        .filter(|(_, empty)| !empty)
        .map(|(f, _)| f)
        .collect();
        if !assets.is_empty() {
            ops.insert("assets".into(), serde_json::json!(assets));
        }
        if !self.files.is_empty() {
            ops.insert(
                "files".into(),
                serde_json::to_value(&self.files).expect("serialisable"),
            );
            kind.push("files");
        }
        let mut doc = serde_json::json!({
            "schema_version": crate::manifest::SCHEMA_VERSION,
            "id": self.id,
            "name": self.name,
            "version": self.version.to_string(),
            "author": self.author,
            "kind": kind,
            "compat": { "game": crate::GAME_ID, "launcher": format!(">={}", launcher_floor()) },
            "operations": ops,
        });
        if !self.game_builds.is_empty() {
            doc["compat"]["game_builds"] = serde_json::to_value(&self.game_builds).expect("serialisable");
        }
        if let Some(d) = self
            .description
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            doc["description"] = d.into();
        }
        if let Some(c) = self.category {
            doc["category"] = serde_json::to_value(c).expect("serialisable");
        }
        if let Some(o) = &self.origin {
            doc["origin"] = serde_json::to_value(o).expect("serialisable");
        }
        let m: AddonManifest = serde_json::from_value(doc).map_err(|e| Error::Manifest(e.to_string()))?;
        m.validate()?;
        Ok(m)
    }

    fn check(&self) -> Result<()> {
        let fail = |m: String| Err(Error::Manifest(format!("{}: {m}", self.id)));
        if !valid_id(&self.id) {
            return fail("id must be lowercase dotted [a-z0-9._-] with at least two parts".into());
        }
        if let Some(prefix) = RESERVED.iter().find(|p| self.id.starts_with(*p)) {
            return fail(format!("the {prefix} prefix is reserved"));
        }
        if self.name.trim().is_empty() {
            return fail("the addon needs a name".into());
        }
        if let Some(rel) = self.sources.keys().find(|r| !relative_inside(r)) {
            return fail(format!("{rel} must be a relative path inside the addon"));
        }
        if let Some(f) = self.files.iter().find(|f| !relative_inside(&f.target)) {
            return fail(format!(
                "{} must be a relative path inside the game folder",
                f.target
            ));
        }
        let named = self.sources.keys().chain(self.files.iter().map(|f| &f.target));
        if let Some(rel) = named.into_iter().find(|r| is_code(Path::new(r))) {
            return fail(format!("{rel} is a program; a generated addon carries data only"));
        }
        let mut targets = std::collections::BTreeSet::new();
        if let Some(f) = self.files.iter().find(|f| !targets.insert(f.target.as_str())) {
            return fail(format!("{} is put twice", f.target));
        }
        let ops = self.erp.iter().cloned().map(AssetOp::Erp).chain(self.assets());
        for op in ops.collect::<Vec<_>>() {
            for src in op.sources() {
                self.carried(src).or_else(fail)?;
            }
        }
        Ok(())
    }

    fn carried(&self, src: &Source) -> std::result::Result<(), String> {
        if src.root != crate::asset::Root::Addon {
            return Err(format!(
                "{}: a data addon carries its sources in its own folder",
                src.path
            ));
        }
        let path = src.path.trim_end_matches('/');
        let found = self.sources.keys().any(|rel| {
            path == rel
                || path
                    .strip_prefix(rel.as_str())
                    .is_some_and(|r| r.starts_with('/'))
        });
        match found {
            true => Ok(()),
            false => Err(format!("{path} is named by an op but not added with source")),
        }
    }

    fn assets(&self) -> impl Iterator<Item = AssetOp> + '_ {
        let text = self.text.iter().cloned().map(AssetOp::Lang);
        let tracks = self.tracks.iter().map(|rel| {
            AssetOp::Track(TrackOp::Add {
                source: Source::addon(rel),
            })
        });
        text.chain(tracks)
    }

    /// Writes the addon into `dir`, which must be missing or empty, and returns its manifest.
    pub fn write(&self, dir: &Path) -> Result<AddonManifest> {
        self.check()?;
        let m = self.manifest()?;
        for from in self.sources.values() {
            refuse_code(from, 0)?;
        }
        if std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some()) {
            return Err(Error::Other(format!("{} is not empty", dir.display())));
        }
        mkdir(dir)?;
        for (rel, from) in &self.sources {
            copy(from, &dir.join(rel), 0)?;
        }
        let mut out = vec![(MANIFEST_FILE, serde_json::to_value(&m).expect("serialisable"))];
        if !self.database.ops.is_empty() {
            out.push((
                DATABASE_FILE,
                serde_json::to_value(&self.database).expect("serialisable"),
            ));
        }
        if !self.erp.is_empty() {
            out.push((
                ERP_FILE,
                serde_json::json!({ "schema_version": 1, "ops": self.erp }),
            ));
        }
        for (name, ops) in [
            (
                TEXT_FILE,
                self.text.iter().cloned().map(AssetOp::Lang).collect::<Vec<_>>(),
            ),
            (
                TRACKS_FILE,
                self.assets().filter(|o| matches!(o, AssetOp::Track(_))).collect(),
            ),
        ] {
            if !ops.is_empty() {
                out.push((name, serde_json::json!({ "schema_version": 1, "ops": ops })));
            }
        }
        for (name, value) in out {
            let path = dir.join(name);
            let mut bytes = serde_json::to_vec_pretty(&value).expect("serialisable");
            bytes.push(b'\n');
            std::fs::write(&path, bytes).map_err(|source| Error::Io { path, source })?;
        }
        AddonManifest::load(dir)
    }
}

fn mkdir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|source| Error::Io {
        path: dir.into(),
        source,
    })
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.to_path_buf();
    move |source| Error::Io { path, source }
}

fn is_code(path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
    CODE.iter().any(|c| ext.eq_ignore_ascii_case(c))
}

/// Whether `path` is a folder, refusing links, which could leave the source or loop.
fn is_dir(path: &Path, depth: usize) -> Result<bool> {
    let meta = std::fs::symlink_metadata(path).map_err(io(path))?;
    if meta.file_type().is_symlink() {
        return Err(Error::Other(format!(
            "{} is a link; a generated addon copies plain files and folders",
            path.display()
        )));
    }
    if depth > MAX_DEPTH {
        return Err(Error::Other(format!("{} is nested too deep", path.display())));
    }
    Ok(meta.is_dir())
}

fn refuse_code(path: &Path, depth: usize) -> Result<()> {
    if is_dir(path, depth)? {
        for e in std::fs::read_dir(path).map_err(io(path))? {
            refuse_code(&e.map_err(io(path))?.path(), depth + 1)?;
        }
        return Ok(());
    }
    match is_code(path) {
        true => Err(Error::Other(format!(
            "{} is a program; a generated addon carries data only",
            path.display()
        ))),
        false => Ok(()),
    }
}

fn copy(from: &Path, to: &Path, depth: usize) -> Result<()> {
    if is_dir(from, depth)? {
        mkdir(to)?;
        for e in std::fs::read_dir(from).map_err(io(from))? {
            let e = e.map_err(io(from))?;
            copy(&e.path(), &to.join(e.file_name()), depth + 1)?;
        }
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        mkdir(parent)?;
    }
    std::fs::copy(from, to).map_err(io(from))?;
    Ok(())
}

#[cfg(test)]
mod tests;
