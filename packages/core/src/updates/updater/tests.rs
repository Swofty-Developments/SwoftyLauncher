use std::sync::Mutex;

use semver::{Version, VersionReq};

use super::apply::*;
use super::catalog::*;
use super::plan::*;
use super::probe::*;
use super::track::*;
use super::*;
use crate::launcher::Progress;
use crate::updates::managed;
use crate::updates::release::{self, Failure, Release};
use crate::updates::release::{Asset, Install, SUMS_FILE};
use crate::{Error, Result};

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

fn asset(name: &str) -> Asset {
    Asset {
        name: name.into(),
        size: 10,
        browser_download_url: format!("https://example.invalid/{name}"),
        digest: None,
    }
}

fn launcher_release(tag: &str) -> Release {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let platform = release::asset_name(tag, os, arch).unwrap_or_else(|| "none".into());
    Release {
        tag_name: tag.into(),
        assets: vec![asset(&platform), asset(SUMS_FILE)],
        ..Default::default()
    }
}

fn addon_release(tag: &str) -> Release {
    Release {
        tag_name: tag.into(),
        assets: vec![asset(managed::ASSET), asset(SUMS_FILE)],
        ..Default::default()
    }
}

struct Log(Mutex<Vec<(f32, String)>>);

impl Progress for Log {
    fn report(&self, done: f32, text: &str) {
        self.0.lock().unwrap().push((done, text.to_owned()));
    }
}

#[test]
fn a_cdn_manifest_sets_the_offer_without_a_probe() {
    let mut t = Track::default();
    let mut bare = launcher_release("v1.0.9");
    bare.assets.retain(|a| a.name != SUMS_FILE);
    t.on_manifest(Part::Launcher, bare, 100);
    assert_eq!(t.probe_at, 100 + CHECK_INTERVAL.as_secs());
    assert!(t.tag.is_none() && t.error.is_some());
    let mut ok = launcher_release("v1.0.9");
    ok.assets.retain(|a| a.name != SUMS_FILE);
    for a in &mut ok.assets {
        a.digest = Some(format!("sha256:{}", "a".repeat(64)));
    }
    t.on_manifest(Part::Launcher, ok, 200);
    assert!(t.error.is_none());
    assert_eq!(
        t.offer(Some(&v("1.0.8"))).map(|r| r.tag_name.as_str()),
        Some("v1.0.9")
    );
    assert!(t.offer(Some(&v("1.0.9"))).is_none());
}

#[test]
fn reads_the_tag_from_the_latest_redirect() {
    let url = "https://github.com/o/r/releases/latest";
    assert_eq!(
        read_probe(url, 302, Some("https://github.com/o/r/releases/tag/v1.0.8")),
        Probe::Tag("v1.0.8".into())
    );
    assert_eq!(
        read_probe(url, 302, Some("/o/r/releases/tag/v1.0.9-rc.1%2Bbuild?x=1")),
        Probe::Tag("v1.0.9-rc.1+build".into())
    );
    assert_eq!(
        read_probe(url, 302, Some("https://github.com/o/r/releases")),
        Probe::NoRelease
    );
    assert_eq!(read_probe(url, 302, None), Probe::NoRelease);
    assert!(matches!(
        read_probe(url, 200, None),
        Probe::Refused(Failure::Failed(_))
    ));
}

#[test]
fn checks_every_minute_and_backs_off_when_limited() {
    let mut t = Track::default();
    assert!(t.probe_due(0));
    t.on_probe(Probe::Tag("v1.0.8".into()), 1000);
    assert!(!t.probe_due(1059));
    assert!(t.probe_due(1060));
    t.on_probe(Probe::Refused(Failure::Failed("offline".into())), 1060);
    assert_eq!(
        t.tag.as_deref(),
        Some("v1.0.8"),
        "a failed check keeps what it knew"
    );
    assert_eq!(t.error.as_deref(), Some("offline"));
    assert!(t.probe_due(1120));
    t.on_probe(Probe::Refused(Failure::Wait(900, "limited".into())), 1120);
    assert!(!t.probe_due(1120 + 899));
    assert!(t.probe_due(1120 + 900));
    t.on_probe(Probe::Refused(Failure::Wait(5, "limited".into())), 3000);
    assert!(!t.probe_due(3030), "never sooner than the interval");
    t.on_probe(Probe::Tag("v1.0.9".into()), 4000);
    assert!(t.error.is_none());
    t.on_probe(Probe::NoRelease, 5000);
    assert!(t.tag.is_none() && t.release.is_none());
}

