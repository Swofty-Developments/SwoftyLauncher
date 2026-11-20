use std::path::{Path, PathBuf};

use super::*;
use crate::saves::vault::restore::Running;

const DATA: &str = "savegame@profile#NXDSMWW.BWW";
const INDEX: &str = "savegame@profile#QKRHMYXE";
const T0: i64 = 1_793_404_800;

/// The perf timeline of a session that read the profile and sat on the start screen until Stop (Jonnie,
/// launcher 1.8.4, 4 Nov 2026: 162 s on splash_transition).
const STALLED_PERF: &str = "\
== Summary
uptime: 212 s

== Timeline (last 15; each row runs until the next one)
at          took ms   frames   slow   worst ms  swofty ms  what
00:00:00      13963        0      0          0      18.19  start  [core assets manifest parse 14.82 ms]
00:00:13        283        0      0          0       6.59  flow #e11edb87 Entering
00:00:23        745       40      0         36      49.85  screen video_1
00:00:24      11384      340      2        140     168.35  screen seizure_warning
00:00:36       8409      451      3        636     106.54  screen splash
00:00:44       3566      356      0         23       9.44  screen render_badge
00:00:48       1716      171      0         13      44.24  flow #a4d3a4b2 Exiting; flow #fd9f4f95 Entering; flow #fd9f4f95 Exiting; flow #04fc8ffa Entering
00:00:50     104956     6070     23       2251     278.68  screen splash_transition  [core flow tick (events, career poll) 250.51 ms]
00:02:35      56379     2930      3       6933          -  flow #04fc8ffa Exiting (current)
";

/// A session that went on to the menus.
const PASSED_PERF: &str = "\
== Summary
uptime: 900 s

== Timeline (last 15; each row runs until the next one)
at          took ms   frames   slow   worst ms  swofty ms  what
00:00:36       8409      451      3        636     106.54  screen splash
00:00:50      14956     6070     23       2251     278.68  screen splash_transition
00:01:05       2585      469      7       1420          -  screen fe_home
00:02:05       2585      469      7       1420          -  screen pause (current)
";

const LOADER: &str = "\
2026-11-04 03:32:34.875 [ 5256] [saves] profile load 1: read from launcher saves (profile colorgrading-testing, C:\\Users\\x\\AppData\\Local\\Swofty\\saves\\colorgrading-testing\\remote); save file savegame@profile#QKRHMYXE: 416 bytes, modified 2026-10-31 03:36:07 UTC
2026-11-04 03:32:34.875 [ 5256] [saves] profile load 1: manager 5 slot 2: gm_my_team id=16 key=66f051ee751e04d1 career=my_team:66f051ee751e04d1 flags=0x20 setup=13 season=1 weekend=0 date=1793403769 name=\"My Team Save 3\"
2026-11-04 03:32:43.658 [24120] [game] ACTOR_WARN (warn): MISSING RESOURCE [.emb?context=noyear-custom-myteam] using fallback [context=default]
";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-save-stall-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn store(root: &Path, profiles: &[&str]) -> Store {
    let data = root.join("data");
    std::fs::create_dir_all(data.join("profiles")).unwrap();
    for p in profiles {
        std::fs::write(data.join(format!("profiles/{p}.json")), "{}").unwrap();
    }
    Store::new(data)
}

fn put(loc: &Location, bytes: &[u8]) {
    std::fs::create_dir_all(&loc.dir).unwrap();
    std::fs::write(loc.dir.join(DATA), bytes).unwrap();
    std::fs::write(loc.dir.join(INDEX), [7u8; 416]).unwrap();
}

fn steam(root: &Path, bytes: &[u8]) -> Location {
    let remote = root.join("steam/userdata/221994709/3059520/remote");
    std::fs::create_dir_all(&remote).unwrap();
    let loc = Location::steam(&remote).unwrap();
    put(&loc, bytes);
    loc
}

/// A launcher session on `loc`: the vault's before and after snapshots, and the session folder with its
/// loader log and perf timeline.
fn session(store: &Store, loc: &Location, stamp: &str, perf: &str, at: i64) {
    let id = format!("req-{stamp}");
    let v = Vault::of(&store.root, loc).unwrap();
    let mut meta = Meta::new(Reason::BeforeSession).profile("colorgrading-testing");
    meta.session_id = Some(id.clone());
    v.before_session(loc, meta.clone(), at).unwrap();
    meta.reason = Reason::AfterSession;
    v.after_session(loc, meta, at + 200).unwrap();
    let dir = store.sessions_dir().join(stamp);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("request_id"), format!("{id}\n")).unwrap();
    std::fs::write(dir.join("loader.log"), LOADER).unwrap();
    std::fs::write(dir.join("perf.log"), perf).unwrap();
}

