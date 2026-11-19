use super::*;

#[test]
fn reads_the_pid_the_stub_created() {
    let log = "18:27:06.887 [stub] [offline] session launcher invoked, args: <none>\n\
               18:27:08.903 [stub] [offline] game created (pid 19420)\n\
               18:27:09.903 [stub] [offline] game created (pid 27488)\n\
               18:27:09.375 [stub] [offline] image not listed while suspended; resuming\n";
    assert_eq!(created_pid(log), Some(27488));
    assert_eq!(created_pid("nothing here\n"), None);
}

#[test]
fn reads_code_integrity_and_defender_events_about_the_game() {
    let defender = format!(
        "<Event xmlns='http://schemas.microsoft.com/win/2004/08/events/event'><System><EventID>1116</EventID>\
         <TimeCreated SystemTime=\"2026-10-23T16:27:11.000Z\"/></System><EventData>\
         <Data Name='Threat Name'>Trojan:Win64/Example!MTB</Data>\
         <Data Name='Path'>file:_{GAME}\\version.dll</Data><Data Name='Process Name'>Unknown</Data>\
         <Data Name='Detection User'></Data></EventData></Event>"
    );
    let other = ci(
        3077,
        r"\Device\HarddiskVolume2\Tools\thing.dll",
        Some("Other &amp; Co"),
    )
    .replace("steamapps\\common\\F1 25\\F1_25.exe", "Tools\\thing.exe")
    .replace("SteamLibrary\\", "");
    let xml = format!(
        "{}{other}{}{defender}",
        ci(3077, PROXY_PATH, Some("VerifiedAndReputableDesktop")),
        ci(
            3033,
            r"\Device\HarddiskVolume3\SteamLibrary\steamapps\common\F1 25\swofty\addons\core\swofty_core.dll",
            None
        )
    );
    let got = events(&xml, &game());
    assert_eq!(got.len(), 3, "{got:#?}");
    assert_eq!(
        got[0],
        Event {
            id: 3077,
            at: Some("16:27:09 UTC".into()),
            file: Some(PROXY_PATH.into()),
            process: Some(r"\Device\HarddiskVolume3\SteamLibrary\steamapps\common\F1 25\F1_25.exe".into()),
            policy: Some("VerifiedAndReputableDesktop".into()),
            threat: None,
            proxy: true,
        }
    );
    assert_eq!(got[1].id, 3033);
    assert!(!got[1].proxy && got[1].policy.is_none());
    assert_eq!(got[2].id, 1116);
    assert_eq!(got[2].at.as_deref(), Some("16:27:11 UTC"));
    assert_eq!(got[2].threat.as_deref(), Some("Trojan:Win64/Example!MTB"));
    assert_eq!(got[2].file, Some(format!(r"file:_{GAME}\version.dll")));
    assert!(got[2].proxy && got[2].blocks());
    assert!(events("", &game()).is_empty());
    assert!(
        events(
            "<Event xmlns='x'><System><EventID>nope</EventID>version.dll",
            &game()
        )
        .is_empty()
    );
}

#[test]
fn sorts_a_process_module_list() {
    let paths: Vec<String> = [
        format!(r"{GAME}\F1_25.exe"),
        format!(r"{GAME}\version.dll"),
        format!(r"{GAME}\swofty\addons\core\swofty_core.dll"),
        SYSTEM.to_owned(),
        r"C:\WINDOWS\System32\DriverStore\FileRepository\nv\nvwgf2umx.dll".to_owned(),
        r"C:\Program Files\Avast Software\Avast\aswhook.dll".to_owned(),
        r"C:\Program Files (x86)\Steam\GameOverlayRenderer64.dll".to_owned(),
        r"c:\program files\avast software\avast\ASWHOOK.DLL".to_owned(),
    ]
    .into();
    let m = modules(&paths, &game(), Some(r"C:\Windows"));
    assert_eq!(m.count, 8);
    assert_eq!(m.version, [format!(r"{GAME}\version.dll"), SYSTEM.to_owned()]);
    assert_eq!(m.others, ["aswhook.dll", "GameOverlayRenderer64.dll"]);
}

#[test]
fn reads_the_version_dll_wine_mapped() {
    let maps = "140000000-140001000 r--p 00000000 103:02 1 /games/F1 25/F1_25.exe\n\
                6ffff0000-6ffff1000 r--p 00000000 103:02 2 /steam/proton/files/lib/wine/x86_64-windows/version.dll\n\
                6ffff1000-6ffff2000 r-xp 00001000 103:02 2 /steam/proton/files/lib/wine/x86_64-windows/version.dll\n\
                6fffe0000-6fffe1000 r--p 00000000 103:02 3 /steam/proton/files/lib/wine/x86_64-windows/kernel32.dll\n\
                7ffe0000-7fff0000 rw-p 00000000 00:00 0 [stack]\n";
    let m = mapped_modules(maps, Path::new("/games/F1 25"));
    assert_eq!(m.count, 2);
    assert_eq!(
        m.version,
        ["/steam/proton/files/lib/wine/x86_64-windows/version.dll"]
    );
    let native = "6ffff0000-6ffff1000 r--p 00000000 103:02 2 /games/F1 25/version.dll\n";
    let m = mapped_modules(native, Path::new("/games/F1 25"));
    assert!(in_folder(&m.version[0], Path::new("/games/F1 25")));
}

#[test]
fn shortens_the_home_folder_in_paths() {
    let short = if cfg!(windows) { "%USERPROFILE%" } else { "~" };
    assert_eq!(
        tidy_with(
            r"C:\Users\Matthijs Prinsen\AppData\Local\x.dll",
            Some(r"C:\Users\Matthijs Prinsen")
        ),
        format!(r"{short}\AppData\Local\x.dll")
    );
    assert_eq!(
        tidy_with(
            r"c:/users/matthijs prinsen/x.dll",
            Some(r"C:\Users\Matthijs Prinsen\")
        ),
        format!("{short}/x.dll")
    );
    assert_eq!(tidy_with(SYSTEM, Some(r"C:\Users\Matthijs Prinsen")), SYSTEM);
    assert_eq!(
        tidy_with(
            r"C:\Users\Matthijs Prinsen2\x.dll",
            Some(r"C:\Users\Matthijs Prinsen")
        ),
        r"C:\Users\Matthijs Prinsen2\x.dll"
    );
    assert_eq!(tidy_with("/home/a/x.dll", None), "/home/a/x.dll");
}

#[test]
fn reads_the_download_mark() {
    assert_eq!(
        zone_id("[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://x\r\n"),
        Some(3)
    );
    assert_eq!(zone_id("[ZoneTransfer]\r\n"), None);
}

#[test]
fn names_the_antivirus_and_whether_it_is_on() {
    assert_eq!(
        antivirus_name("Avast Antivirus", Some(0x41000)),
        "Avast Antivirus (on)"
    );
    assert_eq!(
        antivirus_name("Microsoft Defender Antivirus", Some(0x60100)),
        "Microsoft Defender Antivirus (off)"
    );
    assert_eq!(antivirus_name("Norton", None), "Norton");
}
