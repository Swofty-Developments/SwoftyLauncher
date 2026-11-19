use swofty_formats::erp::Erp;
use swofty_formats::pkg::Pkg;

use super::pack;
use super::{ErpBuild, addon_of, checked, fail};
use crate::Result;

/// Where two addons that replace the same whole UI resource were merged node by node, and the nodes
/// they changed differently (the later addon's version is kept there).
#[derive(Debug, Clone, PartialEq)]
pub struct UiMerge {
    pub erp: String,
    pub resource: String,
    pub earlier: Vec<String>,
    pub later: String,
    pub stats: crate::ui::merge::Stats,
    pub conflicts: Vec<UiConflict>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiConflict {
    pub path: String,
    pub earlier: Vec<String>,
}

impl ErpBuild<'_> {
    // The bytes of a UI resource as another addon's whole replacement left it, when this op replaces
    // it whole again.
    pub(super) fn earlier_whole(
        &mut self,
        erp: &str,
        resource: &str,
        origin: &str,
    ) -> Result<Option<Vec<u8>>> {
        let id = addon_of(origin);
        let earlier = self
            .whole
            .get(&(erp.to_owned(), resource.to_owned()))
            .is_some_and(|v| v.iter().any(|(a, _)| a != id));
        if !earlier || self.baseline.is_none() {
            return Ok(None);
        }
        self.fragment_zero(erp, resource)
    }

    fn fragment_zero(&mut self, erp: &str, resource: &str) -> Result<Option<Vec<u8>>> {
        let Some(res) = self.current(erp, resource)? else {
            return Ok(None);
        };
        if res.fragments.len() != 1 {
            return Ok(None);
        }
        Ok(Some(res.fragments[0].data()?))
    }

    fn stock_ui(&mut self, erp: &str, resource: &str) -> Result<Option<Pkg>> {
        let key = (erp.to_owned(), resource.to_owned());
        if let Some(found) = self.ui_base.get(&key) {
            return Ok(found.clone());
        }
        let found = match self.baseline {
            Some(of) => {
                let path = of(checked(erp)?);
                Erp::read_matching(&path, |n| n == resource)
                    .map_err(|e| fail(format!("{erp}: {e}")))?
                    .into_iter()
                    .next()
                    .filter(|r| r.fragments.len() == 1)
                    .map(|r| r.fragments[0].data())
                    .transpose()?
                    .filter(|d| swofty_formats::pkg::is_pkg(d))
                    .and_then(|d| Pkg::parse(&d).ok())
            }
            None => None,
        };
        self.ui_base.insert(key, found.clone());
        Ok(found)
    }

    pub(super) fn record_whole(
        &mut self,
        erp: &str,
        resource: &str,
        origin: &str,
        before: Option<Vec<u8>>,
    ) -> Result<()> {
        let packer = self.packer.clone();
        let Some(theirs_bytes) = self.fragment_zero(erp, resource)? else {
            return Ok(());
        };
        let Ok(theirs) = Pkg::parse(&theirs_bytes) else {
            self.whole.remove(&(erp.to_owned(), resource.to_owned()));
            return Ok(());
        };
        let id = addon_of(origin).to_owned();
        let key = (erp.to_owned(), resource.to_owned());
        let earlier: Vec<(String, Pkg)> = self
            .whole
            .get(&key)
            .map(|v| v.iter().filter(|(a, _)| *a != id).cloned().collect())
            .unwrap_or_default();
        if let (Some(ours_bytes), false) = (before, earlier.is_empty())
            && let Some(base) = self.stock_ui(erp, resource)?
            && let Ok(ours) = Pkg::parse(&ours_bytes)
        {
            let kind = crate::ui::merge::Kind::of(resource).expect("checked by the caller");
            let merged = crate::ui::merge::merge(kind, &base, &ours, &theirs)?;
            let bytes = merged
                .pkg
                .to_bytes()
                .map_err(|e| fail(format!("{erp}: {resource}: {e}")))?;
            let target = self.erp(erp)?;
            let i = target.find(erp, resource)?;
            pack::set_fragment(&packer, target.owned(i)?, 0, &bytes)
                .map_err(|e| fail(format!("{erp}: {e}")))?;
            let conflicts = merged
                .conflicts
                .iter()
                .map(|c| {
                    let keys = c.path.keys();
                    let changed: Vec<String> = earlier
                        .iter()
                        .filter(|(_, p)| {
                            let (a, b) = (
                                crate::ui::merge::lookup(&p.root, &keys),
                                crate::ui::merge::lookup(&base.root, &keys),
                            );
                            match (a, b) {
                                (Some(a), Some(b)) => !crate::ui::merge::same(a, b),
                                (None, None) => false,
                                _ => true,
                            }
                        })
                        .map(|(a, _)| a.clone())
                        .collect();
                    UiConflict {
                        path: c.path.show(),
                        earlier: if changed.is_empty() {
                            earlier.iter().map(|(a, _)| a.clone()).collect()
                        } else {
                            changed
                        },
                    }
                })
                .collect();
            self.ui_merges.push(UiMerge {
                erp: erp.to_owned(),
                resource: resource.to_owned(),
                earlier: earlier.iter().map(|(a, _)| a.clone()).collect(),
                later: id.clone(),
                stats: merged.stats,
                conflicts,
            });
        }
        let versions = self.whole.entry(key).or_default();
        versions.retain(|(a, _)| *a != id);
        versions.push((id, theirs));
        Ok(())
    }
}
