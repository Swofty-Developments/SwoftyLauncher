use super::*;

const RELEASE: [&str; 5] = [
    "loader/version.dll",
    "dxc/dxcompiler.dll",
    "LICENSE",
    "README.md",
    "THIRD_PARTY.md",
];

fn release(root: &Path) {
    write(
        &root
            .join("bin")
            .join(crate::updates::release::exe("swofty-launcher")),
        "launcher",
    );
    for rel in RELEASE {
        write(&root.join(rel), rel);
    }
}

fn release_whole(root: &Path) {
    assert_eq!(
        read(
            &root
                .join("bin")
                .join(crate::updates::release::exe("swofty-launcher"))
        ),
        "launcher"
    );
    for rel in RELEASE {
        assert_eq!(read(&root.join(rel)), rel, "{rel} in {}", root.display());
    }
}

fn located(default: &Path, install: &Path) -> Location {
    Location {
        install: Some(install.to_path_buf()),
        ..loc(default)
    }
}

fn untouched(dir: &Path, data: &Path, to: &Path) {
    whole(data);
    assert!(!to.exists());
    assert!(!journal_left(&dir.join("Local/Swofty")));
}

fn shared_root(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let dir = temp(name);
    let (default, install) = (dir.join("Local/Swofty"), dir.join("E/Modowanie F1"));
    release(&install);
    seed(&install);
    std::fs::create_dir_all(default.parent().unwrap()).unwrap();
    link::create(&install, &default).unwrap();
    (dir, default, install)
}

#[test]
fn a_data_folder_that_is_the_install_folder_is_not_moved() {
    let (dir, default, install) = shared_root("is-install");
    let to = dir.join("D/Data");
    for ops in [SAME_VOLUME, OTHER_DRIVE] {
        let err = plan(&located(&default, &install), &to, &ops).unwrap_err();
        assert_eq!(err, Error::IsInstall(real_path(&install)));
    }
    untouched(&dir, &install, &to);
    release_whole(&install);
    assert!(same_path(&link::target(&default).unwrap(), &install));
}

#[test]
fn the_refusal_names_the_folder_the_link_resolves_to_and_the_installer() {
    let (dir, default, install) = shared_root("is-install-text");
    let text = plan(&located(&default, &install), &dir.join("D/Data"), &SAME_VOLUME)
        .unwrap_err()
        .to_string();
    assert!(
        text.contains(&real_path(&install).display().to_string()),
        "{text}"
    );
    assert!(!text.contains(&default.display().to_string()), "{text}");
    assert!(text.contains("installer"), "{text}");
    assert!(text.contains("new, empty folder"), "{text}");
}

#[test]
fn a_default_folder_that_is_the_install_folder_is_not_moved() {
    let dir = temp("default-is-install");
    let default = dir.join("Local/Swofty");
    release(&default);
    seed(&default);
    let to = dir.join("D/Data");
    assert_eq!(
        refused(&located(&default, &default), &to),
        Error::IsInstall(real_path(&default))
    );
    untouched(&dir, &default, &to);
    release_whole(&default);
}

#[test]
fn an_install_folder_reached_through_a_link_or_a_dot_is_still_the_data_folder() {
    let (dir, default, install) = shared_root("install-alias");
    let alias = dir.join("Alias");
    link::create(&install, &alias).unwrap();
    let to = dir.join("D/Data");
    for given in [alias.clone(), default.clone(), install.join(".")] {
        assert_eq!(
            refused(&located(&default, &given), &to),
            Error::IsInstall(real_path(&install)),
            "{}",
            given.display()
        );
    }
    untouched(&dir, &install, &to);
}

#[test]
fn a_data_folder_that_holds_the_install_folder_is_not_moved() {
    let dir = temp("holds-install");
    let (default, data) = (dir.join("Local/Swofty"), dir.join("D/Swofty"));
    seed(&data);
    let install = data.join("SwoftyLauncher");
    release(&install);
    std::fs::create_dir_all(default.parent().unwrap()).unwrap();
    link::create(&data, &default).unwrap();
    let to = dir.join("E/Data");
    for ops in [SAME_VOLUME, OTHER_DRIVE] {
        assert_eq!(
            plan(&located(&default, &install), &to, &ops).unwrap_err(),
            Error::HoldsInstall {
                data: real_path(&data),
                install: real_path(&install)
            }
        );
    }
    untouched(&dir, &data, &to);
    release_whole(&install);
}

