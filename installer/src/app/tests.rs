use std::path::Path;

use swofty_core::session::COMPAT_LAYER;
use swofty_core::updates::release as update;

use super::install::{command, install_folder};
use super::view::addons_text;
use crate::{FOLDER_NAME, data, resolve};

#[test]
fn starts_the_launcher_in_its_folder_without_the_compatibility_layers() {
    let root = Path::new("SwoftyLauncher");
    let c = command(&root.join("bin").join(update::exe("swofty-launcher")), None);
    assert_eq!(c.get_current_dir(), Some(root));
    assert!(c.get_envs().any(|(k, v)| k == COMPAT_LAYER && v.is_none()));
}

#[test]
fn hands_the_launcher_the_moved_data_folder() {
    let exe = Path::new("SwoftyLauncher")
        .join("bin")
        .join(update::exe("swofty-launcher"));
    let c = command(&exe, Some(Path::new("D:/Swofty")));
    assert!(
        c.get_envs()
            .any(|(k, v)| k == data::ENV && v == Some(Path::new("D:/Swofty").as_os_str()))
    );
    assert!(!command(&exe, None).get_envs().any(|(k, _)| k == data::ENV));
}

#[test]
fn counts_addons_in_words() {
    assert_eq!(addons_text(0), "0 addons");
    assert_eq!(addons_text(1), "1 addon");
    assert_eq!(addons_text(12), "12 addons");
}

#[test]
fn resolves_typed_install_folders() {
    assert_eq!(resolve("  "), Err("Choose a folder to install in.".to_owned()));
    let home = dirs::home_dir().unwrap();
    assert_eq!(resolve("~"), Ok(home.clone()));
    assert_eq!(resolve(" ~/Games/SL "), Ok(home.join("Games/SL")));
    assert_eq!(resolve("~other"), Ok(std::path::absolute("~other").unwrap()));
    assert_eq!(
        resolve("rel/SL"),
        Ok(std::env::current_dir().unwrap().join("rel/SL"))
    );
}

#[test]
fn installs_into_a_named_folder_unless_the_pick_already_is_one() {
    let dir = std::env::temp_dir().join(format!("swofty-installer-main-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let empty = dir.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(install_folder(empty.clone()), empty);
    let full = dir.join("full");
    std::fs::create_dir_all(&full).unwrap();
    std::fs::write(full.join("notes.txt"), "x").unwrap();
    assert_eq!(install_folder(full.clone()), full.join(FOLDER_NAME));
    let installed = dir.join("installed");
    std::fs::create_dir_all(installed.join("bin")).unwrap();
    std::fs::write(installed.join("bin").join(update::exe("swofty-launcher")), "").unwrap();
    assert_eq!(install_folder(installed.clone()), installed);
    let named = full.join("swoftylauncher");
    std::fs::create_dir_all(&named).unwrap();
    std::fs::write(named.join("notes.txt"), "x").unwrap();
    assert_eq!(install_folder(named.clone()), named);
    assert_eq!(
        install_folder(dir.join("missing")),
        dir.join("missing").join(FOLDER_NAME)
    );
    std::fs::remove_dir_all(&dir).ok();
}