fn data_of(loc: &Location) -> Vec<u8> {
    std::fs::read(loc.dir.join(DATA)).unwrap()
}

fn sha(bytes: &[u8]) -> String {
    let p = tmp("sha").join("f");
    std::fs::write(&p, bytes).unwrap();
    sha256_file(&p).unwrap()
}

fn seen(session: &str, before: &str, outcome: Outcome) -> Seen {
    Seen {
        session: session.into(),
        before: before.into(),
        size: 932_920,
        after: Some(before.into()),
        profile: Some("colorgrading-testing".into()),
        outcome,
    }
}

#[test]
fn a_session_that_read_the_profile_and_sat_on_the_start_screen_stalled() {
    let root = tmp("outcome");
    let s = store(&root, &[]);
    let dir = s.sessions_dir().join("20261104-023146");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("loader.log"), LOADER).unwrap();
    std::fs::write(dir.join("perf.log"), STALLED_PERF).unwrap();
    assert_eq!(outcome(&dir, &s), Outcome::Stalled(162.0));
    std::fs::write(dir.join("perf.log"), PASSED_PERF).unwrap();
    assert_eq!(outcome(&dir, &s), Outcome::Passed);
    let badge = STALLED_PERF.replace("screen splash_transition", "flow #1");
    std::fs::write(dir.join("perf.log"), badge).unwrap();
    assert_eq!(
        outcome(&dir, &s),
        Outcome::Unknown,
        "ended on render_badge, before the main menu"
    );
    let slow = STALLED_PERF.replace("uptime: 212 s", "uptime: 150 s");
    std::fs::write(dir.join("perf.log"), slow).unwrap();
    assert_eq!(
        outcome(&dir, &s),
        Outcome::Unknown,
        "100 s can still be a slow menu load"
    );
    std::fs::write(dir.join("perf.log"), STALLED_PERF).unwrap();
    std::fs::write(dir.join("loader.log"), "no profile load here\n").unwrap();
    assert_eq!(
        outcome(&dir, &s),
        Outcome::Unknown,
        "a stall before any profile load is not the save's"
    );
}

#[test]
fn a_stall_shows_when_the_game_folder_refused_perf_log() {
    let root = tmp("perf-fallback");
    let s = store(&root, &[]);
    let game = root.join("game");
    std::fs::create_dir_all(game.join("swofty")).unwrap();
    let dir = s.sessions_dir().join("20261119-184700");
    std::fs::create_dir_all(&dir).unwrap();
    let launched = std::time::SystemTime::now() - std::time::Duration::from_secs(5);
    std::fs::create_dir_all(s.root.join("state")).unwrap();
    std::fs::write(s.root.join(crate::session::LOADER_FALLBACK), LOADER).unwrap();
    std::fs::write(s.root.join(crate::session::PERF_FALLBACK), STALLED_PERF).unwrap();
    assert_eq!(
        outcome(&dir, &s),
        Outcome::Unknown,
        "nothing kept from the game yet"
    );
    crate::session::keep_game_logs(&s.root, &game, &dir, None, launched);
    assert!(!game.join(crate::session::PERF_LOG).exists());
    assert_eq!(outcome(&dir, &s), Outcome::Stalled(162.0));
}

#[test]
fn two_stalls_on_the_same_save_learn_it() {
    let mut ledger = Ledger::default();
    let one = [seen("a", "bad", Outcome::Stalled(162.0))];
    assert!(
        learn(&mut ledger, &one, T0).is_empty(),
        "one short stall can be a slow load"
    );
    let two = [
        seen("a", "bad", Outcome::Stalled(162.0)),
        seen("b", "bad", Outcome::Stalled(130.0)),
        seen("c", "good", Outcome::Passed),
    ];
    let lines = learn(&mut ledger, &two, T0);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains("2 session(s) (a, b), up to 162 s"),
        "{}",
        lines[0]
    );
    assert_eq!(ledger.saves.len(), 1);
    assert_eq!(ledger.saves[0].sha256, "bad");
    assert_eq!(ledger.saves[0].secs, 162);
    assert!(learn(&mut ledger, &two, T0).is_empty(), "learned once");
    let long = [seen("d", "worse", Outcome::Stalled(LONG_SECS))];
    assert_eq!(learn(&mut ledger, &long, T0).len(), 1, "one long stall is enough");
}

