use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use super::index_file::*;
use super::play::*;
use super::record::*;
use super::rows::*;
use super::*;
use crate::manifest::AddonManifest;
use crate::row_ids::IdOwners;

const RECORD: &str = r#"{
  "career": "6f3e9a",
  "profile": "progressive",
  "label": "Progressive Career: Max Verstappen, 2016, season 1",
  "match": { "driver": 4103, "team": 2, "season_index": 0 },
  "made_with": { "launcher": "1.6.0", "core": "0.3.0", "game_build": 25251167 },
  "addons": [
    { "id": "swofty.progressive", "version": "1.3.0", "need": "^1.3.0", "why": "career content f1_career_progressive" },
    { "id": "mod.sochi", "version": "1.1.0", "need": "^1.1.0", "why": "calendar round 4: track mod.sochi/sochi" }
  ],
  "records": [ { "ref": "track:venue/sochi", "resolved": "mod.sochi/sochi", "stat_id": 18 } ]
}"#;

fn manifest(id: &str, version: &str) -> AddonManifest {
    serde_json::from_str(&format!(
        r#"{{ "schema_version": 1, "id": "{id}", "name": "N {id}", "version": "{version}", "author": "a",
             "compat": {{ "game": "f1_25" }} }}"#
    ))
    .unwrap()
}

