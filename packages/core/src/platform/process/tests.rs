use super::image::*;
use super::list::*;
use super::times::*;
use super::*;

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn a_flag_keeps_a_value_that_starts_with_a_dash() {
        assert_eq!(flag("profile", "-swofty"), "--profile=-swofty");
        assert_eq!(flag("game", Path::new("/games/F1 25")), "--game=/games/F1 25");
    }

    #[test]
    fn reports_the_live_session_of_a_running_game() {
        let root = std::env::temp_dir().join(format!("swofty-status-{}", std::process::id()));
        let dir = root.join("sessions/2026-10-20T14-22-33");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(root.join("sessions/latest.txt"), format!("{}\n", dir.display())).unwrap();
        std::fs::write(dir.join("game_start"), "100.5\n").unwrap();
        let game = [GameProcess {
            pid: 48211,
            started: Some(UNIX_EPOCH + std::time::Duration::from_secs(100)),
            origin: Origin::default(),
        }];
        let status = status_of(&game, Some(&root));
        assert_eq!(
            status,
            GameStatus {
                running: true,
                pid: Some(48211),
                since: Some(100.0),
                session: Some("2026-10-20T14-22-33".into()),
            }
        );
        std::fs::write(dir.join("game_end"), "200\n").unwrap();
        assert_eq!(status_of(&game, Some(&root)).session, None);
        assert_eq!(status_of(&[], Some(&root)), GameStatus::default());
        assert_eq!(status_of(&game, None).session, None);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn picks_only_this_copys_games_the_session_neither_saw_nor_started() {
        let ea = Path::new(r"D:\EA Games\F1 25");
        let mut running = [11076, 15472, 9960, 4242, 5150].map(|pid| GameProcess {
            pid,
            started: None,
            origin: Origin {
                image: Some(r"D:\EA Games\F1 25\F1_25.exe".into()),
                ..Origin::default()
            },
        });
        running[1].origin = Origin::default();
        running[4].origin.image = Some(r"C:\Steam\steamapps\common\F1 25\F1_25.exe".into());
        let ours = |pid| pid == 9960;
        assert_eq!(started_elsewhere(&running, ea, &[4242], ours), vec![11076, 15472]);
        assert_eq!(
            started_elsewhere(&running[2..], ea, &[4242], ours),
            Vec::<u32>::new()
        );
        assert_eq!(started_elsewhere(&[], ea, &[], ours), Vec::<u32>::new());
    }
}

#[cfg(test)]
mod origin_tests {
    use super::*;

    fn seen(image: Option<&str>, command: Option<&str>, prefix: Option<&Path>) -> GameProcess {
        GameProcess {
            pid: 1,
            started: None,
            origin: Origin {
                image: image.map(str::to_owned),
                command: command.map(str::to_owned),
                prefix: prefix.map(Path::to_path_buf),
            },
        }
    }

