use std::path::{Path, PathBuf};

use super::restore::{Running, restore};
use super::*;

const T0: i64 = 1_793_404_800;
const DATA: &str = "savegame@profile#NXDSMWW.BWW";
const INDEX: &str = "savegame@profile#QKRHMYXE";

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-vault-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A fake Steam account's save folder with the profile pair, a highlight
/// and Steam's remotecache.vdf.
fn steam(root: &Path, id3: &str, data: &[u8]) -> Location {
    let remote = root.join(format!("steam/userdata/{id3}/3059520/remote"));
    std::fs::create_dir_all(&remote).unwrap();
    std::fs::write(remote.join(DATA), data).unwrap();
    std::fs::write(remote.join(INDEX), [7u8; 416]).unwrap();
    std::fs::write(remote.join("savegame@highlights001#QKRHMYXE"), b"replay").unwrap();
    let (_, _, sha1) = hashes(&remote.join(DATA)).unwrap();
    std::fs::write(
        remote.parent().unwrap().join("remotecache.vdf"),
        format!(
            "\"3059520\"\n{{\n\t\"ChangeNumber\"\t\t\"510\"\n\t\"{DATA}\"\n\t{{\n\t\t\"root\"\t\t\"0\"\n\t\t\"size\"\t\t\"{}\"\n\
             \t\t\"sha\"\t\t\"{sha1}\"\n\t\t\"syncstate\"\t\t\"1\"\n\t}}\n}}\n",
            data.len()
        ),
    )
    .unwrap();
    Location::steam(&remote).unwrap()
}

fn vault(root: &Path, loc: &Location) -> Vault {
    Vault::of(&root.join("data"), loc).unwrap()
}

fn new(t: Taken) -> Snapshot {
    match t {
        Taken::New(s) => s,
        other => panic!("expected a new snapshot, got {other:?}"),
    }
}

fn kinds(v: &Vault) -> Vec<EventKind> {
    v.events().into_iter().map(|e| e.kind).collect()
}

#[test]
fn dedupes_by_content() {
    let root = tmp("dedupe");
    let loc = steam(&root, "843502855", &[1u8; 900]);
    let v = vault(&root, &loc);
    assert_eq!(v.dir, root.join("data/career-vault/steam-843502855"));
    let a = new(v.snapshot(&loc, Meta::new(Reason::LauncherStart), T0).unwrap());
    assert_eq!(a.id, "20261031-000000-launcher-start");
    assert_eq!(a.files.len(), 2, "highlights stay out");
    assert_eq!(a.files[0].name, "profile/NXDSMWW.BWW");
    assert_eq!(a.files[0].file, DATA);
    assert_eq!(a.files[0].remotecache.as_ref().unwrap().syncstate, "1");
    assert_eq!(
        a.cloud(),
        cloud::Status::Unknown,
        "the index has no remotecache entry"
    );
    assert_eq!(a.total(), 900 + 416);
    let again = v
        .snapshot(&loc, Meta::new(Reason::BeforeSession), T0 + 60)
        .unwrap();
    assert_eq!(again, Taken::Unchanged(a.clone()));
    assert_eq!(v.list().unwrap().len(), 1);
    std::fs::write(loc.dir.join(DATA), [2u8; 950]).unwrap();
    let b = new(v
        .snapshot(&loc, Meta::new(Reason::AfterSession), T0 + 120)
        .unwrap());
    assert_eq!(b.files[0].remotecache.as_ref().map(|r| r.size), Some(900));
    assert_eq!(
        cloud::status(b.files[0].remotecache.as_ref(), &b.files[0].sha1),
        cloud::Status::Waiting
    );
    let blobs = std::fs::read_dir(v.dir.join("blobs")).unwrap().count();
    assert_eq!(blobs, 3, "the unchanged index is stored once");
    assert_eq!(v.usage(), 900 + 950 + 416);
    assert_eq!(
        kinds(&v),
        [EventKind::Snapshot, EventKind::Unchanged, EventKind::Snapshot]
    );
    assert_eq!(std::fs::read(v.blob(&b.files[0].sha256)).unwrap(), [2u8; 950]);
}