#[test]
fn asks_the_api_once_per_new_release() {
    let current = v("1.0.8");
    let mut t = Track::default();
    t.on_probe(Probe::Tag("v1.0.8".into()), 1000);
    assert!(!t.fetch_due(Some(&current), 1000), "up to date costs no API call");
    t.on_probe(Probe::Tag("v1.0.9".into()), 1060);
    assert!(t.fetch_due(Some(&current), 1060));
    t.on_fetch(Part::Launcher, Ok(launcher_release("v1.0.9")), 1060);
    assert_eq!(t.offer(Some(&current)).unwrap().tag_name, "v1.0.9");
    assert!(t.offer(Some(&v("1.0.9"))).is_none());
    t.on_probe(Probe::Tag("v1.0.9".into()), 1120);
    assert!(
        !t.fetch_due(Some(&current), 1120),
        "the known release is not fetched again"
    );
    t.on_probe(Probe::Tag("v1.1.0".into()), 2000);
    assert!(
        t.offer(Some(&current)).is_none(),
        "a stale release is not offered"
    );
    assert!(t.fetch_due(Some(&current), 2000));
}

#[test]
fn waits_for_an_incomplete_or_limited_release() {
    let current = v("1.0.8");
    let mut t = Track::default();
    t.on_probe(Probe::Tag("v1.0.9".into()), 1000);
    let bare = Release {
        assets: vec![],
        ..launcher_release("v1.0.9")
    };
    t.on_fetch(Part::Launcher, Ok(bare), 1000);
    assert!(t.offer(Some(&current)).is_none());
    assert!(!t.fetch_due(Some(&current), 1000 + RELEASE_RECHECK.as_secs() - 1));
    assert!(t.fetch_due(Some(&current), 1000 + RELEASE_RECHECK.as_secs()));
    t.on_fetch(Part::Launcher, Ok(launcher_release("v1.0.8")), 1300);
    assert!(
        t.offer(Some(&current)).is_none(),
        "an older API answer is ignored"
    );
    t.on_fetch(Part::Launcher, Err(Failure::Wait(3000, "limited".into())), 1600);
    assert!(!t.fetch_due(Some(&current), 4599));
    assert!(t.fetch_due(Some(&current), 4600));
    t.on_fetch(Part::Launcher, Ok(launcher_release("v1.0.9")), 4600);
    assert!(t.offer(Some(&current)).is_some());
}

#[test]
fn a_missing_addon_is_always_offered() {
    let mut t = Track::default();
    t.on_probe(Probe::Tag("v0.5.2".into()), 10);
    assert!(t.fetch_due(None, 10));
    t.on_fetch(Part::Addon, Ok(addon_release("v0.5.2")), 10);
    assert!(t.offer(None).is_some());
    assert!(t.offer(Some(&v("0.5.2"))).is_none());
    let mut l = Track::default();
    l.on_probe(Probe::Tag("v0.5.2".into()), 10);
    l.on_fetch(Part::Launcher, Ok(addon_release("v0.5.2")), 10);
    assert!(l.release.is_none(), "an addon release is not a launcher release");
}

#[test]
fn update_takes_the_play_button_only_when_safe() {
    assert_eq!(gate(&Phase::Idle, false, false), Gate::Play);
    assert_eq!(gate(&Phase::Idle, true, false), Gate::Update);
    assert_eq!(
        gate(&Phase::Idle, true, true),
        Gate::Play,
        "the game keeps its button"
    );
    assert_eq!(gate(&Phase::Installing, true, false), Gate::Installing);
    assert_eq!(gate(&Phase::Restarting, true, true), Gate::Restarting);
    let failed = Phase::Failed("no network".into());
    assert_eq!(gate(&failed, true, false), Gate::Retry);
    assert_eq!(gate(&failed, true, true), Gate::Play);
    assert_eq!(gate(&failed, false, false), Gate::Play);
}

fn catalog(releases: &[(&str, Option<&str>)]) -> Catalog {
    let mut c = Catalog::default();
    c.on_list(
        Ok(releases.iter().map(|(tag, _)| addon_release(tag)).collect()),
        0,
    );
    for (tag, needs) in releases {
        let found = c
            .releases
            .iter_mut()
            .find(|r| r.release.tag_name == *tag)
            .unwrap();
        found.needs = needs.map(|n| n.parse().unwrap());
    }
    c
}

fn tags(plan: &Plan) -> Vec<(Part, &str)> {
    plan.steps()
        .into_iter()
        .map(|(part, r)| (part, r.tag_name.as_str()))
        .collect()
}

