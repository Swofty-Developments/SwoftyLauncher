use std::path::{Path, PathBuf};

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
    if !cfg!(windows) {
        assert_eq!(ea_blocker(Path::new("/games/F1 25"), None, true), None);
    }
}

#[test]
fn error_740_is_the_one_that_needs_elevation() {
    assert!(needs_elevation(&std::io::Error::from_raw_os_error(740)));
    for code in [0, 2, 5, 193, 225, 1223, 4551, 7400] {
        assert!(
            !needs_elevation(&std::io::Error::from_raw_os_error(code)),
            "{code}"
        );
    }
    assert!(!needs_elevation(&std::io::Error::other("740")));
}

#[test]
fn reads_the_session_launchers_elevation_line() {
    let refused = "10:06:01.120 [stub] [offline] session launcher invoked, args: <none>\n\
                   10:06:01.380 [stub] [offline] CreateProcessW failed, error 740 (ERROR_ELEVATION_REQUIRED): \
                   Windows will only start F1_25.exe as administrator and the session launcher is not elevated\n";
    assert!(stub_needs_elevation(refused));
    assert!(stub_needs_elevation(
        "[stub] [offline] CreateProcessW failed, error 740\r\n"
    ));
    assert!(!stub_needs_elevation(
        "[stub] [offline] CreateProcessW failed, error 7400\n"
    ));
    assert!(!stub_needs_elevation(
        "[stub] [offline] CreateProcessW failed, error 2\n"
    ));
    assert!(!stub_needs_elevation("[core] CreateProcessW failed, error 740\n"));
    assert!(!stub_needs_elevation(""));
}

const EA_GAME: &str = r"D:\Games\F1 25";
const EA_LAUNCHER: &str = r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe";

fn ea_listings() -> Vec<(String, bool)> {
    let user = concat!(
        "\r\n",
        "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers\r\n",
        "    D:\\Games\\F1 25\\F1_25.exe    REG_SZ    ~ RUNASADMIN\r\n",
        "    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EALauncher.exe    REG_SZ    ~ RUNASADMIN\r\n",
        "    C:\\Games\\Other\\other.exe    REG_SZ    ~ RUNASADMIN\r\n",
    );
    let machine = concat!(
        "\r\n",
        "HKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers\r\n",
        "    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EADesktop.exe    REG_SZ    ~ RUNASADMIN\r\n",
        "    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\ealauncher.exe    REG_SZ    ~ RUNASADMIN\r\n",
    );
    vec![(user.into(), false), (machine.into(), true)]
}

fn at(launcher: bool, ea_app: bool) -> impl FnOnce() -> Elevation {
    move || Elevation { launcher, ea_app }
}

#[test]
fn an_ea_copys_game_set_to_run_as_admin_blocks_however_the_launcher_runs() {
    let listings = ea_listings();
    let game = Path::new(EA_GAME);
    for launcher in [false, true] {
        let found = ea_blocker_in(&listings, game, None, true, at(launcher, false)).unwrap();
        assert_eq!(
            found.exes,
            [PathBuf::from(r"D:\Games\F1 25\F1_25.exe")],
            "{launcher}"
        );
        assert!(found.game);
    }
    assert_eq!(ea_blocker_in(&listings, game, None, true, at(false, true)), None);
    assert_eq!(
        ea_blocker_in(&listings, game, None, false, || panic!("nothing to ask about")),
        None
    );
    let text = ea_blocker_in(&listings, game, None, true, at(true, false))
        .unwrap()
        .text();
    assert!(
        text.starts_with(
            "F1_25.exe is set to run as administrator, so Windows won't start it for an EA app session \
             (error 740). The EA app starts F1 25 without administrator rights, so running Swofty Launcher \
             as administrator doesn't help. Right-click D:\\Games\\F1 25\\F1_25.exe, open Properties > \
             Compatibility and untick \"Run this program as an administrator\", also under \"Change settings \
             for all users\". Then press Play again."
        ),
        "{text}"
    );
}

