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
    let gd = p.generation_dir(5);
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
        generation: 5,
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
    std::fs::write(p.root.join("CURRENT"), "5").unwrap();
    p
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
    assert!(p.pins().get(&6).is_some());
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
