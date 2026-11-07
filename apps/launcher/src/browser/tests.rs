use std::collections::{BTreeMap, HashSet};

use swofty_core::catalog::requires::{Event, Plan, Step};
use swofty_core::catalog::{Addon, Stage, Update, Version};

use super::detail::shown_versions;
use super::host;
use super::jobs::{RetryAction, requested_version_available, retry_action};
use super::model::{Job, Progress, Verb};
use super::updates::{due_updates, held_failure};
use crate::format;

fn versions(list: &[&str]) -> Vec<Version> {
    list.iter()
        .map(|v| {
            serde_json::from_value(serde_json::json!({
                "version": v, "size": 0, "sha256": "", "url": ""
            }))
            .unwrap()
        })
        .collect()
}

fn shown(list: &[&str], installed: &str) -> usize {
    let all = versions(list);
    let refs: Vec<&Version> = all.iter().collect();
    shown_versions(&refs, &semver::Version::parse(installed).unwrap())
}

#[test]
fn shows_the_newer_versions_and_the_installed_one() {
    assert_eq!(shown(&["1.3.0", "1.2.0", "1.1.0", "1.0.0"], "1.1.0"), 3);
}

#[test]
fn shows_two_when_up_to_date() {
    assert_eq!(shown(&["1.2.0", "1.1.0", "1.0.0"], "1.2.0"), 2);
    assert_eq!(shown(&["1.2.0"], "1.2.0"), 1);
}

#[test]
fn an_unlisted_install_stops_at_the_versions_above_it() {
    assert_eq!(shown(&["1.3.0", "1.2.0", "1.0.0"], "1.1.0"), 2);
    assert_eq!(shown(&["1.3.0", "1.2.5", "1.2.0", "1.0.0"], "1.1.0"), 3);
    assert_eq!(shown(&["1.3.0", "1.2.0"], "2.0.0"), 2);
}

fn step(id: &str, size: u64) -> Step {
    let addon: Addon =
        serde_json::from_value(serde_json::json!({ "id": id, "name": format!("{id} name") })).unwrap();
    let version: Version = serde_json::from_value(serde_json::json!({
        "version": "1.0.0", "size": size, "sha256": "", "url": ""
    }))
    .unwrap();
    Step {
        addon,
        version,
        replaces: None,
    }
}

#[test]
fn queued_install_reports_partial_failure_and_retries_without_duplication() {
    use crate::download_queue::{Queue, State};
    use std::collections::BTreeSet;
    let mut queue = Queue::default();
    assert!(queue.enqueue("root".into(), "1.0.0", BTreeSet::new()));
    assert!(!queue.enqueue("root".into(), "2.0.0", BTreeSet::new()));
    assert_eq!(queue.start_next().as_deref(), Some("root"));
    let job = Job::new(100);
    let dependency: swofty_core::manifest::AddonManifest = serde_json::from_value(serde_json::json!({
        "schema_version": 1, "id": "dep", "name": "Dependency", "version": "1.0.0",
        "author": "test", "compat": {"game": "f1_25"}
    }))
    .unwrap();
    job.track("root", Event::Installed(&dependency));
    assert!(queue.finish("root", Err(job.failure("root failed"))));
    assert!(matches!(&queue.jobs()[0].state, State::Failed(message)
        if message.contains("root failed") && message.contains("Dependency 1.0.0")));
    assert!(queue.retry_at(0, "root"));
    assert_eq!(queue.jobs()[0].request, "1.0.0");
    assert!(matches!(queue.jobs()[0].state, State::Queued));
    assert_eq!(queue.start_next().as_deref(), Some("root"));
    assert!(queue.finish("root", Ok(())));
    assert_eq!(queue.pending(), 0);
    assert!(matches!(queue.jobs()[0].state, State::Completed));
}

#[test]
fn retry_action_distinguishes_original_and_current_versions() {
    let old = step("test.addon", 100);
    let mut addon = old.addon.clone();
    addon.versions = vec![old.version.clone()];
    assert_eq!(retry_action(Some(&addon), &old.version), RetryAction::Retry);
    addon.versions[0].yanked = true;
    assert_eq!(retry_action(Some(&addon), &old.version), RetryAction::Unavailable);
    let mut newer = old.version.clone();
    newer.version = "2.0.0".parse().unwrap();
    addon.versions.push(newer);
    assert_eq!(
        retry_action(Some(&addon), &old.version),
        RetryAction::RequestCurrent
    );
    assert_eq!(retry_action(None, &old.version), RetryAction::Unavailable);
}

