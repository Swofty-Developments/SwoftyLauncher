use swofty_core::dbedit::draft::Written;

use super::state::Autosave;

fn changed() -> Autosave {
    let mut autosave = Autosave::default();
    autosave.mark();
    autosave
}

#[test]
fn only_the_last_change_in_a_burst_starts_a_write() {
    let mut autosave = Autosave::default();
    let first = autosave.mark();
    let second = autosave.mark();
    assert!(!autosave.due(first));
    assert!(autosave.due(second));
}

#[test]
fn a_write_starts_only_with_changes_and_none_running() {
    let mut autosave = Autosave::default();
    assert!(!autosave.begin());
    autosave.mark();
    assert!(autosave.begin());
    assert!(autosave.writing && !autosave.unwritten);
    autosave.mark();
    assert!(!autosave.begin());
}

#[test]
fn changes_made_during_a_write_are_written_after_it() {
    let mut autosave = changed();
    autosave.begin();
    autosave.mark();
    assert!(autosave.written(Ok(Written::Saved)));
    assert!(!autosave.writing && autosave.error.is_none());
}

#[test]
fn a_saved_write_with_nothing_new_is_idle() {
    let mut autosave = changed();
    autosave.begin();
    assert!(!autosave.written(Ok(Written::Saved)));
    assert!(autosave.idle());
}

#[test]
fn a_newer_draft_from_another_window_stops_writing_until_one_is_picked() {
    let mut autosave = changed();
    autosave.begin();
    assert!(!autosave.written(Ok(Written::Newer)));
    assert!(autosave.newer && autosave.unwritten);
    autosave.mark();
    assert!(!autosave.begin());
    let before = autosave.ticket;
    autosave.give_way();
    assert!(autosave.idle());
    assert!(autosave.ticket > before);
}

#[test]
fn a_failed_write_keeps_the_changes_and_says_why() {
    let mut autosave = changed();
    autosave.begin();
    assert!(!autosave.written(Err("disk full".into())));
    assert!(autosave.unwritten);
    assert!(autosave.error.as_deref().is_some_and(|e| e.contains("disk full")));
    assert!(autosave.begin());
}
