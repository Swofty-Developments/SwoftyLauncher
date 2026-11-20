use super::*;
use crate::games::f1_25::saves::bag::{List, encode};

fn maps() -> IdMaps {
    IdMaps {
        teams: BTreeMap::from([(502, 644), (503, 645)]),
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

fn list(items: Vec<Value>) -> Value {
    let mut l = List::new();
    l.items = items;
    Value::List(l)
}

fn career() -> Value {
    bag(vec![(
        "data",
        bag(vec![
            (
                "slots",
                list(vec![bag(vec![
                    ("info_team", Value::U16(503)),
                    ("info_vehicle", Value::U16(509)),
                    ("info_driver", Value::U16(235)),
                    ("info_standings_team_7", Value::U16(502)),
                    ("info_standings_driver_7", Value::U16(235)),
                    ("info_standings_points_7", Value::I32(502)),
                ])]),
            ),
            (
                "driverContracts",
                list(vec![bag(vec![
                    ("driverStatId", Value::U16(235)),
                    ("currentContracts", bag(vec![("teamStatId", Value::U16(502))])),
                    (
                        "previousContracts",
                        list(vec![bag(vec![("teamStatId", Value::U16(503))])]),
                    ),
                ])]),
            ),
            (
                "team_rnd_heads",
                list(vec![bag(vec![("packed_team_stat_id", Value::I32(503))])]),
            ),
            (
                "press_interviews_harsh_answers_given",
                list(vec![bag(vec![("statId", Value::U16(235))])]),
            ),
            ("other", list(vec![bag(vec![("statId", Value::U16(235))])])),
            ("email_team", bag(vec![("team_id", Value::U16(0x3fff))])),
            ("score", Value::U32(502)),
            ("teamStatId_wide", Value::U16(502)),
        ]),
    )])
}

fn root(name: &str, container: Value) -> Bag {
    let mut containers = Bag::new();
    containers.push(name, container);
    let mut save = Bag::new();
    save.push("uid", Value::U64(1));
    save.push("containers", Value::Bag(containers));
    let mut root = Bag::new();
    root.push("save_game", Value::Bag(save));
    root
}

fn find<'a>(v: &'a Value, path: &[&str]) -> &'a Value {
    let mut cur = v;
    for key in path {
        cur = match cur {
            Value::Bag(b) => b.get(key).unwrap(),
            Value::List(l) => &l.items[key.parse::<usize>().unwrap()],
            _ => panic!("leaf"),
        };
    }
    cur
}

#[test]
fn maps_every_holder_by_kind() {
    let mut c = career();
    let Value::Bag(b) = &mut c else { unreachable!() };
    let done = remap_container(b, &maps());
    assert_eq!(
        done,
        Remapped {
            teams: 5,
            vehicles: 1,
            drivers: 4
        }
    );
    assert_eq!(find(&c, &["data", "slots", "0", "info_team"]), &Value::U16(645));
    assert_eq!(
        find(&c, &["data", "slots", "0", "info_vehicle"]),
        &Value::U16(693)
    );
    assert_eq!(find(&c, &["data", "slots", "0", "info_driver"]), &Value::U16(362));
    assert_eq!(
        find(&c, &["data", "slots", "0", "info_standings_team_7"]),
        &Value::U16(644)
    );
    assert_eq!(
        find(&c, &["data", "slots", "0", "info_standings_driver_7"]),
        &Value::U16(362)
    );
    assert_eq!(
        find(
            &c,
            &["data", "driverContracts", "0", "currentContracts", "teamStatId"]
        ),
        &Value::U16(644)
    );
    assert_eq!(
        find(
            &c,
            &[
                "data",
                "driverContracts",
                "0",
                "previousContracts",
                "0",
                "teamStatId"
            ]
        ),
        &Value::U16(645)
    );
    assert_eq!(
        find(&c, &["data", "team_rnd_heads", "0", "packed_team_stat_id"]),
        &Value::I32(645)
    );
    assert_eq!(
        find(
            &c,
            &["data", "press_interviews_harsh_answers_given", "0", "statId"]
        ),
        &Value::U16(362)
    );
}

#[test]
fn leaves_everything_else_alone() {
    let mut c = career();
    let Value::Bag(b) = &mut c else { unreachable!() };
    remap_container(b, &maps());
    assert_eq!(
        find(&c, &["data", "slots", "0", "info_standings_points_7"]),
        &Value::I32(502)
    );
    assert_eq!(find(&c, &["data", "other", "0", "statId"]), &Value::U16(235));
    assert_eq!(find(&c, &["data", "email_team", "team_id"]), &Value::U16(0x3fff));
    assert_eq!(find(&c, &["data", "score"]), &Value::U32(502));
    assert_eq!(find(&c, &["data", "teamStatId_wide"]), &Value::U16(502));
}

#[test]
fn only_career_containers_are_touched() {
    let mut r = root("online_custom_SaveSlotGameModeComponent", career());
    assert_eq!(remap_root(&mut r, &maps()).total(), 0);
    assert_eq!(r, root("online_custom_SaveSlotGameModeComponent", career()));
    for name in [
        "gm_driver_career_1990",
        "driver_career_1990_SaveSlotGameModeComponent",
    ] {
        let mut r = root(name, career());
        assert_eq!(remap_root(&mut r, &maps()).total(), 10);
    }
}

#[test]
fn a_second_pass_changes_nothing() {
    let mut r = root("gm_driver_career_1990", career());
    remap_root(&mut r, &maps());
    let once = r.clone();
    assert_eq!(remap_root(&mut r, &maps()).total(), 0);
    assert_eq!(r, once);
}

