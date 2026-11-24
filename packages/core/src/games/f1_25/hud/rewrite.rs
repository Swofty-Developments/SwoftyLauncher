//! Edits to resources after the composer's other steps: a resource the composed archives hold is
//! edited in every copy alike, and any other is read from the stock and goes out in new archives over
//! every stock archive that carries it. The yield records and the screen nodes of yield records write
//! through this.

use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::erp::{Erp, Packer, Resource};

use super::composer::archive;
use super::stock::Stock;
use super::textures::Res;
use super::{Built, TemplateBase, err};
use crate::Result;

pub struct Rewrite<'a> {
    stock: &'a dyn Stock,
    parsed: Vec<Erp>,
    copies: BTreeMap<String, Vec<Vec<u8>>>,
    fresh: BTreeSet<String>,
    changed: BTreeSet<String>,
}

impl<'a> Rewrite<'a> {
    pub fn new(stock: &'a dyn Stock, archives: &[Built]) -> Result<Self> {
        let parsed = archives
            .iter()
            .map(|a| Erp::parse(&a.bytes).map_err(|e| err(format!("{}: {e}", a.file))))
            .collect::<Result<_>>()?;
        Ok(Self {
            stock,
            parsed,
            copies: BTreeMap::new(),
            fresh: BTreeSet::new(),
            changed: BTreeSet::new(),
        })
    }

    /// Every fragment of `resource` as the edits so far leave it: the composed copy (every copy
    /// alike), else the stock copy in `erp` with `base` applied to it.
    pub fn read(&mut self, resource: &str, erp: &str, base: Option<TemplateBase>) -> Result<Vec<Vec<u8>>> {
        if let Some(c) = self.copies.get(resource) {
            return Ok(c.clone());
        }
        let held: Vec<&Erp> = self.parsed.iter().filter(|e| e.find(resource).is_ok()).collect();
        let frags = if held.is_empty() {
            let mut frags = self.stock.read(erp, resource)?;
            if frags.is_empty() {
                return Err(err(format!("{resource} has no data")));
            }
            if let Some(edit) = base {
                frags[0] = edit(&frags[0])?;
            }
            self.fresh.insert(resource.to_owned());
            frags
        } else {
            let all: Vec<Vec<Vec<u8>>> = held
                .iter()
                .map(|e| {
                    e.find(resource)?
                        .fragments
                        .iter()
                        .map(|f| f.data().map_err(Into::into))
                        .collect::<Result<Vec<_>>>()
                })
                .collect::<Result<_>>()?;
            if all.iter().any(|c| c[0] != all[0][0]) {
                return Err(err(format!(
                    "{resource}: the composed archives hold different copies, so it cannot be edited in all of them"
                )));
            }
            all[0].clone()
        };
        self.copies.insert(resource.to_owned(), frags.clone());
        Ok(frags)
    }

    /// Replaces the first fragment of a resource [`Rewrite::read`] read.
    pub fn write(&mut self, resource: &str, first: Vec<u8>) -> Result<()> {
        let c = self
            .copies
            .get_mut(resource)
            .ok_or_else(|| err(format!("{resource} is written before it is read")))?;
        c[0] = first;
        self.changed.insert(resource.to_owned());
        Ok(())
    }

    /// Puts the written resources in `archives`: in place where they hold them, else in new archives
    /// `<stem>.erp`, `<stem>_2.erp` … one per set of stock archives that carry them.
    pub fn finish(
        mut self,
        archives: &mut [Built],
        stem: &str,
        what: &str,
        lines: &mut Vec<String>,
        packer: &Packer,
    ) -> Result<Vec<Built>> {
        let mut touched = BTreeSet::new();
        let mut fresh: Vec<Res> = Vec::new();
        for name in &self.changed {
            let frags = self.copies[name].clone();
            if self.fresh.contains(name) {
                let kind = if name.ends_with(".idf") {
                    "UILayout"
                } else {
                    "UIDF"
                };
                fresh.push((name.clone(), kind, frags));
                continue;
            }
            for (i, e) in self.parsed.iter_mut().enumerate() {
                let Ok(r) = e.find_mut(name) else { continue };
                let mut out = Vec::with_capacity(r.fragments.len());
                out.push(frags[0].clone());
                for f in &r.fragments[1..] {
                    out.push(f.data()?);
                }
                *r = Resource::like_with(packer, name, r, &out)?;
                touched.insert(i);
            }
        }
        for i in touched {
            archives[i].bytes = self.parsed[i].to_bytes()?;
        }
        let names: Vec<&str> = fresh.iter().map(|(n, _, _)| n.as_str()).collect();
        let carriers = self.stock.carriers(&names)?;
        let mut groups: BTreeMap<Vec<String>, Vec<Res>> = BTreeMap::new();
        for (name, kind, frags) in fresh {
            let erps = carriers.get(&name).cloned().unwrap_or_default();
            if erps.is_empty() {
                return Err(err(format!("no game archive carries {name}")));
            }
            groups.entry(erps).or_default().push((name, kind, frags));
        }
        let mut added = Vec::new();
        for (n, (erps, resources)) in groups.into_iter().enumerate() {
            let file = match n {
                0 => format!("{stem}.erp"),
                n => format!("{stem}_{}.erp", n + 1),
            };
            lines.push(format!(
                "{file}: {} for {what}, over {}",
                resources
                    .iter()
                    .map(|(r, _, _)| r.rsplit('/').next().unwrap_or(r).to_owned())
                    .collect::<Vec<_>>()
                    .join(", "),
                erps.iter()
                    .map(|e| e.rsplit('/').next().unwrap_or(e))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            added.push(Built {
                file,
                erps,
                bytes: archive(self.stock, resources, packer)?,
            });
        }
        Ok(added)
    }
}
