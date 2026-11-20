use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use swofty_core::GAME_EXE;
use swofty_core::addons::builtin::loader_dir;
use swofty_core::asset::{AssetBatch, AssetOp, FileOp, Op, Origin, Root, Source};
use swofty_core::build::GVDB;
use swofty_core::game_folder::baseline::Identity;
use swofty_core::game_folder::busy::{Busy, Step};
use swofty_core::install::Installer;
use swofty_core::launcher::{Addon, BuildCx, Builtin};
use swofty_core::manifest::AddonManifest;
use swofty_formats::gvdb::Gvdb;

use super::conflicts::conflicts_text;
use super::heartbeat::{Beat, Copied, track};
use super::install::{denied, outdated_with};
use super::profile::build_profile;
use super::*;

#[test]
fn names_the_game_folder_when_windows_refuses_a_write() {
    let (data, game) = (Path::new("/data/Swofty"), Path::new("/games/F1 25"));
    let refused = swofty_core::Error::Io {
        path: game.join("2025_asset_groups/ui_package/databases.erp"),
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    };
    let text = denied(anyhow::Error::from(refused), data, game).to_string();
    assert!(
        text.ends_with(
            "Windows refused to let Swofty change files in /games/F1 25; run Swofty Launcher as \
             administrator, or move F1 25 to a folder outside Program Files"
        ),
        "{text}"
    );
    let elsewhere = swofty_core::Error::Io {
        path: "/other/cache/x".into(),
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    };
    assert_eq!(
        denied(anyhow::Error::from(elsewhere), data, game).to_string(),
        "/other/cache/x: permission denied"
    );
}

#[test]
fn names_the_antivirus_exclusions_when_windows_refuses_an_overlay_archive() {
    let (data, game) = (Path::new("/data/Swofty"), Path::new("/games/F1 25"));
    let overlay = data.join("cache/overlay/default/0123/2025_asset_groups/ui_package/databases.erp");
    let refused = swofty_core::Error::Formats(swofty_formats::Error::IoAt {
        path: overlay.clone(),
        source: std::io::Error::from(std::io::ErrorKind::PermissionDenied),
    });
    assert_eq!(
        denied(anyhow::Error::from(refused), data, game).to_string(),
        format!(
            "{}: permission denied. Windows refused to let Swofty change its own files in /data/Swofty, which \
             an antivirus does while it scans them; add /data/Swofty and the F1 25 folder /games/F1 25 as \
             exclusions in your antivirus, then try again",
            overlay.display()
        )
    );
    let missing = swofty_core::Error::Io {
        path: overlay,
        source: std::io::Error::from(std::io::ErrorKind::NotFound),
    };
    assert!(
        !denied(anyhow::Error::from(missing), data, game)
            .to_string()
            .contains("antivirus")
    );
}

const APPLY_FAILED: &str = "applying swofty.classic-seasons: eaid://ui_package/databases/overrides.uidb: \
                            overrides.Contents.teams.Contents.mp_team_select_logos.Contents: no entry \
                            [Name=vanamersfoort_26]";

#[test]
fn an_addon_that_does_not_apply_to_an_old_game_leads_with_the_update() {
    let names = BTreeMap::from([("swofty.classic-seasons".to_owned(), "Classic Seasons".to_owned())]);
    let failed = || anyhow::Error::from(swofty_core::Error::Other(APPLY_FAILED.to_owned()));
    assert_eq!(
        format!("{:#}", outdated_with(failed(), Some("1.0.136.6820"), &names)),
        format!(
            "Classic Seasons needs F1 25 1.0.145.2472, the latest update. Your F1 25 is 1.0.136.6820. \
             Update F1 25 in Steam or the EA app, then press Play again.\n({APPLY_FAILED})"
        )
    );
    for version in [Some("1.0.145.2472"), None, Some("")] {
        assert_eq!(
            format!("{:#}", outdated_with(failed(), version, &names)),
            APPLY_FAILED,
            "{version:?}"
        );
    }
    let other = anyhow::anyhow!("version.dll already exists in the game folder; run swofty restore");
    assert_eq!(
        outdated_with(other, Some("1.0.136.6820"), &names).to_string(),
        "version.dll already exists in the game folder; run swofty restore"
    );
}

struct Writer(AddonManifest);

impl Addon for Writer {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, cx: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        cx.cx.set_state(&serde_json::json!({"built": true}))?;
        Ok(AssetBatch::default())
    }
}

#[test]
fn keeps_profile_changes_made_during_the_build() {
    if loader_dir().is_some() {
        return;
    }
    let root = std::env::temp_dir().join(format!("swofty-cli-build-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let manifest = AddonManifest::parse(
        r#"{"schema_version":1,"id":"t.writer","name":"Writer","version":"1.0.0","author":"t","compat":{"game":"f1_25"}}"#,
    )
    .unwrap();
    let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
        addon: Arc::new(Writer(manifest)),
        locked: false,
        default_on: true,
    }]);
    build_profile(&store, "default", &game, None, &[]).unwrap();
    let p = store.profile("default").unwrap();
    assert_eq!(p.state["t.writer"], serde_json::json!({"built": true}));
    std::fs::remove_dir_all(&root).ok();
}

