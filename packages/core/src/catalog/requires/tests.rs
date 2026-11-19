use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use semver::VersionReq;

use super::downloads::*;
use super::install::*;
use super::manifest::*;
use super::walk::*;
use super::*;
use crate::addons::builtin::CORE_ID;
use crate::catalog::{Addon, Stage, Version};
use crate::manifest::{AddonManifest, MANIFEST_FILE, Requirement};
use crate::store::Store;
use crate::{Error, Result};

fn version(v: &str, requires: &[(&str, &str)]) -> Version {
    Version {
        version: semver::Version::parse(v).unwrap(),
        launcher: None,
        game: "f1_25".into(),
        size: 3,
        sha256: String::new(),
        url: format!("http://localhost/{v}"),
        published_at: String::new(),
        changelog: String::new(),
        native: false,
        yanked: false,
        platforms: Vec::new(),
        requires: Some(
            requires
                .iter()
                .map(|(id, req)| Requirement {
                    id: (*id).into(),
                    version: VersionReq::parse(req).unwrap(),
                })
                .collect(),
        ),
    }
}

fn addon(id: &str, versions: Vec<Version>) -> Addon {
    Addon {
        id: id.into(),
        name: format!("{id} name"),
        author: "a".into(),
        summary: String::new(),
        description: String::new(),
        category: None,
        kind: Vec::new(),
        homepage: None,
        repository: None,
        icon: None,
        publisher: None,
        publishers: Vec::new(),
        downloads: 0,
        downloads_30d: 0,
        stars: 0,
        trend: None,
        created_at: String::new(),
        updated_at: String::new(),
        latest: versions.first().cloned(),
        versions,
        media: Vec::new(),
    }
}

fn manifest(id: &str, v: &str, requires: &[(&str, &str)]) -> AddonManifest {
    let requires: Vec<serde_json::Value> = requires
        .iter()
        .map(|(id, req)| serde_json::json!({ "id": id, "version": req }))
        .collect();
    serde_json::from_value(serde_json::json!({
        "schema_version": 1,
        "id": id,
        "name": format!("{id} name"),
        "version": v,
        "author": "a",
        "compat": { "game": "f1_25" },
        "requires": requires,
    }))
    .unwrap()
}

fn launcher() -> semver::Version {
    semver::Version::new(1, 4, 0)
}

fn core() -> AddonManifest {
    manifest(CORE_ID, "1.0.0", &[])
}

fn resolve(target: &Addon, catalog: &[Addon], installed: &[AddonManifest]) -> Result<Plan> {
    let have = Have::new(installed, |_| false);
    preview(
        target,
        target.offered(&launcher()).unwrap(),
        catalog,
        &have,
        &launcher(),
    )
    .unwrap()
}

fn labels(plan: &Plan) -> Vec<String> {
    plan.steps
        .iter()
        .map(|s| format!("{}@{}", s.addon.id, s.version.version))
        .collect()
}

/// A zip with the listed manifest under each of `manifests`.
fn package(path: &Path, manifests: &[&str]) {
    let text = r#"{"schema_version":1,"id":"t.a","name":"a","version":"1.0.0","author":"a","compat":{"game":"f1_25"}}"#;
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for name in manifests {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut z, text.as_bytes()).unwrap();
    }
    z.finish().unwrap();
}

