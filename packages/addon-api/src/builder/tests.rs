use super::*;
use crate::asset::{FileKind, OpFile};
use crate::manifest::Kind;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("swofty-builder-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample(src: &Path) -> AddonBuilder {
    std::fs::write(src.join("a.dds"), "dds").unwrap();
    std::fs::write(src.join("r.bin"), "frag").unwrap();
    std::fs::create_dir_all(src.join("okayama")).unwrap();
    std::fs::write(src.join("okayama/manifest.json"), "{}").unwrap();
    let mut b = AddonBuilder::new("you.okayama", "Okayama", "You");
    b.category = Some(Category::Tracks);
    b.description = Some("Okayama International Circuit".into());
    b.origin = Some(Origin {
        tool: "teamsimplified.mods".into(),
        tool_version: Some("2.0.0".into()),
        file: Some("okayama.zip".into()),
        sha256: Some("ab".into()),
    });
    b.db(DbOp::PutRecord {
        key: "rules.laps".into(),
        tag: 0,
        value: "3135".into(),
    })
    .erp(ErpOp::ReplaceResource {
        erp: "2025_asset_groups/t.erp".into(),
        resource: "eaid://t/a.idf".into(),
        fragment: 0,
        source: Source::addon("erp/r.bin"),
    })
    .source("erp/r.bin", src.join("r.bin"))
    .text("*", "lng_okayama", "Okayama")
    .file(src.join("a.dds"), "livery_package_shared/a.dds")
    .track(src.join("okayama"), "okayama");
    b
}

#[test]
fn writes_a_data_addon_that_loads() {
    let root = tmp("write");
    let b = sample(&root);
    let dir = root.join("out");
    let m = b.write(&dir).unwrap();
    assert_eq!(m, AddonManifest::load(&dir).unwrap());
    assert_eq!(m.category, Some(Category::Tracks));
    assert_eq!(m.origin.as_ref().unwrap().file.as_deref(), Some("okayama.zip"));
    assert_eq!(
        m.compat.launcher.as_ref().unwrap().to_string(),
        format!(">={}", launcher_floor())
    );
    assert!(launcher_floor() <= Version::new(1, 3, 0));
    assert_eq!(m.kind, [Kind::Database, Kind::Erp, Kind::Text, Kind::Files]);
    assert_eq!(m.operations.database, [DATABASE_FILE]);
    assert_eq!(m.operations.erp, [ERP_FILE]);
    assert_eq!(m.operations.assets, [TEXT_FILE, TRACKS_FILE]);
    assert_eq!(m.operations.files[0].source, "files/livery_package_shared/a.dds");
    assert!(m.operations.native.is_empty() && m.launcher.is_none());
    let read = |name: &str, kind| {
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join(name)).unwrap()).unwrap();
        OpFile::from_value(v, kind).unwrap()
    };
    assert!(matches!(
        read(DATABASE_FILE, FileKind::Database).ops[0],
        AssetOp::Db(DbOp::PutRecord { .. })
    ));
    assert!(matches!(
        read(ERP_FILE, FileKind::Erp).ops[0],
        AssetOp::Erp(ErpOp::ReplaceResource { .. })
    ));
    assert!(matches!(
        read(TEXT_FILE, FileKind::Assets).ops[0],
        AssetOp::Lang(LangOp::Set { .. })
    ));
    assert!(matches!(
        read(TRACKS_FILE, FileKind::Assets).ops[0],
        AssetOp::Track(_)
    ));
    assert_eq!(std::fs::read(dir.join("erp/r.bin")).unwrap(), b"frag");
    assert_eq!(
        std::fs::read(dir.join("files/livery_package_shared/a.dds")).unwrap(),
        b"dds"
    );
    assert!(dir.join("tracks/okayama/manifest.json").is_file());
    assert!(b.write(&dir).unwrap_err().to_string().contains("not empty"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn refuses_bad_ids_paths_unnamed_sources_and_code() {
    let root = tmp("refuse");
    let out = root.join("out");
    let base = sample(&root);
    let refused = |b: &AddonBuilder| {
        let e = b.write(&out).unwrap_err().to_string();
        assert!(!out.exists(), "{e}");
        e
    };
    for id in ["Okayama", "single", "swofty.okayama", "dbedit.okayama"] {
        let mut b = base.clone();
        b.id = id.into();
        refused(&b);
    }
    let mut b = base.clone();
    b.source("../escape", root.join("r.bin"));
    assert!(refused(&b).contains("inside the addon"));
    let mut b = base.clone();
    b.file(root.join("a.dds"), "/etc/passwd");
    assert!(refused(&b).contains("inside the game folder"));
    let mut b = base.clone();
    b.file(root.join("a.dds"), "livery_package_shared/a.dds");
    assert!(refused(&b).contains("put twice"));
    let mut b = base.clone();
    b.erp(ErpOp::PutResource {
        erp: "a.erp".into(),
        resource: "r".into(),
        source: Source::addon("erp/missing.erp"),
        rename: None,
    });
    assert!(refused(&b).contains("erp/missing.erp"));
    let mut b = base.clone();
    b.lang(LangOp::Table {
        language: "eng".into(),
        source: Source {
            root: crate::asset::Root::Data,
            path: "eng.lng".into(),
        },
    });
    assert!(refused(&b).contains("own folder"));
    let mut b = base.clone();
    b.source("bin/x64/version.dll", root.join("r.bin"));
    assert!(refused(&b).contains("data only"));
    let mut b = base.clone();
    b.file(root.join("a.dds"), "bin/x64/dinput8.DLL");
    assert!(refused(&b).contains("data only"));
    std::fs::write(root.join("okayama/plugin.dll"), "MZ").unwrap();
    assert!(refused(&base).contains("data only"));
    std::fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn refuses_links_in_sources() {
    let root = tmp("links");
    let out = root.join("out");
    let base = sample(&root);
    std::os::unix::fs::symlink(&root, root.join("okayama/loop")).unwrap();
    let e = base.write(&out).unwrap_err().to_string();
    assert!(e.contains("is a link") && !out.exists(), "{e}");
    std::fs::remove_file(root.join("okayama/loop")).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", root.join("okayama/host")).unwrap();
    assert!(base.write(&out).unwrap_err().to_string().contains("is a link"));
    std::fs::remove_file(root.join("okayama/host")).unwrap();
    let mut b = base.clone();
    std::os::unix::fs::symlink(root.join("a.dds"), root.join("linked.dds")).unwrap();
    b.file(root.join("linked.dds"), "livery_package_shared/b.dds");
    assert!(b.write(&out).unwrap_err().to_string().contains("is a link"));
    base.write(&out).unwrap();
    std::fs::remove_dir_all(&root).ok();
}
