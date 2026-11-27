//! Where the composer reads the stock resources from: a game folder's archives, or those archives with the
//! changes other addons make.

use super::dials;
use super::err;
use crate::{Error, Result};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use swofty_formats::erp::{ErpFile, Resource};

/// Where the stock resources come from.
pub trait Stock {
    /// Every fragment of a resource of an archive, decoded.
    fn read(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>>;
    /// An index-only resource whose layout (type argument, fragment names, flags, compression) a
    /// new resource of `kind` with `fragments` fragments copies. A resource that exists in the
    /// stock archive under `name` keeps its own layout.
    fn layout(&self, name: &str, kind: &str, fragments: usize) -> Result<Resource>;
    fn header(&self) -> Result<([u8; swofty_formats::erp::HEADER_LEN], u32)>;
    /// For each of `names`, every stock archive of its asset group that carries it, sorted. The game
    /// loads a resource from whichever of them reads it first, so a composed resource goes to all.
    fn carriers(&self, names: &[&str]) -> Result<BTreeMap<String, Vec<String>>>;
    /// The game's own copy of a resource, before any other addon's change to it.
    fn game_copy(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>> {
        self.read(erp, resource)
    }
}

/// The stock archives of a game folder.
pub struct GameStock {
    pub(super) game: PathBuf,
    pub(super) originals: BTreeMap<String, PathBuf>,
    pub(super) known: Option<BTreeSet<String>>,
    pub(super) files: RefCell<BTreeMap<String, Rc<ErpFile>>>,
    pub(super) carried: RefCell<BTreeMap<String, Vec<String>>>,
}

impl GameStock {
    pub fn new(game: impl Into<PathBuf>) -> Self {
        Self {
            game: game.into(),
            originals: BTreeMap::new(),
            known: None,
            files: RefCell::new(BTreeMap::new()),
            carried: RefCell::new(BTreeMap::new()),
        }
    }

    /// The game folder with each file the launcher keeps an original of read from there: a folder
    /// Swofty has built may already carry composed HUD archives, and the composer starts from stock.
    pub fn with_baseline(game: impl Into<PathBuf>, data: &Path) -> Result<Self> {
        let game = game.into();
        let exe = crate::game_folder::baseline::sha256_file(&game.join(crate::GAME_EXE))?;
        let mut best: Option<crate::game_folder::baseline::Baseline> = None;
        let dir = data.join("baseline");
        let entries = std::fs::read_dir(&dir).map_err(|e| Error::Io {
            path: dir.clone(),
            source: e,
        })?;
        for e in entries.flatten() {
            let Ok(text) = std::fs::read_to_string(e.path().join("baseline.json")) else {
                continue;
            };
            let Ok(b) = serde_json::from_str::<crate::game_folder::baseline::Baseline>(&text) else {
                continue;
            };
            if b.exe_sha256 == exe && best.as_ref().is_none_or(|o| b.files.len() > o.files.len()) {
                best = Some(b);
            }
        }
        let base = best.ok_or_else(|| err("the launcher has no record of the game's original files"))?;
        let objects = crate::game_folder::baseline::Objects {
            root: dir.join("objects"),
        };
        let originals = base
            .files
            .iter()
            .map(|(rel, rec)| (rel.clone(), objects.path(&rec.sha256)))
            .filter(|(_, p)| p.is_file())
            .collect();
        let known = base.files.keys().map(|rel| rel.to_ascii_lowercase()).collect();
        Ok(Self {
            game,
            originals,
            known: Some(known),
            files: RefCell::new(BTreeMap::new()),
            carried: RefCell::new(BTreeMap::new()),
        })
    }

    /// The file the composer reads `rel` from: the launcher's original when it keeps one, else the game folder's.
    pub fn original(&self, rel: &str) -> PathBuf {
        self.originals
            .get(rel)
            .cloned()
            .unwrap_or_else(|| self.game.join(rel))
    }

    fn file(&self, erp: &str) -> Result<Rc<ErpFile>> {
        if let Some(f) = self.files.borrow().get(erp) {
            return Ok(f.clone());
        }
        let path = self
            .originals
            .get(erp)
            .cloned()
            .unwrap_or_else(|| self.game.join(erp));
        let f = Rc::new(ErpFile::open(&path)?);
        self.files.borrow_mut().insert(erp.to_owned(), f.clone());
        Ok(f)
    }

    /// Reads which archives carry each of `names` into the cache, one pass over each asset group
    /// folder, skipping files the launcher's baseline has no record of (copies the player left there); every archive is read from the original the launcher keeps of it when there is one.
    fn scan(&self, names: &[&str]) -> Result<()> {
        let mut by_group: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        {
            let carried = self.carried.borrow();
            for n in names.iter().filter(|n| !carried.contains_key(**n)) {
                match crate::erp::group_dir(n) {
                    Some(g) => {
                        by_group.entry(g).or_default().insert(n);
                    }
                    None => {
                        by_group.entry(String::new()).or_default().insert(n);
                    }
                }
            }
        }
        let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (group, wanted) in &by_group {
            for n in wanted {
                found.insert((*n).to_owned(), Vec::new());
            }
            if group.is_empty() {
                continue;
            }
            let dir = self.game.join(group);
            if !dir.is_dir() {
                return Err(err(format!("{} is not a folder", dir.display())));
            }
            let mut files = Vec::new();
            crate::erp::erp_files(&dir, group, &mut files);
            files.sort();
            for rel in files {
                if self
                    .known
                    .as_ref()
                    .is_some_and(|k| !k.contains(&rel.to_ascii_lowercase()))
                {
                    continue;
                }
                let Ok(f) = ErpFile::open(&self.original(&rel)) else {
                    continue;
                };
                for r in &f.erp.resources {
                    if wanted.contains(r.name.as_str())
                        && let Some(list) = found.get_mut(&r.name)
                        && list.last() != Some(&rel)
                    {
                        list.push(rel.clone());
                    }
                }
            }
        }
        self.carried.borrow_mut().extend(found);
        Ok(())
    }
}

impl Stock for GameStock {
    fn read(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>> {
        let f = self.file(erp)?;
        let res = f
            .erp
            .resources
            .iter()
            .find(|r| r.name == resource)
            .ok_or_else(|| Error::ResourceMissing {
                erp: erp.to_owned(),
                resource: resource.to_owned(),
            })?;
        let loaded = f.load(res)?;
        Ok(loaded
            .fragments
            .iter()
            .map(|fr| fr.data())
            .collect::<std::result::Result<_, _>>()?)
    }

    fn layout(&self, name: &str, kind: &str, fragments: usize) -> Result<Resource> {
        let f = self.file(dials::STOCK_ERP)?;
        let same = |r: &&Resource| r.kind() == kind && r.fragments.len() == fragments;
        f.erp
            .resources
            .iter()
            .filter(same)
            .find(|r| r.name == name)
            .or_else(|| f.erp.resources.iter().find(same))
            .cloned()
            .ok_or_else(|| {
                err(format!(
                    "no stock {kind} with {fragments} fragment(s) to copy a layout from"
                ))
            })
    }

    fn header(&self) -> Result<([u8; swofty_formats::erp::HEADER_LEN], u32)> {
        let f = self.file(dials::STOCK_ERP)?;
        Ok((f.erp.header, f.erp.version))
    }

    fn carriers(&self, names: &[&str]) -> Result<BTreeMap<String, Vec<String>>> {
        self.scan(names)?;
        let carried = self.carried.borrow();
        Ok(names
            .iter()
            .map(|n| ((*n).to_owned(), carried.get(*n).cloned().unwrap_or_default()))
            .collect())
    }
}

/// The stock archives with the changes other addons make to the resources the composer writes whole.
/// The composer starts from these, so its overlay carries those changes instead of overwriting them.
pub struct Layered<'a> {
    pub inner: &'a dyn Stock,
    pub build: &'a crate::erp::ErpBuild<'a>,
}

impl Stock for Layered<'_> {
    fn read(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>> {
        match self.build.fragments(erp, resource)? {
            Some(f) => Ok(f),
            None => self.inner.read(erp, resource),
        }
    }

    fn layout(&self, name: &str, kind: &str, fragments: usize) -> Result<Resource> {
        self.inner.layout(name, kind, fragments)
    }

    fn header(&self) -> Result<([u8; swofty_formats::erp::HEADER_LEN], u32)> {
        self.inner.header()
    }

    fn carriers(&self, names: &[&str]) -> Result<BTreeMap<String, Vec<String>>> {
        self.inner.carriers(names)
    }

    fn game_copy(&self, erp: &str, resource: &str) -> Result<Vec<Vec<u8>>> {
        self.inner.game_copy(erp, resource)
    }
}
