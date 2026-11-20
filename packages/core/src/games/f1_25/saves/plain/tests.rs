use std::path::PathBuf;

use super::fixture::*;

const GM: &str = "gm_driver_career_progressive";
const SLOT: &str = "driver_career_progressive_SaveSlotGameModeComponent";
use super::*;
use crate::saves::vault::restore::Running;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plain-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_generation_newer_than_current_or_torn_is_ignored() {
    let d = dir("torn");
    let p = Plain::at(d.join("plain"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
    write(&p, &career_gen(2, "save", Some("s1"), &[("0x0a", "Jacob")]));
    write(&p, &career_gen(3, "save", Some("s1"), &[("0x0a", "Jacob")]));
    current(&p, 2);
    assert_eq!(p.newest().unwrap().generation, 2);
    std::fs::remove_file(p.bag_path(2, &p.manifest(2).unwrap().containers[2])).unwrap();
    assert_eq!(p.newest().unwrap().generation, 1);
    std::fs::write(p.generation_dir(1).join("manifest.json"), "{ not json").unwrap();
    assert!(p.newest().is_none());
    assert!(p.generations().is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn no_current_means_no_store() {
    let d = dir("nocurrent");
    let p = Plain::at(d.join("plain"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
    assert!(p.newest().is_none());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn careers_come_from_the_newest_generation_slots() {
    let d = dir("careers");
    let p = Plain::at(d.join("plain"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Old")]));
    write(
        &p,
        &career_gen(2, "save", Some("s1"), &[("0x0a", "Jacob"), ("0x0b", "Ada")]),
    );
    current(&p, 2);
    let rows = p.careers();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .any(|r| r.mode == "driver_career_progressive" && r.slot.name == "Ada")
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_load_that_lacks_a_container_the_last_save_had_is_a_loss() {
    let d = dir("loss");
    let p = Plain::at(d.join("plain"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
    write(
        &p,
        &Gen {
            n: 2,
            event: "load",
            session: None,
            containers: pair("driver_career", "game", vec![("0x01", "Stock")]),
        },
    );
    current(&p, 2);
    let loss = p.loss(&[]).unwrap();
    assert_eq!((loss.good, loss.newest), (1, 2));
    assert_eq!(loss.modes(), ["driver_career_progressive"]);
    assert_eq!(
        loss.containers(),
        [GM, SLOT],
        "both halves of the mode go back together"
    );
    assert_eq!(loss.names(), ["Jacob, season 2, round 5"]);
    assert!(
        p.loss(&[
            PutBack {
                name: GM.into(),
                careers: 1,
                generation: 1
            },
            PutBack {
                name: SLOT.into(),
                careers: 0,
                generation: 1
            }
        ])
        .is_none()
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_put_back_container_and_a_deleted_career_are_not_lost() {
    let d = dir("putback");
    let p = Plain::at(d.join("plain"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
    write(&p, &career_gen(2, "load", None, &[("0x0a", "Jacob")]));
    current(&p, 2);
    assert!(p.loss(&[]).is_none());

    let mut injected = career_gen(3, "load", None, &[("0x0a", "Jacob")]);
    injected.containers[2].1 = "injected";
    injected.containers[3].1 = "injected";
    write(&p, &injected);
    current(&p, 3);
    assert!(p.loss(&[]).is_none());

    write(&p, &career_gen(4, "save", Some("s2"), &[]));
    current(&p, 4);
    assert!(
        p.loss(&[]).is_none(),
        "a later save without a career is a deletion"
    );
    write(&p, &career_gen(5, "load", None, &[]));
    current(&p, 5);
    assert!(p.loss(&[]).is_none(), "the deletion sticks");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_career_missing_from_a_container_the_load_still_has_is_named() {
    let d = dir("career");
    let p = Plain::at(d.join("plain"));
    write(
        &p,
        &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob"), ("0x0b", "Ada")]),
    );
    write(&p, &career_gen(2, "load", None, &[("0x0a", "Jacob")]));
    current(&p, 2);
    let loss = p.loss(&[]).unwrap();
    assert!(!loss.items[0].whole);
    assert_eq!(loss.items[0].careers[0].guid, "0x0b");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_log_lines_of_a_put_back_parse() {
    let log = "x\n[saves] put back driver_career_progressive: 2 career(s) from generation 41\nz";
    assert_eq!(
        put_backs(log),
        [PutBack {
            name: "driver_career_progressive".into(),
            careers: 2,
            generation: 41
        }]
    );
}

#[test]
fn pruning_keeps_the_newest_pinned_and_each_sessions_last_save() {
    let d = dir("prune");
    let p = Plain::at(d.join("plain"));
    for n in 1..=9 {
        let (event, session) = match n {
            1..=2 => ("save", Some("sa")),
            3..=5 => ("save", Some("sb")),
            _ => ("load", None),
        };
        write(&p, &career_gen(n, event, session, &[("0x0a", "Jacob")]));
    }
    current(&p, 9);
    p.pin(1, "restore").unwrap();
    let running = Running {
        game: true,
        ..Running::default()
    };
    assert!(
        p.prune(&d, Keep { newest: 2 }, running)
            .unwrap()
            .skipped
            .is_some()
    );
    let out = p.prune(&d, Keep { newest: 2 }, Running::default()).unwrap();
    let left: Vec<u64> = p.generations().iter().map(|m| m.generation).collect();
    assert_eq!(left, [9, 8, 5, 2, 1]);
    assert_eq!(out.removed.len(), 4);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_restore_request_pins_the_generation_and_replaces_the_pending_one() {
    let d = dir("inject");
    let p = Plain::at(d.join("plain"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
    current(&p, 1);
    let r = p.restore_mode(1, "driver_career_progressive", 50).unwrap();
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|x| x.replace && x.generation == 1));
    assert_eq!(
        r.iter().map(|x| x.container.as_str()).collect::<Vec<_>>(),
        [GM, SLOT]
    );
    assert_eq!(p.pending(), r);
    assert_eq!(p.pins().get(&1).unwrap(), &["restore"]);
    assert!(p.restore_mode(1, "nope", 51).is_err());
    assert!(p.restore_mode(7, "driver_career", 51).is_err());
    let half = p.restore_mode(1, SLOT, 52).unwrap();
    assert_eq!(half.len(), 2, "naming either half restores the mode");
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(p.root.join("inject.json")).unwrap()).unwrap();
    assert_eq!(v["schema"], 1);
    assert_eq!(v["requests"][0]["replace"], true);
    assert_eq!(v["requests"][0]["generation"], 1);
    assert_eq!(v["requests"][1]["container"], SLOT);
    std::fs::write(
        p.root.join("injected.json"),
        r#"{"schema":1,"results":[{"id":"r1","ok":true,"detail":"d"}]}"#,
    )
    .unwrap();
    assert_eq!(p.answers()[0].detail, "d");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn requests_a_load_took_and_never_answered_are_withdrawn_once() {
    let d = dir("withdraw");
    let p = Plain::at(root_of_source(&d, "steam-123"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
    current(&p, 1);
    assert!(
        withdraw_all_unanswered(&d).is_empty(),
        "nothing taken, nothing withdrawn"
    );
    let r = p.restore_mode(1, "driver_career_progressive", 50).unwrap();
    std::fs::rename(p.root.join(inject::INJECT), p.root.join(inject::APPLYING)).unwrap();
    std::fs::write(
        p.root.join(inject::INJECTED),
        r#"{"schema":1,"results":[{"id":"older","ok":true,"detail":"kept"}]}"#,
    )
    .unwrap();
    assert!(p.pending().is_empty());
    assert_eq!(p.applying(), r);
    let lines = withdraw_all_unanswered(&d);
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains(&r[0].id) && lines[0].contains("generation 1"));
    assert!(!p.root.join(inject::APPLYING).exists());
    let answers = p.answers();
    assert_eq!(answers.len(), 3);
    assert_eq!(answers[0].detail, "kept");
    assert!(
        answers[1..]
            .iter()
            .all(|a| !a.ok && a.detail == inject::WITHDRAWN)
    );
    assert_eq!(answers[1].id, r[0].id);
    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(p.root.join(inject::INJECTED)).unwrap()).unwrap();
    assert_eq!(v["schema"], 1);
    assert!(
        withdraw_all_unanswered(&d).is_empty(),
        "a second pass finds nothing"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn roots_follow_the_contract() {
    let data = std::path::Path::new("/d");
    assert_eq!(
        root_of_source(data, "launcher-default"),
        data.join("saves/default/plain")
    );
    assert_eq!(root_of_source(data, "steam-123"), data.join("plain/steam-123"));
    assert_eq!(root_of_source(data, "ea-me"), data.join("plain/ea-me"));
}

#[test]
fn a_mode_with_one_half_missing_is_lost_and_the_mode_comes_from_the_name_or_the_pair_field() {
    assert_eq!(mode_of("gm_driver_career"), "driver_career");
    assert_eq!(
        mode_of("driver_career_SaveSlotGameModeComponent"),
        "driver_career"
    );
    assert_eq!(mode_of("swofty_registry"), "swofty_registry");
    let d = dir("half");
    let p = Plain::at(d.join("plain"));
    write(&p, &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob")]));
    let mut load = career_gen(2, "load", None, &[("0x0a", "Jacob")]);
    load.containers.remove(3);
    write(&p, &load);
    current(&p, 2);
    let loss = p.loss(&[]).unwrap();
    assert_eq!(loss.items.len(), 1);
    assert!(!loss.items[0].whole);
    assert_eq!(loss.items[0].mode, "driver_career_progressive");
    assert_eq!(loss.containers(), [GM, SLOT]);

    let manifest = p.manifest(1).unwrap();
    let mut with_pair = manifest.containers[2].clone();
    with_pair.pair = Some("other".into());
    assert_eq!(with_pair.mode(), "other");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_career_guid_missing_from_either_half_of_a_mode_is_lost() {
    let d = dir("guid");
    let p = Plain::at(d.join("plain"));
    write(
        &p,
        &career_gen(1, "save", Some("s1"), &[("0x0a", "Jacob"), ("0x0b", "Ada")]),
    );
    write(&p, &career_gen(2, "load", None, &[("0x0a", "Jacob")]));
    current(&p, 2);
    let loss = p.loss(&[]).unwrap();
    assert_eq!(loss.items[0].careers.len(), 1);
    assert_eq!(loss.items[0].careers[0].guid, "0x0b");
    assert!(p.careers().iter().all(|r| !r.mode.is_empty()));
    std::fs::remove_dir_all(&d).ok();
}
