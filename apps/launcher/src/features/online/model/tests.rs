use std::collections::HashSet;

use super::*;

#[test]
fn gaps_and_scores_read_like_a_leaderboard() {
    assert_eq!(gap_text(91_671, 91_234), "+0.437");
    assert_eq!(gap_text(91_234, 91_234), "");
    assert_eq!(gap_text(160_000, 91_234), "+68.766");
    assert_eq!(score_text(8430.4), "8,430");
    assert_eq!(score_text(12.), "12");
    assert_eq!(score_text(1_234_567.), "1,234,567");
}

#[test]
fn sectors_are_purple_for_the_overall_best_and_green_for_a_personal_best() {
    assert_eq!(
        sector_mark(29_000, Some(29_000), Some(29_000)),
        SectorMark::Overall
    );
    assert_eq!(
        sector_mark(29_400, Some(29_000), Some(29_400)),
        SectorMark::Personal
    );
    assert_eq!(sector_mark(29_800, Some(29_000), Some(29_400)), SectorMark::Plain);
}

#[test]
fn the_best_sector_of_each_column_is_found() {
    let d = Driver {
        id: "1".into(),
        name: "a".into(),
        avatar: None,
    };
    let row = |s: Vec<u64>| Row {
        rank: 1,
        lap_id: "x".into(),
        driver: d.clone(),
        time_ms: s.iter().sum(),
        sectors_ms: s,
        car: None,
        date: String::new(),
        input: None,
        aids: None,
        setup: None,
        game_replay: false,
    };
    let rows = [row(vec![30, 31, 32]), row(vec![29, 33, 31])];
    assert_eq!(best_sectors(&rows, 3), [Some(29), Some(31), Some(31)]);
    assert_eq!(best_sectors(&[], 3), [None, None, None]);
}

#[test]
fn the_trail_pushes_pops_and_cuts_back_to_a_crumb() {
    let mut t = Trail::new();
    assert_eq!(*t.current(), Screen::Landing);
    assert!(!t.back());
    t.push(Screen::Season("2016".into()), "2016");
    t.push(
        Screen::Board(BoardKey {
            content: "c".into(),
            track: "suzuka".into(),
            weather: "dry".into(),
        }),
        "Suzuka",
    );
    assert_eq!(t.crumbs(), [(2, "2016"), (3, "Suzuka")]);
    t.jump(2);
    assert_eq!(*t.current(), Screen::Season("2016".into()));
    t.push(Screen::Profile("7".into()), "swofty");
    assert!(t.back());
    assert_eq!(t.depth(), 2);
    t.push(Screen::Season("2016".into()), "2016");
    assert_eq!(t.depth(), 2);
}

fn card(name: &str, record: Option<u64>, drivers: u32, laps: u32, weather: &str) -> TrackCard {
    let d = Driver {
        id: "1".into(),
        name: "a".into(),
        avatar: None,
    };
    TrackCard {
        key: BoardKey {
            content: "c".into(),
            track: name.to_lowercase(),
            weather: weather.into(),
        },
        name: name.into(),
        flag: None,
        layout: None,
        record: record.map(|t| PodiumEntry {
            driver: d.clone(),
            time_ms: t,
        }),
        podium: Vec::new(),
        drivers,
        laps,
        custom: false,
        addon: None,
    }
}

fn sample() -> Vec<TrackCard> {
    vec![
        card("Suzuka", Some(88_000), 5, 40, "dry"),
        card("Monza", Some(80_000), 9, 12, "dry"),
        card("Baku", None, 0, 0, "dry"),
        card("Spa", Some(103_000), 7, 90, "wet"),
    ]
}

