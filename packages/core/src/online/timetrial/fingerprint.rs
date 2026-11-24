//! The addon set a lap was recorded with, in the shape the Time Trial boards server reads, and the
//! [`Session`] that works it out once for a game session, before its first lap.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde_json::{Value, json};

use crate::addons::builtin::{CORE_DLL, CORE_ID, loader_dir};
use crate::game_folder::baseline::Identity;
use crate::online::multiplayer::fingerprint::{self as addon_set, Fingerprint, OUTPUTS_FILE, Source};
use crate::store::Store;

pub const CORE: &str = "core";
pub const BUILTIN: &str = "builtin";
pub const REGISTRY: &str = "registry";
pub const LOCAL: &str = "local";

/// `source` is `core` for the launcher's own core, `builtin` for an addon shipped inside the launcher,
/// `registry` (with the SHA-256 of the Addon Browser download as `registry`) for an installed registry
/// copy, and `local` for anything else; the server ranks only laps whose addons are all approved ones.
pub fn wire(fp: &Fingerprint) -> Value {
    let addons: Vec<Value> = fp
        .addons
        .iter()
        .map(|a| {
            let (source, registry) = if a.id == CORE_ID {
                (CORE, None)
            } else if matches!(a.source, Source::Builtin(_)) {
                (BUILTIN, None)
            } else if let Some(sha) = &a.registry {
                (REGISTRY, Some(sha.clone()))
            } else {
                (LOCAL, None)
            };
            let mut doc = json!({"id": a.id, "version": a.version.to_string(), "source": source});
            if let Some(sha) = registry {
                doc["registry"] = sha.into();
            }
            doc
        })
        .collect();
    json!({
        "digest": fp.digest,
        "launcher": fp.launcher,
        "game_build": fp.game_build.exe_sha256,
        "addons": addons,
    })
}

/// How long a lap may wait for the session start's fingerprint before the run log says so.
const TOLD_WAIT: Duration = Duration::from_millis(100);

/// What a [`Session`] works the fingerprint out from.
pub trait Setup: Send + Sync + 'static {
    /// The files that say which addons the profile plays, at which versions and from where, and what its
    /// last build made, with their sizes and times: no file is hashed or parsed.
    fn inputs(&self) -> Inputs;
    /// The fingerprint as it is now, hashing the addon files; `None` when it can't be made.
    fn fingerprint(&self) -> Option<Fingerprint>;
    /// The core's stat ids as they are now: it writes new ones while the game starts.
    fn registry_ids(&self) -> Value;
}

/// A file's size and modification time.
type Stat = (u64, Option<SystemTime>);

/// The sizes and modification times of a [`Setup`]'s inputs, `None` for one that is missing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inputs(Vec<(PathBuf, Option<Stat>)>);

impl Inputs {
    pub fn of(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self(
            paths
                .into_iter()
                .map(|p| {
                    let at = std::fs::metadata(&p).ok().map(|m| (m.len(), m.modified().ok()));
                    (p, at)
                })
                .collect(),
        )
    }
}

/// The profile `profile` of `store` on the game build `game_build`, as the launcher fingerprints it.
pub struct Played {
    pub store: Store,
    pub profile: String,
    pub game_build: Identity,
}

impl Setup for Played {
    /// The profile file, the last builds' outputs, the core DLL, and for each installed addon the files at the top
    /// of its folder (its manifest, the file list its install wrote, a DB Editor addon's edits) and the Addon
    /// Browser download it came from.
    fn inputs(&self) -> Inputs {
        let mut paths = vec![
            self.store.profiles_dir().join(format!("{}.json", self.profile)),
            self.store.root.join(OUTPUTS_FILE),
        ];
        paths.extend(loader_dir().map(|d| d.join(CORE_DLL)));
        for dir in sorted(&self.store.addons_dir(), |p| p.is_dir()) {
            let id = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            paths.extend(sorted(&dir, |p| p.is_file()));
            paths.push(self.store.registry_install_path(&id));
        }
        Inputs::of(paths)
    }

    fn fingerprint(&self) -> Option<Fingerprint> {
        let p = self.store.profile(&self.profile).ok()?;
        addon_set::fingerprint(&self.store, &p, &self.game_build, None).ok()
    }

    fn registry_ids(&self) -> Value {
        addon_set::registry_ids(&self.store.root)
    }
}