#[test]
fn reads_every_shape_of_the_index() {
    let by_key = format!(r#"{{ "f1_career_progressive/pg-1849223-max_verstappen": {RECORD} }}"#);
    let c = parse(&by_key);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].career, "6f3e9a");
    assert_eq!(c[0].profile.as_deref(), Some("progressive"));
    assert_eq!(
        c[0].key.as_deref(),
        Some("f1_career_progressive/pg-1849223-max_verstappen")
    );
    assert_eq!(c[0].addons[1].need.as_deref(), Some("^1.1.0"));
    assert!(c[0].extra.contains_key("made_with"));
    assert_eq!(parse(&format!(r#"{{ "careers": [{RECORD}] }}"#)).len(), 1);
    assert_eq!(parse(&format!("[{RECORD}]")).len(), 1);
    assert!(parse("garbage").is_empty());
    assert!(parse(r#"{ "version": 1 }"#).is_empty());
}

#[test]
fn names_what_a_career_needs_in_a_profile() {
    let c = &parse(&format!("[{RECORD}]"))[0];
    let installed: BTreeMap<String, AddonManifest> = [
        ("swofty.progressive", manifest("swofty.progressive", "1.3.2")),
        ("mod.sochi", manifest("mod.sochi", "1.0.0")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let both: Vec<String> = vec!["swofty.progressive".into(), "mod.sochi".into()];
    assert_eq!(
        missing(c, &both, &installed),
        ["needs N mod.sochi (mod.sochi) ^1.1.0, and 1.0.0 is installed"]
    );
    assert_eq!(
        missing(c, &both[..1], &installed),
        ["needs N mod.sochi (mod.sochi), which is off"]
    );
    assert_eq!(
        missing(c, &both, &BTreeMap::new()),
        [
            "needs swofty.progressive, which isn't installed",
            "needs mod.sochi, which isn't installed"
        ]
    );
}

#[test]
fn says_which_careers_an_addon_would_drop_and_one_play_line() {
    let record = |id: &str, status: &str, addon: &str| {
        format!(
            r#"{{"career":"{id}","label":"L {id}","status":"{status}","slot":"{id}","mode":"driver_career","addons":[{{"id":"{addon}"}}]}}"#
        )
    };
    let list = parse(&format!(
        "[{}, {}, {}]",
        record("a", "present", "x.one"),
        record("b", "missing", "x.one"),
        record("c", "present", "x.two")
    ));
    let names = |v: Vec<&Career>| v.iter().map(|c| c.career.clone()).collect::<Vec<_>>();
    assert_eq!(names(needing(&list, "x.one")), ["a"]);
    assert_eq!(names(needing(&list, "x.two")), ["c"]);
    assert!(needing(&list, "x.none").is_empty());
    assert_eq!(list[0].field("mode"), Some("driver_career"));
    assert_eq!(list[0].field("driver"), None);
    assert_eq!(list[1].status(), "missing");
    let installed: BTreeMap<String, AddonManifest> = [
        ("x.one", manifest("x.one", "1.0.0")),
        ("x.two", manifest("x.two", "1.0.0")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let both: Vec<String> = vec!["x.one".into(), "x.two".into()];
    assert_eq!(play_line(&list, &both, &installed), None);
    let one: Vec<String> = vec!["x.one".into()];
    assert_eq!(
        play_line(&list, &one, &installed).as_deref(),
        Some("1 career won't load in this profile: L c needs N x.two (x.two), which is off")
    );
    assert_eq!(
        play_line(&list, &[], &installed).as_deref(),
        Some(
            "2 careers won't load in this profile: L a needs N x.one (x.one), which is off; L c needs N x.two (x.two), which is off"
        )
    );
}

#[test]
fn careers_deleted_in_game_are_left_out_of_the_play_line_and_stop_line() {
    let record = |id: &str, status: &str| {
        format!(
            r#"{{"career":"{id}","label":"L {id}","status":"{status}","slot":"{id}","addons":[{{"id":"x.one","why":"{OWN_WHY}"}}]}}"#
        )
    };
    let installed = installed_of(&["x.one"]);
    let list_of = |status: &str| parse(&format!("[{}]", record("a", status)));

    assert!(play_line(&list_of("present"), &[], &installed).is_some());
    assert!(stop_line(&list_of("present"), &[], &installed).is_some());
    assert!(needing(&list_of("present"), "x.one").len() == 1);

    let deleted = list_of("deleted");
    assert_eq!(play_line(&deleted, &[], &installed), None);
    assert_eq!(stop_line(&deleted, &[], &installed), None);
    assert!(needing(&deleted, "x.one").is_empty());

    let missing = list_of("missing");
    assert_eq!(play_line(&missing, &[], &installed), None);
    assert_eq!(stop_line(&missing, &[], &installed), None);
    assert!(needing(&missing, "x.one").is_empty());
}

#[test]
fn a_record_no_save_slot_backs_raises_nothing() {
    let installed = installed_of(&["x.one"]);
    let list = parse(&format!(
        r#"[{{"career":"f1_career_2026","label":"f1_career_2026","status":"present","mode":"driver_career",
             "addons":[{{"id":"x.one","why":"{OWN_WHY}"}},{{"id":"x.two","why":"career content x"}}]}}]"#
    ));
    assert!(!list[0].in_slots());
    assert!(!list[0].uses_addons());
    assert_eq!(stop_line(&list, &[], &installed), None);
    assert_eq!(play_line(&list, &[], &installed), None);
    assert!(needing(&list, "x.one").is_empty());
    assert!(needing(&list, "x.two").is_empty());
}

fn installed_of(ids: &[&str]) -> BTreeMap<String, AddonManifest> {
    ids.iter()
        .map(|id| ((*id).to_owned(), manifest(id, "1.0.0")))
        .collect()
}

fn my_team(saved: i64) -> Vec<Career> {
    parse(&format!(
        r#"[{{"career":"mt-1","profile":"p","label":"My Team","mode":"my_team","slot":"1","vehicle_raw":16387,"last_saved":{saved},"addons":[]}},
           {{"career":"mt-2","profile":"other","label":"Elsewhere","mode":"my_team","slot":"2","vehicle_raw":16387,"last_saved":{saved},"addons":[]}},
           {{"career":"mt-3","profile":"p","label":"Old","mode":"my_team","slot":"3","vehicle_raw":16387,"status":"missing","last_saved":{saved},"addons":[]}},
           {{"career":"mt-4","profile":"p","label":"Stock engine","mode":"my_team","slot":"4","vehicle_raw":16388,"last_saved":{saved},"addons":[]}}]"#
    ))
}

const LOST: &str = "[myteam] keep: My Team car (custom vehicle 3) on 'myteam_x_porsche' (engine 12, base car 3fff0000) has no vehicle row (its addon is off or gone); it loads as the stock 'myteam_mercedes'\n\
                    [myteam] save: My Team vehicle base 0 (stat id 9) uses vehicle 'myteam_x_audi', which is not in the game data (no entrants.vehicles row)\n\
                    [myteam] save: My Team vehicle base 1 (stat id 4) uses vehicle 'myteam_mercedes', whose row index reads as missing; it is not a known addon's vehicle\n\
                    [myteam] vehicle keep armed\n";

fn car_owners() -> BTreeMap<String, BTreeSet<String>> {
    [
        (
            "myteam_x_porsche".to_owned(),
            BTreeSet::from(["x.engines".to_owned()]),
        ),
        (
            "myteam_x_audi".to_owned(),
            BTreeSet::from(["x.engines".to_owned()]),
        ),
    ]
    .into()
}

#[test]
fn records_the_addon_of_a_my_team_car_the_core_found_without_its_row() {
    let lost = lost_cars(LOST);
    assert_eq!(
        lost.iter().cloned().collect::<Vec<_>>(),
        [
            (3, "myteam_x_porsche".to_owned()),
            (9, "myteam_x_audi".to_owned())
        ]
    );
    let installed = installed_of(&["x.engines"]);
    let mut list = my_team(2_000);
    let mine = |c: &Career| c.profile.as_deref() == Some("p");
    assert!(record_cars(&mut list, &mine, &lost, &car_owners(), &installed));
    assert_eq!(list[0].addons.len(), 1);
    assert_eq!(list[0].addons[0].id, "x.engines");
    assert_eq!(list[0].addons[0].version.as_deref(), Some("1.0.0"));
    assert_eq!(list[0].addons[0].why.as_deref(), Some(CAR_WHY));
    assert!(list[1].addons.is_empty(), "another profile's career");
    assert!(list[2].addons.is_empty(), "a career gone from the save");
    assert!(list[3].addons.is_empty(), "a My Team career on another car");
    assert!(!record_cars(&mut list, &mine, &lost, &car_owners(), &installed));
    let mut unowned = my_team(2_000);
    assert!(!record_cars(
        &mut unowned,
        &mine,
        &lost,
        &BTreeMap::new(),
        &installed
    ));
}

#[test]
fn a_profile_boots_only_the_careers_of_its_own_save() {
    let root = std::env::temp_dir().join(format!("swofty-careers-scope-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    for (name, launcher_saves) in [("a", true), ("b", true), ("s1", false), ("s2", false)] {
        let dir = root.join("profiles");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{name}.json")),
            format!(r#"{{"name":"{name}","launcher_saves":{launcher_saves}}}"#),
        )
        .ok();
    }
    let list = parse(
        r#"[{"career":"1","profile":"a"},{"career":"2","profile":"b"},{"career":"3"},
            {"career":"4","profile":"s1"},{"career":"5","profile":"s2"}]"#,
    );
    let ids = |p: &str| -> Vec<String> {
        in_save_of(&list, &root, p)
            .iter()
            .map(|c| c.career.clone())
            .collect()
    };
    assert_eq!(ids("a"), ["1", "3"]);
    assert_eq!(ids("b"), ["2", "3"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn only_the_careers_of_other_profiles_look_for_the_game_copies() {
    let root = std::env::temp_dir().join(format!("swofty-careers-looks-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let looks = || crate::game_folder::copies::LOOKS.with(std::cell::Cell::get);
    let before = looks();
    let own = parse(r#"[{"career":"1","profile":"a"},{"career":"3"}]"#);
    assert_eq!(in_save_of(&own, &root, "a").len(), 2);
    assert!(in_save_of(&[], &root, "a").is_empty());
    assert_eq!(looks(), before, "careers of this profile and of none");
    let others =
        parse(r#"[{"career":"1","profile":"a"},{"career":"2","profile":"b"},{"career":"4","profile":"b"}]"#);
    in_save_of(&others, &root, "a");
    assert_eq!(
        looks(),
        before + 2,
        "this profile's save and b's, each found once"
    );
}

fn copy_switch(name: &str, ea: bool) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-careers-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join(crate::GAME_EXE), b"exe").unwrap();
    match ea {
        true => {
            std::fs::create_dir_all(game.join("__Installer")).unwrap();
            std::fs::write(game.join("__Installer/installerdata.xml"), b"<game/>").unwrap();
        }
        false => std::fs::write(game.join("EAJavelinInstaller_installscript.vdf"), b"").unwrap(),
    }
    let pfx = root.join("pfx");
    std::fs::create_dir_all(pfx.join("drive_c/users/steamuser/AppData/Local/F1 25/savegame/ea/profile"))
        .unwrap();
    std::fs::write(pfx.join("user.reg"), b"").unwrap();
    let store = crate::store::Store::new(&root);
    let mut config = crate::session::Config::default();
    config.games.active = Some(game.clone());
    config.games.added = vec![game];
    config.launch.ea_prefix = Some(pfx);
    store.save_config(&config).unwrap();
    let dir = root.join("profiles");
    std::fs::create_dir_all(&dir).unwrap();
    for p in ["hi", "default"] {
        std::fs::write(
            dir.join(format!("{p}.json")),
            format!(r#"{{"name":"{p}","launcher_saves":true}}"#),
        )
        .unwrap();
    }
    root
}

fn classic_careers() -> Vec<Career> {
    parse(&format!(
        r#"[{{"career":"dc-2019","profile":"default","label":"Driver Career 2019","addons":[
               {{"id":"swofty.classic-seasons","version":"1.0.0","why":"{OWN_WHY}"}}],"slot":"19"}},
            {{"career":"dc-hi","profile":"hi","label":"Driver Career","addons":[]}}]"#
    ))
}

#[test]
fn an_ea_copy_profile_switch_keeps_each_profiles_own_save() {
    let root = copy_switch("ea-switch", true);
    let list = classic_careers();
    let installed = installed_of(&["swofty.classic-seasons"]);
    let scoped: Vec<Career> = in_save_of(&list, &root, "hi").into_iter().cloned().collect();
    let ids: Vec<&str> = scoped.iter().map(|c| c.career.as_str()).collect();
    assert_eq!(ids, ["dc-hi"]);
    assert_eq!(stop_line(&scoped, &[], &installed), None);
    if !cfg!(windows) {
        for (p, source, layout) in [
            ("hi", "launcher-ea.hi", crate::saves::vault::Layout::LauncherEa),
            (
                "default",
                "launcher-ea.default",
                crate::saves::vault::Layout::LauncherEa,
            ),
            (
                crate::profile::VANILLA,
                "ea-steamuser",
                crate::saves::vault::Layout::Ea,
            ),
        ] {
            let at = crate::saves::vault::Location::of_profile(&root, p).expect("the EA save");
            assert_eq!((at.source.as_str(), at.layout), (source, layout), "{p}");
        }
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_steam_copy_profile_switch_keeps_each_profiles_own_save() {
    let root = copy_switch("steam-switch", false);
    let list = classic_careers();
    let installed = installed_of(&["swofty.classic-seasons"]);
    let scoped: Vec<Career> = in_save_of(&list, &root, "hi").into_iter().cloned().collect();
    let ids: Vec<&str> = scoped.iter().map(|c| c.career.as_str()).collect();
    assert_eq!(ids, ["dc-hi"]);
    assert_eq!(stop_line(&scoped, &[], &installed), None);
    let at = crate::saves::vault::Location::of_profile(&root, "hi").expect("the launcher save");
    assert_eq!(
        (at.source.as_str(), at.layout),
        ("launcher-hi", crate::saves::vault::Layout::Launcher)
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn stops_play_when_the_addon_of_a_careers_my_team_car_is_off_or_gone() {
    let installed = installed_of(&["x.engines"]);
    let mut list = my_team(2_000);
    let on: Vec<String> = vec!["x.engines".into()];
    record_cars(
        &mut list,
        &|c| c.profile.as_deref() == Some("p"),
        &lost_cars(LOST),
        &car_owners(),
        &installed,
    );
    assert_eq!(stop_line(&list, &on, &installed), None);
    let stop = stop_line(&list, &[], &installed).unwrap();
    assert_eq!(
        stop,
        "1 career in your save uses a team, car or driver from N x.engines (off) (My Team). Without it, \
         that career may not load, or may load with a stock car in its place. Turn it back on and press \
         Play, or restore a save from before in Profiles > Saves."
    );
    assert!(!stop.contains("won't start"), "{stop}");
    assert_eq!(needing(&list, "x.engines").len(), 1);
    let gone = stop_line(&list, &on, &BTreeMap::new()).unwrap();
    assert!(gone.contains("x.engines (not installed)"), "{gone}");
    list[0]
        .extra
        .insert("status".into(), Value::String("missing".into()));
    assert_eq!(stop_line(&list, &[], &installed), None);
}

#[test]
fn an_addon_that_was_only_on_when_a_career_was_saved_raises_nothing() {
    let installed = installed_of(&["x.engines", "track.nyc"]);
    let list = parse(&format!(
        r#"[{{"career":"driver_career:a","label":"Driver Career","slot":"a","status":"present",
             "driver_raw":3,"team_raw":7,"vehicle_raw":9,"addons":[
               {{"id":"x.engines","why":"{ROWS_WHY}"}},
               {{"id":"track.nyc","why":"{TRACK_WHY}"}},
               {{"id":"track.gone","why":"{TRACK_WHY}"}}]}}]"#
    ));
    assert_eq!(stop_line(&list, &[], &installed), None);
    assert_eq!(play_line(&list, &[], &installed), None);
    assert!(missing(&list[0], &[], &installed).is_empty());
    assert!(!list[0].uses_addons());
    for addon in ["x.engines", "track.nyc", "track.gone"] {
        assert!(needing(&list, addon).is_empty(), "{addon}");
    }
}

#[test]
fn a_track_in_a_careers_calendar_is_needed_and_one_it_never_races_is_not() {
    let installed = installed_of(&["mod.sochi", "track.nyc"]);
    let list = parse(&format!(
        r#"[{{"career":"driver_career:a","label":"Progressive","slot":"a","status":"present","addons":[
               {{"id":"mod.sochi","why":"calendar round 4: track mod.sochi/sochi"}},
               {{"id":"track.nyc","why":"{TRACK_WHY}"}}]}}]"#
    ));
    let both: Vec<String> = vec!["mod.sochi".into(), "track.nyc".into()];
    assert_eq!(play_line(&list, &both, &installed), None);
    assert_eq!(play_line(&list, &both[..1], &installed), None);
    assert!(needing(&list, "track.nyc").is_empty());
    assert_eq!(
        play_line(&list, &both[1..], &installed).as_deref(),
        Some("1 career won't load in this profile: Progressive needs N mod.sochi (mod.sochi), which is off")
    );
    assert_eq!(needing(&list, "mod.sochi").len(), 1);
}

fn old_careers() -> Vec<Career> {
    parse(
        r#"{"version":1,"careers":[
          {"career":"driver_career:a","label":"","addons":[],"driver_raw":3,"team_raw":480,"vehicle_raw":217,"source":"slots","slot":"s","status":"present"},
          {"career":"driver_career:b","label":"","addons":[],"driver_raw":16388,"team_raw":501,"vehicle_raw":510,"source":"slots","slot":"s","status":"present"},
          {"career":"driver_career:c","label":"","addons":[],"driver_raw":102,"team_raw":155,"vehicle_raw":161,"source":"slots","slot":"s","status":"present"},
          {"career":"my_team:d","label":"","addons":[],"driver_raw":3,"team_raw":16387,"vehicle_raw":16390,"source":"slots","slot":"s","status":"present"},
          {"career":"driver_career:e","label":"","addons":[],"driver_raw":3,"team_raw":501,"vehicle_raw":510,"source":"slots","status":"deleted"}]}"#,
    )
}

fn stock_max() -> BTreeMap<String, i64> {
    [
        ("entrants.teams.team".to_owned(), 475),
        ("entrants.vehicles.vehicle".to_owned(), 211),
        ("entrants.drivers.driver".to_owned(), 300),
    ]
    .into()
}

#[test]
fn a_career_on_the_highest_stock_ids_raises_nothing() {
    let mut list = parse(
        r#"[{"career":"driver_career:s","addons":[],"driver_raw":300,"team_raw":475,"vehicle_raw":211,"status":"present"}]"#,
    );
    let owners: BTreeMap<(String, i64), BTreeSet<String>> = [(
        ("entrants.teams.team".to_owned(), 475),
        BTreeSet::from(["x.season".to_owned()]),
    )]
    .into();
    let installed = installed_of(&["x.season"]);
    assert!(!settle_rows(
        &mut list,
        &owners.into(),
        &stock_max(),
        &installed,
        None
    ));
    assert!(list[0].addons.is_empty());
    let off: BTreeSet<String> = ["x.season".to_owned()].into();
    assert_eq!(unowned_line(&list, &[], &installed, &off, &stock_max()), None);
    list[0].extra.insert("team_raw".into(), 476.into());
    assert!(unowned_line(&list, &[], &installed, &off, &stock_max()).is_some());
    assert_eq!(unowned_line(&list, &[], &installed, &off, &BTreeMap::new()), None);
}

fn id_owners() -> IdOwners {
    let one =
        |table: &str, id: i64, addon: &str| ((table.to_owned(), id), BTreeSet::from([addon.to_owned()]));
    [
        one("entrants.teams.team", 501, "x.season"),
        one("entrants.vehicles.vehicle", 510, "x.season"),
        one("entrants.vehicles.vehicle", 600, "x.other"),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>()
    .into()
}

#[test]
fn records_the_owners_of_the_added_ids_a_career_holds() {
    let installed = installed_of(&["x.season"]);
    let mut list = old_careers();
    assert!(settle_rows(
        &mut list,
        &id_owners(),
        &stock_max(),
        &installed,
        None
    ));
    assert_eq!(list[1].addons.len(), 1);
    assert_eq!(list[1].addons[0].id, "x.season");
    assert_eq!(list[1].addons[0].why.as_deref(), Some(OWN_WHY));
    assert!(list[0].addons.is_empty() && list[2].addons.is_empty() && list[3].addons.is_empty());
    assert!(list[4].addons.is_empty());
    assert!(!settle_rows(
        &mut list,
        &id_owners(),
        &stock_max(),
        &installed,
        None
    ));
    let stop = stop_line(&list, &[], &installed).unwrap();
    assert!(stop.contains("x.season"), "{stop}");
    assert_eq!(stop_line(&list, &["x.season".to_owned()], &installed), None);
}

#[test]
fn the_stock_database_is_read_only_for_careers_that_may_hold_added_ids() {
    let reads = std::cell::Cell::new(0);
    let read = || {
        reads.set(reads.get() + 1);
        stock_max()
    };
    assert!(stock_max_for(&[], read).is_empty());
    let mut gone = old_careers();
    gone.retain(Career::is_gone);
    assert!(stock_max_for(&gone, read).is_empty());
    let mut sentinel = parse(
        r#"[{"career":"my_team:s","addons":[],"driver_raw":16384,"team_raw":16387,"vehicle_raw":16390,"status":"present"}]"#,
    );
    assert!(stock_max_for(&sentinel, read).is_empty());
    assert_eq!(reads.get(), 0);
    sentinel.extend(old_careers());
    assert_eq!(stock_max_for(&sentinel, read), stock_max());
    assert_eq!(reads.get(), 1);
}

#[test]
fn an_addon_that_was_on_is_needed_only_when_it_owns_the_careers_rows() {
    let installed = installed_of(&["x.season", "x.recorded"]);
    let mut list = old_careers();
    for c in list.iter_mut() {
        for id in ["x.season", "x.recorded"] {
            c.addons.push(CareerAddon {
                id: id.into(),
                why: Some(ROWS_WHY.into()),
                ..CareerAddon::default()
            });
        }
    }
    assert_eq!(stop_line(&list, &[], &installed), None);
    assert!(!settle_rows(
        &mut list,
        &id_owners(),
        &BTreeMap::new(),
        &installed,
        None
    ));
    assert!(settle_rows(
        &mut list,
        &id_owners(),
        &stock_max(),
        &installed,
        None
    ));
    let why = |c: &Career, id: &str| c.addons.iter().find(|a| a.id == id).and_then(|a| a.why.clone());
    assert_eq!(why(&list[1], "x.season").as_deref(), Some(OWN_WHY));
    assert_eq!(why(&list[1], "x.recorded").as_deref(), Some(ROWS_WHY));
    assert_eq!(why(&list[0], "x.season").as_deref(), Some(ROWS_WHY));
    let stop = stop_line(&list, &[], &installed).unwrap();
    assert!(
        stop.contains("N x.season (off)") && !stop.contains("x.recorded"),
        "{stop}"
    );
    assert!(stop.contains("(driver_career:b)"), "{stop}");
    assert_eq!(needing(&list, "x.season").len(), 1);
    assert!(needing(&list, "x.recorded").is_empty());
    list[1].extra.insert("team_raw".into(), 7.into());
    list[1].extra.insert("vehicle_raw".into(), 9.into());
    assert!(settle_rows(
        &mut list,
        &id_owners(),
        &stock_max(),
        &installed,
        None
    ));
    assert_eq!(why(&list[1], "x.season"), None);
    assert_eq!(stop_line(&list, &[], &installed), None);
}

#[test]
fn warns_about_added_ids_no_installed_addon_owns() {
    let installed = installed_of(&["x.season"]);
    let owners: BTreeSet<String> = ["x.season".to_owned()].into();
    let mut list = old_careers();
    settle_rows(&mut list, &id_owners(), &stock_max(), &installed, None);
    let line = unowned_line(&list, &[], &installed, &owners, &stock_max()).unwrap();
    assert!(
        line.contains(
            "1 career in your save uses a team, car or driver that no installed addon provides \
             (driver_career:a). Turn on"
        ),
        "{line}"
    );
    assert!(line.contains("Addons that are off: N x.season"), "{line}");
    assert_eq!(
        unowned_line(&list[1..], &[], &installed, &owners, &stock_max()),
        None
    );
    assert_eq!(unowned_line(&[], &[], &installed, &owners, &stock_max()), None);
}

#[test]
fn a_career_on_a_registry_team_or_car_names_its_season_and_is_never_unowned() {
    let root = std::env::temp_dir().join(format!("swofty-careers-registry-ids-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let ids = crate::stdlib::reports::path(&root, crate::stdlib::reports::IDS);
    std::fs::create_dir_all(ids.parent().unwrap()).unwrap();
    std::fs::write(
        &ids,
        r#"{"version":1,"ids":{
          "team.identity":{"x.season/sw15_redbull":{"id":536,"since":"2026-11-13"}},
          "vehicle":{"x.season/redbull":{"id":545,"since":"2026-11-13"},
                     "y.season/williams":{"id":528,"since":"2026-11-13"}}},"careers":{}}"#,
    )
    .unwrap();
    let owners = crate::row_ids::id_owners(&root);
    std::fs::remove_dir_all(&root).ok();
    let stock_max: BTreeMap<String, i64> = [
        ("entrants.teams.team".to_owned(), 499),
        ("entrants.vehicles.vehicle".to_owned(), 508),
        ("entrants.drivers.driver".to_owned(), 254),
    ]
    .into();
    let installed = installed_of(&["x.season", "y.season"]);
    let none = BTreeSet::new();
    let on = ["x.season".to_owned(), "y.season".to_owned()];
    let mut list = parse(
        r#"[{"career":"driver_career:a","addons":[],"driver_raw":2,"team_raw":536,"vehicle_raw":545,"source":"slots","slot":"s","status":"present"},
            {"career":"driver_career:b","addons":[],"driver_raw":62,"team_raw":479,"vehicle_raw":528,"source":"slots","slot":"s","status":"present"},
            {"career":"driver_career:c","addons":[],"driver_raw":2,"team_raw":760,"vehicle_raw":700,"source":"slots","slot":"s","status":"present"}]"#,
    );
    assert!(settle_rows(&mut list, &owners, &stock_max, &installed, None));
    assert_eq!(unowned_line(&list[..2], &on, &installed, &none, &stock_max), None);
    assert_eq!(stop_line(&list, &on, &installed), None);
    let stop = stop_line(&list, &on[1..], &installed).unwrap();
    assert!(
        stop.contains("N x.season (off)") && stop.contains("(driver_career:a)"),
        "{stop}"
    );
    assert!(!stop.contains("y.season"), "{stop}");
    assert_eq!(
        unowned_line(&list[..2], &on[1..], &installed, &none, &stock_max),
        None
    );
    let gone = stop_line(&list, &on, &installed_of(&["x.season"])).unwrap();
    assert!(
        gone.contains("y.season (not installed)") && gone.contains("(driver_career:b)"),
        "{gone}"
    );
    let line = unowned_line(&list, &on, &installed, &none, &stock_max).unwrap();
    assert!(
        line.starts_with(
            "1 career in your save uses a team, car or driver that no installed addon provides \
             (driver_career:c). Turn on"
        ),
        "{line}"
    );
    assert!(!line.contains("crash"), "{line}");
}

#[test]
fn an_id_a_removed_addon_held_before_is_not_a_careers_need() {
    let root = std::env::temp_dir().join(format!("swofty-careers-former-ids-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let ids = crate::stdlib::reports::path(&root, crate::stdlib::reports::IDS);
    std::fs::create_dir_all(ids.parent().unwrap()).unwrap();
    std::fs::write(
        crate::row_ids::path(&root),
        r#"{"version":2,"rows":{
          "old.season/entrants.teams.team/cs17_mclaren":{"id":528,"from":"next"},
          "off.season/entrants.teams.team/t":{"id":530,"from":"next"},
          "old.season/entrants.teams.team/alone":{"id":560,"from":"next"}}}"#,
    )
    .unwrap();
    std::fs::write(
        &ids,
        r#"{"version":1,"ids":{
          "team.identity":{"new.season/sw17_sauber":{"id":528,"since":"2026-11-09"},
                           "new.season/sw17_haas":{"id":530,"since":"2026-11-09"}},
          "driver.identity":{"new.season/sw17_wehrlein":{"id":300,"since":"2026-11-09"}},
          "vehicle":{"new.season/sauber":{"id":607,"since":"2026-11-09"},
                     "old.season/mclaren":{"id":829,"since":"2026-11-04","pin":607},
                     "old.season/ferrari":{"id":825,"since":"2026-11-04","pin":522},
                     "old.season/haas":{"id":610,"since":"2026-11-04"}}},
          "careers":{
            "progressive:new1":{"vehicle":{"old.season/ferrari":825,"old.season/mclaren":829}},
            "driver_career:pinned":{"vehicle":{"new.season/sauber":610}}}}"#,
    )
    .unwrap();
    let owners = crate::row_ids::id_owners(&root);
    std::fs::remove_dir_all(&root).ok();
    let stock_max: BTreeMap<String, i64> = [
        ("entrants.teams.team".to_owned(), 499),
        ("entrants.vehicles.vehicle".to_owned(), 508),
        ("entrants.drivers.driver".to_owned(), 254),
    ]
    .into();
    let installed = installed_of(&["new.season", "off.season"]);
    let on = ["new.season".to_owned()];
    let none = BTreeSet::new();
    let former = r#"{"id":"old.season","why":"the career's team, car or driver"}"#;
    let mut list = parse(&format!(
        r#"[{{"career":"driver_career:a","addons":[{former}],"driver_raw":300,"team_raw":528,"vehicle_raw":607,"source":"slots","slot":"a","status":"present"}},
            {{"career":"progressive:new1","addons":[],"driver_raw":300,"team_raw":528,"vehicle_raw":607,"source":"slots","slot":"b","status":"present"}},
            {{"career":"driver_career:c","addons":[],"driver_raw":2,"team_raw":530,"vehicle_raw":9,"source":"slots","slot":"c","status":"present"}},
            {{"career":"driver_career:pinned","addons":[],"driver_raw":2,"team_raw":7,"vehicle_raw":610,"source":"slots","slot":"d","status":"present"}},
            {{"career":"driver_career:e","addons":[],"driver_raw":2,"team_raw":560,"vehicle_raw":9,"source":"slots","slot":"e","status":"present"}}]"#,
    ));
    let stale = stop_line(&list, &on, &installed).unwrap();
    assert!(stale.contains("old.season (not installed)"), "{stale}");

    let played: BTreeSet<String> = list.iter().map(|c| c.career.clone()).collect();
    assert!(settle_rows(
        &mut list,
        &owners,
        &stock_max,
        &installed,
        Some((&played, &on))
    ));
    let ids = |c: &Career| c.addons.iter().map(|a| a.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&list[0]), ["new.season"]);
    assert_eq!(ids(&list[1]), ["new.season"]);
    assert_eq!(ids(&list[2]), ["new.season"]);
    assert_eq!(ids(&list[3]), ["new.season"]);
    assert_eq!(ids(&list[4]), ["old.season"]);
    assert_eq!(stop_line(&list[..4], &on, &installed), None);
    assert_eq!(unowned_line(&list, &on, &installed, &none, &stock_max), None);
    assert!(
        needing(&list, "old.season")
            .iter()
            .all(|c| c.career == "driver_career:e")
    );
    assert!(needing(&list, "off.season").is_empty());
    assert!(list[..4].iter().all(Career::uses_addons));
    let alone = stop_line(&list, &on, &installed).unwrap();
    assert!(
        alone.contains("old.season (not installed)") && alone.contains("(driver_career:e)"),
        "{alone}"
    );

    let mut elsewhere = parse(
        r#"[{"career":"driver_career:c","addons":[],"driver_raw":2,"team_raw":530,"vehicle_raw":9,"source":"slots","slot":"c","status":"present"}]"#,
    );
    settle_rows(&mut elsewhere, &owners, &stock_max, &installed, None);
    assert_eq!(ids(&elsewhere[0]), ["new.season", "off.season"]);
    let off = stop_line(&elsewhere, &on, &installed).unwrap();
    assert!(off.contains("off.season (off)"), "{off}");
    let other = ["off.season".to_owned()];
    settle_rows(
        &mut elsewhere,
        &owners,
        &stock_max,
        &installed,
        Some((&played, &other)),
    );
    assert_eq!(ids(&elsewhere[0]), ["off.season"]);
}

