//! The list of files an install writes into an addon's folder, with each file's size, CRC-32,
//! first bytes and write time, and the checks that compare the folder with it. Play stats the
//! files and reads the first bytes of the ones the build and the game load; Verify reads every
//! file.
//!
//! A file that differs from the list and was not written since stops Play when the build or
//! the game loads it. An addon installed before installs wrote lists, and a listed file written
//! since its install, stop Play only for a file its manifest names that is missing, or one the
//! build reads that is empty or holds only zeros; anything else they show is a log line.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata, OpenOptions};
use std::io::Read;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::asset::FileKind;
use crate::manifest::AddonManifest;
use crate::store::Store;
use crate::{Error, Result, io_err};

/// The file list, in the addon's folder. Only installs write it.
pub const INDEX_FILE: &str = ".swofty-files.json";
const SCHEMA: u32 = 1;
/// How much of a file the checks read to tell content from zeros or blanks.
const PEEK: usize = 4096;
/// How many first bytes the list keeps of each file.
const HEAD: usize = 8;
const ERP_MAGIC: &[u8; 4] = b"ERPK";
const DLL_MAGIC: &[u8; 2] = b"MZ";

/// A file's size and CRC-32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub size: u64,
    pub crc32: u32,
}

/// A file as its install wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listed {
    pub size: u64,
    pub crc32: u32,
    /// The first bytes, in hex.
    pub head: String,
    /// The last write, in nanoseconds since the Unix epoch; 0 when the file system has none.
    pub modified: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub schema: u32,
    pub files: BTreeMap<String, Listed>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Missing,
    Empty,
    Size { listed: u64, found: u64 },
    Zeros,
    Blank,
    Head,
    Magic { expected: &'static str },
    Content,
}

/// A file of an installed addon that is damaged or looks it. `stops` marks damage the build or
/// the game fails on; the rest is logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Damage {
    pub path: String,
    pub problem: Problem,
    pub stops: bool,
}

impl Damage {
    pub fn line(&self) -> String {
        let path = &self.path;
        match self.problem {
            Problem::Missing => format!("{path} is missing"),
            Problem::Empty => format!("{path} is empty"),
            Problem::Size { listed, found } => {
                format!("{path} is {found} bytes; the install wrote {listed}")
            }
            Problem::Zeros => format!("{path} holds only zeros"),
            Problem::Blank => format!("{path} holds only blanks"),
            Problem::Head => format!("{path} does not start with the bytes the install wrote"),
            Problem::Magic { expected } => format!("{path} does not start with {expected}"),
            Problem::Content => format!("{path} differs from the file the install wrote"),
        }
    }
}

/// An installed addon with damaged files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Damaged {
    pub id: String,
    pub name: String,
    pub damage: Vec<Damage>,
}

impl Damaged {
    /// True when the build or the game would fail on it.
    pub fn stops_play(&self) -> bool {
        self.damage.iter().any(|d| d.stops)
    }

    /// The first files that stop Play.
    pub fn summary(&self) -> String {
        const SHOWN: usize = 3;
        let stopping: Vec<&Damage> = self.damage.iter().filter(|d| d.stops).collect();
        let mut text = stopping
            .iter()
            .take(SHOWN)
            .map(|d| d.line())
            .collect::<Vec<_>>()
            .join(", ");
        if stopping.len() > SHOWN {
            text.push_str(&format!(" and {} more", stopping.len() - SHOWN));
        }
        text
    }

    /// Every damaged file, the ones that don't stop Play marked as warnings.
    pub fn lines(&self) -> Vec<String> {
        self.damage
            .iter()
            .map(|d| match d.stops {
                true => d.line(),
                false => format!("warning: {}", d.line()),
            })
            .collect()
    }
}

