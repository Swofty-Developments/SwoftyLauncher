use super::schemas::{shown_path, validate_report};
use super::*;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-check-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn addon(dir: &Path, id: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join(MANIFEST_FILE),
        format!(r#"{{"schema_version":1,"id":"{id}","name":"X","version":"1.0.0","author":"t","kind":["native"],"compat":{{"game":"f1_25"}}}}"#),
    )
    .unwrap();
}

#[test]
fn validate_paths_show_as_registry_files_on_every_os() {
    let names = vec![
        "registry/layers.json".to_owned(),
        "registry/bad-layers.json".to_owned(),
    ];
    assert_eq!(
        shown_path(
            r"C:\Users\RUNNER~1\AppData\Local\Temp\x\mod.esp\registry\layers.json",
            &names
        ),
        "registry/layers.json"
    );
    assert_eq!(
        shown_path(r"c:\users\runneradmin\x\MOD.ESP\Registry\Bad-Layers.json", &names),
        "registry/bad-layers.json"
    );
    assert_eq!(
        shown_path("/tmp/x/mod.esp/registry/layers.json", &names),
        "registry/layers.json"
    );
    assert_eq!(shown_path("registry/layers.json", &names), "registry/layers.json");
    assert_eq!(shown_path(r"registry\other.json", &names), "registry/other.json");
    let mut r = Report::default();
    validate_report(
        "ok    C:\\Users\\RUNNER~1\\T\\mod.esp\\registry\\layers.json (1.0)\n\
         FAIL  C:\\Users\\RUNNER~1\\T\\mod.esp\\registry\\bad-layers.json (1.0)\n\
         \x20       1.0 /vehicle/game~1ferrari_2025/tyre_wear_scale: not a number\n",
        &names,
        &mut r,
    );
    assert_eq!(
        r.lines,
        [
            "ok: registry/layers.json (1.0)",
            "error: registry/bad-layers.json (1.0)",
            "    1.0 /vehicle/game~1ferrari_2025/tyre_wear_scale: not a number"
        ]
    );
    assert_eq!(r.errors, 1);
}

