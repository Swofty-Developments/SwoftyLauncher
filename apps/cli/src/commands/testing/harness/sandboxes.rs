use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use swofty_core::addons::storage as addon_storage;
use swofty_core::testsuite::bench;
use swofty_core::testsuite::fixture::Fixture;
use swofty_core::testsuite::plan::Batch;
use swofty_core::testsuite::report::{BatchReport, Status, TestReport};
use swofty_core::testsuite::sandbox::{self, RunDir};
use swofty_core::testsuite::suite::Kind;

use crate::commands::testing::harness::Harness;
use crate::commands::testing::say;

/// A batch's save sandbox: the fixture copied into the run folder's saves
/// and the player's save folder listing before the game started.
pub(crate) struct Sandbox {
    pub(crate) fixture: String,
    /// The fixture's careers' game mode, when it tells which to continue.
    pub(crate) mode: Option<String>,
    pub(crate) storage_ids: BTreeSet<String>,
    pub(crate) live_before: Option<BTreeMap<String, (u64, Option<SystemTime>)>>,
}

/// What `close_sandbox` marks: the batch's report, its tests' reports and
/// which of those tests ran.
pub(crate) struct BatchResults<'r> {
    pub(crate) br: &'r mut BatchReport,
    pub(crate) reports: &'r mut [TestReport],
    pub(crate) runnable: &'r [usize],
}

/// The benchmark XML staged in the game folder for one batch, and removed
/// when the batch ends.
pub(crate) struct Staged {
    pub(crate) path: PathBuf,
    pub(crate) made_dir: Option<PathBuf>,
}

impl Staged {
    pub(crate) fn write(game: &Path, b: &Batch) -> std::io::Result<Option<Staged>> {
        if b.env.kind != Kind::Benchmark {
            return Ok(None);
        }
        let dir = game.join(bench::DIR);
        let made_dir = match dir.is_dir() {
            true => None,
            false => {
                std::fs::create_dir_all(&dir)?;
                Some(dir.clone())
            }
        };
        let path = dir.join(bench::FILE);
        std::fs::write(&path, bench::xml(&b.env))?;
        Ok(Some(Staged { path, made_dir }))
    }

    pub(crate) fn remove(&mut self) -> std::io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        if let Some(d) = self.made_dir.take() {
            std::fs::remove_dir(d).ok();
        }
        Ok(())
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        self.remove().ok();
    }
}

impl Harness<'_> {
    /// The fixture's saves in the run folder's sandbox and the fixture's
    /// addon storage as the batch profile's copy, which the session loads
    /// into the addons' storage when it starts.
    pub(crate) fn open_sandbox(
        &self,
        f: Fixture,
        b: &Batch,
        profile: &str,
        run: &RunDir,
    ) -> std::result::Result<Sandbox, String> {
        let data = &self.store.root;
        let ids: BTreeSet<String> = f
            .storage_ids()
            .into_iter()
            .chain(b.addons.iter().cloned())
            .collect();
        for id in &ids {
            let d = addon_storage::profile_dir(data, id, profile);
            if d.exists() {
                std::fs::remove_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
        }
        let storage = |id: &str| addon_storage::profile_dir(data, id, profile);
        let copied = match sandbox::ea_copy(self.game) {
            true => f.install_ea(&run.ea_title(), &storage)?,
            false => f.install(&run.remote(), &storage)?,
        };
        let installed = self.store.addons().map_err(|e| e.to_string())?;
        let addons: BTreeMap<String, String> = b
            .addons
            .iter()
            .filter_map(|id| Some((id.clone(), installed.get(id)?.version.to_string())))
            .collect();
        std::fs::write(
            run.root.join("addons.json"),
            serde_json::to_vec_pretty(&addons).expect("serialisable"),
        )
        .map_err(|e| format!("{}: {e}", run.root.display()))?;
        for id in f.storage_ids().iter().filter(|id| !b.addons.contains(id)) {
            say(&format!(
                "fixture {}: it carries storage for {id}, which the batch does not enable; the game won't see it",
                f.manifest.name
            ));
        }
        say(&format!(
            "save sandbox {}: fixture {} ({} file(s)) copied in",
            run.saves().display(),
            f.manifest.name,
            copied.len()
        ));
        Ok(Sandbox {
            fixture: f.manifest.name.clone(),
            mode: career_mode(&f, b.env.uint("slot")),
            storage_ids: ids,
            live_before: self.live_saves.as_deref().map(sandbox::listing),
        })
    }

    /// After the batch: keep its addon storage in the run folder for a
    /// capture, check the core armed the sandbox and the player's save folder
    /// did not change, and retire the run token.
    pub(crate) fn close_sandbox(
        &self,
        s: Sandbox,
        run: Option<&RunDir>,
        b: &Batch,
        profile: &str,
        results: BatchResults<'_>,
    ) {
        let BatchResults {
            br,
            reports,
            runnable,
        } = results;
        let data = &self.store.root;
        let Some(run) = run else { return };
        for id in &s.storage_ids {
            let from = addon_storage::profile_dir(data, id, profile);
            let to = run.storage().join(id);
            if from.is_dir() && copy_tree(&from, &to).is_err() {
                br.notes
                    .push(format!("could not keep {id}'s storage in {}", to.display()));
            }
        }
        let status = run.status();
        let mut why = match &status {
            None if br.started_game => Some(
                "the core never reported the save sandbox (no sandbox.json): the save device may not have started"
                    .to_owned(),
            ),
            Some(st) if !st.armed() => Some(format!("the save sandbox did not arm: {}", st.why)),
            _ => None,
        };
        if let (Some(before), Some(dir)) = (&s.live_before, &self.live_saves) {
            let changed = sandbox::changed(before, &sandbox::listing(dir));
            if !changed.is_empty() {
                why = Some(format!(
                    "THE PLAYER'S {} SAVE FOLDER CHANGED during a sandboxed run ({}): {}",
                    if sandbox::ea_copy(self.game) {
                        "EA"
                    } else {
                        "STEAM"
                    },
                    dir.display(),
                    changed.join(", ")
                ));
            }
        }
        if let Some(why) = why {
            say(&format!("{}: {why}", b.label()));
            br.error = Some(why.clone());
            for &i in runnable {
                reports[i].status = Status::Error;
                reports[i].reason = Some(why.clone());
            }
        }
    }

    pub(crate) fn drop_profile(&self, sandboxed: bool, profile: &str, br: &mut BatchReport) {
        if sandboxed
            && self
                .store
                .profile_names()
                .is_ok_and(|n| n.contains(&profile.to_owned()))
            && let Err(e) = self.store.delete_profile(profile)
        {
            br.notes
                .push(format!("could not delete the batch profile {profile}: {e}"));
        }
    }
}

/// The game mode of the career a career environment continues, from the
/// fixture's `holds`: the career in the test's slot, else the only one.
pub(crate) fn career_mode(f: &Fixture, slot: Option<u64>) -> Option<String> {
    let careers = &f.manifest.holds.careers;
    let c = match slot {
        Some(n) => careers.iter().find(|c| c.slot == Some(n)),
        None if careers.len() == 1 => careers.first(),
        None => None,
    }?;
    let m = c.mode.to_ascii_lowercase();
    if m.contains("my_team") || m.contains("myteam") {
        Some("my_team_career".into())
    } else if m.contains("driver") {
        Some("driver_career".into())
    } else {
        None
    }
}

pub(crate) fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type()?.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
            let modified = e.metadata()?.modified()?;
            std::fs::File::options()
                .write(true)
                .open(&dst)?
                .set_modified(modified)?;
        }
    }
    Ok(())
}