#[test]
fn refuses_a_file_that_changes_while_it_is_copied() {
    let root = tmp("changing");
    let loc = steam(&root, "1", b"data");
    let v = vault(&root, &loc);
    let err = v.store_blob(&loc.dir.join(DATA), "0000").unwrap_err();
    assert!(
        err.to_string().contains("changed while it was backed up"),
        "{err}"
    );
    assert!(!v.blob("0000").exists());
}

#[test]
fn keeps_accounts_apart() {
    let root = tmp("accounts");
    let a = steam(&root, "11", b"a");
    let b = steam(&root, "22", b"b");
    let va = vault(&root, &a);
    assert_ne!(va.dir, vault(&root, &b).dir);
    assert!(va.snapshot(&b, Meta::new(Reason::Manual), T0).is_err());
    new(va.snapshot(&a, Meta::new(Reason::Manual), T0).unwrap());
    new(vault(&root, &b)
        .snapshot(&b, Meta::new(Reason::Manual), T0)
        .unwrap());
    assert_eq!(Vault::sources(&root.join("data")), ["steam-11", "steam-22"]);
    assert!(Vault::open(&root.join("data"), "../x").is_err());
}

#[test]
fn retention_keeps_pins_and_the_cap() {
    let root = tmp("retention");
    let loc = steam(&root, "5", &[0u8; 1000]);
    let mut v = vault(&root, &loc);
    v.policy.cap_bytes = 4000;
    let mut ids = Vec::new();
    for i in 0..6u8 {
        std::fs::write(loc.dir.join(DATA), [i; 1000]).unwrap();
        let meta = match i {
            1 => Meta::new(Reason::BeforeSession).pin("vanilla-session"),
            _ => Meta::new(Reason::BeforeSession),
        };
        ids.push(new(v.snapshot(&loc, meta, T0 + i64::from(i) * 60).unwrap()).id);
    }
    let left: Vec<String> = v.list().unwrap().into_iter().map(|s| s.id).collect();
    assert!(left.contains(&ids[1]), "pinned");
    assert!(left.contains(&ids[5]), "newest");
    assert!(!left.contains(&ids[0]), "oldest unpinned goes first");
    let blobs: u64 = v.usage();
    assert!(blobs <= 4000, "{blobs}");
    assert!(kinds(&v).contains(&EventKind::Pruned));
    let far = T0 + 400 * retention::DAY;
    v.policy.cap_bytes = u64::MAX;
    std::fs::write(loc.dir.join(DATA), [9u8; 1000]).unwrap();
    let last = new(v.snapshot(&loc, Meta::new(Reason::LauncherStart), far).unwrap());
    let left: Vec<String> = v.list().unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(
        left,
        [last.id.clone(), ids[1].clone()],
        "old unpinned snapshots age out"
    );
    assert_eq!(v.unpin(&ids[1]).unwrap().pinned, Vec::<String>::new());
}

#[test]
fn finds_a_save_changed_outside_the_launcher() {
    let root = tmp("outside");
    let loc = steam(&root, "7", &[1u8; 90_000]);
    let v = vault(&root, &loc);
    let data = root.join("data");
    let before = new(v
        .before_session(&loc, Meta::new(Reason::BeforeSession).profile("default"), T0)
        .unwrap()
        .taken);
    let after = v
        .after_session(&loc, Meta::new(Reason::AfterSession), Some(&before), T0 + 600)
        .unwrap();
    assert_eq!(after.taken, Taken::Unchanged(before.clone()));
    let quiet = v.check(&loc, Meta::new(Reason::LauncherStart), T0 + 700).unwrap();
    assert!(quiet.outside.is_none() && quiet.notice.is_none());

    std::fs::write(loc.dir.join(DATA), [3u8; 89_000]).unwrap();
    let small = v.check(&loc, Meta::new(Reason::LauncherStart), T0 + 800).unwrap();
    let ch = small.outside.unwrap();
    assert_eq!(ch.from.id, before.id);
    assert_eq!(ch.to.reason, Reason::ChangedOutside);
    assert_eq!(ch.to.note.as_deref(), Some("found when the launcher started"));
    assert!(ch.from.pinned.contains(&"before-outside-change".to_owned()));
    assert!(small.notice.is_none(), "1 KB smaller is ordinary play");
    assert!(Notice::read(&data).is_none());

    std::fs::write(loc.dir.join(DATA), [4u8; 40_000]).unwrap();
    let shrunk = v.check(&loc, Meta::new(Reason::BeforeSession), T0 + 900).unwrap();
    let n = shrunk.notice.unwrap();
    assert_eq!(n.kind, NoticeKind::ChangedOutside);
    assert_eq!(n.snapshot, ch.to.id);
    assert_eq!(Notice::read(&data), Some(n.clone()));
    let text = n.text();
    assert!(text.contains("changed outside a launcher session"), "{text}");
    assert!(
        text.contains(&format!("swofty saves restore {}", ch.to.id)),
        "{text}"
    );
    assert!(!text.contains("gone") && !text.contains("lost"), "{text}");
    let k = kinds(&v);
    assert!(k.contains(&EventKind::ChangedOutside) && k.contains(&EventKind::Shrank));
}

