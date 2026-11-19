use swofty_formats::erp::{Erp, Resource};
use swofty_formats::pkg::Pkg;

use super::{ErpBuild, fail};
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
            for r in archive.written_mut() {
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
        let res = e.loaded(e.find(erp, resource)?)?;
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
        let res = e.loaded(e.find(erp, resource)?)?;
        let mut out = Vec::with_capacity(res.fragments.len());
        for f in &res.fragments {
            out.push(f.data()?);
        }
        Ok(Some(out))
    }

    pub fn resources(&self) -> impl Iterator<Item = (&str, &Resource, &Erp, &str)> {
        self.erps.iter().flat_map(move |(rel, a)| {
            a.written().map(move |r| {
                let origin = self
                    .writers
                    .get(&(rel.clone(), r.name.clone()))
                    .map_or("", String::as_str);
                (rel.as_str(), r, &a.file.erp, origin)
            })
        })
    }
}
