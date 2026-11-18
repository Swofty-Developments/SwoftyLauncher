use super::*;

#[test]
fn an_attached_loader_waiting_for_the_window_is_not_a_miss() {
    let f = waiting();
    assert_eq!(
        f.attached,
        Some(Attached {
            pid: 12320,
            at: Some("19:02:11.120".into()),
            window: false,
            window_after: None,
            last: Some(
                "fallback watcher: waiting for a visible main window if no trigger fires (timeout 300 s)"
                    .into()
            ),
            last_after: Some(Duration::from_millis(300)),
            ran: Some(Duration::from_secs(46)),
            proof: Proof::Running,
        })
    );
    let text = f.attached_text().unwrap();
    assert!(
        text.starts_with("The mod loader is running in F1 25 and is waiting for the game window."),
        "{text}"
    );
    assert!(!text.contains("did not load"), "{text}");
    let lines = f.lines(&game());
    assert!(
        lines.contains(
            &"loader.log: the loader attached at 19:02:11.120, pid 12320 (a running F1_25.exe), and is waiting \
              for the game window: no window stage line yet; the game was started 46 s ago; its last line, 0.3 s \
              after it attached: fallback watcher: waiting for a visible main window if no trigger fires \
              (timeout 300 s)"
                .to_owned()
        ),
        "{lines:#?}"
    );
}

#[test]
fn an_attached_loader_past_the_window_stage_says_how_long_it_took() {
    let log = format!(
        "{WAITING}2026-11-14 00:00:02.620 [ 8124] stage: window (USER32!CreateWindowExW), addons were loaded \
         at the early stage\r\n"
    );
    let a = attached(&log, &[], true).unwrap();
    assert_eq!(a.proof, Proof::Rotated);
    assert!(a.window);
    assert_eq!(a.window_after, Some(Duration::from_millis(17_871_500)));
    assert!(
        a.line()
            .contains("and reached the window stage 17871 s after it attached"),
        "{}",
        a.line()
    );
    let f = Findings {
        attached: Some(a),
        ..Findings::default()
    };
    assert_eq!(
        f.attached_text(),
        Some("The mod loader started in F1 25 and reached the game window, where the mods load.")
    );
}

#[test]
fn a_log_with_no_attach_line_is_still_a_miss() {
    let stub = "18:27:08.903 [stub] [offline] game created (pid 12320)\n\
                18:27:09.211 [stub] [offline] game image at 0x140000000, patching\n";
    assert_eq!(attached(stub, &[12320], true), None);
    assert_eq!(attached("", &[12320], true), None);
    let f = found(
        Vec::new(),
        vec![running(&[&format!(r"{GAME}\version.dll")], Mitigation::default())],
    );
    assert_eq!(f.attached_text(), None);
    assert!(matches!(f.cause(&game()), Some(Cause::LoadedSilently { .. })));
}

#[test]
fn an_attach_line_from_an_earlier_launch_is_still_a_miss() {
    assert_eq!(attached(WAITING, &[27488], false), None);
    let appended = format!(
        "{WAITING}18:27:08.903 [stub] [offline] game created (pid 27488)\n\
         18:27:09.211 [stub] [offline] game image at 0x140000000, patching\n"
    );
    assert_eq!(attached(&appended, &[27488], false), None);
    let clobbered = ".dll proxy) attached, pid 12316\r\n2026-11-13 19:02:11.121 [ 8124] host: F1_25.exe\r\n";
    assert_eq!(attached(clobbered, &[27488], false), None);
    let a = attached(clobbered, &[12316], false).unwrap();
    assert_eq!((a.pid, a.at, a.last_after), (12316, None, None));
    assert_eq!(a.last.as_deref(), Some("host: F1_25.exe"));
}