#[test]
fn a_session_that_never_ended_is_not_outside() {
    let root = tmp("unfinished");
    let loc = steam(&root, "8", &[1u8; 90_000]);
    let v = vault(&root, &loc);
    v.before_session(&loc, Meta::new(Reason::BeforeSession), T0)
        .unwrap();
    std::fs::write(loc.dir.join(DATA), [2u8; 10_000]).unwrap();
    let c = v.check(&loc, Meta::new(Reason::LauncherStart), T0 + 60).unwrap();
    assert!(c.outside.is_none());
    let s = c.taken.snapshot().unwrap();
    assert_eq!(s.reason, Reason::AfterSession);
    assert!(
        s.note
            .as_deref()
            .unwrap()
            .contains("ended without its after-session snapshot")
    );
    std::fs::write(loc.dir.join(DATA), [3u8; 10_000]).unwrap();
    assert!(
        v.check(&loc, Meta::new(Reason::LauncherStart), T0 + 120)
            .unwrap()
            .outside
            .is_some(),
        "the next change is outside again"
    );
}

#[test]
fn hints_at_a_session_shrink_only_with_an_addon_left_out() {
    let root = tmp("shrink");
    let loc = steam(&root, "9", &[1u8; 90_000]);
    let v = vault(&root, &loc);
    let before = new(v
        .before_session(&loc, Meta::new(Reason::BeforeSession), T0)
        .unwrap()
        .taken);
    std::fs::write(loc.dir.join(DATA), [2u8; 20_000]).unwrap();
    let c = v
        .after_session(&loc, Meta::new(Reason::AfterSession), Some(&before), T0 + 60)
        .unwrap();
    assert!(c.shrank.is_some() && c.notice.is_none());
    let mut meta = Meta::new(Reason::BeforeSession);
    meta.absent = vec!["swofty.progressive".into()];
    std::fs::write(loc.dir.join(DATA), [1u8; 90_000]).unwrap();
    let before = new(v.before_session(&loc, meta.clone(), T0 + 180).unwrap().taken);
    assert_eq!(
        before.reason,
        Reason::ChangedOutside,
        "the save changed since the last snapshot"
    );
    std::fs::write(loc.dir.join(DATA), [3u8; 20_000]).unwrap();
    meta.reason = Reason::AfterSession;
    let n = v
        .after_session(&loc, meta, Some(&before), T0 + 240)
        .unwrap()
        .notice
        .unwrap();
    assert_eq!(n.kind, NoticeKind::Shrank);
    assert_eq!(n.snapshot, before.id);
    assert!(n.text().contains("in a session without swofty.progressive"));
}

#[test]
fn restore_refuses_while_steam_or_the_game_runs() {
    let root = tmp("refuse");
    let loc = steam(&root, "10", &[1u8; 900]);
    let v = vault(&root, &loc);
    let s = new(v.snapshot(&loc, Meta::new(Reason::Manual), T0).unwrap());
    std::fs::write(loc.dir.join(DATA), [2u8; 500]).unwrap();
    let steam_up = Running {
        steam: true,
        ..Running::default()
    };
    let err = restore(&v, &s.id, &loc, steam_up, T0 + 60)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("Steam is running") && err.contains("Exit Steam completely"),
        "{err}"
    );
    let game = Running {
        game: true,
        ..Running::default()
    };
    assert!(
        restore(&v, &s.id, &loc, game, T0 + 60)
            .unwrap_err()
            .to_string()
            .contains("F1 25 is running")
    );
    assert_eq!(std::fs::read(loc.dir.join(DATA)).unwrap(), [2u8; 500]);
    assert_eq!(v.list().unwrap().len(), 1, "a refused restore takes no snapshot");
    assert!(kinds(&v).contains(&EventKind::RestoreRefused));
    let ea = Running {
        steam: true,
        ..Running::default()
    };
    assert_eq!(
        restore::blocker(Layout::Ea, ea),
        None,
        "Steam doesn't hold the EA copy's save"
    );
}

