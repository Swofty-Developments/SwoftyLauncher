use std::time::Duration;

use swofty_core::store::Store;
use swofty_core::testsuite::lock::GameLock;

use crate::commands::testing::game_lock::{hh_mm, take_lock};
use crate::commands::testing::preflight::terms;
use crate::commands::testing::run::Stop;

use crate::commands::testing::fakes::*;

#[test]
fn a_held_lock_is_busy_and_unaccepted_terms_are_infra() {
    let root = temp("lock-busy");
    let dir = root.join("game.lock.d");
    let held = GameLock::try_acquire(&dir, "someone else 10:00")
        .unwrap()
        .unwrap();
    match take_lock(&dir, "swofty-test r s 10:01", Duration::ZERO) {
        Err(Stop::Busy(why)) => assert!(why.contains("held by \"someone else 10:00\""), "{why}"),
        other => panic!("{other:?}"),
    }
    assert!(dir.is_dir());
    drop(held);
    let lock = take_lock(&dir, "swofty-test r s 10:01", Duration::ZERO).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("owner")).unwrap(),
        "swofty-test r s 10:01\n"
    );
    drop(lock);
    match take_lock(&root.join("missing/x.lock.d"), "x", Duration::ZERO) {
        Err(Stop::Infra(why)) => assert!(why.contains("SWOFTY_GAME_LOCK"), "{why}"),
        other => panic!("{other:?}"),
    }
    let store = Store::new(root.join("data"));
    let why = terms(&store).unwrap_err();
    assert!(why.contains("a test run never accepts them for you"), "{why}");
    swofty_core::offline::record_accept(&store.root).unwrap();
    assert!(terms(&store).is_ok());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_owner_line_has_a_clock_time() {
    let t = hh_mm();
    assert_eq!(t.len(), 5, "{t}");
    assert_eq!(&t[2..3], ":");
}