/// Writes the file list of the addon folder `dir`, flushing every file to disk first, so an
/// install that is swapped in has its files on disk.
pub fn seal(dir: &Path) -> Result<Index> {
    let mut index = Index {
        schema: SCHEMA,
        files: BTreeMap::new(),
    };
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for entry in std::fs::read_dir(&d).map_err(io_err(&d))? {
            let entry = entry.map_err(io_err(&d))?;
            let path = entry.path();
            let kind = entry.file_type().map_err(io_err(&path))?;
            if kind.is_dir() {
                dirs.push(path);
                continue;
            }
            let rel = relative(dir, &path);
            if !kind.is_file() || rel == INDEX_FILE {
                continue;
            }
            index.files.insert(rel, settled(&path)?);
            #[cfg(test)]
            tests::after_file();
        }
        crate::sync_dir(Some(&d));
    }
    crate::write_json(&dir.join(INDEX_FILE), &index)?;
    Ok(index)
}

fn relative(dir: &Path, path: &Path) -> String {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Opens `path` to flush it, or only to read it when it is read-only, waiting out a scanner
/// that briefly holds a file just written.
fn open(path: &Path) -> std::io::Result<(File, bool)> {
    const DELAYS_MS: [u64; 5] = [50, 150, 300, 600, 1000];
    let mut attempt = 0;
    loop {
        let opened = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map(|f| (f, true))
            .or_else(|_| File::open(path).map(|f| (f, false)));
        match opened {
            Err(e) if crate::is_locked_file_error(&e) && attempt < DELAYS_MS.len() => {
                std::thread::sleep(std::time::Duration::from_millis(DELAYS_MS[attempt]));
                attempt += 1;
            }
            r => return r,
        }
    }
}

/// Flushes the file at `path` to disk and reads it back: its size and CRC-32.
pub(crate) fn settle(path: &Path) -> Result<Entry> {
    settled(path).map(|l| Entry {
        size: l.size,
        crc32: l.crc32,
    })
}

fn settled(path: &Path) -> Result<Listed> {
    let (mut file, writable) = open(path).map_err(io_err(path))?;
    let listed = file.metadata().map_err(io_err(path))?.len();
    let (entry, head) = digest(&mut file).map_err(io_err(path))?;
    if entry.size != listed {
        return Err(Error::Other(format!(
            "{}: read {} bytes, the file is {listed}",
            path.display(),
            entry.size
        )));
    }
    if writable {
        file.sync_all().ok();
    }
    let meta = file.metadata().map_err(io_err(path))?;
    Ok(Listed {
        size: entry.size,
        crc32: entry.crc32,
        head: crate::game_folder::baseline::hex(&head),
        modified: modified(&meta),
    })
}

fn digest(r: &mut impl Read) -> std::io::Result<(Entry, Vec<u8>)> {
    let mut crc = flate2::Crc::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut head = Vec::with_capacity(HEAD);
    let mut size = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        let want = HEAD.saturating_sub(head.len()).min(n);
        head.extend_from_slice(&buf[..want]);
        crc.update(&buf[..n]);
        size += n as u64;
    }
    Ok((
        Entry {
            size,
            crc32: crc.sum(),
        },
        head,
    ))
}

fn modified(meta: &Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// Up to `len` first bytes of the file at `path`.
fn first_bytes(path: &Path, len: usize) -> Option<Vec<u8>> {
    let mut head = Vec::with_capacity(len);
    File::open(path)
        .ok()?
        .take(len as u64)
        .read_to_end(&mut head)
        .ok()?;
    Some(head)
}

pub fn read_index(dir: &Path) -> Option<Index> {
    crate::read_json::<Index>(&dir.join(INDEX_FILE))
        .ok()
        .filter(|i| i.schema == SCHEMA)
}

fn normal(rel: &str) -> String {
    rel.trim_start_matches("./").replace('\\', "/")
}

/// The first bytes the file of `rel` must start with, for the kinds the build and the game
/// load whole: ERP archives and native plugins.
fn magic(rel: &str) -> Option<(&'static [u8], &'static str)> {
    let ext = rel.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    match ext.as_str() {
        "erp" | "erpk" => Some((ERP_MAGIC, "ERPK")),
        "dll" => Some((DLL_MAGIC, "MZ")),
        _ => None,
    }
}

/// Every file under `dir` but the file list, relative with `/`.
fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut dirs = vec![dir.to_path_buf()];
    while let Some(d) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(t) if t.is_dir() => dirs.push(path),
                Ok(t) if t.is_file() => out.push(relative(dir, &path)),
                _ => {}
            }
        }
    }
    out.retain(|rel| rel != INDEX_FILE);
    out
}

