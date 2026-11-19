//! The list of files an install writes into an addon's folder, with each file's size and
//! CRC-32, and the checks that compare the folder with it. Play checks sizes and the first
//! bytes of the files the build and the game load; Verify reads every file.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::manifest::AddonManifest;
use crate::store::Store;
use crate::{Error, Result, io_err};

/// The file list, in the addon's folder. Only installs write it.
pub const INDEX_FILE: &str = ".swofty-files.json";
const SCHEMA: u32 = 1;
/// How much of an op file the size check reads to tell JSON from zeros or blanks.
const PEEK: usize = 4096;
const ERP_MAGIC: &[u8; 4] = b"ERPK";
const DLL_MAGIC: &[u8; 2] = b"MZ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub size: u64,
    pub crc32: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    pub schema: u32,
    pub files: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Missing,
    Empty,
    Size { listed: u64, found: u64 },
    Blank,
    Magic { expected: &'static str },
    Content,
}

/// A file of an installed addon that is not what the install wrote. `read_by_build` marks
/// the op files the build parses and the archives and plugins it places, which fail the
/// build or the game when damaged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Damage {
    pub path: String,
    pub problem: Problem,
    pub read_by_build: bool,
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
            Problem::Blank => format!("{path} holds no JSON, only zeros or blanks"),
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
    /// True when the build would fail on it.
    pub fn stops_play(&self) -> bool {
        self.damage.iter().any(|d| d.read_by_build)
    }

    /// The first damaged files, build-read ones first.
    pub fn summary(&self) -> String {
        const SHOWN: usize = 3;
        let mut damage: Vec<&Damage> = self.damage.iter().collect();
        damage.sort_by_key(|d| !d.read_by_build);
        let mut text = damage
            .iter()
            .take(SHOWN)
            .map(|d| d.line())
            .collect::<Vec<_>>()
            .join(", ");
        if damage.len() > SHOWN {
            text.push_str(&format!(" and {} more", damage.len() - SHOWN));
        }
        text
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
            index.files.insert(rel, settle(&path)?);
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
    let (mut file, writable) = open(path).map_err(io_err(path))?;
    let listed = file.metadata().map_err(io_err(path))?.len();
    let entry = digest(&mut file).map_err(io_err(path))?;
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
    Ok(entry)
}

fn digest(r: &mut impl Read) -> std::io::Result<Entry> {
    let mut crc = flate2::Crc::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        size += n as u64;
    }
    Ok(Entry {
        size,
        crc32: crc.sum(),
    })
}

pub fn read_index(dir: &Path) -> Option<Index> {
    crate::read_json::<Index>(&dir.join(INDEX_FILE))
        .ok()
        .filter(|i| i.schema == SCHEMA)
}

