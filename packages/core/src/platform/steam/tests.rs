use std::path::{Path, PathBuf};

use super::client;

use super::library::*;
use super::proton::*;
use super::saves::*;
use super::*;

fn vdf_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

fn proton_tree(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-proton-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    std::fs::create_dir_all(root.join("config")).unwrap();
    root
}

fn put_proton(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("proton"), "").unwrap();
}

fn mapping(root: &Path, body: &str) {
    std::fs::write(
            root.join("config/config.vdf"),
            format!(
                "\"InstallConfigStore\" {{ \"Software\" {{ \"Valve\" {{ \"Steam\" {{ \"CompatToolMapping\" {{ {body} }} }} }} }} }}"
            ),
        )
        .unwrap();
}

#[test]
fn compat_tool_falls_back_to_global_default() {
    let root = proton_tree("global");
    mapping(&root, "\"0\" { \"name\" \"proton_9\" }");
    assert_eq!(compat_tool(&root).as_deref(), Some("proton_9"));
    mapping(
        &root,
        "\"0\" { \"name\" \"proton_9\" } \"3059520\" { \"name\" \"GE-Proton9-1\" }",
    );
    assert_eq!(compat_tool(&root).as_deref(), Some("GE-Proton9-1"));
    mapping(&root, "\"3059520\" { \"name\" \"\" }");
    assert_eq!(compat_tool(&root), None);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn finds_valve_proton_by_internal_name_in_main_library() {
    let root = proton_tree("valve");
    put_proton(&root.join("steamapps/common/Proton 9.0"));
    put_proton(&root.join("steamapps/common/Proton - Experimental"));
    let other = root.join("drive2");
    std::fs::create_dir_all(other.join("steamapps/common/F1 25")).unwrap();
    let hit = find_proton(&root, Some(&other), "proton_9");
    assert_eq!(hit.found, Some(root.join("steamapps/common/Proton 9.0")));
    let hit = find_proton(&root, Some(&other), "proton_experimental");
    assert_eq!(
        hit.found,
        Some(root.join("steamapps/common/Proton - Experimental"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn finds_proton_in_listed_library_and_by_manifest_name() {
    let root = proton_tree("libs");
    let lib = std::env::temp_dir().join(format!("swofty-proton-libs2-{}", std::process::id()));
    put_proton(&lib.join("steamapps/common/Proton 10.0"));
    std::fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        format!(
            "\"libraryfolders\" {{ \"1\" {{ \"path\" \"{}\" }} }}",
            vdf_path(&lib)
        ),
    )
    .unwrap();
    let custom = root.join("compatibilitytools.d/some-folder");
    put_proton(&custom);
    std::fs::write(
            custom.join("compatibilitytool.vdf"),
            "\"compatibilitytools\" { \"compat_tools\" { \"my-tool\" { \"install_path\" \".\" \"display_name\" \"My Tool\" } } }",
        )
        .unwrap();
    assert_eq!(
        find_proton(&root, None, "proton_10").found,
        Some(lib.join("steamapps/common/Proton 10.0"))
    );
    assert_eq!(find_proton(&root, None, "my-tool").found, Some(custom.clone()));
    assert_eq!(find_proton(&root, None, "My Tool").found, Some(custom));
    let miss = find_proton(&root, None, "nothing");
    assert!(miss.found.is_none());
    assert!(miss.searched.contains(&lib.join("steamapps/common")));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&lib);
}

#[test]
fn finds_proton_in_system_compat_dirs_after_user_dir() {
    let root = proton_tree("system");
    let sys = std::env::temp_dir().join(format!("swofty-proton-sys-{}", std::process::id()));
    put_proton(&sys.join("proton-ge-custom"));
    put_proton(&root.join("steamapps/common/proton-ge-custom"));
    let sys_list = [sys.clone()];
    let hit = find_proton_in(&root, None, "proton-ge-custom", &sys_list);
    assert_eq!(hit.found, Some(sys.join("proton-ge-custom")));
    put_proton(&root.join("compatibilitytools.d/proton-ge-custom"));
    let hit = find_proton_in(&root, None, "proton-ge-custom", &sys_list);
    assert_eq!(
        hit.found,
        Some(root.join("compatibilitytools.d/proton-ge-custom"))
    );
    let miss = find_proton_in(&root, None, "nothing", &sys_list);
    assert!(miss.found.is_none());
    let pos = |p: &Path| miss.searched.iter().position(|s| s == p).unwrap();
    assert!(pos(&sys) < pos(&root.join("steamapps/common")));
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&sys);
}

#[test]
fn exact_folder_name_still_resolves() {
    let root = proton_tree("exact");
    put_proton(&root.join("compatibilitytools.d/proton-cachyos-10.0-20260425-slr-x86_64"));
    assert_eq!(
        find_proton(&root, None, "proton-cachyos-10.0-20260425-slr-x86_64").found,
        Some(root.join("compatibilitytools.d/proton-cachyos-10.0-20260425-slr-x86_64"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn finds_game_in_secondary_library() {
    let root = std::env::temp_dir().join(format!("swofty-steam-{}", std::process::id()));
    let lib = root.join("lib2");
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    std::fs::create_dir_all(lib.join("steamapps/common/F1 25")).unwrap();
    std::fs::create_dir_all(lib.join("steamapps/compatdata/3059520/pfx")).unwrap();
    std::fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        format!(
            r#""libraryfolders" {{ "0" {{ "path" "{}" }} "1" {{ "path" "{}" }} }}"#,
            vdf_path(&root),
            vdf_path(&lib)
        ),
    )
    .unwrap();
    std::fs::write(
        lib.join("steamapps/appmanifest_3059520.acf"),
        r#""AppState" { "appid" "3059520" "installdir" "F1 25" "buildid" "25251167" }"#,
    )
    .unwrap();
    let g = find_game_in(&root).unwrap();
    assert_eq!(g.dir, lib.join("steamapps/common/F1 25"));
    assert_eq!(g.build_id, Some(25_251_167));
    assert_eq!(busy(&g), None);
    for (flags, state) in [
        ("4", None),
        ("6", None),
        ("131076", Some("verifying")),
        ("1030", Some("updating")),
    ] {
        std::fs::write(
            lib.join("steamapps/appmanifest_3059520.acf"),
            format!(r#""AppState" {{ "appid" "3059520" "installdir" "F1 25" "StateFlags" "{flags}" }}"#),
        )
        .unwrap();
        assert_eq!(busy(&g), state, "{flags}");
    }
    std::fs::write(
        lib.join("steamapps/appmanifest_3059520.acf"),
        r#""AppState" { "appid" "3059520" "installdir" "F1 25" "buildid" "25251167" }"#,
    )
    .unwrap();
    assert!(g.prefix().unwrap().ends_with("compatdata/3059520/pfx"));
    assert!(in_steam_library(&g.dir));
    assert!(!in_steam_library(&root.join("steamapps/common/F1 25")));
    assert!(!in_steam_library(&lib));
    assert!(!steamvr_installed(&root));
    std::fs::write(
        lib.join("steamapps/appmanifest_250820.acf"),
        r#""AppState" { "appid" "250820" "installdir" "SteamVR" }"#,
    )
    .unwrap();
    assert!(steamvr_installed(&root));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn finds_the_steam_root_of_a_game_on_another_drive_with_spaces() {
    let base = std::env::temp_dir().join(format!("swofty-root-{}", std::process::id()));
    let root = base.join("Bagas").join("New Folder").join("Apps").join("Steam");
    std::fs::create_dir_all(root.join("steamapps/common/F1 25")).unwrap();
    std::fs::create_dir_all(root.join("userdata/1163457660/3059520/remote")).unwrap();
    let game = root.join("steamapps").join("common").join("F1 25");
    assert_eq!(root_of_game_in(&game, &[]), Some(root.clone()));
    assert_eq!(
        save_dir(&root),
        Some(root.join("userdata/1163457660/3059520/remote"))
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn finds_the_steam_root_of_a_game_in_a_secondary_library() {
    let base = std::env::temp_dir().join(format!("swofty-root2-{}", std::process::id()));
    let root = base.join("Steam Main");
    let lib = base.join("Games Lib").join("SteamLibrary");
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    std::fs::create_dir_all(root.join("userdata/5")).unwrap();
    std::fs::create_dir_all(lib.join("steamapps/common/F1 25")).unwrap();
    std::fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        format!(r#""libraryfolders" {{ "1" {{ "path" "{}" }} }}"#, vdf_path(&lib)),
    )
    .unwrap();
    let game = lib.join("steamapps").join("common").join("F1 25");
    assert_eq!(
        root_of_game_in(&game, std::slice::from_ref(&root)),
        Some(root.clone())
    );
    assert_eq!(root_of_game_in(&game, &[]), None);
    assert_eq!(root_of_game_in(&base.join("loose").join("F1 25"), &[root]), None);
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn a_windows_shaped_game_path_without_a_steam_root_finds_none() {
    let game = Path::new(r"D:\Bagas\New Folder\Apps\Steam\steamapps\common\F1 25");
    assert_eq!(root_of_game_in(game, &[]), None);
}

#[test]
fn finds_the_most_recent_users_save_folder() {
    let root = std::env::temp_dir().join(format!("swofty-saves-{}", std::process::id()));
    for id3 in ["11", "22"] {
        std::fs::create_dir_all(root.join(format!("userdata/{id3}/3059520/remote"))).unwrap();
    }
    std::fs::create_dir_all(root.join("config")).unwrap();
    std::fs::write(
        root.join("config/loginusers.vdf"),
        format!(
            r#""users" {{ "{}" {{ "AccountName" "a" "MostRecent" "0" "Timestamp" "900" }}
                   "{}" {{ "AccountName" "b" "MostRecent" "1" "Timestamp" "100" }} }}"#,
            STEAMID64_BASE + 11,
            STEAMID64_BASE + 22
        ),
    )
    .unwrap();
    assert_eq!(save_dir(&root), Some(root.join("userdata/22/3059520/remote")));
    std::fs::remove_file(root.join("config/loginusers.vdf")).unwrap();
    assert!(save_dir(&root).is_some_and(|d| d.ends_with("3059520/remote")));
    std::fs::remove_dir_all(root.join("userdata")).unwrap();
    assert_eq!(save_dir(&root), None);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn reads_what_steam_does_to_the_game_from_its_content_log() {
    let dir = std::env::temp_dir().join(format!("swofty-content-log-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("content_log.txt");
    let at = |lines: &[&str]| {
        std::fs::write(&log, lines.join("\n") + "\n").unwrap();
        log_busy(&log)
    };
    let shader = [
        "[2026-10-24 12:48:43] AppID 3059520 Shader update changed : Running Update,Reconfiguring,",
        "[2026-10-24 12:48:45] AppID 3059520 state changed : Fully Installed,Update Queued,",
        "[2026-10-24 12:48:45] AppID 3059520 state changed : Fully Installed,",
    ];
    assert_eq!(at(&shader), None);
    let verify = [
        "[2026-10-24 15:23:18] Start validating appID 3059520 on BuildID 25251167",
        "[2026-10-24 15:23:18] AppID 3059520 state changed : Fully Installed,Update Queued,Update Running,",
        "[2026-10-24 15:23:18] AppID 228980 state changed : Fully Installed,Update Running,",
        "[2026-10-24 15:23:18] AppID 3059520 App update changed : Running Update,Verifying Installed,",
    ];
    assert_eq!(at(&verify), Some("verifying"));
    let mut done = verify.to_vec();
    done.push("[2026-10-24 15:31:02] AppID 3059520 App update changed : None");
    done.push("[2026-10-24 15:31:02] AppID 3059520 state changed : Fully Installed,");
    assert_eq!(at(&done), None);
    let mut update = done.clone();
    update.push("[2026-10-24 16:00:00] AppID 3059520 state changed : Fully Installed,Update Running,");
    update.push("[2026-10-24 16:00:01] AppID 3059520 App update changed : Running Update,Downloading,");
    assert_eq!(at(&update), Some("updating"));
    assert_eq!(log_busy(&dir.join("missing.txt")), None);
    std::fs::remove_dir_all(&dir).ok();
}

struct FakeReg(Vec<(&'static str, String)>);

impl client::Registry for FakeReg {
    fn query(&self, key: &str) -> Option<String> {
        self.0.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone())
    }
}

fn fixture(tag: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("swofty-lib-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&base).ok();
    base
}

fn save(root: &Path, id3: &str, files: &[&str]) {
    let remote = root.join("userdata").join(id3).join("3059520/remote");
    std::fs::create_dir_all(&remote).unwrap();
    for f in files {
        std::fs::write(remote.join(f), b"x").unwrap();
    }
}

fn logins(root: &Path, users: &[(u64, &str, u64)]) {
    std::fs::create_dir_all(root.join("config")).unwrap();
    let body: String = users
        .iter()
        .map(|(id3, recent, stamp)| {
            format!(
                r#""{}" {{ "MostRecent" "{recent}" "Timestamp" "{stamp}" }} "#,
                STEAMID64_BASE + id3
            )
        })
        .collect();
    std::fs::write(
        root.join("config/loginusers.vdf"),
        format!(r#""users" {{ {body} }}"#),
    )
    .unwrap();
}

fn library_on_other_drive(base: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let root = base.join("C").join("Program Files (x86)").join("Steamé Ünïcode");
    let lib = base.join("D").join("SteamLibrary");
    let game = lib.join("steamapps/common/F1 25");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    std::fs::write(
        root.join("steamapps/libraryfolders.vdf"),
        format!(
            r#""libraryfolders" {{ "0" {{ "path" "{}" }} "1" {{ "path" "{}" }} }}"#,
            root.display().to_string().replace('\\', "\\\\"),
            lib.display().to_string().replace('\\', "\\\\")
        ),
    )
    .unwrap();
    (root, lib, game)
}

#[test]
fn a_library_on_another_drive_finds_the_save_under_the_steam_root() {
    let base = fixture("drive");
    let (root, lib, game) = library_on_other_drive(&base);
    assert!(!lib.join("userdata").exists());
    logins(&root, &[(11, "0", 900), (22, "1", 100)]);
    save(&root, "11", &["a.sav"]);
    save(&root, "22", &["ProfileSave.dat"]);
    save(&root, "33", &["old.sav"]);
    let found = search_save_in(Some(&game), std::slice::from_ref(&root), &[], Some(0));
    assert_eq!(found.dir, Some(root.join("userdata/22/3059520/remote")));
    let log = found.tried.join("\n");
    assert!(
        log.contains("the Steam install that lists the game's library"),
        "{log}"
    );
    assert!(log.contains("loginusers.vdf MostRecent): chosen"), "{log}");
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn an_empty_most_recent_folder_is_rejected_for_one_with_saves() {
    let base = fixture("empty");
    let root = base.join("Steam");
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    logins(&root, &[(11, "1", 900), (22, "0", 100)]);
    save(&root, "11", &[]);
    save(&root, "22", &["ProfileSave.dat"]);
    let mut tried = Vec::new();
    assert_eq!(
        save_dir_with(&root, Some(0), &mut tried),
        Some(root.join("userdata/22/3059520/remote"))
    );
    let log = tried.join("\n");
    let empty = root.join("userdata").join("11").join("3059520").join("remote");
    assert!(
        log.contains(&empty.display().to_string()) && log.contains("rejected, the folder is empty"),
        "{log}"
    );
    assert_eq!(
        save_dir_with(&root, Some(11), &mut Vec::new()),
        Some(root.join("userdata/22/3059520/remote")),
        "ActiveUser first, but only when its folder holds saves"
    );
    save(&root, "22", &[]);
    std::fs::remove_file(root.join("userdata/22/3059520/remote/ProfileSave.dat")).unwrap();
    assert_eq!(
        save_dir_with(&root, Some(22), &mut Vec::new()),
        Some(root.join("userdata/22/3059520/remote")),
        "no folder holds saves: the first that exists"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn active_user_leads_over_loginusers() {
    let base = fixture("active");
    let root = base.join("Steam");
    std::fs::create_dir_all(root.join("steamapps")).unwrap();
    logins(&root, &[(11, "1", 900)]);
    save(&root, "11", &["a.sav"]);
    save(&root, "22", &["b.sav"]);
    assert_eq!(
        save_dir_with(&root, Some(22), &mut Vec::new()),
        Some(root.join("userdata/22/3059520/remote"))
    );
    assert_eq!(
        save_dir_with(&root, Some(0), &mut Vec::new()),
        Some(root.join("userdata/11/3059520/remote")),
        "ActiveUser 0 means nobody is signed in"
    );
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn the_registry_names_the_steam_root_not_the_library() {
    let reg = FakeReg(vec![
            (
                r"HKCU\Software\Valve\Steam",
                "    SteamPath    REG_SZ    c:/program files (x86)/steam\r\n    SteamExe    REG_SZ    c:/program files (x86)/steam/steam.exe\r\n".to_owned(),
            ),
            (
                r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam",
                "    InstallPath    REG_SZ    C:\\Program Files (x86)\\Steam\r\n".to_owned(),
            ),
        ]);
    assert_eq!(
        registered_roots(&reg),
        [
            PathBuf::from("c:/program files (x86)/steam"),
            PathBuf::from("C:\\Program Files (x86)\\Steam")
        ]
    );
    assert!(registered_roots(&FakeReg(Vec::new())).is_empty());
}

#[test]
fn a_steam_nobody_registered_is_found_by_looking_on_the_drives() {
    let base = fixture("scan");
    let (root, _, game) = library_on_other_drive(&base);
    let odd = base.join("E").join("Steam");
    std::fs::create_dir_all(odd.join("steamapps")).unwrap();
    save(&odd, "44", &["ProfileSave.dat"]);
    assert_eq!(
        drive_roots(&[base.join("E"), base.join("F")]),
        std::slice::from_ref(&odd)
    );
    let found = search_save_in(Some(&game), &[], &drive_roots(&[base.join("E")]), None);
    assert_eq!(found.dir, Some(odd.join("userdata/44/3059520/remote")));
    assert!(
        found
            .tried
            .join("\n")
            .contains("no known Steam root lists this library")
    );
    let none = search_save_in(Some(&game), &[], &[], None);
    assert_eq!(none.dir, None);
    assert!(
        none.tried.join("\n").contains("no Steam install found"),
        "{:?}",
        none.tried
    );
    let _ = root;
    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn detects_wrapper() {
    assert!(wrapper_installed(
        "~/.local/share/swofty/bin/swofty run -- %command%"
    ));
    assert!(!wrapper_installed("echo %command%"));
}