#[test]
fn lists_ready_importers_newest_first() {
    let mut c = Catalog::default();
    let draft = Release {
        draft: true,
        ..addon_release("v0.9.0")
    };
    let pre = Release {
        prerelease: true,
        ..addon_release("v0.8.1")
    };
    let bare = Release {
        assets: vec![],
        ..addon_release("v0.4.0")
    };
    let list = vec![
        addon_release("v0.6.0"),
        draft,
        pre,
        addon_release("v0.8.0"),
        bare,
        addon_release("latest"),
        addon_release("v0.7.0"),
    ];
    c.on_list(Ok(list.clone()), 1000);
    let listed: Vec<&str> = c.releases.iter().map(|r| r.release.tag_name.as_str()).collect();
    assert_eq!(listed, ["v0.8.0", "v0.7.0", "v0.6.0"]);
    assert_eq!(c.list_at, 1000 + LIST_RECHECK.as_secs());
    c.releases[1].needs = Some(">=1.3.0".parse().unwrap());
    c.on_list(Ok(list), 2000);
    assert!(
        c.releases[1].needs.is_some(),
        "a known requirement survives a new list"
    );
    let replaced = Release {
        assets: vec![
            Asset {
                size: 11,
                ..asset(managed::ASSET)
            },
            asset(SUMS_FILE),
        ],
        ..addon_release("v0.7.0")
    };
    c.on_list(Ok(vec![replaced]), 3000);
    assert!(c.releases[0].needs.is_none(), "a replaced package is read again");
    c.on_list(
        Ok(vec![
            addon_release("v0.7.0"),
            Release {
                assets: vec![],
                ..addon_release("v0.8.0")
            },
        ]),
        4000,
    );
    assert_eq!(
        c.list_at,
        4000 + RELEASE_RECHECK.as_secs(),
        "a release still uploading is looked at again sooner"
    );
}

#[test]
fn lists_releases_on_a_new_latest_tag_or_every_quarter_hour() {
    let mut c = Catalog::default();
    assert!(c.list_due(0));
    c.latest.on_probe(Probe::Tag("v0.6.0".into()), 1000);
    c.on_list(Ok(vec![addon_release("v0.6.0")]), 1000);
    assert!(!c.list_due(1060), "an unchanged latest tag costs no API call");
    assert!(c.list_due(1000 + LIST_RECHECK.as_secs()));
    c.latest.on_probe(Probe::Tag("v0.7.0".into()), 1120);
    assert!(c.list_due(1120));
    c.on_list(Err(Failure::Wait(3000, "limited".into())), 1120);
    assert!(!c.list_due(1120 + 2999));
    assert!(c.list_due(1120 + 3000));
    c.on_list(Err(Failure::Failed("offline".into())), 5000);
    assert!(!c.list_due(5060), "a failed list is not retried every minute");
    assert!(c.list_due(5000 + RELEASE_RECHECK.as_secs()));
    assert_eq!(c.error.as_deref(), Some("offline"));
}

#[test]
fn learns_requirements_newest_first_until_one_fits() {
    let mut c = catalog(&[("v0.8.0", None), ("v0.7.0", None), ("v0.6.0", None)]);
    let needs = |r: &Release| -> Result<VersionReq> {
        Ok(match r.tag_name.as_str() {
            "v0.8.0" => ">=1.4.0",
            "v0.7.0" => ">=1.3.0",
            _ => ">=1.0.4",
        }
        .parse()
        .unwrap())
    };
    let mut asked = Vec::new();
    c.learn(Some(&v("0.6.0")), &v("1.3.1"), 0, |r| {
        asked.push(r.tag_name.clone());
        needs(r)
    });
    assert_eq!(
        asked,
        ["v0.8.0", "v0.7.0"],
        "stops at the first that fits, never reads the installed one"
    );
    assert!(
        c.to_learn(Some(&v("0.6.0")), &v("1.3.1"), 0).is_empty(),
        "each release is read once"
    );
    let mut c = catalog(&[("v0.8.0", None), ("v0.7.0", None)]);
    c.learn(None, &v("1.3.1"), 100, |_| Err(Error::Other("offline".into())));
    assert!(c.error.as_deref().unwrap().contains("offline"));
    assert!(
        c.to_learn(None, &v("1.3.1"), 101).is_empty(),
        "a failed read waits"
    );
    assert_eq!(
        c.to_learn(None, &v("1.3.1"), 100 + RELEASE_RECHECK.as_secs())
            .len(),
        2
    );
}

