use super::*;

#[test]
fn lists_the_tracks_installed_addons_add() {
    let root = std::env::temp_dir().join(format!("swofty-installed-tracks-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let addon = |id: &str, ops: &str, package: Option<String>| {
        let dir = store.addons_dir().join(id);
        std::fs::create_dir_all(dir.join("ops")).unwrap();
        std::fs::write(
            dir.join("swofty.json"),
            format!(
                r#"{{"schema_version":1,"id":"{id}","name":"{id}","version":"1.0.0","author":"t",
                    "compat":{{"game":"f1_25"}},"operations":{{"assets":["ops/track.json"]}}}}"#
            ),
        )
        .unwrap();
        std::fs::write(dir.join("ops/track.json"), ops).unwrap();
        if let Some(package) = package {
            std::fs::create_dir_all(dir.join("track")).unwrap();
            std::fs::write(dir.join("track/manifest.json"), package).unwrap();
        }
    };
    let add = r#"{"schema_version":1,"ops":[{"op":"track.add","source":"track"}]}"#;
    let like = |code: &str, name: &str| {
        crate::content::track_package::tests::OKAYAMA_LIKE
            .replace(
                r#""track_code": "testring""#,
                &format!(r#""track_code": "{code}""#),
            )
            .replace(
                r#""track_name": "Test Ring""#,
                &format!(r#""track_name": "{name}""#),
            )
    };
    addon("mod.bahrain24", add, Some(like("bahrain24", "Sepang")));
    addon("mod.okayama", add, Some(like("okayama", "")));
    addon("mod.twice", add, Some(like("okayama", "Again")));
    addon("mod.unread", add, None);
    addon("mod.other", r#"{"schema_version":1,"ops":[]}"#, None);
    addon(
        "mod.portimao",
        add,
        Some(r#"{ "track_code": "portimao", "track_name": "Portimao", "erp": "portimao.erp" }"#.into()),
    );
    let found = installed(&store);
    assert_eq!(
        found,
        [
            PlanTrack {
                code: "bahrain24".into(),
                name: Some("Sepang".into()),
                addon: Some("mod.bahrain24".into()),
            },
            PlanTrack {
                code: "okayama".into(),
                name: None,
                addon: Some("mod.okayama".into()),
            },
        ]
    );
    let addons = store.addons().unwrap();
    let bad = |id: &str| unusable(&store, id, &addons[id]);
    assert!(bad("mod.okayama").is_empty() && bad("mod.other").is_empty());
    assert_eq!(bad("mod.unread")[0].why, "it has no manifest.json");
    let portimao = bad("mod.portimao");
    assert_eq!(portimao.len(), 1);
    assert_eq!(portimao[0].why, crate::content::track_package::NO_BASE_TRACK);
    assert_eq!(
        left_out_line("Portimao", "mod.portimao", &portimao[0]),
        "Portimao (mod.portimao): track Portimao is left out; its manifest.json doesn't say which track \
         it replaces or copies. Import a fixed version, or ask its author for one"
    );
    assert!(left_out_line("mod.portimao", "mod.portimao", &portimao[0]).starts_with("mod.portimao: track "));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn adders_name_the_addon_whose_track_record_adds_a_track() {
    let root = std::env::temp_dir().join(format!("swofty-track-adders-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let dir = store.addons_dir().join("track.testring");
    std::fs::create_dir_all(dir.join("registry")).unwrap();
    std::fs::write(
        dir.join("swofty.json"),
        r#"{"schema_version":1,"id":"track.testring","name":"Testring","version":"1.0.0","author":"t",
            "compat":{"game":"f1_25"}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("registry/track.json"),
        r#"{"track":{"testring":{"code":"testring","name":"Testring","files":"track/"}}}"#,
    )
    .unwrap();
    let codes: BTreeSet<String> = ["testring".to_owned(), "nowhere".to_owned()].into();
    let got = adders(&store, &codes);
    std::fs::remove_dir_all(&root).ok();
    assert_eq!(
        got,
        BTreeMap::from([("testring".to_owned(), "track.testring".to_owned())])
    );
}
