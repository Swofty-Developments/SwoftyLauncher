//! Restoring the game folder to its baseline, setting the launcher's proxy aside, and verifying the folder
//! against the baseline.

use std::path::{Path, PathBuf};

use super::files::shipped_proxy;
use super::wine_override::{has_override, remove_override};
use super::{Drift, Installer, LOADER_DIR, PROXY};
use crate::game_folder::baseline::{Baseline, Hashes, sha256_opt};
use crate::game_folder::journal::Journal;
use crate::game_folder::placed::Placed;
use crate::{Result, addons::storage as addon_storage, io_err, with_retry};

impl<'a> Installer<'a> {
    pub fn restore(&self) -> Result<Vec<Drift>> {
        self.restore_setting_aside().map(|(report, _)| report)
    }

    /// [`restore`](Self::restore), and where in the set-aside folder it moved
    /// a `version.dll` that is not the launcher's own: another tool's proxy,
    /// such as an ASI loader or a renamed upscaler, is kept, never deleted.
    pub fn restore_setting_aside(&self) -> Result<(Vec<Drift>, Option<PathBuf>)> {
        let mut report = self.recover()?;
        self.sweep()?;
        let proxy = self.path(PROXY);
        let baseline = self.baseline()?;
        let mut set_aside = None;
        if proxy.is_file() && !baseline.files.contains_key(PROXY) {
            if shipped_proxy(&proxy)? {
                with_retry(&proxy, || std::fs::remove_file(&proxy))?;
            } else {
                set_aside = Some(self.set_proxy_aside(&proxy)?);
            }
            report.retain(|d| !matches!(d, Drift::LeftInPlace(p) | Drift::Stranded(p) if p == PROXY));
        }
        let loader = self.path(LOADER_DIR);
        if loader.is_dir() {
            crate::stdlib::careers::keep(self.data, &self.game.dir)?;
            let ids = addon_storage::present(&self.game.dir)?;
            self.keep_storage(&ids, &Journal::open(self.data)?)?;
            let stranded = addon_storage::addon_dir(self.data, addon_storage::LOGO_CACHE);
            if stranded.is_dir() {
                with_retry(&stranded, || std::fs::remove_dir_all(&stranded))?;
            }
            let logs = self.data.join("sessions/orphaned-logs");
            for e in std::fs::read_dir(&loader).map_err(io_err(&loader))?.flatten() {
                if e.path().extension().is_some_and(|x| x == "log") {
                    std::fs::create_dir_all(&logs).map_err(io_err(&logs))?;
                    crate::move_file(&e.path(), &logs.join(e.file_name()))?;
                }
            }
            addon_storage::clear_tree(&loader)?;
        }
        if let Some(reg) = &self.game.user_reg
            && let Ok(text) = std::fs::read_to_string(reg)
            && let Some(out) = remove_override(&text)
        {
            self.objects().put_file(reg)?;
            std::fs::write(reg, out).map_err(io_err(reg))?;
        }
        let mut hashes = Hashes::load(self.data);
        for (rel, rec) in &baseline.files {
            let path = self.path(rel);
            if hashes.sha256_opt(&path)?.as_deref() != Some(rec.sha256.as_str()) {
                self.objects().restore_to(&rec.sha256, &path)?;
                hashes.wrote(&path, &rec.sha256);
            }
        }
        hashes.save();
        report.extend(self.verify()?);
        Ok((report, set_aside))
    }

    /// Move `proxy` to a folder of its own under the set-aside folder.
    pub(super) fn set_proxy_aside(&self, proxy: &Path) -> Result<PathBuf> {
        let dir = crate::game_folder::recovery::set_aside_dir(self.data);
        let mut stamp = crate::updates::release::now_secs();
        while dir.join(stamp.to_string()).join(PROXY).exists() {
            stamp += 1;
        }
        let to = dir.join(stamp.to_string()).join(PROXY);
        let parent = to.parent().expect("has parent");
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
        crate::move_file(proxy, &to)?;
        Ok(to)
    }

    pub fn verify(&self) -> Result<Vec<Drift>> {
        let mut out: Vec<Drift> = Journal::open(self.data)?
            .into_iter()
            .map(|j| Drift::OpenJournal(j.id))
            .collect();
        let baseline = self.baseline()?;
        for (rel, rec) in &baseline.files {
            match sha256_opt(&self.path(rel))? {
                None => out.push(Drift::Missing(rel.clone())),
                Some(s) if s != rec.sha256 => out.push(Drift::Changed(rel.clone())),
                Some(_) => {}
            }
        }
        if self.path(PROXY).exists() && !baseline.files.contains_key(PROXY) {
            out.push(Drift::Addition(PROXY.into()));
        }
        if self.path(LOADER_DIR).exists()
            && !baseline.files.contains_key(LOADER_DIR)
            && let Some(rel) = self.added_file(&baseline, LOADER_DIR)
        {
            out.push(Drift::Addition(rel));
        }
        for f in Placed::load(self.data)?.files.values() {
            if f.replaced
                && !baseline.files.contains_key(&f.path)
                && sha256_opt(&self.path(&f.path))?.as_deref() == Some(f.sha256.as_str())
            {
                out.push(Drift::NoOriginal(f.path.clone()));
            }
        }
        if let Some(reg) = &self.game.user_reg
            && std::fs::read_to_string(reg).is_ok_and(|t| has_override(&t))
        {
            out.push(Drift::RegistryOverride);
        }
        let latest = std::fs::read_dir(self.data.join("baseline"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| crate::read_json::<Baseline>(&e.path().join("baseline.json")).ok())
            .find(|b| {
                b.build_id != self.game.identity.build_id && b.files.keys().any(|k| self.path(k).exists())
            });
        if let Some(old) = latest
            && baseline.files.is_empty()
        {
            out.push(Drift::NewBuild {
                baseline: old.build_id,
                current: self.game.identity.build_id.clone(),
            });
        }
        Ok(out)
    }

    /// The first file under the folder `rel` that the baseline has no original for, or `rel` itself
    /// when it is not a folder that can be listed. A folder that holds only empty folders has none.
    pub(super) fn added_file(&self, baseline: &Baseline, rel: &str) -> Option<String> {
        let Ok(rd) = std::fs::read_dir(self.path(rel)) else {
            return Some(rel.to_owned());
        };
        let mut entries: Vec<std::fs::DirEntry> = rd.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        entries.into_iter().find_map(|e| {
            let child = format!("{rel}/{}", e.file_name().to_string_lossy());
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                self.added_file(baseline, &child)
            } else {
                (!baseline.files.contains_key(&child)).then_some(child)
            }
        })
    }
}