#[test]
fn a_handoff_program_set_to_run_as_admin_blocks_an_unelevated_launcher() {
    let listings = ea_listings();
    let game = Path::new(EA_GAME);
    let launcher = Path::new(EA_LAUNCHER);
    let found = ea_blocker_in(&listings, game, Some(launcher), false, at(false, false)).unwrap();
    assert_eq!(
        found.paths(),
        [
            EA_LAUNCHER,
            r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EADesktop.exe",
        ]
    );
    assert!(!found.game);
    let text = found.text();
    assert!(
        text.starts_with("These files are set to run as administrator, so Windows won't start them for an EA app session (error 740): C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EALauncher.exe, C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EADesktop.exe. For each one, right-click it, open Properties > Compatibility"),
        "{text}"
    );
    assert!(!text.contains("Swofty Launcher as administrator"), "{text}");
    assert_eq!(
        ea_blocker_in(&listings, game, Some(launcher), false, at(true, false)),
        None
    );
    let both = ea_blocker_in(&listings, game, Some(launcher), true, at(false, false)).unwrap();
    assert_eq!(both.names(), ["EALauncher.exe", "EADesktop.exe", "F1_25.exe"]);
    assert!(both.game);
    let unset = Path::new(r"C:\EA\EALauncher.exe");
    let clean = vec![(
        "    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EADesktop.exe    REG_SZ    ~ RUNASADMIN\r\n"
            .to_owned(),
        false,
    )];
    assert_eq!(
        ea_blocker_in(&clean, game, Some(unset), false, || panic!(
            "nothing to ask about"
        )),
        None
    );
}

#[test]
fn a_refused_start_names_the_program_and_the_ea_apps_settings() {
    let listings = ea_listings();
    let r = refused_start_in(&listings, Path::new(EA_LAUNCHER));
    assert_eq!(r.names(), ["EALauncher.exe", "EADesktop.exe"]);
    assert!(!r.game);
    let alone = refused_start_in(&[], Path::new(EA_LAUNCHER));
    assert_eq!(alone.paths(), [EA_LAUNCHER]);
    assert_eq!(
        alone.headline(),
        "Windows won't start EALauncher.exe: it is set to run as administrator"
    );
    assert!(
        alone
            .text()
            .contains(&format!("Right-click {EA_LAUNCHER}, open Properties"))
    );
    let game = game_refused_in(&listings, Path::new(EA_GAME));
    assert_eq!(game.paths(), [r"D:\Games\F1 25\F1_25.exe"]);
    assert!(game.game);
    let unread = game_refused_in(&[], Path::new(EA_GAME));
    assert_eq!(unread.names(), [GAME_EXE]);
    assert!(unread.game);
    assert_eq!(
        r.headline(),
        "Windows won't start EALauncher.exe, EADesktop.exe: they are set to run as administrator"
    );
}

#[test]
fn a_run_as_admin_failure_round_trips() {
    let r = RunAsAdmin::new(
        [
            PathBuf::from(r"D:\Games\F1 25\F1_25.exe"),
            PathBuf::from("d:/games/f1 25/f1_25.exe"),
        ],
        true,
    );
    assert_eq!(r.exes.len(), 1);
    let json = serde_json::to_string(&crate::diagnostics::launch::Cause::RunAsAdmin(r.clone())).unwrap();
    assert_eq!(
        serde_json::from_str::<crate::diagnostics::launch::Cause>(&json).unwrap(),
        crate::diagnostics::launch::Cause::RunAsAdmin(r)
    );
}

