use super::*;

#[test]
fn writes_the_findings_as_lines() {
    let mut g = running(&[SYSTEM], Mitigation::default());
    if let Ok(m) = g.modules.as_mut() {
        m.others = vec!["aswhook.dll".into()];
    }
    let f = Findings {
        proxy: Proxy::Present {
            size: 412_160,
            same: Some(true),
            zone: Some(3),
        },
        exe_size: Some(423_518_208),
        created: Some(27488),
        games: vec![g],
        search: Some(Ok(Search::default())),
        exec_options: Some(Ok(Vec::new())),
        prefer_from: Some(Ok(Vec::new())),
        smart_app_control: Some(Ok(None)),
        logs: vec![
            EventLog {
                name: "Code Integrity",
                read: Ok(events(
                    &ci(3077, PROXY_PATH, Some("VerifiedAndReputableDesktop")),
                    &game(),
                )),
            },
            EventLog {
                name: "Defender",
                read: Err("access denied".into()),
            },
        ],
        antivirus: Some(Ok(vec!["Avast Antivirus (on)".into()])),
        ..Findings::default()
    };
    assert_eq!(
        f.lines(&game()),
        [
            "version.dll: in the game folder, 412160 bytes, the bytes the launcher placed, marked as downloaded \
             (Zone.Identifier ZoneId=3)"
                .to_owned(),
            format!(
                r"game pid 27488 (the one the offline play stub created): {GAME}\F1_25.exe (423518208 bytes), the game folder's F1_25.exe"
            ),
            format!("pid 27488: 212 modules, version.dll from {SYSTEM}"),
            "pid 27488: modules from outside the game and Windows folders: aswhook.dll".to_owned(),
            "pid 27488: DLL load policies: none".to_owned(),
            "DLL search: version.dll is not a KnownDLL, SafeDllSearchMode default (on)".to_owned(),
            "Image File Execution Options for F1_25.exe: none".to_owned(),
            "prefer System32 DLLs: not turned on in the Exploit protection program or system settings or by an \
             Exploit protection policy for F1_25.exe"
                .to_owned(),
            "Smart App Control: not set".to_owned(),
            "Code Integrity log: 1 event(s) about version.dll or F1_25.exe since the game started".to_owned(),
            format!(
                "  Code Integrity 3077 at 16:27:09 UTC: {PROXY_PATH} in F1_25.exe: blocked by an App Control \
                 policy (VerifiedAndReputableDesktop)"
            ),
            "Defender log: not readable (access denied)".to_owned(),
            "antivirus registered with Windows: Avast Antivirus (on)".to_owned(),
        ]
    );
}

#[test]
fn writes_what_proton_shows() {
    let f = Findings {
        proxy: Proxy::Present {
            size: 10,
            same: Some(false),
            zone: None,
        },
        created: Some(4242),
        wine_override: Some(false),
        ..Findings::default()
    };
    assert_eq!(
        f.lines(Path::new("/games/F1 25")),
        [
            "version.dll: in the game folder, 10 bytes, NOT the bytes the launcher placed",
            "game: no F1_25.exe is running",
            "game: the offline play stub created pid 4242, which is not running",
            "Proton prefix: user.reg lacks the version.dll override, so Wine uses its builtin version.dll",
        ]
    );
    let ours = Seen {
        pid: 7,
        image: None,
        image_size: None,
        ours: false,
        modules: Ok(Modules {
            count: 3,
            version: vec!["/games/F1 25/version.dll".into()],
            others: Vec::new(),
            shims: Vec::new(),
        }),
        mitigation: None,
    };
    let f = Findings {
        proxy: Proxy::Missing,
        games: vec![ours],
        ..Findings::default()
    };
    assert_eq!(
        f.lines(Path::new("/games/F1 25")),
        [
            "version.dll: gone from the game folder",
            "game pid 7: exe path unknown, NOT the game folder's F1_25.exe",
            "pid 7: 3 modules, version.dll from the game folder's (the loader)",
        ]
    );
}