const MIP: &str =
    "environment_package/tracks/suzuka24/mipmaps/f55b20d9a14b_pit_perch_ferrari_b_n.tif.2025.mipmaps";

struct Putter(AddonManifest);

impl Addon for Putter {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn build(&self, _: &BuildCx) -> swofty_addon::Result<AssetBatch> {
        Ok(AssetBatch {
            ops: vec![Op {
                op: AssetOp::File(FileOp::Put {
                    source: Source {
                        root: Root::Data,
                        path: "mip".into(),
                    },
                    target: MIP.into(),
                }),
                origin: Origin {
                    addon: "t.putter".into(),
                    item: None,
                },
            }],
            ..AssetBatch::default()
        })
    }
}

struct Harness {
    root: PathBuf,
    store: Store,
    game: Game,
}

impl Harness {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-cli-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let dir = root.join("game");
        std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
        std::fs::write(dir.join(GAME_EXE), "exe").unwrap();
        std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
        let manifest = AddonManifest::parse(
            r#"{"schema_version":1,"id":"t.putter","name":"Putter","version":"1.0.0","author":"t","compat":{"game":"f1_25"}}"#,
        )
        .unwrap();
        let store = Store::new(root.join("data")).with_builtins(vec![Builtin {
            addon: Arc::new(Putter(manifest)),
            locked: false,
            default_on: true,
        }]);
        let data = store.addon_data_dir("t.putter");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("mip"), "mip-ferrari").unwrap();
        Self {
            game: Game {
                identity: Identity::detect(&dir, Some(1)).unwrap(),
                dir,
                user_reg: None,
            },
            store,
            root,
        }
    }

    fn install(&self, log: &RefCell<Vec<String>>) -> Result<()> {
        if !swofty_core::game_folder::setup::done(&self.store.root) {
            swofty_core::game_folder::setup::reset(&self.store.root, &self.game, false)?;
            swofty_core::game_folder::setup::finish(&self.store.root)?;
        }
        install(&self.store, "default", &self.game, true, &|l| {
            log.borrow_mut().push(l.to_owned())
        })
    }

    fn mip(&self) -> String {
        std::fs::read_to_string(self.game.dir.join(MIP)).unwrap()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[test]
fn a_leftover_from_an_interrupted_session_is_recovered_and_the_launch_proceeds() {
    if loader_dir().is_some() {
        return;
    }
    let h = Harness::new("leftover");
    let log = RefCell::new(Vec::new());
    h.install(&log).unwrap();
    assert_eq!(h.mip(), "mip-ferrari");
    assert!(
        log.borrow().iter().all(|l| l.starts_with("installing: ")),
        "{:?}",
        log.borrow()
    );
    log.borrow_mut().clear();
    // The session was interrupted and its journal is gone; the mip file
    // it placed is still in the game folder.
    std::fs::remove_dir_all(h.store.root.join("state/journal")).unwrap();

    h.install(&log).unwrap();
    let log = log.into_inner();
    assert_eq!(log[0], swofty_core::game_folder::recovery::HEADLINE);
    assert_eq!(log[1], format!("  removed {MIP}"));
    assert!(log.iter().any(|l| l.starts_with("doctor: data dir")), "{log:?}");
    assert!(log.iter().any(|l| l.starts_with("doctor: baseline")), "{log:?}");
    assert_eq!(h.mip(), "mip-ferrari");
    let open = swofty_core::game_folder::journal::Journal::open(&h.store.root).unwrap();
    assert_eq!(open.len(), 1);
    assert!(open[0].session);
    assert!(open[0].entries.iter().any(|e| e.path == MIP));
    let notice = swofty_core::game_folder::recovery::Notice::read(&h.store.root).unwrap();
    let removed: Vec<_> = notice
        .lines
        .iter()
        .filter(|l| !l.starts_with("removed swofty/"))
        .collect();
    assert_eq!(
        removed,
        [&format!("removed {MIP}"), "removed videos/cm_f1_sting.bk2"]
    );

    let inst = Installer::new(&h.store.root, &h.game);
    inst.roll_back(true).unwrap();
    assert!(!h.game.dir.join("environment_package").exists());
    assert_eq!(inst.verify().unwrap(), []);
}

#[test]
fn a_hand_copied_file_is_backed_up_and_put_back_never_deleted() {
    if loader_dir().is_some() {
        return;
    }
    let h = Harness::new("by-hand");
    std::fs::create_dir_all(h.game.dir.join(MIP).parent().unwrap()).unwrap();
    std::fs::write(h.game.dir.join(MIP), "copied in by hand").unwrap();
    let log = RefCell::new(Vec::new());
    h.install(&log).unwrap();
    assert_eq!(h.mip(), "mip-ferrari");
    Installer::new(&h.store.root, &h.game).roll_back(true).unwrap();
    assert_eq!(h.mip(), "copied in by hand");
}

#[test]
fn a_file_of_unknown_provenance_stops_the_launch_untouched_with_what_restore_does() {
    if loader_dir().is_some() {
        return;
    }
    let h = Harness::new("unknown");
    let built = build_profile(&h.store, "default", &h.game, None, &[]).unwrap();
    assert!(built.plan.added.contains_key(MIP));
    // A copy that turns up after the build is only found when the files
    // are placed, as with track mip files, which always go in as
    // additions.
    std::fs::create_dir_all(h.game.dir.join(MIP).parent().unwrap()).unwrap();
    std::fs::write(h.game.dir.join(MIP), "copied in by hand").unwrap();
    let err = Installer::new(&h.store.root, &h.game)
        .apply(&built.plan)
        .map_err(|e| match e {
            swofty_core::Error::Leftovers(found) => crate::session_lock::blocked(&h.store, &h.game, &found),
            e => e.into(),
        })
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with(&format!(
            "{MIP} is already in the game folder, and swofty did not put it there; it was left untouched in case it is a mod copied in by hand. Restore game folder (swofty restore) rolls back swofty's own changes and moves this file to "
        )),
        "{err}"
    );
    assert!(err.ends_with(", where you can get it back"), "{err}");
    assert!(!err.contains('\n'));
    assert_eq!(h.mip(), "copied in by hand");
    assert!(
        swofty_core::game_folder::journal::Journal::open(&h.store.root)
            .unwrap()
            .is_empty()
    );
    let blocked = swofty_core::game_folder::recovery::Blocked::read(&h.store.root).unwrap();
    assert_eq!(blocked.files[0].path, MIP);

    let done = swofty_core::game_folder::recovery::restore(&h.store.root, &h.game).unwrap();
    assert_eq!(
        std::fs::read_to_string(&done.set_aside[0]).unwrap(),
        "copied in by hand"
    );
    assert!(!h.game.dir.join("environment_package").exists());
    let log = RefCell::new(Vec::new());
    h.install(&log).unwrap();
    assert_eq!(h.mip(), "mip-ferrari");
}