#[test]
fn a_content_addon_is_not_a_play_stop_and_sync_keeps_the_recorded_rows() {
    let list = parse(&format!("[{RECORD}]"));
    assert_eq!(stop_line(&list, &[], &BTreeMap::new()), None);
    let root = std::env::temp_dir().join(format!("swofty-careers-rows-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (data, game) = (root.join("data"), root.join("game"));
    let mut mine = my_team(2_000);
    mine[0].addons.push(CareerAddon {
        id: "x.engines".into(),
        why: Some(ROWS_WHY.into()),
        ..CareerAddon::default()
    });
    std::fs::create_dir_all(data.join("careers")).unwrap();
    std::fs::write(
        data_path(&data),
        serde_json::json!({ "version": 1, "careers": mine }).to_string(),
    )
    .unwrap();
    std::fs::create_dir_all(game.join("swofty/careers")).unwrap();
    std::fs::write(
        game.join(GAME_PATH),
        r#"[{"career":"mt-1","profile":"p","label":"My Team","addons":[]}]"#,
    )
    .unwrap();
    let merged = sync(&data, Some(&game), None).unwrap();
    let kept = merged.iter().find(|c| c.career == "mt-1").unwrap();
    assert_eq!(kept.addons.len(), 1);
    assert_eq!(kept.addons[0].id, "x.engines");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_launchers_copy_keeps_careers_and_their_profile() {
    let root = std::env::temp_dir().join(format!("swofty-careers-{}", std::process::id()));
    let (data, game) = (root.join("data"), root.join("game"));
    std::fs::create_dir_all(game.join("swofty/careers")).unwrap();
    std::fs::write(game.join(GAME_PATH), format!("[{RECORD}]")).unwrap();
    let first = sync(&data, Some(&game), Some("progressive")).unwrap();
    assert_eq!(first.len(), 1);
    let later = RECORD
        .replace("\"profile\": \"progressive\",", "")
        .replace("season 1", "season 2");
    let other = RECORD
        .replace("6f3e9a", "77aa01")
        .replace("progressive\",", "vanilla-ish\",");
    std::fs::write(game.join(GAME_PATH), format!("[{later}, {other}]")).unwrap();
    let merged = sync(&data, Some(&game), Some("other")).unwrap();
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].profile.as_deref(), Some("progressive"));
    assert!(merged[0].label.ends_with("season 2"));
    std::fs::remove_file(game.join(GAME_PATH)).unwrap();
    assert_eq!(sync(&data, Some(&game), None).unwrap().len(), 2);
    assert_eq!(sync(&data, None, None).unwrap().len(), 2);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_career_without_a_profile_gets_the_sessions_and_the_ids_survive_a_restore() {
    let root = std::env::temp_dir().join(format!("swofty-careers-keep-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (data, game) = (root.join("data"), root.join("game"));
    let unstamped = RECORD.replace("\"profile\": \"progressive\",", "");
    std::fs::create_dir_all(game.join("swofty/careers")).unwrap();
    std::fs::write(game.join(GAME_PATH), format!("[{unstamped}]")).unwrap();
    let synced = sync(&data, Some(&game), Some("tails")).unwrap();
    assert_eq!(synced[0].profile.as_deref(), Some("tails"));
    let ids = crate::stdlib::reports::legacy_path(&game, crate::stdlib::reports::IDS);
    std::fs::create_dir_all(ids.parent().unwrap()).unwrap();
    std::fs::write(&ids, "{\"track\":{\"mod.sochi/sochi\":18}}").unwrap();
    keep(&data, &game).unwrap();
    std::fs::remove_dir_all(game.join("swofty")).unwrap();
    assert_eq!(read(&data_path(&data)).len(), 1);
    assert_eq!(
        std::fs::read_to_string(kept_ids(&data)).unwrap(),
        "{\"track\":{\"mod.sochi/sochi\":18}}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn migrating_moves_the_old_files_once_and_the_newer_ids_win() {
    let root = std::env::temp_dir().join(format!("swofty-careers-migrate-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (data, game) = (root.join("d\u{e4}ta \u{30b9}"), root.join("game"));
    let put = |p: &Path, t: &str| {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    };
    use crate::stdlib::reports::{IDS, ROOM, legacy_path, path};
    put(&legacy_path(&game, IDS), "{\"old\":1}");
    put(&legacy_path(&game, ROOM), "{\"presize\":\"applied\"}");
    put(&game.join(GAME_PATH), &format!("[{RECORD}]"));
    put(&path(&data, ROOM), "{\"presize\":\"kept\"}");
    let moved = migrate(&data, &game).unwrap();
    assert_eq!(moved, ["career index", "ids.json"]);
    assert_eq!(std::fs::read_to_string(path(&data, IDS)).unwrap(), "{\"old\":1}");
    assert_eq!(
        std::fs::read_to_string(path(&data, ROOM)).unwrap(),
        "{\"presize\":\"kept\"}"
    );
    assert_eq!(read(&data_path(&data)).len(), 1);
    for gone in [
        legacy_path(&game, IDS),
        legacy_path(&game, ROOM),
        game.join(GAME_PATH),
    ] {
        assert!(!gone.exists(), "{}", gone.display());
    }
    assert!(migrate(&data, &game).unwrap().is_empty());
    put(&legacy_path(&game, IDS), "{\"stale\":1}");
    std::fs::File::options()
        .write(true)
        .open(legacy_path(&game, IDS))
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    assert!(migrate(&data, &game).unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(path(&data, IDS)).unwrap(), "{\"old\":1}");
    assert!(!legacy_path(&game, IDS).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn forgetting_takes_only_a_gone_careers_record_off_the_index() {
    let root = std::env::temp_dir().join(format!("swofty-careers-forget-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let record = |id: &str, status: &str| {
        format!(
            r#"{{"career":"{id}","profile":"p","label":"L {id}","status":"{status}","slot":"{id}","addons":[{{"id":"x.one","why":"{OWN_WHY}"}}]}}"#
        )
    };
    let index = data_path(&root);
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(
        &index,
        format!(
            r#"{{"version":1,"careers":[{},{},{}]}}"#,
            record("a", "present"),
            record("b", "deleted"),
            record("c", "missing")
        ),
    )
    .unwrap();
    let kept = root.join("careers/data/b/x.one.json");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    std::fs::write(&kept, r#"{"version":1,"data":{}}"#).unwrap();
    let ids = || read(&index).into_iter().map(|c| c.career).collect::<Vec<_>>();

    assert!(!forget(&root, "a").unwrap());
    assert!(!forget(&root, "nope").unwrap());
    assert_eq!(ids(), ["a", "b", "c"]);
    assert!(forget(&root, "b").unwrap());
    assert_eq!(ids(), ["a", "c"]);
    assert!(!forget(&root, "b").unwrap());
    assert!(forget(&root, "c").unwrap());
    assert_eq!(ids(), ["a"]);
    let a = &read(&index)[0];
    assert_eq!(a.label, "L a");
    assert_eq!(a.field("slot"), Some("a"));
    assert_eq!(a.addons[0].id, "x.one");
    assert!(kept.is_file(), "the career's addon data stays");

    std::fs::write(
        &index,
        format!(
            r#"{{"version":1,"careers":[{},{}]}}"#,
            record("a", "present"),
            record("b", "present")
        ),
    )
    .unwrap();
    assert_eq!(ids(), ["a", "b"]);
    assert!(read(&index).iter().all(|c| !c.is_gone()));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn resume_needs_label_an_addon_mode_with_its_addon() {
    let list = parse(
        r#"[{"career":"a","slot":"1","mode":"driver_career","addons":[{"id":"x.mode","why":"career content g"}]},
          {"career":"b","slot":"2","mode":"driver_career_1990","addons":[{"id":"x.other"},{"id":"x.mode","why":"career content g"}]},
          {"career":"c","slot":"3","mode":"driver_career_1990","addons":[{"id":"x.other"}]},
          {"career":"d","slot":"4","addons":[]}]"#,
    );
    let mut m = manifest("x.mode", "1.0.0");
    m.name = "F1 1990".into();
    let installed: BTreeMap<String, AddonManifest> = [
        ("x.mode".to_owned(), m),
        ("x.other".to_owned(), manifest("x.other", "1.0.0")),
    ]
    .into_iter()
    .collect();
    let out = resume_needs(&list, &[], &installed, None);
    assert_eq!(out[0].mode_label, "");
    assert_eq!(out[1].mode_label, "F1 1990");
    assert_eq!(out[2].mode_label, installed["x.other"].name);
    assert_eq!(out[3].mode_label, "");
}

#[test]
fn resume_needs_say_what_is_off_missing_or_another_version() {
    let list = parse(&format!(
        r#"[{{"career":"c1","slot":"67cdf241e227f68c","addons":[
            {{"id":"a.on","version":"1.0.0","need":"^1.0.0"}},
            {{"id":"a.off","version":"1.0.0"}},
            {{"id":"a.old","version":"1.0.0","need":"^1.0.0"}},
            {{"id":"a.gone","version":"1.0.0"}},
            {{"id":"a.guess","why":"{ROWS_WHY}"}}]}},
          {{"career":"c2","slot":"7","addons":[]}},
          {{"career":"c3","addons":[{{"id":"a.on"}}]}},
          {{"career":"c4","slot":"9","status":"deleted","addons":[{{"id":"a.on"}}]}}]"#
    ));
    let installed: BTreeMap<String, AddonManifest> = [
        ("a.on", manifest("a.on", "1.2.0")),
        ("a.off", manifest("a.off", "1.0.0")),
        ("a.old", manifest("a.old", "2.0.0")),
        ("a.guess", manifest("a.guess", "1.0.0")),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
    let order = vec!["a.on".to_owned(), "a.old".to_owned()];
    let out = resume_needs(&list, &order, &installed, None);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].key, "67cdf241e227f68c");
    let by = |id: &str| out[0].addons.iter().find(|a| a.id == id).unwrap();
    assert!(by("a.on").on && by("a.on").at_version);
    assert!(!by("a.off").on && by("a.off").at_version);
    assert!(by("a.old").on && !by("a.old").at_version);
    assert_eq!(
        (by("a.old").made_with.as_str(), by("a.old").wants.as_str()),
        ("1.0.0", "^1.0.0")
    );
    assert_eq!(
        (by("a.off").made_with.as_str(), by("a.off").wants.as_str()),
        ("1.0.0", "")
    );
    assert!(!by("a.gone").on && by("a.gone").name == "a.gone");
    assert_eq!(out[0].addons.len(), 4);
    assert_eq!(out[1].key, "7");
    assert!(out[1].addons.is_empty());
}

#[test]
fn resume_status_comes_from_the_played_profiles_career_with_the_same_slot_key() {
    let list = parse(
        r#"[{"career":"a","profile":"A","slot":"5","addons":[{"id":"x.gone","version":"1.0.0"}]},
          {"career":"b","profile":"B","slot":"5","addons":[]}]"#,
    );
    let installed = BTreeMap::new();
    let a = resume_needs(&of_profile(&list, "A"), &[], &installed, None);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].career, "a");
    assert_eq!(a[0].addons.len(), 1);
    let b = resume_needs(&of_profile(&list, "B"), &[], &installed, None);
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].career, "b");
    assert!(b[0].addons.is_empty());
}

#[test]
fn a_career_with_no_profile_counts_only_where_no_career_of_the_profile_has_its_slot() {
    let list = parse(
        r#"[{"career":"own","profile":"A","slot":"5","addons":[]},
          {"career":"old5","slot":"5","addons":[{"id":"x.gone"}]},
          {"career":"old6","slot":"6","addons":[]},
          {"career":"other","profile":"B","slot":"7","addons":[]}]"#,
    );
    let out: Vec<String> = of_profile(&list, "A").into_iter().map(|c| c.career).collect();
    assert_eq!(out, ["own", "old6"]);
}

