use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::fnv1a64;

use super::*;

#[test]
fn keeps_a_change_another_window_saved_while_editing() {
    let root = std::env::temp_dir().join(format!("swofty-cli-db-edit-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(root.clone());
    let records = vec![
        Record::new("entrants.teams._team_instancecount_", 0, "1"),
        Record::new("entrants.teams.team_0._metadata_", 2, "mclaren_26"),
        Record::new("entrants.teams.team_0.stat_id", 0, "100"),
        Record::new("entrants.teams.team_0.prestige", 0, "5"),
    ];
    let cat = Catalog::load(&Gvdb::new(records).to_bytes()).unwrap();
    let id = Package::create(&store, "Mine").unwrap().id;
    let (stat, prestige) = (
        fnv1a64("entrants.teams.team_0.stat_id"),
        fnv1a64("entrants.teams.team_0.prestige"),
    );
    let mut calls = 0;
    edit(&store, &cat, &id, |p| {
        calls += 1;
        if calls == 1 {
            std::thread::sleep(std::time::Duration::from_millis(20));
            let mut other = Package::open(&store, &id)?;
            other.edits.set(&cat, stat, "7").map_err(|e| anyhow!(e))?;
            other.save(&store, &cat)?;
        }
        p.edits.set(&cat, prestige, "9").map_err(|e| anyhow!(e))
    })
    .unwrap();
    assert_eq!(calls, 2);
    let saved = Package::open(&store, &id).unwrap().edits;
    assert_eq!(saved.value(&cat, stat), Some("7"));
    assert_eq!(saved.value(&cat, prestige), Some("9"));
    std::fs::remove_dir_all(&root).ok();
}