#[test]
fn restore_writes_the_files_and_leaves_remotecache_alone() {
    let root = tmp("restore");
    let loc = steam(&root, "12", &[1u8; 90_000]);
    let data = root.join("data");
    let v = vault(&root, &loc);
    let good = new(v.snapshot(&loc, Meta::new(Reason::AfterSession), T0).unwrap());
    std::fs::write(loc.dir.join(DATA), [2u8; 20_000]).unwrap();
    std::fs::write(loc.dir.join("savegame@profile#D1990.UEL"), b"1990").unwrap();
    v.check(&loc, Meta::new(Reason::LauncherStart), T0 + 60).unwrap();
    let cache = loc.remotecache().unwrap();
    let cache_before = std::fs::read(&cache).unwrap();
    let cache_time = std::fs::metadata(&cache).unwrap().modified().unwrap();
    assert!(Notice::read(&data).is_some());

    let r = restore(&v, &good.id, &loc, Running::default(), T0 + 120).unwrap();
    assert_eq!(std::fs::read(loc.dir.join(DATA)).unwrap(), [1u8; 90_000]);
    assert_eq!(std::fs::read(loc.dir.join(INDEX)).unwrap(), [7u8; 416]);
    assert_eq!(std::fs::read(&cache).unwrap(), cache_before);
    assert_eq!(std::fs::metadata(&cache).unwrap().modified().unwrap(), cache_time);
    assert_eq!(
        std::fs::read(loc.dir.join("savegame@highlights001#QKRHMYXE")).unwrap(),
        b"replay"
    );
    assert_eq!(r.kept, ["profile/D1990.UEL"]);
    assert!(r.log_line().starts_with(&format!("[vault] restored {}", good.id)));
    let before = v.find(r.before.as_deref().unwrap()).unwrap();
    assert!(before.pinned.contains(&"before-restore".to_owned()));
    let data_file = before
        .files
        .iter()
        .find(|f| f.name == "profile/NXDSMWW.BWW")
        .unwrap();
    assert_eq!(std::fs::read(v.blob(&data_file.sha256)).unwrap(), [2u8; 20_000]);
    assert_eq!(v.newest().unwrap().unwrap().reason, Reason::Restored);
    assert!(Notice::read(&data).is_none());
    assert!(kinds(&v).contains(&EventKind::Restored));
    let after = v.check(&loc, Meta::new(Reason::LauncherStart), T0 + 180).unwrap();
    assert!(matches!(after.taken, Taken::Unchanged(_)) && after.outside.is_none());
    assert!(
        !loc.dir
            .read_dir()
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".swofty-restore"))
    );

    std::fs::write(v.blob(&good.files[1].sha256), b"tampered").unwrap();
    let err = restore(&v, &good.id, &loc, Running::default(), T0 + 240).unwrap_err();
    assert!(err.to_string().contains("is damaged"), "{err}");
    assert_eq!(std::fs::read(loc.dir.join(INDEX)).unwrap(), [7u8; 416]);
    assert!(restore(&v, "nope", &loc, Running::default(), T0 + 240).is_err());
}

