use swofty_core::dbedit::ImportReport;
use swofty_core::dbedit::draft::Written;
use swofty_core::dbedit::package::{Imported, PackageInfo};

use super::draft::DraftBook;
use super::import::imported_text;
use super::save::{overrides_text, parts_text, saved_new_text};
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

fn installed(id: &str, name: &str) -> PackageInfo {
    PackageInfo {
        id: id.into(),
        name: name.into(),
        version: "1.0.0".into(),
        author: String::new(),
        description: String::new(),
        changes: 0,
        game_build: String::new(),
    }
}

fn book_on(addon: Option<&str>) -> DraftBook {
    let mut book = DraftBook {
        addons: vec![
            installed("swofty-db-a", "Faster pits"),
            installed("swofty-db-b", "Wet tyres"),
        ],
        ..DraftBook::default()
    };
    book.draft.addon = addon.map(Into::into);
    book
}

#[test]
fn a_draft_for_no_addon_is_the_new_addon() {
    let book = book_on(None);
    assert!(book.bound().is_none());
    assert_eq!(book.draft_name(), "the new addon");
}

#[test]
fn a_draft_bound_to_an_installed_addon_goes_by_its_name() {
    let book = book_on(Some("swofty-db-b"));
    assert_eq!(book.bound().map(|p| p.name.as_str()), Some("Wet tyres"));
    assert_eq!(
        book.addon("swofty-db-a").map(|p| p.name.as_str()),
        Some("Faster pits")
    );
    assert_eq!(book.draft_name(), "Wet tyres");
}

#[test]
fn a_draft_whose_addon_is_gone_goes_by_its_id() {
    let book = book_on(Some("swofty-db-gone"));
    assert!(book.bound().is_none());
    assert_eq!(book.draft_name(), "swofty-db-gone");
}

#[test]
fn a_save_drops_the_read_error_and_reads_the_addon_again() {
    let mut book = DraftBook {
        addon_error: Some("The addon file is damaged".into()),
        seen: Some(None),
        ..book_on(Some("swofty-db-a"))
    };
    book.saved();
    assert!(book.addon_error.is_none());
    assert!(book.seen.is_none());
    assert_eq!(book.draft_name(), "Faster pits");
}

#[test]
fn a_new_addons_notice_names_the_profile_it_was_turned_on_in() {
    assert_eq!(
        saved_new_text("Faster pits", Some("Main")),
        "Saved Faster pits and turned it on in Main"
    );
    assert_eq!(saved_new_text("Faster pits", None), "Saved Faster pits");
}

#[test]
fn an_imported_addons_notice_reports_the_import_a_rename_and_the_profile() {
    let done = Imported {
        package: installed("swofty-db-a-2", "Faster pits"),
        report: ImportReport {
            format: "package",
            imported: 3,
            unchanged: 1,
            skipped: Vec::new(),
        },
        renamed_from: Some("swofty-db-a".into()),
    };
    assert_eq!(
        imported_text(&done, Some("Main")),
        "DB Editor addon\n3 imported · 1 unchanged\nswofty-db-a was already installed; this copy is swofty-db-a-2\nTurned on in Main"
    );
    let first = Imported {
        renamed_from: None,
        ..done
    };
    assert_eq!(
        imported_text(&first, None),
        "DB Editor addon\n3 imported · 1 unchanged"
    );
}

#[test]
fn the_driver_manager_popup_names_the_profile_and_each_drivers_values() {
    use swofty_core::dbedit::driver_manager::Part;
    assert_eq!(
        overrides_text("Main"),
        "Driver Manager is on in Main. When you play, its Game drivers changes replace these values from this addon:"
    );
    assert_eq!(parts_text(&[Part::Head]), "Head");
    assert_eq!(parts_text(&[Part::Helmet]), "Helmet");
    assert_eq!(parts_text(&[Part::Head, Part::Helmet]), "Head and helmet");
}
