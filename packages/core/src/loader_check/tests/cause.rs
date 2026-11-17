use super::*;

#[test]
fn a_blocking_event_about_version_dll_names_the_cause() {
    let xml = ci(3077, PROXY_PATH, Some("VerifiedAndReputableDesktop"));
    let f = found(
        events(&xml, &game()),
        vec![running(&[SYSTEM], Mitigation::default())],
    );
    assert_eq!(f.cause(&game()), Some(Cause::SmartAppControl { seen: true }));
    let xml = ci(3077, PROXY_PATH, Some("Contoso Games Policy"));
    let f = found(events(&xml, &game()), Vec::new());
    assert_eq!(
        f.cause(&game()),
        Some(Cause::Policy {
            name: Some("Contoso Games Policy".into())
        })
    );
    let mut f = found(events(&ci(3077, PROXY_PATH, None), &game()), Vec::new());
    assert_eq!(f.cause(&game()), Some(Cause::Policy { name: None }));
    f.smart_app_control = Some(Ok(Some(1)));
    assert_eq!(f.cause(&game()), Some(Cause::SmartAppControl { seen: true }));
    let f = found(events(&ci(3033, PROXY_PATH, None), &game()), Vec::new());
    assert_eq!(f.cause(&game()), Some(Cause::SignedOnly));
    let defender = Event {
        id: 1121,
        file: Some(format!(r"{GAME}\version.dll")),
        proxy: true,
        ..Event::default()
    };
    assert_eq!(
        found(vec![defender], Vec::new()).cause(&game()),
        Some(Cause::Defender)
    );
}

#[test]
fn audits_and_events_about_other_files_name_no_cause() {
    let audit = events(
        &ci(3076, PROXY_PATH, Some("VerifiedAndReputableDesktop")),
        &game(),
    );
    let core = events(
        &ci(
            3033,
            r"\Device\HarddiskVolume3\SteamLibrary\steamapps\common\F1 25\swofty\x.dll",
            None,
        ),
        &game(),
    );
    let loaded = running(&[&format!(r"{GAME}\version.dll"), SYSTEM], Mitigation::default());
    let f = found([audit, core].concat(), vec![loaded]);
    assert!(matches!(f.cause(&game()), Some(Cause::LoadedSilently { .. })));
}

#[test]
fn the_game_process_names_the_cause_when_no_event_does() {
    let signed = Mitigation {
        microsoft_signed: true,
        ..Mitigation::default()
    };
    let f = found(Vec::new(), vec![running(&[SYSTEM], signed)]);
    assert_eq!(f.cause(&game()), Some(Cause::SignedOnly));
    let mut f = found(Vec::new(), vec![running(&[SYSTEM], Mitigation::default())]);
    f.search = Some(Ok(Search {
        known_dll: true,
        safe_search: None,
    }));
    assert_eq!(f.cause(&game()), Some(Cause::KnownDll));
    let f = found(Vec::new(), vec![running(&[SYSTEM], Mitigation::default())]);
    assert_eq!(
        f.cause(&game()),
        Some(Cause::Unexplained {
            path: SYSTEM.into(),
            hooks: Vec::new()
        })
    );
    let prefer = Mitigation {
        prefer_system32: true,
        ..Mitigation::default()
    };
    let f = found(Vec::new(), vec![running(&[SYSTEM], prefer)]);
    assert_eq!(
        f.cause(&game()),
        Some(Cause::OtherCopy {
            path: SYSTEM.into(),
            prefer_system32: true,
            from: None,
        })
    );
}

