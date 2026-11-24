use std::cell::RefCell;
use std::collections::BTreeMap;

use serde_json::json;

use super::*;
use crate::online::multiplayer::types::RoomAddon;

fn sha(c: char) -> String {
    c.to_string().repeat(64)
}

fn version(v: &str, sha256: &str, size: u64) -> catalog::Version {
    catalog::Version {
        version: semver::Version::parse(v).unwrap(),
        launcher: None,
        game: "f1_25".into(),
        size,
        sha256: sha256.into(),
        url: format!("http://localhost/{v}"),
        published_at: String::new(),
        changelog: String::new(),
        native: false,
        yanked: false,
        platforms: Vec::new(),
        requires: Some(Vec::new()),
    }
}

fn listed(id: &str, name: &str, versions: Vec<catalog::Version>) -> Addon {
    Addon {
        id: id.into(),
        name: name.into(),
        author: "a".into(),
        summary: String::new(),
        description: String::new(),
        category: None,
        kind: Vec::new(),
        homepage: None,
        repository: None,
        icon: None,
        publisher: None,
        publishers: Vec::new(),
        downloads: 0,
        downloads_30d: 0,
        stars: 0,
        trend: None,
        created_at: String::new(),
        updated_at: String::new(),
        latest: versions.first().cloned(),
        versions,
        media: Vec::new(),
    }
}

fn host(id: &str, name: &str, v: &str, registry: Option<&str>, order: Option<usize>) -> HostAddon {
    HostAddon {
        id: id.into(),
        name: name.into(),
        version: semver::Version::parse(v).unwrap(),
        registry: registry.map(str::to_owned),
        order,
        builtin: false,
    }
}

fn have(v: &str, from: Option<&str>) -> Have {
    Have {
        version: semver::Version::parse(v).unwrap(),
        from: from.map(str::to_owned),
    }
}

fn exe(sha: &str) -> HostGame<'_> {
    HostGame {
        exe_sha256: Some(sha),
        version: None,
    }
}

fn own(have: &[(&str, Have)], catalog: Vec<Addon>) -> Own {
    Own {
        launcher: "1.10.14".into(),
        exe_sha256: Some(sha('e')),
        game_version: None,
        dlc: None,
        have: have.iter().map(|(id, h)| ((*id).to_owned(), h.clone())).collect(),
        catalog,
    }
}

fn manifest(id: &str, v: &str, requires: &[&str], cosmetic: bool) -> AddonManifest {
    let requires: Vec<serde_json::Value> = requires
        .iter()
        .map(|id| json!({ "id": id, "version": "*" }))
        .collect();
    let mut doc = json!({
        "schema_version": 1,
        "id": id,
        "name": format!("{id} name"),
        "version": v,
        "author": "a",
        "compat": { "game": "f1_25" },
        "requires": requires,
    });
    if cosmetic {
        doc["multiplayer"] = json!("cosmetic");
    }
    serde_json::from_value(doc).unwrap()
}

fn installed(list: &[AddonManifest]) -> BTreeMap<String, AddonManifest> {
    list.iter().map(|m| (m.id.clone(), m.clone())).collect()
}

#[test]
fn an_addon_installed_at_the_hosts_version_needs_nothing() {
    let plan = plan(
        &[host("x.a", "A", "1.0.0", Some(&sha('a')), Some(0))],
        Some("1.10.14"),
        exe(&sha('e')),
        None,
        &own(&[("x.a", have("1.0.0", Some(&sha('a'))))], Vec::new()),
    );
    assert_eq!(plan, Plan::default());
}

#[test]
fn a_missing_addon_installs_from_the_addon_browser_when_it_lists_the_hosts_copy() {
    let season = listed("x.s", "SWOFTY 2012", vec![version("1.2.0", &sha('b'), 40 << 20)]);
    let plan = plan(
        &[host("x.s", "SWOFTY 2012", "1.2.0", Some(&sha('b')), Some(1))],
        None,
        HostGame::default(),
        None,
        &own(&[], vec![season]),
    );
    assert!(plan.blockers.is_empty());
    assert_eq!(plan.installs.len(), 1);
    assert_eq!(plan.installs[0].name(), "SWOFTY 2012");
    assert_eq!(plan.installs[0].replaces, None);
    assert_eq!(plan.bytes(), 40 << 20);
}

