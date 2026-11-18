//! An install moves each game file it replaces aside on the game's drive, and the rollback renames it back.
//! However an install or a rollback is cut short, the game folder ends up holding exactly what it held
//! before, on a build output linked from the same drive, one copied into the link cache from another
//! drive, and a game drive without hard links. Everything runs on throwaway folders.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_core::cache::link as link_cache;
use swofty_core::game_folder::baseline::{Identity, sha256_bytes};
use swofty_core::game_folder::journal::{Journal, Op};
use swofty_core::install::{Game, Installer, Plan};
use swofty_core::platform::disk::Volume;

const GVDB: &str = "game_package/gamedatabases/gamedatabases.2025.bin";
const UI: &str = "2025_asset_groups/ui_package/flow_frontend.erp";
/// A second game file with the same bytes as `UI`, so both are moved aside under one name.
const TWIN: &str = "2025_asset_groups/ui_package/flow_frontend_twin.erp";
const ADDED: &str = "2025_asset_groups/environment_package/tracks/okayama/wep/okayama_common.erp";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// The build output is on the game's drive and linked into the game folder.
    Linked,
    /// The build output is on another drive, so the files are linked from the link cache.
    Apart,
    /// The game's drive has no hard links, so every file is copied.
    Copied,
}

const MODES: [Mode; 3] = [Mode::Linked, Mode::Apart, Mode::Copied];

struct Fixture {
    root: PathBuf,
    game: PathBuf,
    data: PathBuf,
    mode: Mode,
    sources: BTreeMap<&'static str, PathBuf>,
}

impl Fixture {
    fn new(name: &str, mode: Mode) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-stash-{name}-{mode:?}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let (game, data) = (root.join("game"), root.join("data"));
        for (rel, body) in [
            ("F1_25.exe", "exe-v1"),
            (GVDB, "db-v1"),
            (UI, "ui-v1"),
            (TWIN, "ui-v1"),
            ("environment_package/tracks/suzuka/suzuka.erp", "stock-suzuka"),
        ] {
            write(&game.join(rel), body);
        }
        let built = data.join("cache/overlay/MODDED/0123456789abcdef");
        let mut sources = BTreeMap::new();
        for (rel, body) in [
            (GVDB, "db-modded"),
            (UI, "ui-modded"),
            (TWIN, "twin-modded"),
            (ADDED, "okayama"),
        ] {
            write(&built.join(rel), body);
            sources.insert(rel, built.join(rel));
        }
        Self {
            root,
            game,
            data,
            mode,
            sources,
        }
    }

    fn game(&self) -> Game {
        Game {
            dir: self.game.clone(),
            user_reg: None,
            identity: Identity::detect(&self.game, Some(1)).unwrap(),
        }
    }

    fn plan(&self) -> Plan {
        let pick = |rels: &[&str]| -> BTreeMap<String, PathBuf> {
            rels.iter()
                .map(|rel| ((*rel).to_owned(), self.sources[rel].clone()))
                .collect()
        };
        Plan {
            profile: "MODDED".into(),
            data: pick(&[GVDB, UI, TWIN]),
            added: pick(&[ADDED]),
            loader: BTreeMap::new(),
            registry_override: false,
        }
    }

    fn installer<'a>(&'a self, game: &'a Game) -> Installer<'a> {
        Installer {
            session: true,
            free_space: roomy,
            volume: if self.mode == Mode::Apart { apart } else { one },
            copy_only: self.mode == Mode::Copied,
            ..Installer::new(&self.data, game)
        }
    }

    /// Every file and folder in the game folder, except the link cache the launcher keeps between
    /// sessions. A moved-aside original left behind shows up.
    fn snapshot(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        walk(&self.game, &self.game, &mut out);
        out.retain(|k, _| {
            k != "swofty-cache/" && !k.starts_with("swofty-cache/overlay") && k != "swofty-cache/index.json"
        });
        out
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.game.join(rel)).unwrap()
    }

    fn originals(&self) -> PathBuf {
        link_cache::originals(&self.game)
    }

    fn stashed(&self, body: &str) -> PathBuf {
        self.originals().join(sha256_bytes(body.as_bytes()))
    }

    fn installed(&self, game: &Game) {
        self.installer(game).apply(&self.plan()).unwrap();
        assert_eq!(self.read(GVDB), "db-modded");
        assert!(self.stashed("db-v1").is_file(), "{:?}", self.mode);
    }

    fn recovered(&self, game: &Game, original: &BTreeMap<String, String>, what: &str) {
        let restarted = self.installer(game);
        assert_eq!(restarted.recover().unwrap(), [], "{what} {:?}", self.mode);
        assert_eq!(self.snapshot(), *original, "{what} {:?}", self.mode);
        assert_eq!(restarted.verify().unwrap(), [], "{what} {:?}", self.mode);
        assert!(Journal::open(&self.data).unwrap().is_empty(), "{what}");
        assert!(!self.originals().exists(), "{what} {:?}", self.mode);
        if self.mode != Mode::Apart {
            assert!(!link_cache::dir(&self.game).exists(), "{what} {:?}", self.mode);
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn roomy(_: &Path) -> Option<u64> {
    Some(1 << 40)
}

fn one(_: &Path) -> Option<Volume> {
    Some(Volume {
        id: "one".into(),
        name: "/".into(),
    })
}

fn apart(path: &Path) -> Option<Volume> {
    let data = path.components().any(|c| c.as_os_str() == "data");
    let id = if data { "data" } else { "game" };
    Some(Volume {
        id: id.into(),
        name: id.into(),
    })
}

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let rel = e
            .path()
            .strip_prefix(base)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if e.path().is_dir() {
            out.insert(format!("{rel}/"), "dir".into());
            walk(base, &e.path(), out);
        } else {
            out.insert(rel, sha256_bytes(&std::fs::read(e.path()).unwrap()));
        }
    }
}