/// The files of an addon its manifest names for the build and the game to read.
struct Reads {
    /// Op files the build parses. A DB Editor addon's database op file is left out: the build
    /// makes its ops from its edits.
    ops: BTreeSet<String>,
    plugins: BTreeSet<String>,
    /// True when the addon has ERP op files, which place its ERP archives.
    erp: bool,
}

impl Reads {
    fn of(dir: &Path, m: &AddonManifest) -> Self {
        let edits = dir.join(crate::dbedit::package::MARKER).is_file();
        let mut erp = false;
        let ops = m
            .operations
            .op_files()
            .filter(|(kind, _)| !(edits && *kind == FileKind::Database))
            .inspect(|(kind, _)| erp |= *kind == FileKind::Erp)
            .map(|(_, rel)| normal(rel))
            .chain(m.operations.hud.iter().map(|rel| normal(rel)))
            .collect();
        let plugins = m.operations.native.iter().map(|n| normal(&n.dll)).collect();
        Self { ops, plugins, erp }
    }

    fn named(&self, rel: &str) -> bool {
        self.ops.contains(rel) || self.plugins.contains(rel)
    }

    fn loads(&self, rel: &str) -> bool {
        self.named(rel) || magic(rel).is_some()
    }
}

/// What a look at the file of `rel` alone finds, with no list to compare it with, and whether
/// that stops Play.
fn sniff(dir: &Path, rel: &str, reads: &Reads) -> Option<(Problem, bool)> {
    let named = reads.named(rel);
    let kind = magic(rel);
    if !named && kind.is_none() {
        return None;
    }
    let path = dir.join(rel);
    let Ok(meta) = std::fs::metadata(&path) else {
        return named.then_some((Problem::Missing, true));
    };
    if meta.len() == 0 {
        return Some((Problem::Empty, named));
    }
    let head = first_bytes(&path, PEEK)?;
    if head.iter().all(|b| *b == 0) {
        let archive = kind.is_some_and(|(_, expected)| expected == "ERPK");
        return Some((Problem::Zeros, named || archive && reads.erp));
    }
    if reads.ops.contains(rel) && meta.len() <= PEEK as u64 && head.iter().all(u8::is_ascii_whitespace) {
        return Some((Problem::Blank, false));
    }
    match kind {
        Some((bytes, expected)) if !head.starts_with(bytes) => Some((Problem::Magic { expected }, false)),
        _ => None,
    }
}

/// The damage a stat of each file finds in the addon folder `dir`, with the first bytes of the
/// files the build and the game load. See the module docs for what stops Play.
pub fn check(dir: &Path, m: &AddonManifest) -> Vec<Damage> {
    let reads = Reads::of(dir, m);
    let mut found: BTreeMap<String, Damage> = BTreeMap::new();
    let mut note = |rel: &str, problem: Problem, stops: bool| {
        found.entry(rel.to_owned()).or_insert(Damage {
            path: rel.to_owned(),
            problem,
            stops,
        });
    };
    let mut unlisted: BTreeSet<String> = reads.ops.iter().chain(&reads.plugins).cloned().collect();
    match read_index(dir) {
        Some(index) => {
            unlisted.retain(|rel| !index.files.contains_key(rel));
            for (rel, listed) in &index.files {
                let path = dir.join(rel);
                let Ok(meta) = std::fs::metadata(&path) else {
                    note(rel, Problem::Missing, reads.loads(rel));
                    continue;
                };
                if listed.modified == 0 || modified(&meta) != listed.modified {
                    unlisted.insert(rel.clone());
                    continue;
                }
                let found = meta.len();
                let problem = if found != listed.size {
                    match found {
                        0 => Problem::Empty,
                        _ => Problem::Size {
                            listed: listed.size,
                            found,
                        },
                    }
                } else if reads.loads(rel)
                    && found > 0
                    && first_bytes(&path, HEAD)
                        .is_none_or(|h| crate::game_folder::baseline::hex(&h) != listed.head)
                {
                    Problem::Head
                } else {
                    continue;
                };
                note(rel, problem, reads.loads(rel));
            }
        }
        None => unlisted.extend(walk(dir).into_iter().filter(|rel| magic(rel).is_some())),
    }
    for rel in &unlisted {
        if let Some((problem, stops)) = sniff(dir, rel, &reads) {
            note(rel, problem, stops);
        }
    }
    found.into_values().collect()
}