#[test]
fn chooses_the_newest_importer_the_launcher_accepts() {
    let c = catalog(&[
        ("v0.8.0", Some(">=1.4.0")),
        ("v0.7.0", Some(">=1.3.0")),
        ("v0.6.0", Some(">=1.0.4")),
    ]);
    let installed = v("0.5.2");
    let tag = |launcher: &str| {
        c.choose(&v(launcher), Some(&installed))
            .map(|r| r.tag_name.clone())
    };
    assert_eq!(tag("1.2.6").as_deref(), Some("v0.6.0"));
    assert_eq!(tag("1.3.1").as_deref(), Some("v0.7.0"));
    assert_eq!(tag("1.4.0").as_deref(), Some("v0.8.0"));
    assert!(
        c.choose(&v("1.3.1"), Some(&v("0.7.0"))).is_none(),
        "never a downgrade or the same"
    );
    assert_eq!(
        c.waiting(&v("1.3.1"), Some(&installed)).unwrap().version,
        v("0.8.0")
    );
    assert!(c.waiting(&v("1.4.0"), Some(&installed)).is_none());
    let unknown = catalog(&[("v0.8.0", None), ("v0.7.0", Some(">=1.3.0"))]);
    assert_eq!(
        unknown.choose(&v("1.3.1"), Some(&installed)).unwrap().tag_name,
        "v0.7.0",
        "a release whose requirement isn't known yet is passed over"
    );
}

#[test]
fn the_incident_updates_the_launcher_and_the_importer_in_one_run() {
    let store = managed::testing::store("incident");
    managed::install_release(
        &store,
        &managed::testing::seed(&store, "0.6.0", ">=1.0.4"),
        &v("1.2.6"),
        &Log(Mutex::default()),
    )
    .unwrap();
    let mut c = Catalog::default();
    c.latest.on_probe(Probe::Tag("v0.7.0".into()), 0);
    let newest = managed::testing::seed(&store, "0.7.0", ">=1.3.0");
    c.on_list(Ok(vec![newest, addon_release("v0.6.0")]), 0);
    let (current, installed) = (v("1.2.6"), managed::installed(&store));
    assert_eq!(installed, Some(v("0.6.0")));
    c.learn(installed.as_ref(), &current, 0, |r| {
        managed::requirement(&store, r)
    });
    assert_eq!(c.releases[0].needs, Some(">=1.3.0".parse().unwrap()));

    let alone = plan(&c, installed.as_ref(), None, &current, None);
    assert!(
        alone.is_empty(),
        "1.2.6 alone is never offered an importer it can't load"
    );
    assert_eq!(gate(&Phase::Idle, !alone.is_empty(), false), Gate::Play);

    let launcher = launcher_release("v1.3.0");
    let both = plan(&c, installed.as_ref(), Some(&launcher), &current, None);
    assert_eq!(tags(&both), [(Part::Launcher, "v1.3.0"), (Part::Addon, "v0.7.0")]);
    assert_eq!(gate(&Phase::Idle, !both.is_empty(), false), Gate::Update);
    let log = Log(Mutex::default());
    let mut seen = Vec::new();
    let ran = run(&both, &current, &log, |part, release, next, progress| {
        seen.push((part, next.clone()));
        match part {
            Part::Launcher => Ok(release.version().unwrap()),
            Part::Addon => managed::install_release(&store, release, next, progress),
        }
    });
    assert_eq!(
        seen,
        [(Part::Launcher, v("1.2.6")), (Part::Addon, v("1.3.0"))],
        "the importer is checked against the launcher it updates to"
    );
    assert_eq!(ran.error(), None);
    assert_eq!(
        ran.done,
        [(Part::Launcher, v("1.3.0")), (Part::Addon, v("0.7.0"))]
    );
    assert!(ran.restart(), "one restart, into 1.3.0");
    assert_eq!(managed::installed(&store), Some(v("0.7.0")));
    let after = plan(&c, managed::installed(&store).as_ref(), None, &v("1.3.0"), None);
    assert!(after.is_empty(), "nothing is left after the restart");
    let log = log.0.into_inner().unwrap();
    assert!(
        log.iter().any(|(_, t)| t == "Updating Swofty Launcher (1 of 2)"),
        "{log:?}"
    );
    assert!(
        log.iter()
            .any(|(_, t)| t == "Updating Simplified for Swofty (2 of 2)"),
        "{log:?}"
    );
    assert_eq!(summary(&ran.done), "Launcher + Simplified");
    assert_eq!(
        describe(&ran.done),
        "Swofty Launcher 1.3.0 and Simplified for Swofty 0.7.0"
    );
    std::fs::remove_dir_all(&store.root).ok();
}

