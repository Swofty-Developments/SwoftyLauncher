use super::*;

#[test]
fn a_file_another_program_holds_gets_its_own_hint() {
    let (data, game) = (Path::new("/data/Swofty"), Path::new("/games/F1 25"));
    let held = game_refused_text(data, game, true);
    assert!(
        held.starts_with("another program has a file in /games/F1 25 open"),
        "{held}"
    );
    assert!(
        held.contains("F1_25.exe") && held.contains("/data/Swofty"),
        "{held}"
    );
    assert!(!held.contains("administrator"), "{held}");
    assert!(game_refused_text(data, game, false).contains("run Swofty Launcher as administrator"));
}

#[cfg(windows)]
#[test]
fn a_sharing_violation_in_the_game_folder_is_not_an_administrator_problem() {
    let (data, game) = (
        Path::new("C:/data/Swofty"),
        Path::new("E:/SteamLibrary/steamapps/common/F1 25"),
    );
    let held = Error::Io {
        path: game.join("2025_asset_groups/animation_package/scenes/incardriver/cs16/incardriver_f1.erp"),
        source: std::io::Error::from_raw_os_error(32),
    };
    let text = refused_fix(data, game, &held).unwrap();
    assert!(text.starts_with("another program has a file in"), "{text}");
}

#[test]
fn a_missing_or_cut_short_file_names_its_addon_or_the_game() {
    let root = std::env::temp_dir().join(format!("swofty-named-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(root.join("data"));
    let game = root.join("game");
    let addon = store.addons_dir().join("mod.miami24");
    std::fs::create_dir_all(addon.join("track")).unwrap();
    std::fs::write(
        addon.join("swofty.json"),
        r#"{"schema_version":1,"id":"mod.miami24","name":"Igora Drive","version":"0.9.13","author":"t","compat":{"game":"f1_25"}}"#,
    )
    .unwrap();
    let io = |path: PathBuf, kind| Error::Io {
        path,
        source: std::io::Error::from(kind),
    };
    let gone = named(
        &store,
        &game,
        io(
            addon.join("track").join("miami24.erp"),
            std::io::ErrorKind::NotFound,
        ),
    );
    assert_eq!(
        gone.to_string(),
        "Igora Drive (mod.miami24) is missing its file track/miami24.erp. Turn Igora Drive off in this \
         profile, or remove it on the Addons page and install or import it again."
    );
    let short = named(
        &store,
        &game,
        Error::Formats(swofty_formats::Error::IoAt {
            path: addon.join("track/mipmaps/a.mipmaps"),
            source: std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
        }),
    );
    assert!(
        short
            .to_string()
            .starts_with("Igora Drive (mod.miami24) has an incomplete or damaged track/mipmaps/a.mipmaps."),
        "{short}"
    );
    let stock = named(
        &store,
        &game,
        io(
            game.join("2025_asset_groups/a.erp"),
            std::io::ErrorKind::UnexpectedEof,
        ),
    );
    assert!(
        stock
            .to_string()
            .starts_with("2025_asset_groups/a.erp in the game folder is incomplete or damaged; verify"),
        "{stock}"
    );
    let missing = named(
        &store,
        &game,
        io(
            game.join("character_package/a.mipmaps"),
            std::io::ErrorKind::NotFound,
        ),
    );
    assert!(
        missing
            .to_string()
            .starts_with("character_package/a.mipmaps is missing from the game folder; update F1 25"),
        "{missing}"
    );
    let denied = named(
        &store,
        &game,
        io(
            addon.join("track/miami24.erp"),
            std::io::ErrorKind::PermissionDenied,
        ),
    );
    assert!(matches!(denied, Error::Io { .. }), "{denied}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_missing_backup_stops_the_build_and_sends_the_player_through_the_setup_again() {
    let _pin = crate::addons::builtin::LoaderPin::none();
    let root = std::env::temp_dir().join(format!("swofty-build-backup-gone-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join("a.erp"), "stock").unwrap();
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    let records = vec![swofty_formats::gvdb::Record::new(
        "entrants.teams._team_instancecount_",
        0,
        "0",
    )];
    std::fs::write(dir.join(GVDB), Gvdb::new(records).to_bytes()).unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    crate::game_folder::setup::reset(&store.root, &game, false).unwrap();
    crate::game_folder::setup::finish(&store.root).unwrap();
    let objects = Objects {
        root: store.root.join("baseline/objects"),
    };
    let mut baseline = Baseline::new(&game.identity);
    let sha = baseline
        .record(&objects, &dir, "a.erp", &())
        .unwrap()
        .sha256
        .clone();
    baseline.save(&store.root).unwrap();

    build(&store, "default", &game).unwrap();
    assert!(crate::game_folder::setup::done(&store.root));

    std::fs::remove_dir_all(objects.path(&sha).parent().unwrap()).unwrap();
    let e = build(&store, "default", &game).unwrap_err();
    assert_eq!(
        e.to_string(),
        "Swofty's backup of the original a.erp is missing from its data folder. Run the one-time setup \
         again: open Swofty Launcher and press Play, or run `swofty setup reset`, repair the game in the EA \
         app or verify the game files in Steam, then run `swofty setup done`"
    );
    assert!(!crate::game_folder::setup::done(&store.root));
    assert!(
        build(&store, crate::profile::VANILLA, &game).is_ok(),
        "a build without addons reads no backups"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_missing_loader_is_named_instead_of_a_track_limit() {
    assert_eq!(
        loader_missing(Some(Path::new("/apps/swofty"))),
        "Swofty Launcher's game files (version.dll, swofty_core.dll) are missing from /apps/swofty; your antivirus may have removed them. Restore them or run the installer again"
    );
    assert!(loader_missing(None).contains("missing from the launcher's folder;"));
}

#[test]
fn the_build_log_names_the_exe_folder_and_the_loader() {
    assert_eq!(
        loader_line(Some(Path::new("/apps/swofty")), None),
        "loader: none; swofty.exe in /apps/swofty"
    );
    assert_eq!(
        loader_line(Some(Path::new("/apps/swofty")), Some(Path::new("/apps/loader"))),
        "loader: /apps/loader; swofty.exe in /apps/swofty"
    );
}

#[test]
fn missing_archive_warnings_count_the_files() {
    let rels: BTreeSet<String> = ["b/x.erp", "a/x.erp"].map(str::to_owned).into();
    assert_eq!(
        missing_archives("Cams", &rels),
        "Cams: archive changes skipped; 2 of the files it changes are not in F1 25, such as a/x.erp. \
         Turn Cams off if it doesn't work, and tell its author"
    );
}