fn ids(teams: &[i64], vehicles: &[i64], drivers: &[i64]) -> RowSet {
    RowSet {
        teams: teams.iter().copied().collect(),
        vehicles: vehicles.iter().copied().collect(),
        drivers: drivers.iter().copied().collect(),
        aliases: Default::default(),
    }
}

fn named(id: &str, name: &str) -> (String, AddonManifest) {
    let mut m = manifest(id, "1.0.0");
    m.name = name.into();
    (id.to_owned(), m)
}

#[test]
fn a_career_whose_team_and_car_rows_the_build_lacks_says_which_addon_made_them() {
    let list = parse(
        r#"[{"career":"driver_career_1990:a","slot":"7","mode":"driver_career","content":"f1_career_1990",
              "team_raw":503,"vehicle_raw":510,"driver_raw":61,
              "addons":[{"id":"x.f1990","why":"career content f1_career_1990"}]},
            {"career":"driver_career:b","slot":"6","mode":"driver_career","team_raw":513,"vehicle_raw":520,"driver_raw":240,
              "addons":[{"id":"x.f1990","why":"the career's team, car or driver"}]},
            {"career":"driver_career:c","slot":"5","mode":"driver_career","team_raw":646,"vehicle_raw":695,"driver_raw":105,
              "addons":[{"id":"x.f1990","why":"the career's team, car or driver"}]},
            {"career":"driver_career:d","slot":"4","mode":"driver_career","team_raw":513,"vehicle_raw":16383,"driver_raw":16386,"addons":[]},
            {"career":"my_team:e","slot":"3","mode":"my_team","team_raw":16384,"vehicle_raw":16384,"driver_raw":197,"addons":[]}]"#,
    );
    let installed: BTreeMap<String, AddonManifest> = [named("x.f1990", "F1 1990")].into_iter().collect();
    let rows = ids(&[646, 7], &[695, 9], &[105, 61, 197]);
    let out = resume_needs(&list, &[], &installed, Some(&rows));
    let gap = |i: usize| out[i].missing_rows.clone();
    assert_eq!(
        gap(0),
        Some(MissingRows {
            addon: "F1 1990".into(),
            tables: vec!["team".into(), "vehicle".into()]
        })
    );
    assert_eq!(gap(0).unwrap().text(), "Mismatched rows from F1 1990");
    assert_eq!(gap(1).unwrap().addon, "F1 1990");
    assert_eq!(gap(1).unwrap().tables, ["team", "vehicle", "driver"]);
    assert_eq!(gap(2), None, "rows the build has are not missing");
    let unknown = gap(3).unwrap();
    assert_eq!(unknown.addon, "");
    assert_eq!(unknown.text(), "Mismatched rows: team");
    assert_eq!(
        gap(4),
        None,
        "the game's own placeholder ids are never rows to find"
    );
    assert!(
        resume_needs(&list, &[], &installed, None)
            .iter()
            .all(|n| n.missing_rows.is_none())
    );
}

