use super::home::*;
use super::listing::*;
use super::page_rows::*;
use super::text::*;
use super::*;
use crate::addons::settings::{SessionAddon, SessionMap, SessionMod};

#[test]
fn a_description_past_the_tile_s_lines_ends_with_dots() {
    let desc = "Istanbul Park (Turkish Grand Prix) as track istanbul, with its own names in all 11 languages. Track v2.72 by Apoc & SK.";
    let wrapped = wrap_text(desc, 32);
    assert_eq!(wrapped.lines().count(), 5);
    let cut = clip_lines(&wrapped, 4, 32);
    let lines: Vec<&str> = cut.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[3].ends_with("..."), "{cut}");
    assert!(lines.iter().all(|l| l.chars().count() <= 32), "{cut}");
    assert_eq!(clip_lines("short", 4, 32), "short");
    assert_eq!(title_lines(&tagged("ISTANBUL PARK", "ADDON", TAG_STYLE)), 1);
    assert_eq!(
        title_lines(&tagged("SIMPLIFIED FOR SWOFTY", "ADDON", TAG_STYLE)),
        1
    );
    assert_eq!(
        title_lines(&tagged(
            "AUTODROMO INTERNAZIONALE DEL MUGELLO",
            "ADDON",
            TAG_STYLE
        )),
        2
    );
}

#[test]
fn a_long_description_is_cut_to_fit_the_games_strings() {
    let long = format!("by Swofty, v0.4.0\n{}", "é".repeat(200));
    let cut = fit(long);
    assert!(cut.len() <= 255, "{}", cut.len());
    assert!(cut.ends_with('…'));
    assert_eq!(fit("short".into()), "short");
}

#[test]
fn a_restart_note_is_kept_when_the_description_is_long() {
    let note = RESTART_NOTE;
    let long = "x".repeat(248);
    let body = with_note(&long, note);
    assert!(body.len() <= 255, "{}", body.len());
    assert!(body.ends_with(note));
    assert!(body.contains('…'));
}

#[test]
fn a_description_that_exactly_fits_is_not_cut() {
    let text = "y".repeat(255 - RESTART_NOTE.len() - 1);
    let body = with_note(&text, RESTART_NOTE);
    assert_eq!(body.len(), 255);
    assert_eq!(body, format!("{text} {RESTART_NOTE}"));
    assert_eq!(
        with_note("Short.", RESTART_NOTE),
        "Short. Applies after a restart."
    );
}

#[test]
fn a_restart_note_survives_multibyte_text_at_the_boundary() {
    for n in 100..140 {
        let text = format!("{}{}", "a".repeat(n % 3), "é🏁".repeat(n));
        let body = with_note(&text, RESTART_NOTE);
        assert!(body.len() <= 255, "{n}: {}", body.len());
        assert!(body.ends_with(RESTART_NOTE));
    }
}

#[test]
fn an_empty_description_keeps_only_the_note() {
    assert_eq!(with_note("", RESTART_NOTE), RESTART_NOTE);
    assert_eq!(with_note("  ", ""), "");
    assert!(with_note(&"z".repeat(400), "").len() <= 255);
}

#[test]
fn a_long_folder_name_is_cut_to_fit_the_games_strings() {
    let name = format!("{}ü🏁", "é".repeat(300)).to_uppercase();
    let cut = fit(name.clone());
    assert!(name.len() > 255);
    assert!(cut.len() <= 255, "{}", cut.len());
    assert!(cut.ends_with('…'));
    let emoji = fit("🏁".repeat(100));
    assert!(emoji.len() <= 255 && emoji.ends_with('…'));
    let tagged_title = tagged(&fit_to(name, 200), "ADDON", TAG_STYLE);
    assert!(tagged_title.len() <= 255, "{}", tagged_title.len());
}

fn map(code: &str, name: &str) -> SessionMap {
    SessionMap {
        code: code.into(),
        name: name.into(),
        addon: String::new(),
        version: "1.0".into(),
        author: "a".into(),
        description: String::new(),
    }
}

fn a_mod(id: &str, name: &str) -> SessionMod {
    SessionMod {
        id: id.into(),
        name: name.into(),
        version: "2.0".into(),
        author: "b".into(),
        description: String::new(),
    }
}

