//! A file for each Addon Browser install or update while it runs. Each ends with an `ok` or
//! `failed` line in the registry log: from the launcher when the job finishes, or from the
//! next start when the launcher closed first.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::manifest::AddonManifest;
use crate::store::Store;

pub const DIR: &str = "state/registry-jobs";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Mark {
    id: String,
    version: semver::Version,
    from: Option<semver::Version>,
    started: String,
}

impl Mark {
    /// The registry log's last line for a job that ended without one.
    fn line(&self, now: Option<&semver::Version>) -> String {
        let Mark {
            id,
            version,
            from,
            started,
        } = self;
        let (done, failed) = match from {
            Some(_) => ("Updated", "Couldn't update"),
            None => ("Installed", "Couldn't install"),
        };
        if now == Some(version) {
            return format!(
                "ok {done} {id} {version}: it finished, but the launcher closed before it said so (started {started})"
            );
        }
        let kept = match now {
            Some(v) => format!("{id} stays at {v}"),
            None => format!("{id} is not installed"),
        };
        format!(
            "failed {failed} {id} {version}: the launcher closed before it finished (started {started}); {kept}"
        )
    }
}

/// Held while an install runs; [`Pending::end`] once its result is logged. Dropped without
/// that, as when the launcher quits part way, the job's file stays for the next start's
/// [`settle`].
#[derive(Debug)]
pub struct Pending {
    path: PathBuf,
    file: Option<File>,
}

impl Pending {
    pub fn begin(store: &Store, id: &str, version: &semver::Version, from: Option<&semver::Version>) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = store.root.join(DIR);
        let path = dir.join(format!("{id}-{}-{n}.json", std::process::id()));
        let mark = Mark {
            id: id.to_owned(),
            version: version.clone(),
            from: from.cloned(),
            started: super::iso(super::now_secs()),
        };
        let file = std::fs::create_dir_all(&dir)
            .and_then(|()| {
                OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&path)
            })
            .and_then(|mut f| {
                f.try_lock().ok();
                f.write_all(&serde_json::to_vec(&mark).expect("serialisable"))?;
                f.sync_all()?;
                Ok(f)
            })
            .ok();
        Self { path, file }
    }

    pub fn end(self) {
        if self.file.is_some() {
            std::fs::remove_file(&self.path).ok();
        }
    }
}

fn installed_version(store: &Store, id: &str) -> Option<semver::Version> {
    AddonManifest::load(&store.addons_dir().join(id))
        .ok()
        .map(|m| m.version)
}

/// Logs the end of each install an earlier run left without one, and deletes its file.
/// The files of installs still running in another launcher are left alone.
pub fn settle(store: &Store) -> Vec<String> {
    let dir = store.root.join(DIR);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    let mut notes = Vec::new();
    for path in files {
        let Some(mark) = claim(&path) else {
            continue;
        };
        if let Some(mark) = mark {
            let line = mark.line(installed_version(store, &mark.id).as_ref());
            super::log(&store.root, &line);
            notes.push(line);
        }
        std::fs::remove_file(&path).ok();
    }
    notes
}

/// The job in `path` once no running launcher holds it: `None` while one does, `Some(None)`
/// when the file is unreadable.
fn claim(path: &Path) -> Option<Option<Mark>> {
    let mut file = OpenOptions::new().read(true).write(true).open(path).ok()?;
    if let Err(std::fs::TryLockError::WouldBlock) = file.try_lock() {
        return None;
    }
    let mut text = Vec::new();
    file.read_to_end(&mut text).ok()?;
    Some(serde_json::from_slice(&text).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> (PathBuf, Store) {
        let root = std::env::temp_dir().join(format!("swofty-pending-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(&root).unwrap();
        (root.clone(), Store::new(root))
    }

    fn install(store: &Store, id: &str, version: &str) {
        let dir = store.addons_dir().join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("swofty.json"),
            format!(r#"{{"schema_version":1,"id":"{id}","name":"Real","version":"{version}","author":"a"}}"#),
        )
        .unwrap();
    }

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    fn log(root: &Path) -> String {
        std::fs::read_to_string(root.join(super::super::LOG_FILE)).unwrap_or_default()
    }

    fn jobs(root: &Path) -> usize {
        std::fs::read_dir(root.join(DIR)).map_or(0, |rd| rd.count())
    }

    #[test]
    fn an_ended_job_leaves_no_file_and_no_line() {
        let (root, store) = tmp("ended");
        let job = Pending::begin(&store, "a.real", &v("1.1.7"), Some(&v("1.1.6")));
        assert_eq!(jobs(&root), 1);
        job.end();
        assert_eq!(jobs(&root), 0);
        assert_eq!(log(&root), "");
        assert!(settle(&store).is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_job_a_closed_launcher_left_gets_its_failed_line_on_the_next_start() {
        let (root, store) = tmp("left");
        install(&store, "a.real", "1.1.6");
        let job = Pending::begin(&store, "a.real", &v("1.1.7"), Some(&v("1.1.6")));
        let live = settle(&store);
        assert!(live.is_empty(), "{live:?}");
        assert_eq!(jobs(&root), 1);
        drop(job);
        let notes = settle(&store);
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].starts_with(
                "failed Couldn't update a.real 1.1.7: the launcher closed before it finished (started "
            ),
            "{}",
            notes[0]
        );
        assert!(notes[0].ends_with("; a.real stays at 1.1.6"), "{}", notes[0]);
        assert!(log(&root).contains(&notes[0]));
        assert_eq!(jobs(&root), 0);
        assert!(settle(&store).is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_job_that_finished_before_the_launcher_closed_gets_an_ok_line() {
        let (root, store) = tmp("finished");
        let job = Pending::begin(&store, "a.real", &v("1.1.7"), Some(&v("1.1.6")));
        install(&store, "a.real", "1.1.7");
        drop(job);
        let notes = settle(&store);
        assert!(
            notes[0].starts_with("ok Updated a.real 1.1.7: it finished, but the launcher closed"),
            "{}",
            notes[0]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_dropped_new_install_is_settled_on_the_next_start() {
        let (root, store) = tmp("dropped");
        drop(Pending::begin(&store, "a.new", &v("0.1.0"), None));
        assert_eq!(jobs(&root), 1);
        assert_eq!(log(&root), "");
        let notes = settle(&store);
        assert!(
            notes[0]
                .starts_with("failed Couldn't install a.new 0.1.0: the launcher closed before it finished"),
            "{}",
            notes[0]
        );
        assert!(notes[0].ends_with("; a.new is not installed"), "{}", notes[0]);
        assert_eq!(jobs(&root), 0);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_unreadable_job_file_is_deleted_without_a_line() {
        let (root, store) = tmp("unreadable");
        std::fs::create_dir_all(root.join(DIR)).unwrap();
        std::fs::write(root.join(DIR).join("a.real-1-0.json"), b"").unwrap();
        assert!(settle(&store).is_empty());
        assert_eq!(jobs(&root), 0);
        assert_eq!(log(&root), "");
        std::fs::remove_dir_all(&root).ok();
    }
}
