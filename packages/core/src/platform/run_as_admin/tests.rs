use std::path::Path;

use super::compat::*;
use super::write_access::*;
use super::*;
use crate::GAME_EXE;

const LISTING: &str = concat!(
    "\r\n",
    "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers\r\n",
    "    C:\\Program Files (x86)\\Steam\\steamapps\\common\\F1 25\\F1_25.exe    REG_SZ    ~ RUNASADMIN\r\n",
    "    C:\\Games\\Other\\other.exe    REG_SZ    ~ HIGHDPIAWARE\r\n",
    "    C:\\Games\\Tool\\tool.exe    REG_EXPAND_SZ    RUNASADMIN WIN7RTM\r\n",
    "    C:\\Games\\Empty\\empty.exe    REG_SZ    \r\n",
    "    C:\\Games\\Flag\\flag.exe    REG_DWORD    0x1\r\n",
    "\r\n",
);

#[test]
fn parses_reg_query_listings() {
    let values = parse_reg_query(LISTING);
    assert_eq!(
        values,
        [
            (
                r"C:\Program Files (x86)\Steam\steamapps\common\F1 25\F1_25.exe",
                "~ RUNASADMIN"
            ),
            (r"C:\Games\Other\other.exe", "~ HIGHDPIAWARE"),
            (r"C:\Games\Tool\tool.exe", "RUNASADMIN WIN7RTM"),
            (r"C:\Games\Empty\empty.exe", ""),
        ]
        .map(|(n, d)| (n.to_owned(), d.to_owned()))
    );
    assert!(
        parse_reg_query("ERROR: The system was unable to find the specified registry key or value.\r\n")
            .is_empty()
    );
}

#[test]
fn reads_dword_values() {
    let listing = "\r\nHKEY_CURRENT_USER\\Software\\Valve\\Steam\\ActiveProcess\r\n    \
                   pid    REG_DWORD    0x4c8\r\n    ActiveUser    REG_DWORD    0x0\r\n    \
                   Universe    REG_SZ    Public\r\n";
    assert_eq!(reg_dword(listing, "pid"), Some(0x4c8));
    assert_eq!(reg_dword(listing, "activeuser"), Some(0));
    assert_eq!(reg_dword(listing, "Universe"), None);
    assert_eq!(reg_dword(listing, "SteamPID"), None);
}

#[test]
fn reads_the_runasadmin_layer() {
    assert!(has_run_as_admin("~ RUNASADMIN"));
    assert!(has_run_as_admin("RUNASADMIN HIGHDPIAWARE"));
    assert!(has_run_as_admin("~ HIGHDPIAWARE runasadmin"));
    assert!(!has_run_as_admin("~ HIGHDPIAWARE"));
    assert!(!has_run_as_admin("~ RUNASINVOKER"));
    assert!(!has_run_as_admin("~ NORUNASADMIN"));
    assert!(!has_run_as_admin(""));
}

#[test]
fn reads_any_compatibility_layer() {
    assert!(has_layers("~ HIGHDPIAWARE"));
    assert!(has_layers("WIN7RTM"));
    assert!(!has_layers("~"));
    assert!(!has_layers(""));
    let other = matching_with(LISTING, false, Path::new(r"C:\Games\Other\other.exe"), has_layers);
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].layers, "~ HIGHDPIAWARE");
    assert!(matching(LISTING, false, Path::new(r"C:\Games\Other\other.exe")).is_empty());
    assert!(matching_with(LISTING, false, Path::new(r"C:\Games\Empty\empty.exe"), has_layers).is_empty());
}

#[test]
fn finds_the_layers_on_the_programs_that_start_the_game() {
    let game = Path::new(r"D:\Games\F1 25");
    let user = concat!(
        "\r\n",
        "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers\r\n",
        "    C:\\Tools\\Swofty Launcher\\swofty-launcher.exe    REG_SZ    ~ WIN8RTM\r\n",
        "    D:\\Games\\F1 25\\F1_25.exe    REG_SZ    ~ HIGHDPIAWARE\r\n",
        "    D:\\Battlefield\\EAAntiCheat.GameServiceLauncher.exe    REG_SZ    ~ RUNASADMIN\r\n",
        "    d:\\games\\f1 25\\eaanticheat.gameservicelauncher.exe    REG_SZ    WIN7RTM\r\n",
        "    C:\\Other\\EADesktop.exe.bak    REG_SZ    ~ RUNASADMIN\r\n",
        "    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EALauncher.exe    REG_SZ    ~\r\n",
    );
    let machine = concat!(
        "\r\n",
        "HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers\r\n",
        "    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\eadesktop.exe    REG_SZ    ~ RUNASADMIN\r\n",
    );
    let found = starter_layers_in(&[(user.into(), false), (machine.into(), true)], game);
    let seen: Vec<(&str, &str, bool)> = found
        .iter()
        .map(|l| (l.name.as_str(), l.layers.as_str(), l.all_users))
        .collect();
    assert_eq!(
        seen,
        [
            ("EADesktop.exe", "~ RUNASADMIN", true),
            ("swofty-launcher.exe", "~ WIN8RTM", false),
            ("EAAntiCheat.GameServiceLauncher.exe", "WIN7RTM", false),
        ]
    );
    assert_eq!(
        found[0].exe,
        r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\eadesktop.exe"
    );
    assert!(starter_layers_in(&[(LISTING.into(), false)], game).is_empty());
    assert!(starter_layers_in(&[], game).is_empty());
}