#[test]
fn a_career_driving_a_car_the_core_adds_from_the_registry_is_not_missing_rows() {
    let list = parse(
        r#"[{"career":"driver_career:667c9aad208c7d9c","slot":"667c9aad208c7d9c","mode":"driver_career",
              "team_raw":476,"vehicle_raw":509,"driver_raw":16386,
              "addons":[{"id":"swofty.classic-seasons-2016","version":"0.8.2","why":"the career's team, car or driver"}]}]"#,
    );
    let installed: BTreeMap<String, AddonManifest> =
        [named("swofty.classic-seasons-2016", "F1 2016 GP/TT Addon")]
            .into_iter()
            .collect();
    let mut rows = ids(&[476, 500], &[3], &[]);
    let gap = resume_needs(&list, &[], &installed, Some(&rows))[0]
        .missing_rows
        .clone()
        .unwrap();
    assert_eq!(
        gap.tables,
        ["vehicle"],
        "the database file alone lacks the registry's car"
    );
    assert_eq!(gap.text(), "Mismatched rows from F1 2016 GP/TT Addon");
    rows.add(&[("entrants.vehicles.vehicle".to_owned(), [509].into())].into());
    assert_eq!(
        resume_needs(&list, &[], &installed, Some(&rows))[0].missing_rows,
        None
    );
    rows.add(&[("entrants.engines.Engine".to_owned(), [600].into())].into());
    assert!(!rows.vehicles.contains(&600) && !rows.teams.contains(&600));
}