#[test]
fn a_long_list_splits_into_home_screens_of_at_most_24_tiles() {
    let plan = home_plan(70);
    let page = |s: &str, tiles: std::ops::Range<usize>, prev: Option<&str>, next: Option<&str>| HomePage {
        page: 0,
        pages: 3,
        screen: s.to_owned(),
        tiles,
        prev: prev.map(str::to_owned),
        next: next.map(str::to_owned),
    };
    let want = [
        page("swofty_settings", 0..24, None, Some("swofty_settings_2")),
        page(
            "swofty_settings_2",
            24..48,
            Some("swofty_settings"),
            Some("swofty_settings_3"),
        ),
        page("swofty_settings_3", 48..70, Some("swofty_settings_2"), None),
    ];
    for (i, (got, mut want)) in plan.iter().zip(want).enumerate() {
        want.page = i;
        assert_eq!(got, &want);
    }
    assert_eq!(plan.len(), 3);
    assert!(plan.iter().all(|h| h.pages == 3));
    for i in 0..70 {
        assert_eq!(home_of(i), plan[i / HOME_PAGE_TILES].screen);
        assert!(plan[i / HOME_PAGE_TILES].tiles.contains(&i));
    }
    assert_eq!(home_plan(24).len(), 1);
    assert_eq!(home_plan(25).len(), 2);
    assert_eq!(home_plan(0)[0].screen, HOME);
}

fn foldered(id: &str, folder: Option<&str>) -> SessionAddon {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": id, "version": "1.0.0", "author": "t", "enabled": true,
        "locked": false, "enable_commands": ["", ""], "folder": folder
    }))
    .unwrap()
}

fn slot_ids(slots: &[Slot]) -> Vec<usize> {
    slots
        .iter()
        .map(|s| match s {
            Slot::Entry(i) => *i,
            _ => usize::MAX,
        })
        .collect()
}

#[test]
fn the_home_lists_a_row_per_folder_in_the_order_the_folders_come() {
    let addons = [
        foldered("swofty.core", Some("Built In")),
        foldered("a", Some("Cars")),
        foldered("b", None),
        foldered("c", Some("Cars")),
        foldered("track.x", Some("Tracks")),
    ];
    let maps = [map("imola", "Imola")];
    let mods = [a_mod("ts.sauber", "Sauber")];
    let flow = entries(&addons, &maps, &mods);
    let ids: Vec<&str> = flow.iter().map(|e| e.id).collect();
    assert_eq!(
        ids,
        ["swofty.core", "a", "c", "b", "track.x", "imola", "ts.sauber"],
        "a folder's addons sit together, loose track packages join Tracks"
    );
    let plan = home_plan(flow.len());
    let rows = home_rows(&flow, &plan[0]);
    let shape: Vec<(&str, Vec<usize>)> = rows.iter().map(|(f, s)| (f.as_str(), slot_ids(s))).collect();
    assert_eq!(
        shape,
        [
            ("Built In", vec![0]),
            ("Cars", vec![1, 2]),
            (LOOSE_ADDONS, vec![3]),
            ("Tracks", vec![4, 5]),
            (LOOSE_MODS, vec![6]),
        ]
    );
}

#[test]
fn page_tiles_take_a_row_after_the_page_s_tiles() {
    let addons: Vec<SessionAddon> = (0..70).map(|i| foldered(&format!("a{i}"), None)).collect();
    let flow = entries(&addons, &[], &[]);
    let plan = home_plan(flow.len());
    let rows: Vec<Vec<(String, Vec<Slot>)>> = plan.iter().map(|h| home_rows(&flow, h)).collect();
    assert_eq!(rows[0].len(), 2);
    assert_eq!(slot_ids(&rows[0][0].1), (0..24).collect::<Vec<_>>());
    assert_eq!(rows[0][1], (PAGES_ROW.to_owned(), vec![Slot::Next]));
    assert_eq!(rows[1][1].1, [Slot::Next, Slot::Prev]);
    assert_eq!(rows[2][1].1, [Slot::Prev]);
    assert_eq!(slot_ids(&rows[2][0].1), (48..70).collect::<Vec<_>>());
}

#[test]
fn only_what_scrolling_hides_gets_scroll_signals() {
    let items: Vec<String> = (0..5).map(|i| format!("t{i}")).collect();
    let pins = |to| -> Vec<(String, String)> {
        scroll_to(&items, to, 3, "e")
            .iter()
            .map(|n| {
                let o = n.o().unwrap();
                (
                    o.str_at("to_object_name").unwrap().to_owned(),
                    o.str_at("to_pin_name").unwrap().to_owned(),
                )
            })
            .collect()
    };
    let pair = |t: &str, p: &str| (t.to_owned(), p.to_owned());
    assert_eq!(pins(1), [pair("t0", "show"), pair("t1", "show")]);
    assert_eq!(pins(3), [pair("t0", "hide"), pair("t1", "show")]);
    assert_eq!(pins(4), [pair("t0", "hide"), pair("t1", "hide")]);
    assert!(
        scroll_to(&items[..3], 2, 3, "e").is_empty(),
        "a row that fits never scrolls"
    );
}

