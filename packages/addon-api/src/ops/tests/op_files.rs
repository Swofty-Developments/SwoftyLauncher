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

fn patch(edits: &str) -> ErpOp {
    serde_json::from_str(&format!(
        r#"{{"op":"patch-pkg","erp":"a.erp","resource":"eaid://x.layer","edits":{edits}}}"#
    ))
    .unwrap()
}

#[test]
fn placement_ids_past_the_games_32_or_at_0_are_refused() {
    let clone = |key: &str, id: &str| {
        let n = id.trim_matches('"');
        patch(&format!(
            r#"[{{"op":"clone-entry","at":"instances","src":"[name=y$PitBox12]","as":"x$PitBox{n}","set":{{"{key}":{id}}}}}]"#
        ))
    };
    for key in ["overrides.VehPlacement.Id", "overrides.F1Garage.id"] {
        for id in ["1", "32", "\"25\""] {
            assert_eq!(clone(key, id).refusal(), None, "{key} {id}");
        }
        for id in ["0", "33", "-1", "4294967295", "\"40\""] {
            let why = clone(key, id).refusal().unwrap_or_else(|| panic!("{key} {id}"));
            assert!(
                why.starts_with("a.erp eaid://x.layer: clone-entry x$PitBox"),
                "{why}"
            );
            assert!(why.contains(&format!("{key} {}", id.trim_matches('"'))), "{why}");
            assert!(
                why.ends_with("is outside 1..=32, the vehicle placements the game holds per kind"),
                "{why}"
            );
        }
    }
    let set = |path: &str, value: &str| {
        patch(&format!(
            r#"[{{"op":"set-value","path":"{path}","value":{value}}}]"#
        ))
        .refusal()
    };
    assert_eq!(
        set("instances[name=v$GridSlot3].overrides.VehPlacement.Id", "3"),
        None
    );
    assert_eq!(
        set("instances[name=v$GridSlot3].overrides.VehPlacement.Id", "33").as_deref(),
        Some(
            "a.erp eaid://x.layer: set-value instances[name=v$GridSlot3].overrides.VehPlacement.Id: \
             overrides.VehPlacement.Id 33 is outside 1..=32, the vehicle placements the game holds per kind"
        )
    );
    assert!(set("instances[name=v$F1Garage3].overrides.F1Garage.id", "0").is_some());
    assert_eq!(
        set(
            "instances[name=v$GridSlot3].overrides.WorldObject.transform",
            "[0]"
        ),
        None
    );
    let mixed = patch(
        r#"[{"op":"clone-entry","at":"instances","src":"[name=y$GridSlot22]","as":"x$GridSlot23","set":{"overrides.VehPlacement.Id":23}},
            {"op":"clone-entry","at":"instances","src":"[name=y$GridSlot22]","as":"x$GridSlot40","set":{"overrides.VehPlacement.Id":40}}]"#,
    );
    assert!(
        mixed
            .refusal()
            .unwrap()
            .contains("clone-entry x$GridSlot40: overrides.VehPlacement.Id 40")
    );
    let other = OpFile::from_value(
        serde_json::from_str(
            r#"{"schema_version":1,"ops":[{"op":"remove-resource","erp":"a.erp","resource":"r"}]}"#,
        )
        .unwrap(),
        FileKind::Erp,
    )
    .unwrap();
    assert!(other.ops.iter().all(|op| op.refusal().is_none()));
}
