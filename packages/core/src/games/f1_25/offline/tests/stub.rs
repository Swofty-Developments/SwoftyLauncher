use super::*;

#[test]
fn only_a_program_eas_certificate_names_counts_as_eas() {
    let d = temp("signer");
    let file = d.join("file.exe");
    for (bytes, want) in [
        (program(Some("Electronic Arts"), "launcher"), Some(true)),
        (program(Some("Someone Else"), "launcher"), Some(false)),
        (program(None, "stub"), Some(false)),
        (b"genuine".to_vec(), None),
    ] {
        std::fs::write(&file, bytes).unwrap();
        assert_eq!(ea_signed(&file).unwrap(), want);
    }
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_ea_apps_repaired_launcher_becomes_the_backup_and_the_stub_goes_back() {
    let d = temp("restage-replaced");
    let (journal, launcher, parked, _) = staged(&d);
    let a = stub_artifacts(&d);
    let repaired = program(Some("Electronic Arts"), "a newer launcher");
    std::fs::write(&launcher, &repaired).unwrap();

    assert_eq!(slot(&journal).unwrap(), Some(Slot::Replaced));
    assert_eq!(restage(&a, &journal).unwrap(), Some(Slot::Replaced));
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "stub");
    assert_eq!(std::fs::read(&parked).unwrap(), repaired);
    assert_eq!(recovered(&d.join("game")), vec![b"genuine".to_vec()]);
    assert_eq!(slot(&journal).unwrap(), Some(Slot::Staged));
    let text = std::fs::read_to_string(&journal).unwrap();
    assert!(text.contains(&sha256_file(&parked).unwrap()), "{text}");

    restore(&journal, false).unwrap();
    assert_eq!(std::fs::read(&launcher).unwrap(), repaired);
    assert!(!parked.exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_stub_the_ea_app_removed_is_put_back_over_the_same_backup() {
    let d = temp("restage-missing");
    let (journal, launcher, parked, _) = staged(&d);
    let a = stub_artifacts(&d);
    assert_eq!(restage(&a, &journal).unwrap(), Some(Slot::Staged));
    std::fs::remove_file(&launcher).unwrap();

    assert_eq!(restage(&a, &journal).unwrap(), Some(Slot::Missing));
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "stub");
    assert_eq!(std::fs::read_to_string(&parked).unwrap(), "genuine");
    restore(&journal, false).unwrap();
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "genuine");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_launcher_that_is_neither_eas_nor_the_stub_is_left_alone() {
    let d = temp("restage-unknown");
    let (journal, launcher, parked, _) = staged(&d);
    let a = stub_artifacts(&d);
    std::fs::write(&launcher, program(Some("Someone Else"), "x")).unwrap();
    assert_eq!(restage(&a, &journal).unwrap(), Some(Slot::Unknown));
    assert_eq!(
        std::fs::read(&launcher).unwrap(),
        program(Some("Someone Else"), "x")
    );
    assert_eq!(std::fs::read_to_string(&parked).unwrap(), "genuine");
    assert!(restage(&a, &d.join("none.jsonl")).unwrap().is_none());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn restore_keeps_a_newer_launcher_the_ea_app_put_back() {
    let d = temp("restore-ea-newer");
    let (journal, launcher, parked, _) = staged(&d);
    let repaired = program(Some("Electronic Arts"), "a newer launcher");
    std::fs::write(&launcher, &repaired).unwrap();
    let done = restore(&journal, false).unwrap();
    assert!(
        done.iter().any(|l| l.contains("was replaced by the EA app")),
        "{done:?}"
    );
    assert_eq!(std::fs::read(&launcher).unwrap(), repaired);
    assert!(!parked.exists());
    assert_eq!(recovered(&d.join("game")), vec![b"genuine".to_vec()]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn restore_never_puts_the_stub_back_as_the_original() {
    let d = temp("restore-stub-backup");
    let (journal, launcher, parked, _) = staged(&d);
    std::fs::write(&parked, "stub").unwrap();
    let done = restore(&journal, false).unwrap();
    assert!(done.iter().any(|l| l.contains("not EA's")), "{done:?}");
    assert!(!launcher.exists());
    assert!(!parked.exists());
    assert_eq!(recovered(&d.join("game")), vec![b"stub".to_vec()]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn staging_refuses_a_game_folder_with_no_anti_cheat_launcher() {
    let d = temp("no-launcher");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let a = stub_artifacts(&d);
    let journal = d.join("state/offline/journal.jsonl");
    let err = stage(&game, &a, &journal).unwrap_err();
    assert!(
        matches!(&err, Error::MissingStock { files, .. } if files == &[EAC_EXE.to_owned()]),
        "{err}"
    );
    assert!(
        err.to_string().contains("verify the game files in Steam"),
        "{err}"
    );
    assert!(!game.join(EAC_EXE).exists());
    assert!(!journal.exists());
    std::fs::remove_dir_all(&d).ok();
}
