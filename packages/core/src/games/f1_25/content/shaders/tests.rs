use std::path::PathBuf;

use swofty_shaders::{Active, ActiveInstance, Origin, Target};

use crate::profile::Profile;
use crate::store::Store;

use super::compile::*;
use super::configs::*;
use super::live::*;
use super::*;

pub(crate) const SPARK: &str = r#"{
        "id": "spark", "name": "Spark", "description": "A test effect.",
        "params": [{ "id": "intensity", "type": "float", "default": 1.0, "min": 0, "max": 4,
                     "label": "Intensity", "doc": "How much." }],
        "passes": [{ "name": "apply", "shader": "spark.hlsl" }]
    }"#;

pub(crate) const SPARK_HLSL: &str = "#include \"swofty/params.hlsli\"\n\
        float4 effect(float2 uv, uint2 p) { return scene(uv) * param_intensity(); }\n";

pub(crate) fn temp(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-shaders-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    root
}

pub(crate) fn addon(store: &Store, id: &str, extra: &str, configs: &[(&str, &str)], effect: bool) {
    let dir = store.addons_dir().join(id);
    std::fs::create_dir_all(dir.join("shaders")).unwrap();
    std::fs::create_dir_all(dir.join("fx")).unwrap();
    let mut shaders = Vec::new();
    for (file, text) in configs {
        std::fs::write(dir.join("shaders").join(file), text).unwrap();
        let force = *file == "forced.swfx";
        shaders.push(format!(r#"{{ "config": "shaders/{file}", "force": {force} }}"#));
    }
    if effect {
        std::fs::write(dir.join("fx/spark.effect.json"), SPARK).unwrap();
        std::fs::write(dir.join("fx/spark.hlsl"), SPARK_HLSL).unwrap();
    }
    let effects = if effect {
        r#""effects": ["fx/spark.effect.json"],"#
    } else {
        ""
    };
    std::fs::write(
        dir.join("swofty.json"),
        format!(
            r#"{{ "schema_version": 1, "id": "{id}", "name": "{id} name", "version": "1.0.0",
                    "author": "t", {extra} {effects} "shaders": [{}] }}"#,
            shaders.join(",")
        ),
    )
    .unwrap();
}

const PLAIN: &str = "# keep me\nname = \"Plain\"\n\n[spark]\nintensity = 2.0\n";

#[test]
fn personal_configs_are_created_renamed_copied_and_deleted() {
    let root = temp("personal");
    let store = Store::new(&root);
    let r = create(&store, "My Look", Some(PLAIN)).unwrap();
    assert_eq!(r, ConfigRef::Personal("my-look".into()));
    assert_eq!(
        create(&store, "my look", None).unwrap_err().to_string(),
        "you already have a shader config named my look"
    );
    let text = read(&store, &r).unwrap();
    assert!(
        text.contains("# keep me") && text.contains("name = \"My Look\""),
        "{text}"
    );
    save(&store, &r, &format!("{text}# more\n")).unwrap();

    let mut p = store.profile("default").unwrap();
    set_active(&store, &mut p, Some(r.clone())).unwrap();
    assert_eq!(active(&store, &p), Some(r.clone()));

    let renamed = rename(&store, &r, "Bright").unwrap();
    assert_eq!(renamed, ConfigRef::Personal("bright".into()));
    let text = read(&store, &renamed).unwrap();
    assert!(text.contains("# keep me") && text.contains("# more") && text.contains("name = \"Bright\""));
    assert!(read(&store, &r).is_err(), "the old file is gone");
    let p = store.profile("default").unwrap();
    assert_eq!(
        p.shader.as_deref(),
        Some("personal:bright"),
        "the profile follows the rename"
    );

    let copy = copy_to_personal(&store, &renamed).unwrap();
    assert_eq!(copy, ConfigRef::Personal("bright-copy".into()));
    let names: Vec<_> = list(&store).into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["Bright", "Bright (copy)"]);

    let out = root.join("export.swfx");
    export(&store, &copy, &out).unwrap();
    let imported = import(&store, &out).unwrap();
    assert_eq!(
        read(&store, &imported).unwrap(),
        read(&store, &copy).unwrap().replace("(copy)", "(copy) (2)")
    );

    delete(&store, &renamed).unwrap();
    assert_eq!(store.profile("default").unwrap().shader, None);
    assert_eq!(active(&store, &store.profile("default").unwrap()), None);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn with_name_changes_only_the_name_line() {
    assert_eq!(with_name("a = 1\n", "X"), "name = \"X\"\na = 1\n");
    assert_eq!(
        with_name("# c\nname = 'Old'  # note\n[t]\nname = \"inner\"\n", "New \"q\""),
        "# c\nname = \"New \\\"q\\\"\"\n[t]\nname = \"inner\"\n"
    );
    assert_eq!(name_of("[t]\nname = \"inner\"\n", "stem"), "stem");
    assert_eq!(slug("  Hello, World!! "), "hello-world");
    assert_eq!(slug("!!!"), "config");
}