#[test]
fn a_content_career_is_labelled_by_its_addon_though_its_mode_is_driver_career() {
    let list = parse(
        r#"[{"career":"driver_career_1990:a","slot":"7","mode":"driver_career","content":"f1_career_1990",
              "addons":[{"id":"x.f1990","why":"career content f1_career_1990"}]}]"#,
    );
    let installed: BTreeMap<String, AddonManifest> = [named("x.f1990", "F1 1990")].into_iter().collect();
    assert_eq!(
        resume_needs(&list, &[], &installed, None)[0].mode_label,
        "F1 1990"
    );
}

#[test]
fn a_saved_id_the_build_aliases_to_a_row_is_not_missing() {
    let list = parse(
        r#"[{"career":"driver_career_1990:a","slot":"7","mode":"driver_career","content":"f1_career_1990",
              "team_raw":503,"vehicle_raw":510,"driver_raw":61,
              "addons":[{"id":"x.f1990","why":"career content f1_career_1990"}]}]"#,
    );
    let installed: BTreeMap<String, AddonManifest> = [named("x.f1990", "F1 1990")].into_iter().collect();
    let mut rows = ids(&[644], &[], &[61]);
    rows.aliases.tables.insert("teams".into(), [(503, 644)].into());
    rows.aliases.tables.insert("vehicles".into(), [(510, 700)].into());
    assert_eq!(rows.resolve("team_raw", 503), Some(644));
    assert_eq!(rows.resolve("team_raw", 644), Some(644));
    assert_eq!(
        rows.resolve("vehicle_raw", 510),
        None,
        "an alias to a row the build lacks resolves to nothing"
    );
    let gap = resume_needs(&list, &[], &installed, Some(&rows))[0]
        .missing_rows
        .clone()
        .unwrap();
    assert_eq!(gap.tables, ["vehicle"]);
    rows.vehicles.insert(700);
    assert_eq!(
        resume_needs(&list, &[], &installed, Some(&rows))[0].missing_rows,
        None
    );
}