fn checked(name: &str, manifests: &[&str]) -> Result<Vec<Requirement>> {
    let dir = std::env::temp_dir().join(format!("swofty-requires-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.a.swoftyaddon");
    package(&path, manifests);
    let a = addon("t.a", vec![version("1.0.0", &[])]);
    let result = requires_in(&path, &a, &a.versions[0]);
    std::fs::remove_dir_all(&dir).ok();
    result
}

#[test]
fn a_download_with_two_spellings_of_its_manifest_is_refused() {
    for alias in [
        "x/../swofty.json",
        "./swofty.json",
        "x\\..\\swofty.json",
        "SWOFTY.JSON",
        "swofty.json.",
        "swofty.json ",
        "swofty.json::$DATA",
    ] {
        let err = checked("alias", &["swofty.json", alias]).unwrap_err();
        assert!(
            err.to_string().contains("more than one swofty.json"),
            "{alias}: {err}"
        );
    }
    for names in [
        &["pack/swofty.json", "SWOFTY.JSON"],
        &["pack/swofty.json", "x/../PACK/swofty.json"],
    ] {
        let err = checked("nested", names).unwrap_err();
        assert!(
            err.to_string().contains("more than one swofty.json"),
            "{names:?}: {err}"
        );
    }
}

#[test]
fn a_download_with_one_manifest_passes_the_check() {
    checked("root", &["swofty.json"]).unwrap();
    checked("wrapped", &["pack/swofty.json"]).unwrap();
    checked("other-folder", &["swofty.json", "templates/swofty.json"]).unwrap();
}

fn refusal(result: Result<Plan>) -> Refusal {
    match result {
        Err(Error::Requires(r)) => r,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn updating_root_preserves_installed_dependents() {
    let target = addon("dependency", vec![version("2.0.0", &[])]);
    let installed = [
        manifest("dependency", "1.0.0", &[]),
        manifest("consumer", "1.0.0", &[("dependency", "^1")]),
    ];
    let failure = refusal(resolve(&target, std::slice::from_ref(&target), &installed));
    assert!(failure.to_string().contains("consumer name"));
    assert!(failure.to_string().contains("2.0.0"));

    let compatible = addon("dependency", vec![version("1.5.0", &[])]);
    assert!(resolve(&compatible, std::slice::from_ref(&compatible), &installed).is_ok());
}

#[test]
fn root_update_does_not_block_on_preexisting_unmet_requirement() {
    let target = addon("dependency", vec![version("3.0.0", &[])]);
    let installed = [
        manifest("dependency", "2.0.0", &[]),
        manifest("consumer", "1.0.0", &[("dependency", "^1")]),
    ];
    assert!(resolve(&target, std::slice::from_ref(&target), &installed).is_ok());
}

#[test]
fn installs_every_requirement_first() {
    let a = addon("a", vec![version("1.0.0", &[("b", "^1"), ("c", ">=2.0.0")])]);
    let catalog = vec![
        a.clone(),
        addon(
            "b",
            vec![
                version("2.0.0", &[]),
                version("1.2.0", &[]),
                version("1.0.0", &[]),
            ],
        ),
        addon("c", vec![version("2.1.0", &[]), version("1.0.0", &[])]),
    ];
    let plan = resolve(&a, &catalog, &[core()]).unwrap();
    assert_eq!(labels(&plan), ["b@1.2.0", "c@2.1.0", "a@1.0.0"]);
    assert_eq!(plan.addon().addon.id, "a");
    assert_eq!(plan.also().len(), 2);
    assert_eq!(plan.size(), 9);
}

#[test]
fn follows_requirements_of_requirements() {
    let a = addon("a", vec![version("1.0.0", &[("b", "*")])]);
    let catalog = vec![
        a.clone(),
        addon("b", vec![version("1.0.0", &[("c", "^0.3")])]),
        addon("c", vec![version("0.3.4", &[("d", "*")])]),
        addon("d", vec![version("5.0.0", &[])]),
    ];
    let plan = resolve(&a, &catalog, &[core()]).unwrap();
    assert_eq!(labels(&plan), ["d@5.0.0", "c@0.3.4", "b@1.0.0", "a@1.0.0"]);
}

#[test]
fn installed_and_built_in_requirements_need_nothing() {
    let a = addon(
        "a",
        vec![version(
            "1.0.0",
            &[("b", "^1"), (CORE_ID, ">=0.1.0"), ("tracks", "*")],
        )],
    );
    let catalog = vec![a.clone(), addon("b", vec![version("1.9.0", &[])])];
    let installed = [
        core(),
        manifest("b", "1.1.0", &[("not.anywhere", "*")]),
        manifest("tracks", "0.1.0", &[]),
    ];
    let have = Have::new(&installed, |id| id == "tracks");
    assert!(have.builtin.contains("tracks") && have.builtin.contains(CORE_ID));
    let plan = preview(&a, &a.versions[0], &catalog, &have, &launcher())
        .unwrap()
        .unwrap();
    assert_eq!(labels(&plan), ["a@1.0.0"]);
}

#[test]
fn updates_an_installed_requirement_that_does_not_match() {
    let a = addon("a", vec![version("1.0.0", &[("b", ">=1.2.0")])]);
    let catalog = vec![
        a.clone(),
        addon("b", vec![version("1.3.0", &[("c", "*")]), version("1.0.0", &[])]),
        addon("c", vec![version("1.0.0", &[])]),
    ];
    let plan = resolve(&a, &catalog, &[core(), manifest("b", "1.0.0", &[])]).unwrap();
    assert_eq!(labels(&plan), ["c@1.0.0", "b@1.3.0", "a@1.0.0"]);
    assert_eq!(plan.steps[1].replaces, Some(semver::Version::new(1, 0, 0)));
    assert_eq!(plan.steps[0].replaces, None);
}

#[test]
fn refuses_requirements_the_browser_does_not_have() {
    let a = addon(
        "a",
        vec![version("1.0.0", &[("gone", ">=1.0.0"), ("b", ">=3"), ("c", "*")])],
    );
    let catalog = vec![
        a.clone(),
        addon("b", vec![version("2.0.0", &[]), version("1.0.0", &[])]),
        addon("c", vec![version("1.0.0", &[("deep", "^2")])]),
    ];
    let r = refusal(resolve(&a, &catalog, &[core()]));
    assert_eq!(r.missing(), ["gone", "b", "deep"]);
    let text = r.to_string();
    assert!(
        text.contains("a name needs gone >=1.0.0, which isn't on the Addon Browser"),
        "{text}"
    );
    assert!(
        text.contains("a name needs b name (b) >=3; the Addon Browser has 2.0.0 and 1.0.0"),
        "{text}"
    );
    assert!(
        text.contains("c name needs deep ^2, which isn't on the Addon Browser"),
        "{text}"
    );
    assert!(text.ends_with("Nothing was installed."), "{text}");
}

#[test]
fn refuses_an_installed_requirement_the_browser_cannot_replace() {
    let a = addon("a", vec![version("1.0.0", &[("local", "^2")])]);
    let r = refusal(resolve(
        &a,
        std::slice::from_ref(&a),
        &[core(), manifest("local", "1.0.0", &[])],
    ));
    assert!(
        r.to_string()
            .contains("local name (local) ^2, which isn't on the Addon Browser; you have 1.0.0"),
        "{r}"
    );
    let b = addon("b", vec![version("1.0.0", &[(CORE_ID, ">=9.0.0")])]);
    let core_listed = addon(CORE_ID, vec![version("9.0.0", &[])]);
    let r = refusal(resolve(&b, &[b.clone(), core_listed], &[core()]));
    assert!(r.to_string().contains("this launcher has 1.0.0 built in"), "{r}");
    let c = addon("c", vec![version("1.0.0", &[("teamsimplified.mods", "^3")])]);
    let simplified = addon("teamsimplified.mods", vec![version("3.0.0", &[])]);
    let installed = [core(), manifest("teamsimplified.mods", "2.0.0", &[])];
    let r = refusal(resolve(&c, &[c.clone(), simplified], &installed));
    assert!(
        r.to_string()
            .contains("you have 2.0.0, which the Addon Browser doesn't replace"),
        "{r}"
    );
}

#[test]
fn skips_yanked_versions_and_ones_for_newer_launchers() {
    let a = addon("a", vec![version("1.0.0", &[("b", "^1")])]);
    let mut yanked = version("1.5.0", &[]);
    yanked.yanked = true;
    let mut newer = version("1.4.0", &[]);
    newer.launcher = Some(VersionReq::parse(">=9.0.0").unwrap());
    let catalog = vec![
        a.clone(),
        addon("b", vec![yanked, newer.clone(), version("1.1.0", &[])]),
    ];
    assert_eq!(
        labels(&resolve(&a, &catalog, &[core()]).unwrap()),
        ["b@1.1.0", "a@1.0.0"]
    );
    let only_newer = vec![a.clone(), addon("b", vec![newer])];
    let r = refusal(resolve(&a, &only_newer, &[core()]));
    assert!(r.to_string().contains("need a newer launcher"), "{r}");
}

#[test]
fn refuses_cycles() {
    let a = addon("a", vec![version("1.0.0", &[("b", "*")])]);
    let catalog = vec![
        a.clone(),
        addon("b", vec![version("1.0.0", &[("c", "*")])]),
        addon("c", vec![version("1.0.0", &[("a", "*"), ("c", "*")])]),
    ];
    let r = refusal(resolve(&a, &catalog, &[core()]));
    assert_eq!(
        r.problems,
        [
            Problem::Cycle(vec!["a".into(), "b".into(), "c".into()]),
            Problem::Cycle(vec!["c".into()]),
        ]
    );
    let text = r.to_string();
    assert!(text.contains("a, b and c require each other in a loop"), "{text}");
    assert!(text.contains("c requires itself"), "{text}");
    let pair = vec![
        addon("x", vec![version("1.0.0", &[("y", "*")])]),
        addon("y", vec![version("1.0.0", &[("x", "*")])]),
    ];
    let r = refusal(resolve(&pair[0], &pair, &[core()]));
    assert_eq!(
        r.to_string(),
        "x and y require each other. Nothing was installed."
    );
}

#[test]
fn picks_a_version_every_requirement_accepts() {
    let a = addon("a", vec![version("1.0.0", &[("b", "^1"), ("c", "*")])]);
    let catalog = vec![
        a.clone(),
        addon(
            "b",
            vec![
                version("1.5.0", &[]),
                version("1.1.0", &[]),
                version("0.9.0", &[]),
            ],
        ),
        addon("c", vec![version("1.0.0", &[("b", "<1.2.0")])]),
    ];
    let plan = resolve(&a, &catalog, &[core()]).unwrap();
    assert_eq!(labels(&plan), ["b@1.1.0", "c@1.0.0", "a@1.0.0"]);
    let clash = vec![
        a.clone(),
        catalog[1].clone(),
        addon("c", vec![version("1.0.0", &[("b", "<1.0.0")])]),
    ];
    let r = refusal(resolve(&a, &clash, &[core()]));
    let text = r.to_string();
    assert!(
        text.contains("a name needs b name (b) ^1 and c name needs b name (b) <1.0.0"),
        "{text}"
    );
}

#[test]
fn keeps_installed_addons_working() {
    let a = addon("a", vec![version("1.0.0", &[("b", "^2")])]);
    let catalog = vec![
        a.clone(),
        addon("b", vec![version("2.0.0", &[]), version("1.0.0", &[])]),
    ];
    let installed = [
        core(),
        manifest("b", "1.0.0", &[]),
        manifest("y", "1.0.0", &[("b", "^1")]),
    ];
    let r = refusal(resolve(&a, &catalog, &installed));
    assert!(r.to_string().contains("y name needs b name (b) ^1"), "{r}");
    let broken = [
        core(),
        manifest("b", "1.0.0", &[]),
        manifest("y", "1.0.0", &[("b", "^3")]),
    ];
    assert_eq!(
        labels(&resolve(&a, &catalog, &broken).unwrap()),
        ["b@2.0.0", "a@1.0.0"]
    );
}

#[test]
fn previews_only_what_the_registry_lists() {
    let mut a = addon("a", vec![version("1.0.0", &[("b", "*")])]);
    let mut b = addon("b", vec![version("1.0.0", &[])]);
    b.versions[0].requires = None;
    let have = Have::new(&[], |_| false);
    let catalog = vec![a.clone(), b];
    assert!(preview(&a, &a.versions[0], &catalog, &have, &launcher()).is_none());
    a.versions[0].requires = Some(Vec::new());
    assert!(preview(&a, &a.versions[0], &catalog, &have, &launcher()).is_some());
}

#[test]
fn reads_requires_from_the_registry_when_listed() {
    let listed = r#"{"version":"1.0.0","size":1,"sha256":"x","url":"u",
        "requires":[{"id":"b","version":">=1.0.0"},{"id":"c"}]}"#;
    let v: Version = serde_json::from_str(listed).unwrap();
    let reqs = v.requires.unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[1].version, VersionReq::STAR);
    let unlisted: Version =
        serde_json::from_str(r#"{"version":"1.0.0","size":1,"sha256":"x","url":"u"}"#).unwrap();
    assert!(unlisted.requires.is_none());
    let unreadable: Version = serde_json::from_str(
        r#"{"version":"1.0.0","size":1,"sha256":"x","url":"u","requires":[{"id":"b","version":"soon"}]}"#,
    )
    .unwrap();
    assert!(unreadable.requires.is_none());
}

fn zipped(prefix: &str, manifest: &AddonManifest) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    z.start_file(format!("{prefix}{MANIFEST_FILE}"), opts).unwrap();
    std::io::Write::write_all(&mut z, serde_json::to_string(manifest).unwrap().as_bytes()).unwrap();
    z.finish().unwrap().into_inner()
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut z, bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn shader_only(v: &str, icon: Option<&str>) -> String {
    let icon = icon.map_or(String::new(), |i| format!(r#""icon":"{i}","#));
    format!(
        r#"{{"schema_version":1,"id":"t.look","name":"Look","version":"{v}","author":"t",{icon}
        "category":"shaders","compat":{{"game":"f1_25","launcher":">=1.0.0"}},
        "shaders":[{{"config":"look.swfx"}}]}}"#
    )
}

#[test]
fn updates_a_shader_only_addon_whose_download_nests_its_older_package() {
    let root = std::env::temp_dir().join(format!("swofty-requires-shader-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let (old, new) = (
        shader_only("1.0.0", None),
        shader_only("1.1.0", Some("icons/look.png")),
    );
    let nested = zip_of(&[(MANIFEST_FILE, old.as_bytes()), ("look.swfx", b"look")]);
    let old_zip = zip_of(&[
        (MANIFEST_FILE, old.as_bytes()),
        ("look.swfx", b"look"),
        ("t.look.swoftyaddon", &nested),
    ]);
    let new_zip = zip_of(&[
        ("t.look/icons/look.png", b"png"),
        ("t.look/look.swfx", b"look"),
        ("t.look/swofty.json", new.as_bytes()),
        ("t.look/t.look.swoftyaddon", &nested),
    ]);
    let base = serve(vec![
        ("/old.zip".into(), old_zip.clone()),
        ("/new.zip".into(), new_zip.clone()),
    ]);
    let mut from = hosted(&base, "/old.zip", &old_zip, Some(&[]));
    let mut to = hosted(&base, "/new.zip", &new_zip, Some(&[]));
    from.version = semver::Version::new(1, 0, 0);
    to.version = semver::Version::new(1, 1, 0);
    let listed = addon("t.look", vec![to.clone(), from.clone()]);
    let catalog = vec![listed.clone()];
    store.save_profile(&crate::profile::Profile::new("p")).unwrap();

    let done = install_in_profile(&store, &catalog, &listed, &from, "p", &|_| {}).unwrap();
    assert!(done[0].is_shader_only());
    let due =
        crate::catalog::store_updates(&store, &catalog, &crate::updates::release::current_version()).unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].to.version, to.version);

    let done = install_in_profile(&store, &catalog, &listed, &due[0].to, "p", &|_| {}).unwrap();
    assert_eq!(done.last().unwrap().version, to.version);
    let installed = &store.addons().unwrap()["t.look"];
    assert_eq!(installed.version, to.version);
    assert!(store.addons_dir().join("t.look/icons/look.png").is_file());
    assert!(
        crate::catalog::store_updates(&store, &catalog, &crate::updates::release::current_version())
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir_all(&root).ok();
}

/// Serves `files` from a thread per request. Each response ends with a
/// write shutdown and a read to the end, so Windows closes the socket
/// without a reset that drops the body.
fn serve(files: Vec<(String, Vec<u8>)>) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let files = Arc::new(files);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let files = files.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).ok();
                let path = request.split_whitespace().nth(1).unwrap_or_default().to_owned();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                        break;
                    }
                }
                let body = files.iter().find(|(p, _)| *p == path).map(|(_, b)| b.clone());
                let (status, body) = body.map_or((404, Vec::new()), |b| (200, b));
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).ok();
                stream.write_all(&body).ok();
                stream.shutdown(std::net::Shutdown::Write).ok();
                std::io::copy(&mut reader, &mut std::io::sink()).ok();
            });
        }
    });
    base
}

