use super::*;
use crate::asset::{FileKind, OpFile};
use crate::ops::*;

#[test]
fn side_data_lists_ignore_patterns_and_rejects_the_rest() {
    let file = ops(
        r#"{"schema_version":1,"side_data":{"ignore":["bank.*","accolades.AccoladeDataGroup_N.driver_data.AccoladeDriver_N"]},"ops":[]}"#,
    );
    assert_eq!(file.side_data.ignore.len(), 2);
    let parse = |json: &str| serde_json::from_str::<DbOps>(json).unwrap_err().to_string();
    assert!(
        parse(r#"{"schema_version":1,"side_data":{"ignored":[]},"ops":[]}"#)
            .contains("unknown field `ignored`")
    );
    assert!(
        parse(r#"{"schema_version":1,"side_data":{"ignore":["bank initial"]},"ops":[]}"#)
            .contains("is not a table or field path")
    );
    assert!(
        parse(r#"{"schema_version":1,"side_data":{"ignore":[""]},"ops":[]}"#).contains("side_data.ignore")
    );
    let roundtrip = serde_json::to_string(&ops(r#"{"schema_version":1,"ops":[]}"#)).unwrap();
    assert!(!roundtrip.contains("side_data"), "{roundtrip}");
}

#[test]
fn erp_ops_parse_patch_pkg_and_warn() {
    let file = |json: &str| OpFile::from_value(serde_json::from_str(json).unwrap(), FileKind::Erp);
    let ops = file(
        r#"{"schema_version":1,"ops":[
            {"op":"patch-pkg","erp":"a.erp","resource":"eaid://x.layer","edits":[
              {"op":"clone-entry","at":"instances","from":{"erp":"b.erp","resource":"eaid://y.layer"},
               "src":"[name=y$A]","as":"x$B","set":{"overrides.VehPlacement.Id":25}}]},
            {"op":"warn","message":"Imola has room for 26 cars"}]}"#,
    )
    .unwrap();
    assert_eq!(ops.ops.len(), 2);
    assert!(file(r#"{"schema_version":1,"ops":[{"op":"warn"}]}"#).is_err());
}

#[test]
fn pkg_patch_parses_design_example() {
    let p: PkgPatch = serde_json::from_str(
            r#"{"schema_version":1,"targets":[{"erp":"2025_asset_groups/ui_package/flow_frontend.erp",
            "resource":"eaid://ui_package/screens/fe_home.ui"}],"root":"Screens[0]:fe_home","edits":[
            {"op":"clone-node","src":"settings","as":"mod_menu_listener","set":{"action_name":"Menu Right Stick Click"},
             "replace_event":{"event":"pressed","signals":[{"to":"fe_home.Action","pin":"route","attrs":{"key":"mod_menu"}}]}},
            {"op":"set-text","node":"contextual_panels.offline_contextual_panel.subtitle","value":"x","localise":false}]}"#,
        )
        .unwrap();
    assert_eq!(p.edits.len(), 2);
}
