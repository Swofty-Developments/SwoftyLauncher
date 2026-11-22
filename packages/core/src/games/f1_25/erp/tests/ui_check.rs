use super::*;

// A screen `page` with a node `logo`, ids fixed; `edit` changes it after.
fn page(edit: &dyn Fn(&mut Node)) -> Vec<u8> {
    use crate::ui::node::{NodeExt, arr, fix_ids, node, ui_bytes};
    let mut scr = node("page", "UISC", vec![], vec![], &[], &[]);
    scr.a_mut().unwrap()[crate::ui::node::CHILDREN] =
        arr(vec![node("logo", "UIGP", vec![], vec![], &[], &[])]);
    fix_ids(&mut scr, None);
    edit(&mut scr);
    ui_bytes(vec![scr]).unwrap()
}

fn check_page_edit(name: &str, edited: Vec<u8>) -> Result<Vec<String>> {
    let root = std::env::temp_dir().join(format!("swofty-checkui-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let (game, addon) = (root.join("game"), root.join("addon"));
    let (erp_rel, ui) = (
        "2025_asset_groups/ui_package/t.erp",
        "eaid://ui_package/screens/t.ui",
    );
    std::fs::create_dir_all(game.join("2025_asset_groups/ui_package")).unwrap();
    std::fs::create_dir_all(&addon).unwrap();
    std::fs::write(game.join(erp_rel), typed_erp("UIDF", &[(ui, page(&|_| {}))])).unwrap();
    std::fs::write(addon.join("t.ui"), edited).unwrap();
    let original = |rel: &str| game.join(rel);
    let resolve = |s: &Source| Ok(s.resolve(&addon)?);
    let mut b = ErpBuild::new(&original);
    let op = &erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{{"op":"replace-resource","erp":"{erp_rel}","resource":"{ui}","source":"t.ui"}}]}}"#
    ))[0];
    b.apply_for(op, "t.mod", &resolve).unwrap();
    let got = b.check_ui(&|origin| format!("Mod <{origin}>"));
    std::fs::remove_dir_all(&root).ok();
    got
}

#[test]
fn a_renamed_node_that_keeps_its_hashes_warns_and_names_the_addon() {
    use crate::ui::node::NodeExt;
    let renamed = page(&|scr| {
        let logo = &mut scr.slot_mut(crate::ui::node::CHILDREN).unwrap()[0];
        crate::ui::node::rename(logo, "logo2").unwrap();
    });
    let warnings = check_page_edit("renamed", renamed).unwrap();
    let stale: Vec<&String> = warnings.iter().filter(|w| w.contains("page.logo2")).collect();
    assert_eq!(stale.len(), 2, "{warnings:?}");
    for w in stale {
        assert!(
            w.starts_with(
                "Mod <t.mod>: 2025_asset_groups/ui_package/t.erp: eaid://ui_package/screens/t.ui: "
            ),
            "{w}"
        );
        assert!(w.contains("the game still finds it by the hash"), "{w}");
    }
}

#[test]
fn a_signal_to_a_missing_node_fails_the_build_and_names_the_addon() {
    use crate::ui::node::{NodeExt, add_events, event, fix_ids, sig};
    let broken = page(&|scr| {
        let logo = &mut scr.slot_mut(crate::ui::node::CHILDREN).unwrap()[0];
        add_events(
            logo,
            vec![event("pressed", vec![sig("page.gone", "focus", "pressed")])],
        )
        .unwrap();
        fix_ids(scr, None);
    });
    let err = check_page_edit("missing", broken).unwrap_err().to_string();
    assert!(
        err.contains("UI edit(s) break rules the game's UI loader enforces"),
        "{err}"
    );
    assert!(
        err.contains("\n  Mod <t.mod>: 2025_asset_groups/ui_package/t.erp: "),
        "{err}"
    );
    assert!(err.contains("-> missing page.gone.focus"), "{err}");
}
