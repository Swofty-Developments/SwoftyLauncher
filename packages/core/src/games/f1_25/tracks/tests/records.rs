use super::*;

#[test]
fn track_records_become_packages_from_their_files_folder() {
    let root = std::env::temp_dir().join(format!("swofty-record-tracks-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(root.join("registry")).unwrap();
    std::fs::create_dir_all(root.join("track/mipmaps")).unwrap();
    std::fs::create_dir_all(root.join("copy")).unwrap();
    std::fs::write(
        root.join("registry/track.json"),
        r#"{ "track": {
                "ring": { "code": "ring", "name": "Ring", "base": "game/austria", "files": "track/" },
                "copy": { "code": "testring", "base": "game/melbourne", "files": "copy/" },
                "nofiles": { "code": "nofiles", "base": "game/melbourne" },
                "outside": { "code": "outside", "base": "game/melbourne", "files": "../x/" },
                "$select": [],
                "game/melbourne": { "name": "a layer" },
                "nocode": { "base": "game/melbourne" } },
              "track.data": { "mod.x/ring/f1_2025": { "laps": 3 } } }"#,
    )
    .unwrap();
    std::fs::write(root.join("track/ring.erp"), b"erp").unwrap();
    std::fs::write(
        root.join("copy/manifest.json"),
        crate::content::track_package::tests::OKAYAMA_LIKE,
    )
    .unwrap();
    let found = record_tracks(&root, "mod.x", &["registry/track.json".into()]);
    let ids: Vec<&str> = found.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        ["mod.x/copy", "mod.x/nofiles", "mod.x/outside", "mod.x/ring"]
    );
    let by = |id: &str| found.iter().find(|t| t.id == id).unwrap();
    let (dir, ring) = record_package(&root, by("mod.x/ring")).unwrap();
    assert_eq!(dir, root.join("track"));
    assert_eq!(
        (
            ring.track_code.as_str(),
            ring.track_name.as_deref(),
            ring.erp.as_str()
        ),
        ("ring", Some("Ring"), "ring.erp")
    );
    assert_eq!(ring.activation.base_track(), Some("austria"));
    assert_eq!(ring.copy_directories.len(), 1);
    let (_, copy) = record_package(&root, by("mod.x/copy")).unwrap();
    assert_eq!(copy.track_code, "testring");
    assert!(
        record_package(&root, by("mod.x/nofiles"))
            .unwrap_err()
            .why
            .contains("names no files")
    );
    assert!(
        record_package(&root, by("mod.x/outside"))
            .unwrap_err()
            .why
            .contains("leaves")
    );
    std::fs::remove_file(root.join("track/ring.erp")).unwrap();
    assert!(
        record_package(&root, by("mod.x/ring"))
            .unwrap_err()
            .why
            .contains("has no ring.erp")
    );
    let wrong = RecordTrack {
        code: "other".into(),
        ..by("mod.x/copy").clone()
    };
    assert!(
        record_package(&root, &wrong)
            .unwrap_err()
            .why
            .contains("is for testring")
    );
    std::fs::remove_dir_all(&root).ok();
}
