use std::path::{Path, PathBuf};

use super::*;
use crate::saves::vault::{Meta, Reason};

const DATA: &str = "savegame@profile#NXDSMWW.BWW";
const INDEX: &str = "savegame@profile#QKRHMYXE";
const T0: i64 = 1_795_100_000;
const PROFILE: &str = "kok";

/// Every Team HQ entry of a session on a save written inside the round 12 weekend (szariq_, launcher 1.10.4, F2
/// Revamp driver career, 20 Nov 2026): Team HQ twice, then quit from Team HQ.
const STUCK: &str = "\
2026-11-20 12:20:32.482 [11860] [career] career.loaded: mode driver_career, key 'dc-00c90000' (components live, component mode driver_career, Team HQ answer none)
2026-11-20 12:20:36.785 [23412] [career] Team HQ entered: the game reports not My Team (Career_Is_MyTeamGameMode)
2026-11-20 12:20:42.344 [24496] [career] career.hub entered (mode driver_career)
2026-11-20 12:20:42.348 [24496] [career] weekend already started, session never ran: round 12/22, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/hungaroring
2026-11-20 12:20:42.361 [24496] [career] career.hub left (mode driver_career)
2026-11-20 12:20:49.847 [14628] [career] career.hub entered (mode driver_career)
2026-11-20 12:20:49.847 [14628] [career] weekend already started, session never ran: round 12/22, weekend in progress, current session 1 of 7 (practice_1), 0 of 7 sessions complete, track game/hungaroring
2026-11-20 12:20:49.857 [14628] [hub] silly_season: Team HQ tab 'Silly Season' added after STANDINGS (key 0x5221fb03, page swofty_hub_silly_season, data view_model_data.hud_swofty_hub_silly_season)
";

/// A Progressive career on the same kind of save (launcher 1.9.8x, 19 Nov 2026), with no track name read.
const STUCK_NO_TRACK: &str = "\
2026-11-19 22:03:45.970 [20652] [career] career.hub entered (mode driver_career)
2026-11-19 22:03:45.970 [20652] [career] weekend already started, session never ran: round 12/21, weekend in progress, current session 1 of 9 (practice_1), 0 of 9 sessions complete, track unknown
";

/// The same career after a restore of the version from before round 12: Team HQ, Advance, then practice 1 loads.
const ADVANCED: &str = "\
2026-11-19 22:11:34.747 [18752] [career] Team HQ weekend: round 11/21, weekend complete, current session 7 of 7 (race), 7 of 7 sessions complete, track unknown
2026-11-19 22:12:06.486 [ 9704] [career] flow \"StateTeamHQTimeAdvance\" None -> Entering
2026-11-19 22:12:06.486 [ 9704] [career] flow \"StateTeamHQTimeAdvance\" Exiting -> None
2026-11-19 22:12:09.116 [33616] [career] flow \"StateLoadSession\" None -> Entering
2026-11-19 22:12:09.264 [33616] [career] flow \"StateLoadSession\" Entering -> Updating
";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-save-stuck-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn store(root: &Path) -> Store {
    let data = root.join("data");
    std::fs::create_dir_all(data.join("profiles")).unwrap();
    std::fs::write(data.join(format!("profiles/{PROFILE}.json")), "{}").unwrap();
    Store::new(data)
}

fn put(loc: &Location, bytes: &[u8]) {
    std::fs::create_dir_all(&loc.dir).unwrap();
    std::fs::write(loc.dir.join(DATA), bytes).unwrap();
    std::fs::write(loc.dir.join(INDEX), [7u8; 416]).unwrap();
}

fn sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    crate::game_folder::baseline::hex(&Sha256::digest(bytes))
}

/// A launcher session on `loc` that starts on the save there, leaves `wrote` in its place when given, and logs
/// `loader`. With `ended` false the vault has its start snapshot only.
fn session(
    store: &Store,
    loc: &Location,
    stamp: &str,
    loader: &str,
    wrote: Option<&[u8]>,
    at: i64,
    ended: bool,
) {
    let id = format!("req-{stamp}");
    let v = Vault::of(&store.root, loc).unwrap();
    let mut meta = Meta::new(Reason::BeforeSession).profile(PROFILE);
    meta.session_id = Some(id.clone());
    v.before_session(loc, meta.clone(), at).unwrap();
    if let Some(bytes) = wrote {
        put(loc, bytes);
    }
    if ended {
        meta.reason = Reason::AfterSession;
        v.after_session(loc, meta, at + 600).unwrap();
    }
    let dir = store.sessions_dir().join(stamp);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("request_id"), format!("{id}\n")).unwrap();
    std::fs::write(dir.join("loader.log"), loader).unwrap();
}

