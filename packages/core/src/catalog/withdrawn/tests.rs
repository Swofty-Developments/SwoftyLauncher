use std::collections::BTreeMap;
use std::path::PathBuf;

use super::*;
use crate::catalog::Version;
use crate::manifest::MANIFEST_FILE;

const NOW: i64 = 1_800_000_000;

fn v(text: &str) -> semver::Version {
    semver::Version::parse(text).unwrap()
}

fn version(text: &str, yanked: bool) -> Version {
    Version {
        version: v(text),
        launcher: None,
        game: "f1_25".into(),
        size: 1,
        sha256: "00".into(),
        url: format!("http://localhost/{text}.zip"),
        published_at: String::new(),
        changelog: String::new(),
        native: false,
        yanked,
        requires: None,
        platforms: Vec::new(),
    }
}

fn addon(id: &str, versions: Vec<Version>) -> Addon {
    serde_json::from_value::<Addon>(serde_json::json!({ "id": id, "name": format!("{id} name") }))
        .map(|mut a| {
            a.latest = versions.iter().find(|v| !v.yanked).cloned();
            a.versions = versions;
            a
        })
        .unwrap()
}

fn manifest(id: &str, version: &str) -> AddonManifest {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "id": id, "name": format!("{id} name"), "version": version,
        "author": "a", "compat": { "game": "f1_25" }
    }))
    .unwrap()
}

fn check(version: &str, withdrawn: bool, checked: i64) -> Check {
    Check {
        version: v(version),
        withdrawn,
        checked,
    }
}

fn saved(entries: &[(&str, Check)]) -> Checks {
    Checks(
        entries
            .iter()
            .map(|(id, c)| ((*id).to_owned(), c.clone()))
            .collect(),
    )
}

/// Runs [`update`] with a registry that answers from `pages`, and the ids it was asked about.
fn run(
    saved: &Checks,
    installed: &[AddonManifest],
    catalog: &[Addon],
    pages: &[(&str, Result<Addon, Trouble>)],
) -> (Checks, Vec<String>, Vec<String>) {
    let mut asked = Vec::new();
    let (checks, lines) = update(saved, installed, catalog, NOW, &mut |id| {
        asked.push(id.to_owned());
        pages
            .iter()
            .find(|(page, _)| *page == id)
            .map(|(_, answer)| answer.clone())
            .unwrap_or_else(|| Err(Trouble::Failed(format!("http://x/v1/addons/{id}: HTTP 404"))))
    });
    (checks, lines, asked)
}

fn bmw_page() -> Addon {
    addon(
        "tom.bmw",
        vec![
            version("1.0.7", true),
            version("1.0.6", true),
            version("1.0.2", true),
        ],
    )
}

#[test]
fn an_installed_version_the_registry_withdrew_is_found_and_logged() {
    let installed = [manifest("tom.bmw", "1.0.7"), manifest("a.listed", "2.0.0")];
    let catalog = [addon("a.listed", vec![version("2.0.0", false)])];
    let (checks, lines, asked) = run(
        &Checks::default(),
        &installed,
        &catalog,
        &[("tom.bmw", Ok(bmw_page()))],
    );
    assert_eq!(asked, ["tom.bmw"]);
    assert_eq!(checks, saved(&[("tom.bmw", check("1.0.7", true, NOW))]));
    assert!(checks.is_withdrawn("tom.bmw", &v("1.0.7")));
    assert!(!checks.is_withdrawn("tom.bmw", &v("1.0.8")));
    assert_eq!(
        lines,
        ["tom.bmw 1.0.7 is installed and was withdrawn from the Addon Browser"]
    );
}

#[test]
fn a_withdrawn_version_of_an_addon_still_listed_is_found() {
    let installed = [manifest("a.b", "1.1.0")];
    let catalog = [addon("a.b", vec![version("1.0.0", false)])];
    let page = addon("a.b", vec![version("1.1.0", true), version("1.0.0", false)]);
    let (checks, _, asked) = run(&Checks::default(), &installed, &catalog, &[("a.b", Ok(page))]);
    assert_eq!(asked, ["a.b"]);
    assert!(checks.is_withdrawn("a.b", &v("1.1.0")));
}

#[test]
fn a_version_the_catalog_lists_again_is_no_longer_withdrawn() {
    let before = saved(&[("tom.bmw", check("1.0.7", true, NOW - 60))]);
    let catalog = [addon("tom.bmw", vec![version("1.0.7", false)])];
    let (checks, lines, asked) = run(&before, &[manifest("tom.bmw", "1.0.7")], &catalog, &[]);
    assert!(asked.is_empty());
    assert!(lines.is_empty());
    assert_eq!(checks, Checks::default());
}

