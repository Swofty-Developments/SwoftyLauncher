use swofty_formats::erp::{ErpFile, Resource};

use super::{ErpBuild, SCENE, Slot, fail};
use crate::Result;
use crate::activation::wheel_check;

pub(super) fn is_handling(r: &Resource) -> bool {
    r.name.to_ascii_lowercase().ends_with(".vtf")
}

pub fn texture_findings_in(file: &ErpFile) -> Result<Vec<crate::content::texture_check::Finding>> {
    use crate::content::texture_check::{check_surface, view_name};
    let mut out = Vec::new();
    for r in file.erp.resources.iter().filter(|r| r.kind() == "GfxSurfaceRes") {
        let view = match view_name(&r.name) {
            Some(v) => file
                .erp
                .resources
                .iter()
                .find(|x| x.name == v)
                .map(|x| file.load(x))
                .transpose()?,
            None => None,
        };
        out.extend(check_surface(&file.load(r)?, view.as_ref()));
    }
    Ok(out)
}

impl ErpBuild<'_> {
    /// The warnings for the handling files addons wrote that move a stock car's wheels: its render positions,
    /// visual height offsets or tyre sizes, beyond [`wheel_check::TOLERANCE`].
    pub fn wheel_warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (rel, a) in &self.erps {
            for r in a.written().filter(|r| is_handling(r)) {
                let Some(stock) = a.file.erp.resources.iter().find(|s| s.name == r.name) else {
                    continue;
                };
                let moved = (|| -> Result<Vec<wheel_check::Change>> {
                    let before = a.file.load(stock)?.fragments[0].data()?;
                    wheel_check::changes_in_data(&before, &r.fragments[0].data()?)
                })()
                .unwrap_or_default();
                let by = self
                    .writers
                    .get(&(rel.clone(), r.name.clone()))
                    .map_or("an addon", String::as_str);
                let car = r.name.rsplit('/').next().unwrap_or(&r.name);
                out.extend(wheel_check::warning(by, car, &moved));
            }
        }
        out
    }

    /// Checks the textures an addon wrote or appended to against what the
    /// game's D3D12 renderer accepts. Each finding names the addon that last
    /// wrote the resource, as `name` gives it, and the archive.
    pub fn check_textures(
        &self,
        name: &dyn Fn(&str) -> String,
    ) -> Vec<(crate::content::texture_check::Severity, String)> {
        use crate::content::texture_check::{check_surface, view_name};
        let mut out = Vec::new();
        for (rel, a) in &self.erps {
            for r in a.written() {
                let view = view_name(&r.name).and_then(|v| a.written().find(|v2| v2.name == v));
                let key = (rel.clone(), r.name.clone());
                let by = self
                    .writers
                    .get(&key)
                    .or_else(|| self.appended.get(&key))
                    .map_or_else(|| "an addon".to_owned(), |o| name(o));
                for f in check_surface(r, view) {
                    out.push((f.severity, format!("{by}: {rel}: {f}")));
                }
            }
        }
        out
    }

    /// Checks the archives with UI resources that ops wrote against the rules
    /// the game's UI loader enforces. Each finding starts with the origin that
    /// last wrote its resource, as `name` gives it. A name that no longer
    /// matches its stored hash is a warning: the game finds the node, event or
    /// signal by the stored hash, as it did before the rename.
    pub fn check_ui(&self, name: &dyn Fn(&str) -> String) -> Result<Vec<String>> {
        use crate::ui::check::{ArchiveUi, Finding, STALE_HASH};
        let ui = |r: &Resource| matches!(r.kind().as_str(), "UIDF" | SCENE) && !r.name.contains('?');
        let frags = |r: &Resource| {
            r.fragments
                .iter()
                .map(|f| f.data())
                .collect::<swofty_formats::Result<Vec<_>>>()
        };
        let (mut edited, mut stock) = (Vec::new(), Vec::new());
        for (rel, a) in &self.erps {
            if !a.written().any(ui) {
                continue;
            }
            let mut was = ArchiveUi {
                archive: rel.clone(),
                ..Default::default()
            };
            for r in a.file.erp.resources.iter().filter(|r| ui(r)) {
                let loaded = a.file.load(r)?;
                was.resources.insert(r.name.clone(), (r.kind(), frags(&loaded)?));
            }
            let mut now = ArchiveUi {
                archive: rel.clone(),
                ..Default::default()
            };
            for s in &a.slots {
                let r = a.entry(s);
                if !ui(r) {
                    continue;
                }
                let entry = match (s, was.resources.get(&r.name)) {
                    (Slot::Stock(_), Some(stock)) => stock.clone(),
                    _ => (r.kind(), frags(r)?),
                };
                now.resources.insert(r.name.clone(), entry);
            }
            edited.push(now);
            stock.push(was);
        }
        if edited.is_empty() {
            return Ok(Vec::new());
        }
        let report = crate::diagnostics::phases::time_part("build.check.ui.report", || {
            crate::ui::check::archives(
                &edited,
                &stock,
                crate::ui::catalog::stock(),
                crate::ui::catalog::engine(),
            )
        })
        .map_err(|e| fail(format!("checking UI edits: {e}")))?;
        let by = |f: &Finding| {
            self.writers
                .iter()
                .find(|((erp, res), _)| {
                    f.at.strip_prefix(erp.as_str())
                        .and_then(|r| r.strip_prefix(": "))
                        .and_then(|r| r.strip_prefix(res.as_str()))
                        .is_some_and(|r| r.starts_with(": "))
                })
                .map_or_else(|| "the game's own copy".to_owned(), |(_, origin)| name(origin))
        };
        let (renamed, hard): (Vec<Finding>, Vec<Finding>) =
            report.hard.into_iter().partition(|f| f.code == STALE_HASH);
        if !hard.is_empty() {
            let lines: Vec<String> = hard.iter().map(|f| format!("{}: {f}", by(f))).collect();
            return Err(fail(format!(
                "{} UI edit(s) break rules the game's UI loader enforces:\n  {}",
                lines.len(),
                lines.join("\n  ")
            )));
        }
        Ok(renamed
            .iter()
            .map(|f| {
                format!(
                    "{}: {}: {}: the name no longer matches its stored hash, so the game still finds it by the hash, as before the rename",
                    by(f),
                    f.at,
                    f.message
                )
            })
            .chain(report.warnings.iter().map(|f| format!("{}: {f}", by(f))))
            .collect())
    }
}