#[test]
fn tracks_filter_by_name_laps_weather_and_mine() {
    let tracks = sample();
    let none = HashSet::new();
    let names = |f: &TrackFilter, mine: &HashSet<BoardKey>| -> Vec<String> {
        f.apply(&tracks, mine).iter().map(|t| t.name.clone()).collect()
    };
    assert_eq!(
        names(&TrackFilter::default(), &none),
        ["Baku", "Monza", "Spa", "Suzuka"]
    );
    let q = TrackFilter {
        query: " su".into(),
        ..Default::default()
    };
    assert_eq!(names(&q, &none), ["Suzuka"]);
    let has = TrackFilter {
        laps: LapsFilter::Has,
        ..Default::default()
    };
    assert_eq!(names(&has, &none), ["Monza", "Spa", "Suzuka"]);
    let empty = TrackFilter {
        laps: LapsFilter::None,
        ..Default::default()
    };
    assert_eq!(names(&empty, &none), ["Baku"]);
    let wet = TrackFilter {
        weather: Some("wet".into()),
        ..Default::default()
    };
    assert_eq!(names(&wet, &none), ["Spa"]);
    let mine_only = TrackFilter {
        mine: true,
        ..Default::default()
    };
    let mine: HashSet<BoardKey> = [tracks[1].key.clone()].into();
    assert_eq!(names(&mine_only, &mine), ["Monza"]);
    assert!(mine_only.active() && !TrackFilter::default().active());
}

#[test]
fn tracks_sort_by_record_drivers_and_laps() {
    let tracks = sample();
    let none = HashSet::new();
    let order = |sort| -> Vec<String> {
        TrackFilter {
            sort,
            ..Default::default()
        }
        .apply(&tracks, &none)
        .iter()
        .map(|t| t.name.clone())
        .collect()
    };
    assert_eq!(order(Sort::Record), ["Monza", "Suzuka", "Spa", "Baku"]);
    assert_eq!(order(Sort::Drivers), ["Monza", "Spa", "Suzuka", "Baku"]);
    assert_eq!(order(Sort::Laps), ["Spa", "Suzuka", "Monza", "Baku"]);
    assert_eq!(weathers(&tracks), ["dry", "wet"]);
    assert!(weathers(&tracks[..3]).is_empty());
}

#[test]
fn recent_seasons_keep_the_newest_first_without_repeats_and_survive_a_restart() {
    let season = |i: usize| format!("swofty.swofty-seasons-{}/sw_{0}", 2000 + i);
    let mut r = Recents::default();
    r.visit("game/f1_2025", "A");
    r.visit(&season(1), "B");
    r.visit("game/f1_2025", "A");
    assert_eq!(
        r.list().iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["game/f1_2025", season(1).as_str()]
    );
    for i in 0..20 {
        r.visit(&season(i), "S");
    }
    assert_eq!(r.list().len(), RECENT_CAP);
    assert_eq!(r.list()[0].id, season(19));
    let dir = std::env::temp_dir().join(format!("swofty-tt-recent-{}", std::process::id()));
    r.save(&dir);
    assert_eq!(Recents::load(&dir), r);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(Recents::load(&dir), Recents::default());
}

