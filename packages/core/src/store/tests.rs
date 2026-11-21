use std::path::{Path, PathBuf};
use std::sync::Arc;

use swofty_addon::launcher::Builtin;

use super::scan::*;
use super::swap::*;
use super::*;
use crate::addons::builtin::CORE_ID;
use crate::addons::settings::{self, SessionSettings};
use crate::manifest::{AddonManifest, MANIFEST_FILE};
use crate::profile::{self, Profile};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-store-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn apply_fix_writes_the_order_and_the_folders_together() {
    use crate::profile::folders::Key;
    let root = tmp("apply-fix");
    let store = Store::new(&root);
    let mut p = store.profile(DEFAULT_PROFILE).unwrap();
    for id in ["t.a", "t.c", "t.b"] {
        p.set_enabled(id, true).unwrap();
    }
    store.save_profile(&p).unwrap();
    let plan = crate::profile::order::FixPlan {
        order: [CORE_ID, "t.a", "t.b", "t.c"].map(str::to_owned).to_vec(),
        ejected: vec![("t.c".to_owned(), Key::Own("f".into()))],
        moved: 2,
    };
    let mut f = crate::profile::folders::Folders::default();
    f.placed.insert("t.c".into(), Key::Own("f".into()).text());
    store.save_folders(&f).unwrap();
    let default = Key::Category(swofty_addon::manifest::Category::Seasons);
    store
        .apply_fix(DEFAULT_PROFILE, &plan, &|_| Some(default.clone()))
        .unwrap();
    let ids: Vec<String> = store
        .profile(DEFAULT_PROFILE)
        .unwrap()
        .addons
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(ids, plan.order);
    assert_eq!(store.folders().place("t.c", false, None), None);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn installs_template_addon_and_keeps_core() {
    let root = tmp("addons");
    let store = Store::new(&root);
    let template = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/addon");
    let m = store.install_addon(&template).unwrap();
    assert!(store.addons_dir().join(&m.id).join(MANIFEST_FILE).is_file());
    let addons = store.addons().unwrap();
    assert!(addons.contains_key(CORE_ID));
    assert!(addons.contains_key(&m.id));
    let mut p = store.profile(DEFAULT_PROFILE).unwrap();
    p.set_enabled(&m.id, true).unwrap();
    store.save_profile(&p).unwrap();
    let plan = crate::profile::resolver::resolve(&store.profile(DEFAULT_PROFILE).unwrap(), &addons).unwrap();
    assert_eq!(plan.order, [CORE_ID, m.id.as_str()]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_addon_in_a_running_build_is_not_replaced_or_removed() {
    let root = tmp("busy-addon");
    let store = Store::new(&root);
    let src = root.join("src");
    for id in ["t.used", "t.other"] {
        put_addon(&store, id, &manifest_json(id));
        std::fs::create_dir_all(src.join(id)).unwrap();
        std::fs::write(src.join(id).join(MANIFEST_FILE), manifest_json(id)).unwrap();
    }
    let busy = crate::game_folder::busy::Busy {
        pid: std::process::id().wrapping_add(1),
        ..crate::game_folder::busy::Busy::new("default", vec!["t.used".into()])
    };
    busy.write(&root).unwrap();
    let lock = crate::game_folder::recovery::SessionLock::try_acquire(&root)
        .unwrap()
        .unwrap();
    let err = store.install_addon(&src.join("t.used")).unwrap_err().to_string();
    assert!(err.contains("is in the build that is running"), "{err}");
    let err = store.remove_addon("t.used").unwrap_err().to_string();
    assert!(err.contains("is in the build that is running"), "{err}");
    assert!(store.addons_dir().join("t.used").join(MANIFEST_FILE).is_file());
    store.install_addon(&src.join("t.other")).unwrap();
    drop(lock);
    drop(
        crate::game_folder::recovery::SessionLock::wait(&root, std::time::Duration::from_secs(10))
            .unwrap()
            .expect("a forked child still holds the dropped lock"),
    );
    store.install_addon(&src.join("t.used")).unwrap();
    store.remove_addon("t.used").unwrap();
    std::fs::remove_dir_all(&root).ok();
}

fn put_addon(store: &Store, folder: &str, json: &str) -> PathBuf {
    let dir = store.addons_dir().join(folder);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(MANIFEST_FILE), json).unwrap();
    dir
}

#[test]
fn an_addon_for_a_newer_launcher_is_skipped_and_named() {
    let root = tmp("future-addon");
    let store = Store::new(&root);
    put_addon(&store, "t.good", &manifest_json("t.good"));
    put_addon(
        &store,
        "w3.hud",
        r#"{"schema_version":1,"id":"w3.hud","name":"Race HUD","version":"0.1.0","author":"t",
            "compat":{"game":"f1_25"},"hud":{"widgets":[]}}"#,
    );
    put_addon(
        &store,
        "t.look",
        r#"{"schema_version":1,"id":"t.look","name":"Look","version":"1.0.0","author":"t",
            "compat":{"game":"f1_25","launcher":">=9.0.0"},"experimental_features":["holograms"]}"#,
    );
    put_addon(&store, "t.broken", "{\"id\": 5}");
    let addons = store.addons().unwrap();
    assert!(addons.contains_key("t.good") && addons.contains_key(CORE_ID));
    assert!(!addons.contains_key("w3.hud") && !addons.contains_key("t.look"));
    let bad = store.unreadable_addons();
    let lines: Vec<String> = bad.iter().map(Unreadable::line).collect();
    let launcher = crate::updates::release::current_version();
    assert_eq!(
        lines,
        [
            "t.broken can't be read: invalid type: integer `5`, expected a string at line 1 column 8"
                .to_owned(),
            format!(
                "Look (t.look) needs Swofty Launcher >=9.0.0; this is {launcher} (unknown value `holograms`)"
            ),
            "Race HUD (w3.hud) needs a newer Swofty Launcher (unknown field `widgets`)".to_owned(),
        ]
    );
    assert_eq!(
        bad.iter().map(|u| u.newer).collect::<Vec<_>>(),
        [false, true, true]
    );
    assert_eq!(bad[2].version.as_deref(), Some("0.1.0"));

    store
        .update_profile(DEFAULT_PROFILE, |p| {
            p.set_enabled("t.good", true)?;
            p.set_enabled("w3.hud", true)
        })
        .unwrap();
    let registry = store.registry().unwrap();
    assert!(registry.get("t.good").is_some());
    let err = store
        .install_addon(&store.addons_dir().join("w3.hud"))
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Race HUD (w3.hud) needs a newer Swofty Launcher (unknown field `widgets`)"
    );
    store.remove_addon("w3.hud").unwrap();
    assert!(store.unreadable_addons().iter().all(|u| u.id != "w3.hud"));
    assert!(
        !store
            .profile(DEFAULT_PROFILE)
            .unwrap()
            .addons
            .iter()
            .any(|a| a.id == "w3.hud")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_profile_with_fields_from_a_newer_launcher_loads_and_keeps_them() {
    let root = tmp("future-profile");
    let store = Store::new(&root);
    put_addon(&store, "t.good", &manifest_json("t.good"));
    std::fs::create_dir_all(store.profiles_dir()).unwrap();
    std::fs::write(
        store.profiles_dir().join("default.json"),
        r#"{"schema_version":2,"name":"default","addons":[{"id":"t.good","enabled":true,"pinned":true}],
            "options":{},"overlay_off":["t.good"]}"#,
    )
    .unwrap();
    let p = store.profile(DEFAULT_PROFILE).unwrap();
    assert!(p.enabled().any(|id| id == "t.good"));
    assert_eq!(p.unknown_fields(), ["overlay_off", "addons[t.good].pinned"]);
    store
        .update_profile(DEFAULT_PROFILE, |p| p.set_enabled("t.good", false))
        .unwrap();
    let saved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(store.profiles_dir().join("default.json")).unwrap()).unwrap();
    assert_eq!(saved["overlay_off"], serde_json::json!(["t.good"]));
    let good = saved["addons"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "t.good")
        .unwrap();
    assert_eq!(good["pinned"], serde_json::json!(true));
    assert_eq!(good["enabled"], serde_json::json!(false));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn installs_addon_zips() {
    let root = tmp("addon-zips");
    let store = Store::new(root.join("data"));
    let manifest = r#"{"schema_version":1,"id":"t.zipped","name":"Zipped","version":"1.0.0","author":"t","compat":{"game":"f1_25"}}"#;
    for (file, prefix) in [("flat.swoftymod", ""), ("wrapped.zip", "zipped/")] {
        let path = root.join(file);
        let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        z.start_file(format!("{prefix}{MANIFEST_FILE}"), opts).unwrap();
        std::io::Write::write_all(&mut z, manifest.as_bytes()).unwrap();
        z.start_file(format!("{prefix}plugins/a.dll"), opts).unwrap();
        z.finish().unwrap();
        assert_eq!(store.install_addon(&path).unwrap().id, "t.zipped");
        assert!(store.addons_dir().join("t.zipped/plugins/a.dll").is_file());
        assert_eq!(scratch_dirs(&store), 0);
    }
    let bare = root.join("bare.zip");
    zip::ZipWriter::new(std::fs::File::create(&bare).unwrap())
        .finish()
        .unwrap();
    assert!(store.install_addon(&bare).is_err());
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_failed_install_keeps_the_previous_version() {
    let root = tmp("failed-install");
    let store = Store::new(&root);
    let good = root.join("good");
    std::fs::create_dir_all(good.join("track")).unwrap();
    std::fs::write(good.join(MANIFEST_FILE), manifest_json("a.sochi")).unwrap();
    std::fs::write(good.join("track/sochi.erp"), b"v1").unwrap();
    store.install_addon(&good).unwrap();
    let broken = root.join("broken");
    std::fs::create_dir_all(broken.join("track")).unwrap();
    std::fs::write(
        broken.join(MANIFEST_FILE),
        manifest_json("a.sochi").replace("1.0.0", "1.1.0"),
    )
    .unwrap();
    std::os::unix::fs::symlink(root.join("nowhere"), broken.join("track/sochi.erp")).unwrap();
    assert!(store.install_addon(&broken).is_err());
    let dest = store.addons_dir().join("a.sochi");
    assert_eq!(std::fs::read(dest.join("track/sochi.erp")).unwrap(), b"v1");
    assert_eq!(store.addons().unwrap()["a.sochi"].version.to_string(), "1.0.0");
    let left = std::fs::read_dir(store.addons_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".installing"))
        .count();
    assert_eq!(left, 0);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_failed_first_install_leaves_nothing_listed() {
    let root = tmp("failed-first-install");
    let store = Store::new(&root);
    let broken = root.join("broken");
    std::fs::create_dir_all(broken.join("track")).unwrap();
    std::fs::write(broken.join(MANIFEST_FILE), manifest_json("a.sochi")).unwrap();
    std::os::unix::fs::symlink(root.join("nowhere"), broken.join("track/sochi.erp")).unwrap();
    assert!(store.install_addon(&broken).is_err());
    assert!(!store.addons().unwrap().contains_key("a.sochi"));
    assert!(!store.addons_dir().join("a.sochi").exists());
    std::fs::remove_dir_all(&root).ok();
}

fn scratch_dirs(store: &Store) -> usize {
    std::fs::read_dir(store.addons_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".importing"))
        .count()
}

