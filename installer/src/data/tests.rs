use super::*;
use std::io::{self, ErrorKind};

use crate::data::moving::Record;
use std::sync::Mutex;
use swofty_core::data::relocate::link::{create as link, target as link_target};
use swofty_core::launcher::Progress;

struct Quiet;

impl Progress for Quiet {
    fn report(&self, _: f32, _: &str) {}
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-installer-data-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn seed(data: &Path) {
    write(
        &data.join("addons/classic-seasons/swofty.json"),
        "{\"id\":\"cs\"}",
    );
    write(&data.join("profiles/default.json"), "{}");
    write(&data.join("state/session.json"), "{\"a\":1}");
}

fn data(default: &Path) -> Data {
    Data {
        default: default.to_path_buf(),
        env: None,
        install: None,
    }
}

const fn ops(rename: fn(&Path, &Path) -> io::Result<()>) -> Ops {
    Ops {
        rename,
        blocked: || None,
        free: |_| None,
        set_env: |_| Ok(()),
    }
}

const SAME_VOLUME: Ops = ops(|a, b| std::fs::rename(a, b));
const OTHER_DRIVE: Ops = ops(|_, _| Err(io::Error::from(ErrorKind::CrossesDevices)));

fn moved_whole(to: &Path) {
    assert_eq!(
        read(&to.join("addons/classic-seasons/swofty.json")),
        "{\"id\":\"cs\"}"
    );
    assert_eq!(read(&to.join("profiles/default.json")), "{}");
    assert_eq!(read(&to.join("state/session.json")), "{\"a\":1}");
}

#[test]
fn refuses_to_move_while_a_launcher_move_is_unfinished() {
    let dir = temp("journal");
    let (default, to) = (dir.join("Local/Swofty"), dir.join("D/SwoftyData"));
    seed(&default);
    write(&dir.join("Local/swofty-data-move.json"), "{}");
    let err = plan(&data(&default), &to).unwrap_err();
    assert!(err.contains("Open Swofty Launcher to finish it"), "{err}");
    std::fs::remove_file(dir.join("Local/swofty-data-move.json")).unwrap();
    assert!(plan(&data(&default), &to).unwrap().is_some());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn renames_the_data_folder_on_the_same_volume_and_links_the_default_to_it() {
    let dir = temp("rename");
    let (default, to) = (dir.join("Local/Swofty"), dir.join("D/SwoftyData"));
    seed(&default);
    let d = data(&default);
    let m = plan(&d, &to).unwrap().unwrap();
    assert_eq!(m.record, Record::Link);
    m.run(&SAME_VOLUME, &Quiet).unwrap();
    moved_whole(&to);
    assert_eq!(link_target(&default), Some(to.clone()));
    moved_whole(&default);
    assert_eq!(d.current(), to);
    assert!(!dir.join("Local/Swofty.old").exists());
    assert_eq!(plan(&d, &to).unwrap(), None, "nothing left to move");
    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn copies_to_another_drive_checks_the_copy_then_deletes_the_old_folder() {
    let dir = temp("copy");
    let (default, to) = (dir.join("Local/Swofty"), dir.join("D/SwoftyData"));
    seed(&default);
    let game = dir.join("F1 25");
    write(&game.join("F1_25.exe"), "game");
    std::os::unix::fs::symlink(&game, default.join("state/game-link")).unwrap();
    std::fs::create_dir_all(&to).unwrap();
    plan(&data(&default), &to)
        .unwrap()
        .unwrap()
        .run(&OTHER_DRIVE, &Quiet)
        .unwrap();
    moved_whole(&to);
    assert_eq!(
        std::fs::read_link(to.join("state/game-link")).unwrap(),
        game,
        "links inside the data folder stay links"
    );
    assert_eq!(link_target(&default), Some(to.clone()));
    assert!(!dir.join("Local/Swofty.old").exists());
    assert_eq!(read(&game.join("F1_25.exe")), "game");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn refuses_a_copy_that_does_not_fit() {
    let dir = temp("full");
    let (default, to) = (dir.join("Local/Swofty"), dir.join("D/SwoftyData"));
    seed(&default);
    let full = Ops {
        free: |_| Some(3),
        ..OTHER_DRIVE
    };
    let err = plan(&data(&default), &to)
        .unwrap()
        .unwrap()
        .run(&full, &Quiet)
        .unwrap_err();
    assert!(err.contains("Not enough free space"), "{err}");
    moved_whole(&default);
    assert!(link_target(&default).is_none());
    assert!(!to.exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn re_points_an_existing_link_and_moves_the_data_from_its_old_target() {
    for (name, how) in [("repoint-rename", SAME_VOLUME), ("repoint-copy", OTHER_DRIVE)] {
        let dir = temp(name);
        let (default, old, new) = (dir.join("Local/Swofty"), dir.join("D/Old"), dir.join("E/New"));
        seed(&old);
        std::fs::create_dir_all(default.parent().unwrap()).unwrap();
        link(&old, &default).unwrap();
        let d = data(&default);
        assert_eq!(d.current(), old);
        assert_eq!(plan(&d, &old).unwrap(), None);
        plan(&d, &new).unwrap().unwrap().run(&how, &Quiet).unwrap();
        moved_whole(&new);
        assert_eq!(link_target(&default), Some(new.clone()), "{name}");
        assert!(!old.exists(), "{name}");
        moved_whole(&default);
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn moving_back_to_the_default_removes_the_link() {
    for (name, how) in [("back-rename", SAME_VOLUME), ("back-copy", OTHER_DRIVE)] {
        let dir = temp(name);
        let (default, old) = (dir.join("Local/Swofty"), dir.join("D/Old"));
        seed(&old);
        std::fs::create_dir_all(default.parent().unwrap()).unwrap();
        link(&old, &default).unwrap();
        let m = plan(&data(&default), &default).unwrap().unwrap();
        assert_eq!(m.record, Record::Unlink);
        m.run(&how, &Quiet).unwrap();
        assert!(link_target(&default).is_none(), "{name}");
        assert!(default.symlink_metadata().unwrap().is_dir());
        moved_whole(&default);
        assert!(!old.exists(), "{name}");
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[test]
fn refuses_a_target_that_is_not_empty_or_inside_the_data_folder() {
    let dir = temp("refuse");
    let default = dir.join("Local/Swofty");
    seed(&default);
    let d = data(&default);
    let busy = dir.join("D/Busy");
    write(&busy.join("notes.txt"), "mine");
    let err = plan(&d, &busy).unwrap_err();
    assert!(err.contains("not empty"), "{err}");
    let file = dir.join("D/file");
    write(&file, "x");
    assert!(plan(&d, &file).unwrap_err().contains("not a folder"));
    let err = plan(&d, &default.join("addons/deeper")).unwrap_err();
    assert!(err.contains("outside"), "{err}");
    assert_eq!(plan(&d, &default).unwrap(), None);
    std::fs::create_dir_all(dir.join("D/Empty")).unwrap();
    assert!(plan(&d, &dir.join("D/Empty")).unwrap().is_some());
    assert_eq!(read(&busy.join("notes.txt")), "mine");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn stops_instead_of_starting_fresh_when_the_link_target_is_missing() {
    let dir = temp("dangling");
    let (default, gone) = (dir.join("Local").join("Swofty"), dir.join("D").join("Old"));
    std::fs::create_dir_all(&gone).unwrap();
    std::fs::create_dir_all(default.parent().unwrap()).unwrap();
    link(&gone, &default).unwrap();
    std::fs::remove_dir(&gone).unwrap();
    let d = data(&default);
    for to in [default.clone(), dir.join("E").join("New")] {
        let err = plan(&d, &to).unwrap_err();
        assert!(err.contains("is missing"), "{err}");
    }
    assert!(default.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(!dir.join("E").join("New").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn refuses_to_move_while_the_launcher_or_the_game_runs() {
    let dir = temp("running");
    let (default, to) = (dir.join("Local/Swofty"), dir.join("D/SwoftyData"));
    seed(&default);
    let busy = Ops {
        blocked: || {
            crate::running::holding(
                &[crate::running::Running {
                    pid: 7,
                    part: "swofty-launcher",
                }],
                true,
            )
        },
        ..SAME_VOLUME
    };
    let err = plan(&data(&default), &to)
        .unwrap()
        .unwrap()
        .run(&busy, &Quiet)
        .unwrap_err();
    assert_eq!(
        err,
        "Close Swofty Launcher and F1 25 before moving the data folder, then select Try again."
    );
    moved_whole(&default);
    assert!(!to.exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn leaves_the_data_where_it_was_when_the_move_fails() {
    let dir = temp("fail");
    let (default, to) = (dir.join("Local/Swofty"), dir.join("D/SwoftyData"));
    seed(&default);
    let locked = ops(|_, _| Err(io::Error::from(ErrorKind::PermissionDenied)));
    let err = plan(&data(&default), &to)
        .unwrap()
        .unwrap()
        .run(&locked, &Quiet)
        .unwrap_err();
    assert!(err.starts_with("Could not move"), "{err}");
    moved_whole(&default);
    assert!(link_target(&default).is_none());
    std::fs::remove_dir_all(&dir).ok();
}

static SET: Mutex<Option<PathBuf>> = Mutex::new(None);

#[test]
fn refuses_a_data_folder_inside_or_around_the_install_folder() {
    let dir = temp("apart");
    let (install, default) = (dir.join("Games/Swofty Launcher"), dir.join("Local/Swofty"));
    let d = Data {
        install: Some(install.clone()),
        ..data(&default)
    };
    for inside in [install.clone(), install.join("Swofty"), install.join("a/b")] {
        let e = plan(&d, &inside).unwrap_err();
        assert!(e.contains("inside the install folder"), "{e}");
        assert!(d.describe(&inside).is_err());
        assert!(d.apart(&inside).is_err());
    }
    let e = plan(&d, &dir.join("Games")).unwrap_err();
    assert!(
        e.contains("does not contain") || e.contains("inside the data folder"),
        "{e}"
    );
    assert!(plan(&d, &dir.join("Games/Swofty Launcher Data")).is_ok());
    assert!(plan(&d, &dir.join("Elsewhere/Swofty")).is_ok());
    assert!(d.apart(&default).is_ok());
    assert!(data(&default).apart(&install).is_ok());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn asks_only_when_the_drives_differ() {
    let dir = temp("asks");
    let d = data(&dir.join("Local/Swofty"));
    assert!(drives_differ(&d, true));
    assert!(!drives_differ(&d, false));
    let env = Data {
        env: Some(dir.join("E")),
        ..d.clone()
    };
    assert_eq!(env.movable(), cfg!(windows));
    assert!(!drives_differ(&env, true));
    let relative = Data {
        env: Some(PathBuf::from("relative")),
        ..d
    };
    assert!(!drives_differ(&relative, true));
    assert!(!asks(&dir.join("Install"), &data(&dir.join("Local/Swofty"))));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn defaults_to_leave_and_only_moves_when_asked_and_chosen() {
    assert_eq!(Choice::default(), Choice::Leave);
    let install = Path::new("/drive/SwoftyLauncher");
    assert_eq!(target_with(Choice::Leave, true, install, "typed"), "typed");
    assert_eq!(target_with(Choice::Move, false, install, "typed"), "typed");
    let moved = target_with(Choice::Move, true, install, "typed");
    assert_ne!(moved, "typed");
    assert!(moved.ends_with(FOLDER), "{moved}");
    assert!(!Path::new(&moved).starts_with(install));
}

#[test]
fn the_drive_choice_lands_in_a_folder_outside_the_install() {
    let on = on_install_drive(Path::new("/mnt/d/Games/SwoftyLauncher"));
    assert!(!on.starts_with("/mnt/d/Games/SwoftyLauncher"));
    if cfg!(windows) {
        assert_eq!(
            on_install_drive(Path::new(r"D:\Games\SwoftyLauncher")),
            PathBuf::from(r"D:\Swofty")
        );
    }
}

#[test]
fn counts_the_addons_of_a_folder_without_the_managed_one() {
    let dir = temp("count");
    write(&dir.join("addons/a.one/swofty.json"), "{}");
    write(&dir.join("addons/a.two/swofty.json"), "{}");
    write(&dir.join("addons/teamsimplified.mods/swofty.json"), "{}");
    write(&dir.join("addons/no-manifest/readme.txt"), "x");
    assert_eq!(addon_count(&dir), 2);
    assert_eq!(addon_count(&dir.join("missing")), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn the_finder_offers_a_folder_only_when_the_current_one_has_no_addons() {
    let dir = temp("finder");
    let default = dir.join("Local/Swofty");
    std::fs::create_dir_all(&default).unwrap();
    let old = dir.join("Local/Swofty.old");
    write(&old.join("addons/mine.one/swofty.json"), "{\"id\":\"mine.one\"}");
    write(&old.join("profiles/default.json"), "{}");
    let found = finder(&data(&default), &[]);
    assert_eq!(
        found.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
        vec![old.clone()]
    );
    write(&default.join("addons/mine.two/swofty.json"), "{}");
    assert!(finder(&data(&default), &[]).is_empty());
    std::fs::remove_dir_all(&default).unwrap();
    let env = Data {
        env: Some(dir.join("E")),
        ..data(&default)
    };
    assert!(finder(&env, &[]).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn using_a_found_folder_points_the_default_at_it() {
    let dir = temp("adopt");
    let (default, old) = (dir.join("Local/Swofty"), dir.join("D/Old"));
    write(&old.join("addons/mine.one/swofty.json"), "{\"id\":\"mine.one\"}");
    write(&old.join("config.json"), "{}");
    write(&default.join("config.json"), "{}");
    adopt(&data(&default), &old, &[]).unwrap();
    assert_eq!(link_target(&default), Some(old.clone()));
    assert!(read(&default.join("config.json")).contains("{}"));
    let aside = std::fs::read_dir(dir.join("Local"))
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().contains(".fresh-"));
    assert!(aside);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_drive_root_is_refused_as_a_data_folder() {
    let dir = temp("drive-root");
    let root = if cfg!(windows) {
        Path::new(r"D:\")
    } else {
        Path::new("/")
    };
    let e = plan(&data(&dir.join("Local/Swofty")), root).unwrap_err();
    assert!(e.contains("whole drive"), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn data_that_stays_inside_the_install_folder_is_refused_too() {
    let dir = temp("apart-stay");
    let install = dir.join("Install");
    let d = Data {
        install: Some(install.clone()),
        ..data(&install.join("Swofty"))
    };
    assert!(plan(&d, &install.join("Swofty")).is_err());
    let env = Data {
        env: Some(install.join("Swofty")),
        ..d
    };
    assert!(env.apart(&env.current()).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn follows_swofty_data_dir_and_updates_it() {
    let dir = temp("env");
    let (default, old, new) = (dir.join("Local/Swofty"), dir.join("D/Old"), dir.join("E/New"));
    seed(&old);
    let d = Data {
        default: default.clone(),
        env: Some(old.clone()),
        install: None,
    };
    assert_eq!(d.current(), old);
    assert_eq!(plan(&d, &old).unwrap(), None);
    if !d.movable() {
        assert!(plan(&d, &new).unwrap_err().contains(ENV));
        assert!(d.describe(&new).unwrap().contains("Change the variable"));
        std::fs::remove_dir_all(&dir).ok();
        return;
    }
    let records = Ops {
        set_env: |to| {
            *SET.lock().unwrap() = Some(to.to_path_buf());
            Ok(())
        },
        ..SAME_VOLUME
    };
    let m = plan(&d, &new).unwrap().unwrap();
    assert_eq!(m.env(), Some(new.as_path()));
    m.run(&records, &Quiet).unwrap();
    moved_whole(&new);
    assert_eq!(SET.lock().unwrap().as_deref(), Some(new.as_path()));
    assert!(!default.exists(), "no link with {ENV}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn creates_the_chosen_folder_on_a_fresh_install() {
    let dir = temp("fresh");
    let (default, to) = (dir.join("Local/Swofty"), dir.join("D/SwoftyData"));
    let m = plan(&data(&default), &to).unwrap().unwrap();
    assert!(m.describe().starts_with("Creates"));
    m.run(&SAME_VOLUME, &Quiet).unwrap();
    assert!(to.is_dir());
    assert_eq!(link_target(&default), Some(to.clone()));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn puts_a_swofty_folder_inside_a_busy_folder_it_is_given() {
    let dir = temp("pick");
    let d = data(&dir.join("Local/Swofty"));
    write(&dir.join("D/Games/notes.txt"), "x");
    assert_eq!(pick(dir.join("D/Games"), &d), dir.join("D/Games").join(FOLDER));
    std::fs::create_dir_all(dir.join("D/Empty")).unwrap();
    assert_eq!(pick(dir.join("D/Empty"), &d), dir.join("D/Empty"));
    assert_eq!(pick(dir.join("D").join(FOLDER), &d), dir.join("D").join(FOLDER));
    std::fs::remove_dir_all(&dir).ok();
}