/// The entries of `dir` that `keep` keeps, in name order.
fn sorted(dir: &std::path::Path, keep: impl Fn(&std::path::Path) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| keep(p))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The fingerprint of a game session's addon set, worked out on a thread of its own from the session's start, so
/// a lap does not wait while the addon files are hashed. [`Session::get`] waits for that work only as long as it
/// still runs, works the fingerprint out again when the profile, its addons or the build changed since, and takes
/// in the stat ids the core wrote since.
#[derive(Clone)]
pub struct Session {
    shared: Arc<Shared>,
}

struct Shared {
    setup: Box<dyn Setup>,
    slot: Mutex<Slot>,
    done: Condvar,
    say: Box<dyn Fn(&str) + Send + Sync>,
}

enum Slot {
    Working,
    Idle,
    Made(Box<Made>),
}

struct Made {
    inputs: Inputs,
    print: Option<Fingerprint>,
    /// Whether it stands without one: the session start's `None` is tried again by the first lap.
    settled: bool,
}

impl Made {
    fn of(setup: &dyn Setup, settled: bool) -> (Self, Duration) {
        let started = Instant::now();
        let inputs = setup.inputs();
        let print = setup.fingerprint();
        let made = Self {
            inputs,
            print,
            settled,
        };
        (made, started.elapsed())
    }
}