#[test]
fn addon_configs_are_listed_read_only_whether_the_addon_is_on_or_off() {
    let root = temp("addons");
    let store = Store::new(&root);
    addon(
        &store,
        "t.pack",
        r#""multiplayer": "cosmetic","#,
        &[("a.swfx", "name = \"Alpha\"\n"), ("b.swfx", "[spark]\n")],
        true,
    );
    addon(&store, "t.only", "", &[("o.swfx", "name = \"Only\"\n")], false);
    create(&store, "Mine", Some(PLAIN)).unwrap();

    let all = list(&store);
    let seen: Vec<_> = all
        .iter()
        .map(|e| (e.r#ref.to_string(), e.name.as_str(), e.editable))
        .collect();
    assert_eq!(
        seen,
        [
            ("personal:mine".to_owned(), "Mine", true),
            ("addon:t.only/o".to_owned(), "Only", false),
            ("addon:t.pack/a".to_owned(), "Alpha", false),
            ("addon:t.pack/b".to_owned(), "b", false),
        ]
    );
    assert_eq!(all[2].source, "t.pack name");
    let alpha: ConfigRef = "addon:t.pack/a".parse().unwrap();
    assert!(save(&store, &alpha, "x").is_err());
    assert!(delete(&store, &alpha).is_err() && rename(&store, &alpha, "x").is_err());
    let copy = copy_to_personal(&store, &alpha).unwrap();
    assert_eq!(copy, ConfigRef::Personal("alpha".into()));
    assert!(
        !store.profile("default").unwrap().has("t.pack"),
        "the addon was never turned on"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_forced_config_needs_its_addon_on_unless_the_addon_is_shader_only() {
    let root = temp("forced");
    let store = Store::new(&root);
    addon(
        &store,
        "t.mixed",
        r#""multiplayer": "cosmetic","#,
        &[("forced.swfx", "name = \"Forced\"\n")],
        false,
    );
    let mut p = store.profile("default").unwrap();
    assert_eq!(forced(&store, &p), None);
    p.set_enabled("t.mixed", true).unwrap();
    assert_eq!(
        forced(&store, &p),
        Some((
            ConfigRef::Addon {
                addon: "t.mixed".into(),
                file: "forced".into()
            },
            "t.mixed name".into()
        ))
    );
    p.set_enabled("t.mixed", false).unwrap();
    assert_eq!(forced(&store, &p), None);
    assert_eq!(forced(&store, &Profile::vanilla()), None);

    addon(
        &store,
        "t.only",
        "",
        &[("forced.swfx", "name = \"Only forced\"\n")],
        false,
    );
    let (r, name) = forced(&store, &p).unwrap();
    assert_eq!(
        (r.to_string(), name.as_str()),
        ("addon:t.only/forced".to_owned(), "t.only name")
    );
    assert_eq!(
        forced(&store, &Profile::vanilla()),
        None,
        "the vanilla profile has no shaders"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_toggle_key_defaults_to_b_and_is_checked() {
    let root = temp("key");
    let store = Store::new(&root);
    assert_eq!(toggle_key(&store), "B");
    set_toggle_key(&store, "f9").unwrap();
    assert_eq!(toggle_key(&store), "F9");
    set_toggle_key(&store, "none").unwrap();
    assert_eq!(toggle_key(&store), "None");
    assert!(set_toggle_key(&store, "ctrl").is_err());
    assert_eq!(toggle_key(&store), "None");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn push_live_replaces_active_json_atomically() {
    let root = temp("live");
    let active = Active {
        version: 1,
        serial: 7,
        name: "Realistic".into(),
        source: "personal:realistic".into(),
        forced_by: String::new(),
        toggle_key: "B".into(),
        instances: vec![ActiveInstance {
            effect: "spark".into(),
            label: String::new(),
            params: vec![2.0],
        }],
        rules: Vec::new(),
    };
    push_live(&root, &active).unwrap();
    push_live(
        &root,
        &Active {
            serial: 8,
            ..active.clone()
        },
    )
    .unwrap();
    let path = root.join("swofty/shaders/active.json");
    let back: Active = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(back.serial, 8);
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1,
        "no temp file is left"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_catalog_holds_every_installed_addons_effects_and_the_first_clash_wins() {
    let root = temp("catalog");
    let store = Store::new(&root);
    addon(&store, "t.one", "", &[], true);
    addon(&store, "t.two", r#""multiplayer": "cosmetic","#, &[], true);
    let loaded = load(&store);
    let ids: Vec<_> = loaded.catalog.effects.iter().map(|e| e.id.as_str()).collect();
    assert!(ids.contains(&"spark"), "{ids:?} {:?}", loaded.warnings);
    assert_eq!(ids.iter().filter(|i| **i == "spark").count(), 1);
    assert!(
        loaded
            .warnings
            .iter()
            .any(|w| w.contains("spark") && w.contains("already provides")),
        "{:?}",
        loaded.warnings
    );
    assert!(
        matches!(&loaded.catalog.get("spark").unwrap().origin, Origin::Addon { id, .. } if id == "t.one")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_active_file_resolves_the_chosen_config() {
    let root = temp("activefile");
    let store = Store::new(&root);
    addon(
        &store,
        "t.one",
        "",
        &[("a.swfx", "name = \"Alpha\"\n[spark]\nintensity = 2.5\n")],
        true,
    );
    let mut p = store.profile("default").unwrap();
    let none = active_file(&store, &p);
    assert_eq!(
        (none.name.as_str(), none.instances.len(), none.toggle_key.as_str()),
        ("", 0, "B")
    );
    let r = ConfigRef::Addon {
        addon: "t.one".into(),
        file: "a".into(),
    };
    set_active(&store, &mut p, Some(r.clone())).unwrap();
    let a = active_file(&store, &p);
    assert_eq!((a.name.as_str(), a.source.as_str()), ("Alpha", "addon:t.one/a"));
    assert_eq!(a.instances.len(), 1);
    assert_eq!(a.instances[0].params, [2.5]);
    assert!(a.serial > 0);
    assert!(a.rules.is_empty());

    let ruled = create(
            &store,
            "Ruled",
            Some("name = \"Ruled\"\n[spark]\nintensity = 2.5\n[[when]]\nname = \"lap\"\non = \"session.lap\"\n[when.set.spark]\nintensity = 4\n"),
        )
        .unwrap();
    set_active(&store, &mut p, Some(ruled)).unwrap();
    let a = active_file(&store, &p);
    assert_eq!(a.rules.len(), 1);
    assert_eq!(a.rules[0].name, "lap");
    assert!(a.rules[0].event);
    assert_eq!(a.rules[0].set[0].slot, Some(0));
    let json = serde_json::to_value(&a).unwrap();
    assert_eq!(json["rules"][0]["when"][0]["op"], "event");
    assert_eq!(json["rules"][0]["set"][0]["mode"], "blend");

    let bad = create(&store, "Bad", Some("[nothing]\n")).unwrap();
    set_active(&store, &mut p, Some(bad)).unwrap();
    assert!(
        active_file(&store, &p).instances.is_empty(),
        "a config with errors runs nothing"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_shader_only_addon_installs_from_a_folder_and_a_zip_and_removing_it_clears_the_profiles() {
    let root = temp("install");
    let store = Store::new(root.join("data"));
    let manifest = r#"{"schema_version":1,"id":"t.look","name":"Look","version":"1.0.0","author":"t",
            "category":"shaders","compat":{"game":"f1_25"},"shaders":[{"config":"look.swfx"}]}"#;
    let config = "name = \"Look\"\n[spark]\nintensity = 2.0\n";
    let folder = root.join("src/t.look");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("swofty.json"), manifest).unwrap();
    std::fs::write(folder.join("look.swfx"), config).unwrap();
    let zip = root.join("look.swoftyaddon");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("swofty.json", opts).unwrap();
    std::io::Write::write_all(&mut z, manifest.as_bytes()).unwrap();
    z.start_file("look.swfx", opts).unwrap();
    std::io::Write::write_all(&mut z, config.as_bytes()).unwrap();
    z.finish().unwrap();

    let r = ConfigRef::Addon {
        addon: "t.look".into(),
        file: "look".into(),
    };
    for src in [&folder, &zip] {
        assert!(store.install_addon(src).unwrap().is_shader_only());
        let listed: Vec<_> = list(&store).into_iter().map(|e| e.r#ref).collect();
        assert_eq!(listed, std::slice::from_ref(&r));
        let mut p = store.profile("default").unwrap();
        set_active(&store, &mut p, Some(r.clone())).unwrap();
        assert_eq!(active(&store, &p), Some(r.clone()));
        assert_eq!(read(&store, &r).unwrap(), config);
        store.remove_addon("t.look").unwrap();
        assert!(list(&store).is_empty());
        let p = store.profile("default").unwrap();
        assert_eq!(p.shader, None);
        assert_eq!(active(&store, &p), None);
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_catalog_effects_compile_to_dxil_when_dxc_is_at_hand() {
    if crate::content::postfx::dxc().is_none() {
        return;
    }
    let root = temp("compile");
    let store = Store::new(&root);
    addon(&store, "t.one", "", &[], true);
    let (compiled, warnings) = compile_all(&store, Target::Dxil);
    assert!(warnings.is_empty(), "{warnings:?}");
    let spark = compiled.iter().find(|c| c.effect.id == "spark").unwrap();
    assert!(spark.blobs.len() == 1 && spark.blobs[0].starts_with(b"DXBC"));
    std::fs::remove_dir_all(&root).ok();
}