fn op_files(m: &AddonManifest) -> Vec<String> {
    let mut out: Vec<String> = m
        .operations
        .op_files()
        .map(|(_, rel)| rel.to_owned())
        .chain(m.operations.hud.iter().cloned())
        .map(|rel| rel.trim_start_matches("./").replace('\\', "/"))
        .collect();
    out.sort();
    out.dedup();
    out
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

fn starts_with(path: &Path, magic: &[u8]) -> bool {
    let mut head = vec![0u8; magic.len()];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok_and(|()| head == magic)
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

fn blank(path: &Path, len: u64) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut head = vec![0u8; PEEK.min(len as usize)];
    if file.read_exact(&mut head).is_err() {
        return false;
    }
    match head.iter().find(|b| !b.is_ascii_whitespace()) {
        Some(0) => true,
        Some(_) => false,
        None => len as usize <= PEEK,
    }
}

/// The damage a stat of each file finds in the addon folder `dir`: files the install listed
/// that are missing or not their listed size, op files that are missing, empty or blank, and
/// ERP archives and plugins that are empty or don't start as one. An addon installed before
/// installs wrote file lists has the files in its folder checked.
pub fn check(dir: &Path, m: &AddonManifest) -> Vec<Damage> {
    let ops = op_files(m);
    let plugins: Vec<String> = m
        .operations
        .native
        .iter()
        .map(|n| n.dll.trim_start_matches("./").replace('\\', "/"))
        .collect();
    let loaded = |rel: &str| ops.iter().any(|o| o == rel) || magic(rel).is_some();
    let mut found: BTreeMap<String, Damage> = BTreeMap::new();
    let mut note = |path: &str, problem: Problem| {
        found.entry(path.to_owned()).or_insert(Damage {
            path: path.to_owned(),
            problem,
            read_by_build: loaded(path),
        });
    };
    let index = read_index(dir);
    let mut files = match &index {
        Some(index) => index.files.keys().cloned().collect(),
        None => walk(dir),
    };
    if let Some(index) = &index {
        for (rel, entry) in &index.files {
            match std::fs::metadata(dir.join(rel)) {
                Err(_) => note(rel, Problem::Missing),
                Ok(meta) if meta.len() == entry.size => {}
                Ok(meta) if meta.len() == 0 => note(rel, Problem::Empty),
                Ok(meta) => note(
                    rel,
                    Problem::Size {
                        listed: entry.size,
                        found: meta.len(),
                    },
                ),
            }
        }
    }
    for rel in &ops {
        let path = dir.join(rel);
        match std::fs::metadata(&path) {
            Err(_) => note(rel, Problem::Missing),
            Ok(meta) if meta.len() == 0 => note(rel, Problem::Empty),
            Ok(meta) if blank(&path, meta.len()) => note(rel, Problem::Blank),
            Ok(_) => {}
        }
    }
    files.extend(plugins);
    files.sort();
    files.dedup();
    for rel in &files {
        let Some((bytes, expected)) = magic(rel) else {
            continue;
        };
        let path = dir.join(rel);
        match std::fs::metadata(&path) {
            Err(_) => note(rel, Problem::Missing),
            Ok(meta) if meta.len() == 0 => note(rel, Problem::Empty),
            Ok(_) if !starts_with(&path, bytes) => note(rel, Problem::Magic { expected }),
            Ok(_) => {}
        }
    }
    found.into_values().collect()
}

/// [`check`], then the CRC-32 of every listed file whose size matches.
pub fn verify(dir: &Path, m: &AddonManifest) -> Vec<Damage> {
    let mut damage = check(dir, m);
    let Some(index) = read_index(dir) else {
        return damage;
    };
    let ops = op_files(m);
    for (rel, entry) in &index.files {
        if damage.iter().any(|d| d.path == *rel) {
            continue;
        }
        let same = File::open(dir.join(rel))
            .and_then(|mut f| digest(&mut f))
            .is_ok_and(|e| e == *entry);
        if !same {
            damage.push(Damage {
                path: rel.clone(),
                problem: Problem::Content,
                read_by_build: ops.iter().any(|o| o == rel) || magic(rel).is_some(),
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

    fn manifest(assets: &[&str]) -> AddonManifest {
        let doc = serde_json::json!({
            "schema_version": 1,
            "id": "a.real",
            "name": "Real Liveries",
            "version": "1.1.7",
            "author": "a",
            "operations": { "assets": assets },
        });
        serde_json::from_value(doc).unwrap()
    }

    fn addon(name: &str) -> std::path::PathBuf {
        let dir = tmp(name);
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("swofty.json"), "{}").unwrap();
        std::fs::write(dir.join("assets/lang.json"), br#"{"schema_version":1,"ops":[]}"#).unwrap();
        std::fs::write(dir.join("assets/cars.erp"), erp(70_000)).unwrap();
        dir
    }

    fn erp(len: usize) -> Vec<u8> {
        let mut data = vec![7u8; len];
        data[..4].copy_from_slice(ERP_MAGIC);
        data
    }

    #[test]
    fn a_sealed_folder_checks_clean_and_lists_every_file_with_its_crc() {
        let dir = addon("clean");
        let index = seal(&dir).unwrap();
        assert_eq!(
            index.files.keys().collect::<Vec<_>>(),
            ["assets/cars.erp", "assets/lang.json", "swofty.json"]
        );
        assert_eq!(index.files["assets/cars.erp"].size, 70_000);
        assert_eq!(read_index(&dir), Some(index));
        let m = manifest(&["assets/lang.json"]);
        assert_eq!(check(&dir, &m), []);
        assert_eq!(verify(&dir, &m), []);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_empty_op_file_is_damage_the_build_reads_with_or_without_a_file_list() {
        let dir = addon("empty-lang");
        let m = manifest(&["assets/lang.json"]);
        std::fs::write(dir.join("assets/lang.json"), b"").unwrap();
        let damage = check(&dir, &m);
        assert_eq!(
            damage,
            [Damage {
                path: "assets/lang.json".into(),
                problem: Problem::Empty,
                read_by_build: true,
            }]
        );
        assert_eq!(damage[0].line(), "assets/lang.json is empty");

        std::fs::write(dir.join("assets/lang.json"), br#"{"ops":[]}"#).unwrap();
        seal(&dir).unwrap();
        std::fs::write(dir.join("assets/lang.json"), b"").unwrap();
        assert_eq!(check(&dir, &m)[0].problem, Problem::Empty);
        assert!(check(&dir, &m)[0].read_by_build);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_op_file_of_zeros_or_blanks_is_damage() {
        let dir = addon("zeros");
        let m = manifest(&["assets/lang.json"]);
        std::fs::write(dir.join("assets/lang.json"), [0u8; 274]).unwrap();
        assert_eq!(check(&dir, &m)[0].problem, Problem::Blank);
        std::fs::write(dir.join("assets/lang.json"), b"  \r\n ").unwrap();
        assert_eq!(check(&dir, &m)[0].problem, Problem::Blank);
        std::fs::write(dir.join("assets/lang.json"), b"\n {\"ops\": []}").unwrap();
        assert_eq!(check(&dir, &m), []);
        std::fs::remove_file(dir.join("assets/lang.json")).unwrap();
        assert_eq!(check(&dir, &m)[0].problem, Problem::Missing);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn listed_files_with_another_size_or_missing_are_damage_the_build_may_not_read() {
        let dir = addon("sizes");
        let m = manifest(&["assets/lang.json"]);
        std::fs::write(dir.join("readme.txt"), "hello").unwrap();
        seal(&dir).unwrap();
        std::fs::write(dir.join("readme.txt"), "hi").unwrap();
        let damage = check(&dir, &m);
        assert_eq!(
            damage,
            [Damage {
                path: "readme.txt".into(),
                problem: Problem::Size { listed: 5, found: 2 },
                read_by_build: false,
            }]
        );
        let d = Damaged {
            id: "a.real".into(),
            name: "Real Liveries".into(),
            damage,
        };
        assert!(!d.stops_play());
        std::fs::write(dir.join("assets/cars.erp"), erp(100)).unwrap();
        let damage = check(&dir, &m);
        assert_eq!(damage[0].path, "assets/cars.erp");
        assert_eq!(
            damage[0].problem,
            Problem::Size {
                listed: 70_000,
                found: 100,
            }
        );
        assert!(damage[0].read_by_build);
        std::fs::remove_file(dir.join("assets/cars.erp")).unwrap();
        assert_eq!(check(&dir, &m)[0].problem, Problem::Missing);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn verify_reads_every_file_and_finds_a_change_of_the_same_size() {
        let dir = addon("verify");
        let m = manifest(&["assets/lang.json"]);
        seal(&dir).unwrap();
        let mut changed = erp(70_000);
        changed[69_999] = 8;
        std::fs::write(dir.join("assets/cars.erp"), changed).unwrap();
        assert_eq!(check(&dir, &m), []);
        let damage = verify(&dir, &m);
        assert_eq!(damage.len(), 1);
        assert_eq!(damage[0].problem, Problem::Content);
        assert_eq!(damage[0].path, "assets/cars.erp");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_archive_or_plugin_that_does_not_start_as_one_is_damage_the_build_reads() {
        let dir = addon("magic");
        let doc = serde_json::json!({
            "schema_version": 1, "id": "a.season", "name": "Season", "version": "1.0.10", "author": "a",
            "operations": {
                "erp": ["erp/frontend.json"],
                "native": [{ "dll": "native/season.dll" }],
            },
        });
        let m: AddonManifest = serde_json::from_value(doc).unwrap();
        std::fs::create_dir_all(dir.join("erp/frontend")).unwrap();
        std::fs::create_dir_all(dir.join("native")).unwrap();
        std::fs::write(dir.join("erp/frontend.json"), br#"{"ops":[]}"#).unwrap();
        std::fs::write(dir.join("erp/frontend/drivers.erpk"), erp(711)).unwrap();
        std::fs::write(dir.join("native/season.dll"), b"MZ\x90\x00").unwrap();
        assert_eq!(check(&dir, &m), []);

        std::fs::write(dir.join("erp/frontend/drivers.erpk"), [0u8; 711]).unwrap();
        let damage = check(&dir, &m);
        assert_eq!(
            damage,
            [Damage {
                path: "erp/frontend/drivers.erpk".into(),
                problem: Problem::Magic { expected: "ERPK" },
                read_by_build: true,
            }]
        );
        assert_eq!(
            damage[0].line(),
            "erp/frontend/drivers.erpk does not start with ERPK"
        );

        seal(&dir).unwrap();
        assert_eq!(
            check(&dir, &m),
            damage,
            "a file list of the same size still sniffs"
        );
        std::fs::write(dir.join("erp/frontend/drivers.erpk"), b"").unwrap();
        std::fs::write(dir.join("native/season.dll"), [0u8; 4]).unwrap();
        let damage = check(&dir, &m);
        assert_eq!(damage.len(), 2);
        assert_eq!(damage[0].problem, Problem::Empty);
        assert_eq!(damage[1].problem, Problem::Magic { expected: "MZ" });
        assert!(damage.iter().all(|d| d.read_by_build));
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
    fn the_summary_names_build_read_files_first() {
        let damage = vec![
            Damage {
                path: "b.erp".into(),
                problem: Problem::Missing,
                read_by_build: false,
            },
            Damage {
                path: "assets/lang.json".into(),
                problem: Problem::Empty,
                read_by_build: true,
            },
        ];
        let d = Damaged {
            id: "a".into(),
            name: "A".into(),
            damage,
        };
        assert!(d.stops_play());
        assert_eq!(d.summary(), "assets/lang.json is empty, b.erp is missing");
    }
}
