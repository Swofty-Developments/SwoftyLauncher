use std::collections::BTreeMap;

pub use crate::asset::Kind;
use crate::asset::{AssetOp, FileKind, Op, OpFile, Origin};
use crate::ops::{DbOps, ErpOp};
use crate::profile::conflicts::Changes;

use super::{Conflict, Planner, Route, Target, Touched, target};

impl<'a> Planner<'a> {
    pub fn push(&mut self, op: Op) {
        self.rank(&op.origin);
        let placed: Vec<String> = match (&op.op, self.mipmaps) {
            (AssetOp::Erp(ErpOp::AddMipmaps { source, target }), Some(names)) => names(&op.origin, source)
                .iter()
                .map(|rel| crate::erp::mipmap_path(target, rel))
                .collect(),
            _ => Vec::new(),
        };
        if placed.is_empty()
            && let Some((target, replaces)) = target(&op.op)
        {
            self.write(target, &op.origin, replaces);
        }
        for path in placed {
            self.write(Target::File(path), &op.origin, true);
        }
        if let (AssetOp::Erp(ErpOp::Overlay { erps, source }), Some(names)) = (&op.op, self.overlays) {
            for uri in names(&op.origin, source) {
                for erp in erps {
                    let target = Target::Resource {
                        erp: erp.clone(),
                        uri: uri.clone(),
                    };
                    self.write(target, &op.origin, true);
                }
            }
        }
        if matches!(op.op, AssetOp::Erp(ErpOp::RemoveResource { .. })) {
            self.removals += 1;
        }
        if matches!(op.op, AssetOp::Track(_)) {
            self.tracks.push(op);
            return;
        }
        if matches!(op.op, AssetOp::Item(_)) {
            self.items.push(op);
            return;
        }
        let Some(kind) = op.op.kind() else {
            self.erp.push(op);
            return;
        };
        match self.route(kind) {
            Route::Runtime if !matches!(kind, Kind::Db | Kind::Erp) => self.planned.runtime.push(op),
            Route::Waiting => self.planned.waiting.push(op),
            Route::Build | Route::Runtime => match op.op {
                AssetOp::Db(db) => match self.db.last_mut() {
                    Some((origin, ops)) if *origin == op.origin => ops.ops.push(db),
                    _ => self.db.push((
                        op.origin,
                        DbOps {
                            schema_version: 1,
                            fields: BTreeMap::new(),
                            strict: false,
                            side_data: Default::default(),
                            ops: vec![db],
                        },
                    )),
                },
                AssetOp::Erp(_) => self.erp.push(op),
                _ => self.files.push(op),
            },
        }
    }

    pub fn push_file(&mut self, origin: &Origin, kind: FileKind, file: OpFile) {
        let db = file.ops.iter().any(|op| matches!(op, AssetOp::Db(_)));
        if kind == FileKind::Database || db {
            let ops = DbOps {
                schema_version: file.schema_version,
                fields: file.fields,
                strict: file.strict,
                side_data: file.side_data,
                ops: Vec::new(),
            };
            self.db.push((origin.clone(), ops));
        }
        for op in file.ops {
            self.push(Op {
                op,
                origin: origin.clone(),
            });
        }
    }

    pub fn add_fields(&mut self, addon: &str, fields: BTreeMap<String, Vec<String>>) {
        if !fields.is_empty() {
            self.fields.entry(addon.to_owned()).or_default().extend(fields);
        }
    }

    pub(super) fn rank(&mut self, origin: &Origin) -> usize {
        if self.ranks.last() == Some(origin) {
            return self.ranks.len() - 1;
        }
        match self.ranks.iter().position(|o| o == origin) {
            Some(at) => at,
            None => {
                self.ranks.push(origin.clone());
                self.ranks.len() - 1
            }
        }
    }

    fn overrides(&mut self, target: &Target, winner: &Origin, lost: Vec<Origin>) {
        if lost.is_empty() {
            return;
        }
        let conflicts = &mut self.planned.conflicts;
        match conflicts.iter_mut().find(|c| c.target == *target) {
            Some(c) => {
                let before = std::mem::replace(&mut c.winner, winner.clone());
                for o in std::iter::once(before).chain(lost) {
                    if o != *winner && !c.overridden.contains(&o) {
                        c.overridden.push(o);
                    }
                }
                c.overridden.retain(|o| o != winner);
            }
            None => conflicts.push(Conflict {
                target: target.clone(),
                winner: winner.clone(),
                overridden: lost,
            }),
        }
    }

    // Splits the other writers of `target` into those before `origin` in load
    // order, which it overrides and which leave the list, and the latest after it.
    fn split(
        &mut self,
        target: &Target,
        origin: &Origin,
        keep_earlier: bool,
    ) -> (Vec<Origin>, Option<Origin>) {
        let mine = self.rank(origin);
        let ws = self.writers.get(target).cloned().unwrap_or_default();
        let (mut earlier, mut later, mut kept) = (Vec::new(), None, Vec::new());
        for w in ws {
            let rank = self.rank(&w);
            if w == *origin || rank > mine {
                if rank > mine && later.as_ref().is_none_or(|(r, _)| rank > *r) {
                    later = Some((rank, w.clone()));
                }
                kept.push(w);
            } else {
                if keep_earlier {
                    kept.push(w.clone());
                }
                earlier.push(w);
            }
        }
        if let Some(ws) = self.writers.get_mut(target) {
            *ws = kept;
        }
        (earlier, later.map(|(_, w)| w))
    }