#[test]
fn recent_seasons_leave_out_unlisted_content() {
    let mut r = Recents::default();
    r.visit("swofty.classic-seasons-2020/f1_2020", "F1 2020 GP/TT Addon");
    assert!(r.list().is_empty());
    let dir = std::env::temp_dir().join(format!("swofty-tt-recent-old-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("state")).unwrap();
    std::fs::write(
        dir.join("state").join("tt-recent.json"),
        r#"{"seasons":[{"id":"bydanix.f1-2012-season/ex_f1_2012","name":"F1 2012"},{"id":"game/f1_2025","name":"F1 25"}]}"#,
    )
    .unwrap();
    assert_eq!(
        Recents::load(&dir)
            .list()
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["game/f1_2025"]
    );
    std::fs::remove_dir_all(&dir).ok();
}

fn setup_of(front: f32, compound: &str) -> Setup {
    Setup {
        values: vec![
            ("front_wing", front),
            ("brake_bias", 56.),
            ("front_camber", -3.5),
            ("fuel_load", 30.),
        ],
        compound: Some(compound.into()),
        changed: false,
    }
}

#[test]
fn setup_values_read_like_the_game_screen() {
    assert_eq!(setup_value_text(25., Unit::Plain), "25");
    assert_eq!(setup_value_text(56., Unit::Percent), "56%");
    assert_eq!(setup_value_text(-3.5, Unit::Degrees), "-3.5\u{b0}");
    assert_eq!(setup_value_text(22.5, Unit::Psi), "22.5\u{a0}psi");
    assert_eq!(setup_value_text(30., Unit::Kg), "30 kg");
}

#[test]
fn a_setup_is_grouped_copied_and_compared() {
    let a = setup_of(25., "soft");
    let b = setup_of(20., "soft");
    let text = a.text();
    assert!(text.contains("Aerodynamics\n  Front wing: 25\n"));
    assert!(text.contains("Tyres\n  Compound: soft"));
    assert!(!text.contains("Rear wing"));
    assert_eq!(a.difference(&b, "front_wing"), Some(5.));
    assert_eq!(a.difference(&b, "brake_bias"), None);
    assert_eq!(a.difference(&b, "rear_wing"), None);
    assert!(Setup::default().is_empty() && !a.is_empty());
}

fn page(n: usize, next: Option<u32>) -> RankingPage {
    let d = Driver {
        id: "1".into(),
        name: "a".into(),
        avatar: None,
    };
    RankingPage {
        rows: (0..n)
            .map(|i| RankEntry {
                rank: i as u32 + 1,
                driver: d.clone(),
                points: 1.,
                wins: 0,
                podiums: 0,
            })
            .collect(),
        total: 120,
        next_offset: next,
    }
}

#[test]
fn the_feed_pages_in_and_asks_for_more_near_the_end() {
    let mut f = Feed::default();
    assert!(f.wants_more(0).is_none());
    f.apply(0, page(50, Some(50)));
    assert_eq!(f.wants_more(45), Some(50));
    assert_eq!(f.wants_more(10), None);
    f.loading = true;
    assert_eq!(f.wants_more(49), None);
    f.apply(50, page(50, Some(100)));
    assert_eq!(f.rows.len(), 100);
    f.apply(100, page(20, None));
    assert_eq!(f.rows.len(), 120);
    assert!(f.finished() && f.wants_more(120).is_none());
    f.apply(0, page(50, Some(50)));
    assert_eq!(f.rows.len(), 50);
    f.failed("down".into());
    assert!(f.wants_more(49).is_none() && !f.finished());
}

#[test]
fn the_top_three_are_the_first_chunk_and_rows_follow_in_fives() {
    assert_eq!(CHUNK as f32 * ROW_H, CHUNK_H);
    assert_eq!(3. * TOP_ROW_H, CHUNK_H);
    assert_eq!(chunk_slots(0, 50), [Slot::Top(0), Slot::Top(1), Slot::Top(2)]);
    assert_eq!(
        chunk_slots(1, 50),
        [
            Slot::Row(3),
            Slot::Row(4),
            Slot::Row(5),
            Slot::Row(6),
            Slot::Row(7)
        ]
    );
    assert_eq!(chunk_count(50), 1 + 48usize.div_ceil(5));
    let last = chunk_count(50) - 1;
    assert_eq!(chunk_slots(last, 50).last(), Some(&Slot::Footer));
    assert_eq!(chunk_slots(0, 2), [Slot::Top(0), Slot::Top(1)]);
    assert_eq!(chunk_slots(1, 2), [Slot::Footer]);
    assert_eq!(chunk_count(0), 2);
}

#[test]
fn the_category_maps_to_the_query_and_is_remembered() {
    assert_eq!(Category::default().query(), "");
    let c = Category {
        assists: AssistFilter::None,
        device: DeviceFilter::Wheel,
    };
    assert_eq!(c.query(), "&assists=none&device=wheel");
    assert_eq!(
        Category {
            assists: AssistFilter::Assists,
            device: DeviceFilter::Keyboard
        }
        .query(),
        "&assists=any&device=keyboard"
    );
    assert_eq!(c.suffix(), "No assists - Wheel");
    let dir = std::env::temp_dir().join(format!("swofty-tt-cat-{}", std::process::id()));
    assert_eq!(Category::load(&dir), Category::default());
    c.save(&dir);
    assert_eq!(Category::load(&dir), c);
    std::fs::remove_dir_all(&dir).ok();
    assert!(!c.is_default() && Category::default().is_default());
}

#[test]
fn an_empty_overview_under_a_saved_filter_keeps_the_filter_bar() {
    let none = Overview {
        seasons: Vec::new(),
        global: Vec::new(),
        custom: Vec::new(),
        me: None,
    };
    let no_assists = Category {
        assists: AssistFilter::None,
        device: DeviceFilter::All,
    };
    let dir = std::env::temp_dir().join(format!("swofty-tt-empty-{}", std::process::id()));
    no_assists.save(&dir);
    let restarted = Category::load(&dir);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(landing_empty(&none, &restarted), Some(LandingEmpty::Filtered));
    assert_eq!(
        landing_empty(&none, &Category::default()),
        Some(LandingEmpty::NothingYet)
    );
    let some = Overview {
        custom: vec![card("Hockenheim", None, 0, 0, "dry")],
        ..none
    };
    assert_eq!(landing_empty(&some, &no_assists), None);
}

#[test]
fn a_filtered_board_shows_positions_with_the_overall_rank_beside_them() {
    assert_eq!(shown_rank(0, 4, true), (1, Some(4)));
    assert_eq!(shown_rank(2, 3, true), (3, None));
    assert_eq!(shown_rank(0, 4, false), (4, None));
}

#[test]
fn inputs_and_assists_are_understood() {
    assert_eq!(InputDevice::parse("pad"), Some(InputDevice::Controller));
    assert_eq!(InputDevice::parse("Wheel"), Some(InputDevice::Wheel));
    assert_eq!(InputDevice::parse("tv"), None);
    let a = Assists {
        levels: vec![(Aid::TractionControl, 0), (Aid::Abs, 0)],
    };
    assert!(a.none() && !a.on(Aid::Abs));
    let b = Assists {
        levels: vec![(Aid::TractionControl, 2), (Aid::Abs, 0)],
    };
    assert!(!b.none() && b.on(Aid::TractionControl));
    assert!(!Assists::default().none());
}

fn board_key(track: &str) -> BoardKey {
    BoardKey {
        content: "s".into(),
        track: track.into(),
        weather: "dry".into(),
    }
}

#[test]
fn a_track_opens_in_place_one_at_a_time_and_reads_its_board_once() {
    let mut peek: Peek<u32> = Peek::default();
    assert!(
        peek.toggle(board_key("bahrain")),
        "a track never read asks for its board"
    );
    assert!(peek.is_open(&board_key("bahrain")));
    peek.put(board_key("bahrain"), 1);
    assert!(
        peek.toggle(board_key("monza")),
        "opening another closes the first"
    );
    assert!(!peek.is_open(&board_key("bahrain")));
    assert!(
        !peek.toggle(board_key("bahrain")),
        "a board read before shows at once"
    );
    assert_eq!(peek.page(&board_key("bahrain")), Some(&1));
    assert!(!peek.toggle(board_key("bahrain")), "the open track closes");
    assert!(!peek.is_open(&board_key("bahrain")));
}

#[test]
fn a_reload_forgets_the_boards_but_keeps_the_open_track_to_read_again() {
    let mut peek: Peek<u32> = Peek::default();
    assert_eq!(peek.reset(), None);
    peek.toggle(board_key("suzuka"));
    peek.put(board_key("suzuka"), 7);
    assert_eq!(peek.reset(), Some(board_key("suzuka")));
    assert_eq!(peek.page(&board_key("suzuka")), None);
    assert!(peek.is_open(&board_key("suzuka")));
}

fn lap_row(driver: &str, lap: &str) -> Row {
    Row {
        rank: 1,
        lap_id: lap.into(),
        driver: Driver {
            id: driver.into(),
            name: format!("name-{driver}"),
            avatar: None,
        },
        time_ms: 90_000,
        sectors_ms: Vec::new(),
        car: None,
        date: String::new(),
        input: None,
        aids: None,
        setup: None,
        game_replay: false,
    }
}

fn lap_of(row: Option<&Row>) -> Option<&str> {
    row.map(|r| r.lap_id.as_str())
}

#[test]
fn the_own_lap_is_the_one_the_server_marks_else_the_signed_in_drivers_row() {
    let rows = [lap_row("1", "a"), lap_row("2", "b")];
    let marked = lap_row("2", "z");
    assert_eq!(lap_of(own_row(&rows, Some(&marked), Some("1"))), Some("z"));
    assert_eq!(lap_of(own_row(&rows, None, Some("2"))), Some("b"));
    assert_eq!(own_row(&rows, None, Some("9")), None);
    assert_eq!(own_row(&rows, None, None), None);
}

#[test]
fn a_full_board_pins_the_own_lap_only_when_its_rows_lack_it() {
    let rows = [lap_row("1", "a"), lap_row("2", "b")];
    let marked = lap_row("3", "z");
    let found = own_row(&rows, None, Some("2"));
    assert_eq!(pinned_row(&rows, found), None);
    let outside = own_row(&rows, Some(&marked), Some("2"));
    assert_eq!(lap_of(pinned_row(&rows, outside)), Some("z"));
    let marked_inside = lap_row("1", "a");
    assert_eq!(
        pinned_row(&rows, own_row(&rows, Some(&marked_inside), None)),
        None
    );
    assert_eq!(pinned_row(&rows, own_row(&rows, None, Some("9"))), None);
}

#[test]
fn a_debug_start_names_a_track_and_its_weather() {
    let key = |track: &str, weather: &str| BoardKey {
        content: "s".into(),
        track: track.into(),
        weather: weather.into(),
    };
    assert_eq!(start_track("bahrain", "s"), Some(key("bahrain", "dry")));
    assert_eq!(start_track(" spa/wet ", "s"), Some(key("spa", "wet")));
    assert_eq!(start_track("", "s"), None);
    assert_eq!(start_track("spa/", "s"), None);
}

#[test]
fn the_hub_lists_the_picked_season_else_the_last_viewed_else_the_first() {
    let season = |id: &str| SeasonCard {
        id: id.into(),
        name: id.into(),
        cover: None,
        tracks: 0,
        drivers: 0,
        top: Vec::new(),
    };
    let recent = |id: &str| RecentSeason {
        id: id.into(),
        name: id.into(),
    };
    let seasons = [season("2016"), season("2017"), season("2018")];
    let recents = [recent("gone"), recent("2018")];
    let id = |s: Option<&SeasonCard>| s.map(|s| s.id.clone());
    assert_eq!(
        id(hub_season(&seasons, Some("2017"), &recents)).as_deref(),
        Some("2017")
    );
    assert_eq!(
        id(hub_season(&seasons, Some("gone"), &recents)).as_deref(),
        Some("2018")
    );
    assert_eq!(id(hub_season(&seasons, None, &[])).as_deref(), Some("2016"));
    assert_eq!(hub_season(&[], Some("2017"), &recents), None);
}

#[test]
fn the_time_trial_card_shows_the_players_best_against_the_record() {
    assert_eq!(own_gap(None, Some(90_000)), OwnGap::NoLap);
    assert_eq!(own_gap(Some(90_000), Some(90_000)), OwnGap::Record);
    assert_eq!(own_gap(Some(91_234), Some(90_000)), OwnGap::Behind(1_234));
    assert_eq!(own_gap(Some(91_234), None), OwnGap::Record);
}

#[test]
fn time_trial_looks_as_in_1_10_without_access_and_linked_with_the_rooms_with_it() {
    let classic = TrialLook::of(false);
    assert_eq!(classic, TrialLook::Classic);
    assert!(classic.lists_seasons());
    assert!(!classic.opens_in_place());
    assert!(classic.trail_at_root());
    let linked = TrialLook::of(true);
    assert_eq!(linked, TrialLook::Linked);
    assert!(!linked.lists_seasons());
    assert!(linked.opens_in_place());
    assert!(!linked.trail_at_root());
}

#[test]
fn a_track_opens_its_board_page_without_access_and_in_place_with_it() {
    use crate::features::online::Target;
    use crate::features::online::tracks::track_item;
    let t = card("Bahrain", Some(90_000), 3, 9, "dry");
    match track_item(TrialLook::of(false).opens_in_place(), &t) {
        Target::Board(c, name) => assert!(c.key == t.key && name == "Bahrain"),
        other => panic!("expected the board page, got {other:?}"),
    }
    match track_item(TrialLook::of(true).opens_in_place(), &t) {
        Target::Track(key) => assert!(key == t.key),
        other => panic!("expected the track in place, got {other:?}"),
    }
}
