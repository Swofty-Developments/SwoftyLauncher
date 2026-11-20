use super::*;

#[test]
fn heal_restores_a_half_staged_folder() {
    // The exact state an interrupted session leaves: the real launcher is
    // in .bak and the stub sits in its slot, with no journal.
    let d = temp("heal");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let launcher = game.join("EAAntiCheat.GameServiceLauncher.exe");
    std::fs::write(&launcher, "stub").unwrap();
    std::fs::write(game.join("EAAntiCheat.GameServiceLauncher.exe.bak"), "original").unwrap();
    let hook = game.join("swofty/mods/core/f1op_hook.dll");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "stale hook").unwrap();

    let journal = d.join("state/offline/staging.jsonl");
    let notes = heal_stale_backup(&game, &journal).unwrap();
    assert!(!hook.exists(), "the stale hook is swept");
    assert!(notes.iter().any(|n| n.contains("stale hook")), "{notes:?}");
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "original");
    assert!(!game.join("EAAntiCheat.GameServiceLauncher.exe.bak").exists());
    let kept = std::fs::read_dir(game.join("swofty/offline-recovery"))
        .unwrap()
        .flatten()
        .next()
        .expect("the stub is kept, never destroyed");
    assert_eq!(std::fs::read_to_string(kept.path()).unwrap(), "stub");
    assert!(notes.iter().any(|n| n.contains("restored")), "{notes:?}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn heal_drops_an_identical_stale_backup() {
    let d = temp("heal-same");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let launcher = game.join("EAAntiCheat.GameServiceLauncher.exe");
    std::fs::write(&launcher, "same").unwrap();
    std::fs::write(game.join("EAAntiCheat.GameServiceLauncher.exe.bak"), "same").unwrap();

    let journal = d.join("state/offline/staging.jsonl");
    heal_stale_backup(&game, &journal).unwrap();
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "same");
    assert!(!game.join("EAAntiCheat.GameServiceLauncher.exe.bak").exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn heal_replays_a_leftover_journal_first() {
    let d = temp("heal-journal");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let launcher = game.join("EAAntiCheat.GameServiceLauncher.exe");
    let parked = game.join("EAAntiCheat.GameServiceLauncher.exe.bak");
    std::fs::write(&launcher, "stub").unwrap();
    std::fs::write(&parked, "original").unwrap();
    let journal = d.join("state/offline/staging.jsonl");
    let line = serde_json::json!({
        "op": "rename",
        "from": launcher.display().to_string(),
        "to": parked.display().to_string(),
        "source": "",
        "sha256": "",
    });
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    std::fs::write(&journal, format!("{line}\n")).unwrap();

    let notes = heal_stale_backup(&game, &journal).unwrap();
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "original");
    assert!(!journal.exists());
    assert!(notes.iter().any(|n| n.contains("restored")), "{notes:?}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_stale_backup_is_set_aside_when_the_live_exe_is_steams_copy() {
    let d = temp("heal-depot-stale");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let launcher = game.join(EAC_EXE);
    let parked = game.join(EAC_BAK);
    std::fs::write(&launcher, "stock exe").unwrap();
    std::fs::write(&parked, "a stale backup of the wrong size").unwrap();
    let depot = FakeDepot {
        rel: EAC_EXE.into(),
        bytes: b"stock exe".to_vec(),
    };

    let notes = heal_launcher(&game, Some(&depot)).unwrap();
    assert_eq!(
        std::fs::read_to_string(&launcher).unwrap(),
        "stock exe",
        "the Steam-verified live exe is kept"
    );
    assert!(!parked.exists(), "the stale backup is moved out of the game root");
    let kept = std::fs::read_dir(game.join("swofty/offline-recovery"))
        .unwrap()
        .flatten()
        .next()
        .expect("the stale backup is set aside, never destroyed");
    assert_eq!(
        std::fs::read_to_string(kept.path()).unwrap(),
        "a stale backup of the wrong size"
    );
    assert!(notes.iter().any(|n| n.contains("set a stale")), "{notes:?}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_offline_session_backup_is_restored_over_a_stub_with_a_depot() {
    let d = temp("heal-depot-session");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let launcher = game.join(EAC_EXE);
    let parked = game.join(EAC_BAK);
    std::fs::write(&launcher, "offline play stub").unwrap();
    std::fs::write(&parked, "the real launcher").unwrap();
    let depot = FakeDepot {
        rel: EAC_EXE.into(),
        bytes: b"the real launcher".to_vec(),
    };

    let notes = heal_launcher(&game, Some(&depot)).unwrap();
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "the real launcher");
    assert!(!parked.exists());
    let kept = std::fs::read_dir(game.join("swofty/offline-recovery"))
        .unwrap()
        .flatten()
        .next()
        .expect("the stub is kept, never destroyed");
    assert_eq!(std::fs::read_to_string(kept.path()).unwrap(), "offline play stub");
    assert!(notes.iter().any(|n| n.contains("restored")), "{notes:?}");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn heal_never_restores_a_stub_backup() {
    let d = temp("heal-stub-backup");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let stub = program(None, "an older stub");
    std::fs::write(game.join(EAC_EXE), &stub).unwrap();
    std::fs::write(game.join(EAC_BAK), &stub).unwrap();

    let notes = heal_stale_backup(&game, &d.join("state/offline/staging.jsonl")).unwrap();
    assert!(!game.join(EAC_EXE).exists(), "{notes:?}");
    assert!(!game.join(EAC_BAK).exists(), "{notes:?}");
    assert_eq!(recovered(&game), vec![stub.clone(), stub]);
    assert!(
        notes.iter().any(|n| n.contains("Repair F1 25 in the EA app")),
        "{notes:?}"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn heal_before_check_waits_while_a_session_is_pending() {
    let d = temp("heal-pending");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join(EAC_EXE), "stub").unwrap();
    std::fs::write(game.join(EAC_BAK), "real").unwrap();
    let journal = journal_path(&d);
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    std::fs::write(&journal, "{}\n").unwrap();
    let depot = FakeDepot {
        rel: EAC_EXE.into(),
        bytes: b"real".to_vec(),
    };

    let notes = heal_before_check(&d, &game, Some(&depot)).unwrap();
    assert!(
        notes.is_empty(),
        "the pending session's journal owns the launcher"
    );
    assert!(game.join(EAC_BAK).exists());
    assert_eq!(std::fs::read_to_string(game.join(EAC_EXE)).unwrap(), "stub");
    std::fs::remove_dir_all(&d).ok();
}