#[test]
fn queued_version_must_remain_available_and_unchanged() {
    let requested = step("test.addon", 100);
    let mut addon = requested.addon.clone();
    addon.versions = vec![requested.version.clone()];
    assert!(requested_version_available(Some(&addon), &requested.version));
    assert!(!requested_version_available(None, &requested.version));
    addon.versions[0].yanked = true;
    assert!(!requested_version_available(Some(&addon), &requested.version));
    addon.versions[0].yanked = false;
    addon.versions[0].sha256 = "changed".into();
    assert!(!requested_version_available(Some(&addon), &requested.version));
    addon.versions[0] = requested.version.clone();
    addon.versions[0].launcher = Some(">=999.0.0".parse().unwrap());
    assert!(!requested_version_available(Some(&addon), &requested.version));
}

#[test]
fn tallies_every_download_of_an_install() {
    let (dep, root) = (step("dep", 300), step("root", 100));
    let job = Job::new(100);
    job.track("root", Event::Addon(&root.addon, &root.version));
    job.track("root", Event::Stage(Stage::Downloading(100)));
    let plan = Plan {
        steps: vec![dep.clone(), root.clone()],
    };
    job.track("root", Event::Planned(&plan));
    assert_eq!(job.also(), ["dep name 1.0.0"]);
    job.track("root", Event::Addon(&dep.addon, &dep.version));
    job.track("root", Event::Stage(Stage::Downloading(200)));
    assert_eq!(job.item().as_deref(), Some("dep name"));
    let Progress::Downloading { done, size, .. } = job.progress() else {
        panic!("still downloading");
    };
    assert_eq!((done, size), (300, 400));
    assert!(
        job.progress()
            .long(job.item().as_deref())
            .starts_with("Downloading dep name · ")
    );
    job.track("root", Event::Addon(&root.addon, &root.version));
    job.track("root", Event::Stage(Stage::Installing));
    assert_eq!(job.item(), None);
    assert_eq!(job.progress().long(None), "Installing…");
}

#[test]
fn names_sizes_counts_and_hosts() {
    assert_eq!(host("https://api.f1.swofty.net"), "api.f1.swofty.net");
    assert_eq!(host("http://localhost:8471"), "localhost:8471");
    assert_eq!(format::size(512), "512 B");
    assert_eq!(format::size(1_892_759), "1.8 MB");
    assert_eq!(format::compact(999), "999");
    assert_eq!(format::compact(3412), "3.4k");
    assert_eq!(format::compact(41_000), "41k");
}

fn update(id: &str, to: &str) -> Update {
    Update {
        id: id.into(),
        name: id.into(),
        installed: semver::Version::new(1, 0, 0),
        to: serde_json::from_value(serde_json::json!({
            "version": to, "size": 1, "sha256": "", "url": ""
        }))
        .unwrap(),
    }
}

#[test]
fn only_addons_that_are_on_hold_up_play() {
    let updates = [update("on", "1.1.0"), update("off", "1.1.0")];
    let on = HashSet::from(["on"]);
    let due = due_updates(&updates, &on, &BTreeMap::new());
    assert_eq!(due.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["on"]);
    assert!(due_updates(&updates, &HashSet::new(), &BTreeMap::new()).is_empty());
}

#[test]
fn a_successfully_updated_addon_is_not_reported_as_failed_during_refresh() {
    assert!(!held_failure(""));
    assert!(held_failure("network error"));
}

#[test]
fn failed_install_toast_title_names_the_addon() {
    assert_eq!(Verb::Install.failure_title("Foo", false), "Could not install Foo");
    assert_eq!(
        Verb::Update.failure_title("Foo", true),
        "Can't update Foo: missing requirements"
    );
}

#[test]
fn an_update_that_failed_at_that_version_is_skipped() {
    let updates = [update("on", "1.1.0")];
    let on = HashSet::from(["on"]);
    let mut held = BTreeMap::new();
    held.insert("on".to_owned(), (semver::Version::new(1, 1, 0), "x".to_owned()));
    assert!(due_updates(&updates, &on, &held).is_empty());
    held.insert("on".to_owned(), (semver::Version::new(1, 0, 5), "x".to_owned()));
    assert_eq!(due_updates(&updates, &on, &held).len(), 1);
}