#[test]
fn names_what_turns_on_prefer_system32() {
    let prefer = Mitigation {
        prefer_system32: true,
        ..Mitigation::default()
    };
    let from = |f: &Findings| match f.cause(&game()) {
        Some(Cause::OtherCopy {
            path,
            prefer_system32: true,
            from,
        }) if path == SYSTEM => from,
        other => panic!("{other:?}"),
    };
    let mut f = found(Vec::new(), vec![running(&[SYSTEM], prefer)]);
    f.exec_options = Some(Ok(vec!["MaxLoaderThreads".into(), "UseFilter".into()]));
    f.prefer_from = Some(Ok(Vec::new()));
    assert_eq!(from(&f), None);
    f.prefer_from = None;
    assert_eq!(from(&f), None);
    f.prefer_from = Some(Err("access denied".into()));
    assert_eq!(from(&f), None);
    let lines = f.lines(&game());
    assert!(
        lines.contains(&"Image File Execution Options for F1_25.exe: MaxLoaderThreads, UseFilter".to_owned())
            && lines.contains(&"prefer System32 DLLs: settings not readable (access denied)".to_owned()),
        "{lines:#?}"
    );
    f.prefer_from = Some(Ok(Vec::new()));
    assert!(
        f.lines(&game()).contains(
            &"prefer System32 DLLs: not turned on in the Exploit protection program or system settings or by an \
              Exploit protection policy for F1_25.exe"
                .to_owned()
        )
    );
    f.exec_options = Some(Ok(vec![
        "MaxLoaderThreads".into(),
        "UseFilter".into(),
        r"{f05071b2-1d0e-4f5a-9b3c-000000000000}\FilterFullPath".into(),
        r"{f05071b2-1d0e-4f5a-9b3c-000000000000}\MitigationOptions".into(),
    ]));
    let exe = format!(r"{GAME}\F1_25.exe");
    f.prefer_from = Some(Ok(vec![
        PreferSource::Path { path: exe.clone() },
        PreferSource::System,
    ]));
    assert_eq!(from(&f), Some(PreferSource::Path { path: exe.clone() }));
    assert!(f.lines(&game()).contains(&format!(
        "prefer System32 DLLs: turned on by the Exploit protection program settings for {exe} and Exploit \
             protection's system settings"
    )));
    f.prefer_from = None;
    assert_eq!(from(&f), Some(PreferSource::Program));
    f.prefer_from = Some(Ok(vec![PreferSource::System]));
    assert_eq!(from(&f), Some(PreferSource::System));
    f.prefer_from = Some(Ok(vec![PreferSource::Policy { from: None }]));
    assert_eq!(from(&f), Some(PreferSource::Policy { from: None }));
}

#[test]
fn reads_prefer_system32_from_mitigation_options() {
    let mut options = [0u8; 24];
    assert!(!prefers_system32(&options));
    options[7] = 0x10;
    assert!(prefers_system32(&options));
    options[7] = 0x20;
    assert!(!prefers_system32(&options), "always off");
    options[7] = 0x11;
    options[1] = 0x01;
    assert!(prefers_system32(&options));
    assert!(prefers_system32(&((1u64 << 60) | 0x100).to_le_bytes()));
    assert!(!prefers_system32(&((1u64 << 56) | (1u64 << 52)).to_le_bytes()));
    assert!(!prefers_system32(&[0, 0, 0, 0x10]));
    assert!(!prefers_system32(&[]));
}

const POLICY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<MitigationPolicy>
  <SystemConfig>
    <DEP Enable="true" EmulateAtlThunks="false" />
  </SystemConfig>
  <AppConfig Executable="chrome.exe">
    <ImageLoad PreferSystem32="true" />
  </AppConfig>
  <AppConfig Executable="E:\SteamLibrary\steamapps\common\F1 25\F1_25.exe">
    <ImageLoad BlockRemoteImageLoads="true" PreferSystem32="true" />
  </AppConfig>
</MitigationPolicy>"#;