    // A whole-resource write overrides the earlier slice writes into that
    // resource, and a slice write overrides the earlier whole-resource writes on
    // that slice, while a later write of either kind overrides it.
    fn across(&mut self, target: &Target, origin: &Origin) -> Vec<Origin> {
        let others: Vec<Target> = match target {
            Target::Slice { erp, uri, .. } => vec![Target::Resource {
                erp: erp.clone(),
                uri: uri.clone(),
            }],
            Target::Resource { erp, uri } => {
                let slice = |slice| Target::Slice {
                    erp: erp.clone(),
                    uri: uri.clone(),
                    slice,
                };
                self.writers
                    .range(slice(0)..=slice(usize::MAX))
                    .map(|(t, _)| t.clone())
                    .collect()
            }
            _ => return Vec::new(),
        };
        let mut lost = Vec::new();
        for other in others {
            let whole = matches!(other, Target::Resource { .. });
            let (earlier, later) = self.split(&other, origin, whole);
            if let Some(winner) = later {
                self.overrides(&other, &winner, vec![origin.clone()]);
            }
            lost.extend(earlier);
        }
        lost
    }

    pub(super) fn write(&mut self, target: Target, origin: &Origin, replaces: bool) {
        if !self.touched.contains_key(&origin.addon) {
            self.touched.insert(origin.addon.clone(), Touched::default());
        }
        let touched = self.touched.get_mut(&origin.addon).expect("inserted above");
        match target {
            Target::Record(key) => touched.records.insert(key),
            Target::Group(_) => false,
            _ => touched.other.insert(target.clone()),
        };
        if replaces {
            let (mut lost, later) = self.split(&target, origin, false);
            for w in self.across(&target, origin) {
                if !lost.contains(&w) {
                    lost.push(w);
                }
            }
            match later {
                Some(winner) => {
                    lost.push(origin.clone());
                    self.overrides(&target, &winner, lost);
                }
                None => self.overrides(&target, origin, lost),
            }
        }
        let writers = self.writers.entry(target).or_default();
        if !writers.contains(origin) {
            writers.push(origin.clone());
        }
    }

    /// What each addon writes so far: distinct records, resources, files and strings.
    pub fn changes(&self) -> BTreeMap<String, Changes> {
        self.touched
            .iter()
            .map(|(addon, t)| {
                let mut c = Changes {
                    database: t.records.len(),
                    ..Changes::default()
                };
                for target in &t.other {
                    c.count(target);
                }
                (addon.clone(), c)
            })
            .collect()
    }

    /// Records that `origin` replaced `target` (a merged texture-array slice, say).
    pub fn replaced(&mut self, target: Target, origin: &Origin) {
        self.write(target, origin, true);
    }

    /// Records that the writers of `target` were merged: none of them overrides another there.
    pub fn merged(&mut self, target: &Target) {
        self.planned.conflicts.retain(|c| c.target != *target);
    }

    /// Records that `keeper` keeps `target`, which `lost` replaces later in load order.
    pub fn kept(&mut self, target: Target, keeper: &Origin, lost: &Origin) {
        let writers = self.writers.entry(target.clone()).or_default();
        writers.retain(|w| w != lost);
        if !writers.contains(keeper) {
            writers.push(keeper.clone());
        }
        self.overrides(&target, keeper, vec![lost.clone()]);
    }

    /// Records that `origin` puts the whole archive `erp` with a `file.put`: it
    /// writes every resource other ops write in it, and load order decides.
    pub fn put_archive(&mut self, erp: &str, origin: &Origin) {
        let inside: Vec<Target> = self
            .writers
            .keys()
            .filter(
                |t| matches!(t, Target::Resource { erp: e, .. } | Target::Slice { erp: e, .. } if e == erp),
            )
            .cloned()
            .collect();
        for target in inside {
            self.write(target, origin, true);
        }
    }

    /// The origin that writes any of `targets` last in load order.
    pub fn last_writer(&self, targets: &[Target]) -> Option<&Origin> {
        let rank = |o: &Origin| self.ranks.iter().position(|r| r == o);
        targets
            .iter()
            .filter_map(|t| self.writers.get(t))
            .flatten()
            .max_by_key(|o| rank(o))
    }

    pub fn features(&self) -> Vec<Kind> {
        self.features
            .experimental
            .intersection(self.opt_in)
            .copied()
            .collect()
    }
}

impl Planner<'_> {
    pub fn pending_lang(&self) -> impl Iterator<Item = &Op> {
        self.planned
            .runtime
            .iter()
            .chain(self.files.iter())
            .filter(|op| matches!(op.op, AssetOp::Lang(_)))
    }
}