fn partial(path: &Path, kind: &str) -> PathBuf {
    path.with_file_name(format!(".{}.{kind}", path.file_name().unwrap().to_string_lossy()))
}

#[cfg(unix)]
fn inode(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).unwrap().ino()
}

#[test]
fn a_session_moves_the_replaced_files_aside_and_the_rollback_renames_them_back() {
    for mode in MODES {
        let f = Fixture::new("rename", mode);
        let game = f.game();
        let original = f.snapshot();
        #[cfg(unix)]
        let before = [GVDB, UI, TWIN].map(|rel| inode(&f.game.join(rel)));
        // Held open so a freed original keeps its inode number and a later file cannot be given it.
        #[cfg(unix)]
        let _held = [GVDB, UI, TWIN].map(|rel| std::fs::File::open(f.game.join(rel)).unwrap());
        f.installed(&game);
        assert_eq!(f.read(UI), "ui-modded");
        assert_eq!(f.read(TWIN), "twin-modded");

        let done = f.installer(&game).roll_back(true).unwrap();
        assert_eq!(done.len(), 1, "{mode:?}");
        assert_eq!(f.snapshot(), original, "{mode:?}");
        assert_eq!(f.installer(&game).verify().unwrap(), [], "{mode:?}");
        assert!(!f.originals().exists(), "{mode:?}");
        if mode != Mode::Apart {
            assert!(!link_cache::dir(&f.game).exists(), "{mode:?}");
        }
        // The twin took the shared name last, so it is renamed back and the first one is copied from
        // the backup.
        #[cfg(unix)]
        {
            let after = [GVDB, UI, TWIN].map(|rel| inode(&f.game.join(rel)));
            assert_eq!(after[0], before[0], "{mode:?}");
            assert_ne!(after[1], before[1], "{mode:?}");
            assert_eq!(after[2], before[2], "{mode:?}");
        }
        assert_eq!(std::fs::read_to_string(&f.sources[GVDB]).unwrap(), "db-modded");
        assert_eq!(std::fs::read_to_string(&f.sources[TWIN]).unwrap(), "twin-modded");
    }
}