fn installed_at(versions: &[(&str, &str)]) -> BTreeMap<String, AddonManifest> {
    versions
        .iter()
        .map(|(id, v)| ((*id).to_owned(), manifest(id, v)))
        .collect()
}

#[test]
fn a_patch_update_of_the_recorded_version_is_not_a_mismatch() {
    let recorded = [
        ("a.patch", "1.0.5", "1.0.6"),
        ("a.same", "1.0.5", "1.0.5"),
        ("a.short", "1.0", "1.0.3"),
        ("a.zero", "0.7.1", "0.7.3"),
        ("a.older", "1.0.5", "1.0.4"),
        ("a.pre", "1.0.5", "1.0.5-beta.1"),
        ("a.minor", "1.0.5", "1.1.0"),
        ("a.major", "1.0.5", "2.0.5"),
        ("a.zero_minor", "0.7.1", "0.8.0"),
        ("a.off", "1.0.5", "1.0.6"),
        ("a.gone", "1.0.5", ""),
    ];
    let addons: Vec<String> = recorded
        .iter()
        .map(|(id, was, _)| format!(r#"{{"id":"{id}","version":"{was}","why":"{OWN_WHY}"}}"#))
        .collect();
    let list = parse(&format!(
        r#"[{{"career":"c1","slot":"1","addons":[{}]}}]"#,
        addons.join(",")
    ));
    let installed = installed_at(
        &recorded
            .iter()
            .filter(|(_, _, now)| !now.is_empty())
            .map(|(id, _, now)| (*id, *now))
            .collect::<Vec<_>>(),
    );
    let order: Vec<String> = recorded
        .iter()
        .map(|(id, _, _)| (*id).to_owned())
        .filter(|id| id != "a.off")
        .collect();
    let out = resume_needs(&list, &order, &installed, None);
    let by = |id: &str| out[0].addons.iter().find(|a| a.id == id).unwrap().clone();
    for id in ["a.patch", "a.same", "a.short", "a.zero"] {
        assert!(by(id).on && by(id).at_version, "{id} loads at its version");
    }
    for id in ["a.older", "a.pre", "a.minor", "a.major", "a.zero_minor"] {
        assert!(by(id).on && !by(id).at_version, "{id} is at another version");
    }
    assert_eq!(
        (by("a.patch").made_with.as_str(), by("a.patch").version.as_str()),
        ("1.0.5", "1.0.6")
    );
    let gone = by("a.gone");
    assert!(
        !gone.on && !gone.at_version,
        "an addon that isn't installed is missing"
    );
    assert!(!by("a.off").on, "an addon that is off is missing");
}

fn saved_careers() -> Vec<Career> {
    let own = |id: &str| format!(r#"{{"id":"{id}","version":"1.0.5","why":"{OWN_WHY}"}}"#);
    parse(&format!(
        r#"[{{"career":"played","profile":"p","slot":"1","listed_session":"1795144582000","last_played":1795167539,
              "addons":[{},{},{},{{"id":"x.guess","version":"1.0.5","why":"{ROWS_WHY}"}}]}},
           {{"career":"listed","profile":"p","slot":"2","listed_session":"1795170570918","last_played":1795167539,
              "addons":[{}]}},
           {{"career":"elsewhere","profile":"other","slot":"3","listed_session":"1795144582000","last_played":1795167539,
              "addons":[{}]}},
           {{"career":"gone","profile":"p","slot":"4","status":"deleted","listed_session":"1795144582000",
              "last_played":1795167539,"addons":[{}]}}]"#,
        own("x.2017"),
        own("x.minor"),
        own("x.off"),
        own("x.2017"),
        own("x.2017"),
        own("x.2017"),
    ))
}

