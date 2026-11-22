use std::path::{Path, PathBuf};

use swofty_formats::erp::Erp;
use swofty_formats::pkg::{Node, Pkg, Value};

use super::ops::{replaces, whole_write};
use super::pack;
use super::{ErpBuild, SCENE, checked, fail, mipmap_path, overlay_names, unsupported};
use crate::addons::builtin::loader_files;
use crate::asset::Source;
use crate::content::pkg_edit::{added_entry, apply_edits, holds_added, sources};
use crate::ops::{ErpOp, PkgEdit, PkgTarget};
use crate::ui::node::component_id;
use crate::{Error, Result};

pub(super) type SourceKey = (String, String, usize);

#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub erp: String,
    pub resource: String,
    pub origin: String,
}

fn scene_ids(data: &[u8]) -> Option<Vec<u64>> {
    let pkg = swofty_formats::pkg::is_pkg(data)
        .then(|| Pkg::parse(data).ok())
        .flatten()?;
    let list = pkg.root.as_obj()?.get("scenelist")?.as_arr()?;
    let ids: Vec<u64> = list
        .items
        .iter()
        .filter_map(|s| match &s.as_obj()?.get("name")?.as_val()?.value {
            Value::Raw(b) if b.len() == 8 => Some(u64::from_le_bytes(b[..].try_into().ok()?)),
            _ => None,
        })
        .collect();
    (!ids.is_empty()).then_some(ids)
}

fn check_scene(uri: &str, data: &[u8]) -> std::result::Result<(), String> {
    let want = component_id(uri.strip_suffix(".idf").unwrap_or(uri));
    match scene_ids(data) {
        None => Err("the source is not a scene: a UILayout resource holds a !pkg with a scenelist".into()),
        Some(ids) if ids.contains(&want) => Ok(()),
        Some(ids) => Err(format!(
            "the source is scene {:#018x}, this resource is scene {want:#018x}; it was saved from another .idf",
            ids[0]
        )),
    }
}

fn key(t: &PkgTarget) -> SourceKey {
    (t.erp.clone(), t.resource.clone(), t.fragment)
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(crate::io_err(path))
}

fn read_resource(path: &Path, t: &PkgTarget) -> Result<Node> {
    let found = Erp::read_matching(path, |n| n == t.resource).map_err(|e| fail(format!("{}: {e}", t.erp)))?;
    let [res] = found.as_slice() else {
        return Err(fail(format!(
            "{}: {} found {} times",
            t.erp,
            t.resource,
            found.len()
        )));
    };
    let frag = res
        .fragments
        .get(t.fragment)
        .ok_or_else(|| fail(format!("{}: no fragment {}", t.resource, t.fragment)))?;
    Ok(Pkg::parse(&frag.data()?)?.root)
}

