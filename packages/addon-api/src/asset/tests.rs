use super::*;
use crate::ops::PkgEdit;

const DESIGN: &str = r#"{
      "schema_version": 1,
      "ops": [
        { "op": "erp.put-resource", "erp": "2025_asset_groups/f1_2026_vehicle_package/teams/toro_rosso/wep/toro_rosso.erp",
          "resource": "eaid://f1_2026_vehicle_package/teams/toro_rosso/wep/toro_rosso.wep", "source": "erp/car.erp" },
        { "op": "db.set", "select": "entrants.teams.team[_metadata_=toro_rosso_26]", "field": "display_name_code", "value": "AT" },
        { "op": "lang.set", "language": "*", "key": "lng_team_tor_hashtag", "value": "AlphaTauri" },
        { "op": "file.put", "source": "files/sponsor.dds", "target": "livery_package_shared/sponsor.dds" },
        { "op": "group.register", "group": "toro_rosso", "erps": ["a.erp"] },
        { "op": "lang.table", "language": "eng", "source": { "root": "data", "path": "lang/eng.lng" } },
        { "op": "db.put-record", "key": "0x0123456789abcdef", "type": 4, "value": "4154" },
        { "op": "db.remove-record", "key": "entrants.teams.team_0.stat_id" },
        { "op": "warn", "message": "hi" }
      ]
    }"#;

#[test]
fn parses_and_writes_the_design_example() {
    let doc: Value = serde_json::from_str(DESIGN).unwrap();
    let file = OpFile::from_value(doc.clone(), FileKind::Assets).unwrap();
    assert_eq!(file.ops.len(), 9);
    assert!(matches!(
        &file.ops[0],
        AssetOp::Erp(ErpOp::PutResource { source, rename: None, .. }) if *source == Source::addon("erp/car.erp")
    ));
    assert!(matches!(
        &file.ops[5],
        AssetOp::Lang(LangOp::Table {
            source: Source { root: Root::Data, .. },
            ..
        })
    ));
    let again = OpFile::from_value(serde_json::to_value(&file).unwrap(), FileKind::Assets).unwrap();
    assert_eq!(again, file);
    assert_eq!(serde_json::to_value(&file.ops[3]).unwrap(), doc["ops"][3]);
    let kinds: Vec<Option<Kind>> = file.ops.iter().map(AssetOp::kind).collect();
    assert_eq!(
        kinds[..4],
        [
            Some(Kind::Erp),
            Some(Kind::Db),
            Some(Kind::Lang),
            Some(Kind::File)
        ]
    );
    assert_eq!(kinds[8], None);
    let sources: Vec<&str> = file
        .ops
        .iter()
        .flat_map(AssetOp::sources)
        .map(|s| s.path.as_str())
        .collect();
    assert_eq!(sources, ["erp/car.erp", "files/sponsor.dds", "lang/eng.lng"]);
}

#[test]
fn item_show_round_trips() {
    let doc: Value = serde_json::from_str(
            r#"{ "op": "item.show", "id": "ts.sauber", "name": "Sauber 2026", "author": "Team Simplified", "version": "1.2.0" }"#,
        )
        .unwrap();
    let op = AssetOp::deserialize(doc.clone()).unwrap();
    assert!(matches!(
        &op,
        AssetOp::Item(ItemOp::Show { id, name, author, version, description })
            if id == "ts.sauber" && name == "Sauber 2026" && author == "Team Simplified"
                && version == "1.2.0" && description.is_empty()
    ));
    assert_eq!(op.kind(), None);
    assert!(op.sources().is_empty());
    assert_eq!(serde_json::to_value(&op).unwrap(), doc);
}

#[test]
fn merge_slices_and_lang_default_round_trip() {
    let doc: Value = serde_json::from_str(
        r#"[{ "op": "erp.merge-slices", "erp": "2025_asset_groups/vfx_package/common.erp",
                  "resource": "eaid://ui_package/a.texarray.image", "source": "logos/ui_textures.erp" },
                { "op": "lang.default", "language": "eng", "key": "lng_a", "value": "A" }]"#,
    )
    .unwrap();
    let ops: Vec<AssetOp> = serde_json::from_value(doc.clone()).unwrap();
    assert!(matches!(
        &ops[0],
        AssetOp::Erp(ErpOp::MergeSlices { erp, source, .. })
            if erp == "2025_asset_groups/vfx_package/common.erp" && *source == Source::addon("logos/ui_textures.erp")
    ));
    assert_eq!(ops[0].sources(), [&Source::addon("logos/ui_textures.erp")]);
    assert_eq!(
        ops[1],
        AssetOp::Lang(LangOp::Default {
            language: "eng".into(),
            key: "lng_a".into(),
            value: "A".into()
        })
    );
    assert_eq!(ops[1].kind(), Some(Kind::Lang));
    assert_eq!(serde_json::to_value(&ops).unwrap(), doc);
    let erp: Value = serde_json::from_str(
            r#"{"schema_version":1,"ops":[{"op":"merge-slices","erp":"a.erp","resource":"r","source":"s.erp","fragment":1}]}"#,
        )
        .unwrap();
    assert!(OpFile::from_value(erp, FileKind::Erp).is_err());
}

#[test]
fn prefixes_legacy_files() {
    let erp: Value = serde_json::from_str(
        r#"{"schema_version":1,"foreach":"x","ops":[
            {"op":"patch-pkg","erp":"a.erp","resource":"r","edits":[{"op":"set-value","path":"a","value":1}]},
            {"op":"warn","message":"m"}]}"#,
    )
    .unwrap();
    let file = OpFile::from_value(erp, FileKind::Erp).unwrap();
    let AssetOp::Erp(ErpOp::PatchPkg { edits, .. }) = &file.ops[0] else {
        panic!("{:?}", file.ops[0]);
    };
    assert!(matches!(edits[0], PkgEdit::SetValue { .. }));
    assert_eq!(file.ops[1], AssetOp::Warn { message: "m".into() });
    let db: Value = serde_json::from_str(
        r#"{"schema_version":1,"fields":{"t.r":["a"]},"ops":[{"op":"remove-row","select":"t.r_0"}]}"#,
    )
    .unwrap();
    let file = OpFile::from_value(db.clone(), FileKind::Database).unwrap();
    assert_eq!(file.fields["t.r"], ["a"]);
    assert!(matches!(file.ops[0], AssetOp::Db(DbOp::RemoveRow { .. })));
    assert!(OpFile::from_value(db, FileKind::Assets).is_err());
}

#[test]
fn rejects_unknown_ops_and_fields() {
    for bad in [
        r#"{"op":"erp.warn","message":"m"}"#,
        r#"{"op":"set","select":"a","field":"b","value":"c"}"#,
        r#"{"op":"tex.put"}"#,
        r#"{"op":"file.put","source":"a","target":"b","extra":1}"#,
        r#"{"op":"lang.table","language":"eng","source":{"root":"game","path":"a"}}"#,
        r#"{"message":"m"}"#,
    ] {
        assert!(serde_json::from_str::<AssetOp>(bad).is_err(), "{bad}");
    }
}

#[test]
fn resolves_sources_inside_their_root() {
    let dir = Path::new("/addon");
    assert_eq!(
        Source::addon("erp/a.erp").resolve(dir).unwrap(),
        Path::new("/addon/erp/a.erp")
    );
    assert!(Source::addon("../a.erp").resolve(dir).is_err());
    assert!(Source::addon("/a.erp").resolve(dir).is_err());
}
