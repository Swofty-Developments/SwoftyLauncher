use super::*;
use std::io::ErrorKind;

use crate::existing::lnk::{HAS_ID_LIST, HAS_LINK_INFO, HAS_WORKING_DIR, IS_UNICODE, lnk_roots, read_lnk};
use crate::existing::roots::{collect, root_of};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-installer-{name}-{}", std::process::id()));
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

fn release(root: &Path) {
    write(&root.join("bin").join(update::exe("swofty-launcher")), "launcher");
    write(&root.join("bin").join(update::exe("swofty")), "cli");
    write(&root.join("loader/version.dll"), "proxy");
    write(&root.join("loader/swofty_core.dll"), "core");
    write(&root.join("dxc/bin").join(update::exe("dxc")), "dxc");
    write(&root.join("share/icons/hicolor/256x256/apps/icon.png"), "icon");
    write(&root.join("LICENSE"), "license");
    write(&root.join("README.md"), "readme");
}

#[test]
fn recognises_an_install_by_its_launcher_and_loader() {
    let dir = temp("recognise");
    let full = dir.join("full");
    release(&full);
    assert!(is_install(&full));
    let no_loader = dir.join("no-loader");
    write(
        &no_loader.join("bin").join(update::exe("swofty-launcher")),
        "launcher",
    );
    assert!(!is_install(&no_loader));
    let no_launcher = dir.join("no-launcher");
    write(&no_launcher.join("loader/version.dll"), "proxy");
    assert!(!is_install(&no_launcher));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn lists_each_install_once_in_the_order_found() {
    let dir = temp("collect");
    let (a, b) = (dir.join("a"), dir.join("b"));
    release(&a);
    release(&b);
    let again = PathBuf::from(format!("{}/", a.display()));
    let found = collect([dir.join("missing"), b.clone(), a.clone(), again, dir.clone()]);
    assert_eq!(found, [b, a]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn finds_the_install_a_launcher_runs_from() {
    let exe = Path::new("/games/SwoftyLauncher/bin").join(update::exe("swofty-launcher"));
    assert_eq!(root_of(&exe), Some(PathBuf::from("/games/SwoftyLauncher")));
    assert_eq!(root_of(Path::new("/games/swofty-launcher")), None);
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn string_data(s: &str) -> Vec<u8> {
    let mut out = (s.encode_utf16().count() as u16).to_le_bytes().to_vec();
    out.extend(utf16(s));
    out
}

fn file_item(name: &str, last: bool) -> Vec<u8> {
    let ascii = name.is_ascii();
    let kind = match (last, ascii) {
        (false, true) => 0x31,
        (false, false) => 0x35,
        (true, true) => 0x32,
        (true, false) => 0x36,
    };
    let mut item = vec![0, 0, kind, 0];
    item.extend([0u8; 10]);
    if ascii {
        item.extend(name.as_bytes());
        item.push(0);
    } else {
        item.extend(utf16(name));
        item.extend([0, 0]);
    }
    item.extend([0x1a, 0, 8, 0, 0x04, 0, 0xef, 0xbe]);
    item.extend([0u8; 14]);
    item.extend(utf16(name));
    item.extend([0, 0, 0x14, 0]);
    let size = item.len() as u16;
    item[..2].copy_from_slice(&size.to_le_bytes());
    item
}

fn mslnk(target: &str, working_dir: &str) -> Vec<u8> {
    let mut data = vec![0u8; 0x4C];
    data[0] = 0x4C;
    let flags = (HAS_ID_LIST | 0x8 | HAS_WORKING_DIR | IS_UNICODE) as u32;
    data[0x14..0x18].copy_from_slice(&flags.to_le_bytes());
    let mut list = vec![
        0x14, 0x00, 0x1F, 0x50, 0xE0, 0x4F, 0xD0, 0x20, 0xEA, 0x3A, 0x69, 0x10, 0xA2, 0xD8, 0x08, 0x00, 0x2B,
        0x30, 0x30, 0x9D,
    ];
    let (drive, rest) = target.split_at(3);
    let mut drive_item = vec![0x19, 0x00, 0x2f];
    drive_item.extend(drive.as_bytes());
    drive_item.resize(25, 0);
    list.extend(drive_item);
    let parts: Vec<&str> = rest.split('\\').collect();
    for (i, part) in parts.iter().enumerate() {
        list.extend(file_item(part, i + 1 == parts.len()));
    }
    list.extend([0, 0]);
    data.extend((list.len() as u16).to_le_bytes());
    data.extend(list);
    data.extend(string_data("./swofty-launcher.exe"));
    data.extend(string_data(working_dir));
    data
}

#[test]
fn reads_where_an_installer_shortcut_points() {
    let target = r"C:\Users\Jöns\Games\SwoftyLauncher\bin\swofty-launcher.exe";
    let link = read_lnk(&mslnk(target, r"C:\Users\Jöns\Games\SwoftyLauncher")).unwrap();
    assert_eq!(link.target.as_deref(), Some(target));
    assert_eq!(
        link.working_dir.as_deref(),
        Some(r"C:\Users\Jöns\Games\SwoftyLauncher")
    );
}

#[test]
fn prefers_the_path_windows_records_in_link_info() {
    let mut data = vec![0u8; 0x4C];
    data[0] = 0x4C;
    data[0x14..0x18].copy_from_slice(&((HAS_LINK_INFO | IS_UNICODE) as u32).to_le_bytes());
    let base = r"D:\Games\SwoftyLauncher\bin\swofty-launcher.exe";
    let mut info = vec![0u8; 0x24];
    info[4..8].copy_from_slice(&0x24u32.to_le_bytes());
    info[8..12].copy_from_slice(&1u32.to_le_bytes());
    info[16..20].copy_from_slice(&0x24u32.to_le_bytes());
    info[24..28].copy_from_slice(&0x24u32.to_le_bytes());
    info.extend([b'X', 0]);
    let unicode = info.len() as u32;
    info[28..32].copy_from_slice(&unicode.to_le_bytes());
    info.extend(utf16(base));
    info.extend([0, 0]);
    let suffix = info.len() as u32;
    info[32..36].copy_from_slice(&suffix.to_le_bytes());
    info.extend([0, 0]);
    let size = info.len() as u32;
    info[..4].copy_from_slice(&size.to_le_bytes());
    data.extend(info);
    let link = read_lnk(&data).unwrap();
    assert_eq!(link.target.as_deref(), Some(base));
    assert_eq!(link.working_dir, None);
}

#[test]
fn ignores_files_that_are_not_shortcuts() {
    assert_eq!(read_lnk(b"[Desktop Entry]\nExec=x\n"), None);
    let whole = mslnk(r"C:\SwoftyLauncher\bin\swofty-launcher.exe", r"C:\SwoftyLauncher");
    for cut in [0x10, 0x4C, 0x60, whole.len() - 4] {
        assert_eq!(read_lnk(&whole[..cut]), None);
    }
    assert!(lnk_roots(&whole[..0x60]).is_empty());
}

#[cfg(not(windows))]
#[test]
fn reads_back_the_menu_entry_it_writes() {
    use crate::existing::roots::desktop_roots;
    use crate::shortcut;

    let root = PathBuf::from("/home/a b/$HOME \"q\" `x` 100%/SwoftyLauncher");
    let exe = root.join("bin/swofty-launcher");
    let entry = shortcut::entry(&root, &exe);
    assert_eq!(desktop_roots(&entry), [root.clone(), root]);
    assert_eq!(
        desktop_roots("[Desktop Entry]\nExec=/opt/SL/bin/swofty-launcher %u\nPath=\n"),
        [PathBuf::from("/opt/SL")]
    );
}

#[test]
fn plans_the_release_files_and_never_carries_a_data_folder() {
    let dir = temp("plan");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    write(&from.join("notes.txt"), "mine");
    write(&from.join("Swofty/profiles/default.json"), "{}");
    let names = |m: &Move| -> Vec<String> { m.entries.iter().map(|e| e.display().to_string()).collect() };
    let release_only = ["bin", "loader", "dxc", "share", "LICENSE", "README.md"];
    let absolute = plan(&from, &to, &from.join("Swofty")).unwrap();
    assert_eq!(names(&absolute), release_only);
    assert_eq!(
        names(&plan(&from, &to, Path::new("../Swofty")).unwrap()),
        release_only
    );
    let relative = plan(&from, &to, Path::new("./Swofty")).unwrap();
    assert_eq!(names(&relative), release_only);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn refuses_a_move_onto_an_install_into_itself_or_over_files() {
    let dir = temp("refuse");
    let from = dir.join("old");
    release(&from);
    let data = Path::new("/elsewhere/swofty");
    assert!(plan(&dir.join("none"), &dir.join("new"), data).is_err());
    let other = dir.join("other");
    release(&other);
    assert!(plan(&from, &other, data).is_err());
    assert!(plan(&from, &from.join("bin/deeper"), data).is_err());
    let busy = dir.join("busy");
    write(&busy.join("loader/old.dll"), "x");
    assert!(plan(&from, &busy, data).is_err());
    assert!(plan(&from, &dir.join("new"), data).is_ok());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn moves_an_install_and_removes_the_old_folder() {
    let dir = temp("move");
    let (from, to) = (dir.join("old"), dir.join("deeper/new"));
    release(&from);
    let m = plan(&from, &to, Path::new("/elsewhere/swofty")).unwrap();
    m.relocate(|a, b| std::fs::rename(a, b)).unwrap();
    assert!(is_install(&to));
    assert_eq!(read(&to.join("loader/swofty_core.dll")), "core");
    assert!(!from.exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn keeps_the_old_folder_while_it_holds_other_files() {
    let dir = temp("keep");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    write(&from.join("notes.txt"), "mine");
    plan(&from, &to, Path::new("/elsewhere/swofty"))
        .unwrap()
        .relocate(|a, b| std::fs::rename(a, b))
        .unwrap();
    assert!(is_install(&to));
    assert!(!to.join("notes.txt").exists());
    assert_eq!(read(&from.join("notes.txt")), "mine");
    assert_eq!(std::fs::read_dir(&from).unwrap().count(), 1);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn copies_to_another_drive_and_removes_the_old_files_after_checking() {
    let dir = temp("copy");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    let other_drive = |_: &Path, _: &Path| Err(std::io::Error::from(ErrorKind::CrossesDevices));
    plan(&from, &to, Path::new("/elsewhere/swofty"))
        .unwrap()
        .relocate(other_drive)
        .unwrap();
    assert!(is_install(&to));
    assert_eq!(read(&to.join("bin").join(update::exe("swofty"))), "cli");
    assert_eq!(
        read(&to.join("share/icons/hicolor/256x256/apps/icon.png")),
        "icon"
    );
    assert!(!from.exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn puts_everything_back_when_a_step_fails() {
    let dir = temp("undo");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    let locked = |a: &Path, b: &Path| {
        if a.ends_with("share") {
            Err(std::io::Error::from(ErrorKind::PermissionDenied))
        } else {
            std::fs::rename(a, b)
        }
    };
    let err = plan(&from, &to, Path::new("/elsewhere/swofty"))
        .unwrap()
        .relocate(locked)
        .unwrap_err();
    assert!(err.contains("share"), "{err}");
    assert!(is_install(&from));
    assert_eq!(read(&from.join("loader/version.dll")), "proxy");
    assert!(!to.exists());
    std::fs::remove_dir_all(&dir).ok();
}

fn with_user_data(root: &Path) {
    release(root);
    write(&root.join("addons/swofty.progressive/swofty.json"), "{}");
    write(&root.join("profiles/default.json"), "{}");
    write(&root.join("saves/a.sav"), "save");
    write(&root.join("config.json"), "{}");
}

fn kept(root: &Path) {
    for rel in [
        "addons/swofty.progressive/swofty.json",
        "profiles/default.json",
        "saves/a.sav",
        "config.json",
    ] {
        assert!(root.join(rel).is_file(), "{rel} left {}", root.display());
    }
}

#[test]
fn moving_the_install_leaves_addons_when_the_data_folder_is_the_install_root() {
    let dir = temp("same-root");
    let (from, to) = (dir.join("shared"), dir.join("new"));
    with_user_data(&from);
    for data in [from.clone(), from.join("."), PathBuf::from(&from)] {
        let m = plan(&from, &to, &data).unwrap();
        assert!(
            !m.entries.iter().any(|e| e == Path::new("addons")),
            "{:?}",
            m.entries
        );
    }
    plan(&from, &to, &from)
        .unwrap()
        .relocate(|a, b| std::fs::rename(a, b))
        .unwrap();
    assert!(is_install(&to));
    assert!(!to.join("addons").exists());
    kept(&from);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn moving_the_install_with_data_in_a_subfolder_leaves_it_in_place() {
    let dir = temp("subfolder");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    write(&from.join("Swofty/addons/swofty.progressive/swofty.json"), "{}");
    write(&from.join("Swofty/profiles/default.json"), "{}");
    plan(&from, &to, &from.join("Swofty"))
        .unwrap()
        .relocate(|a, b| std::fs::rename(a, b))
        .unwrap();
    assert!(is_install(&to));
    assert!(
        from.join("Swofty/addons/swofty.progressive/swofty.json")
            .is_file()
    );
    assert!(from.join("Swofty/profiles/default.json").is_file());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_absolute_data_path_is_not_carried() {
    let dir = temp("absolute");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    let m = plan(&from, &to, &from.join("Swofty")).unwrap();
    assert!(!m.entries.iter().any(|e| e == Path::new("Swofty")));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn data_elsewhere_still_moves_the_bundled_addons_with_the_install() {
    let dir = temp("bundled");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    write(&from.join("addons/swofty.bundled/swofty.json"), "{}");
    let m = plan(&from, &to, &dir.join("data")).unwrap();
    assert!(m.entries.iter().any(|e| e == Path::new("addons")));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn an_existing_install_is_always_moved_unless_a_second_copy_is_asked_for() {
    let dir = temp("always-moves");
    let (from, to) = (dir.join("old"), dir.join("new"));
    release(&from);
    let mut existing = Existing {
        found: vec![from.clone()],
        picked: 0,
        keep: false,
    };
    assert!(existing.planned(&to).unwrap().is_some());
    assert!(existing.planned(&from).unwrap().is_none());
    assert!(existing.differs_from(&to));
    existing.keep = true;
    assert!(existing.planned(&to).unwrap().is_none());
    std::fs::remove_dir_all(&dir).ok();
}
