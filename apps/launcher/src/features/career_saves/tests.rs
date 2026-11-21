use std::path::PathBuf;

use swofty_core::saves::vault::careers::{Brief, Row, Status};
use swofty_core::saves::vault::{self, Vault};

use swofty_core::saves::launcher::{self as launcher_saves, Marker};

use super::{
    Banner, Missing, SavesPage, awaiting_import, banner_from, build_saves, list, reason_label, steam_save_in,
    steam_save_missing, whose,
};

#[test]
fn a_saves_view_builds_without_the_shell_so_it_can_run_off_the_ui_thread() {
    fn is_send<T: Send>() {}
    is_send::<super::Built>();
    let root = std::env::temp_dir().join(format!("swofty-build-saves-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let store = swofty_core::store::Store::new(&root);
    let built = build_saves(&store, None, "Race", false);
    assert_eq!(built.view.profile, "Race");
    std::fs::remove_dir_all(&root).ok();
}

fn synthetic_vault(root: &std::path::Path, profile: &str, snapshots: usize) {
    let index = swofty_core::stdlib::careers::data_path(root);
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    let loc = vault::Location::launcher(root, profile);
    std::fs::create_dir_all(&loc.dir).unwrap();
    let v = Vault::of(root, &loc).unwrap();
    for n in 0..snapshots {
        let careers: Vec<String> = (0..400 + n)
            .map(|c| {
                format!(
                    r#"{{"career":"driver_career:s{c}","profile":"{profile}","slot":"s{c}","status":"present","mode":"driver_career","driver":"Driver {c}","team":"Team {c}","driver_raw":{c},"team_raw":{c},"slot_season":3,"slot_weekend":4,"last_played":1761600000,"slot_name":"Slot {c}",{}}}"#,
                    (0..40).map(|f| format!(r#""field_{f}":"value {f} of career {c}""#)).collect::<Vec<_>>().join(",")
                )
            })
            .collect();
        std::fs::write(
            &index,
            format!(r#"{{"version":1,"careers":[{}]}}"#, careers.join(",")),
        )
        .unwrap();
        std::fs::write(
            loc.dir.join("savegame@profile#NXDSMWW.BWW"),
            vec![n as u8; 4096 + n],
        )
        .unwrap();
        v.snapshot(
            &loc,
            vault::Meta::new(vault::Reason::AfterSession),
            1_793_404_800 + n as i64 * 600,
        )
        .unwrap();
    }
}

#[test]
fn a_hidden_or_unchanged_saves_page_is_not_refreshed() {
    let mut page = SavesPage::default();
    assert!(page.refresh_due(true, Some(1)));
    assert!(!page.refresh_due(false, Some(1)));
    assert!(!page.refresh_due(true, None));
    page.print = Some(1);
    assert!(!page.refresh_due(true, Some(1)));
    assert!(page.refresh_due(true, Some(2)));
    page.loading = true;
    assert!(!page.refresh_due(true, Some(2)));
}

#[test]
fn the_print_follows_the_vault_and_the_save_folder_and_nothing_else() {
    let root = std::env::temp_dir().join(format!("swofty-print-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    synthetic_vault(&root, "Race", 2);
    let print = |game| super::saves_print(&root, "Race", game);
    let first = print(false);
    assert_eq!(first, print(false));
    assert_ne!(first, print(true));
    assert_eq!(print(false), super::saves_print(&root, "Race", false));
    let loc = vault::Location::launcher(&root, "Race");
    std::fs::write(loc.dir.join("savegame@profile#NXDSMWW.BWW"), vec![9u8; 7000]).unwrap();
    let changed = print(false);
    assert_ne!(first, changed);
    let v = Vault::of(&root, &loc).unwrap();
    v.snapshot(&loc, vault::Meta::new(vault::Reason::Manual), 1_793_500_000)
        .unwrap();
    assert_ne!(changed, print(false));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn snapshot_details_are_cached_until_the_index_or_the_current_careers_change() {
    let root = std::env::temp_dir().join(format!("swofty-details-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    synthetic_vault(&root, "Race", 3);
    let loc = vault::Location::launcher(&root, "Race");
    let v = Vault::of(&root, &loc).unwrap();
    let snaps = v.list().unwrap();
    let now = swofty_core::stdlib::careers::data_path(&root);
    let cache = std::sync::Mutex::new(super::DetailCache::default());
    let first = super::details(&v, &snaps, &now, &cache);
    assert_eq!(first.len(), 3);
    assert_eq!(cache.lock().unwrap().0.len(), 3);

    let index = v.dir.join(snaps[0].careers_ref.as_deref().unwrap());
    let held = first[0].held.clone();
    std::fs::write(
        &index,
        r#"{"careers":[{"career":"x:1","slot":"1","status":"present"}]}"#,
    )
    .unwrap();
    let again = super::details(&v, &snaps, &now, &cache);
    assert_ne!(again[0].held, held, "a changed index file is read again");
    assert_eq!(again[1].held, first[1].held, "the others stay cached");

    let before = again[2].diff.clone();
    std::fs::write(
        &now,
        r#"{"careers":[{"career":"y:2","slot":"2","status":"present"}]}"#,
    )
    .unwrap();
    let after = super::details(&v, &snaps, &now, &cache);
    assert_ne!(
        after[2].diff, before,
        "a changed current index recomputes every diff"
    );

    let fewer = super::details(&v, &snaps[..1], &now, &cache);
    assert_eq!(fewer.len(), 1);
    assert_eq!(
        cache.lock().unwrap().0.len(),
        1,
        "snapshots that are gone leave the cache"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
#[ignore]
fn bench_saves_page_with_25_snapshots() {
    let root = std::path::PathBuf::from(
        std::env::var("SAVES_BENCH_DIR").unwrap_or_else(|_| "/mnt/work/.saves-perf-bench".into()),
    );
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    synthetic_vault(&root, "Race", 25);
    let store = swofty_core::store::Store::new(&root);
    for round in 0..3 {
        let started = std::time::Instant::now();
        let built = build_saves(&store, None, "Race", false);
        assert_eq!(built.view.versions.len(), 25);
        eprintln!("bench round {round}: {} ms", started.elapsed().as_millis());
    }
}

#[test]
fn an_empty_launcher_save_with_a_steam_save_waits_for_an_import() {
    assert!(awaiting_import(false, false, true));
    assert!(!awaiting_import(true, false, true));
    assert!(!awaiting_import(false, true, true));
    assert!(!awaiting_import(false, false, false));
}

#[test]
fn a_missed_steam_save_is_told_apart_from_an_empty_one() {
    assert!(steam_save_missing(false, false, false, true));
    assert!(!steam_save_missing(false, false, false, false));
    assert!(!steam_save_missing(false, false, true, true));
    assert!(!steam_save_missing(false, true, false, true));
    assert!(!steam_save_missing(true, false, false, true));
}

#[test]
fn a_picked_folder_may_be_remote_the_app_folder_or_the_steam_root() {
    let base = std::env::temp_dir().join(format!("swofty-pick-{}", std::process::id()));
    let remote = base.join("Steam Ünï/userdata/843502855/3059520/remote");
    std::fs::create_dir_all(&remote).unwrap();
    assert!(steam_save_in(&remote).is_none());
    std::fs::write(remote.join("savegame@profile#X"), b"x").unwrap();
    for pick in [
        remote.clone(),
        remote.parent().unwrap().to_path_buf(),
        base.join("Steam Ünï/userdata"),
        base.join("Steam Ünï"),
    ] {
        assert_eq!(
            steam_save_in(&pick).map(|l| l.dir),
            Some(remote.clone()),
            "{pick:?}"
        );
    }
    assert!(steam_save_in(&base).is_none());
    std::fs::remove_dir_all(&base).ok();
}

fn row(id: &str, status: Status, at: i64) -> Row {
    Row {
        career: id.into(),
        mode: None,
        label: format!("{id} label"),
        driver: None,
        team: None,
        season: Some(2016),
        round: Some(7),
        profile: None,
        addons: Vec::new(),
        first_seen: "a".into(),
        first_seen_at: 1,
        last_seen: format!("snap-{at}"),
        last_seen_at: at,
        last_played: None,
        status,
        found_at: None,
        dismissed: false,
    }
}

fn data(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-launcher-saves-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(d.join("career-vault/steam-1")).unwrap();
    d
}

type Slots<'a> = &'a [(&'a str, &'a str)];

fn write_generation(
    root: &std::path::Path,
    n: u64,
    event: &str,
    session: Option<&str>,
    containers: &[(&str, &str, Slots)],
) {
    let fnv = |name: &str| {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in name.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
        format!("{h:016x}")
    };
    let gd = root.join("generations").join(format!("{n:012}"));
    std::fs::create_dir_all(gd.join("containers")).unwrap();
    let list: Vec<serde_json::Value> = containers
        .iter()
        .map(|(name, source, slots)| {
            let bytes = format!("{name}-{n}");
            std::fs::write(gd.join("containers").join(format!("{}.bag", fnv(name))), &bytes).unwrap();
            serde_json::json!({
                "name": name, "fnv64": fnv(name), "type_id": 1, "bytes": bytes.len(), "sha256": format!("{name}:{}", slots.iter().map(|s| s.1).collect::<Vec<_>>().join(",")),
                "registered": *source == "game", "source": source,
                "slots": slots.iter().enumerate().map(|(i, (guid, who))| serde_json::json!({
                    "slot": i, "guid": guid, "name": who, "season": 2016, "round": 7, "last_played": 1_000 + i
                })).collect::<Vec<_>>()
            })
        })
        .collect();
    let manifest = serde_json::json!({
        "schema": 1, "generation": n, "event": event, "written_at": 1_000 + n, "session": session,
        "containers": list
    });
    std::fs::write(gd.join("manifest.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("CURRENT"), n.to_string()).unwrap();
}

fn steam_source(d: &std::path::Path) -> vault::Location {
    let remote = d
        .join("steam")
        .join("userdata")
        .join("843502855")
        .join("3059520")
        .join("remote");
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join("savegame@profile#NXDSMWW.BWW"), vec![1u8; 4000]).unwrap();
    vault::Location::steam(&remote).unwrap()
}

#[test]
fn the_banner_names_a_career_the_newest_generation_lacks_and_offers_its_container() {
    let d = std::env::temp_dir().join(format!("swofty-launcher-saves-lost-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    let loc = steam_source(&d);
    let v = Vault::of(&d, &loc).unwrap();
    v.snapshot(&loc, vault::Meta::new(vault::Reason::AfterSession), 1_000)
        .unwrap();
    let plain = d.join("plain").join(&loc.source);
    write_generation(
        &plain,
        1,
        "save",
        Some("s1"),
        &[
            ("driver_career", "game", &[("0x01", "Stock")]),
            (
                "driver_career_progressive",
                "kept",
                &[("0x0a", "Progressive 2016")],
            ),
        ],
    );
    write_generation(
        &plain,
        2,
        "load",
        None,
        &[("driver_career", "game", &[("0x01", "Stock")])],
    );
    match banner_from(&d, loc.source.clone(), "p".into()) {
        Some(Banner::Lost(Missing {
            names,
            source,
            generation,
            modes,
            ..
        })) => {
            assert_eq!(names, ["Progressive 2016, season 2016, round 7"]);
            assert_eq!(generation, 1);
            assert_eq!(modes, ["driver_career_progressive"]);
            assert_eq!(source, loc.source);
        }
        _ => panic!("expected a lost banner"),
    }
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_ea_copy_load_beside_a_steam_save_raises_no_banner_for_the_games_records_or_addon_off_careers() {
    let d = std::env::temp_dir().join(format!("swofty-launcher-saves-ea-load-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    let loc = vault::Location::launcher_ea(&d, "default");
    std::fs::create_dir_all(&loc.dir).unwrap();
    std::fs::write(loc.dir.join("NXDSMWW.BWW"), vec![1u8; 4000]).unwrap();
    std::fs::write(loc.dir.join("QKRHMYXE"), vec![2u8; 416]).unwrap();
    let v = Vault::of(&d, &loc).unwrap();
    v.snapshot(&loc, vault::Meta::new(vault::Reason::Imported), 1_000)
        .unwrap();
    let plain = swofty_core::saves::plain::root_of_source(&d, &loc.source);
    write_generation(
        &plain,
        39,
        "save",
        Some("steam"),
        &[
            ("camera_camera_system", "game", &[]),
            ("options_options", "game", &[]),
            ("journal_entries_entry4", "game", &[]),
            ("gm_dev", "game", &[]),
            ("gm_driver_career_26", "game", &[]),
            (
                "driver_career_26_SaveSlotGameModeComponent",
                "game",
                &[("0x01", "Career Save 1")],
            ),
            ("gm_driver_career_sw_progressive", "game", &[]),
            (
                "driver_career_sw_progressive_SaveSlotGameModeComponent",
                "game",
                &[("0x0a", "Career Save 1")],
            ),
            ("gm_driver_career_progressive", "kept", &[]),
            ("driver_career_progressive_SaveSlotGameModeComponent", "kept", &[]),
        ],
    );
    write_generation(
        &plain,
        40,
        "load",
        Some("ea"),
        &[
            ("gm_driver_career_sw_progressive", "injected", &[]),
            (
                "driver_career_sw_progressive_SaveSlotGameModeComponent",
                "injected",
                &[],
            ),
            ("gm_driver_career_progressive", "injected", &[]),
            (
                "driver_career_progressive_SaveSlotGameModeComponent",
                "injected",
                &[],
            ),
        ],
    );
    assert!(banner_from(&d, loc.source.clone(), "default".into()).is_none());
    write_generation(
        &plain,
        41,
        "load",
        Some("steam-addons-off"),
        &[
            ("camera_camera_system", "game", &[]),
            ("options_options", "game", &[]),
            ("gm_driver_career_26", "game", &[]),
            (
                "driver_career_26_SaveSlotGameModeComponent",
                "game",
                &[("0x01", "Career Save 1")],
            ),
            ("gm_driver_career_sw_progressive", "kept", &[]),
            (
                "driver_career_sw_progressive_SaveSlotGameModeComponent",
                "kept",
                &[],
            ),
            ("gm_driver_career_progressive", "kept", &[]),
            ("driver_career_progressive_SaveSlotGameModeComponent", "kept", &[]),
        ],
    );
    assert!(banner_from(&d, loc.source.clone(), "default".into()).is_none());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_smaller_save_alone_raises_no_banner() {
    let d = std::env::temp_dir().join(format!("swofty-launcher-saves-small-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    let loc = steam_source(&d);
    let v = Vault::of(&d, &loc).unwrap();
    v.snapshot(&loc, vault::Meta::new(vault::Reason::AfterSession), 1_000)
        .unwrap();
    std::fs::write(loc.dir.join("savegame@profile#NXDSMWW.BWW"), vec![2u8; 10]).unwrap();
    v.snapshot(&loc, vault::Meta::new(vault::Reason::ForeignExit), 2_000)
        .unwrap();
    assert!(banner_from(&d, loc.source.clone(), "p".into()).is_none());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_page_groups_careers_by_mode_and_restores_both_halves_from_one_generation() {
    let d = std::env::temp_dir().join(format!("swofty-launcher-saves-plain-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    let root = d.join("saves").join("p").join("plain");
    let halves = |who: Slots<'static>| -> Vec<(&'static str, &'static str, Slots<'static>)> {
        vec![
            ("gm_driver_career_progressive", "kept", who),
            ("driver_career_progressive_SaveSlotGameModeComponent", "kept", &[]),
            ("gm_driver_career", "game", &[("0x01", "Stock")]),
            ("driver_career_SaveSlotGameModeComponent", "game", &[]),
        ]
    };
    write_generation(&root, 1, "save", Some("s1"), &halves(&[("0x0a", "Old")]));
    write_generation(
        &root,
        2,
        "save",
        Some("s1"),
        &halves(&[("0x0a", "Jacob"), ("0x0b", "Ada")]),
    );
    let view = super::plain::build_plain(&d, "launcher-p").unwrap();
    assert_eq!(view.generation, 2);
    assert_eq!(view.careers.len(), 3);
    let modes: Vec<&str> = view.modes.iter().map(|m| m.mode.as_str()).collect();
    assert_eq!(modes, ["driver_career_progressive", "driver_career"]);
    let progressive = &view.modes[0];
    assert_eq!(progressive.containers.len(), 2);
    assert_eq!(progressive.older.len(), 1);
    assert_eq!(progressive.older[0].generation, 1);
    assert!(view.modes[1].older.is_empty(), "the stock mode never changed");
    let jacob = view.careers.iter().find(|c| c.name == "Jacob").unwrap();
    assert_eq!(jacob.mode, "driver_career_progressive");
    assert!(!jacob.waiting);

    let plain = swofty_core::saves::plain::Plain::of_source(&d, "launcher-p");
    let asked = plain
        .restore_mode(progressive.older[0].generation, "driver_career_progressive", 5)
        .unwrap();
    assert_eq!(asked.len(), 2);
    assert!(asked.iter().all(|r| r.generation == 1));
    let view = super::plain::build_plain(&d, "launcher-p").unwrap();
    assert!(view.modes[0].waiting && !view.modes[1].waiting);
    assert!(view.waiting_line().is_some());
    std::fs::write(
        root.join("injected.json"),
        r#"{"schema":1,"results":[{"id":"r1","ok":true,"detail":"1 career(s)"}]}"#,
    )
    .unwrap();
    assert!(
        super::plain::build_plain(&d, "launcher-p")
            .unwrap()
            .answer_line()
            .unwrap()
            .contains("1 career(s)")
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_banner_names_the_missing_careers_and_the_version_to_restore() {
    let d = data("drop");
    let table = vault::careers::Careers {
        version: 1,
        careers: vec![
            row("a", Status::MissingSince("later".into()), 10),
            row("b", Status::MissingSince("later".into()), 20),
            row("c", Status::Present, 30),
        ],
    };
    table.write(&d.join("career-vault/steam-1")).unwrap();
    match banner_from(&d, "steam-1".into(), "vanilla".into()) {
        Some(Banner::Dropped(Missing {
            snapshot,
            names,
            kept_at,
            profile,
            ..
        })) => {
            assert_eq!(profile, "vanilla");
            assert_eq!(snapshot, "snap-20");
            assert_eq!(kept_at, 20);
            assert_eq!(
                names,
                ["a label, season 2016, round 7", "b label, season 2016, round 7"]
            );
        }
        _ => panic!("expected a drop banner"),
    }
    Vault::open(&d, "steam-1").unwrap().dismiss_drop().unwrap();
    assert!(banner_from(&d, "steam-1".into(), "vanilla".into()).is_none());
}

#[test]
fn no_careers_and_no_notice_means_no_banner() {
    let d = data("none");
    assert!(banner_from(&d, "steam-1".into(), "vanilla".into()).is_none());
}

#[test]
fn mode_names_read_like_the_game() {
    use swofty_core::saves::vault::careers::mode_label;
    assert_eq!(mode_label(Some("driver_career")), "Driver Career");
    assert_eq!(mode_label(Some("my_team")), "My Team");
    assert_eq!(mode_label(Some("progressive")), "Progressive");
    assert_eq!(mode_label(Some("sprint_cup")), "Sprint Cup");
    assert_eq!(mode_label(None), "Career");
}

#[test]
fn versions_and_places_read_short() {
    use super::{careers_held_line, season_round};
    use swofty_core::saves::vault::careers::listing;
    assert_eq!(careers_held_line(10, None, 0), "10 careers");
    assert_eq!(
        careers_held_line(1, Some("Vettel"), 0),
        "1 career, newest: Vettel"
    );
    assert_eq!(
        careers_held_line(3, Some("Vettel"), 2),
        "3 careers, newest: Vettel, 2 marked missing"
    );
    assert_eq!(careers_held_line(0, None, 0), "0 careers");
    let l = listing(
        &swofty_core::stdlib::careers::parse(
            r#"{"careers":[{"career":"f1_career_2026","slot":"0","slot_season":1,"slot_weekend":2}]}"#,
        ),
        None,
    );
    assert_eq!(season_round(&l[0]).as_deref(), Some("Season 1 · Round 3"));
    assert_eq!(l[0].title(), "Driver Career · 2026 season");
}

#[test]
fn lists_name_the_place() {
    let b = |n: &str, p: Option<&str>| Brief {
        career: n.into(),
        name: n.into(),
        place: p.map(str::to_owned),
    };
    assert_eq!(
        list(&[b("A", Some("season 2016, round 7")), b("B", None)]),
        "A (season 2016, round 7), B"
    );
}

fn marker(profiles: &[&str], seen: bool) -> Marker {
    Marker {
        at: 1,
        launcher: "1.7.0".into(),
        profiles: profiles.iter().map(|p| p.to_string()).collect(),
        note: String::new(),
        seen,
    }
}

#[test]
fn the_first_start_notice_shows_once_and_is_remembered() {
    let d = data("notice");
    let mut page = SavesPage::default();
    assert!(!page.take_notice(&d));
    swofty_core::write_json(&launcher_saves::marker_path(&d), &marker(&["default"], false)).unwrap();
    assert!(page.take_notice(&d));
    assert!(!page.take_notice(&d));
    assert!(launcher_saves::marker(&d).unwrap().seen);
    assert!(!SavesPage::default().take_notice(&d));
}

#[test]
fn no_notice_when_the_import_copied_nothing_or_was_seen() {
    let d = data("notice-none");
    swofty_core::write_json(&launcher_saves::marker_path(&d), &marker(&[], false)).unwrap();
    assert!(!SavesPage::default().take_notice(&d));
    swofty_core::write_json(&launcher_saves::marker_path(&d), &marker(&["default"], true)).unwrap();
    assert!(!SavesPage::default().take_notice(&d));
}

#[test]
fn versions_read_in_plain_words() {
    use swofty_core::saves::vault::Reason;
    assert_eq!(reason_label(Reason::BeforeSession), "Before Play");
    assert_eq!(reason_label(Reason::Migrated), "Old backup");
    assert_eq!(reason_label(Reason::Imported), "Imported from Steam");
    assert_eq!(whose("vanilla"), "your Steam save");
    assert_eq!(whose("default"), "profile default's save");
}

#[test]
fn an_imported_save_with_no_listing_gets_a_titled_placeholder() {
    use swofty_core::saves::vault::{Meta, Reason, Taken};
    let d = data("imported");
    let loc = swofty_core::saves::vault::Location::launcher(&d, "default");
    std::fs::create_dir_all(&loc.dir).unwrap();
    std::fs::write(loc.dir.join("savegame@profile#NXDSMWW.BWW"), [1u8; 64]).unwrap();
    std::fs::write(loc.dir.join("savegame@profile#QKRHMYXE"), [2u8; 16]).unwrap();
    let v = Vault::of(&d, &loc).unwrap();
    let note = format!("copied from {}", d.join("saves/progressive/remote").display());
    let Taken::New(_) = v
        .snapshot(&loc, Meta::new(Reason::ImportedProfile).note(note), 700)
        .unwrap()
    else {
        panic!()
    };
    let i = super::imported_from(&v.list().unwrap());
    assert_eq!(i.title, "Copied from progressive");
    assert_eq!(i.at, Some(700));
    assert_eq!(super::imported_from(&[]).title, "Imported Steam save");
}

#[test]
fn the_fixed_columns_leave_room_at_the_narrowest_window() {
    let content = 960. - 220. - 2. * 28.;
    let career_fixed = super::MODE_W + super::PLAYED_W + super::CHIP_W + 3. * 10. + 24.;
    let version_fixed = super::PIN_W + super::WHEN_W + super::SIZE_W + super::ACTIONS_W + 4. * 10. + 24.;
    assert!(content - career_fixed >= 150., "{}", content - career_fixed);
    assert!(content - version_fixed >= 150., "{}", content - version_fixed);
}

#[test]
fn restore_waits_for_what_is_running() {
    use super::short_block;
    use swofty_core::saves::vault::Layout;
    let run = |game, steam| swofty_core::saves::vault::restore::Running {
        game,
        steam,
        ea_app: false,
    };
    assert_eq!(
        short_block(Layout::Launcher, run(true, false)),
        Some("Close F1 25 first")
    );
    assert_eq!(short_block(Layout::Launcher, run(false, true)), None);
    assert_eq!(
        short_block(Layout::Steam, run(false, true)),
        Some("Close Steam first")
    );
    assert_eq!(short_block(Layout::Steam, run(false, false)), None);
}

fn copy_of(name: &str, ea: bool) -> PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-saves-copy-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("game");
    std::fs::create_dir_all(game.join("__Installer")).unwrap();
    std::fs::write(game.join(swofty_core::GAME_EXE), b"exe").unwrap();
    match ea {
        true => std::fs::write(game.join("__Installer/installerdata.xml"), b"<game/>").unwrap(),
        false => std::fs::write(game.join("EAJavelinInstaller_installscript.vdf"), b"").unwrap(),
    }
    let pfx = root.join("pfx");
    let save = pfx.join("drive_c/users/steamuser/AppData/Local/F1 25/savegame/ea/profile");
    std::fs::create_dir_all(&save).unwrap();
    std::fs::write(save.join("NXDSMWW.BWW"), b"data").unwrap();
    std::fs::write(pfx.join("user.reg"), b"").unwrap();
    let mut config = swofty_core::session::Config::default();
    config.games.active = Some(game.clone());
    config.games.added = vec![game];
    config.launch.ea_prefix = Some(pfx);
    swofty_core::store::Store::new(&root)
        .save_config(&config)
        .unwrap();
    std::fs::create_dir_all(root.join("profiles")).unwrap();
    for p in ["hi", "default"] {
        std::fs::write(
            root.join("profiles").join(format!("{p}.json")),
            format!(r#"{{"name":"{p}","launcher_saves":true}}"#),
        )
        .unwrap();
    }
    root
}

#[cfg(not(windows))]
#[test]
fn an_ea_copy_profile_shows_its_own_launcher_save() {
    let root = copy_of("ea", true);
    let store = swofty_core::store::Store::new(&root);
    for p in ["hi", "default"] {
        let built = build_saves(&store, None, p, false);
        let loc = built.view.loc.as_ref().expect("the EA save");
        assert_eq!(
            (loc.source.as_str(), loc.layout),
            (format!("launcher-ea.{p}").as_str(), vault::Layout::LauncherEa),
            "{p}"
        );
        assert!(!built.view.shared_by_every_profile(), "{p}");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_steam_copy_profile_shows_its_own_launcher_save() {
    let root = copy_of("steam", false);
    let store = swofty_core::store::Store::new(&root);
    let built = build_saves(&store, None, "hi", false);
    let loc = built.view.loc.as_ref().expect("the launcher save");
    assert_eq!(
        (loc.source.as_str(), loc.layout),
        ("launcher-hi", vault::Layout::Launcher)
    );
    assert!(!built.view.shared_by_every_profile());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_saves_refresh_overtaken_by_a_newer_one_is_dropped() {
    let root = std::env::temp_dir().join(format!("swofty-saves-refresh-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let store = swofty_core::store::Store::new(&root);
    let mut page = SavesPage::default();
    let old = page.next_generation();
    let new = page.next_generation();
    assert!(!page.took(old, build_saves(&store, None, "Race", false)));
    assert!(page.view.is_none());
    assert!(page.took(new, build_saves(&store, None, "Race", false)));
    assert_eq!(page.view.as_ref().map(|v| v.profile.as_str()), Some("Race"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_save_stuck_in_a_started_weekend_offers_the_version_before_it() {
    use swofty_core::saves::stuck::{self, Ledger, Stuck, Weekend};
    let d = data("stuck");
    let loc = vault::Location::launcher(&d, "p");
    std::fs::create_dir_all(&loc.dir).unwrap();
    let file = loc.dir.join("savegame@profile#NXDSMWW.BWW");
    let v = Vault::of(&d, &loc).unwrap();
    std::fs::write(&file, vec![3u8; 64]).unwrap();
    let good = v
        .snapshot(&loc, vault::Meta::new(vault::Reason::AfterSession), 1_000)
        .unwrap();
    std::fs::write(&file, vec![9u8; 65]).unwrap();
    v.snapshot(&loc, vault::Meta::new(vault::Reason::AfterSession), 2_000)
        .unwrap();
    let sha = swofty_core::game_folder::baseline::sha256_file(&file).unwrap();
    Ledger {
        saves: vec![Stuck {
            source: loc.source.clone(),
            sha256: sha.clone(),
            size: 65,
            weekend: Weekend {
                round: 12,
                rounds: 22,
                session: Some("practice_1".into()),
                track: Some("game/hungaroring".into()),
            },
            sessions: vec!["20261120-104632".into()],
            profile: Some("p".into()),
            found_at: 3_000,
            dismissed: false,
        }],
        read: Vec::new(),
    }
    .write(&d)
    .unwrap();
    let banner = banner_from(&d, loc.source.clone(), "p".into()).expect("a stuck banner");
    let shown = banner.shown();
    assert!(
        shown
            .text
            .contains("inside its round 12 of 22 (hungaroring) weekend"),
        "{}",
        shown.text
    );
    assert_eq!(shown.snapshot, good.snapshot().unwrap().id);
    assert!(shown.restorable);
    stuck::dismiss(&d, &loc.source, &sha).unwrap();
    assert!(banner_from(&d, loc.source.clone(), "p".into()).is_none());
    std::fs::remove_dir_all(&d).ok();
}
