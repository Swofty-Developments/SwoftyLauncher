use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::addons::builtin::CORE_DLL;
use crate::game_folder::baseline::sha256_file;
use crate::game_folder::placed::Placed;
use crate::install::PROXY;
use crate::session::LogMark;

use super::attach::*;
use super::cause::*;
use super::missing::*;
use super::*;

const STUB: &str = "01:12:12.575 [stub] [offline] session launcher invoked, args: <none>\n\
                        01:12:13.901 [stub] [offline] game image at 0x140000000, patching\n";
const LOADED: &str =
    "2026-10-20 21:16:05.789 [ 2548] swofty loader (version.dll proxy) attached, pid 14088\n";
const FALLBACK: &str = "2026-10-24 16:34:40.020 [ 1416] swofty loader (version.dll proxy) attached, pid 24676\r\n\
         2026-10-24 16:34:40.020 [ 1416] log: C:\\Program Files\\EA Games\\F1 25\\swofty\\loader.log could not be \
         opened (error 5); this is the fallback log C:\\Users\\id\\AppData\\Local\\Swofty\\state\\loader-fallback.log\r\n\
         2026-10-24 16:34:40.021 [ 1416] process: user id (S-1-5-21-1-2-3-1001), medium integrity, not elevated, \
         UAC virtualization off, __COMPAT_LAYER=(unset)\r\n";

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-quarantine-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn knows_the_two_windows_virus_codes() {
    assert!(is_virus_code(225));
    assert!(is_virus_code(226));
    assert!(!is_virus_code(5));
    assert!(!is_virus_code(32));
}

#[test]
fn finds_a_placed_dll_that_was_deleted_or_changed() {
    let game = temp("missing");
    std::fs::create_dir_all(game.join("swofty/addons/core")).unwrap();
    std::fs::write(game.join("version.dll"), b"proxy").unwrap();
    std::fs::write(game.join(CORE_DLL), b"core").unwrap();
    let dlls = vec![
        (
            "version.dll".to_owned(),
            sha256_file(&game.join("version.dll")).unwrap(),
        ),
        (CORE_DLL.to_owned(), sha256_file(&game.join(CORE_DLL)).unwrap()),
    ];
    assert!(missing(&game, &dlls).is_empty());
    std::fs::remove_file(game.join("version.dll")).unwrap();
    std::fs::write(game.join(CORE_DLL), b"emptied by a scanner").unwrap();
    assert_eq!(missing(&game, &dlls), ["version.dll", CORE_DLL]);
    std::fs::remove_dir_all(&game).ok();
}

#[test]
fn lists_only_the_dlls_it_placed() {
    let data = temp("placed");
    let mut placed = Placed::load(&data).unwrap();
    placed.insert("version.dll", "a".into(), false);
    placed.insert(CORE_DLL, "b".into(), false);
    placed.insert("swofty/addons/core/symbols.tsv", "c".into(), false);
    placed.insert("2025_asset_groups/ui_package/databases.erp", "d".into(), true);
    placed.save(&data).unwrap();
    let mut got = placed_dlls(&data).unwrap();
    got.sort();
    assert_eq!(
        got,
        [
            (CORE_DLL.to_owned(), "b".to_owned()),
            ("version.dll".to_owned(), "a".to_owned())
        ]
    );
    std::fs::remove_dir_all(&data).ok();
}

