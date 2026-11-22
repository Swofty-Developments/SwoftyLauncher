use std::path::Path;

use swofty_formats::erp::{self, Erp, ErpFile, Resource};
use swofty_formats::pkg::{Node, Pkg};

use super::pack;
use super::{ASSET_GROUPS, ErpBuild, Slot, fail};
use crate::Result;
use crate::manifest::relative_inside;

impl ErpBuild<'_> {
    pub(super) fn owns(&self, erp: &str, resource: &str) -> bool {
        self.erps.get(erp).is_some_and(|a| {
            a.position(resource)
                .ok()
                .flatten()
                .is_some_and(|i| matches!(a.slots[i], Slot::Owned(_)))
        })
    }

    pub(super) fn add(&mut self, erp: &str, name: &str, kind: &str, data: &[Vec<u8>]) -> Result<()> {
        let packer = self.packer.clone();
        let wanted = erp::kind(kind).map_err(|e| fail(format!("add-resource {name}: {e}")))?;
        let target = self.erp(erp)?;
        let existing = target.position(name).map_err(|e| fail(format!("{erp}: {e}")))?;
        if existing.is_some_and(|i| matches!(target.slots[i], Slot::Stock(_))) {
            return Err(fail(format!(
                "{erp} in the game folder already has {name}, so something outside the launcher probably changed this file \
                 (a hand-installed mod or a leftover). \
                 Remove hand-installed mods, then Repair F1 25 in the EA app (Steam: Verify integrity of game files), then Play. \
                 (Addon authors: if the stock file has it, use put-resource.)"
            )));
        }
        let like = target
            .slots
            .iter()
            .map(|s| target.entry(s))
            .find(|r| r.kind == wanted && r.fragments.len() == data.len())
            .ok_or_else(|| {
                fail(format!(
                    "{erp}: add-resource {name} takes its layout from a {kind} resource with {} fragment(s) in the archive, and it has none",
                    data.len()
                ))
            })?;
        let res = pack::like(&packer, name, like, data).map_err(|e| fail(format!("{erp}: {e}")))?;
        match existing {
            Some(i) => target.slots[i] = Slot::Owned(res),
            None => target.slots.push(Slot::Owned(res)),
        }
        Ok(())
    }

    pub(super) fn load_from(&mut self, from: &Path, resource: &str) -> Result<Resource> {
        if !self.archives.contains_key(from) {
            let file = ErpFile::open(from).map_err(|e| fail(format!("{}: {e}", from.display())))?;
            self.archives.insert(from.to_path_buf(), file);
        }
        let file = &self.archives[from];
        let found = file
            .erp
            .find(resource)
            .map_err(|e| fail(format!("{}: {e}", from.display())))?;
        Ok(file.load(found)?)
    }

    pub(super) fn place(&mut self, erp: &str, res: Resource, replace: bool) -> Result<()> {
        let name = res.name.clone();
        let target = self.erp(erp)?;
        match target.position(&name).map_err(|e| fail(format!("{erp}: {e}")))? {
            Some(i) if replace => target.slots[i] = Slot::Owned(res),
            Some(_) => return Err(fail(format!("{erp}: {name} already exists"))),
            None => target.slots.push(Slot::Owned(res)),
        }
        Ok(())
    }

    pub(super) fn copy(
        &mut self,
        from: &Path,
        resource: &str,
        erp: &str,
        name: &str,
        replace: bool,
    ) -> Result<()> {
        let mut res = self.load_from(from, resource)?;
        res.name = name.to_owned();
        self.place(erp, res, replace)
    }

    // The game's own copy of `resource`: in `erp`, else in the first archive directly
    // in the asset group folder its name starts with, such as ui_package.
    pub(super) fn stock_copy(&mut self, erp: &str, resource: &str) -> Result<Option<Resource>> {
        let file = &self.erp(erp)?.file;
        if let Some(r) = file.erp.resources.iter().find(|r| r.name == resource) {
            return Ok(Some(file.load(r)?));
        }
        if let Some(found) = self.stock.get(resource) {
            return Ok(found.clone());
        }
        let mut found = None;
        let package = resource.strip_prefix("eaid://").and_then(|r| r.split('/').next());
        if let Some(package) = package.filter(|p| relative_inside(p)) {
            let dir = format!("{ASSET_GROUPS}/{package}");
            let mut names: Vec<String> = std::fs::read_dir((self.original)(&dir))
                .into_iter()
                .flatten()
                .filter_map(|e| e.ok()?.file_name().into_string().ok())
                .filter(|n| n.to_ascii_lowercase().ends_with(".erp"))
                .collect();
            names.sort();
            for name in names {
                let path = (self.original)(&format!("{dir}/{name}"));
                let hits = Erp::read_matching(&path, |n| n == resource)
                    .map_err(|e| fail(format!("{dir}/{name}: {e}")))?;
                if let Some(r) = hits.into_iter().next() {
                    found = Some(r);
                    break;
                }
            }
        }
        self.stock.insert(resource.to_owned(), found.clone());
        Ok(found)
    }

    pub(super) fn current(&mut self, erp: &str, resource: &str) -> Result<Option<Resource>> {
        let target = self.erp(erp)?;
        let Some(i) = target
            .position(resource)
            .map_err(|e| fail(format!("{erp}: {e}")))?
        else {
            return Ok(None);
        };
        Ok(Some(target.loaded(i)?.into_owned()))
    }

    pub(super) fn patch(
        &mut self,
        erp: &str,
        resource: &str,
        fragment: usize,
        edit: impl FnOnce(&mut Node) -> Result<usize>,
    ) -> Result<()> {
        let packer = self.packer.clone();
        let e = self.erp(erp)?;
        let i = e.find(erp, resource)?;
        let res = e.owned(i)?;
        let body = res
            .fragments
            .get(fragment)
            .ok_or_else(|| fail(format!("{resource}: no fragment {fragment}")))?
            .data()?;
        let mut pkg = Pkg::parse(&body).map_err(|err| fail(format!("{resource}: {err}")))?;
        edit(&mut pkg.root).map_err(|err| fail(format!("{resource}: {err}")))?;
        pack::set_fragment(&packer, res, fragment, &pkg.to_bytes()?)?;
        Ok(())
    }
}