fn hosted(base: &str, path: &str, zip: &[u8], listed: Option<&[(&str, &str)]>) -> Version {
    let mut v = version("1.0.0", listed.unwrap_or_default());
    v.requires = listed.map(|_| v.requires.clone().unwrap());
    v.size = zip.len() as u64;
    v.sha256 = crate::game_folder::baseline::sha256_bytes(zip);
    v.url = format!("{base}{path}");
    v
}

/// Serves `files` from a thread per request, each answered after its
/// delay in milliseconds, and counts the requests for each path.
fn serve_timed(files: Vec<(String, Vec<u8>, u64)>) -> (String, Arc<Mutex<BTreeMap<String, usize>>>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let files = Arc::new(files);
    let hits = Arc::new(Mutex::new(BTreeMap::new()));
    let counted = hits.clone();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let (files, hits) = (files.clone(), counted.clone());
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).ok();
                let path = request.split_whitespace().nth(1).unwrap_or_default().to_owned();
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                        break;
                    }
                }
                *hits.lock().unwrap().entry(path.clone()).or_insert(0) += 1;
                let found = files.iter().find(|(p, _, _)| *p == path);
                if let Some((_, _, ms)) = found {
                    std::thread::sleep(std::time::Duration::from_millis(*ms));
                }
                let (status, body) = found.map_or((404, Vec::new()), |(_, b, _)| (200, b.clone()));
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).ok();
                stream.write_all(&body).ok();
                stream.shutdown(std::net::Shutdown::Write).ok();
                std::io::copy(&mut reader, &mut std::io::sink()).ok();
            });
        }
    });
    (base, hits)
}

