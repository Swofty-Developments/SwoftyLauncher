use std::path::PathBuf;

use swofty_core::ui::node::{NodeExt, ObjExt};
use swofty_formats::erp::Erp;
use swofty_formats::pkg::Pkg;

#[test]
fn the_ui_data_patch_leaves_one_leaderboard_tab() {
    let Some(erp) = std::env::var_os("F1_UI_DATABASES_ERP").map(PathBuf::from) else {
        eprintln!("F1_UI_DATABASES_ERP is not set; skipping");
        return;
    };
    let res = Erp::read_matching(&erp, |n| n == "eaid://ui_package/databases/ui_data.uidb").unwrap();
    let stock = res[0].fragments[0].data().unwrap();
    let list = |data: &[u8]| {
        let mut p = Pkg::parse(data).unwrap();
        let o = p.root.o_mut().unwrap();
        o.at_mut("leaderboard_data")
            .unwrap()
            .o_mut()
            .unwrap()
            .at_mut("Contents")
            .unwrap()
            .o_mut()
            .unwrap()
            .at_mut("table")
            .unwrap()
            .o_mut()
            .unwrap()
            .at_mut("Contents")
            .unwrap()
            .o_mut()
            .unwrap()
            .at_mut("view_type_list")
            .unwrap()
            .o_mut()
            .unwrap()
            .at_mut("Contents")
            .unwrap()
            .a_mut()
            .unwrap()
            .len()
    };
    assert_eq!(list(&stock), 2);
    let patched = swofty_core::ui::patch::ui_data(&stock, Vec::new()).unwrap();
    assert_eq!(list(&patched), 1);
}

#[test]
fn report_a_built_ui_data() {
    let Some(erp) = std::env::var_os("F1_BUILT_DATABASES_ERP").map(PathBuf::from) else {
        return;
    };
    let res = Erp::read_matching(&erp, |n| n == "eaid://ui_package/databases/ui_data.uidb").unwrap();
    let data = res[0].fragments[0].data().unwrap();
    let mut p = Pkg::parse(&data).unwrap();
    let o = p.root.o_mut().unwrap();
    let n = o
        .at_mut("leaderboard_data").unwrap().o_mut().unwrap().at_mut("Contents").unwrap().o_mut().unwrap()
        .at_mut("table").unwrap().o_mut().unwrap().at_mut("Contents").unwrap().o_mut().unwrap()
        .at_mut("view_type_list").unwrap().o_mut().unwrap().at_mut("Contents").unwrap().a_mut().unwrap().len();
    let swofty = o.at_mut("ui_menu_data").unwrap().o_mut().unwrap().at_mut("Contents").unwrap().o_mut().unwrap().get("swofty").is_some();
    eprintln!("BUILT view_type_list entries {n}, ui_menu_data.swofty {swofty}");
}