#[test]
fn data_files_name_paths_that_must_be_in_the_addon() {
    let Ok(sdk) = super::super::sdk_root() else { return };
    let root = scratch("paths");
    let dir = root.join("mod.v12");
    addon(&dir, "mod.v12");
    std::fs::create_dir_all(dir.join("registry")).unwrap();
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/v12.bdl"), "bdl").unwrap();
    std::fs::write(
        dir.join("registry/sounds.json"),
        r#"{"engine_sound":{"v12":{"bundle":"sounds/v12.bdl"},"v8":{"bundle":"sounds/v8.bdl"},
                "stock":{"bundle":{"stock":"audio/x.bdl"}}}}"#,
    )
    .unwrap();
    let mut r = Report::default();
    missing_paths(&sdk, &dir, &data_files(&dir), &mut r);
    assert_eq!(
        r.lines,
        ["error: registry/sounds.json: engine_sound v8 names sounds/v8.bdl, which isn't in the addon"]
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_broken_manifest_stops_the_check() {
    let root = scratch("manifest");
    std::fs::write(root.join(MANIFEST_FILE), "{").unwrap();
    let r = check(&Store::new(root.join("data")), &None, &root).unwrap();
    assert_eq!(r.errors, 1, "{:?}", r.lines);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn registry_json_fields_and_references() {
    let root = scratch("refs");
    let store = Store::new(root.join("data"));
    let dir = root.join("mod.esp");
    addon(&dir, "mod.esp");
    std::fs::write(
        dir.join(REGISTRY_FILE),
        r#"{
              "requires": [ { "record": "vehicle:swofty.classic-seasons-2017/ferrari" },
                            { "registry": "career.season", "version": "^1" },
                            { "record": "vehicle:game/ferrari_2025" },
                            { "id": "swofty.classic-seasons-2017", "version": ">=0.6.0" },
                            { "record": "no-colon" },
                            { "registry": "career.season", "version": "not a range" } ],
              "recommends": [ { "record": "track:venue/sochi" } ],
              "breaks": [ { "addon": "mod.old", "version": "<1" }, { "version": "1" } ],
              "colour": "red"
            }"#,
    )
    .unwrap();
    let r = check(&store, &None, &dir).unwrap();
    let text = r.lines.join("\n");
    assert!(text.contains("doesn't know the field \"colour\""), "{text}");
    assert!(
        text.contains("warning: registry.json: requires[4]: record \"no-colon\""),
        "{text}"
    );
    assert!(text.contains("warning: registry.json: requires[5]: "), "{text}");
    assert!(
        text.contains("warning: registry.json: breaks[1]: no id"),
        "{text}"
    );
    assert!(
        !text.contains("requires[3]") && !text.contains("breaks[0]"),
        "{text}"
    );
    assert!(text.contains("recommends[0]: add a \"for\""), "{text}");
    assert!(text.contains(
        "error: registry.json: no installed addon provides vehicle:swofty.classic-seasons-2017/ferrari"
    ));
    assert!(text.contains("warning: registry.json: no installed addon provides track:venue/sochi"));
    assert!(!text.contains("game/ferrari_2025"));
    assert_eq!((r.errors, r.warnings), (1, 6), "{text}");

    let cs = store.addons_dir().join("swofty.classic-seasons-2017");
    addon(&cs, "swofty.classic-seasons-2017");
    std::fs::write(
        cs.join(LOCK_FILE),
        r#"{"generated_by":"x","provides":{"vehicle":[{"id":"swofty.classic-seasons-2017/ferrari","label":"Ferrari"}]},"needs":[]}"#,
    )
    .unwrap();
    let sochi = store.addons_dir().join("mod.sochi");
    addon(&sochi, "mod.sochi");
    std::fs::create_dir_all(sochi.join("registry")).unwrap();
    std::fs::write(
        sochi.join("registry/tracks.json"),
        r#"{"track":{"venue/sochi":{}}}"#,
    )
    .unwrap();
    let r = check(&store, &None, &dir).unwrap();
    let text = r.lines.join("\n");
    assert!(!text.contains("no installed addon provides vehicle"), "{text}");
    assert!(
        text.contains("track:venue/sochi"),
        "an id outside mod.sochi's own isn't provided by it: {text}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn lock_needs_and_data_files() {
    let root = scratch("data");
    let store = Store::new(root.join("data"));
    let dir = root.join("mod.esp");
    addon(&dir, "mod.esp");
    std::fs::write(
        dir.join(LOCK_FILE),
        r#"{"generated_by":"x","provides":{"engine_sound":["mod.esp/v10"]},
                "needs":[{"record":"engine_sound:mod.esp/v10","policy":"required"},
                         {"record":"livery:mod.l/red","policy":"optional"},
                         {"record":"livery:mod.l/blue","or":["engine_sound:mod.esp/v10"],"policy":"required"}]}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("registry")).unwrap();
    std::fs::write(
        dir.join("registry/layers.json"),
        r#"{"vehicle":{"swofty.classic-seasons-2017/ferrari":{"engine_sound":"mod.esp/v10","livery":null}}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("registry/bad-layers.json"),
        r#"{"vehicle":{"game/ferrari_2025":{"tyre_wear_scale":"fast"}}}"#,
    )
    .unwrap();
    let sdk = super::super::sdk_root().ok();
    let r = check(&store, &sdk, &dir).unwrap();
    let text = r.lines.join("\n");
    assert!(
        text.contains("warning: registry.lock.json is there but no DLL"),
        "{text}"
    );
    assert!(
        text.contains("warning: registry.lock.json: no installed addon provides livery:mod.l/red"),
        "{text}"
    );
    assert!(!text.contains("engine_sound:mod.esp/v10"), "{text}");
    assert!(!text.contains("mod.l/blue"), "a met fallback is met: {text}");
    if super::super::python().is_ok() {
        assert!(text.contains("ok: registry/layers.json"), "{text}");
        assert!(text.contains("error: registry/bad-layers.json"), "{text}");
        assert_eq!(r.errors, 1, "{text}");
    }
    std::fs::remove_dir_all(&root).ok();
}