fn end(store: &Store, loc: &Location, stamp: &str, at: i64) {
    let v = Vault::of(&store.root, loc).unwrap();
    let mut meta = Meta::new(Reason::AfterSession).profile(PROFILE);
    meta.session_id = Some(format!("req-{stamp}"));
    v.after_session(loc, meta, at).unwrap();
}

#[test]
fn a_weekend_line_names_the_round_the_session_and_the_track() {
    let w = stuck_weekend(STUCK).expect("the session ended in the started weekend");
    assert_eq!(
        w,
        Weekend {
            round: 12,
            rounds: 22,
            session: Some("practice_1".into()),
            track: Some("game/hungaroring".into()),
        }
    );
    assert_eq!(w.text(), "round 12 of 22 (hungaroring)");
    let w = stuck_weekend(STUCK_NO_TRACK).unwrap();
    assert_eq!((w.round, w.rounds, w.track.as_deref()), (12, 21, None));
    assert_eq!(w.text(), "round 12 of 21");
}

#[test]
fn a_session_that_got_going_after_the_line_is_not_stuck() {
    assert_eq!(stuck_weekend(ADVANCED), None, "a healthy advance");
    let loaded = format!(
        "{STUCK_NO_TRACK}2026-11-19 22:12:09.116 [33616] [career] flow \"StateLoadSession\" None -> Entering\n"
    );
    assert_eq!(stuck_weekend(&loaded), None, "practice 1 loaded after it");
    let later = format!("{STUCK}{ADVANCED}");
    assert_eq!(
        stuck_weekend(&later),
        None,
        "a later Team HQ found the weekend complete"
    );
    let again = format!("{ADVANCED}{STUCK}");
    assert!(stuck_weekend(&again).is_some(), "stuck at the end of the session");
    assert_eq!(stuck_weekend(""), None);
    let unreadable =
        "2026-11-20 12:20:42.348 [1] [career] Team HQ weekend: the current weekend is not readable\n";
    assert_eq!(stuck_weekend(&format!("{STUCK}{unreadable}")), None);
}

#[test]
fn the_folder_read_matches_the_text_read() {
    let root = tmp("folder");
    let dir = root.join("20261120-104632");
    std::fs::create_dir_all(dir.join("game-logs")).unwrap();
    assert_eq!(stuck_in(&dir), None);
    std::fs::write(dir.join("game-logs/loader.log"), STUCK).unwrap();
    assert_eq!(stuck_in(&dir), stuck_weekend(STUCK));
    std::fs::write(dir.join("loader.log"), ADVANCED).unwrap();
    assert_eq!(stuck_in(&dir), None, "the session's own loader.log comes first");
}

