use super::*;

#[test]
fn antivirus_codes_get_the_plain_message() {
    assert!(virus_code(225) && virus_code(226));
    assert!(!virus_code(5) && !virus_code(32));
    assert!(ANTIVIRUS.contains("antivirus") && ANTIVIRUS.contains("SWOFTY_DATA_DIR"));
    assert!(!ANTIVIRUS.contains("os error"));
    let e = std::io::Error::from_raw_os_error(225);
    assert_eq!(virus_blocked(&e), cfg!(windows));
    assert!(!virus_blocked(&std::io::Error::from_raw_os_error(5)));
}

#[test]
fn a_stub_missing_beside_its_backup_is_not_an_antivirus_block_without_the_log_line() {
    let d = temp("stub-missing");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join(EAC_BAK), "real").unwrap();
    assert_eq!(stub_blocked(&game, &[], 0, None), None);
    let other = "20\t[2026-11-01T01:57:04.895Z]\tINFO\tcreateProcess ok".to_owned();
    assert_eq!(stub_blocked(&game, &[other], 0, None), None);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_proxy_log_names_the_blocked_stub_even_while_it_is_present() {
    let d = temp("stub-logged");
    let game = d.join("game");
    std::fs::create_dir_all(&game).unwrap();
    std::fs::write(game.join(EAC_BAK), "real").unwrap();
    let log = format!(
        "20\t[2026-11-01T01:57:04.895Z]\tERROR   \t(eax::services::process::ProcessWin::createProcess)\t[PROCESS] \
         Create Process: Operation did not complete successfully because the file contains a virus or \
         potentially unwanted software. | exe=[{}] | OS ErrorCode:  0xe1\n",
        game.join(EAC_EXE).display()
    );
    let block = stub_blocked(&game, &[log], 0, None).unwrap();
    assert_eq!(block.code, 225);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_proxy_finding_no_stub_after_staging_is_an_antivirus_removal() {
    let game = Path::new(r"C:\Program Files\EA Games\F1 25");
    let staged = crate::catalog::epoch("2026-11-13T09:59:49Z").unwrap();
    let log = missing_line(game, "2026-11-13T10:00:00.698Z");
    let block = stub_blocked(game, std::slice::from_ref(&log), 0, Some(staged)).unwrap();
    assert_eq!(block, stub_removed(game));
    assert_eq!(
        block.blocker(),
        Some(crate::platform::app_control::Blocker::Antivirus)
    );
    assert!(block.text().contains("antivirus"), "{}", block.text());
    assert_eq!(stub_blocked(game, std::slice::from_ref(&log), 0, None), None);
    let restaged = crate::catalog::epoch("2026-11-13T10:12:00Z").unwrap();
    assert_eq!(stub_blocked(game, &[log], 0, Some(restaged)), None);
}

#[test]
fn a_genuine_ea_repair_leaves_no_antivirus_trace_in_the_proxy_log() {
    let game = Path::new(r"C:\Program Files\EA Games\F1 25");
    let staged = crate::catalog::epoch("2026-11-12T05:50:00Z").unwrap();
    let started = format!(
        "17\t[2026-11-12T05:54:53.240Z]\tINFO    \t(eax::services::process::ProcessWin::createProcess)\t\
         [PROCESS] exe=[{}], args=[] | PID: 16136 | \n",
        game.join(EAC_EXE).display()
    );
    assert_eq!(stub_blocked(game, &[started], 0, Some(staged)), None);
}

#[test]
fn the_staging_time_is_the_journals_last_write() {
    let d = temp("staged-at");
    let journal = d.join("staging.jsonl");
    assert_eq!(staged_at(&journal), None);
    std::fs::write(&journal, "{}\n").unwrap();
    let at = staged_at(&journal).unwrap();
    assert!(at.elapsed().unwrap_or_default() < std::time::Duration::from_secs(60));
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn names_the_stub_when_only_it_is_gone_from_the_extension() {
    let d = temp("ext-stub-gone");
    let dir = extension_dir(&d);
    std::fs::create_dir_all(&dir).unwrap();
    for name in REQUIRED {
        let to = dir.join(name);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, "x").unwrap();
    }
    assert_eq!(stub_removed_from_extension(&d), None);
    std::fs::remove_file(dir.join(STUB)).unwrap();
    let stub = stub_removed_from_extension(&d).unwrap();
    assert_eq!(stub, dir.join(STUB));
    let text = stub_removed_text(&d, &stub);
    assert!(text.contains("antivirus most likely removed it"), "{text}");
    assert!(
        text.contains("quarantine") && text.contains("exclusion"),
        "{text}"
    );
    assert!(text.contains(&d.display().to_string()), "{text}");
    std::fs::remove_file(dir.join("f1op_hook.dll")).unwrap();
    assert_eq!(stub_removed_from_extension(&d), None);
    std::fs::remove_dir_all(&d).ok();
}