#[test]
fn an_old_launcher_update_is_followed_by_the_importer_after_the_restart() {
    let c = catalog(&[("v0.7.0", Some(">=1.3.0")), ("v0.6.0", Some(">=1.0.4"))]);
    let next = plan(&c, Some(&v("0.6.0")), None, &v("1.3.1"), None);
    assert_eq!(tags(&next), [(Part::Addon, "v0.7.0")]);
    assert!(
        follow_up(true, &next),
        "a launcher restarted by an update installs it on its own"
    );
    assert!(!follow_up(false, &next));
    let both = plan(
        &c,
        Some(&v("0.6.0")),
        Some(&launcher_release("v1.3.2")),
        &v("1.3.1"),
        None,
    );
    assert!(!follow_up(true, &both), "a pending launcher waits for a click");
}

#[test]
fn an_importer_for_a_launcher_not_out_yet_is_not_offered() {
    let c = catalog(&[("v0.8.0", Some(">=1.4.0")), ("v0.7.0", Some(">=1.3.0"))]);
    let installed = v("0.7.0");
    let p = plan(&c, Some(&installed), None, &v("1.3.1"), None);
    assert!(p.is_empty());
    assert_eq!(gate(&Phase::Idle, !p.is_empty(), false), Gate::Play);
    assert_eq!(
        c.waiting(&v("1.3.1"), Some(&installed)).unwrap().version,
        v("0.8.0")
    );
    let p = plan(
        &c,
        Some(&installed),
        Some(&launcher_release("v1.3.2")),
        &v("1.3.1"),
        None,
    );
    assert_eq!(
        tags(&p),
        [(Part::Launcher, "v1.3.2")],
        "the launcher still updates"
    );
    let ran = run(&p, &v("1.3.1"), &Log(Mutex::default()), |_, r, _, _| {
        Ok(r.version().unwrap())
    });
    assert_eq!(ran.done, [(Part::Launcher, v("1.3.2"))]);
    assert!(ran.restart());
}

#[test]
fn a_failed_importer_still_updates_the_launcher() {
    let c = catalog(&[("v0.7.0", Some(">=1.3.0"))]);
    let p = plan(
        &c,
        Some(&v("0.6.0")),
        Some(&launcher_release("v1.3.0")),
        &v("1.2.6"),
        None,
    );
    let mut seen = Vec::new();
    let ran = run(&p, &v("1.2.6"), &Log(Mutex::default()), |part, release, _, _| {
        seen.push(part);
        match part {
            Part::Launcher => Ok(release.version().unwrap()),
            Part::Addon => Err(Error::Other("SHA-256 mismatch".into())),
        }
    });
    assert_eq!(seen, [Part::Launcher, Part::Addon]);
    assert_eq!(ran.launcher(), Some(&v("1.3.0")));
    assert!(ran.restart(), "the new launcher still starts");
    assert_eq!(ran.failed_tag(Part::Addon), Some("v0.7.0"));
    assert!(
        ran.error()
            .unwrap()
            .contains("Simplified for Swofty v0.7.0: SHA-256 mismatch")
    );
    let held = plan(&c, Some(&v("0.6.0")), None, &v("1.3.0"), Some("v0.7.0"));
    assert!(held.is_empty(), "an importer that failed doesn't hold Play");
    assert_eq!(
        gate(&Phase::Failed("x".into()), !held.is_empty(), false),
        Gate::Play
    );
}