#[test]
fn a_default_folder_that_holds_the_install_folder_is_not_moved() {
    let dir = temp("default-holds-install");
    let default = dir.join("Local/Swofty");
    seed(&default);
    let install = default.join("app");
    release(&install);
    let to = dir.join("D/Data");
    let err = refused(&located(&default, &install), &to);
    assert_eq!(
        err,
        Error::HoldsInstall {
            data: real_path(&default),
            install: real_path(&install)
        }
    );
    assert!(err.to_string().contains("installer"), "{err}");
    untouched(&dir, &default, &to);
    release_whole(&install);
}

fn data_in_a_subfolder(name: &str) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let dir = temp(name);
    let (default, install) = (dir.join("Local/Swofty"), dir.join("E/F1"));
    release(&install);
    let data = install.join("Swofty");
    seed(&data);
    std::fs::create_dir_all(default.parent().unwrap()).unwrap();
    link::create(&data, &default).unwrap();
    (dir, default, install, data)
}

#[test]
fn a_data_folder_inside_the_install_folder_moves_out_by_rename_without_the_launcher() {
    let (dir, default, install, data) = data_in_a_subfolder("inside-rename");
    let to = dir.join("D/Data");
    let p = plan(&located(&default, &install), &to, &SAME_VOLUME).unwrap();
    assert!(same_path(&p.from, &data));
    let moved = move_data(&p, &SAME_VOLUME, &Quiet, &no_cancel()).unwrap();
    assert_eq!((moved.method, moved.switched), (Method::Renamed, true));
    whole(&to);
    assert!(same_path(&link::target(&default).unwrap(), &to));
    assert!(!data.exists());
    release_whole(&install);
    assert!(!to.join("bin").exists() && !to.join("loader").exists());
}

#[test]
fn a_data_folder_inside_the_install_folder_moves_to_another_drive_without_the_launcher() {
    let (dir, default, install, data) = data_in_a_subfolder("inside-copy");
    let to = dir.join("D/Data");
    let p = plan(&located(&default, &install), &to, &OTHER_DRIVE).unwrap();
    let moved = move_data(&p, &OTHER_DRIVE, &Quiet, &no_cancel()).unwrap();
    assert_eq!((moved.method, moved.switched), (Method::Copied, false));
    release_whole(&install);
    let done = recover(&default, &OTHER_DRIVE).unwrap().unwrap();
    assert!(same_path(&done.to, &to));
    assert_eq!(done.leftover, None);
    whole(&to);
    assert!(same_path(&link::target(&default).unwrap(), &to));
    assert!(!data.exists());
    release_whole(&install);
    assert!(!to.join("bin").exists() && !to.join("loader").exists());
}

#[test]
fn a_target_inside_the_install_folder_is_still_refused_with_data_beside_it() {
    let (_, default, install, data) = data_in_a_subfolder("inside-target");
    assert_eq!(
        refused(&located(&default, &install), &install.join("Data")),
        Error::InsideInstall(install.clone())
    );
    assert!(!install.join("Data").exists());
    whole(&data);
}

#[cfg(windows)]
#[test]
fn an_install_folder_written_in_another_case_is_still_the_data_folder() {
    let (dir, default, install) = shared_root("install-case");
    let shouted = PathBuf::from(install.to_string_lossy().to_uppercase());
    let to = dir.join("D/Data");
    assert_eq!(
        refused(&located(&default, &shouted), &to),
        Error::IsInstall(real_path(&install))
    );
    untouched(&dir, &install, &to);
}

#[cfg(windows)]
#[test]
fn the_real_path_drops_the_verbatim_prefix() {
    let (_, default, install) = shared_root("real-path");
    let shown = real_path(&default).display().to_string();
    assert!(!shown.starts_with(r"\\?\"), "{shown}");
    assert!(same_path(&real_path(&default), &install));
}
