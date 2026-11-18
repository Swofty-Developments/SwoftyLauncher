use std::collections::BTreeMap;

use super::addon_manifest::*;
use super::export::*;
use super::game_edits::*;
use super::import::*;
use super::naming::*;
use super::*;
use crate::Error;
use crate::dbedit::catalog::tests::sample;
use crate::dbedit::{Catalog, Edit, Edits, Origin};
use crate::install::Game;
use crate::manifest::{MANIFEST_FILE, valid_id};
use crate::store::Store;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;
use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::fnv1a64;

fn store(name: &str) -> Store {
    let root = std::env::temp_dir().join(format!("swofty-dbpkg-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    Store::new(root)
}

fn cat() -> Catalog {
    Catalog::load(&sample()).unwrap()
}

fn empty() -> Edits {
    Edits {
        schema_version: 1,
        ..Edits::default()
    }
}

const NUM: &str = "entrants.drivers.driver_0.car_number";

#[test]
fn slugs_and_unique_ids() {
    assert_eq!(slug("  My Changes!! v2 "), "my-changes-v2");
    assert_eq!(slug("Équipe"), "quipe");
    assert_eq!(slug("!!!"), "addon");
    let s = store("ids");
    let a = Package::create(&s, "My changes").unwrap();
    let b = Package::create(&s, "My changes").unwrap();
    let c = Package::create(&s, "my-changes").unwrap();
    assert_eq!(
        [a.id.as_str(), b.id.as_str(), c.id.as_str()],
        ["dbedit.my-changes", "dbedit.my-changes-2", "dbedit.my-changes-3"]
    );
    assert!(valid_id(&a.id) && is_package(&s, &a.id) && !is_package(&s, "swofty.core"));
    assert!(!is_package(&s, "../x") && Package::create(&s, "  ").is_err());
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn reimports_a_long_named_package_under_new_ids() {
    let s = store("long");
    let cat = cat();
    let name = "Formula One twenty twenty five full season grid and car mods";
    assert_eq!(name.len(), 60);
    let mut p = Package::create(&s, name).unwrap();
    p.edits.set(&cat, fnv1a64(NUM), "33").unwrap();
    p.save(&s, &cat).unwrap();
    let zip_path = s.root.join("long.zip");
    Package::export_zip(&s, &p.id, &zip_path).unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let (s2, zip2) = (s.clone(), zip_path.clone());
    std::thread::spawn(move || {
        let cat = Catalog::load(&sample()).unwrap();
        for _ in 0..3 {
            let got = import(&s2, &cat, &zip2).unwrap();
            tx.send((got.package.id, got.package.name)).unwrap();
        }
    });
    let wait = std::time::Duration::from_secs(20);
    let got: Vec<_> = (0..3)
        .map(|_| rx.recv_timeout(wait).expect("import hung"))
        .collect();
    let ids: HashSet<&String> = got.iter().map(|(id, _)| id).chain([&p.id]).collect();
    assert_eq!(ids.len(), 4, "{got:?}");
    let names: Vec<&str> = got.iter().map(|(_, n)| n.as_str()).collect();
    assert_eq!(names, [2, 3, 4].map(|n| format!("{name} ({n})")));
    assert!(got.iter().all(|(id, _)| id.starts_with(&p.id)));
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn addons_accept_every_launcher_since_the_first_with_categories() {
    let v = |s: &str| semver::Version::parse(s).unwrap();
    for (current, floor) in [
        ("1.3.1", "1.3.0"),
        ("1.3.0", "1.3.0"),
        ("1.3.0-beta.1", "1.3.0-beta.1"),
        ("1.2.5", "1.2.5"),
    ] {
        assert_eq!(launcher_floor(&v(current)), v(floor), "{current}");
    }
    let current = crate::updates::release::current_version();
    let m = manifest("dbedit.x", "x").unwrap();
    let req = m.compat.launcher.as_ref().unwrap();
    assert_eq!(req.to_string(), format!(">={}", launcher_floor(&current)));
    assert!(req.matches(&current) && req.matches(&v("1.3.0")));
}

#[test]
fn copied_rows_raise_a_low_launcher_requirement() {
    let v = |s: &str| semver::Version::parse(s).unwrap();
    let req = |s: &str| Some(semver::VersionReq::parse(s).unwrap());
    for (have, current, want) in [
        (req(">=1.3.0"), "1.4.0", ">=1.3.4"),
        (None, "1.4.0", ">=1.3.4"),
        (req(">=1.3.0"), "1.3.3", ">=1.3.3"),
        (req(">=1.5.0"), "1.5.1", ">=1.5.0"),
        (req("^1.2"), "1.5.1", "^1.2"),
    ] {
        let mut r = have;
        raise_launcher(&mut r, &v(current));
        assert_eq!(r.unwrap().to_string(), want, "{current}");
    }
}

#[test]
fn creates_saves_and_reopens_a_package_as_a_data_addon() {
    let s = store("save");
    let cat = cat();
    let mut p = Package::create(&s, "Grid").unwrap();
    assert_eq!(p.manifest.author, DEFAULT_AUTHOR);
    assert!(
        p.manifest
            .compat
            .accepts_launcher(&crate::updates::release::current_version())
    );
    let before = modified(&s, &p.id).unwrap();
    p.edits.set(&cat, fnv1a64(NUM), "33").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    p.save(&s, &cat).unwrap();
    assert!(modified(&s, &p.id).unwrap() > before);
    let m = &s.addons().unwrap()[&p.id];
    assert_eq!(m.operations.database, [OPS_FILE]);
    assert_eq!(m.kind, [crate::manifest::Kind::Database]);
    let ops: crate::ops::DbOps = crate::read_json(&dir(&s, &p.id).join(OPS_FILE)).unwrap();
    assert_eq!(ops, p.edits.to_ops());
    let marker: serde_json::Value = crate::read_json(&dir(&s, &p.id).join(MARKER)).unwrap();
    assert_eq!(marker["format"], FORMAT);
    assert_eq!(marker["format_version"], 1);
    assert_eq!(marker["game_build"], cat.build_id());
    let again = Package::open(&s, &p.id).unwrap();
    assert_eq!(again.edits, p.edits);
    assert_eq!(again.info().changes, 1);
    p.set_details("Grid 2", "1.2.0", "  ", "Bigger").unwrap();
    assert!(p.set_details("x", "one", "a", "").is_err());
    assert!(p.set_details(" ", "1.0.0", "a", "").is_err());
    p.save(&s, &cat).unwrap();
    let info = Package::open(&s, &p.id).unwrap().info();
    assert_eq!(
        (
            info.name.as_str(),
            info.version.as_str(),
            info.author.as_str(),
            info.description.as_str()
        ),
        ("Grid 2", "1.2.0", DEFAULT_AUTHOR, "Bigger")
    );
    std::fs::write(dir(&s, &p.id).join(MARKER), b"{\"format\": \"swofty-db-pa").unwrap();
    assert!(matches!(Package::open(&s, &p.id), Err(Error::Json { .. })));
    assert!(list(&s).unwrap().is_empty());
    let scan = scan(&s).unwrap();
    assert_eq!(scan.damaged.len(), 1);
    assert_eq!(scan.damaged[0].0, p.id);
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn lists_duplicates_and_deletes() {
    let s = store("list");
    let cat = cat();
    let mut b = Package::create(&s, "beta").unwrap();
    b.edits.set(&cat, fnv1a64(NUM), "7").unwrap();
    b.set_details("beta", "2.0.0", "Max", "d").unwrap();
    b.save(&s, &cat).unwrap();
    Package::create(&s, "Alpha").unwrap();
    let names: Vec<_> = list(&s).unwrap().into_iter().map(|i| i.name).collect();
    assert_eq!(names, ["Alpha", "beta"]);
    let copy = Package::duplicate(&s, &b.id, "beta copy").unwrap();
    let info = Package::open(&s, &copy.id).unwrap().info();
    assert_eq!(
        (
            info.id.as_str(),
            info.version.as_str(),
            info.author.as_str(),
            info.changes
        ),
        ("dbedit.beta-copy", "2.0.0", "Max", 1)
    );
    assert_eq!(info.game_build, cat.build_id());
    Package::delete(&s, &b.id).unwrap();
    assert!(!is_package(&s, &b.id) && Package::open(&s, &b.id).is_err());
    assert!(Package::delete(&s, "swofty.core").is_err());
    assert_eq!(list(&s).unwrap().len(), 2);
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn exports_and_imports_zips() {
    let s = store("zip");
    let cat = cat();
    let mut p = Package::create(&s, "Shared").unwrap();
    p.edits.set(&cat, fnv1a64(NUM), "33").unwrap();
    p.save(&s, &cat).unwrap();
    let zip_path = s.root.join("shared.zip");
    Package::export_zip(&s, &p.id, &zip_path).unwrap();
    let z = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let mut names: Vec<String> = z.file_names().map(|n| n.unwrap().into_owned()).collect();
    names.sort();
    assert_eq!(names, [OPS_FILE, MARKER, MANIFEST_FILE]);

    let got = import(&s, &cat, &zip_path).unwrap();
    assert_eq!(got.renamed_from.as_deref(), Some("dbedit.shared"));
    assert_eq!(
        (
            got.package.id.as_str(),
            got.package.name.as_str(),
            got.package.changes
        ),
        ("dbedit.shared-2", "Shared (2)", 1)
    );
    assert_eq!((got.report.format, got.report.imported), ("package", 1));
    Package::delete(&s, &p.id).unwrap();
    let fresh = import(&s, &cat, &zip_path).unwrap();
    assert_eq!(
        (fresh.package.id.as_str(), fresh.renamed_from),
        ("dbedit.shared", None)
    );
    assert!(std::fs::read_dir(s.root.join("cache")).unwrap().next().is_none());

    let plain = s.root.join("plain.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&plain).unwrap());
    z.start_file(MANIFEST_FILE, zip::write::SimpleFileOptions::default())
        .unwrap();
    z.write_all(b"{}").unwrap();
    z.finish().unwrap();
    let e = import(&s, &cat, &plain).unwrap_err().to_string();
    assert_eq!(
        e,
        "plain.zip is not a DB Editor addon; install it on the Addons page"
    );
    assert!(import(&s, &cat, &s.root.join("x.txt")).is_err());
    std::fs::remove_dir_all(&s.root).ok();
}

fn zip_with(path: &Path, files: &[(&str, &[u8])]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (n, d) in files {
        z.start_file(*n, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(d).unwrap();
    }
    z.finish().unwrap();
}

fn smuggling_zip(path: &Path, version: &str) {
    let manifest = serde_json::json!({
        "schema_version": 1, "id": "evil.native", "name": "Grid tweaks", "version": version,
        "author": "x", "description": "d", "kind": ["database", "native", "files"],
        "compat": {"game": "f1_25"},
        "operations": {"database": ["database.json"], "native": [{"dll": "evil.dll"}],
                       "files": [{"source": "evil.dll", "target": "evil.dll"}]}
    });
    let mut edits = empty();
    edits.set(&cat(), fnv1a64(NUM), "33").unwrap();
    let marker = serde_json::json!({"format": FORMAT, "format_version": 1, "game_build": "abc",
        "edits": edits.edits});
    let ops = serde_json::json!({"schema_version": 1, "ops": [{"op": "put-record",
        "key": format!("0x{:016x}", fnv1a64("entrants.drivers.driver_1.car_number")), "tag": 0, "value": "3939"}]});
    zip_with(
        path,
        &[
            (MANIFEST_FILE, manifest.to_string().as_bytes()),
            (MARKER, marker.to_string().as_bytes()),
            (OPS_FILE, ops.to_string().as_bytes()),
            ("evil.dll", b"MZ fake"),
        ],
    );
}

#[test]
fn imports_only_the_details_and_edits_of_a_zip() {
    let s = store("smuggle");
    let cat = cat();
    std::fs::create_dir_all(&s.root).unwrap();
    let zip_path = s.root.join("pkg.zip");
    smuggling_zip(&zip_path, "1.4.0");
    let got = import(&s, &cat, &zip_path).unwrap();
    assert_eq!(
        (got.package.id.as_str(), got.package.name.as_str()),
        ("dbedit.grid-tweaks", "Grid tweaks")
    );
    assert_eq!(
        (
            got.package.version.as_str(),
            got.package.author.as_str(),
            got.package.description.as_str(),
            got.package.game_build.as_str()
        ),
        ("1.4.0", "x", "d", "abc")
    );
    let d = dir(&s, &got.package.id);
    let mut files: Vec<String> = std::fs::read_dir(&d)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    files.sort();
    assert_eq!(files, [OPS_FILE, MARKER, MANIFEST_FILE]);
    let p = Package::open(&s, &got.package.id).unwrap();
    let mut want = manifest(&p.id, "Grid tweaks").unwrap();
    want.version = semver::Version::new(1, 4, 0);
    want.author = "x".into();
    want.description = Some("d".into());
    assert_eq!(p.manifest, want);
    let ops: crate::ops::DbOps = crate::read_json(&d.join(OPS_FILE)).unwrap();
    assert_eq!(ops, p.edits.to_ops());

    smuggling_zip(&zip_path, "one");
    let e = import(&s, &cat, &zip_path).unwrap_err().to_string();
    assert_eq!(e, "\"one\" is not a version like 1.0.0");
    assert_eq!(list(&s).unwrap().len(), 1);

    let foreign = s.addons_dir().join("evil.pkg");
    std::fs::create_dir_all(&foreign).unwrap();
    std::fs::copy(d.join(MARKER), foreign.join(MARKER)).unwrap();
    std::fs::copy(d.join(MANIFEST_FILE), foreign.join(MANIFEST_FILE)).unwrap();
    assert!(!is_package(&s, "evil.pkg") && Package::open(&s, "evil.pkg").is_err());
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn installs_from_the_addons_page_as_a_new_package() {
    let cat = cat();
    let (a, b) = (store("share-a"), store("share-b"));
    let mut pa = Package::create(&a, "My changes").unwrap();
    pa.edits.set(&cat, fnv1a64(NUM), "33").unwrap();
    pa.save(&a, &cat).unwrap();
    let zip_path = a.root.join("my-changes.zip");
    Package::export_zip(&a, &pa.id, &zip_path).unwrap();
    let mut pb = Package::create(&b, "My changes").unwrap();
    pb.edits.set(&cat, fnv1a64(NUM), "44").unwrap();
    pb.edits
        .set(&cat, fnv1a64("entrants.drivers.driver_1.car_number"), "55")
        .unwrap();
    pb.save(&b, &cat).unwrap();

    let m = b.install_addon(&zip_path).unwrap();
    assert_eq!(
        (m.id.as_str(), m.name.as_str()),
        ("dbedit.my-changes-2", "My changes (2)")
    );
    assert_eq!(Package::open(&b, &pb.id).unwrap().edits, pb.edits);
    let got = Package::open(&b, &m.id).unwrap();
    assert_eq!(
        (got.edits, got.game_build),
        (pa.edits.clone(), cat.build_id().to_owned())
    );

    let folder = a.root.join("unzipped").join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    for f in [MANIFEST_FILE, MARKER] {
        std::fs::copy(dir(&a, &pa.id).join(f), folder.join(f)).unwrap();
    }
    std::fs::write(folder.join("evil.dll"), b"MZ").unwrap();
    let m = b.install_addon(&a.root.join("unzipped")).unwrap();
    assert_eq!(m.id, "dbedit.my-changes-3");
    assert!(!dir(&b, &m.id).join("evil.dll").exists());

    let smuggled = a.root.join("smuggled.zip");
    smuggling_zip(&smuggled, "1.0.0");
    let m = b.install_addon(&smuggled).unwrap();
    assert_eq!(m.kind, [crate::manifest::Kind::Database]);
    assert!(m.operations.native.is_empty() && !dir(&b, &m.id).join("evil.dll").exists());
    assert_eq!(list(&b).unwrap().len(), 4);
    std::fs::remove_dir_all(&a.root).ok();
    std::fs::remove_dir_all(&b.root).ok();
}

#[test]
fn other_addons_cannot_take_a_db_editor_id() {
    let cat = cat();
    let s = store("taken-id");
    let mut p = Package::create(&s, "My changes").unwrap();
    p.edits.set(&cat, fnv1a64(NUM), "33").unwrap();
    p.save(&s, &cat).unwrap();
    let plain = |id: &str| {
        let dir = s.root.join("incoming").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let m = format!(
            r#"{{"schema_version":1,"id":"{id}","name":"x","version":"1.0.0","author":"x","compat":{{"game":"f1_25"}}}}"#
        );
        std::fs::write(dir.join(MANIFEST_FILE), m).unwrap();
        s.install_addon(&dir).unwrap_err().to_string()
    };
    assert!(plain(&p.id).contains("is a DB Editor addon you have"));
    assert_eq!(Package::open(&s, &p.id).unwrap().edits, p.edits);
    assert!(plain("dbedit.other").contains("prefix is kept"));
    assert!(!dir(&s, "dbedit.other").exists());
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn imports_json_exports_as_new_packages() {
    let s = store("json");
    let cat = cat();
    let mut edits = empty();
    edits.set(&cat, fnv1a64(NUM), "44").unwrap();
    std::fs::create_dir_all(&s.root).unwrap();
    let export = s.root.join("My Grid.json");
    std::fs::write(&export, edits.export(&cat)).unwrap();
    let got = import(&s, &cat, &export).unwrap();
    assert_eq!(
        (got.package.id.as_str(), got.package.name.as_str()),
        ("dbedit.my-grid", "My Grid")
    );
    assert_eq!(
        (got.report.format, got.report.imported, got.package.changes),
        ("swofty", 1, 1)
    );

    let marker = s.root.join("again.json");
    std::fs::copy(dir(&s, &got.package.id).join(MARKER), &marker).unwrap();
    let again = import(&s, &cat, &marker).unwrap();
    assert_eq!(
        (again.package.id.as_str(), again.package.changes),
        ("dbedit.again", 1)
    );

    let ego = s.root.join("ego.json");
    let doc = serde_json::json!({"format": "ego-gamedatabases-editor-records",
        "records": {"modified_records": {fnv1a64(NUM).to_string(): "3434"}, "added_records": []}});
    std::fs::write(&ego, doc.to_string()).unwrap();
    let got = import(&s, &cat, &ego).unwrap();
    assert_eq!((got.report.format, got.package.changes), ("ego", 1));
    let bad = s.root.join("bad.json");
    std::fs::write(&bad, b"{\"format\": \"nope\"}").unwrap();
    assert!(import(&s, &cat, &bad).is_err());
    assert!(!is_package(&s, "dbedit.bad"));
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn enabled_changes_follow_the_load_order() {
    let s = store("enabled");
    let cat = cat();
    let key = fnv1a64(NUM);
    let mut ids = Vec::new();
    for (name, v) in [("a", "10"), ("b", "20"), ("off", "30")] {
        let mut p = Package::create(&s, name).unwrap();
        p.edits.set(&cat, key, v).unwrap();
        p.save(&s, &cat).unwrap();
        ids.push(p.id);
    }
    s.update_profile("default", |p| {
        p.set_enabled(&ids[1], true)?;
        p.set_enabled(&ids[0], true)?;
        p.set_enabled(&ids[2], false)
    })
    .unwrap();
    let got = enabled_changes(&s, "default").unwrap();
    assert_eq!(
        got[&key],
        [
            (ids[1].clone(), "20".to_owned()),
            (ids[0].clone(), "10".to_owned())
        ]
    );
    assert!(enabled_changes(&s, "vanilla").unwrap().is_empty());
    std::fs::remove_dir_all(&s.root).ok();
}

fn game(s: &Store) -> Game {
    let dir = s.root.join("game");
    let path = dir.join(crate::build::GVDB);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, sample()).unwrap();
    Game {
        dir,
        user_reg: None,
        identity: crate::game_folder::baseline::Identity {
            build_id: "25251167".into(),
            exe_sha256: "ab".repeat(32),
        },
    }
}

fn records(db: &Gvdb) -> Vec<(u64, u8, Vec<u8>)> {
    let mut out: Vec<_> = db
        .records
        .iter()
        .map(|r| (r.key, r.tag, r.value.clone()))
        .collect();
    out.sort();
    out
}

fn unzip_all(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..z.len())
        .map(|i| {
            let mut e = z.by_index(i).unwrap();
            let mut data = Vec::new();
            e.read_to_end(&mut data).unwrap();
            (e.name().unwrap().into_owned(), data)
        })
        .collect()
}

#[test]
fn exports_stacked_packages_as_game_files() {
    let s = store("game");
    let (cat, game) = (cat(), game(&s));
    let team = fnv1a64("entrants.teams.team_0.prestige");
    let pole = fnv1a64("tracks.Track_0.PoleOnRight");
    let mut a = Package::create(&s, "Numbers").unwrap();
    a.edits.set(&cat, fnv1a64(NUM), "33").unwrap();
    a.edits.set(&cat, team, "20").unwrap();
    a.set_details("Numbers", "1.2.0", "Max", "").unwrap();
    a.save(&s, &cat).unwrap();
    let mut b = Package::create(&s, "Grid").unwrap();
    b.edits.set(&cat, fnv1a64(NUM), "44").unwrap();
    let nick = b
        .edits
        .add(&cat, "entrants.drivers.driver_1.nickname", 2, "Lando")
        .unwrap();
    b.edits.remove_key(&cat, pole).unwrap();
    b.edits.edits.push(Edit {
        select: Some("entrants.drivers.driver[_metadata_=nobody]".into()),
        ..b.edits.get(fnv1a64(NUM)).unwrap().clone()
    });
    b.edits.edits.last_mut().unwrap().key = 1;
    b.save(&s, &cat).unwrap();
    let out = s.root.join("export.zip");
    let ids = [a.id.clone(), b.id.clone()];
    let got = export_game_files(&s, &game, &ids, &out).unwrap();
    assert_eq!(got.changes, 4, "{got:?}");
    assert_eq!(got.skipped.len(), 2, "{got:?}");
    assert!(got.skipped[0].starts_with("Grid: change skipped: "), "{got:?}");
    assert_eq!(
        got.skipped[1],
        "Grid: 1 removal(s) of stock database records skipped; the game crashes when a record it reads is missing"
    );
    assert_eq!(got.bytes, std::fs::metadata(&out).unwrap().len());
    let files = unzip_all(&out);
    assert_eq!(files.keys().collect::<Vec<_>>(), [README, crate::build::GVDB]);
    let mut want = Gvdb::parse(&sample()).unwrap();
    want.set("entrants.drivers.driver_0.car_number", "44", None)
        .unwrap();
    want.set("entrants.teams.team_0.prestige", "20", None).unwrap();
    want.records.push(Record {
        key: nick,
        tag: 2,
        value: b"Lando".to_vec(),
    });
    let db = Gvdb::parse(&files[crate::build::GVDB]).unwrap();
    assert_eq!(records(&db), records(&want));
    let readme = String::from_utf8(files[README].clone()).unwrap();
    for part in [
        "- Numbers 1.2.0 by Max (2 change(s))",
        "- Grid 1.0.0 by Unknown (4 change(s))",
        &format!("game build {}", game.identity.key()),
        &format!("stock database {}", cat.build_id()),
        "2 change(s) did not apply",
        "Verify integrity of game files",
        "refuses modded launches",
    ] {
        assert!(readme.contains(part), "{part}\n{readme}");
    }
    assert!(readme.find("Numbers").unwrap() < readme.find("Grid").unwrap());

    let rev = [b.id.clone(), a.id.clone()];
    export_game_files(&s, &game, &rev, &out).unwrap();
    let db = Gvdb::parse(&unzip_all(&out)[crate::build::GVDB]).unwrap();
    assert_eq!(db.get_text(NUM).as_deref(), Some("33"));
    assert_eq!(
        std::fs::read(game.dir.join(crate::build::GVDB)).unwrap(),
        sample()
    );
    let empty = Package::create(&s, "Empty").unwrap();
    assert!(export_game_files(&s, &game, &[empty.id], &out).is_err());
    assert!(export_game_files(&s, &game, &[], &out).is_err());
    assert!(export_game_files(&s, &game, &["swofty.core".into()], &out).is_err());
    let left: Vec<_> = std::fs::read_dir(&s.root)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(left.is_empty());
    std::fs::remove_dir_all(&s.root).ok();
}

fn modded() -> Gvdb {
    let mut db = Gvdb::parse(&sample()).unwrap();
    db.set(NUM, "33", None).unwrap();
    db.set("entrants.drivers.driver_1.available", "False", None)
        .unwrap();
    db.set("nobody.knows.this", "2.5", None).unwrap();
    db.remove("tracks.Track_0.PoleOnRight").unwrap();
    let add = crate::ops::DbOps {
        strict: false,
        side_data: Default::default(),
        schema_version: 1,
        fields: BTreeMap::from([(
            "entrants.drivers.driver".to_owned(),
            ["_metadata_", "first_name", "car_number", "available"]
                .map(str::to_owned)
                .to_vec(),
        )]),
        ops: vec![crate::ops::DbOp::AddRow {
            table: "entrants.drivers".into(),
            from: "entrants.drivers.driver_1".into(),
            set: BTreeMap::from([
                ("_metadata_".to_owned(), "new_guy".to_owned()),
                ("car_number".to_owned(), "77".to_owned()),
            ]),
            copy: None,
            stat_id: None,
        }],
    };
    crate::ops::apply_db_ops(&mut db, &add, &BTreeMap::new()).unwrap();
    db.records.push(Record::new("brand.new.key", 2, "hello"));
    db
}

#[test]
fn imports_a_modded_database_as_a_package() {
    let s = store("bin");
    let cat = cat();
    std::fs::create_dir_all(&s.root).unwrap();
    let modded = modded();
    let bin = s.root.join("Big Mod.bin");
    std::fs::write(&bin, modded.to_bytes()).unwrap();
    let got = import(&s, &cat, &bin).unwrap();
    assert_eq!(
        (got.package.id.as_str(), got.package.name.as_str()),
        ("dbedit.big-mod", "Big Mod")
    );
    assert_eq!(got.report.format, "bin");
    assert_eq!(got.report.imported, 10, "{:?}", got.report);
    assert_eq!(
        got.report.skipped,
        [
            "4 changed, 5 added and 1 removed key(s)",
            "adds rows to entrants.drivers.driver; another addon that adds rows to the same tables won't stack with this one",
        ]
    );
    let p = Package::open(&s, &got.package.id).unwrap();
    let num = p.edits.get(fnv1a64(NUM)).unwrap();
    assert_eq!(
        (num.select.as_deref(), num.stock.as_deref(), &num.origin),
        (
            Some("entrants.drivers.driver[_metadata_=max_verstappen]"),
            Some("1"),
            &Origin::Imported("Big Mod.bin".into())
        )
    );
    let row = p
        .edits
        .get(fnv1a64("entrants.drivers.driver_2.car_number"))
        .unwrap();
    assert_eq!(
        (row.path.as_deref(), row.stock.as_deref(), row.tag),
        (Some("entrants.drivers.driver_2.car_number"), None, 0)
    );
    let unknown = p.edits.get(fnv1a64("brand.new.key")).unwrap();
    assert_eq!((unknown.path.as_deref(), unknown.tag), (None, 2));
    assert_eq!(
        p.edits.get(fnv1a64("tracks.Track_0.PoleOnRight")).unwrap().value,
        None
    );
    let count = fnv1a64("entrants.drivers._driver_instancecount_");
    assert_eq!(p.edits.edits.last().unwrap().key, count);

    let mut db = Gvdb::parse(&sample()).unwrap();
    crate::ops::apply_db_ops(&mut db, &p.edits.to_ops(), &BTreeMap::new()).unwrap();
    assert_eq!(records(&db), records(&modded));

    let zip_path = s.root.join("Old Mod.zip");
    let mut z = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file("readme.txt", opts).unwrap();
    z.write_all(b"copy into the game folder").unwrap();
    z.start_file(format!("Old Mod/{}", crate::build::GVDB), opts)
        .unwrap();
    z.write_all(&modded.to_bytes()).unwrap();
    z.finish().unwrap();
    let zipped = import(&s, &cat, &zip_path).unwrap();
    assert_eq!(
        (
            zipped.package.name.as_str(),
            zipped.report.format,
            zipped.package.changes
        ),
        ("Old Mod", "bin", 10)
    );
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn refuses_stock_and_foreign_databases() {
    let s = store("binbad");
    let cat = cat();
    std::fs::create_dir_all(&s.root).unwrap();
    let err = |name: &str, bytes: &[u8]| {
        let path = s.root.join(name);
        std::fs::write(&path, bytes).unwrap();
        import(&s, &cat, &path).unwrap_err().to_string()
    };
    assert_eq!(
        err("stock.bin", &sample()),
        "stock.bin is the same as this game's stock database"
    );
    let foreign = Gvdb::new(vec![Record::new("a.b", 0, "1")]);
    for (name, bytes) in [
        ("junk.bin", b"not a database".to_vec()),
        ("other.bin", foreign.to_bytes()),
    ] {
        assert_eq!(
            err(name, &bytes),
            format!("{name} is not an F1 25 game database ({GVDB_NAME})")
        );
    }
    let zip_with = |name: &str, files: &[&str]| {
        let path = s.root.join(name);
        let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        for f in files {
            z.start_file(*f, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(&modded().to_bytes()).unwrap();
        }
        z.finish().unwrap();
        import(&s, &cat, &path).unwrap_err().to_string()
    };
    assert_eq!(
        zip_with("none.zip", &["readme.txt"]),
        format!("none.zip is not a DB Editor addon and holds no {GVDB_NAME}")
    );
    assert!(zip_with("two.zip", &[GVDB_NAME, &format!("x/{GVDB_NAME}")]).contains("holds 2"));
    assert_eq!(
        zip_with("addon.zip", &[MANIFEST_FILE, GVDB_NAME]),
        "addon.zip is not a DB Editor addon; install it on the Addons page"
    );
    assert!(list(&s).unwrap().is_empty());
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn imports_a_modded_copy_of_the_real_database() {
    let Some(cat) = crate::dbedit::catalog::tests::real() else {
        eprintln!("F1_25_DIR is not set; skipping");
        return;
    };
    let game = std::path::PathBuf::from(std::env::var_os("F1_25_DIR").unwrap());
    let stock = std::fs::read(game.join(crate::build::GVDB)).unwrap();
    let mut db = Gvdb::parse(&stock).unwrap();
    let tracks = "tracks._Track_instancecount_";
    let n: u32 = db.get_text(tracks).unwrap().parse().unwrap();
    db.set(tracks, (n + 1).to_string(), None).unwrap();
    let num = cat
        .resolve("entrants.drivers.driver[_metadata_=max_verstappen].car_number")
        .unwrap();
    db.records.iter_mut().find(|r| r.key == num).unwrap().value = b"99".to_vec();
    db.records.push(Record::new(
        &format!("tracks.Track_{n}._metadata_"),
        2,
        "new_track",
    ));
    let s = store("realbin");
    std::fs::create_dir_all(&s.root).unwrap();
    let bin = s.root.join(GVDB_NAME);
    std::fs::write(&bin, db.to_bytes()).unwrap();
    let t = std::time::Instant::now();
    let got = import(&s, &cat, &bin).unwrap();
    eprintln!("imported in {:?}: {:?}", t.elapsed(), got.report);
    assert_eq!(got.package.changes, 3);
    assert!(
        got.report.skipped[1].starts_with("adds rows to tracks.Track;"),
        "{:?}",
        got.report
    );
    let p = Package::open(&s, &got.package.id).unwrap();
    let named = p
        .edits
        .get(fnv1a64(format!("tracks.Track_{n}._metadata_")))
        .unwrap();
    assert_eq!(named.path, Some(format!("tracks.Track_{n}._metadata_")));
    let mut again = Gvdb::parse(&stock).unwrap();
    crate::ops::apply_db_ops(&mut again, &p.edits.to_ops(), &BTreeMap::new()).unwrap();
    assert_eq!(records(&again), records(&db));
    std::fs::remove_dir_all(&s.root).ok();
}

#[test]
fn clears_edits_and_turns_off_every_package() {
    let s = store("off");
    let cat = cat();
    let mut ids = Vec::new();
    for name in ["one", "two", "three"] {
        let mut p = Package::create(&s, name).unwrap();
        p.edits.set(&cat, fnv1a64(NUM), "8").unwrap();
        assert_eq!(p.clear_edits(), 1);
        p.save(&s, &cat).unwrap();
        ids.push(p.id);
    }
    assert_eq!(Package::open(&s, &ids[0]).unwrap().info().changes, 0);
    s.update_profile("default", |p| {
        p.set_enabled(&ids[0], true)?;
        p.set_enabled(&ids[2], true)
    })
    .unwrap();
    assert_eq!(
        enabled_packages(&s, "default").unwrap(),
        [ids[0].clone(), ids[2].clone()]
    );
    assert_eq!(turn_off_all(&s, "default").unwrap(), 2);
    assert!(enabled_packages(&s, "default").unwrap().is_empty());
    assert_eq!(turn_off_all(&s, "default").unwrap(), 0);
    assert!(turn_off_all(&s, crate::profile::VANILLA).is_err());
    std::fs::remove_dir_all(&s.root).ok();
}