#[test]
fn a_save_that_ever_loaded_is_never_learned_and_is_forgotten() {
    let mut ledger = Ledger::default();
    let mut wrote = seen("c", "bad", Outcome::Unknown);
    wrote.after = Some("newer".into());
    let s = [
        seen("a", "bad", Outcome::Stalled(400.0)),
        seen("b", "bad", Outcome::Stalled(400.0)),
        wrote,
    ];
    assert!(
        learn(&mut ledger, &s, T0).is_empty(),
        "the game wrote it, so it got past the load"
    );
    assert!(ledger.saves.is_empty());
    learn(&mut ledger, &s[..2], T0);
    assert_eq!(ledger.saves.len(), 1);
    learn(&mut ledger, &[seen("e", "bad", Outcome::Passed)], T0);
    assert!(
        ledger.saves.is_empty(),
        "a later session got past the start screen with it"
    );
}

#[test]
fn the_sweep_pins_a_stalled_store_save_and_offers_the_last_good_one_without_touching_it() {
    let root = tmp("sweep");
    let s = store(&root, &["colorgrading-testing"]);
    let loc = crate::saves::launcher::location(&s.root, "colorgrading-testing");
    let good = [3u8; 715_128 / 100];
    let bad = [9u8; 932_920 / 100];
    put(&loc, &good);
    session(&s, &loc, "20261101-120000", PASSED_PERF, T0);
    put(&loc, &bad);
    session(&s, &loc, "20261104-023037", STALLED_PERF, T0 + 3 * 86_400);
    session(&s, &loc, "20261104-023146", STALLED_PERF, T0 + 3 * 86_400 + 300);

    let lines = sweep(&s, None, Running::default(), T0 + 4 * 86_400);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains("in 2 session(s)"), "{}", lines[0]);
    assert!(
        lines[1].contains("kept F1 25 on Loading... after Press any button"),
        "{}",
        lines[1]
    );
    assert!(lines[1].contains("the save is left as it is"), "{}", lines[1]);
    assert_eq!(data_of(&loc), bad, "the sweep never swaps the live save");

    let ledger = Ledger::read(&s.root);
    assert_eq!(ledger.saves[0].sha256, sha(&bad));
    assert_eq!(ledger.moved.len(), 1);
    let kept = ledger.moved[0].kept.clone().unwrap();
    let v = Vault::of(&s.root, &loc).unwrap();
    let snap = v.find(&kept).unwrap();
    assert!(snap.pinned.iter().any(|p| p == PIN), "{:?}", snap.pinned);
    assert_eq!(
        data_entry(&snap).unwrap().sha256,
        sha(&bad),
        "the vault keeps the stalled save"
    );

    let found = super::offers(&s.root);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].source, loc.source);
    assert_eq!(found[0].kept.as_deref(), Some(kept.as_str()));
    let good_snap = v.find(found[0].snapshot.as_ref().unwrap()).unwrap();
    assert_eq!(data_entry(&good_snap).unwrap().sha256, sha(&good));

    assert!(
        sweep(&s, None, Running::default(), T0 + 5 * 86_400).is_empty(),
        "nothing more to do"
    );
    assert_eq!(data_of(&loc), bad);

    crate::saves::vault::restore::restore(&v, &good_snap.id, &loc, Running::default(), T0 + 6 * 86_400)
        .unwrap();
    assert_eq!(data_of(&loc), good, "the player's restore puts the version back");
    assert!(
        super::offers(&s.root).is_empty(),
        "a restored save has no offer left"
    );
}

#[test]
fn the_same_bytes_in_the_steam_save_are_flagged_too_and_stay_while_steam_runs() {
    let root = tmp("steam");
    let s = store(&root, &["colorgrading-testing"]);
    let good = [3u8; 7_151];
    let bad = [9u8; 9_329];
    let steam = steam(&root, &good);
    Vault::of(&s.root, &steam)
        .unwrap()
        .snapshot(&steam, Meta::new(Reason::LauncherStart), T0)
        .unwrap();
    put(&steam, &bad);
    let loc = crate::saves::launcher::location(&s.root, "colorgrading-testing");
    put(&loc, &bad);
    session(&s, &loc, "20261104-023037", STALLED_PERF, T0 + 100);
    session(&s, &loc, "20261104-023146", STALLED_PERF, T0 + 400);

    let running = Running {
        steam: true,
        ..Running::default()
    };
    let lines = sweep(&s, Some(&steam), running, T0 + 1_000);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(data_of(&steam), bad);
    assert_eq!(data_of(&loc), bad);
    assert!(
        lines
            .iter()
            .all(|l| !l.contains("moved aside") && !l.contains("is back in its place"))
    );
    assert_eq!(Ledger::read(&s.root).moved.len(), 2);
}

