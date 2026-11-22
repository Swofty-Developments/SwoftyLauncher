use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::record::*;
use super::*;

#[test]
fn wine_loads_a_native_version_dll_unless_the_player_set_one() {
    assert_eq!(with_version_override(None).as_deref(), Some("version=n,b"));
    assert_eq!(with_version_override(Some(" ")).as_deref(), Some("version=n,b"));
    assert_eq!(
        with_version_override(Some("dxgi=n;")).as_deref(),
        Some("dxgi=n;version=n,b")
    );
    for set in [
        "version=b",
        "d3d11,Version=n",
        "VERSION.DLL=n,b",
        "*version=b",
        "dinput8;version",
    ] {
        assert_eq!(with_version_override(Some(set)), None, "{set}");
    }
    assert_eq!(
        with_version_override(Some("versions=n")).as_deref(),
        Some("versions=n;version=n,b")
    );
}

#[test]
fn config_remembers_and_clamps_download_job_limit() {
    let mut config: Config = serde_json::from_str("{}").unwrap();
    assert_eq!(config.download_jobs(), 2);
    assert!(!serde_json::to_string(&config).unwrap().contains("download_jobs"));
    config.set_download_jobs(8);
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<Config>(&json).unwrap().download_jobs(), 8);
    config.set_download_jobs(0);
    assert_eq!(config.download_jobs(), 1);
    config.set_download_jobs(100);
    assert_eq!(config.download_jobs(), 10);
    let edited: Config = serde_json::from_str(r#"{"download_jobs":100}"#).unwrap();
    assert_eq!(edited.download_jobs(), 10);
}

