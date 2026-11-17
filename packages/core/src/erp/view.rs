use swofty_formats::erp::{Erp, Resource};
use swofty_formats::pkg::Pkg;

use super::{ErpBuild, Slot, fail};
use crate::Result;

impl ErpBuild<'_> {
    /// Puts `edit`'s copy in place of every resource the ops wrote that it
    /// returns one for, and counts them.
    pub fn edit_written(
        &mut self,
        mut edit: impl FnMut(&str, &Resource) -> Result<Option<Resource>>,
    ) -> Result<usize> {
        let mut n = 0;
        for (rel, archive) in &mut self.erps {
            for slot in &mut archive.slots {
                let Slot::Owned(r) = slot else { continue };
                if let Some(new) = edit(rel, r)? {
                    *r = new;
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    pub fn resource(&self, erp: &str, resource: &str, fragment: usize) -> Result<Option<Pkg>> {
        let Some(e) = self.erps.get(erp) else {
            return Ok(None);
        };
        let i = e.find(erp, resource)?;
        let loaded;
        let res = match &e.slots[i] {
            Slot::Owned(r) => r,
            Slot::Stock(j) => {
                loaded = e.file.load(&e.file.erp.resources[*j])?;
                &loaded
            }
        };
        let frag = res
            .fragments
            .get(fragment)
            .ok_or_else(|| fail(format!("{resource}: no fragment {fragment}")))?;
        Ok(Some(Pkg::parse(&frag.data()?)?))
    }

    /// Every fragment of `resource` in the archive `erp` as the build stands, or `None` while the archive is
    /// not loaded.
    pub fn fragments(&self, erp: &str, resource: &str) -> Result<Option<Vec<Vec<u8>>>> {
        let Some(e) = self.erps.get(erp) else {
            return Ok(None);
        };
        let i = e.find(erp, resource)?;
        let loaded;
        let res = match &e.slots[i] {
            Slot::Owned(r) => r,
            Slot::Stock(j) => {
                loaded = e.file.load(&e.file.erp.resources[*j])?;
                &loaded
            }
        };
        let mut out = Vec::with_capacity(res.fragments.len());
        for f in &res.fragments {
            out.push(f.data()?);
        }
        Ok(Some(out))
    }

    pub fn resources(&self) -> impl Iterator<Item = (&str, &Resource, &Erp, &str)> {
        self.erps.iter().flat_map(move |(rel, a)| {
            a.slots.iter().filter_map(move |s| match s {
                Slot::Owned(r) => {
                    let origin = self
                        .writers
                        .get(&(rel.clone(), r.name.clone()))
                        .map_or("", String::as_str);
                    Some((rel.as_str(), r, &a.file.erp, origin))
                }
                Slot::Stock(_) => None,
            })
        })
    }
}