#[test]
fn the_ea_copy_has_its_own_vault() {
    let root = tmp("ea");
    let pfx = root.join("pfx");
    let profile = pfx.join("drive_c/users/steamuser/AppData/Local/F1 25/savegame/ea/profile");
    std::fs::create_dir_all(&profile).unwrap();
    std::fs::create_dir_all(profile.parent().unwrap().join("midsessions0")).unwrap();
    std::fs::write(profile.join("NXDSMWW.BWW"), [5u8; 5000]).unwrap();
    std::fs::write(profile.join("QKRHMYXE"), [6u8; 416]).unwrap();
    let loc = Location::ea_in_prefix(&pfx).unwrap();
    let v = vault(&root, &loc);
    assert!(v.dir.ends_with("career-vault/ea-steamuser"));
    let s = new(v.snapshot(&loc, Meta::new(Reason::BeforeSession), T0).unwrap());
    assert_eq!(s.layout, Layout::Ea);
    assert_eq!(s.cloud(), cloud::Status::Local);
    let names: Vec<&str> = s.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["profile/NXDSMWW.BWW", "profile/QKRHMYXE"]);
    std::fs::write(profile.join("NXDSMWW.BWW"), [0u8; 10]).unwrap();
    let app = Running {
        ea_app: true,
        ..Running::default()
    };
    let err = restore(&v, &s.id, &loc, app, T0 + 60).unwrap_err().to_string();
    assert!(err.contains("The EA app is running"), "{err}");
    let steam_up = Running {
        steam: true,
        ..Running::default()
    };
    restore(&v, &s.id, &loc, steam_up, T0 + 60).unwrap();
    assert_eq!(std::fs::read(profile.join("NXDSMWW.BWW")).unwrap(), [5u8; 5000]);
    assert!(
        Notice {
            kind: NoticeKind::ChangedOutside,
            source: loc.source.clone(),
            snapshot: s.id.clone(),
            kept_at: T0,
            before: 9000,
            after: 10,
            absent: Vec::new(),
            at: T0,
        }
        .text()
        .contains("the EA app are closed")
    );
}

#[test]
fn migrates_save_backups_and_retires_them_at_the_first_prune() {
    let root = tmp("migrate");
    let data = root.join("data");
    let loc = steam(&root, "843502855", &[1u8; 900]);
    let old = crate::save_backup::take(
        &data,
        &loc.dir,
        "default",
        &["swofty.progressive".into()],
        "20261027-010203",
        "session",
    )
    .unwrap()
    .unwrap();
    std::fs::create_dir_all(data.join("state")).unwrap();
    std::fs::write(
        data.join(crate::save_backup::NOTICE),
        format!(
            r#"{{"backup":"{}","profile":"default","absent":["swofty.progressive"],"before":90000,"after":20000}}"#,
            old.name
        ),
    )
    .unwrap();
    let notes = migrate::migrate(&data).unwrap();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(notes[0].contains("steam-843502855/20261027-010203-migrated"));
    assert!(migrate::migrate(&data).unwrap().is_empty(), "once");
    let v = vault(&root, &loc);
    let s = v.find("20261027-010203-migrated").unwrap();
    assert_eq!(s.reason, Reason::Migrated);
    assert_eq!(s.at, crate::save_backup::parse_stamp("20261027-010203").unwrap());
    assert_eq!(s.profile.as_deref(), Some("default"));
    assert_eq!(s.absent_addons, ["swofty.progressive"]);
    assert_eq!(s.files[0].sha256, old.manifest.files[0].sha256);
    assert_eq!(s.files[0].name, "profile/NXDSMWW.BWW");
    let n = Notice::read(&data).unwrap();
    assert_eq!((n.kind, n.snapshot.as_str()), (NoticeKind::Shrank, s.id.as_str()));
    assert!(!data.join(crate::save_backup::NOTICE).exists());
    assert!(data.join("save-backups").is_dir(), "kept until the first prune");

    let unchanged = v.check(&loc, Meta::new(Reason::LauncherStart), T0).unwrap();
    assert!(matches!(unchanged.taken, Taken::Unchanged(_)));
    assert!(data.join("save-backups").is_dir());
    std::fs::write(loc.dir.join(DATA), [2u8; 900]).unwrap();
    new(v
        .snapshot(&loc, Meta::new(Reason::AfterSession), T0 + 400 * retention::DAY)
        .unwrap());
    assert!(v.list().unwrap().iter().all(|s| s.reason != Reason::Migrated));
    assert!(!data.join("save-backups").exists());
    assert!(migrate::marker(&data).unwrap().retired);
}

#[test]
fn humane_times() {
    assert_eq!(human(T0), "2026-10-31 00:00 UTC");
    assert_eq!(human(T0 + 3723), "2026-10-31 01:02 UTC");
}