#[test]
fn a_crash_at_any_point_of_an_install_ends_with_the_game_folder_as_it_was() {
    let total = {
        let f = Fixture::new("count", Mode::Linked);
        let game = f.game();
        f.installed(&game);
        Journal::open(&f.data).unwrap()[0].entries.len()
    };
    #[derive(Clone, Copy, Debug)]
    enum Cut {
        BeforeTheEntry,
        MovedAside,
        MidLink,
        Placed,
    }
    for mode in MODES {
        for k in 0..=total {
            for cut in [Cut::BeforeTheEntry, Cut::MovedAside, Cut::MidLink, Cut::Placed] {
                let f = Fixture::new(&format!("cut{k}-{cut:?}"), mode);
                let game = f.game();
                let original = f.snapshot();
                let mut inst = f.installer(&game);
                inst.stop_after = Some(k);
                assert_eq!(inst.apply(&f.plan()).is_ok(), k == total, "k={k} {mode:?}");
                let open = Journal::open(&f.data).unwrap();
                assert_eq!(open.len(), 1, "k={k}");
                if let Some(e) = open[0].entries.get(k).filter(|e| e.op == Op::ReplaceFile) {
                    let path = f.game.join(&e.path);
                    let at = f.originals().join(e.before.as_deref().unwrap());
                    let src = &f.sources[e.path.as_str()];
                    if !matches!(cut, Cut::BeforeTheEntry) {
                        std::fs::create_dir_all(f.originals()).unwrap();
                        std::fs::rename(&path, &at).unwrap();
                    }
                    match cut {
                        Cut::MidLink => {
                            std::fs::copy(src, partial(&path, "swofty-link")).unwrap();
                            std::fs::write(partial(&path, "swofty-copy"), "half").unwrap();
                        }
                        Cut::Placed => {
                            std::fs::copy(src, &path).unwrap();
                        }
                        Cut::BeforeTheEntry | Cut::MovedAside => {}
                    }
                }
                f.recovered(&game, &original, &format!("k={k} {cut:?}"));
            }
        }
    }
}

#[test]
fn a_power_cut_that_kept_only_some_renames_is_rolled_back_to_the_game_folder_as_it_was() {
    for mode in MODES {
        for lost in ["replacement", "move aside", "both"] {
            let f = Fixture::new(&format!("power-{}", lost.replace(' ', "-")), mode);
            let game = f.game();
            let original = f.snapshot();
            f.installed(&game);
            let (path, at) = (f.game.join(GVDB), f.stashed("db-v1"));
            match lost {
                "replacement" => std::fs::remove_file(&path).unwrap(),
                "move aside" => std::fs::remove_file(&at).unwrap(),
                _ => std::fs::rename(&at, &path).unwrap(),
            }
            f.recovered(&game, &original, lost);
        }
    }
}

#[test]
fn a_rollback_cut_short_is_finished_by_the_next_start() {
    for mode in MODES {
        for cut in [
            "one renamed back",
            "all renamed back",
            "copy from the backup half done",
        ] {
            let f = Fixture::new(&format!("rollback-{}", cut.replace(' ', "-")), mode);
            let game = f.game();
            let original = f.snapshot();
            f.installed(&game);
            match cut {
                "one renamed back" => std::fs::rename(f.stashed("db-v1"), f.game.join(GVDB)).unwrap(),
                "all renamed back" => {
                    std::fs::rename(f.stashed("db-v1"), f.game.join(GVDB)).unwrap();
                    std::fs::rename(f.stashed("ui-v1"), f.game.join(TWIN)).unwrap();
                    std::fs::copy(f.game.join(TWIN), f.game.join(UI)).unwrap();
                }
                _ => {
                    std::fs::remove_file(f.stashed("db-v1")).unwrap();
                    write(&partial(&f.game.join(GVDB), "swofty-restore"), "db-");
                }
            }
            f.recovered(&game, &original, cut);
        }
    }
}

#[test]
fn an_original_changed_or_deleted_while_aside_is_copied_from_the_backup() {
    for mode in MODES {
        for what in ["changed", "deleted", "cache removed"] {
            let f = Fixture::new(&format!("damaged-{}", what.replace(' ', "-")), mode);
            let game = f.game();
            let original = f.snapshot();
            f.installed(&game);
            match what {
                "changed" => {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    std::fs::write(f.stashed("db-v1"), "db-v2").unwrap();
                }
                "deleted" => std::fs::remove_file(f.stashed("db-v1")).unwrap(),
                _ => {
                    link_cache::remove(&f.game);
                }
            }
            let done = f.installer(&game).roll_back(true).unwrap();
            assert_eq!(done.len(), 1);
            assert!(done[0].drift.is_empty(), "{what} {mode:?}: {:?}", done[0].drift);
            assert_eq!(f.read(GVDB), "db-v1", "{what} {mode:?}");
            assert_eq!(f.snapshot(), original, "{what} {mode:?}");
            assert!(!f.originals().exists(), "{what} {mode:?}");
        }
    }
}

