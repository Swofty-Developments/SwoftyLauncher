//! persistent.ui: the multichoice message gate.

use swofty_formats::pkg::Node;

use super::ONCE_ATTR;
use super::home::{event_mut, popup_seen, sig_to, signal};
use crate::Result;
use crate::ui::node::*;

pub(super) const UIDM: &str = "persistent.consume_input_wrapper.persistent_multichoice_message";

pub(super) fn multichoice(p: &mut swofty_formats::pkg::Pkg) -> Result<&mut Node> {
    let scr = first_screen(p)?;
    if scr.name() != "persistent" {
        return Err(err("the persistent screen file does not hold persistent"));
    }
    scr.child_mut("consume_input_wrapper")?
        .child_mut("persistent_multichoice_message")
}

pub(super) fn relabel(sigs: &[Node], from: &str) -> Result<Vec<Node>> {
    let mut out = sigs.to_vec();
    for s in &mut out {
        set_str(s.o_mut()?, "from_pin_name", from)?;
    }
    Ok(out)
}

/// The screen file with the gate; a file that has it already, as the HUD composer leaves it, is
/// returned as it is.
pub fn persistent_gate(data: &[u8]) -> Result<Vec<u8>> {
    let mut p = swofty_formats::pkg::Pkg::parse(data)?;
    if multichoice(&mut p)?.child("swofty_once_check").is_ok() {
        return Ok(data.to_vec());
    }
    let seen = popup_seen();
    edit(data, |p| {
        let m = multichoice(p)?;
        let act = event_mut(m, "activate_message")?;
        let names: Vec<(String, String)> = act
            .iter()
            .map(|s| {
                let (a, b) = sig_to(s);
                (a.to_owned(), b.to_owned())
            })
            .collect();
        let want = [
            (
                "persistent.consume_input_wrapper".to_owned(),
                "consumeStart".to_owned(),
            ),
            (UIDM.to_owned(), "show".to_owned()),
        ];
        if names != want {
            return Err(err(format!("activate_message changed: {names:?}")));
        }
        let show = act.clone();
        *act = vec![sig(
            &format!("{UIDM}.swofty_once_check"),
            "is_valid",
            "activate_message",
        )];
        let hidden_props = || {
            vec![
                ("enabled", prop(boolv(false), None)),
                ("visible", prop(boolv(false), None)),
            ]
        };
        let mut once = hidden_props();
        once.push((
            "dbEntry",
            prop(s(""), Some(&format!("user_attributes.{ONCE_ATTR}"))),
        ));
        let mut check = hidden_props();
        check.push(("value", prop(boolv(true), None)));
        check.push(("dbEntry", prop(s(&seen), None)));
        let mut mark = hidden_props();
        mark.push(("source", prop(boolv(true), None)));
        mark.push(("target", prop(boolv(false), Some(&seen))));
        let mut no_seen = vec![sig(&format!("{UIDM}.swofty_mark_seen"), "trigger", "no")];
        no_seen.extend(relabel(&show, "no")?);
        let kids = vec![
            node(
                "swofty_once_check",
                "DBVV",
                once,
                vec![
                    event(
                        "yes",
                        vec![sig(&format!("{UIDM}.swofty_seen_check"), "query", "yes")],
                    ),
                    event("no", relabel(&show, "no")?),
                ],
                &[],
                &[],
            ),
            node(
                "swofty_seen_check",
                "SVCD",
                check,
                vec![
                    event("no", no_seen),
                    event(
                        "yes",
                        vec![
                            sig(UIDM, "deactivate_message", "yes"),
                            sig(&format!("{UIDM}.disable_input"), "trigger", "yes"),
                            signal(
                                &format!("{UIDM}.user_response_setter"),
                                "setValue",
                                "yes",
                                &[("value", "cancel")],
                            ),
                        ],
                    ),
                ],
                &[],
                &[],
            ),
            node("swofty_mark_seen", "UIDS", mark, vec![], &[], &[]),
        ];
        m.slot_mut(CHILDREN)?.extend(kids);
        fix_ids(first_screen(p)?, None);
        Ok(())
    })
}
