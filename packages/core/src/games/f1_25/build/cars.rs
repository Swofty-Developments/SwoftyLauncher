use std::collections::BTreeMap;
use std::path::PathBuf;

use super::Pipeline;
use super::inputs::Inputs;
use crate::activation::ers::{self, ErsMap, build_ers};
use crate::activation::handling;
use crate::{Error, Result};

impl Pipeline<'_, '_> {
    /// Gives the stock car archives the handling records' changes and the tracks' ERS settings; the
    /// result is the tracks' ERS settings, which car archives addons change get later.
    pub(super) fn car_archives<'i>(&mut self, inputs: &'i Inputs) -> Result<Vec<ErsMap<'i>>> {
        let packer = self.run.eager_packer();
        let (run, built, caches) = (self.run, &mut self.built, &mut self.caches);
        let (store, game) = (run.store, run.game);
        let (fitted, handling_maps) = (&inputs.tracks, &inputs.handling_maps);
        let resolve = |rel: &str| run.original(rel);
        let ers_maps: Vec<ErsMap> = fitted
            .kept
            .iter()
            .filter(|(_, pkg, _)| !pkg.ers.is_empty())
            .map(|(_, pkg, _)| ErsMap {
                code: &pkg.track_code,
                base: pkg.activation.base_track().unwrap_or(&pkg.track_code),
                values: &pkg.ers,
            })
            .collect();
        let mut handled: BTreeMap<String, PathBuf> = BTreeMap::new();
        if handling_maps
            .iter()
            .any(|m| matches!(m.target, handling::Target::Stock(_)))
        {
            let hb = handling::build_stock(
                handling_maps,
                &resolve,
                &store.root.join("cache/handling"),
                &packer,
            )?;
            caches.extend(hb.dir.clone());
            for line in hb.lines {
                match line.strip_prefix(crate::activation::wheel_check::WARNING_PREFIX) {
                    Some(w) => built.warnings.push(w.to_owned()),
                    None => built.lines.push(line),
                }
            }
            handled = hb.replaced;
        }
        let current = |rel: &str| handled.get(rel).cloned().unwrap_or_else(|| resolve(rel));
        let ers_cars = |rel: &str| !ers_maps.is_empty() && ers::is_car_archive(rel);
        for (rel, src) in &handled {
            if !ers_cars(rel) && built.plan.data.insert(rel.clone(), src.clone()).is_some() {
                return Err(Error::Map(format!("{rel} is replaced twice")));
            }
        }
        if !ers_maps.is_empty() {
            let ers = build_ers(
                &ers_maps,
                &game.dir,
                &current,
                &store.root.join("cache/ers"),
                &packer,
            )?;
            for (rel, src) in &handled {
                if ers_cars(rel) && !ers.replaced.contains_key(rel) {
                    built.plan.data.insert(rel.clone(), src.clone());
                }
            }
            caches.extend(ers.dir.clone());
            built.lines.push(format!(
                "ers: {} handling file(s) in {} car archive(s) get the tracks' settings",
                ers.handling_files,
                ers.replaced.len()
            ));
            built
                .warnings
                .extend(ers.notes.iter().map(|n| format!("ers: {n}")));
            for (rel, src) in ers.replaced {
                built.replace(rel, src)?;
            }
        }
        Ok(ers_maps)
    }
}