#[test]
fn another_version_installs_the_hosts_and_replaces_the_players_copy() {
    let dm = listed(
        "x.dm",
        "Driver Manager",
        vec![version("0.1.4", &sha('d'), 1), version("0.1.3", &sha('c'), 1)],
    );
    let plan = plan(
        &[host("x.dm", "Driver Manager", "0.1.3", Some(&sha('c')), None)],
        None,
        HostGame::default(),
        None,
        &own(&[("x.dm", have("0.1.4", Some(&sha('d'))))], vec![dm]),
    );
    assert_eq!(plan.installs.len(), 1);
    assert_eq!(plan.installs[0].version.version, semver::Version::new(0, 1, 3));
    assert_eq!(plan.installs[0].replaces, Some(semver::Version::new(0, 1, 4)));
}

#[test]
fn the_same_version_from_another_download_is_reinstalled_only_when_the_hosts_is_listed() {
    let a = listed("x.a", "A", vec![version("1.0.0", &sha('a'), 1)]);
    let mine = [("x.a", have("1.0.0", Some(&sha('z'))))];
    let wanted = [host("x.a", "A", "1.0.0", Some(&sha('a')), None)];
    let listed_plan = plan(&wanted, None, HostGame::default(), None, &own(&mine, vec![a]));
    assert_eq!(listed_plan.installs.len(), 1);
    let unlisted = plan(&wanted, None, HostGame::default(), None, &own(&mine, Vec::new()));
    assert_eq!(
        unlisted,
        Plan::default(),
        "the module's comparison warns of that copy instead"
    );
}

