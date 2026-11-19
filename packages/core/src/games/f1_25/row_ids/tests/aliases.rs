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
