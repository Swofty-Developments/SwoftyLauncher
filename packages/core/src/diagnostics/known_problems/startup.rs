//! F1 25 closing itself in its own code as it starts, before its window and before it made its graphics,
//! with an addon on that carries its own copy of F1 25's packed game data (`game.dat`).

use crate::diagnostics::launch::LoaderSeen;

use super::graphics::graphics_started;
use super::wording::{VERIFY, name};
use super::{Crash, Diagnosis};

pub(super) fn packed_game_data(c: &Crash) -> Option<Diagnosis> {
    let e = c.evidence;
    let (id, packed) = e.packed.first()?;
    let in_game = c
        .fatal
        .module
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case(crate::GAME_EXE));
    if !in_game || LoaderSeen::scan(&e.loader).window || graphics_started(c) {
        return None;
    }
    let addon = name(e, id);
    let files = packed.names();
    let made_for = packed
        .other
        .as_ref()
        .map(|o| format!(" {addon} is made for {}, not F1 25 (it has {}).", o.game, o.sign))
        .unwrap_or_default();
    let mut steps = vec![format!("Turn off {addon} in your profile, then press Play.")];
    if packed.other.is_some() {
        steps.push(format!(
            "A mod made for another F1 game doesn't work in F1 25. Remove {addon} on the Addons page."
        ));
    }
    steps.push(VERIFY.to_owned());
    Some(Diagnosis {
        id: "packed_game_data",
        title: format!("{addon} stopped F1 25 from starting"),
        what: format!(
            "F1 25 closed itself as it started, before it set up its graphics. {addon} is on in your profile and \
             carries its own {files}, the file that holds F1 25's packed game data, and F1 25 can't start with \
             that file replaced.{made_for}"
        ),
        steps,
    })
}
