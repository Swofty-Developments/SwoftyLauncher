//! Addons on in the session whose installed version was withdrawn from the Addon Browser
//! ([`crate::catalog::withdrawn`]).

use super::Diagnosis;
use super::evidence::Evidence;
use super::stall::{BOOT_STALL_SECS, minutes, start_screen_secs};
use super::wording::{list, name};

/// The withdrawn addons that are on, as `BMW 2026 1.0.7`.
fn labels(e: &Evidence) -> Vec<String> {
    e.withdrawn
        .iter()
        .map(|(id, version)| format!("{} {version}", name(e, id)))
        .collect()
}

fn title(labels: &[String]) -> String {
    match labels {
        [one] => format!("{one} was withdrawn: turn it off"),
        many => format!("{} addons that are on were withdrawn: turn them off", many.len()),
    }
}

fn turn_off(e: &Evidence) -> String {
    let names: Vec<String> = e.withdrawn.iter().map(|(id, _)| name(e, id)).collect();
    format!(
        "Turn off {} in your profile, then press Play again.",
        list(&names)
    )
}

/// A stall on Loading... after Press any button of [`BOOT_STALL_SECS`] or more with a withdrawn addon on.
pub(super) fn withdrawn_stall(e: &Evidence) -> Option<Diagnosis> {
    let secs = start_screen_secs(e).filter(|s| *s >= BOOT_STALL_SECS)?;
    let labels = labels(e);
    if labels.is_empty() {
        return None;
    }
    let (is, was) = if labels.len() == 1 {
        ("is", "was")
    } else {
        ("are", "were")
    };
    Some(Diagnosis {
        id: "withdrawn_addon_stall",
        title: title(&labels),
        what: format!(
            "F1 25 stayed on Loading... after Press any button for {} and never reached the main menu. {} {is} \
             on, and {was} withdrawn from the Addon Browser.",
            minutes(secs),
            list(&labels)
        ),
        steps: vec![turn_off(e)],
    })
}

/// A crash no other rule recognises, with a withdrawn addon on.
pub(super) fn withdrawn_crash(e: &Evidence) -> Option<Diagnosis> {
    let labels = labels(e);
    let (is, was) = match labels.len() {
        0 => return None,
        1 => ("was", "That version was"),
        _ => ("were", "Those versions were"),
    };
    Some(Diagnosis {
        id: "withdrawn_addon",
        title: title(&labels),
        what: format!(
            "F1 25 crashed while {} {is} on. {was} withdrawn from the Addon Browser.",
            list(&labels)
        ),
        steps: vec![turn_off(e)],
    })
}

/// `d` with a last step naming the withdrawn addons that are on, when it is not about them already.
pub(super) fn noted(mut d: Diagnosis, e: &Evidence) -> Diagnosis {
    let labels = labels(e);
    if labels.is_empty() || d.id.starts_with("withdrawn_addon") {
        return d;
    }
    let (was, it) = if labels.len() == 1 {
        ("was", "it")
    } else {
        ("were", "them")
    };
    d.steps.push(format!(
        "{} {was} withdrawn from the Addon Browser: turn {it} off.",
        list(&labels)
    ));
    d
}