#[test]
fn names_what_an_install_loader_folder_lost() {
    let dir = temp("install");
    std::fs::create_dir_all(dir.join("swofty/addons/core")).unwrap();
    std::fs::write(dir.join(CORE_DLL), b"core").unwrap();
    assert_eq!(damaged_loader(&dir), ["loader/version.dll"]);
    std::fs::write(dir.join(PROXY), b"proxy").unwrap();
    assert!(damaged_loader(&dir).is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn tells_a_stub_only_log_from_a_loaded_one() {
    assert_eq!(attached(STUB), Some(false));
    assert_eq!(
        attached(&format!(
            "{LOADED}2026-10-20 21:16:05.789 [ 2548] host: F1_25.exe\n"
        )),
        Some(true)
    );
    assert_eq!(attached(""), None);
}

#[test]
fn reads_an_attach_line_with_a_clobbered_start() {
    let clobbered = ".dll proxy) attached, pid 12316\r\n\
                         2026-10-27 18:31:45.051 [ 2416] host: F1_25.exe\n";
    assert_eq!(attached(clobbered), Some(true));
}

#[test]
fn misses_the_loader_a_while_after_the_stub_patched_the_game() {
    let dir = temp("watch-miss");
    let log = dir.join("loader.log");
    let t0 = Instant::now();
    let mut w = AttachWatch::new(log.clone(), None, t0);
    assert_eq!(w.poll(t0), Attach::Waiting);
    std::fs::write(&log, STUB).unwrap();
    assert_eq!(w.poll(t0 + Duration::from_secs(2)), Attach::Waiting);
    assert_eq!(w.poll(t0 + Duration::from_secs(20)), Attach::Waiting);
    let late = t0 + Duration::from_secs(3) + ATTACH_WINDOW;
    assert_eq!(w.poll(late), Attach::Missed);
    std::fs::write(&log, LOADED).unwrap();
    assert_eq!(w.poll(late + Duration::from_secs(5)), Attach::Missed);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sees_the_loader_attach_and_ignores_a_stale_log() {
    let dir = temp("watch-ok");
    let log = dir.join("loader.log");
    std::fs::write(&log, LOADED).unwrap();
    let t0 = Instant::now();
    let mut w = AttachWatch::new(log.clone(), LogMark::read(&log), t0);
    assert_eq!(w.poll(t0), Attach::Waiting);
    std::fs::remove_file(&log).unwrap();
    std::fs::write(&log, STUB).unwrap();
    assert_eq!(w.poll(t0 + Duration::from_secs(2)), Attach::Waiting);
    std::fs::write(
        &log,
        "2026-10-21 01:12:14.001 [  420] swofty loader (version.dll proxy) attached, pid 1976\n",
    )
    .unwrap();
    assert_eq!(w.poll(t0 + Duration::from_secs(4)), Attach::Attached);
    assert_eq!(w.ended(t0 + Duration::from_secs(600)), Attach::Attached);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn sees_the_loader_attach_in_its_fallback_log() {
    let dir = temp("watch-fallback");
    let log = dir.join("loader.log");
    let fallback = dir.join("loader-fallback.log");
    std::fs::write(&log, LOADED).unwrap();
    let t0 = Instant::now();
    let mut plain = AttachWatch::new(log.clone(), LogMark::read(&log), t0);
    let mut w = AttachWatch::new(log.clone(), LogMark::read(&log), t0).with_fallback(fallback.clone());
    assert_eq!(w.poll(t0), Attach::Waiting);
    std::fs::write(&fallback, FALLBACK).unwrap();
    assert_eq!(w.poll(t0 + Duration::from_secs(2)), Attach::Attached);
    let late = t0 + Duration::from_secs(1) + ATTACH_WINDOW_NO_STUB;
    assert_eq!(plain.poll(late), Attach::Missed);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn says_why_the_loader_logged_to_its_fallback() {
    assert_eq!(
        refused(FALLBACK).as_deref(),
        Some(
            "C:\\Program Files\\EA Games\\F1 25\\swofty\\loader.log could not be opened (error 5); the game runs as \
                 user id (S-1-5-21-1-2-3-1001), medium integrity, not elevated, UAC virtualization off, \
                 __COMPAT_LAYER=(unset)"
        )
    );
    assert_eq!(refused(LOADED), None);
    assert_eq!(refused(STUB), None);
    assert!(refused_text().contains("addon-storage"));
    assert!(!refused_text().contains("administrator"));
}

#[test]
fn a_session_that_ends_without_the_loader_missed_it() {
    let dir = temp("watch-end");
    let log = dir.join("loader.log");
    std::fs::write(&log, STUB).unwrap();
    let t0 = Instant::now();
    let mut quick = AttachWatch::new(log.clone(), None, t0);
    assert_eq!(quick.ended(t0 + Duration::from_secs(3)), Attach::Waiting);
    let mut long = AttachWatch::new(log, None, t0);
    assert_eq!(long.ended(t0 + Duration::from_secs(13 * 60)), Attach::Missed);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn looks_inside_the_game_once_while_it_runs() {
    let dir = temp("watch-probe");
    let log = dir.join("loader.log");
    let t0 = Instant::now();
    let mut w = AttachWatch::new(log.clone(), None, t0);
    assert!(!w.probe_due(t0 + PROBE_AFTER * 2));
    std::fs::write(&log, STUB).unwrap();
    w.poll(t0);
    assert!(!w.probe_due(t0));
    assert!(!w.probe_due(t0 + PROBE_AFTER - Duration::from_secs(1)));
    assert!(w.probe_due(t0 + PROBE_AFTER));
    assert!(!w.probe_due(t0 + PROBE_AFTER * 2));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_game_that_closed_before_the_look_is_not_blamed_on_the_antivirus() {
    let mut r = Report {
        at: 1.0,
        stage: Stage::InGame,
        files: vec!["version.dll".into()],
        removed: false,
        game: PathBuf::from(r"C:\Steam\steamapps\common\F1 25"),
        launcher: None,
        cause: Some(Box::new(Cause::GameEnded {
            antivirus: Vec::new(),
        })),
    };
    let text = r.text();
    assert!(text.contains("had already closed"), "{text}");
    assert!(
        text.contains("an antivirus is not shown to be the cause"),
        "{text}"
    );
    assert!(!text.contains("most likely blocked"), "{text}");
    let silent = Cause::LoadedSilently {
        exec_options: Vec::new(),
        layers: Vec::new(),
        security: Vec::new(),
    };
    r.cause = Some(Box::new(silent.clone()));
    let text = r.text();
    assert!(text.contains("so nothing blocked it"), "{text}");
    assert!(!text.contains("Image File Execution"), "{text}");
    assert!(silent.unexplained());
    let with = Cause::LoadedSilently {
        exec_options: vec!["MaxLoaderThreads".into()],
        layers: vec!["~ DISABLEDXMAXIMIZEDWINDOWEDMODE HIGHDPIAWARE (for this user)".into()],
        security: vec!["mc-sec-plugin-x64.dll".into()],
    };
    r.cause = Some(Box::new(with));
    let text = r.text();
    assert!(text.contains("MaxLoaderThreads"), "{text}");
    assert!(text.contains(r"Image File Execution Options\F1_25.exe"), "{text}");
    assert!(text.contains("compatibility settings"), "{text}");
    assert!(text.contains("exclusions in mc-sec-plugin-x64.dll"), "{text}");
}

#[test]
fn says_what_happened_and_how_to_fix_it() {
    let mut r = Report {
        at: 1.0,
        stage: Stage::BeforeLaunch,
        files: vec!["version.dll".into(), CORE_DLL.into()],
        removed: false,
        game: PathBuf::from(r"C:\Steam\steamapps\common\F1 25"),
        launcher: Some(PathBuf::from(r"C:\Programs\SwoftyLauncher")),
        cause: None,
    };
    assert_eq!(
        r.text(),
        "F1 25 was not started: an antivirus most likely deleted or quarantined version.dll and \
             swofty_core.dll. To fix it, restore them in Windows Security > Virus & threat protection > \
             Protection history, add exclusions for the game folder (C:\\Steam\\steamapps\\common\\F1 25) \
             and the launcher folder (C:\\Programs\\SwoftyLauncher), then press Play again."
    );
    r.stage = Stage::InGame;
    r.files = vec!["version.dll".into()];
    r.removed = true;
    assert!(r.text().starts_with(
        "F1 25 is running without mods: version.dll was removed from the game folder. An antivirus \
             most likely deleted or quarantined it. To fix it, close F1 25, restore it in"
    ));
    r.removed = false;
    assert!(r.text().starts_with(
        "F1 25 is running without mods. An antivirus most likely blocked version.dll. To fix it"
    ));
    assert_eq!(r.title(), "Mods did not load");
}

#[test]
fn names_what_kept_the_loader_out_when_the_session_found_it() {
    let mut r = Report {
        at: 1.0,
        stage: Stage::InGame,
        files: vec!["version.dll".into()],
        removed: false,
        game: PathBuf::from(r"E:\SteamLibrary\steamapps\common\F1 25"),
        launcher: Some(PathBuf::from(r"C:\Programs\SwoftyLauncher")),
        cause: None,
    };
    let fallback = r.text();
    assert!(fallback.starts_with(
            "F1 25 is running without mods. An antivirus most likely blocked version.dll. To fix it, close F1 25, \
             restore it in Windows Security"
        ));
    let text = |r: &mut Report, cause: Cause| {
        r.cause = Some(Box::new(cause));
        r.text()
    };
    assert_eq!(
        text(&mut r, Cause::SmartAppControl { seen: true }),
        "F1 25 is running without mods: Smart App Control blocked version.dll. To fix it, close F1 25, turn \
             off Smart App Control in Windows Security > App & browser control > Smart App Control settings, then \
             launch again."
    );
    assert!(
        text(&mut r, Cause::SmartAppControl { seen: false })
            .starts_with("F1 25 is running without mods: Smart App Control is on and most likely blocked")
    );
    assert_eq!(
        text(
            &mut r,
            Cause::Policy {
                name: Some("Contoso".into())
            }
        ),
        "F1 25 is running without mods: a Windows App Control policy (Contoso) blocked version.dll. To fix \
             it, close F1 25, ask whoever manages this PC to allow version.dll in the game folder \
             (E:\\SteamLibrary\\steamapps\\common\\F1 25), then launch again."
    );
    let signed = text(&mut r, Cause::SignedOnly);
    assert!(
        signed.contains("load only Microsoft-signed DLLs")
            && signed.contains("Exploit protection > Program settings"),
        "{signed}"
    );
    assert_eq!(
        text(&mut r, Cause::Defender),
        "F1 25 is running without mods: Microsoft Defender blocked version.dll. To fix it, close F1 25, \
             restore it in Windows Security > Virus & threat protection > Protection history, add exclusions for \
             the game folder (E:\\SteamLibrary\\steamapps\\common\\F1 25) and the launcher folder \
             (C:\\Programs\\SwoftyLauncher), then launch again."
    );
    let known = text(&mut r, Cause::KnownDll);
    assert!(
        known.contains(r"HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs"),
        "{known}"
    );
    assert_eq!(
        text(
            &mut r,
            Cause::OtherCopy {
                path: r"C:\WINDOWS\SYSTEM32\VERSION.dll".into(),
                prefer_system32: false,
                from: None,
            }
        ),
        "F1 25 is running without mods: the game loaded C:\\WINDOWS\\SYSTEM32\\VERSION.dll instead of the game \
             folder's version.dll. Security software on this PC most likely redirected it. To fix it, close F1 25, \
             turn off your antivirus's DLL or behaviour protection for F1_25.exe (a folder exclusion is often not \
             enough), then launch again."
    );
    let system = || r"C:\WINDOWS\SYSTEM32\VERSION.dll".to_owned();
    let exploit = text(&mut r, Cause::ExploitProtection { path: system() });
    assert!(
        exploit.contains("has its own Exploit protection settings")
            && exploit.contains("remove every F1_25.exe entry, restart the PC"),
        "{exploit}"
    );
    let layers = text(
        &mut r,
        Cause::CompatLayer {
            path: system(),
            layers: vec!["~ WIN8RTM (for this user)".into()],
            shims: vec!["AcLayers.dll".into()],
            from: None,
        },
    );
    assert!(
        layers.contains("compatibility settings are on for F1_25.exe (~ WIN8RTM (for this user))")
            && layers.contains("Change settings for all users"),
        "{layers}"
    );
    let shims = text(
        &mut r,
        Cause::CompatLayer {
            path: system(),
            layers: Vec::new(),
            shims: vec!["AcLayers.dll".into()],
            from: None,
        },
    );
    assert!(
        shims.contains("Windows applied compatibility shims to it (AcLayers.dll)")
            && shims.contains("fully quit the EA app or Steam")
            && !shims.contains("right-click F1_25.exe"),
        "{shims}"
    );
    const EA: &str = r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EADesktop.exe";
    let shimmed = |from: Carrier| Cause::CompatLayer {
        path: system(),
        layers: Vec::new(),
        shims: vec!["AcLayers.DLL".into(), "AcGenral.DLL".into()],
        from: Some(from),
    };
    let ea = text(
        &mut r,
        shimmed(Carrier {
            name: "EADesktop.exe".into(),
            exe: Some(EA.into()),
            layers: vec!["~ RUNASADMIN (for all users)".into()],
        }),
    );
    assert!(
            ea.contains(
                "Windows applied compatibility shims to it (AcLayers.DLL, AcGenral.DLL), passed on from the \
                 compatibility settings on EADesktop.exe (~ RUNASADMIN (for all users))"
            ) && ea.contains(&format!(
                "fully quit the EA app (right-click its icon in the system tray > Exit), right-click EADesktop.exe \
                 ({EA}), open Properties > Compatibility and untick everything"
            )) && ea.contains("Change settings for all users")
                && !ea.contains("F1_25.exe in the game folder"),
            "{ea}"
        );
    let running = text(
        &mut r,
        shimmed(Carrier {
            name: "EADesktop.exe".into(),
            exe: Some(EA.into()),
            layers: Vec::new(),
        }),
    );
    assert!(
        running.contains("passed on from EADesktop.exe, which runs with them too")
            && running.contains("then press Play in Swofty Launcher without starting the EA app first"),
        "{running}"
    );
    let launcher = text(
        &mut r,
        shimmed(Carrier {
            name: "swofty-launcher.exe".into(),
            exe: Some(r"C:\Swofty\swofty-launcher.exe".into()),
            layers: vec!["~ WIN7RTM (for this user)".into()],
        }),
    );
    assert!(
        launcher.contains("close F1 25 and Swofty Launcher")
            && launcher.contains(r"right-click swofty-launcher.exe (C:\Swofty\swofty-launcher.exe)")
            && launcher.contains("then start Swofty Launcher and launch again"),
        "{launcher}"
    );
    let av = text(
        &mut r,
        Cause::Antivirus {
            path: system(),
            names: vec!["Reason Cybersecurity".into()],
        },
    );
    assert!(
        av.contains("Reason Cybersecurity, the antivirus on this PC, most likely redirected it")
            && av.contains("open Reason Cybersecurity, restore or allow version.dll"),
        "{av}"
    );
    let unexplained = text(
        &mut r,
        Cause::Unexplained {
            path: system(),
            hooks: vec!["Medal".into()],
        },
    );
    assert!(
        unexplained.contains("Swofty Launcher couldn't tell why")
            && unexplained.contains("Close Medal before you press Play")
            && unexplained.contains("support Discord")
            && !unexplained.contains("antivirus"),
        "{unexplained}"
    );
    assert!(
        Cause::Unexplained {
            path: system(),
            hooks: Vec::new()
        }
        .unexplained()
    );
    let unreadable = text(&mut r, Cause::Unreadable);
    assert!(
        unreadable.contains("access denied")
            && unreadable.contains("Send Logs")
            && !unreadable.contains("antivirus"),
        "{unreadable}"
    );
    assert!(Cause::Unreadable.unexplained());
    r.removed = true;
    assert!(
        r.text()
            .starts_with("F1 25 is running without mods: version.dll was removed")
    );
    r.cause = None;
    r.removed = false;
    assert_eq!(r.text(), fallback);
    assert_eq!(r.title(), "Mods did not load");
}

#[test]
fn names_what_makes_the_game_prefer_system32_and_the_fix_for_it() {
    let r = |from: Option<PreferSource>| {
        Report {
            at: 1.0,
            stage: Stage::InGame,
            files: vec!["version.dll".into()],
            removed: false,
            game: PathBuf::from(r"E:\SteamLibrary\steamapps\common\F1 25"),
            launcher: None,
            cause: Some(Box::new(Cause::OtherCopy {
                path: r"C:\WINDOWS\SYSTEM32\VERSION.dll".into(),
                prefer_system32: true,
                from,
            })),
        }
        .text()
    };
    const LOADED: &str = "F1 25 is running without mods: the game loaded C:\\WINDOWS\\SYSTEM32\\VERSION.dll \
                              instead of the game folder's version.dll: ";
    assert_eq!(
        r(None),
        format!(
            "{LOADED}Windows makes F1_25.exe prefer System32 DLLs, and Swofty Launcher found no Exploit protection \
                 setting that asks for that. To fix it, close F1 25, restart the PC, then launch again. If mods still \
                 don't load, run Get-ProcessMitigation -Name F1_25.exe and Get-ProcessMitigation -System in \
                 PowerShell and post what they print in the support Discord."
        )
    );
    assert_eq!(
        r(Some(PreferSource::Program)),
        format!(
            "{LOADED}F1_25.exe's Exploit protection program settings make it prefer System32 DLLs. To fix it, \
                 close F1 25, remove the F1_25.exe settings in Windows Security > App & browser control > Exploit \
                 protection > Program settings, then launch again."
        )
    );
    assert_eq!(
        r(Some(PreferSource::Path {
            path: r"E:\SteamLibrary\steamapps\common\F1 25\F1_25.exe".into()
        })),
        format!(
            "{LOADED}the Exploit protection program settings for \
                 E:\\SteamLibrary\\steamapps\\common\\F1 25\\F1_25.exe make F1_25.exe prefer System32 DLLs. To \
                 fix it, close F1 25, remove the F1_25.exe entry for \
                 E:\\SteamLibrary\\steamapps\\common\\F1 25\\F1_25.exe in Windows Security > App & browser \
                 control > Exploit protection > Program settings, then launch again."
        )
    );
    assert_eq!(
        r(Some(PreferSource::System)),
        format!(
            "{LOADED}Exploit protection's system settings make every program prefer System32 DLLs. To fix it, \
                 close F1 25, run Set-ProcessMitigation -System -Disable PreferSystem32 in PowerShell as \
                 administrator, restart the PC, then launch again. If that command reports an error, or mods still \
                 don't load, run Get-ProcessMitigation -System in PowerShell and post what it prints in the support \
                 Discord."
        )
    );
    assert_eq!(
        r(Some(PreferSource::Policy {
            from: Some(r"C:\Policies\exploit.xml".into())
        })),
        format!(
            "{LOADED}an Exploit protection policy set by an organisation or a tool (C:\\Policies\\exploit.xml) \
                 makes F1_25.exe prefer System32 DLLs. To fix it, close F1 25, ask whoever manages this PC to remove \
                 PreferSystem32 for F1_25.exe from the policy, or undo the tool that set it, then restart the PC and \
                 launch again."
        )
    );
    let managed = r(Some(PreferSource::Policy { from: None }));
    assert!(
        managed.contains("an Exploit protection policy set by an organisation or a tool makes F1_25.exe"),
        "{managed}"
    );
    for from in [
        None,
        Some(PreferSource::System),
        Some(PreferSource::Policy { from: None }),
    ] {
        let text = r(from);
        assert!(!text.contains("Program settings"), "{text}");
    }
}

#[test]
fn an_install_without_a_loader_folder_says_reinstall() {
    let root = temp("no-loader");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    assert_eq!(damaged_in(&bin), [LOADER_FOLDER]);
    assert!(damaged_in(&root.join("debug")).is_empty(), "a source build");
    std::fs::create_dir_all(root.join("loader")).unwrap();
    assert_eq!(
        damaged_in(&bin),
        ["loader/version.dll", "loader/swofty/addons/core/swofty_core.dll"]
    );
    let r = Report::new(
        Stage::BeforeLaunch,
        vec![LOADER_FOLDER.into()],
        Path::new("/games/F1 25"),
    );
    assert_eq!(r.title(), "Swofty Launcher's loader folder is missing");
    assert!(r.text().contains("reinstall Swofty Launcher"), "{}", r.text());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_report_keeps_its_cause_and_reads_one_without() {
    let mut r = Report::new(
        Stage::InGame,
        vec!["version.dll".into()],
        Path::new("/games/F1 25"),
    );
    r.cause = Some(Box::new(Cause::OtherCopy {
        path: r"C:\WINDOWS\SYSTEM32\VERSION.dll".into(),
        prefer_system32: false,
        from: None,
    }));
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["cause"]["kind"], "other_copy");
    assert!(json["cause"].get("from").is_none());
    assert_eq!(serde_json::from_value::<Report>(json).unwrap(), r);
    r.cause = Some(Box::new(Cause::OtherCopy {
        path: r"C:\WINDOWS\SYSTEM32\VERSION.dll".into(),
        prefer_system32: true,
        from: Some(PreferSource::Path {
            path: r"D:\Games\F1 25\F1_25.exe".into(),
        }),
    }));
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["cause"]["from"]["kind"], "path");
    assert_eq!(serde_json::from_value::<Report>(json).unwrap(), r);
    let old = r#"{"at":1.0,"stage":"in_game","files":["version.dll"],"game":"/games/F1 25"}"#;
    assert_eq!(serde_json::from_str::<Report>(old).unwrap().cause, None);
    r.cause = None;
    assert!(serde_json::to_value(&r).unwrap().get("cause").is_none());
}

#[test]
fn a_report_survives_a_round_trip_and_clears() {
    let data = temp("report");
    let r = Report::new(
        Stage::InGame,
        vec!["version.dll".into()],
        Path::new("/games/F1 25"),
    );
    r.write(&data).unwrap();
    let back = Report::read(&data).unwrap();
    assert!((back.at - r.at).abs() < 1e-3);
    assert_eq!(Report { at: r.at, ..back }, r);
    Report::clear(&data).unwrap();
    assert_eq!(Report::read(&data), None);
    Report::clear(&data).unwrap();
    std::fs::remove_dir_all(&data).ok();
}
