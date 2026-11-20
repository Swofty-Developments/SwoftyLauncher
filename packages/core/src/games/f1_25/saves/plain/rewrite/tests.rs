use std::path::PathBuf;

use super::*;
use crate::saves::bag::{Bag, List, Value, decode, encode};

const GM: &str = "gm_driver_career_1990";
const SLOT: &str = "driver_career_1990_SaveSlotGameModeComponent";
const STOCK: &str = "options_options";

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plain-rewrite-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn maps() -> IdMaps {
    IdMaps {
        teams: (502u16..=514).map(|t| (t, t + 142)).collect(),
        vehicles: BTreeMap::from([(509, 693)]),
        drivers: BTreeMap::from([(235, 362)]),
    }
}

fn bag(entries: Vec<(&str, Value)>) -> Value {
    let mut b = Bag::new();
    for (k, v) in entries {
        b.push(k, v);
    }
    Value::Bag(b)
}

fn slot(id: u64, team: u16, driver: u16) -> Value {
    bag(vec![
        ("slot_id", Value::U64(id)),
        ("info_team", Value::U16(team)),
        ("info_driver", Value::U16(driver)),
        ("info_standings_team_0", Value::U16(team)),
    ])
}

fn slot_component(active: u64, slots: Vec<Value>) -> Bag {
    let mut list = List::new();
    list.items = slots;
    let mut data = Bag::new();
    data.push("slots", Value::List(list));
    data.push("active_slot_id", Value::U64(active));
    let mut root = Bag::new();
    root.push("data", Value::Bag(data));
    root
}

fn career(team: u16, driver: u16) -> Bag {
    let mut list = List::new();
    list.items = vec![bag(vec![
        ("teamStatId", Value::U16(team)),
        ("driverStatId", Value::U16(driver)),
    ])];
    let mut data = Bag::new();
    data.push("driverContracts", Value::List(list));
    let mut root = Bag::new();
    root.push("data", Value::Bag(data));
    root
}

fn options() -> Bag {
    let mut root = Bag::new();
    root.push("info_team", Value::U16(503));
    root
}

