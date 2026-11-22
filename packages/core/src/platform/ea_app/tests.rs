use std::path::{Path, PathBuf};
use std::process::Command;

use super::handoff::*;
use super::installs::*;
use super::link::*;
use super::*;
use crate::GAME_EXE;
use crate::session::COMPAT_LAYER;

const INSTALLERDATA: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<DiPManifest version="4.0">
  <buildMetaData><gameVersion version="1.0.141.7865"/></buildMetaData>
  <contentIDs>
<contentID>16425845</contentID>
<contentID>16425845_oa</contentID>
<contentID>16425845</contentID>
  </contentIDs>
  <runtime>
<launcher uid="launcher_1">
  <filePath>[HKEY_LOCAL_MACHINE\SOFTWARE\Codemasters\F1_25\Install Dir]EAAntiCheat.GameServiceLauncher.exe</filePath>
</launcher>
  </runtime>
</DiPManifest>"#;

#[test]
fn reads_the_content_ids_once_each() {
    assert_eq!(content_ids(INSTALLERDATA), ["16425845", "16425845_oa"]);
    assert!(content_ids("<DiPManifest/>").is_empty());
    assert_eq!(content_ids("<contentID> 1 </contentID><contentID>"), ["1"]);
}

#[test]
fn builds_the_launch_link() {
    let ids = content_ids(INSTALLERDATA);
    assert_eq!(
        launch_url(&ids, &[]).unwrap(),
        "origin2://game/launch?offerIds=16425845,16425845_oa&autoDownload=1"
    );
    assert_eq!(
        launch_url(&ids, &["-vr".into()]).unwrap(),
        "origin2://game/launch?offerIds=16425845,16425845_oa&autoDownload=1&cmdParams=-vr"
    );
    assert_eq!(
        launch_url(&ids, &["-vr".into(), "-x=a&b".into()]).unwrap(),
        "origin2://game/launch?offerIds=16425845,16425845_oa&autoDownload=1&cmdParams=-vr%20-x%3Da%26b"
    );
    assert_eq!(launch_url(&[], &[]), None);
}

#[test]
fn reads_install_folders_from_the_registry() {
    let out = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Codemasters\\F1_25\r\n    Install Dir    REG_SZ    D:\\EA Games\\F1 25\\\r\n    Locale    REG_SZ    en_US\r\n\r\n";
    assert_eq!(install_dirs(out), [PathBuf::from("D:\\EA Games\\F1 25\\")]);
    let tree = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{A}\r\n    DisplayName    REG_SZ    Other\r\n    InstallLocation    REG_SZ    C:\\Other\r\n\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{B}\r\n    DisplayName    REG_SZ    F1® 25\r\n    InstallLocation    REG_SZ    \"E:\\Games\\F1 25\"\r\n";
    let blocks = reg_tree(tree);
    assert_eq!(blocks.len(), 2);
    let dirs: Vec<PathBuf> = blocks.iter().flat_map(|(_, v)| install_dirs(v)).collect();
    assert_eq!(
        dirs,
        [PathBuf::from("C:\\Other"), PathBuf::from("E:\\Games\\F1 25")]
    );
}