#[test]
fn the_stream_round_trips_untouched_and_changes_only_the_ids() {
    let original = root("gm_driver_career_1990", career());
    let bytes = encode(&original).unwrap();
    assert_eq!(remap_stream(&bytes, &IdMaps::default()).unwrap(), None);
    let unrelated = encode(&root("options_options", career())).unwrap();
    assert_eq!(remap_stream(&unrelated, &maps()).unwrap(), None);
    let (out, done) = remap_stream(&bytes, &maps()).unwrap().unwrap();
    assert_eq!(done.total(), 10);
    assert_eq!(out.len(), bytes.len());
    let differing = out.iter().zip(&bytes).filter(|(a, b)| a != b).count();
    assert!(differing > 0 && differing <= 10 * 2);
    let mut expected = original;
    remap_root(&mut expected, &maps());
    assert_eq!(super::super::bag::decode(&out).unwrap(), expected);
}

#[test]
fn maps_come_from_the_alias_file_tables() {
    let aliases = IdAliases {
        version: 1,
        tables: BTreeMap::from([
            (
                "teams".to_owned(),
                BTreeMap::from([(502, 644), (7, 7), (-1, 3), (9, 70000)]),
            ),
            ("vehicles".to_owned(), BTreeMap::from([(509, 693)])),
        ]),
    };
    let m = IdMaps::from_aliases(&aliases);
    assert_eq!(m.teams, BTreeMap::from([(502, 644)]));
    assert_eq!(m.vehicles, BTreeMap::from([(509, 693)]));
    assert!(m.drivers.is_empty());
}

#[test]
fn maps_the_driver_and_team_id_buffers_lane_by_lane() {
    use crate::games::f1_25::saves::bag::Hex;
    let lanes = |ids: &[u16]| Value::Bffr(Hex(ids.iter().flat_map(|i| i.to_le_bytes()).collect()));
    let mut c = bag(vec![
        ("packed_driver_stat_ids", lanes(&[235, 0x3fff, 61, 235])),
        ("packed_other_driver_stat_ids", lanes(&[0x3fff, 0x3fff])),
        ("packed_other_team_stat_ids", lanes(&[502, 0x3fff])),
        ("activity_stat_id_buffer", lanes(&[235, 502])),
    ]);
    let Value::Bag(b) = &mut c else { unreachable!() };
    let done = remap_container(b, &maps());
    assert_eq!(
        done,
        Remapped {
            teams: 1,
            vehicles: 0,
            drivers: 2
        }
    );
    assert_eq!(
        find(&c, &["packed_driver_stat_ids"]),
        &lanes(&[362, 0x3fff, 61, 362])
    );
    assert_eq!(find(&c, &["packed_other_team_stat_ids"]), &lanes(&[644, 0x3fff]));
    assert_eq!(find(&c, &["activity_stat_id_buffer"]), &lanes(&[235, 502]));
}

#[test]
fn the_active_slot_and_per_slot_counts() {
    let mut slots = Bag::new();
    let mut data = Bag::new();
    let mut list = List::new();
    list.items = vec![
        bag(vec![("slot_id", Value::U64(6)), ("info_team", Value::U16(502))]),
        bag(vec![("slot_id", Value::U64(8)), ("info_team", Value::U16(646))]),
    ];
    data.push("slots", Value::List(list));
    data.push("active_slot_id", Value::U64(6));
    slots.push("data", Value::Bag(data));
    assert_eq!(active_slot(&slots), Some(6));
    let per = remap_slots(&mut slots, &maps());
    assert_eq!(
        per,
        vec![(
            6,
            Remapped {
                teams: 1,
                vehicles: 0,
                drivers: 0
            }
        )]
    );
}

#[test]
fn a_slot_without_an_earlier_team_id_keeps_its_driver_ids() {
    let mut slots = Bag::new();
    let mut data = Bag::new();
    let mut list = List::new();
    list.items = vec![
        bag(vec![
            ("slot_id", Value::U64(7)),
            ("info_team", Value::U16(502)),
            ("info_driver", Value::U16(235)),
        ]),
        bag(vec![
            ("slot_id", Value::U64(8)),
            ("info_team", Value::U16(646)),
            ("info_driver", Value::U16(235)),
        ]),
    ];
    data.push("slots", Value::List(list));
    slots.push("data", Value::Bag(data));
    let (per, held) = remap_slots_gated(&mut slots, &maps());
    assert_eq!(
        per,
        vec![(
            7,
            Remapped {
                teams: 1,
                vehicles: 0,
                drivers: 1
            }
        )]
    );
    assert_eq!(held, vec![8]);
    let Some(Value::Bag(data)) = slots.get("data") else {
        panic!()
    };
    let Some(Value::List(list)) = data.get("slots") else {
        panic!()
    };
    let driver = |i: usize| match &list.items[i] {
        Value::Bag(b) => b.get("info_driver").cloned(),
        _ => None,
    };
    assert_eq!(driver(0), Some(Value::U16(362)));
    assert_eq!(driver(1), Some(Value::U16(235)));
}

#[test]
fn chained_aliases_are_refused() {
    assert_eq!(maps().refusal(), None);
    let mut chained = maps();
    chained.teams.insert(644, 700);
    assert!(chained.refusal().unwrap().contains("teams aliases chain"));
    let live = BTreeMap::from([("drivers", std::collections::BTreeSet::from([235u16]))]);
    assert!(
        maps()
            .refusal_with_live(&live)
            .unwrap()
            .contains("drivers alias for earlier id 235")
    );
    assert_eq!(maps().refusal_with_live(&BTreeMap::new()), None);
}