#[test]
fn matches_the_game_however_the_path_is_spelled() {
    let game = Path::new(r"c:\program files (x86)\steam\steamapps\common\F1 25");
    let found = matching(LISTING, false, &game.join(GAME_EXE));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, GAME_EXE);
    assert_eq!(found[0].layers, "~ RUNASADMIN");
    assert!(!found[0].all_users);
    assert!(matching(LISTING, false, Path::new(r"C:\Games\Other\other.exe")).is_empty());
    let slashes = Path::new("C:/Program Files (x86)/Steam/steamapps/common/F1 25/F1_25.exe");
    assert_eq!(matching(LISTING, true, slashes).len(), 1);
}

#[cfg(unix)]
#[test]
fn matches_the_game_through_a_linked_folder() {
    let d = std::env::temp_dir().join(format!("swofty-compat-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(d.join("F1 25")).unwrap();
    std::fs::write(d.join("F1 25").join(GAME_EXE), "MZ").unwrap();
    std::os::unix::fs::symlink(d.join("F1 25"), d.join("library")).unwrap();
    let linked = d.join("library").join(GAME_EXE);
    let listing = format!("    {}    REG_SZ    ~ RUNASADMIN\n", linked.display());
    let found = matching(&listing, false, &d.join("F1 25").join(GAME_EXE));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].exe, linked.display().to_string());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn says_what_to_untick() {
    let game = Path::new(r"C:\Games\F1 25");
    let user = AdminLayer {
        all_users: false,
        name: GAME_EXE.into(),
        exe: r"C:\Games\F1 25\F1_25.exe".into(),
        layers: "~ RUNASADMIN".into(),
    };
    let text = explain(std::slice::from_ref(&user), game);
    assert!(
        text.starts_with("F1_25.exe is set to run as administrator (for this user)"),
        "{text}"
    );
    assert!(
        text.contains(r"Right-click F1_25.exe in C:\Games\F1 25"),
        "{text}"
    );
    assert!(
        text.contains("untick \"Run this program as an administrator\", then press Play"),
        "{text}"
    );
    assert!(
        text.ends_with("or run Swofty Launcher as administrator"),
        "{text}"
    );
    let all = AdminLayer {
        all_users: true,
        ..user.clone()
    };
    let text = explain(&[user, all.clone()], game);
    assert!(
        text.starts_with("F1_25.exe is set to run as administrator (for all users)"),
        "{text}"
    );
    assert!(text.contains("under \"Change settings for all users\""), "{text}");
    let both = AdminLayer {
        name: "EAAntiCheat.GameServiceLauncher.exe".into(),
        exe: r"C:\Games\F1 25\EAAntiCheat.GameServiceLauncher.exe".into(),
        ..all.clone()
    };
    let text = explain(&[all, both], game);
    assert!(
        text.starts_with("F1_25.exe and EAAntiCheat.GameServiceLauncher.exe are set to run"),
        "{text}"
    );
    assert!(text.contains("will not start them"), "{text}");
}

#[cfg(not(windows))]
#[test]
#[cfg(unix)]
fn finds_a_game_folder_this_process_cannot_write_in() {
    use std::os::unix::fs::PermissionsExt;
    let game = std::env::temp_dir().join(format!("swofty-unwritable-{}", std::process::id()));
    std::fs::remove_dir_all(&game).ok();
    let ui = game.join(FIRST_WRITE);
    std::fs::create_dir_all(&ui).unwrap();
    assert_eq!(unwritable_dir(&game), None);
    assert_eq!(write_blocker(&game), None);
    let lock =
        |dir: &Path, mode: u32| std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
    lock(&ui, 0o555);
    let root_can_write = can_create_in(&ui);
    if !root_can_write {
        assert_eq!(unwritable_dir(&game), Some(ui.clone()));
        assert_eq!(write_blocker(&game), Some(ui.clone()));
        lock(&game, 0o555);
        assert_eq!(unwritable_dir(&game), Some(game.clone()));
        lock(&game, 0o755);
    }
    lock(&ui, 0o755);
    assert_eq!(unwritable_dir(&game), None);
    assert_eq!(
        std::fs::read_dir(&ui).unwrap().count(),
        0,
        "the check leaves nothing behind"
    );
    assert_eq!(unwritable_dir(&game.join("missing")), None);
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn a_write_windows_virtualised_is_one_it_cannot_make() {
    let dir = std::env::temp_dir().join(format!("swofty-virtualised-{}", std::process::id()));
    let store = dir.join("VirtualStore");
    std::fs::create_dir_all(&store).unwrap();
    assert!(can_create_in_with(&dir, |_| None));
    let shadow = |probe: &Path| {
        let copy = store.join(probe.file_name()?);
        std::fs::write(&copy, b"").ok()?;
        Some(copy)
    };
    assert!(!can_create_in_with(&dir, shadow));
    assert!(
        !dir.join(format!(".swofty-write-check-{}", std::process::id()))
            .exists()
    );
    assert_eq!(
        virtual_store_in(
            Path::new(r"C:\Users\id\AppData\Local"),
            Path::new(r"C:\Program Files\EA Games\F1 25")
        )
        .map(|p| p.to_string_lossy().replace('/', "\\")),
        Some(r"C:\Users\id\AppData\Local\VirtualStore\Program Files\EA Games\F1 25".to_owned())
    );
    assert_eq!(
        virtual_store_in(Path::new("/local"), Path::new("/games/F1 25")),
        None
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn names_the_folder_and_the_fix() {
    let text = write_blocked_text(Path::new("/games/F1 25"));
    assert!(text.contains("/games/F1 25"), "{text}");
    if cfg!(windows) {
        assert!(text.contains("Run Swofty Launcher as administrator"), "{text}");
    } else {
        assert!(text.contains("write access"), "{text}");
    }
}

#[test]
fn never_blocks_off_windows() {
    assert!(elevation_blocker(Path::new("/games/F1 25")).is_none());
}