    #[test]
    fn windows_image_paths_name_their_copy_whatever_the_case_and_slashes() {
        let steam = Path::new(r"c:\program files (x86)\steam\steamapps\common\F1 25");
        let ea = Path::new(r"D:\EA Games\F1 25\");
        let g = seen(
            Some(r"C:\Program Files (x86)\Steam\steamapps\common\F1 25\F1_25.exe"),
            None,
            None,
        );
        assert!(g.is_in(steam) && !g.elsewhere(steam));
        assert!(!g.is_in(ea) && g.elsewhere(ea));
        let g = seen(Some(r"\\?\D:/EA Games/F1 25/F1_25.EXE"), None, None);
        assert!(g.is_in(ea) && g.elsewhere(steam));
        let other = seen(Some(r"C:\Windows\explorer.exe"), None, None);
        assert!(!other.is_in(steam) && !other.elsewhere(steam));
    }

    #[test]
    fn a_game_that_tells_nothing_is_no_copys() {
        let g = seen(None, None, None);
        let dir = Path::new("/games/F1 25");
        assert!(!g.is_in(dir) && !g.elsewhere(dir));
        let g = seen(None, Some("F1_25.exe"), None);
        assert!(!g.is_in(dir) && !g.elsewhere(dir));
        let g = seen(None, Some(r"D:\F1 25\F1_25.exe"), None);
        assert!(
            !g.is_in(dir) && !g.elsewhere(dir),
            "a drive without a prefix maps nowhere"
        );
    }

    #[test]
    fn reads_the_exe_from_a_maps_line_and_the_prefix_from_the_environment() {
        let line = "140000000-140001000 r--p 00000000 103:02 21500751                        \
                    /mnt/gaming/EAApp/Games/F1 25/F1_25.exe";
        assert_eq!(
            mapped_exe(line).as_deref(),
            Some("/mnt/gaming/EAApp/Games/F1 25/F1_25.exe")
        );
        let lib = "7f0000000000-7f0000001000 r--p 00000000 103:02 1 /usr/lib/libc.so.6";
        assert_eq!(mapped_exe(lib), None);
        assert_eq!(
            mapped_exe("7ffe0000-7fff0000 rw-p 00000000 00:00 0 [stack]"),
            None
        );
        let env = b"HOME=/home/a\0WINEPREFIX=/mnt/gaming/EAApp/pfx/pfx/\0STEAM_COMPAT_DATA_PATH=/x\0";
        assert_eq!(
            wine_prefix(env),
            Some(PathBuf::from("/mnt/gaming/EAApp/pfx/pfx/"))
        );
        let env = b"STEAM_COMPAT_DATA_PATH=/steam/compatdata/3059520\0";
        assert_eq!(
            wine_prefix(env),
            Some(PathBuf::from("/steam/compatdata/3059520/pfx"))
        );
        assert_eq!(wine_prefix(b"WINEPREFIXES=/a\0"), None);
    }

    #[cfg(unix)]
    struct Fixture(PathBuf);

    #[cfg(unix)]
    impl Fixture {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("swofty-origin-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn game(&self, rel: &str) -> PathBuf {
            let d = self.0.join(rel);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join(GAME_EXE), b"MZ").unwrap();
            d
        }

        fn windows(&self, rel: &str) -> String {
            format!(r"Z:{}\{rel}", self.0.display()).replace('/', r"\")
        }
    }

    #[cfg(unix)]
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    #[cfg(unix)]
    #[test]
    fn proton_z_paths_name_their_copy() {
        let f = Fixture::new("proton");
        let steam = f.game("steamapps/common/F1 25");
        let ea = f.game("EAApp/Games/F1 25");
        let g = seen(None, Some(&f.windows(r"steamapps\common\F1 25\F1_25.exe")), None);
        assert_eq!(g.origin.folders(), std::slice::from_ref(&steam));
        assert!(g.is_in(&steam) && g.elsewhere(&ea));
        let mapped = format!("{}/F1_25.exe", ea.display());
        let g = seen(Some(&mapped), Some(r"D:\F1 25\F1_25.exe"), None);
        assert!(g.is_in(&ea) && g.elsewhere(&steam));
    }

    #[cfg(unix)]
    #[test]
    fn ea_drive_letters_map_through_the_prefix() {
        let f = Fixture::new("drives");
        let steam = f.game("steamapps/common/F1 25");
        let ea = f.game("EAApp/Games/F1 25");
        let c = f.game("EAApp/pfx/pfx/drive_c/Games/F1 25");
        let prefix = f.0.join("EAApp/pfx/pfx");
        std::fs::create_dir_all(prefix.join("dosdevices")).unwrap();
        std::os::unix::fs::symlink(f.0.join("EAApp/Games"), prefix.join("dosdevices/d:")).unwrap();
        std::os::unix::fs::symlink("../drive_c", prefix.join("dosdevices/c:")).unwrap();
        std::os::unix::fs::symlink("/", prefix.join("dosdevices/z:")).unwrap();
        let g = seen(None, Some(r"D:\F1 25\F1_25.exe"), Some(&prefix));
        assert!(g.is_in(&ea) && g.elsewhere(&steam) && !g.is_in(&steam));
        let g = seen(None, Some(r"c:\Games\F1 25\F1_25.exe"), Some(&prefix));
        assert!(g.is_in(&c) && g.elsewhere(&ea));
        let g = seen(
            None,
            Some(&f.windows(r"steamapps\common\F1 25\F1_25.exe")),
            Some(&prefix),
        );
        assert!(g.is_in(&steam) && g.elsewhere(&ea));
        let g = seen(None, Some(r"E:\F1 25\F1_25.exe"), Some(&prefix));
        assert!(
            !g.is_in(&ea) && !g.elsewhere(&ea),
            "an unmapped drive tells nothing"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_game_link_folder_is_the_copy_it_points_at() {
        let f = Fixture::new("link");
        let steam = f.game("steamapps/common/F1 25");
        let ea = f.game("EAApp/Games/F1 25");
        std::fs::create_dir_all(f.0.join("data/state")).unwrap();
        std::os::unix::fs::symlink(&steam, f.0.join("data/state/game-link")).unwrap();
        let g = seen(None, Some(&f.windows(r"data\state\game-link\F1_25.exe")), None);
        assert!(g.is_in(&steam) && g.elsewhere(&ea));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    #[test]
    fn finds_this_test_by_its_exe_name_in_any_case() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_str().unwrap().to_owned();
        let me = std::process::id();
        let pids = super::win32::process_ids(&name).expect("the process list");
        assert!(pids.contains(&me), "{name}: {pids:?} lacks {me}");
        let shouted = super::named(&name.to_uppercase());
        assert!(shouted.iter().any(|g| g.pid == me), "{shouted:?}");
        assert!(super::named("no-such-process-swofty.exe").is_empty());
    }

    #[test]
    fn reads_own_start_time() {
        let age = super::started(std::process::id())
            .unwrap()
            .elapsed()
            .unwrap_or_default();
        assert!(age.as_secs() < 3600, "{age:?}");
    }
}

#[cfg(test)]
mod filetime_tests {
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn counts_filetime_from_1601() {
        assert_eq!(super::filetime(116_444_736_000_000_000), Some(UNIX_EPOCH));
        assert_eq!(
            super::filetime(134_062_272_000_000_000),
            Some(UNIX_EPOCH + Duration::from_secs(1_761_753_600))
        );
        assert_eq!(super::filetime(0), None);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod ancestry_tests {
    #[test]
    fn traces_ancestry() {
        let mut child = std::process::Command::new("sleep").arg("5").spawn().unwrap();
        assert!(super::descends_from(child.id(), std::process::id()));
        assert!(!super::descends_from(std::process::id(), child.id()));
        child.kill().ok();
        child.wait().ok();
    }

    #[test]
    fn reads_own_start_time() {
        let age = super::started(std::process::id())
            .unwrap()
            .elapsed()
            .unwrap_or_default();
        assert!(age.as_secs() < 3600, "{age:?}");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod zombie_tests {
    use super::*;

    #[test]
    fn a_zombie_or_dead_process_is_not_a_running_game() {
        assert!(state_is_dead("4242 (F1_25.exe) Z 1 4242 4242 0 -1 4228620 0"));
        assert!(state_is_dead("4242 (F1_25.exe) X 1 4242"));
        assert!(!state_is_dead("4242 (F1_25.exe) S 1 4242 4242 0 -1"));
        assert!(!state_is_dead("4242 (F1 (25).exe) R 1 4242"));
        assert!(state_is_dead("4242 (F1 (25).exe) Z 1 4242"));
    }

    #[test]
    fn this_test_process_is_alive() {
        assert!(!is_dead(Path::new("/proc/self")));
    }
}