impl ErpBuild<'_> {
    // Applies an op for an origin, such as an addon or a mod inside one. The
    // runtime route puts each origin's resources in an overlay archive of its own.
    pub fn apply_for(
        &mut self,
        op: &ErpOp,
        origin: &str,
        resolve: &dyn Fn(&Source) -> Result<PathBuf>,
    ) -> Result<()> {
        // An overlay off the runtime route is a put-resource of each resource in it.
        if let ErpOp::Overlay { erps, source } = op {
            let path = resolve(source)?;
            let names = overlay_names(&path)?;
            for erp in erps {
                for name in &names {
                    let put = ErpOp::PutResource {
                        erp: erp.clone(),
                        resource: name.clone(),
                        source: source.clone(),
                        rename: None,
                    };
                    self.apply_for(&put, origin, resolve)?;
                }
            }
            return Ok(());
        }
        if let Some((erp, resource)) = replaces(op).filter(|(e, r)| self.keeps(e, r)) {
            self.held.push(Held {
                erp: erp.to_owned(),
                resource: resource.to_owned(),
                origin: origin.to_owned(),
            });
            return Ok(());
        }
        if let ErpOp::AddResource { erp, resource, .. } = op
            && self.owns(erp, resource)
        {
            let earlier = self
                .writers
                .get(&(erp.clone(), resource.clone()))
                .map_or("an earlier addon", String::as_str);
            self.notes.push(format!(
                "{erp}: {resource} was already written by {earlier}; the add-resource from {origin} replaces it"
            ));
        }
        let whole = whole_write(op).filter(|(_, r)| crate::ui::merge::Kind::of(r).is_some());
        let before = match whole {
            Some((erp, resource)) if self.merge_ui => self.earlier_whole(erp, resource, origin)?,
            _ => None,
        };
        let templates = match op {
            ErpOp::PutResource { .. } => match replaces(op).filter(|(_, r)| r.ends_with(".ui")) {
                Some((erp, resource)) => self.templates_before(erp, resource)?.map(|t| (erp, resource, t)),
                None => None,
            },
            _ => None,
        };
        if let ErpOp::PatchPkg {
            erp,
            resource,
            fragment,
            edits,
        } = op
        {
            for (at, entry) in edits.iter().filter_map(added_entry) {
                if let Some(earlier) =
                    self.added
                        .get(&(erp.clone(), resource.clone(), at.clone(), entry.clone()))
                {
                    return Err(Error::DuplicateEntry {
                        resource: resource.clone(),
                        at,
                        entry,
                        earlier: earlier.clone(),
                    });
                }
            }
            if let Err(e) = self.apply(op, resolve) {
                return Err(match self.original_holds(erp, resource, *fragment, edits) {
                    true => Error::StaleInput {
                        rel: erp.clone(),
                        detail: e.to_string(),
                    },
                    false => e,
                });
            }
            for (at, entry) in edits.iter().filter_map(added_entry) {
                self.added
                    .insert((erp.clone(), resource.clone(), at, entry), origin.to_owned());
            }
        } else {
            self.apply(op, resolve)?;
        }
        if let Some((erp, resource)) = whole {
            self.record_whole(erp, resource, origin, before)?;
        }
        if let Some((erp, resource, t)) = templates {
            self.keep_templates(erp, resource, t, origin)?;
        }
        if let ErpOp::AppendSlices { erp, resource, .. } = op {
            self.appended
                .insert((erp.clone(), resource.clone()), origin.to_owned());
        }
        let written = match op {
            ErpOp::PatchPkg { erp, resource, .. }
            | ErpOp::ReplaceResource { erp, resource, .. }
            | ErpOp::AddResource { erp, resource, .. }
            | ErpOp::SetMaterial { erp, resource, .. } => Some((erp, resource.as_str())),
            ErpOp::CopyResource {
                erp,
                resource,
                rename,
                ..
            }
            | ErpOp::PutResource {
                erp,
                resource,
                rename,
                ..
            } => Some((erp, rename.as_deref().unwrap_or(resource))),
            ErpOp::RemoveResource { erp, resource } => {
                self.writers.remove(&(erp.clone(), resource.clone()));
                self.whole.remove(&(erp.clone(), resource.clone()));
                None
            }
            ErpOp::MergeSlices { erp, resource, .. } => self
                .merges
                .last()
                .filter(|m| m.whole || !m.changed.is_empty())
                .map(|_| (erp, resource.as_str())),
            // Appending replaces nothing an earlier addon wrote.
            ErpOp::AddMipmaps { .. } | ErpOp::Overlay { .. } | ErpOp::AppendSlices { .. } => None,
        };
        if let Some((erp, name)) = written {
            self.writers
                .insert((erp.clone(), name.to_owned()), origin.to_owned());
            if !self.origins.iter().any(|o| o == origin) {
                self.origins.push(origin.to_owned());
            }
        }
        Ok(())
    }

    /// Whether the game's original of `erp` (the baseline's copy, else the input) already holds an entry one
    /// of `edits` adds.
    fn original_holds(&self, erp: &str, resource: &str, fragment: usize, edits: &[PkgEdit]) -> bool {
        let Ok(rel) = checked(erp) else {
            return false;
        };
        let path = self.baseline.unwrap_or(self.original)(rel);
        let target = PkgTarget {
            erp: erp.to_owned(),
            resource: resource.to_owned(),
            fragment,
        };
        read_resource(&path, &target).is_ok_and(|root| edits.iter().any(|e| holds_added(&root, e)))
    }

    pub fn apply(&mut self, op: &ErpOp, resolve: &dyn Fn(&Source) -> Result<PathBuf>) -> Result<()> {
        let packer = self.packer.clone();
        if let Some(name) = unsupported(op) {
            return Err(fail(format!("{name} is not built yet")));
        }
        match op {
            ErpOp::PatchPkg {
                erp,
                resource,
                fragment,
                edits,
            } => {
                if edits.is_empty() {
                    return Ok(());
                }
                for t in sources(edits) {
                    if !self.sources.contains_key(&key(t)) {
                        let node = read_resource(&(self.original)(checked(&t.erp)?), t)?;
                        self.sources.insert(key(t), node);
                    }
                }
                let srcs = std::mem::take(&mut self.sources);
                let result = self.patch(erp, resource, *fragment, |root| {
                    apply_edits(root, edits, &|t| srcs.get(&key(t)))
                });
                self.sources = srcs;
                result
            }
            ErpOp::CopyResource {
                from_erp,
                source,
                erp,
                resource,
                rename,
            } => {
                let path = match (from_erp, source) {
                    (Some(rel), None) => (self.original)(checked(rel)?),
                    (None, Some(src)) => resolve(src)?,
                    _ => {
                        return Err(fail(format!(
                            "copy-resource {resource}: give exactly one of from_erp and source"
                        )));
                    }
                };
                self.copy(&path, resource, erp, rename.as_deref().unwrap_or(resource), false)
            }
            ErpOp::PutResource {
                erp,
                resource,
                source,
                rename,
            } => {
                let path = resolve(source)?;
                self.copy(&path, resource, erp, rename.as_deref().unwrap_or(resource), true)
            }
            ErpOp::SetMaterial {
                erp,
                resource,
                params,
            } => {
                let e = self.erp(erp)?;
                let i = e.find(erp, resource)?;
                let res = e.owned(i)?;
                let mut body = res
                    .fragments
                    .first()
                    .ok_or_else(|| fail(format!("{resource}: no fragment 0")))?
                    .data()?;
                for (name, value) in params {
                    let values = value
                        .values()
                        .map_err(|err| fail(format!("{erp}: {resource}: {name}: {err}")))?;
                    swofty_formats::material::set(&mut body, name, &values)
                        .map_err(|err| fail(format!("{erp}: {resource}: {err}")))?;
                }
                pack::set_fragment(&packer, res, 0, &body).map_err(|err| fail(format!("{erp}: {err}")))
            }
            ErpOp::ReplaceResource {
                erp,
                resource,
                fragment,
                source,
            } => {
                let data = read(&resolve(source)?)?;
                let e = self.erp(erp)?;
                let i = e.find(erp, resource)?;
                let res = e.owned(i)?;
                if res.kind() == SCENE && *fragment == 0 {
                    check_scene(resource, &data).map_err(|err| fail(format!("{erp}: {resource}: {err}")))?;
                }
                pack::set_fragment(&packer, res, *fragment, &data)
                    .map_err(|err| fail(format!("{erp}: {err}")))
            }
            ErpOp::AddResource {
                erp,
                resource,
                kind,
                sources,
            } => {
                let data = sources
                    .iter()
                    .map(|s| read(&resolve(s)?))
                    .collect::<Result<Vec<_>>>()?;
                self.add(erp, resource, kind, &data)
            }
            ErpOp::RemoveResource { erp, resource } => {
                match self
                    .erp(erp)?
                    .position(resource)
                    .map_err(|err| fail(format!("{erp}: {err}")))?
                {
                    Some(i) => {
                        self.erp(erp)?.slots.remove(i);
                    }
                    None => self.notes.push(format!("{erp}: {resource} already removed")),
                }
                Ok(())
            }
            ErpOp::MergeSlices {
                erp,
                resource,
                source,
            } => {
                let path = resolve(source)?;
                let merge = self.merge_slices(erp, resource, &path)?;
                self.merges.push(merge);
                Ok(())
            }
            ErpOp::AppendSlices {
                erp,
                resource,
                source,
            } => {
                let path = resolve(source)?;
                self.append_slices(erp, resource, &path)
            }
            ErpOp::Overlay { erps, source } => {
                let path = resolve(source)?;
                for name in overlay_names(&path)? {
                    for erp in erps {
                        self.copy(&path, &name, erp, &name, true)?;
                    }
                }
                Ok(())
            }
            ErpOp::AddMipmaps { source, target } => {
                let from = resolve(source)?;
                let target = checked(target.trim_end_matches('/'))?;
                if from.is_file() {
                    self.mipmaps.insert(target.to_owned(), from);
                    return Ok(());
                }
                if !from.is_dir() {
                    return Err(fail(format!("add-mipmaps: {} is missing", source.path)));
                }
                let files = loader_files(&from)?;
                if files.is_empty() {
                    return Err(fail(format!("add-mipmaps: {} has no files", source.path)));
                }
                for (rel, path) in files {
                    self.mipmaps.insert(mipmap_path(target, &rel), path);
                }
                Ok(())
            }
        }
    }
}