#[test]
fn installs_at_once_share_a_requirement_download_and_install_it_once() {
    let root = std::env::temp_dir().join(format!("swofty-requires-shared-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let a_zip = zipped("", &manifest("t.a", "1.0.0", &[("t.c", "^1")]));
    let b_zip = zipped("", &manifest("t.b", "1.0.0", &[("t.c", "^1")]));
    let c_zip = zipped("", &manifest("t.c", "1.0.0", &[]));
    let (base, hits) = serve_timed(vec![
        ("/a.zip".into(), a_zip.clone(), 300),
        ("/b.zip".into(), b_zip.clone(), 300),
        ("/c.zip".into(), c_zip.clone(), 300),
    ]);
    let a = addon(
        "t.a",
        vec![hosted(&base, "/a.zip", &a_zip, Some(&[("t.c", "^1")]))],
    );
    let b = addon(
        "t.b",
        vec![hosted(&base, "/b.zip", &b_zip, Some(&[("t.c", "^1")]))],
    );
    let c = addon("t.c", vec![hosted(&base, "/c.zip", &c_zip, Some(&[]))]);
    let catalog = vec![a.clone(), b.clone(), c];
    let installed = Arc::new(Mutex::new(Vec::new()));
    let start = Arc::new(std::sync::Barrier::new(2));
    let runs: Vec<_> = [a, b]
        .into_iter()
        .map(|target| {
            let (store, catalog, start, installed) =
                (store.clone(), catalog.clone(), start.clone(), installed.clone());
            std::thread::spawn(move || {
                start.wait();
                let current = RefCell::new(String::new());
                install(&store, &catalog, &target, &target.versions[0], &|e| match e {
                    Event::Addon(a, _) => *current.borrow_mut() = a.id.clone(),
                    Event::Stage(Stage::Installing) => {
                        installed.lock().unwrap().push(current.borrow().clone())
                    }
                    _ => {}
                })
            })
        })
        .collect();
    let done: Vec<Vec<String>> = runs
        .into_iter()
        .map(|r| r.join().unwrap().unwrap().iter().map(|m| m.id.clone()).collect())
        .collect();
    assert!(
        done.iter().any(|d| *d == ["t.c", "t.a"] || *d == ["t.a"]),
        "{done:?}"
    );
    assert!(
        done.iter().any(|d| *d == ["t.c", "t.b"] || *d == ["t.b"]),
        "{done:?}"
    );
    assert_eq!(
        done.iter().flatten().filter(|id| *id == "t.c").count(),
        1,
        "{done:?}"
    );
    let mut order = installed.lock().unwrap().clone();
    order.sort();
    assert_eq!(order, ["t.a", "t.b", "t.c"]);
    assert_eq!(hits.lock().unwrap().get("/c.zip"), Some(&1));
    let addons = store.addons().unwrap();
    assert!(["t.a", "t.b", "t.c"].iter().all(|id| addons.contains_key(*id)));
    let downloads = store.root.join(super::super::CACHE_DIR).join("downloads");
    assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
    assert!(shared().keys().all(|p| !p.starts_with(&root)));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn concurrent_incompatible_late_requirements_do_not_break_an_installed_addon() {
    let root = std::env::temp_dir().join(format!("swofty-requires-conflict-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let a_zip = zipped("", &manifest("t.a", "1.0.0", &[("t.c", "^1")]));
    let b_zip = zipped("", &manifest("t.b", "1.0.0", &[("t.c", "^2")]));
    let c1_zip = zipped("", &manifest("t.c", "1.0.0", &[]));
    let c2_zip = zipped("", &manifest("t.c", "2.0.0", &[]));
    let base = serve(vec![
        ("/a.zip".into(), a_zip.clone()),
        ("/b.zip".into(), b_zip.clone()),
        ("/c1.zip".into(), c1_zip.clone()),
        ("/c2.zip".into(), c2_zip.clone()),
    ]);
    let a = addon("t.a", vec![hosted(&base, "/a.zip", &a_zip, None)]);
    let b = addon("t.b", vec![hosted(&base, "/b.zip", &b_zip, None)]);
    let c1 = hosted(&base, "/c1.zip", &c1_zip, Some(&[]));
    let mut c2 = hosted(&base, "/c2.zip", &c2_zip, Some(&[]));
    c2.version = semver::Version::new(2, 0, 0);
    let catalog = vec![a.clone(), b.clone(), addon("t.c", vec![c2, c1])];
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let runs: Vec<_> = [a, b]
        .into_iter()
        .map(|target| {
            let (store, catalog, barrier) = (store.clone(), catalog.clone(), barrier.clone());
            std::thread::spawn(move || {
                let waited = std::cell::Cell::new(false);
                install(&store, &catalog, &target, &target.versions[0], &|event| {
                    if matches!(event, Event::Planned(_)) && !waited.replace(true) {
                        barrier.wait();
                    }
                })
            })
        })
        .collect();
    let results: Vec<_> = runs.into_iter().map(|run| run.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "{results:?}"
    );
    assert!(
        results
            .iter()
            .any(|result| matches!(result, Err(Error::Requires(_))))
    );
    let installed = store.addons().unwrap();
    let dep = &installed["t.c"].version;
    let consumer = installed.get("t.a").or_else(|| installed.get("t.b")).unwrap();
    assert!(
        consumer
            .requires
            .iter()
            .any(|r| r.id == "t.c" && crate::addons::version::satisfies(&r.version, dep))
    );
    assert_ne!(installed.contains_key("t.a"), installed.contains_key("t.b"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn catalog_job_keeps_requested_profile_after_switch_or_deletion() {
    for deleted in [false, true] {
        let root = std::env::temp_dir().join(format!(
            "swofty-requires-profile-{}-{deleted}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&root).ok();
        let store = Store::new(&root);
        store.create_profile("source", None).unwrap();
        store.create_profile("other", None).unwrap();
        store.set_active_profile("source").unwrap();
        let root_zip = zipped("", &manifest("t.root", "1.0.0", &[("t.dep", "^1")]));
        let dep_zip = zipped("", &manifest("t.dep", "1.0.0", &[]));
        let base = serve(vec![
            ("/root.zip".into(), root_zip.clone()),
            ("/dep.zip".into(), dep_zip.clone()),
        ]);
        let target = addon(
            "t.root",
            vec![hosted(&base, "/root.zip", &root_zip, Some(&[("t.dep", "^1")]))],
        );
        let dep = addon("t.dep", vec![hosted(&base, "/dep.zip", &dep_zip, Some(&[]))]);
        let catalog = vec![target.clone(), dep];
        let switched = std::cell::Cell::new(false);
        install_in_profile(
            &store,
            &catalog,
            &target,
            &target.versions[0],
            "source",
            &|event| {
                if matches!(event, Event::Planned(_)) && !switched.replace(true) {
                    store.set_active_profile("other").unwrap();
                    if deleted {
                        store.delete_profile("source").unwrap();
                    }
                }
            },
        )
        .unwrap();
        let other = store.profile("other").unwrap();
        assert!(other.has("t.root") && other.has("t.dep"));
        assert!(other.enabled().all(|id| id != "t.root" && id != "t.dep"));
        if deleted {
            assert!(!store.profile_names().unwrap().iter().any(|name| name == "source"));
        } else {
            let source = store.profile("source").unwrap();
            assert!(source.has("t.root") && source.has("t.dep"));
        }
        std::fs::remove_dir_all(&root).ok();
    }
}

#[test]
fn failed_root_reports_dependencies_installed_before_failure() {
    let root = std::env::temp_dir().join(format!("swofty-requires-partial-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let dependency = zipped("", &manifest("t.dep", "1.0.0", &[]));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file(MANIFEST_FILE, zip::write::SimpleFileOptions::default())
        .unwrap();
    std::io::Write::write_all(
        &mut zip,
        br#"{"id":"t.root","version":"1.0.0","requires":[{"id":"t.dep","version":"^1"}]}"#,
    )
    .unwrap();
    let invalid_root = zip.finish().unwrap().into_inner();
    let base = serve(vec![
        ("/root.zip".into(), invalid_root.clone()),
        ("/dep.zip".into(), dependency.clone()),
    ]);
    let target = addon("t.root", vec![hosted(&base, "/root.zip", &invalid_root, None)]);
    let dep = addon("t.dep", vec![hosted(&base, "/dep.zip", &dependency, Some(&[]))]);
    let catalog = vec![target.clone(), dep];
    let installed = RefCell::new(Vec::new());
    let result = install(&store, &catalog, &target, &target.versions[0], &|event| {
        if let Event::Installed(manifest) = event {
            installed.borrow_mut().push(manifest.id.clone());
        }
    });
    assert!(result.is_err(), "invalid root should fail to install");
    assert_eq!(*installed.borrow(), ["t.dep"]);
    assert!(store.addons().unwrap().contains_key("t.dep"));
    assert!(!store.addons().unwrap().contains_key("t.root"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_failed_install_leaves_a_shared_download_to_the_install_still_using_it() {
    let root = std::env::temp_dir().join(format!("swofty-requires-failed-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let a_zip = zipped("", &manifest("t.a", "1.0.0", &[("t.c", "^1")]));
    let z_zip = zipped("", &manifest("t.z", "1.0.0", &[("t.c", "^1")]));
    let c_zip = zipped("", &manifest("t.c", "1.0.0", &[]));
    let (base, _) = serve_timed(vec![
        ("/a.zip".into(), a_zip.clone(), 600),
        ("/c.zip".into(), c_zip.clone(), 100),
    ]);
    let a = addon(
        "t.a",
        vec![hosted(&base, "/a.zip", &a_zip, Some(&[("t.c", "^1")]))],
    );
    let z = addon(
        "t.z",
        vec![hosted(&base, "/z-missing.zip", &z_zip, Some(&[("t.c", "^1")]))],
    );
    let c = addon("t.c", vec![hosted(&base, "/c.zip", &c_zip, Some(&[]))]);
    let catalog = vec![a.clone(), z.clone(), c];
    let start = Arc::new(std::sync::Barrier::new(2));
    let runs: Vec<_> = [a, z]
        .into_iter()
        .map(|target| {
            let (store, catalog, start) = (store.clone(), catalog.clone(), start.clone());
            std::thread::spawn(move || {
                start.wait();
                install(&store, &catalog, &target, &target.versions[0], &|_| {})
            })
        })
        .collect();
    let mut results: Vec<_> = runs.into_iter().map(|r| r.join().unwrap()).collect();
    let z_result = results.pop().unwrap();
    let a_result = results.pop().unwrap();
    let err = z_result.unwrap_err().to_string();
    assert!(err.contains("/z-missing.zip: HTTP 404"), "{err}");
    let ids: Vec<String> = a_result.unwrap().iter().map(|m| m.id.clone()).collect();
    assert_eq!(ids, ["t.c", "t.a"]);
    let downloads = store.root.join(super::super::CACHE_DIR).join("downloads");
    assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn installs_requirements_read_from_the_download() {
    let root = std::env::temp_dir().join(format!("swofty-requires-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = Store::new(&root);
    let a_zip = zipped("", &manifest("t.a", "1.0.0", &[("t.b", "^1")]));
    let b_zip = zipped("b/", &manifest("t.b", "1.0.0", &[]));
    let z_zip = zipped("", &manifest("t.z", "1.0.0", &[("t.gone", ">=2.0.0")]));
    let base = serve(vec![
        ("/a.zip".into(), a_zip.clone()),
        ("/b.zip".into(), b_zip.clone()),
        ("/z.zip".into(), z_zip.clone()),
    ]);
    let a = addon("t.a", vec![hosted(&base, "/a.zip", &a_zip, None)]);
    let b = addon("t.b", vec![hosted(&base, "/b.zip", &b_zip, Some(&[]))]);
    let z = addon("t.z", vec![hosted(&base, "/z.zip", &z_zip, None)]);
    let catalog = vec![a.clone(), b, z.clone()];
    let seen = RefCell::new(Vec::new());
    let installed_events = RefCell::new(Vec::new());
    let done = install(&store, &catalog, &a, &a.versions[0], &|e| match e {
        Event::Planned(p) => seen.borrow_mut().push(format!("plan {}", labels(p).join(" "))),
        Event::Addon(a, _) => seen.borrow_mut().push(a.id.clone()),
        Event::Stage(Stage::Installing) => seen.borrow_mut().push("installing".into()),
        Event::Installed(m) => installed_events.borrow_mut().push(m.id.clone()),
        Event::Stage(_) => {}
    })
    .unwrap();
    let ids: Vec<&str> = done.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["t.b", "t.a"]);
    assert_eq!(*installed_events.borrow(), ["t.b", "t.a"]);
    assert_eq!(
        *seen.borrow(),
        [
            "t.a",
            "plan t.b@1.0.0 t.a@1.0.0",
            "t.b",
            "t.b",
            "installing",
            "t.a",
            "installing"
        ]
    );
    let addons = store.addons().unwrap();
    assert!(addons.contains_key("t.a") && addons.contains_key("t.b"));
    let downloads = store.root.join(super::super::CACHE_DIR).join("downloads");
    assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);

    let err = install(&store, &catalog, &z, &z.versions[0], &|_| {}).unwrap_err();
    assert!(matches!(err, Error::Requires(_)), "{err}");
    assert!(!store.addons().unwrap().contains_key("t.z"));
    assert_eq!(std::fs::read_dir(&downloads).unwrap().count(), 0);
    std::fs::remove_dir_all(&root).ok();
}