fn fnv(name: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in name.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn store(d: &std::path::Path, parts: &[(&str, Bag)], event: Event, written: i64) -> Plain {
    let p = Plain::at(d.join("plain"));
    put_generation(&p, 5, parts, event, written);
    p
}

fn put_generation(p: &Plain, n: u64, parts: &[(&str, Bag)], event: Event, written: i64) {
    let gd = p.generation_dir(n);
    std::fs::create_dir_all(gd.join("containers")).unwrap();
    let mut containers = Vec::new();
    for (name, tree) in parts {
        let bytes = encode(tree).unwrap();
        std::fs::write(gd.join("containers").join(format!("{}.bag", fnv(name))), &bytes).unwrap();
        containers.push(Container {
            name: (*name).to_owned(),
            fnv64: fnv(name),
            type_id: 12,
            bytes: bytes.len() as u64,
            sha256: sha_hex(&bytes),
            registered: true,
            source: Source::Game,
            slots: Vec::new(),
            pair: None,
        });
    }
    let m = Manifest {
        schema: super::super::SCHEMA,
        generation: n,
        event,
        written_at: written,
        core_version: "1.10.0".into(),
        exe_sha256: "e".into(),
        profile: "f1-1990".into(),
        session: Some("s".into()),
        loaded_addons: Vec::new(),
        containers,
    };
    std::fs::write(gd.join("manifest.json"), serde_json::to_vec(&m).unwrap()).unwrap();
    std::fs::write(p.root.join("CURRENT"), n.to_string()).unwrap();
}

fn old_parts() -> Vec<(&'static str, Bag)> {
    vec![
        (GM, career(503, 235)),
        (
            SLOT,
            slot_component(7, vec![slot(6, 513, 18), slot(7, 503, 235), slot(8, 646, 61)]),
        ),
        (STOCK, options()),
    ]
}

fn wrote(o: Outcome) -> Rewrite {
    match o {
        Outcome::Wrote(r) => r,
        other => panic!("expected a rewrite, got {other:?}"),
    }
}

fn container_bag(p: &Plain, n: u64, name: &str) -> Bag {
    let m = p.manifest(n).unwrap();
    let c = m.container(name).unwrap();
    decode(&std::fs::read(p.bag_path(n, c)).unwrap()).unwrap()
}

#[test]
fn rewrites_per_slot_and_leaves_the_clean_slot_and_stock_containers_alone() {
    let d = dir("slots");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    let r = wrote(p.rewrite_for_aliases(&maps(), Some(900), 2000).unwrap());
    assert_eq!((r.from, r.generation), (5, 6));
    assert_eq!(p.current(), Some(6));
    let names: Vec<_> = r.requests.iter().map(|q| q.container.as_str()).collect();
    assert_eq!(names, [SLOT, GM]);
    assert!(r.requests.iter().all(|q| q.replace && q.generation == 6));
    assert_eq!(p.pending().len(), 2);

    let lines: Vec<String> = r.careers.iter().map(CareerMapped::line).collect();
    assert_eq!(
        lines,
        [
            "[saves] career 6 (driver_career_1990): mapped 2 team, 0 vehicle, 0 driver id(s) to this build's rows",
            "[saves] career 7 (driver_career_1990): mapped 3 team, 0 vehicle, 2 driver id(s) to this build's rows",
        ]
    );

    let out = container_bag(&p, 6, SLOT);
    let Some(Value::Bag(data)) = out.get("data") else {
        panic!()
    };
    let Some(Value::List(slots)) = data.get("slots") else {
        panic!()
    };
    let team = |i: usize| match &slots.items[i] {
        Value::Bag(b) => b.get("info_team").cloned().unwrap(),
        _ => panic!(),
    };
    assert_eq!(
        [team(0), team(1), team(2)],
        [Value::U16(655), Value::U16(645), Value::U16(646)]
    );
    assert_eq!(container_bag(&p, 6, STOCK), options());
    let m = p.manifest(6).unwrap();
    assert_eq!(m.container(GM).unwrap().source, Source::Injected);
    assert_eq!(m.container(STOCK).unwrap().source, Source::Game);
    assert!(p.manifest(5).is_some());
    assert!(p.pins().contains_key(&6));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_save_made_with_this_builds_ids_is_idempotent() {
    let d = dir("idem");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    wrote(p.rewrite_for_aliases(&maps(), None, 2000).unwrap());
    assert_eq!(
        p.rewrite_for_aliases(&maps(), None, 2001).unwrap(),
        Outcome::Nothing
    );
    assert_eq!(p.current(), Some(6));

    let clean = dir("clean");
    let parts = vec![
        (GM, career(645, 362)),
        (SLOT, slot_component(7, vec![slot(7, 645, 362), slot(8, 646, 61)])),
    ];
    let q = store(&clean, &parts, Event::Save, 1000);
    assert_eq!(
        q.rewrite_for_aliases(&maps(), None, 2000).unwrap(),
        Outcome::Nothing
    );
    assert_eq!(q.current(), Some(5));
    assert!(q.pending().is_empty());
    std::fs::remove_dir_all(&d).ok();
    std::fs::remove_dir_all(&clean).ok();
}

#[test]
fn a_save_the_game_wrote_after_the_generation_is_left_alone() {
    let d = dir("stale");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    let o = p
        .rewrite_for_aliases(&maps(), Some(1000 + STALE_AFTER + 1), 2000)
        .unwrap();
    assert_eq!(o, Outcome::Stale { generation: 5 });
    assert_eq!(p.current(), Some(5));
    assert!(p.pending().is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn no_aliases_no_generation() {
    let d = dir("none");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    assert_eq!(
        p.rewrite_for_aliases(&IdMaps::default(), None, 2000).unwrap(),
        Outcome::Nothing
    );
    assert_eq!(p.current(), Some(5));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn requests_carry_the_hash_of_the_container_they_were_made_from() {
    let d = dir("base");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    let before = p.manifest(5).unwrap();
    let r = wrote(p.rewrite_for_aliases(&maps(), None, 2000).unwrap());
    for q in &r.requests {
        assert_eq!(
            q.base_sha256.as_deref(),
            Some(before.container(&q.container).unwrap().sha256.as_str())
        );
        let now = p.manifest(6).unwrap();
        assert_ne!(
            now.container(&q.container).unwrap().sha256,
            before.container(&q.container).unwrap().sha256
        );
    }
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_generation_checked_once_is_not_decoded_again() {
    let d = dir("marker");
    let clean = vec![
        (GM, career(645, 362)),
        (SLOT, slot_component(7, vec![slot(7, 645, 362)])),
    ];
    let p = store(&d, &clean, Event::Save, 1000);
    assert_eq!(
        p.rewrite_for_aliases(&maps(), None, 2000).unwrap(),
        Outcome::Nothing
    );
    assert!(p.root.join("remap-checked.json").is_file());
    let gm = p.bag_path(5, p.manifest(5).unwrap().container(GM).unwrap());
    let len = std::fs::metadata(&gm).unwrap().len() as usize;
    std::fs::write(&gm, vec![0xffu8; len]).unwrap();
    assert_eq!(
        p.rewrite_for_aliases(&maps(), None, 2001).unwrap(),
        Outcome::Nothing
    );
    let mut other = maps();
    other.drivers.insert(1, 2);
    assert!(p.rewrite_for_aliases(&other, None, 2002).is_err());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn chained_aliases_write_nothing() {
    let d = dir("chain");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    let mut chained = maps();
    chained.teams.insert(644, 700);
    let o = p.rewrite_for_aliases(&chained, None, 2000).unwrap();
    assert!(matches!(o, Outcome::Refused(why) if why.contains("teams aliases chain")));
    assert_eq!(p.current(), Some(5));
    assert!(p.pending().is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_dry_run_reports_the_careers_and_writes_nothing() {
    let d = dir("dry");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    let aliases = crate::row_ids::IdAliases {
        version: 1,
        tables: BTreeMap::from([("teams".to_owned(), (502i64..=514).map(|t| (t, t + 142)).collect())]),
    };
    let lines = dry_run(&p.root, &aliases).unwrap();
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("career 7") && lines[1].contains("mapped 3 team, 0 vehicle, 0 driver"));
    assert_eq!(p.current(), Some(5));
    assert!(!p.root.join("remap-checked.json").exists() && p.pending().is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_mode_with_only_earlier_driver_ids_is_left_alone() {
    let d = dir("stockmode");
    let parts = vec![
        ("gm_driver_career_26", career(478, 235)),
        (
            "driver_career_26_SaveSlotGameModeComponent",
            slot_component(1, vec![slot(1, 478, 235)]),
        ),
        (GM, career(503, 235)),
        (SLOT, slot_component(7, vec![slot(7, 503, 235)])),
    ];
    let p = store(&d, &parts, Event::Save, 1000);
    let r = wrote(p.rewrite_for_aliases(&maps(), None, 2000).unwrap());
    let names: Vec<_> = r.requests.iter().map(|q| q.container.as_str()).collect();
    assert_eq!(names, [SLOT, GM]);
    assert!(r.careers.iter().all(|c| c.mode == "driver_career_1990"));
    let lines = dry_run(
        &p.root,
        &crate::row_ids::IdAliases {
            version: 1,
            tables: BTreeMap::from([
                ("teams".to_owned(), BTreeMap::from([(503i64, 645i64)])),
                ("drivers".to_owned(), BTreeMap::from([(235, 362)])),
            ]),
        },
    )
    .unwrap();
    assert!(
        lines.iter().any(
            |l| l.contains("mode driver_career_26: earlier driver or vehicle ids but no earlier team id")
        )
    );
    std::fs::remove_dir_all(&d).ok();
}

fn aliases() -> crate::row_ids::IdAliases {
    crate::row_ids::IdAliases {
        version: 1,
        tables: BTreeMap::from([
            ("teams".to_owned(), (502i64..=514).map(|t| (t, t + 142)).collect()),
            ("vehicles".to_owned(), BTreeMap::from([(509, 693)])),
            ("drivers".to_owned(), BTreeMap::from([(235, 362)])),
        ]),
    }
}

#[test]
fn generations_after_a_rewrite_are_in_the_current_numbering_and_never_mapped_again() {
    let d = dir("later");
    let p = store(&d, &old_parts(), Event::Save, 1000);
    wrote(p.rewrite_for_aliases(&maps(), None, 2000).unwrap());
    assert_eq!(p.pending().len(), 2);
    for n in [7, 8] {
        put_generation(&p, n, &old_parts(), Event::Save, 3000 + n as i64);
        assert_eq!(
            p.rewrite_for_aliases(&maps(), None, 4000).unwrap(),
            Outcome::Nothing
        );
        assert_eq!(p.current(), Some(n));
    }
    assert_eq!(p.pending().len(), 2);
    assert!(p.manifest(9).is_none());
    assert!(prepare_lines(&d, &aliases()).is_empty());
    assert_eq!(p.current(), Some(8));
    let mut other = aliases();
    other.tables.get_mut("drivers").unwrap().insert(1, 2);
    assert_eq!(prepare_lines(&d, &other).len(), 2);
    assert_eq!(p.current(), Some(9));
    std::fs::remove_dir_all(&d).ok();
}

fn prepare_lines(d: &std::path::Path, a: &crate::row_ids::IdAliases) -> Vec<String> {
    let plain_root = d.join("plain");
    let maps = IdMaps::from_aliases(a);
    match Plain::at(plain_root)
        .rewrite_for_aliases(&maps, None, 5000)
        .unwrap()
    {
        Outcome::Wrote(r) => r.careers.iter().map(CareerMapped::line).collect(),
        _ => Vec::new(),
    }
}

fn stream() -> Vec<u8> {
    let mut containers = Bag::new();
    containers.push(GM, Value::Bag(career(503, 235)));
    containers.push(
        SLOT,
        Value::Bag(slot_component(
            7,
            vec![slot(6, 513, 18), slot(7, 503, 235), slot(8, 646, 61)],
        )),
    );
    containers.push(STOCK, Value::Bag(options()));
    containers.push(
        "driver_career_26_SaveSlotGameModeComponent",
        Value::Bag(slot_component(1, vec![slot(1, 478, 235)])),
    );
    encode(&save_root(containers)).unwrap()
}

fn save_root(containers: Bag) -> Bag {
    let mut save = Bag::new();
    save.push("uid", Value::U64(1));
    save.push("containers", Value::Bag(containers));
    let mut root = Bag::new();
    root.push("save_game", Value::Bag(save));
    root
}

fn save_file(d: &std::path::Path) -> PathBuf {
    let iv = [7u8; 16];
    let path = d.join("savegame@profile#NXDSMWW.BWW");
    std::fs::write(&path, crate::saves::codec::encode_data(&stream(), iv, 3).unwrap()).unwrap();
    path
}

fn file_stream(path: &std::path::Path) -> Bag {
    decode(
        &crate::saves::codec::decode_data(&std::fs::read(path).unwrap())
            .unwrap()
            .stream,
    )
    .unwrap()
}

#[test]
fn a_store_with_no_generation_maps_the_launcher_save_file_once() {
    let d = dir("file");
    let path = save_file(&d);
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    let iv = crate::saves::codec::decode_data(&std::fs::read(&path).unwrap())
        .unwrap()
        .iv;
    let save = SaveFile {
        path: &path,
        snapshot: Some("20261120-snap"),
    };
    let lines = prepare_play(&d, "p", true, &aliases(), 9000, Some(save));
    assert_eq!(
        lines,
        [
            "[saves] the plain store has no generation yet; mapping the launcher save file",
            "[saves] mode driver_career_26 career 1: earlier driver or vehicle ids but no earlier team id; left alone",
            "[saves] career 6 (driver_career_1990): mapped 2 team, 0 vehicle, 0 driver id(s) in the save file (the vault snapshot before it is 20261120-snap)",
            "[saves] career 7 (driver_career_1990): mapped 3 team, 0 vehicle, 2 driver id(s) in the save file (the vault snapshot before it is 20261120-snap)",
        ]
    );
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(
        crate::saves::codec::decode_data(&std::fs::read(&path).unwrap())
            .unwrap()
            .iv,
        iv
    );
    assert!(!path.with_extension("remap.tmp").exists());
    let mut expected = decode(&stream()).unwrap();
    let mut mapped = 0;
    for (name, b) in crate::saves::remap::save_containers_mut(&mut expected) {
        if name == GM || name == SLOT {
            mapped += crate::saves::remap::remap_container(b, &IdMaps::from_aliases(&aliases())).total();
        }
    }
    assert!(mapped > 0);
    assert_eq!(file_stream(&path), expected);

    let root = crate::saves::plain::root_of_profile(&d, "p");
    assert!(root.join("remap-checked.json").is_file());
    let bytes = std::fs::read(&path).unwrap();
    assert!(prepare_play(&d, "p", true, &aliases(), 9001, Some(save)).is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);

    let plain = Plain::at(root);
    put_generation(&plain, 1, &old_parts(), Event::Save, 9500);
    assert!(prepare_play(&d, "p", true, &aliases(), 9600, Some(save)).is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_save_the_game_outran_the_store_maps_the_file_and_marks_the_store() {
    let d = dir("outran");
    let path = save_file(&d);
    let plain = Plain::at(crate::saves::plain::root_of_profile(&d, "p"));
    put_generation(&plain, 5, &old_parts(), Event::Save, 1);
    let save = SaveFile {
        path: &path,
        snapshot: None,
    };
    let lines = prepare_play(&d, "p", true, &aliases(), 9000, Some(save));
    assert!(lines[0].starts_with(
        "[saves] careers in plain generation 5 hold earlier row ids but the game saved after it"
    ));
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("[saves] career 7 (driver_career_1990): mapped 3 team"))
    );
    assert_eq!(plain.current(), Some(5));
    assert!(plain.already_checked(5, &IdMaps::from_aliases(&aliases())));
    assert!(prepare_play(&d, "p", true, &aliases(), 9001, Some(save)).is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_unreadable_file_is_left_alone_and_tried_again() {
    let d = dir("nofile");
    let path = d.join("missing.BWW");
    let save = SaveFile {
        path: &path,
        snapshot: None,
    };
    let lines = prepare_play(&d, "p", true, &aliases(), 9000, Some(save));
    assert!(lines[1].starts_with("[saves] launcher save file left alone: "));
    assert!(
        !crate::saves::plain::root_of_profile(&d, "p")
            .join("remap-checked.json")
            .exists()
    );
    let none = prepare_play(&d, "p", true, &aliases(), 9000, None);
    assert_eq!(
        none,
        ["[saves] the plain store has no generation yet; not mapped"]
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_save_file_with_this_builds_ids_is_not_written() {
    let d = dir("cleanfile");
    let path = d.join("savegame@profile#NXDSMWW.BWW");
    let mut containers = Bag::new();
    containers.push(GM, Value::Bag(career(645, 362)));
    let root = save_root(containers);
    std::fs::write(
        &path,
        crate::saves::codec::encode_data(&encode(&root).unwrap(), [1u8; 16], 3).unwrap(),
    )
    .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let save = SaveFile {
        path: &path,
        snapshot: None,
    };
    let lines = prepare_play(&d, "p", true, &aliases(), 9000, Some(save));
    assert!(lines.last().unwrap().contains("left alone"));
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(
        crate::saves::plain::root_of_profile(&d, "p")
            .join("remap-checked.json")
            .is_file()
    );
    std::fs::remove_dir_all(&d).ok();
}