#[test]
fn reads_prefer_system32_from_an_exploit_protection_policy() {
    assert!(policy_prefers(POLICY));
    let others = POLICY.replace(
        r#" PreferSystem32="true" />
  </AppConfig>
</MitigationPolicy>"#,
        " />\n  </AppConfig>\n</MitigationPolicy>",
    );
    assert!(!policy_prefers(&others), "{others}");
    assert!(!policy_prefers(&others.replace("F1_25.exe", "F1_24.exe")));
    assert!(policy_prefers(
        &others.replace("<DEP ", "<ImageLoad PreferSystem32='true' /><DEP ")
    ));
    assert!(policy_prefers(&POLICY.replace("F1_25.exe\">", "f1_25.EXE\">")));
    assert_eq!(policy_source(""), None);
    assert_eq!(policy_source(POLICY), Some(PreferSource::Policy { from: None }));
    assert_eq!(policy_source(&others), None);
    let url = "https://contoso.example/exploit-protection.xml";
    assert_eq!(
        policy_source(url),
        Some(PreferSource::Policy {
            from: Some(url.into())
        })
    );
    let dir = std::env::temp_dir().join(format!("swofty-policy-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("settings.xml");
    let utf16: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain(POLICY.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    assert_eq!(xml_text(&utf16), POLICY);
    std::fs::write(&file, &utf16).unwrap();
    let at = file.to_string_lossy().into_owned();
    assert_eq!(
        policy_source(&at),
        Some(PreferSource::Policy {
            from: Some(tidy(&at))
        })
    );
    std::fs::write(&file, &others).unwrap();
    assert_eq!(policy_source(&at), None);
    std::fs::remove_dir_all(&dir).ok();
    let missing = dir.join("gone.xml").to_string_lossy().into_owned();
    assert_eq!(
        policy_source(&missing),
        Some(PreferSource::Policy {
            from: Some(tidy(&missing))
        })
    );
}

#[test]
fn a_game_it_cannot_look_inside_is_not_blamed_on_an_antivirus() {
    let denied = Seen {
        image: None,
        image_size: None,
        ours: false,
        modules: Err("access denied".into()),
        mitigation: Some(Err("access denied".into())),
        ..running(&[], Mitigation::default())
    };
    let mut f = found(Vec::new(), vec![denied]);
    assert_eq!(f.cause(&game()), Some(Cause::Unreadable));
    f.antivirus = Some(Ok(vec!["Microsoft Defender Antivirus (on)".into()]));
    assert_eq!(f.cause(&game()), Some(Cause::Unreadable));
    f.antivirus = Some(Ok(vec!["Bitdefender Antivirus (on)".into()]));
    assert_eq!(f.cause(&game()), None);
    f.antivirus = None;
    f.smart_app_control = Some(Ok(Some(1)));
    assert_eq!(f.cause(&game()), Some(Cause::SmartAppControl { seen: false }));
    assert_eq!(found(Vec::new(), Vec::new()).cause(&game()), None);
}

#[test]
fn a_game_gone_before_the_look_is_unknown_and_a_loaded_loader_is_not_blocked() {
    let mut f = found(Vec::new(), Vec::new());
    f.created = Some(28016);
    assert_eq!(
        f.cause(&game()),
        Some(Cause::GameEnded {
            antivirus: Vec::new()
        })
    );
    f.antivirus = Some(Ok(vec!["Bitdefender Antivirus (on)".into()]));
    assert_eq!(
        f.cause(&game()),
        Some(Cause::GameEnded {
            antivirus: vec!["Bitdefender Antivirus".into()]
        })
    );
    let ours = format!(r"{GAME}\version.dll");
    let f = found(Vec::new(), vec![running(&[ours.as_str()], Mitigation::default())]);
    assert!(matches!(f.cause(&game()), Some(Cause::LoadedSilently { .. })));
}

#[test]
fn silent_loader_names_ifeo_values_and_security_modules() {
    let ours = format!(r"{GAME}\version.dll");
    let mut f = found(Vec::new(), vec![running(&[ours.as_str()], Mitigation::default())]);
    assert_eq!(
        f.cause(&game()),
        Some(Cause::LoadedSilently {
            exec_options: Vec::new(),
            layers: Vec::new(),
            security: Vec::new()
        })
    );
    f.exec_options = Some(Ok(vec!["MaxLoaderThreads".into()]));
    f.layers = Some(Ok(vec!["~ HIGHDPIAWARE (for this user)".into()]));
    let Ok(m) = &mut f.games[0].modules else { panic!() };
    m.others = vec!["mc-sec-plugin-x64.dll".into(), "other.dll".into()];
    assert_eq!(
        f.cause(&game()),
        Some(Cause::LoadedSilently {
            exec_options: vec!["MaxLoaderThreads".into()],
            layers: vec!["~ HIGHDPIAWARE (for this user)".into()],
            security: vec!["mc-sec-plugin-x64.dll".into()]
        })
    );
}

#[test]
fn exploit_protection_settings_explain_another_copy() {
    let mut f = system_copy();
    f.exec_options = Some(Ok(vec!["MaxLoaderThreads".into()]));
    assert!(
        matches!(f.cause(&game()), Some(Cause::Unexplained { .. })),
        "MaxLoaderThreads alone"
    );
    f.exec_options = Some(Ok(vec![
        "MaxLoaderThreads".into(),
        "MitigationOptions".into(),
        "MitigationAuditOptions".into(),
        "EAFModules".into(),
    ]));
    assert_eq!(
        f.cause(&game()),
        Some(Cause::ExploitProtection { path: SYSTEM.into() })
    );
    f.exec_options = Some(Ok(vec![r"0\MitigationOptions".into()]));
    assert_eq!(
        f.cause(&game()),
        Some(Cause::ExploitProtection { path: SYSTEM.into() })
    );
}

#[test]
fn compatibility_settings_or_shims_explain_another_copy() {
    let mut f = system_copy();
    f.layers = Some(Ok(vec!["~ HIGHDPIAWARE (for all users)".into()]));
    assert_eq!(
        f.cause(&game()),
        Some(Cause::CompatLayer {
            path: SYSTEM.into(),
            layers: vec!["~ HIGHDPIAWARE (for all users)".into()],
            shims: Vec::new(),
            from: None
        })
    );
    let mut f = system_copy();
    f.layers = Some(Ok(Vec::new()));
    with_modules(&mut f, &[], &["AcLayers.dll", "AcGenral.dll"]);
    assert!(matches!(f.cause(&game()), Some(Cause::CompatLayer { shims, .. }) if shims.len() == 2));
    let paths: Vec<String> = [
        r"C:\WINDOWS\SYSTEM32\apphelp.dll",
        r"C:\WINDOWS\SYSTEM32\AcLayers.DLL",
        SYSTEM,
    ]
    .map(String::from)
    .to_vec();
    assert_eq!(
        modules(&paths, &game(), Some(r"C:\WINDOWS")).shims,
        ["AcLayers.DLL"]
    );
}

#[test]
fn shims_are_traced_to_the_program_that_starts_the_game() {
    const EA: &str = r"C:\Program Files\Electronic Arts\EA Desktop\EA Desktop\EADesktop.exe";
    let carrier = |f: &Findings| match f.cause(&game()) {
        Some(Cause::CompatLayer { from, .. }) => from,
        other => panic!("{other:?}"),
    };
    let mut f = system_copy();
    f.layers = Some(Ok(Vec::new()));
    with_modules(&mut f, &[], &["AcLayers.DLL", "AcGenral.DLL"]);
    f.starter_layers = Some(Vec::new());
    assert_eq!(carrier(&f), None);

    f.ea_app = vec![
        EaApp {
            pid: 900,
            image: None,
            shims: Err("access denied".into()),
        },
        EaApp {
            pid: 901,
            image: Some(EA.into()),
            shims: Ok(vec!["AcLayers.DLL".into()]),
        },
    ];
    assert_eq!(
        carrier(&f),
        Some(Carrier {
            name: "EADesktop.exe".into(),
            exe: Some(EA.into()),
            layers: Vec::new(),
        })
    );

    f.starter_layers = Some(vec![
        starter("EADesktop.exe", EA, "~ RUNASADMIN", false),
        starter("EADesktop.exe", EA, "WIN8RTM", true),
        starter(
            "swofty-launcher.exe",
            r"C:\Swofty\swofty-launcher.exe",
            "~ WIN7RTM",
            false,
        ),
    ]);
    assert_eq!(
        carrier(&f),
        Some(Carrier {
            name: "EADesktop.exe".into(),
            exe: Some(EA.into()),
            layers: vec![
                "~ RUNASADMIN (for this user)".into(),
                "WIN8RTM (for all users)".into()
            ],
        })
    );
    f.starter_layers = Some(vec![starter(
        "swofty-launcher.exe",
        r"C:\Swofty\swofty-launcher.exe",
        "~ WIN7RTM",
        false,
    )]);
    assert!(matches!(carrier(&f), Some(c) if c.name == "swofty-launcher.exe"));

    f.layers = Some(Ok(vec!["~ HIGHDPIAWARE (for this user)".into()]));
    assert_eq!(carrier(&f), None, "a setting on F1_25.exe itself comes first");

    let lines = f.lines(&game());
    assert!(
        lines.contains(&r"compatibility settings on swofty-launcher.exe: ~ WIN7RTM (for this user) on C:\Swofty\swofty-launcher.exe".to_owned()),
        "{lines:#?}"
    );
    assert!(lines.contains(&"EADesktop.exe pid 900: modules not readable (access denied)".to_owned()));
    assert!(
        lines.contains(&"EADesktop.exe pid 901: Windows compatibility shims loaded: AcLayers.DLL".to_owned())
    );
    f.starter_layers = Some(Vec::new());
    assert!(f.lines(&game()).contains(
        &"compatibility settings on EADesktop.exe, EALauncher.exe, swofty-launcher.exe and the offline play stub: none"
            .to_owned()
    ));
}

#[test]
fn only_a_third_party_antivirus_is_named() {
    let mut f = system_copy();
    f.antivirus = Some(Ok(vec!["Microsoft Defender Antivirus (on)".into()]));
    assert!(matches!(f.cause(&game()), Some(Cause::Unexplained { .. })));
    f.antivirus = Some(Ok(vec![
        "Microsoft Defender Antivirus (off)".into(),
        "Reason Cybersecurity (on)".into(),
        "Norton Security (off)".into(),
    ]));
    assert_eq!(
        f.cause(&game()),
        Some(Cause::Antivirus {
            path: SYSTEM.into(),
            names: vec!["Reason Cybersecurity".into()]
        })
    );
    f.antivirus = Some(Ok(vec!["Bitdefender Antivirus (on)".into()]));
    assert!(matches!(f.cause(&game()), Some(Cause::Antivirus { .. })));
}

#[test]
fn an_unexplained_copy_names_the_hooks_in_the_game() {
    let mut f = system_copy();
    with_modules(
        &mut f,
        &["medal-hook64-v2.dll", "RTSSHooks64.dll", "160_E658703.bin"],
        &[],
    );
    assert_eq!(
        f.cause(&game()),
        Some(Cause::Unexplained {
            path: SYSTEM.into(),
            hooks: vec!["Medal".into(), "RivaTuner Statistics Server".into()]
        })
    );
}

#[test]
fn smart_app_control_on_is_the_cause_only_when_nothing_else_is() {
    let ours = format!(r"{GAME}\version.dll");
    let mut f = found(Vec::new(), vec![running(&[&ours, SYSTEM], Mitigation::default())]);
    assert!(matches!(f.cause(&game()), Some(Cause::LoadedSilently { .. })));
    f.smart_app_control = Some(Ok(Some(1)));
    assert_eq!(f.cause(&game()), Some(Cause::SmartAppControl { seen: false }));
    f.smart_app_control = Some(Ok(Some(2)));
    assert!(matches!(f.cause(&game()), Some(Cause::LoadedSilently { .. })));
    f.games = vec![running(&[SYSTEM], Mitigation::default())];
    f.smart_app_control = Some(Ok(Some(1)));
    assert!(matches!(f.cause(&game()), Some(Cause::Unexplained { .. })));
    assert_eq!(Findings::default().cause(&game()), None);
}