#[test]
fn prints_each_pair_then_its_items() {
    use swofty_core::profile::conflicts::{Pair, Report};
    let mut report = Report {
        profile: "default".into(),
        built_at: 1.0,
        ..Report::default()
    };
    let names = BTreeMap::from([("t.b".to_owned(), "Second".to_owned())]);
    assert_eq!(
        conflicts_text(&report, &names),
        "default: no addon overrides another\n"
    );
    report.pairs.push(Pair {
        winner: "t.b".into(),
        loser: "t.a".into(),
        database: 1,
        resources: 0,
        files: 1,
        strings: 0,
        items: vec!["rules.laps".into(), "x/a.dds".into()],
    });
    assert_eq!(
        conflicts_text(&report, &names),
        "Second overrides t.a on 1 database value and 1 file\n  rules.laps\n  x/a.dds\n"
    );
    let max = swofty_core::profile::conflicts::MAX_ITEMS;
    report.pairs[0] = Pair {
        database: max,
        strings: 2,
        items: vec!["rules.laps".into(); max],
        ..report.pairs[0].clone()
    };
    assert!(conflicts_text(&report, &names).ends_with("  and 3 more\n"));
}

#[test]
fn the_build_file_follows_the_build_then_the_copy() {
    let mut b = Busy::new("default", vec!["swofty.classic-seasons".into()]);
    let copied = Copied::default();
    track(&mut b, 3155, &copied);
    assert_eq!((b.step, b.done, b.total), (Step::Building, 3155, 0));
    copied.0.store(443, Ordering::Relaxed);
    copied.1.store(2544, Ordering::Relaxed);
    track(&mut b, 3160, &copied);
    assert_eq!((b.step, b.done, b.total), (Step::Copying, 443, 2544));
}

#[test]
fn a_step_that_runs_long_still_writes_a_line_a_minute() {
    let t0 = Instant::now();
    let mut beat = Beat::new(100, String::new(), t0);
    let mut at = |s: u64, work: u64| {
        beat.line(
            work,
            "building swofty.classic-seasons".into(),
            t0 + Duration::from_secs(s),
        )
    };
    assert_eq!(at(29, 457), None);
    let line = |s: u64| {
        format!(
            "build: still working after {s} s (357 file(s) and fragment(s) so far; building swofty.classic-seasons)"
        )
    };
    assert_eq!(at(30, 457), Some(line(30)));
    assert_eq!(at(60, 457), None);
    assert_eq!(at(90, 457), Some(line(90)));
    assert_eq!(at(120, 457), None);
    assert_eq!(at(150, 457), Some(line(150)));
    assert!(at(180, 460).is_some());
    let lines = (7..=40).filter_map(|n| at(n * 30, 460)).count();
    assert_eq!(lines, 17, "one a minute over 17 silent minutes");
}