#[test]
fn no_home_screen_grows_past_its_page_whatever_the_addon_count_or_folders() {
    for n in [1, 23, 24, 25, 60, 64, 72, 150, 500] {
        for per_folder in [1, 2, 3, 4, 24] {
            let addons: Vec<SessionAddon> = (0..n)
                .map(|i| foldered(&format!("a{i}"), Some(&format!("f{}", i / per_folder))))
                .collect();
            let flow = entries(&addons, &[], &[]);
            let plan = home_plan(n);
            assert_eq!(plan.len(), n.div_ceil(HOME_PAGE_TILES).max(1), "{n}");
            assert_eq!(plan.iter().map(|h| h.tiles.len()).sum::<usize>(), n, "{n}");
            for h in &plan {
                let rows = home_rows(&flow, h);
                let lens: Vec<usize> = rows.iter().map(|(_, s)| s.len()).collect();
                let tiles: usize = lens.iter().sum();
                assert!(
                    tiles <= HOME_PAGE_TILES + 2,
                    "{n}: {} has {tiles} tiles",
                    h.screen
                );
                // Each restore event scrolls the rows and its tile's row, then focuses.
                let scrolled = rows.len().saturating_sub(HOME_ROWS);
                let restore_signals: usize = lens
                    .iter()
                    .map(|len| len * (scrolled + len.saturating_sub(VISIBLE_COLUMNS) + 1))
                    .sum();
                assert!(restore_signals <= 27 * 24, "{n}/{per_folder}: {restore_signals}");
            }
        }
    }
}

#[test]
fn entries_list_maps_then_mods_with_their_own_screens() {
    let maps = [map("imola", "Imola")];
    let mods = [a_mod("ts.sauber", "Sauber"), a_mod("ts.rb", "Red Bull")];
    let flow = entries(&[], &maps, &mods);
    let shape: Vec<(Kind, &str, usize)> = flow
        .iter()
        .map(|e| (e.kind, e.screen.as_str(), e.index))
        .collect();
    assert_eq!(
        shape,
        [
            (Kind::Map, "swofty_map_0", 0),
            (Kind::Mod, "swofty_mod_0", 0),
            (Kind::Mod, "swofty_mod_1", 1),
        ]
    );
    assert_eq!(Kind::Mod.key(), "mod");
    assert_eq!(Kind::Mod.tag(), "MOD");
    assert_eq!((flow[1].name, flow[1].id), ("Sauber", "ts.sauber"));
    assert!(flow[1].options.is_empty());
}

fn track_addon(id: &str, enabled: bool) -> SessionAddon {
    serde_json::from_value(serde_json::json!({
        "id": id, "name": "Mugello", "version": "0.58.1", "author": "t", "enabled": enabled,
        "locked": false, "enable_commands": ["", ""]
    }))
    .unwrap()
}

#[test]
fn a_track_an_addon_brings_is_on_that_addons_tile_only() {
    let mut brought = map("mugello", "Mugello");
    brought.addon = "track.mugello".into();
    let mut off = map("istanbul", "Istanbul Park");
    off.addon = "ts.istanbul".into();
    let loose = map("imola", "Imola");
    let maps = [brought, off, loose];
    let addons = [
        track_addon("track.mugello", true),
        track_addon("ts.istanbul", false),
    ];
    let flow = entries(&addons, &maps, &[]);
    let shape: Vec<(Kind, &str)> = flow.iter().map(|e| (e.kind, e.screen.as_str())).collect();
    assert_eq!(
        shape,
        [
            (Kind::Addon, "swofty_addon_0"),
            (Kind::Map, "swofty_map_1"),
            (Kind::Map, "swofty_map_2"),
        ],
        "the enabled addon's track has no tile of its own; a disabled addon's and a loose one keep theirs"
    );
    let codes: Vec<&str> = flow[0].maps.iter().map(|m| m.code.as_str()).collect();
    assert_eq!(codes, ["mugello"]);
    let rows = addon_rows(&flow[0]);
    let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["row_code_0", "row_id", "row_version"]);
    assert!(matches!(&rows[0].kind, RowKind::Info(code) if code == "mugello"));
    assert_eq!(rows[1].body, "", "a track addon's page has more than its id");
}