impl Session {
    /// Start working the fingerprint of `setup` out. `say` gets the run log lines: how long it took, and any wait or
    /// second go a lap had.
    pub fn start(setup: impl Setup, say: impl Fn(&str) + Send + Sync + 'static) -> Self {
        let session = Self {
            shared: Arc::new(Shared {
                setup: Box::new(setup),
                slot: Mutex::new(Slot::Working),
                done: Condvar::new(),
                say: Box::new(say),
            }),
        };
        let shared = session.shared.clone();
        let spawned = std::thread::Builder::new()
            .name("tt-fingerprint".into())
            .spawn(move || {
                let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Made::of(&*shared.setup, false)
                }));
                let slot = match made {
                    Ok((made, took)) => {
                        (shared.say)(&match made.print {
                            Some(_) => format!(
                                "addon set fingerprint: worked out in {:.1} s from the session start",
                                took.as_secs_f64()
                            ),
                            None => format!(
                                "addon set fingerprint: could not be worked out at the session start ({:.1} s); the first lap tries again",
                                took.as_secs_f64()
                            ),
                        });
                        Slot::Made(Box::new(made))
                    }
                    Err(_) => Slot::Idle,
                };
                *shared.slot.lock().unwrap_or_else(|e| e.into_inner()) = slot;
                shared.done.notify_all();
            });
        if spawned.is_err() {
            *session.shared.slot.lock().unwrap_or_else(|e| e.into_inner()) = Slot::Idle;
        }
        session
    }

    /// The fingerprint in the shape the server reads, as it is now.
    pub fn get(&self) -> Option<Value> {
        let s = &*self.shared;
        let asked = Instant::now();
        let mut slot = s.slot.lock().unwrap_or_else(|e| e.into_inner());
        while matches!(*slot, Slot::Working) {
            slot = s.done.wait(slot).unwrap_or_else(|e| e.into_inner());
        }
        let waited = asked.elapsed();
        if waited >= TOLD_WAIT {
            (s.say)(&format!(
                "addon set fingerprint: the laps waited {:.1} s for the session start's work",
                waited.as_secs_f64()
            ));
        }
        let why = match &*slot {
            Slot::Made(m) if m.inputs != s.setup.inputs() => {
                Some("the profile, its addons or the build changed since the session start; worked out again")
            }
            Slot::Made(m) if m.print.is_none() && !m.settled => {
                Some("the session start could not work it out; tried again")
            }
            Slot::Made(_) => None,
            _ => Some("the session start did not work it out; worked out for the laps"),
        };
        if let Some(why) = why {
            let (made, took) = Made::of(&*s.setup, true);
            (s.say)(&format!(
                "addon set fingerprint: {why} in {:.1} s",
                took.as_secs_f64()
            ));
            *slot = Slot::Made(Box::new(made));
        }
        let Slot::Made(made) = &*slot else {
            return None;
        };
        let ids = s.setup.registry_ids();
        made.print.clone().map(|p| wire(&p.with_registry_ids(&ids)))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};

    use semver::Version;

    use super::*;
    use crate::online::multiplayer::fingerprint::AddonPrint;
    use crate::profile::Profile;

    fn print(id: &str, source: Source, registry: Option<&str>) -> AddonPrint {
        AddonPrint {
            id: id.into(),
            name: id.into(),
            version: Version::new(1, 2, 0),
            source,
            registry: registry.map(str::to_owned),
            inputs: None,
            order: None,
            detail: None,
            stat_ids: BTreeMap::new(),
        }
    }

    #[test]
    fn names_each_addon_kind_for_the_server() {
        let fp = Fingerprint::new(
            Identity {
                build_id: "1".into(),
                exe_sha256: "exe".into(),
            },
            None,
            vec![
                print(CORE_ID, Source::Builtin("a".into()), None),
                print("swofty.std", Source::Builtin("b".into()), None),
                print("a.reg", Source::Built("c".into()), Some("zipsha")),
                print("a.local", Source::Content("d".into()), None),
            ],
            BTreeMap::new(),
            None,
        );
        let doc = wire(&fp);
        let kinds: Vec<(&str, &str)> = doc["addons"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| (a["id"].as_str().unwrap(), a["source"].as_str().unwrap()))
            .collect();
        assert_eq!(
            kinds,
            [
                ("a.local", "local"),
                ("a.reg", "registry"),
                (CORE_ID, "core"),
                ("swofty.std", "builtin")
            ]
        );
        assert_eq!(doc["addons"][1]["registry"], "zipsha");
        assert!(doc["addons"][0].get("registry").is_none());
        assert_eq!(doc["game_build"], "exe");
    }

    #[derive(Default)]
    struct Knobs {
        inputs: AtomicUsize,
        made: AtomicUsize,
        fail_first: AtomicBool,
        held: Mutex<bool>,
        opened: Condvar,
        ids: Mutex<Value>,
    }

    impl Knobs {
        fn made(&self) -> usize {
            self.made.load(SeqCst)
        }

        fn hold(&self, held: bool) {
            *self.held.lock().unwrap() = held;
            self.opened.notify_all();
        }
    }

    struct Fake(Arc<Knobs>);

    fn made_print(v: usize) -> Fingerprint {
        let mut a = print("t.a", Source::Content("a".repeat(64)), None);
        a.version = Version::new(1, v as u64, 0);
        Fingerprint::new(game_build(), None, vec![a], BTreeMap::new(), None)
    }

    fn game_build() -> Identity {
        Identity {
            build_id: "1".into(),
            exe_sha256: "e".repeat(64),
        }
    }

    impl Setup for Fake {
        fn inputs(&self) -> Inputs {
            Inputs(vec![(
                PathBuf::from(format!("profile-{}", self.0.inputs.load(SeqCst))),
                None,
            )])
        }

        fn fingerprint(&self) -> Option<Fingerprint> {
            let mut held = self.0.held.lock().unwrap();
            while *held {
                held = self.0.opened.wait(held).unwrap();
            }
            drop(held);
            let n = self.0.made.fetch_add(1, SeqCst);
            (n > 0 || !self.0.fail_first.load(SeqCst)).then(|| made_print(self.0.inputs.load(SeqCst)))
        }

        fn registry_ids(&self) -> Value {
            self.0.ids.lock().unwrap().clone()
        }
    }

    fn started(knobs: &Arc<Knobs>) -> Session {
        Session::start(Fake(knobs.clone()), |_| {})
    }

    fn until(what: impl Fn() -> bool) {
        let end = Instant::now() + Duration::from_secs(20);
        while !what() {
            assert!(Instant::now() < end, "timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn the_session_start_works_the_fingerprint_out_before_any_lap_and_the_laps_use_it() {
        let knobs = Arc::new(Knobs::default());
        let session = started(&knobs);
        until(|| knobs.made() == 1);
        assert_eq!(session.get(), Some(wire(&made_print(0))));
        assert_eq!(session.clone().get(), Some(wire(&made_print(0))));
        assert_eq!(knobs.made(), 1);
    }

    #[test]
    fn a_lap_while_the_start_still_works_waits_for_it_rather_than_starting_over() {
        let knobs = Arc::new(Knobs::default());
        knobs.hold(true);
        let session = started(&knobs);
        let lap = {
            let session = session.clone();
            std::thread::spawn(move || session.get())
        };
        std::thread::sleep(Duration::from_millis(200));
        assert!(!lap.is_finished());
        assert_eq!(knobs.made(), 0);
        knobs.hold(false);
        assert_eq!(lap.join().unwrap(), Some(wire(&made_print(0))));
        assert_eq!(knobs.made(), 1);
    }

    #[test]
    fn a_change_to_the_addon_set_or_the_build_works_it_out_again() {
        let knobs = Arc::new(Knobs::default());
        let session = started(&knobs);
        until(|| knobs.made() == 1);
        knobs.inputs.store(1, SeqCst);
        assert_eq!(session.get(), Some(wire(&made_print(1))));
        assert_ne!(wire(&made_print(1)), wire(&made_print(0)));
        assert_eq!(session.get(), Some(wire(&made_print(1))));
        assert_eq!(knobs.made(), 2);
    }

    #[test]
    fn stat_ids_the_core_writes_after_the_start_are_taken_in_without_hashing_again() {
        let knobs = Arc::new(Knobs::default());
        let session = started(&knobs);
        until(|| knobs.made() == 1);
        let ids = json!({ "ids": { "content.package": { "t.a/season": { "id": 116 } } } });
        *knobs.ids.lock().unwrap() = ids.clone();
        let got = session.get().unwrap();
        let fresh = made_print(0).with_registry_ids(&ids);
        assert_ne!(fresh.digest, made_print(0).digest);
        assert_eq!(got, wire(&fresh));
        assert_eq!(knobs.made(), 1);
    }

    #[test]
    fn a_start_that_could_not_work_it_out_leaves_it_to_the_first_lap() {
        let knobs = Arc::new(Knobs::default());
        knobs.fail_first.store(true, SeqCst);
        let session = started(&knobs);
        until(|| knobs.made() == 1);
        assert_eq!(session.get(), Some(wire(&made_print(0))));
        assert_eq!(session.get(), Some(wire(&made_print(0))));
        assert_eq!(knobs.made(), 2);
    }

    fn store(name: &str) -> Store {
        let root = std::env::temp_dir().join(format!("swofty-tt-print-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        Store::new(root)
    }

    fn install(store: &Store, id: &str, ops: &str) {
        let dir = store.addons_dir().join(id);
        std::fs::create_dir_all(dir.join("db")).unwrap();
        let manifest = json!({
            "schema_version": 1,
            "id": id,
            "name": id,
            "version": "1.0.0",
            "author": "a",
            "compat": { "game": "f1_25" },
            "kind": ["database"],
            "operations": { "database": ["db/ops.json"] },
        });
        std::fs::write(dir.join(crate::manifest::MANIFEST_FILE), manifest.to_string()).unwrap();
        std::fs::write(dir.join("db/ops.json"), ops).unwrap();
    }

    fn played(store: &Store) -> Played {
        Played {
            store: store.clone(),
            profile: "p".into(),
            game_build: game_build(),
        }
    }

    fn enable(store: &Store, ids: &[&str]) {
        let mut p = Profile::new("p");
        for id in ids {
            p.set_enabled(id, true).unwrap();
        }
        store.save_profile(&p).unwrap();
    }

    #[test]
    fn the_inputs_change_with_the_profile_the_installed_addons_and_the_build() {
        let store = store("inputs");
        install(&store, "t.a", "1");
        enable(&store, &["t.a"]);
        let setup = played(&store);
        let before = setup.inputs();
        assert_eq!(setup.inputs(), before);
        let changed = |edit: &dyn Fn()| {
            let was = setup.inputs();
            edit();
            setup.inputs() != was
        };
        assert!(changed(&|| install(&store, "t.b", "22")));
        assert!(changed(&|| enable(&store, &["t.a", "t.b"])));
        let outputs = store.root.join(OUTPUTS_FILE);
        std::fs::create_dir_all(outputs.parent().unwrap()).unwrap();
        assert!(changed(&|| std::fs::write(&outputs, "{}").unwrap()));
        let index = store
            .addons_dir()
            .join("t.a")
            .join(crate::addons::files::INDEX_FILE);
        assert!(changed(&|| std::fs::write(&index, "[]").unwrap()));
        let from = store.registry_install_path("t.b");
        std::fs::create_dir_all(from.parent().unwrap()).unwrap();
        assert!(changed(&|| std::fs::write(&from, "{}").unwrap()));
        std::fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn a_session_hands_out_what_a_fresh_fingerprint_of_the_store_is_after_an_addon_is_added() {
        let store = store("session");
        install(&store, "t.a", "1");
        enable(&store, &["t.a"]);
        let session = Session::start(played(&store), |_| {});
        let fresh = || played(&store).fingerprint().map(|f| wire(&f));
        let first = session.get();
        assert!(first.is_some());
        assert_eq!(first, fresh());
        install(&store, "t.b", "22");
        enable(&store, &["t.a", "t.b"]);
        let second = session.get();
        assert_eq!(second, fresh());
        assert_ne!(second.unwrap()["digest"], first.unwrap()["digest"]);
        std::fs::remove_dir_all(&store.root).ok();
    }
}