#[test]
fn collect_takes_an_attach_line_only_from_a_log_begun_this_launch() {
    let dir = std::env::temp_dir().join(format!("swofty-loader-attached-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let game = dir.join("game");
    std::fs::create_dir_all(game.join("swofty")).unwrap();
    let log = game.join(LOADER_LOG);
    let prev = log.with_file_name("loader.prev.log");
    let touch = |path: &Path, at: SystemTime| {
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    };
    let since = SystemTime::now() - Duration::from_secs(46);
    let earlier = since - Duration::from_secs(3600);
    std::fs::write(&log, WAITING).unwrap();
    touch(&log, earlier);
    assert_eq!(collect(&game, None, since, None).attached, None);
    std::fs::write(&prev, "the log before it\n").unwrap();
    touch(&prev, earlier);
    std::fs::write(
        &log,
        format!("{WAITING}18:27:08.903 [stub] [offline] game created (pid 27488)\n"),
    )
    .unwrap();
    let f = collect(&game, None, since, None);
    assert!(!f.rotated);
    assert_eq!(f.attached, None);
    assert_eq!(f.attached_text(), None);
    std::fs::write(&prev, "18:27:08.903 [stub] [offline] game created (pid 12320)\n").unwrap();
    std::fs::write(&log, WAITING).unwrap();
    let f = collect(&game, None, since, None);
    assert!(f.rotated);
    let a = f.attached.as_ref().unwrap();
    assert_eq!((a.pid, a.proof, a.window), (12320, Proof::Rotated, false));
    assert!(a.ran.is_some_and(|d| d >= Duration::from_secs(46)));
    assert_eq!(
        f.attached_text(),
        Some("The mod loader started in F1 25 and was waiting for the game window, where the mods load.")
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn collects_what_a_game_folder_shows() {
    let dir = std::env::temp_dir().join(format!("swofty-loader-check-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join("game/swofty")).unwrap();
    let game = dir.join("game");
    std::fs::write(game.join(PROXY), b"proxy").unwrap();
    std::fs::write(game.join(GAME_EXE), b"MZ exe").unwrap();
    std::fs::write(
        game.join(LOADER_LOG),
        "18:27:08.903 [stub] [offline] game created (pid 27488)\n",
    )
    .unwrap();
    let reg = dir.join("user.reg");
    std::fs::write(
        &reg,
        format!(
            "WINE REGISTRY Version 2\n\n{}\n\"version\"=\"native,builtin\"\n",
            crate::install::OVERRIDE_HEADER
        ),
    )
    .unwrap();
    let placed = sha256_file(&game.join(PROXY)).unwrap();
    let before = SystemTime::now() - Duration::from_secs(5);
    let f = collect(&game, Some(&placed), before, Some(&reg));
    assert_eq!(
        f.proxy,
        Proxy::Present {
            size: 5,
            same: Some(true),
            zone: None
        }
    );
    assert_eq!(f.exe_size, Some(6));
    assert_eq!(f.created, Some(27488));
    assert!(!f.rotated);
    assert_eq!(f.wine_override, (!cfg!(windows)).then_some(true));
    std::fs::write(game.join("swofty/loader.prev.log"), "moved aside").unwrap();
    std::fs::write(game.join(PROXY), b"changed").unwrap();
    std::fs::write(
        &reg,
        format!(
            "WINE REGISTRY Version 2\n\n{} 1792606023\n#time=1dd6186fa06f04c\n",
            crate::install::OVERRIDE_HEADER
        ),
    )
    .unwrap();
    let f = collect(&game, Some(&placed), before, Some(&reg));
    assert!(f.rotated);
    assert!(matches!(
        f.proxy,
        Proxy::Present {
            same: Some(false),
            ..
        }
    ));
    assert_eq!(f.wine_override, (!cfg!(windows)).then_some(false));
    std::fs::remove_file(game.join(PROXY)).unwrap();
    assert_eq!(collect(&game, None, before, None).proxy, Proxy::Missing);
    std::fs::remove_dir_all(&dir).ok();
}
