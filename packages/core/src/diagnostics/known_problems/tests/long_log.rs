use std::io::Cursor;

use super::*;

fn padded(head: &str, tail: &str, filler: usize) -> Vec<u8> {
    let mut log = head.to_owned();
    for i in 0..filler {
        log.push_str(&format!(
            "2026-11-25 10:00:00.000 [1] [overlay] filler line {i}\n"
        ));
    }
    log.push_str(tail);
    log.into_bytes()
}

#[test]
fn crash_lines_past_the_cap_are_read() {
    let log = padded("2026-11-25 09:00:00.000 [1] [system] boot\n", GPU, 4000);
    let text = read_head_and_tail(Cursor::new(log.clone()), log.len() as u64 / 2, 1024).unwrap();
    assert!(text.len() < log.len());
    assert!(gpu_removed(&ev(&text, &["F1_25.exe"])));
}

#[test]
fn boot_lines_before_the_cap_are_kept() {
    let log = padded("2026-11-25 09:00:00.000 [1] [system] boot\n", GPU, 4000);
    let text = read_head_and_tail(Cursor::new(log.clone()), log.len() as u64 / 2, 1024).unwrap();
    assert!(text.starts_with("2026-11-25 09:00:00.000 [1] [system] boot\n"));
    assert!(text.contains("were not read"));
}

#[test]
fn a_log_under_the_cap_is_read_whole() {
    let log = padded("a\n", "b\n", 10);
    let text = read_head_and_tail(Cursor::new(log.clone()), log.len() as u64, 8).unwrap();
    assert_eq!(text.as_bytes(), &log[..]);
}