#[test]
fn the_sweep_learns_the_save_a_stuck_session_ended_on_and_offers_the_version_before_it() {
    let root = tmp("sweep");
    let s = store(&root);
    let loc = crate::saves::launcher::location(&s.root, PROFILE);
    let round_11 = [3u8; 2_595];
    let stuck = [9u8; 2_596];
    put(&loc, &round_11);
    session(&s, &loc, "20261119-180000", ADVANCED, None, T0, true);
    session(
        &s,
        &loc,
        "20261119-184500",
        ADVANCED,
        Some(&stuck),
        T0 + 3_000,
        true,
    );
    session(&s, &loc, "20261120-104632", STUCK, None, T0 + 60_000, true);

    let lines = sweep(&s, T0 + 61_000);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].contains(
            "session 20261120-104632 ended with its career in the round 12 of 22 (hungaroring) weekend"
        ),
        "{}",
        lines[0]
    );
    let ledger = Ledger::read(&s.root);
    assert_eq!(ledger.saves.len(), 1);
    assert_eq!(ledger.saves[0].sha256, sha(&stuck));
    assert_eq!(ledger.saves[0].source, loc.source);
    assert_eq!(ledger.saves[0].profile.as_deref(), Some(PROFILE));
    assert_eq!(ledger.saves[0].sessions, ["20261120-104632"]);
    assert_eq!(ledger.read.len(), 3);

    let offered = offers(&s.root);
    assert_eq!(offered.len(), 1, "{offered:?}");
    assert_eq!(offered[0].weekend.round, 12);
    let at = offered[0].snapshot_at;
    let v = Vault::of(&s.root, &loc).unwrap();
    let offered = v.find(offered[0].snapshot.as_ref().unwrap()).unwrap();
    assert_eq!(
        data_entry(&offered).unwrap().sha256,
        sha(&round_11),
        "the newest version that is not the stuck save"
    );
    assert_eq!(at, Some(offered.at));
    assert_eq!(
        std::fs::read(loc.dir.join(DATA)).unwrap(),
        stuck,
        "the sweep never touches the save"
    );

    assert!(sweep(&s, T0 + 62_000).is_empty(), "learned once");
    session(&s, &loc, "20261120-105635", STUCK, None, T0 + 63_000, true);
    assert!(sweep(&s, T0 + 64_000).is_empty());
    assert_eq!(
        Ledger::read(&s.root).saves[0].sessions,
        ["20261120-104632", "20261120-105635"]
    );

    put(&loc, &round_11);
    assert!(
        offers(&s.root).is_empty(),
        "the stuck save is no longer in the folder"
    );
    put(&loc, &stuck);
    assert_eq!(offers(&s.root).len(), 1);
    dismiss(&s.root, &loc.source, &sha(&stuck)).unwrap();
    assert!(offers(&s.root).is_empty(), "dismissed");
}

#[test]
fn a_session_is_read_once_its_end_snapshot_is_in_the_vault() {
    let root = tmp("unended");
    let s = store(&root);
    let loc = crate::saves::launcher::location(&s.root, PROFILE);
    let stuck = [9u8; 2_596];
    put(&loc, &stuck);
    session(&s, &loc, "20261120-104632", STUCK, None, T0, false);
    assert!(sweep(&s, T0 + 100).is_empty(), "no end snapshot yet");
    assert!(Ledger::read(&s.root).read.is_empty(), "left to read later");
    end(&s, &loc, "20261120-104632", T0 + 600);
    assert_eq!(sweep(&s, T0 + 700).len(), 1);
    let offered = offers(&s.root);
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].snapshot, None, "the vault holds no other version");
}

#[test]
fn a_healthy_session_learns_nothing_and_is_not_read_again() {
    let root = tmp("healthy");
    let s = store(&root);
    let loc = crate::saves::launcher::location(&s.root, PROFILE);
    put(&loc, &[3u8; 2_595]);
    session(
        &s,
        &loc,
        "20261119-221000",
        ADVANCED,
        Some(&[4u8; 2_600]),
        T0,
        true,
    );
    assert!(sweep(&s, T0 + 700).is_empty());
    let ledger = Ledger::read(&s.root);
    assert!(ledger.saves.is_empty());
    assert_eq!(ledger.read, ["20261119-221000"]);
    std::fs::write(s.sessions_dir().join("20261119-221000/loader.log"), STUCK).unwrap();
    assert!(sweep(&s, T0 + 800).is_empty(), "a session folder is read once");
}

#[test]
#[ignore]
fn reads_real_bundles() {
    let Some(roots) = std::env::var_os("SWOFTY_STUCK_CASES") else {
        return;
    };
    let (mut stuck, mut other) = (0, 0);
    for root in std::env::split_paths(&roots) {
        for case in std::fs::read_dir(&root).into_iter().flatten().flatten() {
            let sessions = case.path().join("logs/sessions");
            for dir in std::fs::read_dir(&sessions).into_iter().flatten().flatten() {
                match stuck_in(&dir.path()) {
                    Some(w) => {
                        stuck += 1;
                        println!("STUCK {} {}", dir.path().display(), w.text());
                    }
                    None => other += 1,
                }
            }
        }
    }
    println!("stuck {stuck} other {other}");
}