fn cache_game(tag: &str) -> PathBuf {
    let game = std::env::temp_dir().join(format!("swofty-core-caches-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&game).ok();
    std::fs::create_dir_all(&game).unwrap();
    game
}

#[test]
fn opens_every_core_cache_the_unelevated_game_cannot_create_files_in() {
    let game = cache_game("closed");
    let granted = std::cell::RefCell::new(Vec::new());
    let lines = open_core_caches_with(
        &game,
        |_| Ok(false),
        || true,
        |dir| {
            granted.borrow_mut().push(dir.to_path_buf());
            Ok(())
        },
    );
    let made: Vec<PathBuf> = CORE_CACHES.iter().map(|rel| core_cache_dir(&game, rel)).collect();
    assert_eq!(*granted.borrow(), made);
    assert!(made.iter().all(|d| d.is_dir()));
    assert!(made.contains(&game.join("swofty").join("addons").join("swofty.logos")));
    assert_eq!(lines.len(), CORE_CACHES.len(), "{lines:?}");
    assert!(
        lines[0].starts_with("core caches: gave the Users group Modify on swofty/addons/swofty.logos"),
        "{}",
        lines[0]
    );

    let lines = open_core_caches_with(
        &game,
        |_| Ok(false),
        || true,
        |dir| {
            if dir.ends_with("swofty.logos") {
                Err("Access is denied.".to_owned())
            } else {
                Ok(())
            }
        },
    );
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[0].contains("can't create files in swofty/addons/swofty.logos")
            && lines[0].ends_with(": Access is denied."),
        "{}",
        lines[0]
    );
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn a_game_folder_the_unelevated_game_can_create_files_in_keeps_its_caches_as_they_are() {
    let game = cache_game("open");
    let probed = std::cell::RefCell::new(Vec::new());
    let asked = std::cell::Cell::new(false);
    let lines = open_core_caches_with(
        &game,
        |dir| {
            probed.borrow_mut().push(dir.to_path_buf());
            Ok(true)
        },
        || {
            asked.set(true);
            true
        },
        |dir| panic!("{} was opened", dir.display()),
    );
    assert!(lines.is_empty(), "{lines:?}");
    assert!(
        !asked.get(),
        "a folder the game can change needs no administrator check"
    );
    assert_eq!(*probed.borrow(), vec![game.clone(); CORE_CACHES.len()]);
    assert!(!game.join("swofty").exists());
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn only_the_cache_the_unelevated_game_cannot_create_files_in_is_opened() {
    let game = cache_game("one");
    std::fs::create_dir_all(core_cache_dir(&game, "swofty/addons/swofty.garage")).unwrap();
    let probed = std::cell::RefCell::new(Vec::new());
    let granted = std::cell::RefCell::new(Vec::new());
    let lines = open_core_caches_with(
        &game,
        |dir| {
            probed.borrow_mut().push(dir.to_path_buf());
            Ok(dir.ends_with("swofty.garage"))
        },
        || true,
        |dir| {
            granted.borrow_mut().push(dir.to_path_buf());
            Ok(())
        },
    );
    assert_eq!(
        *probed.borrow(),
        [
            game.join("swofty").join("addons"),
            game.join("swofty").join("addons").join("swofty.garage")
        ]
    );
    assert_eq!(
        *granted.borrow(),
        [core_cache_dir(&game, "swofty/addons/swofty.logos")]
    );
    assert_eq!(lines.len(), 1, "{lines:?}");
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn a_cache_whose_access_cannot_be_read_is_opened() {
    let game = cache_game("unread");
    let granted = std::cell::RefCell::new(0);
    let lines = open_core_caches_with(
        &game,
        |_| Err("GetFileSecurityW: The request is not supported.".to_owned()),
        || true,
        |_| {
            *granted.borrow_mut() += 1;
            Ok(())
        },
    );
    assert_eq!(*granted.borrow(), CORE_CACHES.len());
    assert_eq!(lines.len(), CORE_CACHES.len(), "{lines:?}");
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn a_closed_cache_stays_as_it_is_without_administrator_rights() {
    let game = cache_game("unelevated");
    let lines = open_core_caches_with(
        &game,
        |_| Ok(false),
        || false,
        |dir| panic!("{} was opened", dir.display()),
    );
    assert!(lines.is_empty(), "{lines:?}");
    assert!(!game.join("swofty").exists());
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn a_core_cache_that_cannot_be_made_is_named() {
    let game = cache_game("file");
    std::fs::write(game.join("swofty"), b"a file where the folder goes").unwrap();
    let lines = open_core_caches_with(&game, |_| Ok(false), || true, |_| Ok(()));
    assert_eq!(lines.len(), CORE_CACHES.len(), "{lines:?}");
    assert!(
        lines
            .iter()
            .all(|l| l.starts_with("core caches: ") && l.contains("can't create files in"))
    );
    std::fs::remove_dir_all(&game).ok();
}

#[cfg(windows)]
fn saved_acl(path: &Path) -> String {
    let saved = path.with_extension("acl");
    windows_tool_within(
        "icacls",
        &[path.to_str().unwrap(), "/save", saved.to_str().unwrap()],
        std::time::Duration::from_secs(30),
    )
    .unwrap();
    let bytes = std::fs::read(&saved).unwrap();
    std::fs::remove_file(&saved).ok();
    let wide: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&wide)
}

#[cfg(windows)]
#[test]
fn the_grant_gives_the_users_group_modify_on_the_folder_and_every_file_in_it() {
    let dir = std::env::temp_dir().join(format!("swofty-cache-grant-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let before = dir.join("before.erp");
    std::fs::write(&before, b"made before the grant").unwrap();
    grant_users_modify(&dir).unwrap();
    grant_users_modify(&dir).unwrap();
    let after = dir.join("after.erp");
    std::fs::write(&after, b"made after the grant").unwrap();
    let acl = saved_acl(&dir);
    assert_eq!(acl.matches("(A;OICI;0x1301bf;;;BU)").count(), 1, "{acl}");
    for file in [&before, &after] {
        let acl = saved_acl(file);
        assert!(acl.contains("(A;ID;0x1301bf;;;BU)"), "{}: {acl}", file.display());
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// Gives only the Administrators group access to `dir`, as Program Files
/// gives writes, with nothing inherited.
#[cfg(windows)]
fn administrators_only(dir: &Path) {
    windows_tool_within(
        "icacls",
        &[
            dir.to_str().unwrap(),
            "/inheritance:r",
            "/grant:r",
            "*S-1-5-32-544:(OI)(CI)F",
            "/Q",
        ],
        std::time::Duration::from_secs(30),
    )
    .unwrap();
}

#[cfg(windows)]
#[test]
fn the_unelevated_game_cannot_create_files_in_a_folder_only_administrators_can_change() {
    let dir = std::env::temp_dir().join(format!("swofty-unelevated-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    administrators_only(&dir);
    assert_eq!(
        super::unelevated::can_create_in(&dir),
        Ok(false),
        "{}",
        saved_acl(&dir)
    );
    if elevated() {
        assert!(can_create_in(&dir), "the administrator copy itself can");
    }
    grant_users_modify(&dir).unwrap();
    assert_eq!(
        super::unelevated::can_create_in(&dir),
        Ok(true),
        "{}",
        saved_acl(&dir)
    );
    assert!(super::unelevated::can_create_in(&dir.join("missing")).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

#[cfg(windows)]
#[test]
fn an_elevated_launcher_opens_the_caches_of_a_game_folder_only_administrators_can_change() {
    if !elevated() {
        eprintln!("skipped: this test needs administrator rights");
        return;
    }
    let game = std::env::temp_dir().join(format!("swofty-core-caches-admin-{}", std::process::id()));
    std::fs::remove_dir_all(&game).ok();
    std::fs::create_dir_all(&game).unwrap();
    administrators_only(&game);
    let lines = open_core_caches(&game);
    assert_eq!(lines.len(), CORE_CACHES.len(), "{lines:?}");
    assert!(
        lines.iter().all(|l| l.starts_with("core caches: gave")),
        "{lines:?}"
    );
    for rel in CORE_CACHES {
        let dir = core_cache_dir(&game, rel);
        assert_eq!(
            super::unelevated::can_create_in(&dir),
            Ok(true),
            "{}",
            saved_acl(&dir)
        );
    }
    assert!(open_core_caches(&game).is_empty());
    std::fs::remove_dir_all(&game).ok();
}