#[test]
fn a_failed_copy_after_the_move_aside_puts_the_original_back_at_once() {
    for mode in [Mode::Linked, Mode::Copied] {
        let f = Fixture::new("failed-copy", mode);
        let game = f.game();
        let original = f.snapshot();
        let twin = f.sources[TWIN].clone();
        let progress = |done: usize, _: usize| {
            if done == 1 {
                std::fs::remove_file(&twin).ok();
            }
        };
        let inst = Installer {
            progress: &progress,
            ..f.installer(&game)
        };
        assert!(inst.apply(&f.plan()).is_err(), "{mode:?}");
        assert_eq!(f.read(UI), "ui-modded", "{mode:?}");
        assert_eq!(f.read(TWIN), "ui-v1", "{mode:?}");
        assert_eq!(f.read(GVDB), "db-v1", "{mode:?}");
        f.recovered(&game, &original, "failed copy");
    }
}

#[test]
fn without_room_to_keep_the_originals_they_are_replaced_and_copied_back() {
    for mode in MODES {
        let f = Fixture::new("tight", mode);
        let game = f.game();
        let original = f.snapshot();
        let inst = Installer {
            free_space: |_| Some(1 << 20),
            ..f.installer(&game)
        };
        inst.apply(&f.plan()).unwrap();
        assert_eq!(f.read(GVDB), "db-modded");
        assert!(!f.originals().exists(), "{mode:?}");
        inst.roll_back(true).unwrap();
        assert_eq!(f.snapshot(), original, "{mode:?}");
    }
}

#[test]
#[ignore = "manual rollback timing on a profile-sized install; set TMPDIR to a folder on a disk"]
fn benchmark_rollback_of_a_profile_sized_install() {
    let sizes: Vec<u64> = std::env::var("SWOFTY_BENCH_SIZES")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.split_whitespace().filter_map(|n| n.parse().ok()).collect())
        .unwrap_or_else(|| vec![7_800_000; 137]);
    for keep in [false, true] {
        let f = Fixture::new(if keep { "bench-kept" } else { "bench-copied" }, Mode::Linked);
        let built = f.data.join("cache/overlay/MODDED/0123456789abcdef");
        let mut data = BTreeMap::new();
        for (n, size) in sizes.iter().enumerate() {
            let rel = format!("2025_asset_groups/bench/f{n}.erp");
            let body: Vec<u8> = (0..*size).map(|i| (i as u8) ^ (n as u8)).collect();
            std::fs::create_dir_all(f.game.join(&rel).parent().unwrap()).unwrap();
            std::fs::write(f.game.join(&rel), &body).unwrap();
            std::fs::create_dir_all(built.join(&rel).parent().unwrap()).unwrap();
            std::fs::write(built.join(&rel), &body[..body.len() / 2]).unwrap();
            data.insert(rel.clone(), built.join(&rel));
        }
        let plan = Plan {
            profile: "MODDED".into(),
            data,
            added: BTreeMap::new(),
            loader: BTreeMap::new(),
            registry_override: false,
        };
        let game = f.game();
        let inst = Installer {
            // 2.75 GiB holds the backup and the link cache with its margin, not the originals kept aside too.
            free_space: if keep { roomy } else { |_| Some(11 << 28) },
            ..f.installer(&game)
        };
        inst.apply(&plan).unwrap();
        inst.roll_back(true).unwrap();
        let start = std::time::Instant::now();
        inst.apply(&plan).unwrap();
        let applied = start.elapsed();
        assert_eq!(link_cache::originals(&f.game).exists(), keep);
        let start = std::time::Instant::now();
        inst.roll_back(true).unwrap();
        println!(
            "{} files, {} MiB replaced; originals kept aside: {keep}; install {applied:?}, rollback {:?}",
            sizes.len(),
            sizes.iter().sum::<u64>() >> 20,
            start.elapsed()
        );
    }
}