#[test]
fn finds_installs_by_key_uninstall_entry_and_default_folder() {
    let root = std::env::temp_dir().join(format!("swofty-ea-find-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let make = |rel: &str, marker: bool| {
        let d = root.join(rel);
        std::fs::create_dir_all(d.join("__Installer")).unwrap();
        std::fs::write(d.join(GAME_EXE), b"MZ").unwrap();
        if marker {
            std::fs::write(d.join(MARKER), INSTALLERDATA).unwrap();
        }
        d
    };
    let keyed = make("keyed/F1 25", true);
    let listed = make("listed/F1 25", true);
    let default = make("EA Games/F1 25", true);
    let steamish = make("steamish/F1 25", false);
    let reg = |args: &[&str]| -> Option<String> {
        match args {
            [k] if *k == INSTALL_KEYS[0] => Some(format!(
                "\r\n{k}\r\n    Install Dir    REG_SZ    {}\r\n",
                keyed.display()
            )),
            [k] if *k == INSTALL_KEYS[1] => Some(format!(
                "\r\n{k}\r\n    Install Dir    REG_SZ    {}\r\n",
                keyed.display()
            )),
            [k, "/s"] if *k == UNINSTALL_KEYS[0] => Some(format!(
                "\r\nHKEY_X\\{{1}}\r\n    InstallLocation    REG_SZ    {}\r\n\r\nHKEY_X\\{{2}}\r\n    InstallLocation    REG_SZ    {}\r\n",
                listed.display(),
                steamish.display()
            )),
            _ => None,
        }
    };
    let found = find_installs(reg, &[root.join("EA Games")], is_install);
    assert_eq!(found, [keyed, listed, default]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_copy_moved_by_hand_is_not_where_the_ea_app_has_it() {
    let root = std::env::temp_dir().join(format!("swofty-ea-moved-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let program_files = root.join("Program Files/EA Games/F1 25");
    let moved_to = root.join("EA Games/F1 25");
    for d in [&program_files, &moved_to] {
        std::fs::create_dir_all(d.join("__Installer")).unwrap();
        std::fs::write(d.join(GAME_EXE), b"MZ").unwrap();
        std::fs::write(d.join(MARKER), INSTALLERDATA).unwrap();
    }
    let record = program_files.display().to_string();
    let reg = |args: &[&str]| -> Option<String> {
        match args {
            [k] if *k == INSTALL_KEYS[1] => {
                Some(format!("\r\n{k}\r\n    Install Dir    REG_SZ    {record}\r\n"))
            }
            [k, "/s"] if *k == UNINSTALL_KEYS[0] => Some(format!(
                "\r\nHKEY_X\\{{A}}\r\n    DisplayName    REG_SZ    Other\r\n    InstallLocation    REG_SZ    C:\\Other\r\n\r\n\
                 HKEY_X\\{{B}}\r\n    DisplayName    REG_SZ    F1® 25\r\n    InstallLocation    REG_SZ    \"{record}\"\r\n"
            )),
            _ => None,
        }
    };
    let rec = recorded(reg);
    assert_eq!(rec.len(), 1, "{rec:?}");
    assert!(crate::game_folder::copies::same_dir(&rec[0], &program_files));
    assert_eq!(moved(&program_files, &rec, is_install), None);
    let m = moved(&moved_to, &rec, is_install).unwrap();
    assert!(m.there);
    assert!(crate::game_folder::copies::same_dir(&m.recorded, &program_files));
    let text = m.text();
    assert!(
        text.starts_with(&format!("The EA app has F1 25 in {}", m.recorded.display())),
        "{text}"
    );
    assert!(text.contains(&moved_to.display().to_string()), "{text}");
    assert!(text.contains("Play the copy in"), "{text}");
    assert!(
        text.ends_with("EA app > F1 25 > Manage > Move, not File Explorer"),
        "{text}"
    );

    std::fs::remove_dir_all(&program_files).unwrap();
    let gone = moved(&moved_to, &rec, is_install).unwrap();
    assert!(!gone.there);
    assert!(
        gone.text().contains(". Move the folder back to "),
        "{}",
        gone.text()
    );
    assert_eq!(moved(&moved_to, &[], is_install), None);
    let mangled = [
        PathBuf::from("D:\\Yeni klas?r\\F1 25"),
        PathBuf::from("D:\\Yeni klas\u{fffd}r\\F1 25"),
    ];
    assert_eq!(moved(&moved_to, &mangled, is_install), None);
    assert_eq!(moved(&moved_to, &mangled[..1], is_install), None);
    assert!(recorded(|_| None).is_empty());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn only_f1_25_uninstall_entries_count_as_its_record() {
    assert!(names_f1_25("    DisplayName    REG_SZ    F1® 25\r\n"));
    assert!(names_f1_25("    DisplayName    REG_SZ    F1 25\r\n"));
    assert!(names_f1_25("    DisplayName    REG_SZ    EA SPORTS™ F1® 25\r\n"));
    assert!(!names_f1_25("    DisplayName    REG_SZ    F1® 24\r\n"));
    assert!(!names_f1_25(
        "    DisplayName    REG_SZ    F1 25 Champions Edition Content\r\n"
    ));
    assert!(!names_f1_25("    InstallLocation    REG_SZ    F1 25\r\n"));
}

#[test]
fn hands_the_link_to_the_registered_handler() {
    let out = "\r\nHKEY_CLASSES_ROOT\\origin2\\shell\\open\\command\r\n    (Default)    REG_SZ    \"C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EALauncher.exe\" \"%1\"\r\n";
    let h = Handoff::from_protocol(out).unwrap();
    assert_eq!(
        h.argv("origin2://game/launch?offerIds=1&autoDownload=1"),
        [
            r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe",
            "origin2://game/launch?offerIds=1&autoDownload=1"
        ]
    );
    assert_eq!(
        opener(&h).map(|c| c.get_program().to_owned()),
        Some(r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe".into())
    );
    let bare = Handoff::Program(vec!["EALauncher.exe".into()]);
    assert_eq!(bare.argv("u"), ["EALauncher.exe", "u"]);
    let strips = |c: Command| c.get_envs().any(|(k, v)| k == COMPAT_LAYER && v.is_none());
    assert!(strips(h.command("u")) && strips(opener(&h).unwrap()));
    assert!(strips(opener(&Handoff::Shell("open {url}".into())).unwrap()));
}

#[test]
fn a_shell_setting_gets_the_link_quoted() {
    let h = Handoff::Shell("/games/ea-app.sh wine start {url}".into());
    assert_eq!(
        h.argv("origin2://game/launch?offerIds=1&autoDownload=1"),
        [
            "sh",
            "-c",
            "/games/ea-app.sh wine start 'origin2://game/launch?offerIds=1&autoDownload=1'"
        ]
    );
    assert_eq!(Handoff::Shell("open".into()).argv("a'b")[2], r"open 'a'\''b'");
    assert_eq!(Handoff::from_setting("  "), None);
    let open = opener(&h).unwrap();
    let args: Vec<_> = open.get_args().collect();
    assert_eq!(open.get_program(), "sh");
    assert_eq!(args, ["-c", "/games/ea-app.sh wine start "]);
    assert!(opener(&Handoff::Program(vec!["open.bat".into()])).is_none());
}

fn on_disk<'a>(paths: &'a [&'a str]) -> impl Fn(&Path) -> bool + 'a {
    move |p: &Path| paths.iter().any(|q| Path::new(q) == p)
}

#[test]
fn an_unquoted_proton_path_with_spaces_is_quoted() {
    let disk = [
        "/home/me/.local/share/Steam/steamapps/common/Proton - Experimental/proton",
        "/home/me/.local/share/Steam/steamapps/compatdata/my prefix",
    ];
    let line = "STEAM_COMPAT_DATA_PATH=/home/me/.local/share/Steam/steamapps/compatdata/my prefix \
                /home/me/.local/share/Steam/steamapps/common/Proton - Experimental/proton run {url}";
    assert_eq!(
        quote_spaced_paths(line, &on_disk(&disk)),
        "STEAM_COMPAT_DATA_PATH='/home/me/.local/share/Steam/steamapps/compatdata/my prefix' \
         '/home/me/.local/share/Steam/steamapps/common/Proton - Experimental/proton' run {url}"
    );
    let args = "/opt/ea app.sh wine /games/EA Desktop/EALauncher.exe {url}";
    assert_eq!(
        quote_spaced_paths(
            args,
            &on_disk(&["/opt/ea app.sh", "/games/EA Desktop/EALauncher.exe"])
        ),
        "'/opt/ea app.sh' wine '/games/EA Desktop/EALauncher.exe' {url}"
    );
}

#[test]
fn a_line_that_runs_as_written_is_left_alone() {
    let disk = ["/games/ea-app.sh", "/games/Proton - Experimental/proton"];
    for line in [
        "/games/ea-app.sh wine start {url}",
        "'/games/Proton - Experimental/proton' run {url}",
        "\"/games/Proton - Experimental/proton\" run {url}",
        "u={url}; L='C:\\Program Files\\EA Desktop\\EALauncher.exe'; exec /games/ea-app.sh wine \"$L\" \"$u\"",
        "/missing/Proton - Experimental/proton run {url}",
        "~/Proton - Experimental/proton run {url}",
        "printf %s {url} > '/tmp/got url'",
        "/games/ea-app.sh  wine  start {url}",
    ] {
        assert_eq!(quote_spaced_paths(line, &on_disk(&disk)), line);
    }
    assert_eq!(
        quote_spaced_paths(
            "/games/Proton - Experimental/proton run {url} ; /games/Proton - Experimental/proton",
            &on_disk(&disk)
        ),
        "'/games/Proton - Experimental/proton' run {url} ; /games/Proton - Experimental/proton",
        "nothing from the first shell syntax on is touched"
    );
}

#[cfg(unix)]
#[test]
fn the_setting_runs_a_proton_path_with_spaces_written_without_quotes() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("swofty-ea-spaced-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let proton = root.join("steamapps/common/Proton - Experimental/proton");
    let compat = root.join("steamapps/compatdata/my prefix");
    std::fs::create_dir_all(proton.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&compat).unwrap();
    let got = root.join("got.txt");
    std::fs::write(
        &proton,
        format!(
            "#!/bin/sh\nprintf '%s|%s|%s' \"$STEAM_COMPAT_DATA_PATH\" \"$1\" \"$2\" > '{}'\n",
            got.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&proton, std::fs::Permissions::from_mode(0o755)).unwrap();
    let setting = format!(
        "STEAM_COMPAT_DATA_PATH={} {} run {{url}}",
        compat.display(),
        proton.display()
    );
    let h = Handoff::from_setting(&setting).unwrap();
    let url = "origin2://game/launch?offerIds=1&autoDownload=1";
    let mut status = h.command(url).status().unwrap();
    for _ in 0..20 {
        if status.code() != Some(126) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        status = h.command(url).status().unwrap();
    }
    assert!(status.success(), "{h}: {status}");
    assert_eq!(
        std::fs::read_to_string(&got).unwrap(),
        format!("{}|run|{url}", compat.display())
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_administrator_opens_the_ea_app_through_explorer() {
    let h = Handoff::from_protocol(ORIGIN2_KEY).unwrap();
    let exe = r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe";
    let (open, exit_tells) = desktop_opener(&h, true).unwrap();
    let args: Vec<_> = open.get_args().collect();
    assert_eq!(open.get_program(), DESKTOP_SHELL);
    assert_eq!(args, [exe]);
    assert!(!exit_tells);
    assert!(open.get_envs().any(|(k, v)| k == COMPAT_LAYER && v.is_none()));
    let (open, exit_tells) = desktop_opener(&h, false).unwrap();
    assert_eq!(open.get_program(), exe);
    assert_eq!(open.get_args().count(), 0);
    assert!(exit_tells);
    let shell = Handoff::Shell("/games/ea-app.sh wine start {url}".into());
    let (open, exit_tells) = desktop_opener(&shell, true).unwrap();
    assert_eq!(open.get_program(), "sh");
    assert!(exit_tells);
    assert!(desktop_opener(&Handoff::Program(vec!["open.bat".into()]), true).is_none());
}

// The values the EA app 13.813 installer writes, as `reg query` lists them.
const ORIGIN2_KEY: &str = "\r\nHKEY_CLASSES_ROOT\\origin2\\shell\\open\\command\r\n    (Default)    REG_SZ    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EALauncher.exe \"%1\"\r\n\r\n";
const DESKTOP_KEY: &str = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\WOW6432Node\\Electronic Arts\\EA Desktop\r\n    ClientPath    REG_SZ    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EADesktop.exe\r\n    DesktopAppPath    REG_SZ    C:\\Program Files\\Electronic Arts\\EA Desktop\\EA Desktop\\EADesktop.exe\r\n    InstallLocation    REG_SZ    C:\\Program Files\\Electronic Arts\\EA Desktop\r\n";

#[test]
fn runs_the_unquoted_handler_the_ea_app_registers() {
    let h = Handoff::from_protocol(ORIGIN2_KEY).unwrap();
    let url = "origin2://game/launch?offerIds=16425845,16425845_oa&autoDownload=1&cmdParams=-vr";
    assert_eq!(
        h.argv(url),
        [
            r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe",
            url
        ]
    );
    assert_eq!(
        opener(&h).map(|c| c.get_program().to_owned()),
        Some(r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe".into())
    );
    assert_eq!(
        split_handler(r#"C:\EA app.EXE --x "%1""#),
        [r"C:\EA app.EXE", "--x", "%1"]
    );
    assert_eq!(split_handler(r"C:\a.exe.d\b.exe %1"), [r"C:\a.exe.d\b.exe", "%1"]);
    assert_eq!(split_handler("open %1"), ["open", "%1"]);
    assert_eq!(
        h.program(),
        Some(PathBuf::from(
            r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EALauncher.exe"
        ))
    );
    assert_eq!(Handoff::Program(vec!["open.bat".into()]).program(), None);
    assert_eq!(Handoff::Shell("/games/ea-app.sh {url}".into()).program(), None);
}

#[test]
fn finds_the_launcher_beside_the_desktop_app() {
    let root = std::env::temp_dir().join(format!("swofty-ea-desktop-{}", std::process::id()));
    let app = root.join("EA Desktop");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join(LAUNCHER_EXE), b"MZ").unwrap();
    let values = DESKTOP_KEY.replace(
        r"C:\Program Files\Electronic Arts\EA Desktop",
        &root.display().to_string(),
    );
    assert_eq!(launcher_exe(&values, &[]), Some(app.join(LAUNCHER_EXE)));
    let desktop_only = format!(
        "    DesktopAppPath    REG_SZ    {}\r\n",
        app.join("EADesktop.exe").display()
    );
    assert_eq!(launcher_exe(&desktop_only, &[]), Some(app.join(LAUNCHER_EXE)));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn reads_the_ea_app_library_folders() {
    let machine = "machine.downloadinplacedir=C:\\Program Files\\EA Games\\\r\nmachine.updatebucket=6\r\n";
    let user = "user.locale=en_GB\nuser.downloadinplacedir=D:\\\nuser.downloadinplacedir=\n";
    assert_eq!(
        library_dirs(machine),
        [PathBuf::from("C:\\Program Files\\EA Games\\")]
    );
    assert_eq!(library_dirs(user), [PathBuf::from("D:\\")]);
    assert!(library_dirs("machine.razormodesplashscreenenabled=1").is_empty());
}

#[test]
fn finds_an_install_in_a_library_folder_without_registry_keys() {
    let root = std::env::temp_dir().join(format!("swofty-ea-library-{}", std::process::id()));
    let game = root.join("F1 25");
    std::fs::create_dir_all(game.join("__Installer")).unwrap();
    std::fs::write(game.join(GAME_EXE), b"MZ").unwrap();
    std::fs::write(game.join(MARKER), INSTALLERDATA).unwrap();
    let ini = format!(
        "user.downloadinplacedir={}{}\n",
        root.display(),
        std::path::MAIN_SEPARATOR
    );
    let found = find_installs(|_| None, &library_dirs(&ini), is_install);
    assert_eq!(found.len(), 1);
    assert!(crate::game_folder::copies::same_dir(&found[0], &game));
    assert!(find_installs(|_| None, &[root.join("missing")], is_install).is_empty());
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(windows)]
#[test]
fn windows_lookups_hold_on_a_computer_without_the_ea_app() {
    // CI's runner has no EA app: discovery and the handler lookup must
    // come back empty-handed, not fail, and a found handler must be one
    // that takes the link.
    for dir in installs() {
        assert!(is_install(&dir));
    }
    if let Some(h) = handoff(None) {
        assert!(h.argv("origin2://x").iter().any(|w| w == "origin2://x"));
    }
    assert_eq!(no_handoff(), NO_HANDOFF_WINDOWS);
    let set = handoff(Some(r#"C:\Tools\ea open.exe "%1""#)).unwrap();
    assert_eq!(set.argv("u"), [r"C:\Tools\ea open.exe", "u"]);
}

#[cfg(unix)]
#[test]
fn one_setting_both_opens_the_app_and_hands_over_the_link() {
    let root = std::env::temp_dir().join(format!("swofty-ea-branch-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let out = root.join("out");
    let h = Handoff::Shell(format!(
        r#"u={{url}}; if [ -z "$u" ]; then echo open >> '{0}'; else echo "link $u" >> '{0}'; fi"#,
        out.display()
    ));
    assert!(opener(&h).unwrap().status().unwrap().success());
    let url = "origin2://game/launch?offerIds=1&autoDownload=1&cmdParams=-vr";
    assert!(h.command(url).status().unwrap().success());
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        format!("open\nlink {url}\n")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn splits_quoted_commands() {
    assert_eq!(
        split_command(r#""C:\A B\x.exe" --flag "%1" """#),
        [r"C:\A B\x.exe", "--flag", "%1", ""]
    );
}

#[test]
fn finds_the_launcher_from_the_desktop_key() {
    let root = std::env::temp_dir().join(format!("swofty-ea-launcher-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let exe = root.join(LAUNCHER_EXE);
    std::fs::write(&exe, b"MZ").unwrap();
    let values = format!(
        "\r\nHKEY_X\r\n    InstallLocation    REG_SZ    {}\r\n",
        root.display()
    );
    assert_eq!(launcher_exe(&values, &[]), Some(exe.clone()));
    assert_eq!(launcher_exe("", std::slice::from_ref(&exe)), Some(exe));
    assert_eq!(launcher_exe("", &[root.join("missing.exe")]), None);
    std::fs::remove_dir_all(&root).ok();
}
