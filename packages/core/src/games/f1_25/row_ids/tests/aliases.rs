use super::*;

#[test]
fn aliases_of_addons_outside_the_build_do_not_hide_the_builds_own() {
    let mut store = Store::default();
    let ours = key("x", VEHICLES, "x1");
    let theirs = key("y", VEHICLES, "y1");
    store.insert(ours.clone(), 20);
    store.insert(theirs.clone(), 21);
    store.claimed.extend([ours.clone(), theirs.clone()]);
    store.aliases.entry(ours.clone()).or_default().insert(12);
    store.aliases.entry(theirs.clone()).or_default().insert(12);
    assert!(store.id_aliases().is_empty());
    store.building(["x".to_owned()]);
    store.claimed.insert(ours);
    assert_eq!(store.id_aliases().tables["vehicles"][&12], 20);
}

#[test]
fn an_old_id_another_row_holds_in_the_shared_ids_is_that_rows_alias() {
    let mut store = Store::default();
    let owner = key("x", VEHICLES, "x1");
    let stale = key("x", VEHICLES, "x2");
    store.insert(owner.clone(), 30);
    store.insert(stale.clone(), 31);
    store.claimed.extend([owner.clone(), stale.clone()]);
    store
        .base_ids
        .entry("entrants.vehicles.vehicle".to_owned())
        .or_default()
        .insert(12, owner.clone());
    store.aliases.entry(owner).or_default().insert(12);
    store.aliases.entry(stale).or_default().insert(12);
    assert_eq!(store.id_aliases().tables["vehicles"][&12], 30);
}

#[test]
fn a_kept_row_the_build_did_not_add_gives_no_alias() {
    let mut store = Store::default();
    let added = key("x", VEHICLES, "x1");
    let kept = key("x", VEHICLES, "x2");
    store.insert(added.clone(), 20);
    store.insert(kept.clone(), 21);
    store.claimed.insert(added.clone());
    store.aliases.entry(added).or_default().insert(12);
    store.aliases.entry(kept).or_default().insert(13);
    let aliases = store.id_aliases();
    assert_eq!(aliases.tables["vehicles"].get(&12), Some(&20));
    assert_eq!(aliases.tables["vehicles"].get(&13), None);
}

#[test]
fn an_old_id_a_registry_record_pins_gives_no_alias() {
    let mut store = Store::default();
    let row = key("x", VEHICLES, "x1");
    store.insert(row.clone(), 725);
    store.claimed.insert(row.clone());
    store.aliases.entry(row).or_default().extend([509, 600]);
    store
        .pinned
        .entry("entrants.vehicles.vehicle".to_owned())
        .or_default()
        .insert(509, "y/mercedes".to_owned());
    let aliases = store.id_aliases();
    assert_eq!(aliases.tables["vehicles"].get(&509), None);
    assert_eq!(aliases.tables["vehicles"].get(&600), Some(&725));
}

#[test]
fn an_old_id_pinned_by_an_addon_outside_the_build_still_gives_an_alias() {
    let mut store = Store::default();
    let team = key("swofty.f1-1990", TEAMS, "mclaren_1990");
    store.insert(team.clone(), 644);
    store.building(["swofty.f1-1990".to_owned()]);
    store.claimed.insert(team.clone());
    store.aliases.entry(team).or_default().insert(502);
    let pins = store.pinned.entry("entrants.teams.team".to_owned()).or_default();
    pins.insert(502, "swofty.swofty-seasons-2022/sw22_haas".to_owned());
    let aliases = store.id_aliases();
    assert_eq!(aliases.tables["teams"][&502], 644);
}

#[test]
fn an_old_id_pinned_by_an_addon_in_the_build_gives_no_alias() {
    let mut store = Store::default();
    let team = key("swofty.f1-1990", TEAMS, "mclaren_1990");
    store.insert(team.clone(), 644);
    store.building([
        "swofty.f1-1990".to_owned(),
        "swofty.swofty-seasons-2022".to_owned(),
    ]);
    store.claimed.insert(team.clone());
    store.aliases.entry(team).or_default().insert(502);
    let pins = store.pinned.entry("entrants.teams.team".to_owned()).or_default();
    pins.insert(502, "swofty.swofty-seasons-2022/sw22_haas".to_owned());
    assert!(store.id_aliases().is_empty());
}

#[test]
fn two_rows_moved_from_one_old_id_to_different_ids_give_no_alias() {
    let mut store = Store::default();
    let a = key("a", TEAMS, "a1");
    let b = key("b", TEAMS, "b1");
    store.insert(a.clone(), 644);
    store.insert(b.clone(), 645);
    store.building(["a".to_owned(), "b".to_owned()]);
    store.claimed.extend([a.clone(), b.clone()]);
    store.aliases.entry(a).or_default().insert(502);
    store.aliases.entry(b).or_default().insert(502);
    assert!(store.id_aliases().is_empty());
}