#[test]
fn a_failed_launcher_still_updates_a_compatible_importer() {
    let c = catalog(&[("v0.8.0", Some(">=1.3.1")), ("v0.7.0", Some(">=1.3.0"))]);
    let p = plan(
        &c,
        Some(&v("0.6.0")),
        Some(&launcher_release("v1.3.1")),
        &v("1.3.0"),
        None,
    );
    assert_eq!(tags(&p), [(Part::Launcher, "v1.3.1"), (Part::Addon, "v0.8.0")]);
    let mut seen = Vec::new();
    let ran = run(
        &p,
        &v("1.3.0"),
        &Log(Mutex::default()),
        |part, release, next, _| {
            seen.push((part, release.tag_name.clone(), next.clone()));
            match part {
                Part::Launcher => Err(Error::Other("disk full".into())),
                Part::Addon => Ok(release.version().unwrap()),
            }
        },
    );
    assert_eq!(
        seen,
        [
            (Part::Launcher, "v1.3.1".into(), v("1.3.0")),
            (Part::Addon, "v0.7.0".into(), v("1.3.0"))
        ],
        "the importer falls back to the newest the running launcher accepts"
    );
    assert_eq!(ran.addon(), Some(&v("0.7.0")));
    assert!(!ran.restart(), "the failure stays on screen");
    assert!(ran.error().unwrap().contains("Swofty Launcher v1.3.1: disk full"));
    let only = catalog(&[("v0.8.0", Some(">=1.3.1"))]);
    let p = plan(
        &only,
        Some(&v("0.7.0")),
        Some(&launcher_release("v1.3.1")),
        &v("1.3.0"),
        None,
    );
    let ran = run(
        &p,
        &v("1.3.0"),
        &Log(Mutex::default()),
        |part, release, _, _| match part {
            Part::Launcher => Err(Error::Other("disk full".into())),
            Part::Addon => Ok(release.version().unwrap()),
        },
    );
    assert!(
        ran.done.is_empty(),
        "an importer that needs the failed launcher waits"
    );
    assert_eq!(ran.failed.len(), 1);
    let addon_only = run(
        &plan(&only, Some(&v("0.7.0")), None, &v("1.3.1"), None),
        &v("1.3.1"),
        &Log(Mutex::default()),
        |_, r, _, _| Ok(r.version().unwrap()),
    );
    assert!(addon_only.restart(), "an importer-only update restarts too");
}

#[test]
fn the_log_says_whether_the_running_copy_can_update_itself() {
    let exe = Ok(std::path::PathBuf::from(r"C:\Swofty\bin\swofty-launcher.exe"));
    let ok = Install {
        root: r"C:\Swofty".into(),
    };
    assert_eq!(
        copy_line(&exe, &Ok(ok)),
        r"update: C:\Swofty\bin\swofty-launcher.exe can update itself"
    );
    assert_eq!(
        copy_line(&exe, &Err(r"no write access to C:\Swofty\bin".into())),
        r"update: C:\Swofty\bin\swofty-launcher.exe can't update itself: no write access to C:\Swofty\bin"
    );
    let unread = Err(std::io::Error::other("gone"));
    assert_eq!(
        copy_line(
            &unread,
            &Err("this copy does not run from a release folder".into())
        ),
        "update: the launcher (its path does not read: gone) can't update itself: this copy does not run from \
         a release folder"
    );
}

#[test]
fn the_log_names_a_blocked_update_what_was_installed_and_the_restart() {
    assert_eq!(
        blocked_line(&v("1.9.81"), "no write access to bin"),
        "update: Swofty Launcher 1.9.81 is available, and this copy can't install it: no write access to bin"
    );
    assert_eq!(
        installed_line(&[(Part::Launcher, v("1.9.81")), (Part::Addon, v("0.6.2"))]),
        format!(
            "update: installed Swofty Launcher 1.9.81, {} 0.6.2; restarting",
            Part::Addon.name()
        )
    );
    let exe = std::path::Path::new(r"C:\Swofty\bin\swofty-launcher.exe");
    assert_eq!(
        restart_line(exe, Ok(4242)),
        r"update: started C:\Swofty\bin\swofty-launcher.exe (pid 4242) to restart"
    );
    let missing = std::io::Error::from_raw_os_error(2);
    assert_eq!(
        restart_line(exe, Err(&missing)),
        format!(r"update: could not restart C:\Swofty\bin\swofty-launcher.exe: {missing}")
    );
    let blocked = std::io::Error::from_raw_os_error(4551);
    let line = restart_line(exe, Err(&blocked));
    match crate::platform::app_control::Block::of(exe, &blocked) {
        Some(b) => assert!(line.ends_with(&format!("; {}", b.headline())), "{line}"),
        None => assert_eq!(
            line,
            format!(r"update: could not restart C:\Swofty\bin\swofty-launcher.exe: {blocked}")
        ),
    }
}

#[test]
fn notices_an_update_that_did_not_take() {
    assert!(did_not_take(None, &v("1.0.8")).is_none());
    assert!(did_not_take(Some("v1.0.8"), &v("1.0.8")).is_none());
    assert!(did_not_take(Some("garbage"), &v("1.0.8")).is_none());
    let text = did_not_take(Some("v1.0.9"), &v("1.0.8")).unwrap();
    assert!(text.contains("1.0.9") && text.contains("1.0.8"), "{text}");
}