#[test]
fn with_no_other_version_nothing_is_offered_and_a_dismissal_holds() {
    let root = tmp("empty");
    let s = store(&root, &["colorgrading-testing"]);
    let bad = [9u8; 9_329];
    let loc = crate::saves::launcher::location(&s.root, "colorgrading-testing");
    put(&loc, &bad);
    let steam = steam(&root, &bad);
    session(&s, &loc, "20261104-023037", STALLED_PERF, T0 + 100);
    session(&s, &loc, "20261104-023146", STALLED_PERF, T0 + 400);

    let lines = sweep(&s, Some(&steam), Running::default(), T0 + 1_000);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("holds no earlier version to offer")),
        "{lines:?}"
    );
    assert!(
        crate::saves::launcher::has_files(&loc),
        "the store is never emptied"
    );
    assert_eq!(data_of(&steam), bad);

    let found = offers(&s.root);
    assert!(found.iter().all(|o| o.snapshot.is_none()));
    let first = &found[0];
    dismiss(&s.root, &first.source, &first.sha256).unwrap();
    assert!(offers(&s.root).iter().all(|o| o.source != first.source));
    let kept = Ledger::read(&s.root).moved[0].kept.clone().unwrap();
    let snap = Vault::of(&s.root, &loc).unwrap().find(&kept).unwrap();
    assert!(snap.pinned.iter().any(|p| p == PIN));
}

#[test]
fn the_launcher_never_copies_a_stalled_save() {
    let root = tmp("copy");
    let s = store(&root, &["colorgrading-testing", "default"]);
    let bad = [9u8; 9_329];
    let steam = steam(&root, &bad);
    let mut ledger = Ledger::default();
    learn(
        &mut ledger,
        &[seen("a", &sha(&bad), Outcome::Stalled(LONG_SECS))],
        T0,
    );
    ledger.saves[0].size = bad.len() as u64;
    ledger.write(&s.root).unwrap();

    let e = crate::saves::launcher::seed(&s.root, "default", &steam, Reason::Imported, T0).unwrap_err();
    assert!(e.to_string().contains("does not copy it"), "{e}");
    assert!(!crate::saves::launcher::has_files(
        &crate::saves::launcher::location(&s.root, "default")
    ));

    let e = crate::saves::launcher::copy_to(&s.root, &steam, "default", Running::default(), T0).unwrap_err();
    assert!(e.to_string().contains("does not copy it"), "{e}");

    let from = crate::saves::launcher::location(&s.root, "colorgrading-testing");
    put(&from, &bad);
    let e = crate::saves::launcher::copy_to(&s.root, &from, "default", Running::default(), T0).unwrap_err();
    assert!(e.to_string().contains("does not copy it"), "{e}");
    let e = crate::saves::launcher::export(&s.root, "colorgrading-testing", &steam, Running::default(), T0)
        .unwrap_err();
    assert!(e.to_string().contains("does not copy it"), "{e}");

    put(&steam, &[3u8; 7_151]);
    assert!(crate::saves::launcher::seed(&s.root, "default", &steam, Reason::Imported, T0).is_ok());
}

#[test]
#[ignore]
fn reads_real_bundles() {
    let Some(roots) = std::env::var_os("SWOFTY_STALL_CASES") else {
        return;
    };
    let s = store(&tmp("bundles"), &[]);
    let (mut stalled, mut passed, mut unknown) = (0, 0, 0);
    for root in std::env::split_paths(&roots) {
        for case in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let sessions = case.path().join("logs/sessions");
            for dir in std::fs::read_dir(&sessions).into_iter().flatten().flatten() {
                match outcome(&dir.path(), &s) {
                    Outcome::Stalled(secs) => {
                        stalled += 1;
                        println!("STALL {} {secs}", dir.path().display());
                    }
                    Outcome::Passed => passed += 1,
                    Outcome::Unknown => unknown += 1,
                }
            }
        }
    }
    println!("stalled {stalled} passed {passed} unknown {unknown}");
}
