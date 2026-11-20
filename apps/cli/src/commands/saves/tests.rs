use std::path::{Path, PathBuf};

use swofty_core::GAME_EXE;
use swofty_core::game_folder::baseline::Identity;
use swofty_core::install::Game;
use swofty_core::saves::vault::Reason;

use super::session::before_session_at;
use super::*;

const DATA: &str = "savegame@profile#NXDSMWW.BWW";
const INDEX: &str = "savegame@profile#QKRHMYXE";

struct Fixture {
    root: PathBuf,
    store: Store,
    game: Game,
    steam: Location,
}

fn fixture(name: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!("swofty-cli-saves-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(GAME_EXE), "exe").unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir,
        user_reg: None,
    };
    let remote = root.join("steam/userdata/843502855/3059520/remote");
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join(DATA), [1u8; 900]).unwrap();
    std::fs::write(remote.join(INDEX), [7u8; 416]).unwrap();
    Fixture {
        store: Store::new(root.join("data")),
        steam: Location::steam(&remote).unwrap(),
        game,
        root,
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn the_first_play_copies_the_steam_save_into_an_empty_store_and_plays_on_it() {
    let f = fixture("first");
    let b = before_session_at(
        &f.store,
        "default",
        &f.game,
        "s1",
        None,
        false,
        Some(f.steam.clone()),
    )
    .unwrap();
    let store = launcher_saves::remote_dir(&f.store.root, "default");
    assert_eq!(b.loc.layout, vault::Layout::Launcher);
    assert_eq!(b.loc.dir, store);
    assert_eq!(names(&store), [DATA, INDEX]);
    assert_eq!(std::fs::read(store.join(DATA)).unwrap(), vec![1u8; 900]);
    assert_eq!(names(&f.steam.dir), [DATA, INDEX]);
    let snaps = Vault::open(&f.store.root, "launcher-default")
        .unwrap()
        .list()
        .unwrap();
    assert!(
        snaps
            .iter()
            .any(|s| s.reason == Reason::Imported && s.layout == vault::Layout::Launcher)
    );
    std::fs::remove_dir_all(&f.root).ok();
}

#[test]
fn a_store_with_files_is_not_reseeded() {
    let f = fixture("kept");
    let store = launcher_saves::remote_dir(&f.store.root, "default");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(store.join(DATA), b"launcher career").unwrap();
    let b = before_session_at(
        &f.store,
        "default",
        &f.game,
        "s1",
        None,
        false,
        Some(f.steam.clone()),
    )
    .unwrap();
    assert_eq!(b.loc.layout, vault::Layout::Launcher);
    assert_eq!(names(&store), [DATA]);
    assert_eq!(std::fs::read(store.join(DATA)).unwrap(), b"launcher career");
    std::fs::remove_dir_all(&f.root).ok();
}

#[test]
fn one_session_on_steam_saves_and_a_profile_with_the_setting_off_use_steam() {
    let f = fixture("steam");
    let b = before_session_at(
        &f.store,
        "default",
        &f.game,
        "s1",
        None,
        true,
        Some(f.steam.clone()),
    )
    .unwrap();
    assert_eq!(b.loc.layout, vault::Layout::Steam);
    assert!(!launcher_saves::remote_dir(&f.store.root, "default").exists());
    f.store
        .update_profile("default", |p| {
            p.launcher_saves = false;
            Ok(())
        })
        .unwrap();
    let b = before_session_at(
        &f.store,
        "default",
        &f.game,
        "s2",
        None,
        false,
        Some(f.steam.clone()),
    )
    .unwrap();
    assert_eq!(b.loc.layout, vault::Layout::Steam);
    assert!(!launcher_saves::remote_dir(&f.store.root, "default").exists());
    std::fs::remove_dir_all(&f.root).ok();
}

#[test]
fn a_profile_with_launcher_saves_off_leaves_an_after_play_version_of_the_steam_save() {
    let f = fixture("after-steam");
    f.store
        .update_profile("default", |p| {
            p.launcher_saves = false;
            Ok(())
        })
        .unwrap();
    let b = before_session_at(
        &f.store,
        "default",
        &f.game,
        "s1",
        None,
        false,
        Some(f.steam.clone()),
    )
    .unwrap();
    std::fs::write(f.steam.dir.join(DATA), [9u8; 1200]).unwrap();
    after_session(Some(b));
    let snaps = Vault::of(&f.store.root, &f.steam).unwrap().list().unwrap();
    let last = snaps.iter().find(|s| s.reason == Reason::AfterSession).unwrap();
    assert_eq!(last.layout, vault::Layout::Steam);
    assert_eq!(last.session_id.as_deref(), Some("s1"));
    std::fs::remove_dir_all(&f.root).ok();
}

#[test]
fn the_vanilla_profile_always_plays_on_steam_saves() {
    let f = fixture("vanilla");
    let b = before_session_at(
        &f.store,
        "vanilla",
        &f.game,
        "s1",
        None,
        false,
        Some(f.steam.clone()),
    )
    .unwrap();
    assert_eq!(b.loc.layout, vault::Layout::Steam);
    assert!(!launcher_saves::root(&f.store.root).exists());
    std::fs::remove_dir_all(&f.root).ok();
}

#[test]
fn the_ea_copy_plays_on_the_launcher_saves_of_its_profile() {
    let f = fixture("ea");
    let ea_dir = f
        .root
        .join("pfx/drive_c/users/steamuser/AppData/Local/F1 25/savegame/ea/profile");
    std::fs::create_dir_all(&ea_dir).unwrap();
    std::fs::write(ea_dir.join("NXDSMWW.BWW"), [3u8; 64]).unwrap();
    let ea = Location::ea(&ea_dir, "steamuser");
    let b = before_session_at(&f.store, "default", &f.game, "s1", None, false, Some(ea)).unwrap();
    assert_eq!(b.loc.layout, vault::Layout::LauncherEa);
    let stored = launcher_saves::ea_dir(&f.store.root, "default");
    assert_eq!(std::fs::read(stored.join("NXDSMWW.BWW")).unwrap(), [3u8; 64]);
    std::fs::remove_dir_all(&f.root).ok();
}
