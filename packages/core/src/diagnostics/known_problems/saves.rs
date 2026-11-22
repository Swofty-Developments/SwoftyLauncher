use super::Diagnosis;
use super::evidence::Evidence;

/// The start of the line the core logs when a profile save could not write the kept save containers back
/// (native/src/core/career/save_splice.cpp `children_inner`): the game then fails that save and shows its corrupted
/// save prompt. Code 55 (0x37) is the game's save buffer being full.
pub(super) const KEPT_WRITE_FAILED: &str =
    "[saves] profile save: writing back the kept save containers failed with code ";

/// The line an older core logged when it put the race highlights save's own container back into the profile save
/// (native/src/core/career/plain_saves_logic.hpp `put_back_line`).
pub(super) const HIGHLIGHTS_PUT_BACK: &str = "[saves] put back ReplayHighlights_ReplayHighlights:";

/// The codes of the profile saves that failed while the core wrote the kept save containers back, in order.
pub(super) fn failed_saves(loader: &str) -> Vec<u32> {
    loader
        .lines()
        .filter_map(|l| {
            let rest = &l[l.find(KEPT_WRITE_FAILED)? + KEPT_WRITE_FAILED.len()..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        })
        .collect()
}

/// A session whose profile saves failed: the core could not write the kept careers back into the save. An older core
/// put the race highlights into the profile save, which made every save too big for the game to write.
pub fn unsaved_profile(e: &Evidence) -> Option<Diagnosis> {
    let failed = failed_saves(&e.loader);
    let code = *failed.first()?;
    let highlights = e.loader.contains(HIGHLIGHTS_PUT_BACK);
    let times = match failed.len() {
        1 => "once".to_owned(),
        n => format!("{n} times"),
    };
    let what = if highlights {
        format!(
            "F1 25 could not save your profile this session ({times}) and showed its corrupted save prompt. The mod \
             loader had put your race highlights into the profile save, which made the save too big for the game to \
             write. Your save file was not changed: it holds your careers as of the last save that worked."
        )
    } else {
        format!(
            "F1 25 could not save your profile this session ({times}, code {code}) and showed its corrupted save \
             prompt: the mod loader could not write the careers of your addon career modes back into the save. Your \
             save file was not changed: it holds your careers as of the last save that worked."
        )
    };
    let mut steps = Vec::new();
    if highlights {
        steps.push(
            "Update Swofty Launcher, then press Play. The update keeps the race highlights out of your profile \
             save, so the game saves again, with your careers."
                .to_owned(),
        );
    }
    steps.push(
        "On the corrupted save prompt, Continue Without Saving and Cancel both leave your save file as it is. Only \
         the progress since the last save that worked is not kept."
            .to_owned(),
    );
    steps.push(if highlights {
        "If the game still can't save after the update, press Send Logs so the save lines in this session's log \
         reach support."
            .to_owned()
    } else {
        "Press Send Logs so the save lines in this session's log reach support.".to_owned()
    });
    Some(Diagnosis {
        id: "profile_not_saved",
        title: "F1 25 could not save your profile".to_owned(),
        what,
        steps,
    })
}