/// [`check`], then the CRC-32 of every listed file not written since its install whose size
/// matches.
pub fn verify(dir: &Path, m: &AddonManifest) -> Vec<Damage> {
    let mut damage = check(dir, m);
    let Some(index) = read_index(dir) else {
        return damage;
    };
    let reads = Reads::of(dir, m);
    for (rel, listed) in &index.files {
        if damage.iter().any(|d| d.path == *rel) {
            continue;
        }
        let path = dir.join(rel);
        let Ok(mut file) = File::open(&path) else {
            continue;
        };
        let rewritten = file
            .metadata()
            .is_ok_and(|meta| listed.modified == 0 || modified(&meta) != listed.modified);
        if rewritten {
            continue;
        }
        let same = digest(&mut file).is_ok_and(|(e, _)| e.size == listed.size && e.crc32 == listed.crc32);
        if !same {
            damage.push(Damage {
                path: rel.clone(),
                problem: Problem::Content,
                stops: reads.loads(rel),
            });
        }
    }
    damage.sort_by(|a, b| a.path.cmp(&b.path));
    damage
}

/// The installed addons among `ids` with damaged files, by [`check`]. Built-in addons have no
/// folder and are skipped.
pub fn damaged<'a>(
    store: &Store,
    ids: impl IntoIterator<Item = &'a str>,
    installed: &BTreeMap<String, AddonManifest>,
) -> Vec<Damaged> {
    ids.into_iter()
        .filter(|id| !store.is_builtin(id))
        .filter_map(|id| {
            let m = installed.get(id)?;
            let dir = store.addons_dir().join(id);
            if !dir.is_dir() {
                return None;
            }
            let damage = check(&dir, m);
            (!damage.is_empty()).then(|| Damaged {
                id: id.to_owned(),
                name: m.name.clone(),
                damage,
            })
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::cell::Cell;

    use super::*;

    thread_local! {
        static PANIC_AFTER: Cell<Option<usize>> = const { Cell::new(None) };
    }

    pub(super) fn after_file() {
        PANIC_AFTER.with(|left| match left.get() {
            Some(0) => {
                left.set(None);
                panic!("stopped part way through the install");
            }
            Some(n) => left.set(Some(n - 1)),
            None => {}
        });
    }

    /// The next [`seal`] on this thread panics after `files` files, as a launcher killed
    /// part way through an install stops.
    pub(crate) fn panic_after(files: usize) {
        PANIC_AFTER.with(|left| left.set(Some(files)));
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("swofty-addon-files-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest(operations: serde_json::Value) -> AddonManifest {
        let doc = serde_json::json!({
            "schema_version": 1,
            "id": "a.real",
            "name": "Real Liveries",
            "version": "1.1.7",
            "author": "a",
            "operations": operations,
        });
        serde_json::from_value(doc).unwrap()
    }

    fn lang() -> AddonManifest {
        manifest(serde_json::json!({ "assets": ["assets/lang.json"] }))
    }

    fn season() -> AddonManifest {
        manifest(serde_json::json!({
            "erp": ["erp/frontend.json"],
            "native": [{ "dll": "native/season.dll" }],
        }))
    }

    fn addon(name: &str) -> std::path::PathBuf {
        let dir = tmp(name);
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("swofty.json"), "{}").unwrap();
        std::fs::write(dir.join("assets/lang.json"), br#"{"schema_version":1,"ops":[]}"#).unwrap();
        std::fs::write(dir.join("assets/cars.erp"), erp(70_000)).unwrap();
        dir
    }

    fn season_addon(name: &str) -> std::path::PathBuf {
        let dir = tmp(name);
        std::fs::create_dir_all(dir.join("erp/frontend")).unwrap();
        std::fs::create_dir_all(dir.join("native")).unwrap();
        std::fs::write(dir.join("swofty.json"), "{}").unwrap();
        std::fs::write(dir.join("erp/frontend.json"), br#"{"ops":[]}"#).unwrap();
        std::fs::write(dir.join("erp/frontend/drivers.erpk"), erp(711)).unwrap();
        std::fs::write(dir.join("native/season.dll"), b"MZ\x90\x00").unwrap();
        dir
    }

    fn erp(len: usize) -> Vec<u8> {
        let mut data = vec![7u8; len];
        data[..4].copy_from_slice(ERP_MAGIC);
        data
    }

    /// Writes `bytes` over the file at `path` and puts its write time back, as a disk that
    /// loses or garbles data does.
    fn corrupt(path: &Path, bytes: &[u8]) {
        let was = std::fs::metadata(path).unwrap().modified().unwrap();
        std::fs::write(path, bytes).unwrap();
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(was)
            .unwrap();
    }

    /// Writes `bytes` over the file at `path` a moment after it was last written, as a tool
    /// that changes an installed addon's file does.
    fn rewrite(path: &Path, bytes: &[u8]) {
        let was = std::fs::metadata(path).unwrap().modified().unwrap();
        std::fs::write(path, bytes).unwrap();
        let later = was + std::time::Duration::from_secs(2);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(later)
            .unwrap();
    }

    fn damage(path: &str, problem: Problem, stops: bool) -> Damage {
        Damage {
            path: path.into(),
            problem,
            stops,
        }
    }

    #[test]
    fn a_sealed_folder_checks_clean_and_lists_every_file_with_its_crc_head_and_write_time() {
        let dir = addon("clean");
        let index = seal(&dir).unwrap();
        assert_eq!(
            index.files.keys().collect::<Vec<_>>(),
            ["assets/cars.erp", "assets/lang.json", "swofty.json"]
        );
        let cars = &index.files["assets/cars.erp"];
        assert_eq!(cars.size, 70_000);
        assert_eq!(cars.head, "4552504b07070707");
        assert_ne!(cars.modified, 0);
        assert_eq!(index.files["swofty.json"].head, "7b7d");
        assert_eq!(read_index(&dir), Some(index));
        assert_eq!(check(&dir, &lang()), []);
        assert_eq!(verify(&dir, &lang()), []);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_empty_op_file_stops_play_with_or_without_a_file_list() {
        let dir = addon("empty-lang");
        std::fs::write(dir.join("assets/lang.json"), b"").unwrap();
        let found = check(&dir, &lang());
        assert_eq!(found, [damage("assets/lang.json", Problem::Empty, true)]);
        assert_eq!(found[0].line(), "assets/lang.json is empty");

        std::fs::write(dir.join("assets/lang.json"), br#"{"ops":[]}"#).unwrap();
        seal(&dir).unwrap();
        corrupt(&dir.join("assets/lang.json"), b"");
        assert_eq!(
            check(&dir, &lang()),
            [damage("assets/lang.json", Problem::Empty, true)]
        );
        rewrite(&dir.join("assets/lang.json"), b"");
        assert_eq!(
            check(&dir, &lang()),
            [damage("assets/lang.json", Problem::Empty, true)]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn without_a_file_list_an_op_file_of_zeros_stops_play_and_one_of_blanks_is_logged() {
        let dir = addon("zeros");
        let path = dir.join("assets/lang.json");
        std::fs::write(&path, [0u8; 274]).unwrap();
        assert_eq!(
            check(&dir, &lang()),
            [damage("assets/lang.json", Problem::Zeros, true)]
        );
        std::fs::write(&path, b"  \r\n ").unwrap();
        let found = check(&dir, &lang());
        assert_eq!(found, [damage("assets/lang.json", Problem::Blank, false)]);
        let d = Damaged {
            id: "a.real".into(),
            name: "Real Liveries".into(),
            damage: found,
        };
        assert!(!d.stops_play());
        assert_eq!(d.lines(), ["warning: assets/lang.json holds only blanks"]);
        std::fs::write(&path, b"\n {\"ops\": []}").unwrap();
        assert_eq!(check(&dir, &lang()), []);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            check(&dir, &lang()),
            [damage("assets/lang.json", Problem::Missing, true)]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_listed_file_that_changed_without_a_write_stops_play_when_the_build_or_game_loads_it() {
        let dir = addon("sizes");
        std::fs::write(dir.join("readme.txt"), "hello").unwrap();
        seal(&dir).unwrap();
        corrupt(&dir.join("readme.txt"), b"hi");
        let found = check(&dir, &lang());
        assert_eq!(
            found,
            [damage("readme.txt", Problem::Size { listed: 5, found: 2 }, false)]
        );

        corrupt(&dir.join("assets/cars.erp"), &erp(100));
        assert_eq!(
            check(&dir, &lang())[0],
            damage(
                "assets/cars.erp",
                Problem::Size {
                    listed: 70_000,
                    found: 100,
                },
                true
            )
        );
        let mut same_size = vec![0u8; 70_000];
        same_size[..4].copy_from_slice(b"ERPX");
        corrupt(&dir.join("assets/cars.erp"), &same_size);
        assert_eq!(
            check(&dir, &lang())[0],
            damage("assets/cars.erp", Problem::Head, true)
        );
        std::fs::remove_file(dir.join("assets/cars.erp")).unwrap();
        assert_eq!(
            check(&dir, &lang())[0],
            damage("assets/cars.erp", Problem::Missing, true)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_listed_file_written_since_its_install_is_only_checked_as_an_unlisted_one() {
        let dir = addon("rewritten");
        std::fs::write(dir.join("readme.txt"), "hello").unwrap();
        seal(&dir).unwrap();
        rewrite(&dir.join("readme.txt"), b"hi");
        rewrite(&dir.join("assets/lang.json"), br#"{"ops":[{"op":"x"}]}"#);
        rewrite(&dir.join("assets/cars.erp"), b"");
        assert_eq!(
            check(&dir, &lang()),
            [damage("assets/cars.erp", Problem::Empty, false)]
        );
        assert_eq!(
            verify(&dir, &lang()),
            [damage("assets/cars.erp", Problem::Empty, false)]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_reads_every_file_and_finds_a_change_of_the_same_size() {
        let dir = addon("verify");
        seal(&dir).unwrap();
        let mut changed = erp(70_000);
        changed[69_999] = 8;
        corrupt(&dir.join("assets/cars.erp"), &changed);
        assert_eq!(check(&dir, &lang()), []);
        assert_eq!(
            verify(&dir, &lang()),
            [damage("assets/cars.erp", Problem::Content, true)]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn without_a_file_list_a_zeroed_archive_or_plugin_stops_play_and_odd_bytes_are_logged() {
        let dir = season_addon("magic");
        let m = season();
        assert_eq!(check(&dir, &m), []);

        std::fs::write(dir.join("erp/frontend/drivers.erpk"), [0u8; 711]).unwrap();
        let found = check(&dir, &m);
        assert_eq!(found, [damage("erp/frontend/drivers.erpk", Problem::Zeros, true)]);
        assert_eq!(found[0].line(), "erp/frontend/drivers.erpk holds only zeros");

        std::fs::write(dir.join("erp/frontend/drivers.erpk"), b"PK\x03\x04rest").unwrap();
        std::fs::write(dir.join("native/season.dll"), b"\x7fELF").unwrap();
        assert_eq!(
            check(&dir, &m),
            [
                damage(
                    "erp/frontend/drivers.erpk",
                    Problem::Magic { expected: "ERPK" },
                    false
                ),
                damage("native/season.dll", Problem::Magic { expected: "MZ" }, false),
            ]
        );

        std::fs::write(dir.join("native/season.dll"), [0u8; 4]).unwrap();
        assert_eq!(
            check(&dir, &m)[1],
            damage("native/season.dll", Problem::Zeros, true)
        );
        std::fs::remove_file(dir.join("native/season.dll")).unwrap();
        assert_eq!(
            check(&dir, &m)[1],
            damage("native/season.dll", Problem::Missing, true)
        );

        let no_erp_ops = manifest(serde_json::json!({}));
        std::fs::write(dir.join("erp/frontend/drivers.erpk"), [0u8; 711]).unwrap();
        assert_eq!(
            check(&dir, &no_erp_ops),
            [damage("erp/frontend/drivers.erpk", Problem::Zeros, false)]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn without_a_file_list_an_empty_placeholder_archive_is_logged_and_does_not_stop_play() {
        let dir = season_addon("placeholder");
        std::fs::write(dir.join("erp/frontend/placeholder.erp"), b"").unwrap();
        let found = check(&dir, &season());
        assert_eq!(
            found,
            [damage("erp/frontend/placeholder.erp", Problem::Empty, false)]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn with_a_file_list_a_zeroed_archive_stops_play_and_one_installed_empty_does_not() {
        let dir = season_addon("listed-magic");
        std::fs::write(dir.join("erp/frontend/placeholder.erp"), b"").unwrap();
        seal(&dir).unwrap();
        assert_eq!(check(&dir, &season()), []);
        corrupt(&dir.join("erp/frontend/drivers.erpk"), &[0u8; 711]);
        let found = check(&dir, &season());
        assert_eq!(found, [damage("erp/frontend/drivers.erpk", Problem::Head, true)]);
        assert_eq!(
            found[0].line(),
            "erp/frontend/drivers.erpk does not start with the bytes the install wrote"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_db_editor_addons_database_op_file_is_not_one_the_build_reads() {
        let dir = tmp("dbedit");
        std::fs::write(dir.join(crate::dbedit::package::MARKER), b"{}").unwrap();
        std::fs::write(dir.join("database.json"), b"").unwrap();
        let m = manifest(serde_json::json!({ "database": ["database.json"] }));
        assert_eq!(check(&dir, &m), []);
        std::fs::remove_file(dir.join(crate::dbedit::package::MARKER)).unwrap();
        assert_eq!(check(&dir, &m), [damage("database.json", Problem::Empty, true)]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn settle_reads_back_what_is_on_disk() {
        let dir = tmp("settle");
        std::fs::write(dir.join("a.bin"), b"abc").unwrap();
        let entry = settle(&dir.join("a.bin")).unwrap();
        let mut crc = flate2::Crc::new();
        crc.update(b"abc");
        assert_eq!(
            entry,
            Entry {
                size: 3,
                crc32: crc.sum()
            }
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_summary_names_only_files_that_stop_play() {
        let d = Damaged {
            id: "a".into(),
            name: "A".into(),
            damage: vec![
                damage("b.erp", Problem::Empty, false),
                damage("assets/lang.json", Problem::Empty, true),
                damage("c.json", Problem::Missing, true),
            ],
        };
        assert!(d.stops_play());
        assert_eq!(d.summary(), "assets/lang.json is empty, c.json is missing");
        assert_eq!(
            d.lines(),
            [
                "warning: b.erp is empty",
                "assets/lang.json is empty",
                "c.json is missing"
            ]
        );
    }
}