#[test]
fn an_addon_the_addon_browser_does_not_list_at_the_hosts_version_blocks_the_match() {
    let a = listed("x.a", "A", vec![version("2.0.0", &sha('a'), 1)]);
    let other = listed("x.b", "B", vec![version("1.0.0", &sha('9'), 1)]);
    let plan = plan(
        &[
            host("x.a", "A", "1.0.0", Some(&sha('a')), None),
            host("x.b", "B", "1.0.0", Some(&sha('b')), None),
            host("x.file", "From a file", "1.0.0", None, None),
        ],
        None,
        HostGame::default(),
        None,
        &own(&[], vec![a, other]),
    );
    assert!(plan.installs.is_empty());
    let why: Vec<(&str, Unlisted)> = plan
        .blockers
        .iter()
        .map(|b| match b {
            Blocker::Addon { name, why, .. } => (name.as_str(), *why),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        why,
        [
            ("A", Unlisted::NotListed),
            ("B", Unlisted::OtherCopy),
            ("From a file", Unlisted::NotListed)
        ]
    );
}

#[test]
fn the_ea_and_steam_copies_of_one_game_version_match() {
    let (steam, ea) = (sha('e'), sha('f'));
    let mut me = own(&[], Vec::new());
    me.game_version = Some("1.0.145.2472".into());
    let ea_host = HostGame {
        exe_sha256: Some(&ea),
        version: Some("1.0.145.2472"),
    };
    assert!(plan(&[], None, ea_host, None, &me).blockers.is_empty());
    let host = HostPrint {
        exe_sha256: ea.clone(),
        game_version: Some("1.0.145.2472".into()),
        ..HostPrint::default()
    };
    assert!(plan_for(&host, &me).blockers.is_empty());
    let older = HostGame {
        exe_sha256: Some(&steam),
        version: Some("1.0.141.7865"),
    };
    assert_eq!(plan(&[], None, older, None, &me).blockers, [Blocker::Game]);
    assert_eq!(plan(&[], None, exe(&ea), None, &me).blockers, [Blocker::Game]);
    me.game_version = None;
    assert_eq!(plan(&[], None, ea_host, None, &me).blockers, [Blocker::Game]);
}

#[test]
fn another_launcher_or_game_build_blocks_the_match() {
    let me = own(&[], Vec::new());
    let other = plan(&[], Some("1.10.12"), exe(&sha('f')), None, &me);
    assert_eq!(
        other.blockers,
        [
            Blocker::Launcher {
                host: "1.10.12".into()
            },
            Blocker::Game
        ]
    );
    assert!(
        plan(&[], Some("1.10.14"), exe(&sha('E')), None, &me)
            .blockers
            .is_empty()
    );
    let mut core = host(CORE_ID, "Core", "9.0.0", None, Some(0));
    core.builtin = true;
    let builtin = plan(&[core], None, HostGame::default(), None, &me);
    assert!(matches!(builtin.blockers[..], [Blocker::Builtin { .. }]));
}

#[test]
fn a_room_listing_with_versions_reads_as_the_hosts_addons() {
    let mut a = RoomAddon {
        id: "x.a".into(),
        name: "A".into(),
        version: Some("1.2.0".into()),
        registry: Some(sha('a')),
        order: Some(3),
        builtin: false,
    };
    let listing = RoomAddons {
        count: 1,
        list: vec![a.clone()],
        ..RoomAddons::default()
    };
    let read = listed_addons(&listing).unwrap();
    assert_eq!(read, [host("x.a", "A", "1.2.0", Some(&sha('a')), Some(3))]);
    a.version = None;
    let old = RoomAddons {
        count: 1,
        list: vec![a],
        ..RoomAddons::default()
    };
    assert!(listed_addons(&old).is_none(), "a server that lists no versions");
    assert!(listed_addons(&RoomAddons::default()).is_none());
}

fn two_installs() -> (Plan, Vec<Addon>) {
    let a = listed("x.a", "A", vec![version("1.0.0", &sha('a'), 10)]);
    let b = listed("x.b", "B", vec![version("2.0.0", &sha('b'), 30)]);
    let p = plan(
        &[
            host("x.a", "A", "1.0.0", Some(&sha('a')), Some(0)),
            host("x.b", "B", "2.0.0", Some(&sha('b')), Some(1)),
        ],
        None,
        HostGame::default(),
        None,
        &own(&[], vec![a.clone(), b.clone()]),
    );
    (p, vec![a, b])
}

#[test]
fn installs_run_one_by_one_and_report_addons_and_bytes() {
    let (plan, latest) = two_installs();
    let seen = RefCell::new(Vec::new());
    let done = RefCell::new(Vec::new());
    install_all(
        &plan,
        &latest,
        &semver::Version::new(1, 10, 14),
        &|a, v, bytes| {
            bytes(v.size / 2);
            done.borrow_mut().push(a.id.clone());
            Ok(())
        },
        &|p| seen.borrow_mut().push((p.done, p.total, p.bytes, p.total_bytes)),
    )
    .unwrap();
    assert_eq!(done.into_inner(), ["x.a", "x.b"]);
    let seen = seen.into_inner();
    assert_eq!(seen.first(), Some(&(0, 2, 0, 40)));
    assert!(seen.contains(&(0, 2, 5, 40)));
    assert!(seen.contains(&(1, 2, 25, 40)));
    assert_eq!(seen.last(), Some(&(2, 2, 40, 40)));
}

#[test]
fn an_install_the_addon_browser_no_longer_lists_or_lists_another_copy_of_fails_before_downloading() {
    let (plan, mut latest) = two_installs();
    let downloads = RefCell::new(0);
    let fake = |_: &Addon, _: &catalog::Version, _: &dyn Fn(u64)| {
        *downloads.borrow_mut() += 1;
        Ok(())
    };
    let launcher = semver::Version::new(1, 10, 14);
    latest[1].versions = vec![version("2.0.0", &sha('x'), 30)];
    latest[1].latest = latest[1].versions.first().cloned();
    let err = install_all(&plan, &latest, &launcher, &fake, &|_| {}).unwrap_err();
    assert_eq!(err, "the Addon Browser's B 2.0.0 is no longer the host's copy");
    assert_eq!(*downloads.borrow(), 1, "A went in before B failed");
    latest.remove(1);
    let err = install_all(&plan, &latest, &launcher, &fake, &|_| {}).unwrap_err();
    assert_eq!(err, "the Addon Browser no longer lists B 2.0.0");
}

#[test]
fn a_failed_download_names_the_addon_and_a_retry_runs_the_plan_again() {
    let (plan, latest) = two_installs();
    let launcher = semver::Version::new(1, 10, 14);
    let fail = RefCell::new(true);
    let install = |a: &Addon, _: &catalog::Version, _: &dyn Fn(u64)| {
        if a.id == "x.b" && *fail.borrow() {
            return Err("the download's SHA-256 is not the listed one".to_owned());
        }
        Ok(())
    };
    let err = install_all(&plan, &latest, &launcher, &install, &|_| {}).unwrap_err();
    assert_eq!(err, "B 2.0.0: the download's SHA-256 is not the listed one");
    *fail.borrow_mut() = false;
    assert!(install_all(&plan, &latest, &launcher, &install, &|_| {}).is_ok());
}

#[test]
fn the_room_profile_turns_on_exactly_the_hosts_addons_in_the_hosts_order_with_its_options() {
    let all = installed(&[
        manifest(CORE_ID, "1.0.0", &[], false),
        manifest("x.season", "1.2.0", &[], false),
        manifest("x.career", "1.0.0", &["x.season"], false),
        manifest("x.extra", "1.0.0", &[], false),
        manifest("x.old", "0.9.0", &[], false),
        manifest("x.hud", "1.0.0", &[], true),
        manifest("x.needy", "1.0.0", &["x.gone"], false),
    ]);
    let wanted = [
        host("x.career", "Career", "1.0.0", None, Some(2)),
        host("x.season", "Season", "1.2.0", None, Some(1)),
        host("x.old", "Old", "1.0.0", None, Some(3)),
        host("x.needy", "Needy", "1.0.0", None, None),
    ];
    let mut options = BTreeMap::new();
    options.insert(
        "x.career".to_owned(),
        BTreeMap::from([("laps".to_owned(), json!(5))]),
    );
    options.insert(
        "x.extra".to_owned(),
        BTreeMap::from([("on".to_owned(), json!(true))]),
    );
    let mut kept = Profile::new(ROOM_PROFILE);
    kept.state.insert("x.season".into(), json!({"built": 1}));
    let p = room_profile(
        &wanted,
        &options,
        &["x.hud".into(), "x.extra".into()],
        &all,
        Some(&kept),
    );
    assert_eq!(p.name, ROOM_PROFILE);
    let on: Vec<&str> = p.enabled().collect();
    assert_eq!(on, [CORE_ID, "x.season", "x.career", "x.hud"]);
    assert!(p.has("x.extra") && p.has("x.old"), "the rest are listed, off");
    assert_eq!(p.options.keys().collect::<Vec<_>>(), ["x.career"]);
    assert_eq!(p.options["x.career"]["laps"], json!(5));
    assert_eq!(p.state, kept.state);
    assert!(crate::profile::resolver::resolve(&p, &all).is_ok());
}

#[test]
fn the_room_profile_is_hidden_and_a_player_cannot_name_one_like_it() {
    assert!(is_room_profile(ROOM_PROFILE));
    assert!(!is_room_profile("room"));
    assert!(crate::profile::valid_name(ROOM_PROFILE));
    assert!(!crate::profile::valid_new_name(ROOM_PROFILE));
}

#[test]
fn each_host_addon_gets_its_need_in_the_hosts_order() {
    let b = listed("x.b", "B", vec![version("2.0.0", &sha('b'), 1)]);
    let needs = needs(
        &[
            host("x.a", "A", "1.0.0", Some(&sha('a')), Some(0)),
            host("x.b", "B", "2.0.0", Some(&sha('b')), Some(1)),
            host("x.c", "C", "1.0.0", None, None),
        ],
        None,
        &own(&[("x.a", have("1.0.0", None))], vec![b]),
    );
    assert_eq!(needs[0], Need::Have, "a copy from a file at the host's version");
    assert!(matches!(&needs[1], Need::Install(i) if i.version.version == semver::Version::new(2, 0, 0)));
    assert!(matches!(
        &needs[2],
        Need::Blocked(Blocker::Addon {
            why: Unlisted::NotListed,
            ..
        })
    ));
}