#[test]
fn a_career_saved_at_a_patch_update_records_it_and_lists_clean() {
    let mut list = saved_careers();
    let installed = installed_at(&[
        ("x.2017", "1.0.6"),
        ("x.minor", "1.1.0"),
        ("x.off", "1.0.6"),
        ("x.guess", "1.0.6"),
    ]);
    let on: Vec<String> = ["x.2017", "x.minor", "x.guess"].map(str::to_owned).into();
    let mine = |c: &Career| c.profile.as_deref() == Some("p");
    assert!(refresh_versions(&mut list, &mine, &on, &installed));
    let version = |c: usize, id: &str| {
        list[c]
            .addons
            .iter()
            .find(|a| a.id == id)
            .and_then(|a| a.version.clone())
            .unwrap()
    };
    assert_eq!(version(0, "x.2017"), "1.0.6");
    assert_eq!(
        version(0, "x.minor"),
        "1.0.5",
        "another minor version is not recorded"
    );
    assert_eq!(
        version(0, "x.off"),
        "1.0.5",
        "an addon that was off is not recorded"
    );
    assert_eq!(
        version(0, "x.guess"),
        "1.0.5",
        "an addon that was only on is not a need"
    );
    assert_eq!(version(1, "x.2017"), "1.0.5", "a career the session did not save");
    assert_eq!(version(2, "x.2017"), "1.0.5", "another profile's career");
    assert_eq!(version(3, "x.2017"), "1.0.5", "a career deleted in game");
    assert!(!refresh_versions(&mut list, &mine, &on, &installed));

    let needs = resume_needs(&list[..2], &on, &installed, None);
    let played = |id: &str| needs[0].addons.iter().find(|a| a.id == id).unwrap().clone();
    assert!(played("x.2017").at_version);
    assert_eq!(played("x.2017").made_with, "1.0.6");
    assert!(
        !played("x.minor").at_version,
        "another minor version still asks first"
    );

    let back = installed_at(&[("x.2017", "1.0.5"), ("x.minor", "1.1.0")]);
    let needs = resume_needs(&list[..2], &on, &back, None);
    assert!(
        !needs[0].addons[0].at_version,
        "the career was saved with 1.0.6, so 1.0.5 is older"
    );
    assert!(needs[1].addons[0].at_version);
}

#[test]
fn a_session_records_the_patch_update_a_saved_career_was_saved_with() {
    let root = std::env::temp_dir().join(format!("swofty-careers-patch-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let addon = root.join("addons").join("x.2017");
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(
        addon.join("swofty.json"),
        r#"{ "schema_version": 1, "id": "x.2017", "name": "X 2017", "version": "1.0.6", "author": "a",
             "compat": { "game": "f1_25" } }"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("profiles")).unwrap();
    std::fs::write(
        root.join("profiles").join("p.json"),
        r#"{"schema_version":1,"name":"p","addons":[{"id":"x.2017","enabled":true}]}"#,
    )
    .unwrap();
    let index = data_path(&root);
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(
        &index,
        format!(
            r#"{{"version":1,"careers":[{{"career":"played","profile":"p","slot":"1","listed_session":"1795144582000",
                "last_played":1795167539,"addons":[{{"id":"x.2017","version":"1.0.5","why":"{OWN_WHY}"}}]}}]}}"#
        ),
    )
    .unwrap();
    record_session(&root, "p", "", BTreeMap::new).unwrap();
    let list = read(&index);
    assert_eq!(list[0].addons[0].version.as_deref(), Some("1.0.6"));
    std::fs::remove_dir_all(&root).ok();
}
