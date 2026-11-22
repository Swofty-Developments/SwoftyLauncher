use super::*;
use crate::game_folder::baseline::{Baseline, Identity, Objects};

#[test]
fn a_deleted_test_store_gets_no_compressed_fragments_after_its_build() {
    let _pin = crate::addons::builtin::LoaderPin::none();
    let root = std::env::temp_dir().join(format!("swofty-build-packed-gone-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let dir = root.join("game");
    std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
    std::fs::write(dir.join(crate::GAME_EXE), "exe").unwrap();
    std::fs::write(dir.join("a.erp"), "stock").unwrap();
    let records = vec![swofty_formats::gvdb::Record::new(
        "entrants.teams._team_instancecount_",
        0,
        "0",
    )];
    std::fs::write(
        dir.join(GVDB),
        swofty_formats::gvdb::Gvdb::new(records).to_bytes(),
    )
    .unwrap();
    let game = Game {
        identity: Identity::detect(&dir, Some(1)).unwrap(),
        dir: dir.clone(),
        user_reg: None,
    };
    let store = Store::new(root.join("data"));
    crate::game_folder::setup::reset(&store.root, &game, false).unwrap();
    crate::game_folder::setup::finish(&store.root).unwrap();
    let objects = Objects {
        root: store.root.join("baseline/objects"),
    };
    let mut baseline = Baseline::new(&game.identity);
    baseline.record(&objects, &dir, "a.erp", &()).unwrap();
    baseline.save(&store.root).unwrap();
    build(&store, "default", &game).unwrap();

    std::fs::remove_dir_all(&root).unwrap();
    swofty_formats::erp::compress(&vec![7; 64 << 10]).unwrap();
    assert!(
        !root.exists(),
        "a compression after the build must not write into the store the build used and the test deleted"
    );
}