#[test]
fn a_saved_answer_is_kept_without_asking_until_it_is_due() {
    let before = saved(&[
        ("tom.bmw", check("1.0.7", true, NOW - 10 * RECHECK_SECS)),
        ("a.dev", check("0.1.0", false, NOW - RECHECK_SECS + 1)),
        ("a.old", check("1.0.0", false, NOW - RECHECK_SECS)),
    ]);
    let installed = [
        manifest("tom.bmw", "1.0.7"),
        manifest("a.dev", "0.1.0"),
        manifest("a.old", "1.0.0"),
    ];
    let (checks, lines, asked) = run(&before, &installed, &[], &[]);
    assert_eq!(asked, ["a.old"]);
    assert!(lines.is_empty(), "{lines:?}");
    assert_eq!(checks.0["tom.bmw"], before.0["tom.bmw"]);
    assert_eq!(checks.0["a.dev"], before.0["a.dev"]);
    assert_eq!(checks.0["a.old"], check("1.0.0", false, NOW));
}

#[test]
fn an_addon_updated_to_another_version_is_asked_about_again() {
    let before = saved(&[("tom.bmw", check("1.0.6", true, NOW - 60))]);
    let page = addon("tom.bmw", vec![version("1.0.8", false), version("1.0.6", true)]);
    let (checks, _, asked) = run(
        &before,
        &[manifest("tom.bmw", "1.0.8")],
        &[],
        &[("tom.bmw", Ok(page))],
    );
    assert_eq!(asked, ["tom.bmw"]);
    assert_eq!(checks.0["tom.bmw"], check("1.0.8", false, NOW));
}

#[test]
fn an_addon_the_registry_does_not_have_is_not_withdrawn() {
    let (checks, lines, _) = run(&Checks::default(), &[manifest("me.local", "0.0.1")], &[], &[]);
    assert_eq!(checks, saved(&[("me.local", check("0.0.1", false, NOW))]));
    assert!(lines.is_empty());
}

#[test]
fn offline_keeps_the_saved_answers_and_stops_asking() {
    let before = saved(&[("tom.bmw", check("1.0.7", false, NOW - RECHECK_SECS))]);
    let installed = [manifest("tom.bmw", "1.0.7"), manifest("a.new", "1.0.0")];
    let offline = || Err(Trouble::Offline("http://x: connection refused".into()));
    let (checks, lines, asked) = run(
        &before,
        &installed,
        &[],
        &[("tom.bmw", offline()), ("a.new", offline())],
    );
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(checks, before);
    assert!(lines.is_empty());
}

#[test]
fn a_registry_error_keeps_no_answer_so_the_next_fetch_asks_again() {
    let failed = Err(Trouble::Failed("http://x/v1/addons/a.b: HTTP 502".into()));
    let (checks, _, asked) = run(
        &Checks::default(),
        &[manifest("a.b", "1.0.0")],
        &[],
        &[("a.b", failed)],
    );
    assert_eq!(asked, ["a.b"]);
    assert_eq!(checks, Checks::default());
}

#[test]
fn one_fetch_asks_about_a_bounded_number_of_addons() {
    let installed: Vec<AddonManifest> = (0..ASKED_PER_FETCH + 5)
        .map(|i| manifest(&format!("a.n{i:02}"), "1.0.0"))
        .collect();
    let (checks, _, asked) = run(&Checks::default(), &installed, &[], &[]);
    assert_eq!(asked.len(), ASKED_PER_FETCH);
    assert_eq!(checks.0.len(), ASKED_PER_FETCH);
}

#[test]
fn among_names_the_withdrawn_versions_in_order() {
    let checks = saved(&[
        ("tom.bmw", check("1.0.7", true, NOW)),
        ("a.old", check("1.0.0", true, NOW)),
        ("a.dev", check("0.1.0", false, NOW)),
    ]);
    let installed: BTreeMap<String, AddonManifest> = [
        manifest("tom.bmw", "1.0.7"),
        manifest("a.old", "1.1.0"),
        manifest("a.dev", "0.1.0"),
        manifest("a.z", "1.0.0"),
    ]
    .into_iter()
    .map(|m| (m.id.clone(), m))
    .collect();
    let found = checks.among(
        &installed,
        ["a.z", "a.dev", "a.old", "tom.bmw", "tom.bmw", "a.gone"],
    );
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].id, "tom.bmw");
    assert_eq!(found[0].label(), "tom.bmw name 1.0.7");
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-withdrawn-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    root
}

fn install(store: &Store, m: &AddonManifest) {
    let dir = store.addons_dir().join(&m.id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(MANIFEST_FILE), serde_json::to_vec(m).unwrap()).unwrap();
}

#[test]
fn enabled_reads_the_saved_answers_for_the_addons_that_are_on() {
    let root = scratch("enabled");
    let store = Store::new(&root);
    install(&store, &manifest("tom.bmw", "1.0.7"));
    install(&store, &manifest("a.other", "1.0.0"));
    assert!(enabled(&store, ["tom.bmw"]).is_empty());
    saved(&[("tom.bmw", check("1.0.7", true, NOW))])
        .save(&root)
        .unwrap();
    assert_eq!(
        Checks::load(&root),
        saved(&[("tom.bmw", check("1.0.7", true, NOW))])
    );
    let found = enabled(&store, ["a.other", "tom.bmw"]);
    assert_eq!(
        found.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(),
        ["tom.bmw"]
    );
    assert!(enabled(&store, ["a.other"]).is_empty());
    std::fs::write(root.join(FILE), b"not json").unwrap();
    assert_eq!(Checks::load(&root), Checks::default());
    std::fs::remove_dir_all(&root).ok();
}
