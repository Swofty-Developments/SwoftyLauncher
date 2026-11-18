use super::*;

#[test]
fn restore_replays_the_journal_backwards() {
    let d = temp("restore");
    let game = d.join("game");
    std::fs::create_dir_all(game.join("swofty/mods/core")).unwrap();
    let launcher = game.join("EAAntiCheat.GameServiceLauncher.exe");
    let parked = game.join("EAAntiCheat.GameServiceLauncher.exe.bak");
    let hook = game.join("swofty/mods/core/f1op_hook.dll");
    std::fs::write(&launcher, "original").unwrap();
    std::fs::rename(&launcher, &parked).unwrap();
    std::fs::write(&launcher, "stub").unwrap();
    std::fs::write(&hook, "hook").unwrap();

    let stub_digest = sha256_file(&launcher).unwrap();
    let hook_digest = sha256_file(&hook).unwrap();
    let journal = d.join("staging.jsonl");
    // Written through serde_json so paths are escaped the way the real
    // writer escapes them (Windows paths contain backslashes).
    let lines = [
        serde_json::json!({"op": "meta", "game_dir": "x", "created_utc": "", "schema_version": "1"}),
        serde_json::json!({"op": "rename", "from": launcher.display().to_string(), "to": parked.display().to_string(), "source": "", "sha256": ""}),
        serde_json::json!({"op": "copy", "from": "", "to": launcher.display().to_string(), "source": "stub", "sha256": stub_digest}),
        serde_json::json!({"op": "copy", "from": "", "to": hook.display().to_string(), "source": "hook", "sha256": hook_digest}),
    ];
    std::fs::write(
        &journal,
        lines
            .iter()
            .map(serde_json::Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();

    restore(&journal, false).unwrap();
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "original");
    assert!(!parked.exists());
    assert!(!hook.exists());

    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_ea_app_repair_that_put_the_launcher_back_counts_as_restored() {
    let d = temp("ea-repair");
    let (journal, launcher, parked, hook) = staged(&d);
    std::fs::write(&launcher, "genuine, repaired").unwrap();
    std::fs::remove_file(&parked).unwrap();
    let done = restore(&journal, false).unwrap();
    assert!(
        done.iter()
            .any(|l| l.ends_with("was put back by a game repair or update; nothing to restore")),
        "{done:?}"
    );
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "genuine, repaired");
    assert!(!hook.exists());
    assert_eq!(restore_pending(&d, &d.join("game")).unwrap(), Some(Vec::new()));
    assert!(!journal.exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_steam_verify_that_left_an_identical_backup_counts_as_restored() {
    let d = temp("steam-verify");
    let (journal, launcher, parked, _) = staged(&d);
    std::fs::write(&launcher, "genuine").unwrap();
    let done = restore(&journal, false).unwrap();
    assert!(
        done.iter().any(|l| l.contains("removed its identical backup")),
        "{done:?}"
    );
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "genuine");
    assert!(!parked.exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_backup_that_differs_from_the_repaired_launcher_still_stops_the_restore() {
    let d = temp("differing-backup");
    let (journal, launcher, parked, _) = staged(&d);
    std::fs::write(&launcher, "a newer genuine launcher").unwrap();
    let err = restore(&journal, false).unwrap_err().to_string();
    assert!(err.contains("already exists; left as is"), "{err}");
    assert!(!err.contains("digest mismatch"), "{err}");
    assert_eq!(
        std::fs::read_to_string(&launcher).unwrap(),
        "a newer genuine launcher"
    );
    assert_eq!(std::fs::read_to_string(&parked).unwrap(), "genuine");

    restore(&journal, true).unwrap();
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "genuine");
    assert!(!parked.exists());

    std::fs::write(&parked, "a leftover backup").unwrap();
    std::fs::remove_file(&journal).unwrap();
    let notes = heal_stale_backup(&d.join("game"), &journal).unwrap();
    assert!(!parked.exists(), "{notes:?}");
    assert!(launcher.exists(), "{notes:?}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_launcher_and_backup_both_gone_still_stop_the_restore() {
    let d = temp("both-gone");
    let (journal, launcher, parked, _) = staged(&d);
    std::fs::remove_file(&launcher).unwrap();
    std::fs::remove_file(&parked).unwrap();
    let err = restore(&journal, false).unwrap_err().to_string();
    assert!(err.contains("staged backup is missing"), "{err}");
    assert!(restore_pending(&d, &d.join("game")).is_err());
    assert!(journal.exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_healthy_journal_reads_and_restores_as_before() {
    let d = temp("journal-healthy");
    let (journal, ..) = staged(&d);
    let before = std::fs::read(&journal).unwrap();
    let j = Journal::read(&journal).unwrap().unwrap();
    assert!(!j.damaged());
    assert_eq!((j.lines.len(), j.torn, j.lost), (4, 0, 0));
    j.write(&journal).unwrap();
    assert_eq!(std::fs::read(&journal).unwrap(), before);
    assert_eq!(slot(&journal).unwrap(), Some(Slot::Staged));
    assert_eq!(put_back(&d, &before), Vec::<String>::new());
    assert!(!d.join("game/swofty/offline-recovery").exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_torn_last_line_is_skipped() {
    let d = temp("journal-torn");
    let mut lines = journal_text(&d);
    lines.push(r#"{"op":"copy","from":"","to":"C:\\Games\\F1 25\\swo"#.into());
    let text = lines.join("\n");
    let (journal, ..) = staged(&d);
    std::fs::write(&journal, &text).unwrap();
    assert_eq!(slot(&journal).unwrap(), Some(Slot::Staged));
    let notes = put_back(&d, text.as_bytes());
    assert!(
        notes[0].starts_with("skipped 1 unfinished line(s) at the end"),
        "{notes:?}"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_zero_filled_tail_is_skipped() {
    let d = temp("journal-zeros");
    let mut text = (journal_text(&d).join("\n") + "\n").into_bytes();
    text.extend([0; 4096]);
    let notes = put_back(&d, &text);
    assert!(
        notes[0].starts_with("skipped 1 unfinished line(s) at the end"),
        "{notes:?}"
    );

    let mut text = journal_text(&d).join("\n").into_bytes();
    text.extend([0; 512]);
    let notes = put_back(&d, &text);
    assert!(
        notes[0].starts_with("skipped 1 unfinished line(s) at the end"),
        "{notes:?}"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_unreadable_line_before_the_last_never_strands_the_launcher() {
    let d = temp("journal-middle");
    // The line that parked the game's launcher is the one lost.
    let mut lines = journal_text(&d);
    lines[1] = "\u{0}\u{0}\u{0}garbage".into();
    let notes = put_back(&d, (lines.join("\n") + "\n").as_bytes());
    assert!(notes[0].contains("1 of them before its last entry"), "{notes:?}");
    assert!(
        notes.iter().any(|n| n.starts_with("restored EAAntiCheat")),
        "{notes:?}"
    );

    // The line that placed offline play's file is the one lost: the
    // replay finds an unknown file in the slot, and the backup wins.
    let mut lines = journal_text(&d);
    lines[2] = "not json".into();
    let notes = put_back(&d, (lines.join("\n") + "\n").as_bytes());
    assert!(
        notes.iter().any(|n| n.contains("could not be replayed")),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.starts_with("kept the staged file at")),
        "{notes:?}"
    );

    // A line that is JSON but names no operation is as unreadable.
    let mut lines = journal_text(&d);
    lines[1] = "{}".into();
    let notes = put_back(&d, (lines.join("\n") + "\n").as_bytes());
    assert!(notes[0].contains("1 of them before its last entry"), "{notes:?}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_empty_or_zero_filled_journal_restores_from_the_backup() {
    let d = temp("journal-empty");
    let notes = put_back(&d, b"");
    assert_eq!(notes[0], "offline play's record of this session is empty");
    let notes = put_back(&d, &[0; 2048]);
    assert!(notes[0].starts_with("skipped 1 unfinished line(s)"), "{notes:?}");
    put_back(&d, b"\n\n");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_rewrite_of_a_damaged_journal_still_says_it_was_damaged() {
    let d = temp("journal-marked");
    let mut lines = journal_text(&d);
    lines[1] = "garbage".into();
    let (journal, launcher, parked, _) = staged(&d);
    std::fs::write(&journal, lines.join("\n") + "\n").unwrap();
    let j = Journal::read(&journal).unwrap().unwrap();
    assert_eq!((j.lines.len(), j.torn, j.lost), (3, 0, 1));
    j.write(&journal).unwrap();
    assert!(!journal.with_extension("jsonl.tmp").exists());
    let again = Journal::read(&journal).unwrap().unwrap();
    assert_eq!((again.lines.len(), again.torn, again.lost), (3, 0, 0));
    assert!(again.damaged());
    let notes = restore_pending(&d, &d.join("game")).unwrap().unwrap();
    assert_eq!(
        std::fs::read_to_string(&launcher).unwrap(),
        "genuine",
        "{notes:?}"
    );
    assert!(!parked.exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn restore_refuses_a_changed_file() {
    let d = temp("tamper");
    let launcher = d.join("EAAntiCheat.GameServiceLauncher.exe");
    std::fs::write(&launcher, "tampered").unwrap();
    let journal = d.join("staging.jsonl");
    let line = serde_json::json!({
        "op": "copy",
        "from": "",
        "to": launcher.display().to_string(),
        "source": "stub",
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
    });
    std::fs::write(&journal, format!("{line}\n")).unwrap();
    assert!(restore(&journal, false).is_err());
    assert!(restore(&journal, true).is_ok());
    assert!(!launcher.exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn restore_parses_escaped_paths() {
    // Guard the JSON escaping of paths with backslashes on every platform,
    // not just Windows where the paths actually look like this.
    let d = temp("escape");
    let journal = d.join("staging.jsonl");
    let line = serde_json::json!({
        "op": "copy",
        "from": "",
        "to": "C:\\Games\\Steam\\steamapps\\common\\F1 25\\staged.dll",
        "source": "stub",
        "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
    });
    std::fs::write(&journal, format!("{line}\n")).unwrap();
    // The file does not exist, so restore reports nothing to undo — the
    // point is that the line parses and the backslashes survive.
    let done = restore(&journal, true).unwrap();
    assert!(done.is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn restore_follows_non_ascii_paths() {
    // The extension writes paths as raw UTF-8, as for a data folder under
    // a Windows user name such as Naël.
    let d = temp("unicode");
    let base = d.join("Naël Müller 测试");
    let game = base.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let launcher = game.join("EAAntiCheat.GameServiceLauncher.exe");
    let parked = game.join("EAAntiCheat.GameServiceLauncher.exe.bak");
    std::fs::write(&parked, "original").unwrap();
    std::fs::write(&launcher, "stub").unwrap();
    let stub_digest = sha256_file(&launcher).unwrap();
    let journal = journal_path(&base);
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    let lines = [
        serde_json::json!({"op": "meta", "game_dir": game.display().to_string(), "created_utc": "", "schema_version": "1"}),
        serde_json::json!({"op": "rename", "from": launcher.display().to_string(), "to": parked.display().to_string(), "source": "", "sha256": ""}),
        serde_json::json!({"op": "copy", "from": "", "to": launcher.display().to_string(), "source": base.join("stub.exe").display().to_string(), "sha256": stub_digest}),
    ];
    let text = lines.iter().map(|l| l.to_string() + "\n").collect::<String>();
    assert!(text.contains("Naël Müller 测试"), "{text}");
    std::fs::write(&journal, text).unwrap();

    restore(&journal, false).unwrap();
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "original");
    assert!(!parked.exists());
    std::fs::remove_dir_all(&d).ok();
}