fn manifest_json(id: &str) -> String {
    format!(
        r#"{{"schema_version":1,"id":"{id}","name":"{id}","version":"1.0.0","author":"t","compat":{{"game":"f1_25"}}}}"#
    )
}

fn package(id: &str) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    z.start_file(MANIFEST_FILE, zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(&mut z, manifest_json(id).as_bytes()).unwrap();
    z.finish().unwrap().into_inner()
}

#[test]
fn installs_every_addon_a_rar_holds() {
    let root = tmp("addon-rars");
    let store = Store::new(root.join("data"));
    for (name, version) in [
        ("rar5.rar", rars::ArchiveVersion::Rar50),
        ("rar29.rar", rars::ArchiveVersion::Rar29),
    ] {
        let mut b = rars::Builder::new(version);
        let mut add = |path: &str, data: Vec<u8>| {
            b.add_bytes(path.as_bytes().to_vec(), data, None, None).unwrap();
        };
        add("Pack/t.packed.swoftyaddon", package("t.packed"));
        add(
            &format!("Pack/folder/{MANIFEST_FILE}"),
            manifest_json("t.folder").into_bytes(),
        );
        add("Pack/folder/plugins/a.dll", b"dll".to_vec());
        add("Pack/readme.txt", b"hi".to_vec());
        let rar = root.join(name);
        std::fs::write(&rar, b.to_bytes().unwrap()).unwrap();
        assert_eq!(
            crate::addons::import::addon_claim(&rar),
            swofty_addon::launcher::Claim::Yes
        );
        let ids: Vec<String> = store
            .install_addons(&rar)
            .into_iter()
            .map(|m| m.unwrap().id)
            .collect();
        assert_eq!(ids, ["t.folder", "t.packed"], "{name}");
        assert!(store.addons_dir().join("t.folder/plugins/a.dll").is_file());
        assert!(
            store
                .install_addon(&rar)
                .unwrap_err()
                .to_string()
                .contains("holds 2 addons")
        );
        assert_eq!(scratch_dirs(&store), 0);
    }
    let mut b = rars::Builder::new(rars::ArchiveVersion::Rar50);
    b.add_bytes(b"One/t.one.swoftyaddon".to_vec(), package("t.one"), None, None)
        .unwrap();
    let one = root.join("one.rar");
    std::fs::write(&one, b.to_bytes().unwrap()).unwrap();
    assert_eq!(store.install_addon(&one).unwrap().id, "t.one");
    let mut b = rars::Builder::new(rars::ArchiveVersion::Rar50);
    b.add_bytes(
        b"mod/2025_asset_groups/a.erp".to_vec(),
        b"erp".to_vec(),
        None,
        None,
    )
    .unwrap();
    let game_mod = root.join("mod.rar");
    std::fs::write(&game_mod, b.to_bytes().unwrap()).unwrap();
    assert_eq!(
        crate::addons::import::addon_claim(&game_mod),
        swofty_addon::launcher::Claim::No
    );
    let err = store.install_addons(&game_mod).remove(0).unwrap_err();
    assert!(err.to_string().contains("has no swofty.json"), "{err}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn only_a_managed_release_installs_under_the_managed_id() {
    let root = tmp("managed-claimed");
    let store = Store::new(root.join("data"));
    let fake = root.join("fake");
    write_addon(&fake, crate::updates::managed::ID);

    for err in [
        store.install_addon(&fake).unwrap_err(),
        store.install_addons(&fake).remove(0).unwrap_err(),
        store.install_addon_in_profile(&fake, "default").unwrap_err(),
    ] {
        assert!(err.to_string().contains("managed by the launcher"), "{err}");
    }
    assert!(!store.addons_dir().join(crate::updates::managed::ID).exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_listed_install_refuses_anything_else_before_replacing_an_addon() {
    let root = tmp("listed");
    let store = Store::new(root.join("data"));
    let one = root.join("one");
    write_addon(&one, "t.one");
    store.install_addon(&one).unwrap();
    let v1 = semver::Version::new(1, 0, 0);

    let other = root.join("other");
    write_addon(&other, "t.other");
    let err = store.install_listed(&other, None, "t.one", &v1).unwrap_err();
    assert!(
        err.to_string().contains("the registry lists t.one 1.0.0"),
        "{err}"
    );
    assert!(!store.addons_dir().join("t.other").exists());

    // A listed id that isn't what the download holds can't replace the addon it names, either.
    let err = store
        .install_listed(&one, None, "t.one", &semver::Version::new(2, 0, 0))
        .unwrap_err();
    assert!(
        err.to_string().contains("the download holds t.one 1.0.0"),
        "{err}"
    );

    assert_eq!(
        store.install_listed(&one, None, "t.one", &v1).unwrap().id,
        "t.one"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn manages_the_team_simplified_addon() {
    let root = tmp("managed-addon");
    let store = Store::new(root.join("data"));
    let manifest = r#"{"schema_version":1,"id":"teamsimplified.mods","name":"Simplified for Swofty","version":"0.5.0","author":"Team Simplified","compat":{"game":"f1_25"},"launcher":{"program":{"linux":"bin/helper","windows":"bin/helper.exe"}}}"#;
    let pkg = root.join("teamsimplified.mods.swoftyaddon");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&pkg).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file(MANIFEST_FILE, opts).unwrap();
    std::io::Write::write_all(&mut z, manifest.as_bytes()).unwrap();
    z.start_file("bin/helper", opts).unwrap();
    z.finish().unwrap();

    let m = store
        .install_managed_release(&pkg, &crate::updates::release::current_version())
        .unwrap();
    assert_eq!(m.id, crate::updates::managed::ID);
    assert!(store.addons().unwrap().contains_key(crate::updates::managed::ID));

    let err = store.remove_addon(crate::updates::managed::ID).unwrap_err();
    assert!(err.to_string().contains("managed"), "{err}");
    assert!(
        store
            .addons_dir()
            .join(crate::updates::managed::ID)
            .join(MANIFEST_FILE)
            .is_file()
    );

    let reg = store.registry().unwrap();
    assert!(reg.get(crate::updates::managed::ID).unwrap().locked);

    let p = store.profile(DEFAULT_PROFILE).unwrap();
    assert!(p.enabled().any(|id| id == crate::updates::managed::ID));
    std::fs::remove_dir_all(&root).ok();
}

fn write_addon(dir: &Path, id: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join(MANIFEST_FILE),
        format!(
            r#"{{"schema_version":1,"id":"{id}","name":"{id}","version":"1.0.0","author":"t","compat":{{"game":"f1_25"}}}}"#
        ),
    )
    .unwrap();
}

#[test]
fn migrate_renames_the_mods_dir() {
    let root = tmp("migrate-rename");
    let store = Store::new(&root);
    write_addon(&root.join("mods/a.one"), "a.one");
    std::fs::write(store.config_path(), r#"{"active_profile":"grid"}"#).unwrap();
    assert_eq!(store.migrate().unwrap().len(), 2);
    assert!(!root.join("mods").exists());
    assert!(store.addons().unwrap().contains_key("a.one"));
    let config = store.config();
    assert_eq!(config.data_version, DATA_VERSION);
    assert_eq!(config.active_profile.as_deref(), Some("grid"));
    write_addon(&root.join("mods/a.late"), "a.late");
    assert!(store.migrate().unwrap().is_empty());
    assert!(root.join("mods/a.late").is_dir());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn migrate_merges_into_an_existing_addons_dir() {
    let root = tmp("migrate-merge");
    let store = Store::new(&root);
    write_addon(&root.join("addons/a.one"), "a.one");
    write_addon(&root.join("mods/one-copy"), "a.one");
    write_addon(&root.join("mods/a.two"), "a.two");
    std::fs::create_dir_all(root.join("mods/stray")).unwrap();
    std::fs::write(store.config_path(), r#"{"data_version":"from a newer launcher"}"#).unwrap();
    let notes = store.migrate().unwrap();
    assert_eq!(notes.len(), 4, "{notes:?}");
    assert!(notes[3].starts_with("data_version not recorded"));
    assert!(root.join("addons/a.two").join(MANIFEST_FILE).is_file());
    assert!(root.join("mods/one-copy").is_dir() && root.join("mods/stray").is_dir());
    assert_eq!(
        store.addons().unwrap().keys().collect::<Vec<_>>(),
        ["a.one", "a.two", CORE_ID]
    );

    std::fs::remove_dir_all(root.join("mods/one-copy")).unwrap();
    std::fs::remove_dir_all(root.join("mods/stray")).unwrap();
    write_addon(&root.join("mods/a.three"), "a.three");
    std::fs::remove_file(store.config_path()).unwrap();
    assert_eq!(store.migrate().unwrap().len(), 3);
    assert!(!root.join("mods").exists());
    assert!(root.join("addons/a.three").is_dir());
    assert_eq!(store.config().data_version, DATA_VERSION);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_save_keeps_a_config_or_folders_file_this_build_cant_read() {
    let root = tmp("unreadable-config");
    let store = Store::new(&root);
    let config = r#"{"active_profile":"grid","launch":{"env":{"DXVK_ASYNC":1}}}"#;
    let folders = r#"{"placed":{"t.a":1}}"#;
    std::fs::write(store.config_path(), config).unwrap();
    std::fs::write(store.folders_path(), folders).unwrap();
    assert_eq!(store.config().active_profile, None);
    assert_eq!(store.folders(), crate::profile::folders::Folders::default());
    assert_eq!(std::fs::read_to_string(store.config_path()).unwrap(), config);
    assert_eq!(std::fs::read_to_string(store.folders_path()).unwrap(), folders);

    let aside = |name: &str| -> Vec<PathBuf> {
        let prefix = format!("{name}{}", crate::UNREADABLE_SUFFIX);
        std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&prefix))
            .collect()
    };
    for round in ["other", "third"] {
        store.set_active_profile(round).unwrap();
        store.save_folders(&store.folders()).unwrap();
        assert_eq!(store.config().active_profile.as_deref(), Some(round));
        let kept = aside("config.json");
        assert_eq!(kept.len(), 1, "{round}: {kept:?}");
        assert_eq!(std::fs::read_to_string(&kept[0]).unwrap(), config, "{round}");
        let kept = aside(crate::profile::folders::FILE);
        assert_eq!(kept.len(), 1, "{round}: {kept:?}");
        assert_eq!(std::fs::read_to_string(&kept[0]).unwrap(), folders, "{round}");
    }
    std::fs::remove_dir_all(&root).ok();
}

fn v3_store(name: &str, ids: &[&str]) -> (PathBuf, Store) {
    let root = tmp(name);
    let store = Store::new(&root);
    for id in ids {
        write_addon(&root.join("addons").join(id), id);
    }
    std::fs::write(
        store.config_path(),
        r#"{"data_version":3,"active_profile":"grid"}"#,
    )
    .unwrap();
    (root, store)
}

fn listed(store: &Store, name: &str) -> Vec<(String, bool)> {
    store
        .profile(name)
        .unwrap()
        .addons
        .iter()
        .filter(|a| a.id != CORE_ID && !store.is_builtin(&a.id))
        .map(|a| (a.id.clone(), a.enabled))
        .collect()
}

#[test]
fn migrate_lists_the_installed_addons_a_profile_lacks() {
    let (root, store) = v3_store("backfill", &["a.e", "a.b", "a.d", "a.a", "a.c"]);
    store
        .update_profile("grid", |p| {
            p.set_enabled("a.d", true)?;
            p.set_enabled("a.b", false)
        })
        .unwrap();
    store.save_profile(&Profile::new("other")).unwrap();
    let builtins_before = store.profile("grid").unwrap().addons.len() - 2;
    store.migrate().unwrap();
    assert_eq!(store.config().data_version, DATA_VERSION);
    let want = [
        ("a.d", true),
        ("a.b", false),
        ("a.a", false),
        ("a.c", false),
        ("a.e", false),
    ];
    let got = listed(&store, "grid");
    assert_eq!(got, want.map(|(i, e)| (i.to_owned(), e)));
    assert_eq!(store.profile("grid").unwrap().addons.len(), builtins_before + 5);
    assert_eq!(listed(&store, "other").len(), 5);
    assert!(store.profile(profile::VANILLA).unwrap().is_vanilla());
    assert!(!store.profiles_dir().join("vanilla.json").exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_install_lists_the_addon_off_in_every_profile_and_a_removal_clears_it_everywhere() {
    let root = tmp("everywhere");
    let store = Store::new(root.join("data"));
    store.save_profile(&Profile::new("default")).unwrap();
    store.save_profile(&Profile::new("grid")).unwrap();
    let src = root.join("one");
    write_addon(&src, "t.one");
    store.install_addon(&src).unwrap();
    for name in ["default", "grid"] {
        assert_eq!(
            listed(&store, name).last(),
            Some(&("t.one".to_owned(), false)),
            "{name}"
        );
    }
    store
        .update_profile("default", |p| p.set_enabled("t.one", true))
        .unwrap();
    assert_eq!(
        listed(&store, "default").last(),
        Some(&("t.one".to_owned(), true))
    );
    assert_eq!(listed(&store, "grid").last(), Some(&("t.one".to_owned(), false)));
    let two = root.join("two");
    write_addon(&two, "t.two");
    store.install_addon(&two).unwrap();
    store
        .update_profile("grid", |p| {
            p.set_enabled("t.two", true)?;
            p.set_enabled("t.one", true)
        })
        .unwrap();
    let ids = |name: &str| -> Vec<String> { listed(&store, name).into_iter().map(|(id, _)| id).collect() };
    assert_eq!(ids("default"), ["t.one", "t.two"]);
    assert_eq!(ids("grid"), ["t.one", "t.two"]);
    store.remove_addon("t.one").unwrap();
    assert_eq!(ids("default"), ["t.two"]);
    assert_eq!(ids("grid"), ["t.two"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_new_profile_lists_every_installed_addon_off() {
    let root = tmp("new-profile-all");
    let store = Store::new(root.join("data"));
    store.save_profile(&Profile::new("default")).unwrap();
    let src = root.join("one");
    write_addon(&src, "t.one");
    store.install_addon(&src).unwrap();
    store.create_profile("fresh", None).unwrap();
    assert_eq!(listed(&store, "fresh").last(), Some(&("t.one".to_owned(), false)));
    let enabled: Vec<String> = store
        .profile("fresh")
        .unwrap()
        .enabled()
        .map(str::to_owned)
        .collect();
    assert_eq!(enabled, ["swofty.core"]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_profile_that_lacked_an_addon_loads_unchanged_and_the_on_set_is_what_it_was() {
    let mut p = Profile::new("old");
    p.set_enabled("t.a", true).unwrap();
    p.set_enabled("t.b", false).unwrap();
    let json = serde_json::to_string(&p).unwrap();
    let back: Profile = serde_json::from_str(&json).unwrap();
    assert!(!back.has("t.c"));
    assert_eq!(back.enabled().collect::<Vec<_>>(), ["swofty.core", "t.a"]);
    let mut grown = back.clone();
    grown.add_addon("t.c").unwrap();
    assert_eq!(
        grown.enabled().collect::<Vec<_>>(),
        back.enabled().collect::<Vec<_>>()
    );
}

#[test]
fn migrate_backfill_runs_once() {
    let (root, store) = v3_store("backfill-once", &["a.one", "a.two"]);
    store.save_profile(&Profile::new("grid")).unwrap();
    store.migrate().unwrap();
    let path = store.profiles_dir().join("grid.json");
    let after = std::fs::read(&path).unwrap();
    assert!(store.migrate().unwrap().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), after);

    store.update_profile("grid", |p| p.remove_addon("a.two")).unwrap();
    let removed = std::fs::read(&path).unwrap();
    assert!(store.migrate().unwrap().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), removed);
    assert_eq!(listed(&store, "grid"), [("a.one".to_owned(), false)]);

    std::fs::write(store.config_path(), r#"{"data_version":3}"#).unwrap();
    store.migrate().unwrap();
    assert_eq!(listed(&store, "grid").len(), 2);
    let again = std::fs::read(&path).unwrap();
    std::fs::write(store.config_path(), r#"{"data_version":3}"#).unwrap();
    store.migrate().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), again);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn migrate_leaves_a_missing_data_dir_alone() {
    let root = tmp("migrate-missing").join("data");
    assert!(Store::new(&root).migrate().unwrap().is_empty());
    assert!(!root.exists());
}

#[test]
fn reads_profiles_that_list_mods_and_writes_addons() {
    let root = tmp("old-profile");
    let store = Store::new(&root);
    let path = store.profiles_dir().join("grid.json");
    std::fs::create_dir_all(store.profiles_dir()).unwrap();
    std::fs::write(
        &path,
        r#"{"schema_version":1,"name":"grid","mods":[{"id":"swofty.core"},{"id":"a.b","enabled":false},{"id":"c.d"}],"options":{"c.d":{"x":1}}}"#,
    )
    .unwrap();
    let p = store.profile("grid").unwrap();
    assert_eq!(p.enabled().collect::<Vec<_>>(), [CORE_ID, "c.d"]);
    store.save_profile(&p).unwrap();
    let saved: serde_json::Value = crate::read_json(&path).unwrap();
    assert_eq!(saved["schema_version"], profile::SCHEMA_VERSION);
    assert_eq!(
        saved["addons"][1],
        serde_json::json!({"id": "a.b", "enabled": false})
    );
    assert!(saved.get("mods").is_none());
    assert_eq!(saved["options"]["c.d"]["x"], 1);
    assert_eq!(store.profile("grid").unwrap(), p);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn merges_in_game_requests_into_the_profile() {
    let root = tmp("requests");
    let store = Store::new(&root);
    let path = store.requests_path(DEFAULT_PROFILE);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        &path,
        r#"{"schema":1,"profile":"default","seq":1,"options":{"swofty.core":{"show_launcher_panel":false}}}"#,
    )
    .unwrap();
    let out = store.merge_requests(DEFAULT_PROFILE, false).unwrap().unwrap();
    assert_eq!(out.changed, ["swofty.core.show_launcher_panel = Off"]);
    assert!(path.is_file());
    let core = crate::addons::builtin::core_manifest();
    let p = store.profile(DEFAULT_PROFILE).unwrap();
    assert_eq!(p.option(&core, &core.options[0]), false);
    let again = store.merge_requests(DEFAULT_PROFILE, true).unwrap().unwrap();
    assert!(again.changed.is_empty());
    assert!(!path.exists());
    assert_eq!(store.merge_requests(DEFAULT_PROFILE, true).unwrap(), None);
    let s = store.session_settings(DEFAULT_PROFILE).unwrap();
    assert_eq!(s.addons[0].options[0].current, 0);
    assert!(s.requests.unwrap().ends_with("default.json"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn leaves_builtins_without_options_out_of_addon_settings() {
    let root = tmp("builtin-settings");
    let parse = |json: &str| AddonManifest::parse(json).unwrap();
    let builtin = |m| Builtin {
        addon: Arc::new(crate::addons::registry::DeclaredAddon::new(m)),
        locked: false,
        default_on: true,
    };
    let tuned = || {
        builtin(parse(
            r#"{"schema_version":1,"id":"t.tuned","name":"Tuned","version":"1.0.0","author":"t","compat":{"game":"f1_25"},
                "options":[{"id":"on","type":"toggle","label":"On","default":true,"binds":"native"}]}"#,
        ))
    };
    let quiet = || {
        vec![
            builtin(parse(
                r#"{"schema_version":1,"id":"t.library","name":"Library","version":"1.0.0","author":"t","compat":{"game":"f1_25"}}"#,
            )),
            builtin(parse(
                r#"{"schema_version":1,"id":"t.native","name":"Native","version":"1.0.0","author":"t","compat":{"game":"f1_25"},
                    "operations":{"native":[{"dll":"native/n.dll"}]}}"#,
            )),
            builtin(parse(
                r#"{"schema_version":1,"id":"t.pages","name":"Pages","version":"1.0.0","author":"t","compat":{"game":"f1_25"},
                    "operations":{"screens":[{"name":"pages_screen","file":"ui/p.ui"}]}}"#,
            )),
        ]
    };
    write_addon(&Store::new(&root).addons_dir().join("a.user"), "a.user");
    crate::write_json(
        &crate::saves::launcher::marker_path(&root),
        &crate::saves::launcher::Marker {
            at: 0,
            launcher: String::new(),
            profiles: Vec::new(),
            note: String::new(),
            seen: true,
        },
    )
    .unwrap();
    let plain = Store::new(&root).with_builtins(vec![tuned()]);
    let mut expected = SessionSettings::build(
        &plain.profile(DEFAULT_PROFILE).unwrap(),
        &plain.addons().unwrap(),
        Some(crate::data::paths::game_folder_view(
            &plain.requests_path(DEFAULT_PROFILE),
        )),
    )
    .unwrap();
    for a in &mut expected.addons {
        a.folder = plain.is_builtin(&a.id).then(|| "Built In".to_owned());
    }
    expected.launcher_saves = Some(settings::SessionSaves {
        profile: DEFAULT_PROFILE.into(),
        remote: crate::data::paths::game_folder_view(&crate::saves::launcher::remote_dir(
            &plain.root,
            DEFAULT_PROFILE,
        )),
    });
    expected.plain_saves = Some(settings::SessionPlainSaves {
        root: crate::data::paths::game_folder_view(&crate::saves::plain::root_of_profile(
            &plain.root,
            DEFAULT_PROFILE,
        )),
        schema: 1,
    });
    let plain_json = serde_json::to_vec(&plain.session_settings(DEFAULT_PROFILE).unwrap()).unwrap();
    assert_eq!(plain_json, serde_json::to_vec(&expected).unwrap());

    let mut all = quiet();
    all.push(tuned());
    let store = Store::new(&root).with_builtins(all);
    let p = store.profile(DEFAULT_PROFILE).unwrap();
    assert_eq!(
        p.enabled().collect::<Vec<_>>(),
        [CORE_ID, "t.library", "t.native", "t.pages", "t.tuned"]
    );
    let s = store.session_settings(DEFAULT_PROFILE).unwrap();
    let ids: Vec<&str> = s.addons.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, [CORE_ID, "t.tuned", "a.user"]);
    let screens: Vec<&str> =
        s.ui.as_ref()
            .unwrap()
            .screens
            .iter()
            .map(|x| x.name.as_str())
            .collect();
    assert_eq!(screens, ["pages_screen"]);
    let mut without_pages = s.clone();
    without_pages.ui = None;
    assert_eq!(serde_json::to_vec(&without_pages).unwrap(), plain_json);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_profile_keeps_playing_on_the_steam_save_until_an_import_finds_one() {
    let root = tmp("steam-pending");
    let store = Store::new(&root);
    assert!(
        store
            .session_settings(DEFAULT_PROFILE)
            .unwrap()
            .launcher_saves
            .is_none()
    );
    let remote = crate::saves::launcher::remote_dir(&root, DEFAULT_PROFILE);
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join("savegame@profile#X"), b"x").unwrap();
    assert!(
        store
            .session_settings(DEFAULT_PROFILE)
            .unwrap()
            .launcher_saves
            .is_some()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_session_on_launcher_saves_hands_the_core_the_profiles_plain_root() {
    let root = tmp("plain-root");
    let store = Store::new(&root);
    let remote = crate::saves::launcher::remote_dir(&root, DEFAULT_PROFILE);
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join("savegame@profile#X"), b"x").unwrap();
    let s = store.session_settings(DEFAULT_PROFILE).unwrap();
    let plain = s.plain_saves.unwrap();
    assert_eq!(plain.schema, 1);
    assert!(plain.root.ends_with("saves/default/plain") || plain.root.ends_with("saves\\default\\plain"));
    assert!(
        serde_json::to_string(&store.session_settings(DEFAULT_PROFILE).unwrap())
            .unwrap()
            .contains("\"plain_saves\"")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_ea_app_session_on_launcher_saves_hands_the_core_the_ea_layouts_plain_root() {
    let root = tmp("plain-root-ea");
    let store = Store::new(&root);
    std::fs::create_dir_all(crate::saves::launcher::root(&root)).unwrap();
    std::fs::write(crate::saves::launcher::marker_path(&root), b"{}").unwrap();
    let p = store.profile(DEFAULT_PROFILE).unwrap();
    let ea = store
        .session_settings_on(&p, crate::install::Source::EaApp)
        .unwrap();
    let steam = store
        .session_settings_on(&p, crate::install::Source::Steam)
        .unwrap();
    let view = |path: &std::path::Path| crate::data::paths::game_folder_view(path);
    assert_eq!(
        ea.plain_saves.as_ref().unwrap().root,
        view(&crate::saves::plain::root_of_source(&root, "launcher-ea.default"))
    );
    assert!(
        ea.plain_saves
            .as_ref()
            .unwrap()
            .root
            .ends_with("saves/default/ea-plain")
            || ea
                .plain_saves
                .as_ref()
                .unwrap()
                .root
                .ends_with("saves\\default\\ea-plain")
    );
    assert_eq!(
        steam.plain_saves.as_ref().unwrap().root,
        view(&crate::saves::plain::root_of_source(&root, "launcher-default"))
    );
    assert_eq!(
        ea.launcher_saves, steam.launcher_saves,
        "the core finds the EA title folder beside remote"
    );
    let json: serde_json::Value = serde_json::to_value(&ea).unwrap();
    assert_eq!(json["plain_saves"]["root"], ea.plain_saves.unwrap().root);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_session_on_the_copys_own_save_names_its_plain_root_once_the_launcher_found_the_copy() {
    let root = tmp("plain-copy");
    let store = Store::new(&root);
    assert!(
        store
            .session_settings(DEFAULT_PROFILE)
            .unwrap()
            .plain_saves
            .is_none()
    );
    crate::saves::plain::remember_copy_source(&root, "steam-843502855");
    let plain = store
        .session_settings(DEFAULT_PROFILE)
        .unwrap()
        .plain_saves
        .unwrap();
    assert!(plain.root.ends_with("plain/steam-843502855") || plain.root.ends_with("plain\\steam-843502855"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn manages_profiles() {
    let root = tmp("profiles");
    let store = Store::new(&root);
    assert_eq!(store.profile_names().unwrap(), ["vanilla", "default"]);
    assert_eq!(store.active_profile(), "default");
    let mut p = store.profile(DEFAULT_PROFILE).unwrap();
    p.set_enabled("a.b", true).unwrap();
    store.save_profile(&p).unwrap();
    store.set_active_profile(DEFAULT_PROFILE).unwrap();
    store.create_profile("grid", Some(DEFAULT_PROFILE)).unwrap();
    assert!(store.create_profile("grid", None).is_err());
    assert!(store.create_profile("vanilla", None).is_err());
    store.rename_profile(DEFAULT_PROFILE, "main").unwrap();
    assert_eq!(store.profile_names().unwrap(), ["vanilla", "grid", "main"]);
    assert_eq!(store.active_profile(), "main");
    let grid = store.profile("grid").unwrap();
    assert_eq!(grid.enabled().collect::<Vec<_>>(), [CORE_ID, "a.b"]);
    std::fs::remove_dir_all(&root).ok();
}

fn touch(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "x").unwrap();
}

#[test]
fn deletes_a_profile_with_its_own_files_only() {
    let root = tmp("delete-profile");
    let store = Store::new(&root);
    write_addon(&store.addons_dir().join("a.user"), "a.user");
    store.create_profile("grid", None).unwrap();
    store.create_profile("main", None).unwrap();
    store.set_active_profile("main").unwrap();
    let own = [
        store.overlay_dir("grid", "h1").join(crate::build::PLAN_FILE),
        store.requests_path("grid"),
        store.saves_dir("grid").join("career.sav"),
    ];
    let kept = [
        store.overlay_dir("main", "h2").join(crate::build::PLAN_FILE),
        store.requests_path("main"),
        store.saves_dir("main").join("career.sav"),
        root.join("cache/maps/okayama/a1/.done"),
        store.addon_data_dir("a.user").join("library.json"),
    ];
    own.iter().chain(&kept).for_each(|f| touch(f));

    let plan = store.plan_profile_deletion("grid").unwrap();
    assert_eq!(plan.switch_to, None);
    assert!(plan.saves);
    assert_eq!(
        plan.paths,
        [
            store.overlays_dir("grid"),
            store.requests_path("grid"),
            store.saves_dir("grid"),
            store.profiles_dir().join("grid.json"),
        ]
    );
    assert!(own.iter().all(|f| f.is_file()));

    assert_eq!(store.delete_profile("grid").unwrap(), plan);
    assert_eq!(store.profile_names().unwrap(), ["vanilla", "main"]);
    assert!(plan.paths.iter().all(|p| !p.exists()));
    for f in &kept {
        assert!(f.is_file(), "{} was removed", f.display());
    }
    assert!(store.addons().unwrap().contains_key("a.user"));
    assert_eq!(store.active_profile(), "main");
    let err = store.delete_profile("grid").unwrap_err();
    assert!(err.to_string().contains("no profile named grid"), "{err}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_vr_choice_follows_its_profile() {
    use crate::session::LaunchMode;
    let root = tmp("vr-profile");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.create_profile("main", None).unwrap();
    let mut config = store.config();
    config.set_launch_mode("grid", LaunchMode::Vr);
    store.save_config(&config).unwrap();
    store.rename_profile("grid", "vrgrid").unwrap();
    let config = store.config();
    assert_eq!(config.launch_mode("vrgrid"), LaunchMode::Vr);
    assert_eq!(config.launch_mode("grid"), LaunchMode::Normal);
    store.delete_profile("vrgrid").unwrap();
    assert!(store.config().vr_profiles.is_empty());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_renamed_profile_keeps_its_vault_careers_and_import_record() {
    let root = tmp("rename-vault");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    let vault = root.join("career-vault").join("launcher-grid");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("marker"), "x").unwrap();
    let saves = root.join("saves");
    std::fs::create_dir_all(&saves).unwrap();
    std::fs::write(
        saves.join("imported.json"),
        r#"{"at":1,"launcher":"1","profiles":["grid","other"]}"#,
    )
    .unwrap();
    let careers = root.join("careers");
    std::fs::create_dir_all(&careers).unwrap();
    std::fs::write(
        careers.join("index.json"),
        r#"{"version":1,"careers":[{"career":"a","profile":"grid"},{"career":"b","profile":"other"}]}"#,
    )
    .unwrap();
    store.rename_profile("grid", "vrgrid").unwrap();
    assert!(!vault.exists());
    assert!(
        root.join("career-vault")
            .join("launcher-vrgrid")
            .join("marker")
            .is_file()
    );
    let marker = crate::saves::launcher::marker(&root).unwrap();
    assert_eq!(marker.profiles, ["vrgrid", "other"]);
    let index = crate::stdlib::careers::read(&crate::stdlib::careers::data_path(&root));
    assert_eq!(index[0].profile.as_deref(), Some("vrgrid"));
    assert_eq!(index[1].profile.as_deref(), Some("other"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn deleting_a_profile_keeps_its_versions_in_the_vault() {
    let root = tmp("delete-keeps-vault");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.create_profile("main", None).unwrap();
    let vault = root.join("career-vault").join("launcher-grid");
    std::fs::create_dir_all(&vault).unwrap();
    let plan = store.plan_profile_deletion("grid").unwrap();
    assert!(!plan.paths.contains(&vault));
    store.delete_profile("grid").unwrap();
    assert!(vault.is_dir());
    assert!(!store.profile_names().unwrap().iter().any(|n| n == "grid"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_rename_onto_a_deleted_profiles_name_keeps_both_vaults() {
    let root = tmp("rename-onto-deleted-vault");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.create_profile("main", None).unwrap();
    let vaults = root.join("career-vault");
    for prefix in ["launcher-", "launcher-ea."] {
        for name in ["grid", "main"] {
            let vault = vaults.join(format!("{prefix}{name}"));
            std::fs::create_dir_all(&vault).unwrap();
            std::fs::write(vault.join("marker"), format!("{name}'s")).unwrap();
        }
    }
    store.delete_profile("grid").unwrap();
    store.rename_profile("main", "grid").unwrap();
    for prefix in ["launcher-", "launcher-ea."] {
        let read = |vault: &Path| std::fs::read_to_string(vault.join("marker")).unwrap();
        assert_eq!(read(&vaults.join(format!("{prefix}grid"))), "main's", "{prefix}");
        assert!(!vaults.join(format!("{prefix}main")).exists(), "{prefix}");
        let aside: Vec<PathBuf> = std::fs::read_dir(&vaults)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("{prefix}grid.deleted."))
            })
            .collect();
        assert_eq!(aside.len(), 1, "{prefix}: {aside:?}");
        assert_eq!(read(&aside[0]), "grid's", "{prefix}");
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn keeps_vanilla_and_the_last_profile() {
    let root = tmp("delete-last");
    let store = Store::new(&root);
    let err = store.delete_profile(DEFAULT_PROFILE).unwrap_err();
    assert!(err.to_string().contains("only profile"), "{err}");
    store.create_profile("grid", None).unwrap();
    let err = store.delete_profile("grid").unwrap_err();
    assert!(err.to_string().contains("only profile"), "{err}");
    let err = store.delete_profile(profile::VANILLA).unwrap_err();
    assert!(err.to_string().contains("built in"), "{err}");
    assert!(store.delete_profile("../config").is_err());
    assert!(store.profiles_dir().join("grid.json").is_file());
    store.create_profile("main", None).unwrap();
    let plan = store.plan_profile_deletion("grid").unwrap();
    assert!(!plan.saves);
    assert_eq!(plan.paths, [store.profiles_dir().join("grid.json")]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn deleting_the_active_profile_switches_first() {
    let root = tmp("delete-active");
    let store = Store::new(&root);
    store
        .save_profile(&store.profile(DEFAULT_PROFILE).unwrap())
        .unwrap();
    store.create_profile("grid", None).unwrap();
    store.create_profile("zeta", None).unwrap();
    store.set_active_profile("grid").unwrap();
    crate::write_json(
        &store.launch_request_path(),
        &serde_json::json!({"id": "r1", "profile": "grid", "apply": true}),
    )
    .unwrap();

    let plan = store.delete_profile("grid").unwrap();
    assert_eq!(plan.switch_to.as_deref(), Some(DEFAULT_PROFILE));
    assert_eq!(store.config().active_profile.as_deref(), Some(DEFAULT_PROFILE));
    assert_eq!(store.active_profile(), DEFAULT_PROFILE);
    let request: serde_json::Value = crate::read_json(&store.launch_request_path()).unwrap();
    assert_eq!(request, serde_json::json!({"id": "r1", "apply": true}));

    store.create_profile("alpha", None).unwrap();
    crate::write_json(
        &store.launch_request_path(),
        &serde_json::json!({"id": "r2", "profile": "zeta"}),
    )
    .unwrap();
    let plan = store.delete_profile(DEFAULT_PROFILE).unwrap();
    assert_eq!(plan.switch_to.as_deref(), Some("alpha"));
    assert_eq!(store.active_profile(), "alpha");
    assert_eq!(store.profile_names().unwrap(), ["vanilla", "alpha", "zeta"]);
    let request: serde_json::Value = crate::read_json(&store.launch_request_path()).unwrap();
    assert_eq!(request["profile"], "zeta");

    assert_eq!(store.delete_profile("zeta").unwrap().switch_to, None);
    assert_eq!(store.active_profile(), "alpha");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_name_that_differs_only_in_capitals_is_taken() {
    let root = tmp("create-capitals");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    for name in ["Grid", "GRID", "Vanilla"] {
        let err = store.create_profile(name, None).unwrap_err();
        assert!(err.to_string().contains("already exists"), "{name}: {err}");
    }
    assert!(!store.profile_names().unwrap().contains(&"Grid".to_owned()));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_profile_named_with_a_leading_dash_keeps_working_but_no_new_one_gets_that_name() {
    let root = tmp("leading-dash");
    let store = Store::new(&root);
    // Earlier launchers let a profile be named this way.
    store.save_profile(&Profile::new("-swofty")).unwrap();
    store.set_active_profile("-swofty").unwrap();
    touch(&store.saves_dir("-swofty").join("remote/save"));

    assert!(store.profile_names().unwrap().contains(&"-swofty".to_owned()));
    assert_eq!(store.active_profile(), "-swofty");
    assert_eq!(store.profile("-swofty").unwrap().name, "-swofty");

    for name in ["-grid", "--profile"] {
        let err = store.create_profile(name, None).unwrap_err();
        assert!(err.to_string().contains("starts with -"), "{name}: {err}");
        let err = store.create_profile(name, Some("-swofty")).unwrap_err();
        assert!(err.to_string().contains("starts with -"), "{name}: {err}");
        let err = store.rename_profile("-swofty", name).unwrap_err();
        assert!(err.to_string().contains("starts with -"), "{name}: {err}");
    }
    let err = store.rename_profile("-swofty", "-Swofty").unwrap_err();
    assert!(err.to_string().contains("starts with -"), "{err}");
    assert!(store.profiles_dir().join("-swofty.json").is_file());
    assert!(store.saves_dir("-swofty").join("remote/save").is_file());

    assert_eq!(store.rename_profile("-swofty", "swofty").unwrap().name, "swofty");
    assert_eq!(store.active_profile(), "swofty");
    assert!(store.saves_dir("swofty").join("remote/save").is_file());
    assert!(!store.profile_names().unwrap().contains(&"-swofty".to_owned()));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn renames_a_profile_to_other_capitals() {
    let root = tmp("rename-capitals");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.set_active_profile("grid").unwrap();
    touch(&store.saves_dir("grid").join("remote/save"));

    assert_eq!(store.rename_profile("grid", "Grid").unwrap().name, "Grid");

    let names = store.profile_names().unwrap();
    assert!(names.contains(&"Grid".to_owned()), "{names:?}");
    assert!(!names.contains(&"grid".to_owned()), "{names:?}");
    assert!(!names.iter().any(|n| n.starts_with("renaming")), "{names:?}");
    assert_eq!(store.profile("Grid").unwrap().name, "Grid");
    assert_eq!(store.active_profile(), "Grid");
    assert!(store.saves_dir("Grid").join("remote/save").is_file());
    std::fs::remove_dir_all(&root).ok();
}

// Both names can only exist where file names are case-sensitive.
#[cfg(target_os = "linux")]
#[test]
fn a_rename_to_the_capitals_of_a_profile_that_exists_changes_nothing() {
    let root = tmp("rename-both");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    // Only a case-sensitive folder from before names were compared this way holds both.
    store.save_profile(&Profile::new("Grid")).unwrap();

    let err = store.rename_profile("grid", "Grid").unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    let names = store.profile_names().unwrap();
    assert!(names.contains(&"grid".to_owned()) && names.contains(&"Grid".to_owned()));
    assert!(!names.iter().any(|n| n.starts_with("renaming")), "{names:?}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn refuses_renaming_a_profile_installed_in_the_game_folder() {
    let root = tmp("rename-applied");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.set_active_profile("grid").unwrap();
    touch(&store.saves_dir("grid").join("remote/save"));
    let j = crate::game_folder::journal::Journal::create(&root, "grid", "build-1", true).unwrap();

    let err = store.rename_profile("grid", "main").unwrap_err();
    assert!(err.to_string().contains("installed in the game folder"), "{err}");
    assert!(err.to_string().contains(&j.id), "{err}");
    assert!(store.profiles_dir().join("grid.json").is_file());
    assert!(!store.profiles_dir().join("main.json").exists());
    assert!(store.saves_dir("grid").join("remote/save").is_file());
    assert_eq!(store.active_profile(), "grid");

    j.close().unwrap();
    store.rename_profile("grid", "main").unwrap();
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_rename_onto_saves_left_behind_changes_nothing() {
    let root = tmp("rename-orphan");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.set_active_profile("grid").unwrap();
    touch(&store.saves_dir("grid").join("remote/save"));
    touch(&store.saves_dir("main").join("remote/old"));

    let err = store.rename_profile("grid", "main").unwrap_err();
    assert!(err.to_string().contains("already holds saves"), "{err}");
    assert!(store.profiles_dir().join("grid.json").is_file());
    assert!(!store.profiles_dir().join("main.json").exists());
    assert!(store.saves_dir("grid").join("remote/save").is_file());
    assert!(store.saves_dir("main").join("remote/old").is_file());
    assert_eq!(store.active_profile(), "grid");

    // An empty folder holds nothing to lose, and doesn't stop the rename.
    std::fs::remove_dir_all(store.saves_dir("main")).unwrap();
    std::fs::create_dir_all(store.saves_dir("main")).unwrap();
    store.rename_profile("grid", "main").unwrap();
    assert!(store.saves_dir("main").join("remote/save").is_file());
    std::fs::remove_dir_all(&root).ok();
}

// A file in the saves' way makes the move fail here; Windows may not fail the same way.
#[cfg(unix)]
#[test]
fn a_rename_that_cannot_move_the_saves_puts_the_profile_back() {
    let root = tmp("rename-undo");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.set_active_profile("grid").unwrap();
    touch(&store.saves_dir("grid").join("remote/save"));
    touch(&store.saves_dir("main"));

    assert!(store.rename_profile("grid", "main").is_err());

    assert!(store.profiles_dir().join("grid.json").is_file());
    assert!(!store.profiles_dir().join("main.json").exists());
    assert!(store.saves_dir("grid").join("remote/save").is_file());
    assert_eq!(store.active_profile(), "grid");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn refuses_a_profile_installed_in_the_game_folder() {
    let root = tmp("delete-applied");
    let store = Store::new(&root);
    store.create_profile("grid", None).unwrap();
    store.create_profile("main", None).unwrap();
    touch(&store.overlay_dir("grid", "h1").join(crate::build::PLAN_FILE));
    let j = crate::game_folder::journal::Journal::create(&root, "grid", "build-1", false).unwrap();

    let err = store.delete_profile("grid").unwrap_err();
    assert!(err.to_string().contains("installed in the game folder"), "{err}");
    assert!(err.to_string().contains(&j.id), "{err}");
    assert!(store.profiles_dir().join("grid.json").is_file());
    assert!(store.overlays_dir("grid").is_dir());
    assert_eq!(store.plan_profile_deletion("main").unwrap().switch_to, None);

    j.close().unwrap();
    store.delete_profile("grid").unwrap();
    assert!(!store.overlays_dir("grid").exists());
    std::fs::remove_dir_all(&root).ok();
}

fn denied() -> std::io::Error {
    std::io::Error::from(std::io::ErrorKind::PermissionDenied)
}

fn real_rename(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::rename(from, to)
}

fn real_delete(dir: &Path) -> std::io::Result<()> {
    std::fs::remove_dir_all(dir)
}

fn old_copies(store: &Store) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(store.addons_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(OLD_PREFIX) || n.starts_with(STAGING_PREFIX))
        .collect();
    out.sort();
    out
}

fn installed_with_update(name: &str) -> (PathBuf, Store, PathBuf, PathBuf) {
    let root = tmp(name);
    let store = Store::new(&root);
    let v1 = root.join("v1");
    std::fs::create_dir_all(v1.join("base")).unwrap();
    std::fs::write(v1.join(MANIFEST_FILE), manifest_json("a.livery")).unwrap();
    std::fs::write(v1.join("base/common.erp"), b"v1").unwrap();
    store.install_addon(&v1).unwrap();
    let dest = store.addons_dir().join("a.livery");
    let staging = store.addons_dir().join(format!("{STAGING_PREFIX}a.livery"));
    std::fs::create_dir_all(staging.join("base")).unwrap();
    std::fs::write(
        staging.join(MANIFEST_FILE),
        manifest_json("a.livery").replace("1.0.0", "1.1.0"),
    )
    .unwrap();
    std::fs::write(staging.join("base/common.erp"), b"v2").unwrap();
    (root, store, dest, staging)
}

#[test]
fn finish_swaps_leaves_a_swap_another_process_is_making() {
    let (root, store, dest, staging) = installed_with_update("swap-live");
    let aside = store.addons_dir().join(format!("{OLD_PREFIX}a.livery"));
    std::fs::rename(&dest, &aside).unwrap();
    let lock = SwapLock::try_acquire(&store.addons_dir(), "a.livery")
        .unwrap()
        .unwrap();
    let notes = store.finish_swaps();
    assert!(aside.is_dir() && staging.is_dir() && !dest.exists());
    assert!(notes.iter().any(|n| n.contains("another launcher")));
    drop(lock);
    let notes = store.finish_swaps();
    assert!(dest.is_dir() && !aside.exists() && !staging.exists());
    assert!(notes.iter().any(|n| n.contains("put back a.livery")));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_swap_waits_out_nothing_when_another_process_holds_the_same_addon() {
    let (root, store, dest, staging) = installed_with_update("swap-contended");
    let lock = SwapLock::try_acquire(&store.addons_dir(), "a.livery")
        .unwrap()
        .unwrap();
    let e = swap_in(
        &staging,
        &dest,
        "A",
        &mut |from, to| std::fs::rename(from, to),
        &mut |d| std::fs::remove_dir_all(d),
    )
    .unwrap_err();
    assert!(e.to_string().contains("another Swofty launcher"));
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v1");
    drop(lock);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_update_blocked_by_a_file_in_use_keeps_the_installed_copy_whole() {
    let (root, store, dest, staging) = installed_with_update("swap-locked");
    let mut tries = 0;
    let mut deletes = 0;
    let e = swap_in(
        &staging,
        &dest,
        "Livery",
        &mut |from, to| {
            if from == dest.as_path() {
                tries += 1;
                return Err(denied());
            }
            real_rename(from, to)
        },
        &mut |_| {
            deletes += 1;
            Ok(())
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(tries, 6);
    assert_eq!(deletes, 0);
    assert!(
        e.starts_with("Couldn't update Livery: its files are in use"),
        "{e}"
    );
    assert!(e.contains("Close other Swofty Launcher windows"), "{e}");
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v1");
    assert_eq!(store.addons().unwrap()["a.livery"].version.to_string(), "1.0.0");
    assert!(old_copies(&store).is_empty(), "{:?}", old_copies(&store));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_failed_swap_puts_the_installed_copy_back() {
    let (root, store, dest, staging) = installed_with_update("swap-put-back");
    let e = swap_in(
        &staging,
        &dest,
        "Livery",
        &mut |from, to| {
            if from == staging.as_path() {
                return Err(std::io::Error::other("disk gone"));
            }
            real_rename(from, to)
        },
        &mut real_delete,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(e, "Couldn't update Livery: disk gone");
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v1");
    assert!(old_copies(&store).is_empty(), "{:?}", old_copies(&store));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_old_copy_still_in_use_is_deleted_on_the_next_start() {
    let (root, store, dest, staging) = installed_with_update("swap-old-locked");
    swap_in(
        &staging,
        &dest,
        "Livery",
        &mut real_rename,
        &mut |_| Err(denied()),
    )
    .unwrap();
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v2");
    assert_eq!(store.addons().unwrap()["a.livery"].version.to_string(), "1.1.0");
    assert_eq!(old_copies(&store), [format!("{OLD_PREFIX}a.livery")]);

    let notes = store.migrate().unwrap();
    assert!(
        notes.iter().any(|n| n.starts_with("deleted the old copy")),
        "{notes:?}"
    );
    assert!(old_copies(&store).is_empty(), "{:?}", old_copies(&store));
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v2");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_update_stopped_part_way_is_put_back_on_the_next_start() {
    let (root, store, dest, staging) = installed_with_update("swap-interrupted");
    let aside = set_aside(&dest, &mut real_rename).unwrap();
    assert!(!dest.exists());
    let notes = store.migrate().unwrap();
    assert!(
        notes.iter().any(|n| n.starts_with("put back a.livery")),
        "{notes:?}"
    );
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v1");
    assert!(!aside.exists());
    assert!(!staging.exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_second_old_copy_gets_its_own_name() {
    let (root, store, dest, staging) = installed_with_update("swap-second-old");
    std::fs::create_dir_all(store.addons_dir().join(format!("{OLD_PREFIX}a.livery"))).unwrap();
    swap_in(
        &staging,
        &dest,
        "Livery",
        &mut real_rename,
        &mut |_| Err(denied()),
    )
    .unwrap();
    assert_eq!(
        old_copies(&store),
        [format!("{OLD_PREFIX}a.livery"), format!("{OLD_PREFIX}a.livery-1")]
    );
    store.migrate().unwrap();
    assert!(old_copies(&store).is_empty(), "{:?}", old_copies(&store));
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v2");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_removal_blocked_by_a_file_in_use_deletes_nothing() {
    let (root, _, dest, _) = installed_with_update("remove-locked");
    let mut deletes = 0;
    let e = delete_addon_dir(&dest, "Livery", &mut |_, _| Err(denied()), &mut |_| {
        deletes += 1;
        Ok(())
    })
    .unwrap_err()
    .to_string();
    assert_eq!(deletes, 0);
    assert!(
        e.starts_with("Couldn't remove Livery: its files are in use"),
        "{e}"
    );
    assert!(!e.contains(&root.display().to_string()), "{e}");
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v1");

    let e = delete_addon_dir(
        &dest,
        "Livery",
        &mut |_, _| Err(std::io::Error::other("disk gone")),
        &mut real_delete,
    )
    .unwrap_err()
    .to_string();
    assert_eq!(e, "Couldn't remove Livery: disk gone");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_removed_addon_whose_files_are_still_in_use_is_gone_and_deleted_on_the_next_start() {
    let (root, store, dest, staging) = installed_with_update("remove-old-locked");
    std::fs::remove_dir_all(&staging).unwrap();
    store.remove_addon_with("a.livery", |_| Err(denied())).unwrap();
    assert!(!dest.exists());
    assert!(!store.addons().unwrap().contains_key("a.livery"));
    assert_eq!(old_copies(&store), [format!("{OLD_PREFIX}a.livery")]);
    store.migrate().unwrap();
    assert!(old_copies(&store).is_empty(), "{:?}", old_copies(&store));
    assert!(!dest.exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn migrate_notes_say_what_moved_what_stayed_and_why() {
    let root = tmp("migrate-notes");
    let store = Store::new(&root);
    let (old, new) = (root.join("mods"), root.join("addons"));
    write_addon(&new.join("a.one"), "a.one");
    write_addon(&old.join("one-copy"), "a.one");
    write_addon(&old.join("a.two"), "a.two");
    std::fs::create_dir_all(old.join("stray")).unwrap();
    std::fs::write(store.config_path(), r#"{"data_version":4}"#).unwrap();
    assert!(store.migrate().unwrap().is_empty());
    std::fs::write(store.config_path(), r#"{"data_version":3}"#).unwrap();
    let notes = store.migrate().unwrap();
    assert_eq!(
        notes[..3],
        [
            format!(
                "moved {} to {}",
                old.join("a.two").display(),
                new.join("a.two").display()
            ),
            format!(
                "kept {}: {} already has a.one",
                old.join("one-copy").display(),
                new.display()
            ),
            format!(
                "kept {}: no addon id in its {MANIFEST_FILE}",
                old.join("stray").display()
            ),
        ]
    );
    assert_eq!(store.config().data_version, DATA_VERSION);
    std::fs::remove_dir_all(old.join("one-copy")).unwrap();
    std::fs::remove_dir_all(old.join("stray")).unwrap();
    std::fs::write(store.config_path(), r#"{"data_version":3}"#).unwrap();
    let notes = store.migrate().unwrap();
    assert_eq!(notes, [format!("removed the empty {}", old.display())]);
    assert!(!old.exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_backfill_that_cannot_list_the_profiles_is_retried_and_records_no_data_version() {
    let (root, store) = v3_store("backfill-retry", &["a.one"]);
    std::fs::write(store.profiles_dir(), "not a folder").unwrap();
    let notes = store.migrate().unwrap();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes[0].starts_with("couldn't list installed addons in the profiles, will retry: "),
        "{notes:?}"
    );
    assert_eq!(store.config().data_version, 3);
    std::fs::remove_file(store.profiles_dir()).unwrap();
    store.migrate().unwrap();
    assert_eq!(store.config().data_version, DATA_VERSION);
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn a_data_folder_linked_to_a_missing_folder_is_never_started_over() {
    let dir = tmp("migrate-dangling");
    let (root, target) = (dir.join("data"), dir.join("gone/data"));
    std::os::unix::fs::symlink(&target, &root).unwrap();
    let err = Store::new(&root).migrate().unwrap_err().to_string();
    let want = format!(
        "the data folder {} links to {}, which is missing. Reconnect that drive or restore the folder; the launcher will not start a new data folder in its place",
        root.display(),
        target.display()
    );
    assert_eq!(err, want);
    assert!(!target.exists());
    std::fs::remove_dir_all(&dir).ok();
}

struct MigrationFails(AddonManifest);

impl swofty_addon::launcher::Addon for MigrationFails {
    fn manifest(&self) -> &AddonManifest {
        &self.0
    }

    fn migrate(&self, _: &Path, version: u32) -> swofty_addon::Result<Vec<String>> {
        Err(swofty_addon::Error::Other(format!(
            "cannot leave data version {version}"
        )))
    }
}

#[test]
fn a_built_in_addon_whose_migration_fails_stops_the_migration_before_the_backfill() {
    let (root, store) = v3_store("builtin-fails", &["a.one"]);
    let manifest = AddonManifest::parse(
        r#"{"schema_version":1,"id":"b.fails","name":"Fails","version":"1.0.0","author":"t","compat":{"game":"f1_25"}}"#,
    )
    .unwrap();
    let store = store.with_builtins(vec![Builtin {
        addon: Arc::new(MigrationFails(manifest)),
        locked: false,
        default_on: true,
    }]);
    store.save_profile(&Profile::new("grid")).unwrap();
    let profile = store.profiles_dir().join("grid.json");
    let before = std::fs::read(&profile).unwrap();
    let err = store.migrate().unwrap_err().to_string();
    assert_eq!(err, "migrating b.fails: cannot leave data version 3");
    assert_eq!(store.config().data_version, 3);
    assert_eq!(std::fs::read(&profile).unwrap(), before);
    std::fs::remove_dir_all(&root).ok();
}

fn livery_zip(path: &Path, version: &str, lang: &[u8]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    let manifest = manifest_json("a.livery").replace("1.0.0", version).replace(
        r#""compat""#,
        r#""operations":{"assets":["assets/lang.json"]},"compat""#,
    );
    z.start_file(MANIFEST_FILE, opts).unwrap();
    std::io::Write::write_all(&mut z, manifest.as_bytes()).unwrap();
    z.start_file("assets/lang.json", opts).unwrap();
    std::io::Write::write_all(&mut z, lang).unwrap();
    z.start_file("base/common.erp", opts).unwrap();
    std::io::Write::write_all(&mut z, format!("ERPK{version}").as_bytes()).unwrap();
    z.finish().unwrap();
}

fn leftovers(store: &Store) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(store.addons_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with('.') && !n.starts_with(".swap-a.livery"))
        .collect();
    out.sort();
    out
}

const LANG: &[u8] = br#"{"schema_version":1,"ops":[]}"#;

#[test]
fn an_install_lists_its_files_and_leaves_no_unpack_folder_or_lock() {
    let root = tmp("sealed-install");
    let store = Store::new(root.join("data"));
    let zip = root.join("livery.swoftyaddon");
    livery_zip(&zip, "1.0.0", LANG);
    let m = store.install_addon(&zip).unwrap();
    let dir = store.addons_dir().join("a.livery");
    let index = crate::addons::files::read_index(&dir).unwrap();
    assert_eq!(
        index.files.keys().collect::<Vec<_>>(),
        ["assets/lang.json", "base/common.erp", MANIFEST_FILE]
    );
    assert_eq!(index.files["assets/lang.json"].size, LANG.len() as u64);
    assert!(crate::addons::files::check(&dir, &m).is_empty());
    assert!(leftovers(&store).is_empty(), "{:?}", leftovers(&store));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_update_killed_part_way_keeps_the_installed_copy_and_the_next_start_clears_it() {
    let root = tmp("killed-update");
    let store = Store::new(root.join("data"));
    let zip = root.join("v1.swoftyaddon");
    livery_zip(&zip, "1.0.0", LANG);
    store.install_addon(&zip).unwrap();
    let dest = store.addons_dir().join("a.livery");
    livery_zip(&zip, "1.1.0", LANG);

    crate::addons::files::tests::panic_after(1);
    let killed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| store.install_addon(&zip)));
    assert!(killed.is_err());
    let staging = store.addons_dir().join(format!("{STAGING_PREFIX}a.livery"));
    std::fs::write(staging.join("assets/lang.json"), b"").unwrap();
    assert_eq!(store.addons().unwrap()["a.livery"].version.to_string(), "1.0.0");
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"ERPK1.0.0");
    assert_eq!(std::fs::read(dest.join("assets/lang.json")).unwrap(), LANG);
    let m = &store.addons().unwrap()["a.livery"];
    assert!(crate::addons::files::check(&dest, m).is_empty());

    let scratch = store
        .addons_dir()
        .join(format!("{SCRATCH_PREFIX}{}-999999", std::process::id()));
    std::fs::create_dir_all(scratch.join("assets")).unwrap();
    std::fs::write(scratch.join(MANIFEST_FILE), b"{\"schema_vers").unwrap();

    let notes = store.migrate().unwrap();
    assert!(
        notes
            .iter()
            .any(|n| n == "deleted the unfinished install of a.livery, which stays at 1.0.0"),
        "{notes:?}"
    );
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("deleted the unfinished unpack .importing-")),
        "{notes:?}"
    );
    assert!(leftovers(&store).is_empty(), "{:?}", leftovers(&store));
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"ERPK1.0.0");
    assert_eq!(store.addons().unwrap().len(), 2, "core and the livery");

    store.install_addon(&zip).unwrap();
    assert_eq!(store.addons().unwrap()["a.livery"].version.to_string(), "1.1.0");
    assert!(leftovers(&store).is_empty(), "{:?}", leftovers(&store));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_start_leaves_the_unpack_and_staging_folders_of_a_running_install_alone() {
    let (root, store, _, staging) = installed_with_update("sweep-live");
    let addons = store.addons_dir();
    let scratch = Scratch::new(&addons);
    std::fs::create_dir_all(&scratch.dir).unwrap();
    let lock = SwapLock::try_acquire(&addons, "a.livery").unwrap().unwrap();
    let notes = store.migrate().unwrap();
    assert!(notes.iter().all(|n| !n.contains("unfinished")), "{notes:?}");
    assert!(scratch.dir.is_dir() && staging.is_dir());
    drop(lock);
    let dir = scratch.dir.clone();
    drop(scratch);
    assert!(!dir.exists());
    assert!(!swap_lock_path(&addons, &dir.file_name().unwrap().to_string_lossy()).exists());
    store.migrate().unwrap();
    assert!(!staging.exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_install_of_an_addon_another_launcher_is_changing_is_refused_before_its_staging_is_touched() {
    let (root, store, dest, staging) = installed_with_update("install-contended");
    let lock = SwapLock::try_acquire(&store.addons_dir(), "a.livery")
        .unwrap()
        .unwrap();
    let v3 = root.join("v3");
    std::fs::create_dir_all(&v3).unwrap();
    std::fs::write(
        v3.join(MANIFEST_FILE),
        manifest_json("a.livery").replace("1.0.0", "1.2.0"),
    )
    .unwrap();
    let e = store.install_addon(&v3).unwrap_err().to_string();
    assert!(e.contains("another Swofty launcher"), "{e}");
    assert_eq!(std::fs::read(staging.join("base/common.erp")).unwrap(), b"v2");
    assert_eq!(std::fs::read(dest.join("base/common.erp")).unwrap(), b"v1");
    drop(lock);
    std::fs::remove_dir_all(&root).ok();
}