#[test]
fn ui_mode_is_unchosen_until_the_player_picks() {
    let config = Config::default();
    assert_eq!(config.ui_mode, None);
    assert_eq!(config.ui_mode_or_default(), UiMode::Pc);
    assert!(!serde_json::to_string(&config).unwrap().contains("ui_mode"));
    let old: Config = serde_json::from_str(r#"{ "developer": true }"#).unwrap();
    assert_eq!(old.ui_mode, None);
}

#[test]
fn onboarding_is_due_until_the_current_version_is_seen() {
    let mut config = Config::default();
    assert!(config.needs_onboarding());
    assert!(!serde_json::to_string(&config).unwrap().contains("onboarding"));
    config.onboarding = Some(ONBOARDING_VERSION - 1);
    assert!(config.needs_onboarding());
    config.onboarding = Some(ONBOARDING_VERSION);
    assert!(!config.needs_onboarding());
    let back: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
    assert_eq!(back.onboarding, Some(ONBOARDING_VERSION));
    let old: Config = serde_json::from_str(r#"{ "ui_mode": "tv" }"#).unwrap();
    assert!(old.needs_onboarding());
}

#[test]
fn ui_mode_round_trips() {
    for mode in [UiMode::Pc, UiMode::Tv] {
        let config = Config {
            ui_mode: Some(mode),
            ..Config::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.ui_mode, Some(mode));
        assert_eq!(back.ui_mode_or_default(), mode);
    }
    let tv: Config = serde_json::from_str(r#"{ "ui_mode": "tv" }"#).unwrap();
    assert!(tv.ui_mode_or_default().is_tv());
    assert!(!UiMode::Pc.is_tv());
}

#[test]
fn ui_scale_is_auto_until_chosen_and_round_trips() {
    let config = Config::default();
    assert_eq!(config.ui_scale, None);
    assert_eq!(config.ui_scale_percent(), None);
    assert!(!serde_json::to_string(&config).unwrap().contains("ui_scale"));
    for percent in [1, 75, 90, 100, 110, 125, 150, 237, 400] {
        let config = Config {
            ui_scale: Some(percent),
            ..Config::default()
        };
        let back: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(back.ui_scale_percent(), Some(percent));
    }
    let old: Config = serde_json::from_str(r#"{ "ui_mode": "tv" }"#).unwrap();
    assert_eq!(old.ui_scale, None);
}

#[test]
fn ui_scale_is_held_to_one_through_four_hundred() {
    assert_eq!(clamp_ui_scale(0), 1);
    assert_eq!(clamp_ui_scale(1), 1);
    assert_eq!(clamp_ui_scale(237), 237);
    assert_eq!(clamp_ui_scale(400), 400);
    assert_eq!(clamp_ui_scale(60000), 400);
    let low: Config = serde_json::from_str(r#"{ "ui_scale": 0 }"#).unwrap();
    assert_eq!(low.ui_scale_percent(), Some(1));
    let high: Config = serde_json::from_str(r#"{ "ui_scale": 9000 }"#).unwrap();
    assert_eq!(high.ui_scale_percent(), Some(400));
}

#[test]
fn config_remembers_the_language() {
    let mut config = Config::default();
    assert!(!serde_json::to_string(&config).unwrap().contains("language"));
    config.language = Some("ru".into());
    let json = serde_json::to_string(&config).unwrap();
    let loaded = serde_json::from_str::<Config>(&json).unwrap();
    assert_eq!(loaded.language.as_deref(), Some("ru"));
}

#[test]
fn config_keeps_console_flag() {
    let mut config = Config::default();
    assert!(!serde_json::to_string(&config).unwrap().contains("console"));
    config.console = true;
    let json = serde_json::to_string(&config).unwrap();
    assert!(serde_json::from_str::<Config>(&json).unwrap().console);
}

#[test]
fn profiles_are_on_unless_turned_off() {
    assert!(Config::default().use_profiles);
    assert!(serde_json::from_str::<Config>("{}").unwrap().use_profiles);
    let json = serde_json::to_string(&Config {
        use_profiles: false,
        ..Config::default()
    })
    .unwrap();
    assert!(json.contains(r#""use_profiles":false"#));
    assert!(!serde_json::from_str::<Config>(&json).unwrap().use_profiles);
    assert!(
        !serde_json::to_string(&Config::default())
            .unwrap()
            .contains("use_profiles")
    );
}

#[test]
fn off_addons_are_left_out_of_updates_unless_turned_on() {
    assert!(!Config::default().update_off_addons);
    assert!(!serde_json::from_str::<Config>("{}").unwrap().update_off_addons);
    assert!(
        !serde_json::to_string(&Config::default())
            .unwrap()
            .contains("update_off_addons")
    );
    let on = Config {
        update_off_addons: true,
        ..Config::default()
    };
    let json = serde_json::to_string(&on).unwrap();
    assert!(json.contains(r#""update_off_addons":true"#));
    assert!(serde_json::from_str::<Config>(&json).unwrap().update_off_addons);
}

#[test]
fn time_trial_boards_is_on_unless_turned_off() {
    assert!(Config::default().time_trial_boards);
    assert!(serde_json::from_str::<Config>("{}").unwrap().time_trial_boards);
    assert!(
        !serde_json::to_string(&Config::default())
            .unwrap()
            .contains("time_trial")
    );
    let off = Config {
        time_trial_boards: false,
        ..Config::default()
    };
    let json = serde_json::to_string(&off).unwrap();
    assert!(json.contains(r#""time_trial_boards":false"#));
    assert!(!serde_json::from_str::<Config>(&json).unwrap().time_trial_boards);
}

#[test]
fn time_trial_ingame_is_on_unless_turned_off() {
    assert!(Config::default().time_trial_ingame);
    assert!(
        !serde_json::to_string(&Config::default())
            .unwrap()
            .contains("time_trial_ingame")
    );
    let off = Config {
        time_trial_ingame: false,
        ..Config::default()
    };
    let json = serde_json::to_string(&off).unwrap();
    assert!(json.contains(r#""time_trial_ingame":false"#));
    assert!(!serde_json::from_str::<Config>(&json).unwrap().time_trial_ingame);
}

#[test]
fn home_video_is_on_unless_turned_off() {
    assert!(Config::default().home_video);
    assert!(
        !serde_json::to_string(&Config::default())
            .unwrap()
            .contains("home_video")
    );
    let off = Config {
        home_video: false,
        ..Config::default()
    };
    let json = serde_json::to_string(&off).unwrap();
    assert!(json.contains(r#""home_video":false"#));
    assert!(!serde_json::from_str::<Config>(&json).unwrap().home_video);
}

#[test]
fn old_session_logs_are_kept_unless_clearing_is_turned_on() {
    assert!(!Config::default().clear_old_session_logs);
    let old: Config = serde_json::from_str(r#"{ "developer": true }"#).unwrap();
    assert!(!old.clear_old_session_logs);
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("clear_old_session_logs")
    );
    let on = Config {
        clear_old_session_logs: true,
        ..Config::default()
    };
    let json = serde_json::to_string(&on).unwrap();
    assert!(json.contains(r#""clear_old_session_logs":true"#), "{json}");
    assert!(
        serde_json::from_str::<Config>(&json)
            .unwrap()
            .clear_old_session_logs
    );
}

#[test]
fn starting_the_store_is_on_unless_turned_off() {
    assert!(Config::default().launch.start_store);
    let old: Config = serde_json::from_str(r#"{ "launch": { "winedebug": "-all" } }"#).unwrap();
    assert!(old.launch.start_store);
    assert!(!serde_json::to_string(&old).unwrap().contains("start_"));
    let off: Config = serde_json::from_str(r#"{ "launch": { "start_steam": false } }"#).unwrap();
    assert!(!off.launch.start_store);
    let json = serde_json::to_string(&off).unwrap();
    assert!(json.contains(r#""start_store":false"#), "{json}");
    assert!(!serde_json::from_str::<Config>(&json).unwrap().launch.start_store);
}

#[test]
fn a_located_folder_becomes_the_active_added_copy() {
    let mut old: Config = serde_json::from_str(r#"{ "game_dir": "/games/F1 25" }"#).unwrap();
    old.migrate_games();
    assert_eq!(old.games.active.as_deref(), Some(Path::new("/games/F1 25")));
    assert_eq!(old.games.added, [PathBuf::from("/games/F1 25")]);
    let json = serde_json::to_string(&old).unwrap();
    assert!(!json.contains("game_dir"), "{json}");
    let mut back: Config = serde_json::from_str(&json).unwrap();
    back.migrate_games();
    assert_eq!(back, old);
    let mut both: Config =
        serde_json::from_str(r#"{ "game_dir": "/b", "games": { "active": "/a", "added": ["/a"] } }"#)
            .unwrap();
    both.migrate_games();
    assert_eq!(both.games.active.as_deref(), Some(Path::new("/a")));
    assert_eq!(both.games.added, [PathBuf::from("/a"), PathBuf::from("/b")]);
    assert!(
        !serde_json::to_string(&Config::default())
            .unwrap()
            .contains("games")
    );
}

#[test]
fn config_remembers_vr_per_profile() {
    let mut config = Config::default();
    assert_eq!(config.launch_mode("Default"), LaunchMode::Normal);
    assert!(!serde_json::to_string(&config).unwrap().contains("vr_profiles"));
    config.set_launch_mode("Default", LaunchMode::Vr);
    let back: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
    assert_eq!(back.launch_mode("Default"), LaunchMode::Vr);
    assert_eq!(back.launch_mode("Other"), LaunchMode::Normal);
    config.set_launch_mode("Default", LaunchMode::Normal);
    assert!(config.vr_profiles.is_empty());
}

#[test]
fn config_remembers_closed_news() {
    let mut config = Config::default();
    assert!(!serde_json::to_string(&config).unwrap().contains("news_seen"));
    config.news_seen.insert("discord".into());
    let back: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
    assert!(back.news_seen.contains("discord"));
}

#[test]
fn config_remembers_dismissed_cards() {
    let mut config = Config::default();
    assert!(
        !serde_json::to_string(&config)
            .unwrap()
            .contains("dismissed_cards")
    );
    config.dismissed_cards.insert("discord-banner".into());
    let back: Config = serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
    assert!(back.dismissed_cards.contains("discord-banner"));
}

fn strings(args: &[&str]) -> Vec<String> {
    args.iter().map(|a| (*a).to_owned()).collect()
}

#[test]
fn vr_adds_steams_vr_argument_once() {
    assert_eq!(game_args(&[], LaunchMode::Normal), Vec::<String>::new());
    assert_eq!(game_args(&[], LaunchMode::Vr), ["-vr"]);
    assert_eq!(game_args(&strings(&["-vr"]), LaunchMode::Vr), ["-vr"]);
    assert_eq!(game_args(&strings(&["-VR"]), LaunchMode::Vr), ["-VR"]);
    assert_eq!(game_args(&strings(&["-vr"]), LaunchMode::Normal), ["-vr"]);
    assert_eq!(
        game_args(&strings(&["-windowed"]), LaunchMode::Vr),
        ["-windowed", "-vr"]
    );
    assert_eq!(LaunchMode::of(&strings(&["-windowed", "-vr"])), LaunchMode::Vr);
    assert_eq!(LaunchMode::of(&strings(&["-vrmode"])), LaunchMode::Normal);
    assert_eq!(LaunchMode::Vr.args(), ["-vr"]);
    assert!(LaunchMode::Normal.args().is_empty());
}

#[test]
fn steam_vr_launch_keeps_its_argument() {
    let vr = parse_steam_command(&strings(&[
        "/steam/ubuntu12_32/reaper",
        "SteamLaunch",
        "AppId=3059520",
        "--",
        "/steam/compatibilitytools.d/Proton/proton",
        "waitforexitandrun",
        "/games/F1 25/EAAntiCheat.GameServiceLauncher.exe",
        "-vr",
    ]));
    assert_eq!(vr.game_args(), ["-vr"]);
    assert_eq!(LaunchMode::of(vr.game_args()), LaunchMode::Vr);
    let direct = parse_steam_command(&strings(&[
        "/proton/proton",
        "waitforexitandrun",
        "/games/F1 25/F1_25.exe",
    ]));
    assert!(direct.game_args().is_empty());
    let windows = parse_steam_command(&strings(&["C:\\F1 25\\F1_25.exe", "-vr"]));
    assert_eq!(windows.game_args(), ["-vr"]);
    assert!(parse_steam_command(&[]).game_args().is_empty());
}

#[test]
fn game_env_drops_build_shell_junk() {
    let parent = [
        ("PATH".to_string(), "/usr/bin".to_string()),
        ("HOME".to_string(), "/home/u".to_string()),
        ("SteamAppId".to_string(), "3059520".to_string()),
        ("NIX_LDFLAGS".to_string(), "-L/nix/store/aaaa ".repeat(200)),
        ("CFLAGS".to_string(), "-O2".to_string()),
        ("buildInputs".to_string(), "x".to_string()),
        ("depsBuildBuild".to_string(), String::new()),
    ];
    let env = game_env(parent, &BTreeMap::new());
    assert!(env.contains_key("PATH") && env.contains_key("SteamAppId"));
    for dropped in ["NIX_LDFLAGS", "CFLAGS", "buildInputs", "depsBuildBuild"] {
        assert!(!env.contains_key(dropped), "{dropped} kept");
    }
}

#[test]
fn game_env_drops_an_inherited_compat_layer() {
    let parent = || {
        [
            ("Path".to_string(), r"C:\WINDOWS\system32".to_string()),
            ("__COMPAT_LAYER".to_string(), "DetectorsAppHealth".to_string()),
            ("__compat_layer".to_string(), "RunAsInvoker".to_string()),
        ]
    };
    let env = game_env(parent(), &BTreeMap::new());
    assert_eq!(env.keys().collect::<Vec<_>>(), ["Path"]);
    let mut overrides = BTreeMap::new();
    overrides.insert(COMPAT_LAYER.to_string(), "HighDpiAware".to_string());
    let env = game_env(parent(), &overrides);
    assert_eq!(env.get(COMPAT_LAYER).map(String::as_str), Some("HighDpiAware"));
    assert_eq!(env.len(), 2);
}

#[test]
fn game_env_fits_the_windows_block() {
    let parent = (0..400).map(|i| (format!("VAR{i:03}"), "x".repeat(200)));
    let mut overrides = BTreeMap::new();
    overrides.insert("WINEDEBUG".to_string(), "+all".to_string());
    let env = game_env(parent, &overrides);
    let size: usize = env.iter().map(|(k, v)| k.len() + v.len() + 2).sum();
    assert!(size <= 30_000, "{size}");
    assert_eq!(env.get("WINEDEBUG").map(String::as_str), Some("+all"));
}

#[cfg(unix)]
#[test]
fn sdl_comes_from_steam_only_when_the_host_has_none() {
    let root = std::env::temp_dir().join(format!("swofty-sdl-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let steam = root.join("steam");
    let runtime = steam.join("ubuntu12_32/steam-runtime/usr/lib/x86_64-linux-gnu");
    let host = root.join("host");
    let host32 = root.join("host32");
    let links = root.join("data").join(RUNTIME_LIBS_DIR);
    for d in [&runtime, &host, &host32] {
        std::fs::create_dir_all(d).unwrap();
    }
    let elf64 = b"\x7fELF\x02\x01\x01\0".as_slice();
    std::fs::write(runtime.join(SDL2_SONAME), elf64).unwrap();
    std::fs::write(host32.join(SDL2_SONAME), b"\x7fELF\x01\x01\x01\0").unwrap();
    let host32 = host32.to_str().unwrap();
    let links_s = links.display().to_string();

    let path = sdl_library_path_in(None, &[host32], &steam, &links);
    assert_eq!(path.as_deref(), Some(links_s.as_str()));
    assert_eq!(
        std::fs::read_link(links.join(SDL2_SONAME)).unwrap(),
        runtime.join(SDL2_SONAME)
    );
    let path = sdl_library_path_in(Some("/proton/lib"), &[host32], &steam, &links);
    assert_eq!(path, Some(format!("/proton/lib:{links_s}")));

    std::fs::write(host.join(SDL2_SONAME), elf64).unwrap();
    assert_eq!(
        sdl_library_path_in(None, &[host.to_str().unwrap()], &steam, &links),
        None
    );
    assert_eq!(
        sdl_library_path_in(Some(host.to_str().unwrap()), &[], &steam, &links),
        None
    );
    assert_eq!(
        sdl_library_path_in(None, &[], &root.join("no-steam"), &links),
        None
    );
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn sdl_comes_from_the_64_bit_clients_sdl3_when_the_old_runtime_is_gone() {
    let root = std::env::temp_dir().join(format!("swofty-sdl3-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let steam = root.join("steam");
    let links = root.join("data").join(RUNTIME_LIBS_DIR);
    let elf64 = b"\x7fELF\x02\x01\x01\0".as_slice();
    let platform = |version: &str| {
        steam
            .join(STEAM_CLIENT_PLATFORMS)
            .join(format!("steamrt3c_platform_3c.0.{version}"))
            .join(PLATFORM_LIB_DIR)
    };
    for (version, file) in [
        ("20260119.1", "libSDL2-2.0.so.0.3200.10"),
        ("20261103.2", "libSDL2-2.0.so.0.3200.74"),
    ] {
        std::fs::create_dir_all(platform(version)).unwrap();
        std::fs::write(platform(version).join(file), elf64).unwrap();
    }
    std::fs::create_dir_all(
        steam
            .join(STEAM_CLIENT_PLATFORMS)
            .join("steamrt3c_platform_3c.0.20991231.9"),
    )
    .unwrap();

    assert_eq!(sdl_library_path_in(None, &[], &steam, &links), None);

    std::fs::write(steam.join(STEAM_CLIENT_SDL3), elf64).unwrap();
    let path = sdl_library_path_in(Some("/proton/lib"), &[], &steam, &links);
    assert_eq!(path, Some(format!("/proton/lib:{}", links.display())));
    assert_eq!(
        std::fs::read_link(links.join(SDL2_SONAME)).unwrap(),
        platform("20261103.2").join("libSDL2-2.0.so.0.3200.74")
    );
    assert_eq!(
        std::fs::read_link(links.join(SDL3_SONAME)).unwrap(),
        steam.join(STEAM_CLIENT_SDL3)
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn cpu_lists() {
    assert_eq!(parse_cpu_list("0-2,5"), Some(vec![0, 1, 2, 5]));
    assert_eq!(parse_cpu_list("x"), None);
}

#[test]
fn reads_session_records() {
    let dir = std::env::temp_dir().join(format!("swofty-session-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("request_id"), "abc\n").unwrap();
    std::fs::write(dir.join("game_start"), "100.5\n").unwrap();
    let live = SessionRecord::read(&dir);
    assert_eq!(live.request_id.as_deref(), Some("abc"));
    assert_eq!((live.started, live.exit_code), (Some(100.5), None));
    assert!(!live.crashed());
    std::fs::write(dir.join("exit_code"), "3\n").unwrap();
    assert!(
        !SessionRecord::read(&dir).crashed(),
        "a code that is not a crash code"
    );
    std::fs::write(dir.join("exit_code"), "-1073741819\n").unwrap();
    assert!(SessionRecord::read(&dir).crashed(), "an access violation");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn tells_a_rotated_loader_log_from_a_stale_one() {
    let dir = std::env::temp_dir().join(format!("swofty-loader-mark-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("loader.log");
    assert!(!LogMark::replaced(&log, None));
    std::fs::write(
        &log,
        "2026-10-17 21:44:08.228 [  416] swofty loader (version.dll proxy) attached, pid 412\nold\n",
    )
    .unwrap();
    assert!(LogMark::replaced(&log, None));
    let before = LogMark::read(&log);
    let session = SessionRecord::read(&dir);
    assert!(!session.owns_loader_log(&log));
    std::fs::write(dir.join(LOADER_MARK), serde_json::to_vec(&before).unwrap()).unwrap();
    assert!(!LogMark::replaced(&log, before.as_ref()));
    assert!(!session.owns_loader_log(&log));
    std::fs::rename(&log, dir.join("loader.prev.log")).unwrap();
    std::fs::write(&log, "").unwrap();
    assert!(session.owns_loader_log(&log));
    std::fs::write(
        &log,
        "2026-10-17 22:12:08.004 [  420] swofty loader (version.dll proxy) attached, pid 416\n",
    )
    .unwrap();
    assert!(session.owns_loader_log(&log));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn steam_command_shape() {
    let argv: Vec<String> = [
        "/steam/ubuntu12_32/reaper",
        "SteamLaunch",
        "AppId=3059520",
        "--",
        "/steam/SteamLinuxRuntime_sniper/_v2-entry-point",
        "--verb=waitforexitandrun",
        "--",
        "/steam/compatibilitytools.d/Proton/proton",
        "waitforexitandrun",
        "/games/F1 25/EAAntiCheat.GameServiceLauncher.exe",
    ]
    .map(String::from)
    .to_vec();
    let c = parse_steam_command(&argv);
    assert_eq!(c.verb.as_deref(), Some("waitforexitandrun"));
    assert_eq!(c.target, Some(9));
    assert!(c.proton.unwrap().ends_with("/proton"));
}

#[test]
fn old_update_settings_still_load() {
    let json = serde_json::to_value(UpdateConfig::default()).unwrap();
    assert_eq!(json, serde_json::json!({ "check": true }));
    let off: UpdateConfig = serde_json::from_str(r#"{"check":false}"#).unwrap();
    assert!(!off.check);
    let old = r#"{
            "data_version": 3,
            "updates": { "check": true, "skip": "1.0.7", "checked_at": 1790000000, "latest": "v1.0.7" },
            "managed": { "checked_at": 1790000000 },
            "sidebar_list": true
        }"#;
    let config: Config = serde_json::from_str(old).unwrap();
    assert!(config.updates.is_default() && config.sidebar_list);
    assert_eq!(config.data_version, 3);
    let saved = serde_json::to_value(&config).unwrap();
    assert!(
        saved.get("updates").is_none() && saved.get("managed").is_none(),
        "{saved}"
    );
}
