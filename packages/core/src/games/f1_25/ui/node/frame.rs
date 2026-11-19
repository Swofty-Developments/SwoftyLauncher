//! A screen's frame copied from a stock screen: its action, audio, tooltips, back and watchdog nodes,
//! finished around the screen's own children, with focus chained from row to row.

use swofty_formats::pkg::Node;

use super::build::node;
use super::ext::NodeExt;
use super::ids::{fix_ids, rename, rename_refs};
use super::signals::{Attr, add_events, event, sig, sig_with};
use super::{CHILDREN, EVENTS};
use crate::Result;

pub struct Frame {
    pub action: Node,
    pub audio: Node,
    pub tooltips: Node,
    pub tooltips_old: String,
    pub back: Node,
    pub watchdog: Node,
    pub seconds: f32,
}

pub struct FrameSources<'a> {
    pub action: &'a Node,
    pub audio: &'a Node,
    pub back: &'a Node,
    pub tooltips: &'a Node,
}

pub fn frame(sname: &str, src: &FrameSources, tooltip_names: &[&str]) -> Result<Frame> {
    let back_key = "back";
    let mut action = src.action.clone();
    action.slot_mut(EVENTS)?.clear();
    let mut tooltips = src.tooltips.clone();
    tooltips
        .slot_mut(CHILDREN)?
        .retain(|c| c.is_node() && tooltip_names.contains(&c.name()));
    let mut back = src.back.clone();
    rename(&mut back, "back_listener")?;
    *back.slot_mut(EVENTS)? = vec![event(
        "pressed",
        vec![
            sig(&format!("{sname}.audio_theme_trigger"), "back", "pressed"),
            sig_with(
                &format!("{sname}.Action"),
                "route",
                "pressed",
                &[("key", back_key.into())],
            ),
        ],
    )];
    let watchdog = node(
        "watchdog",
        "TIME",
        vec![],
        vec![event(
            "time_out_expired",
            vec![sig_with(
                &format!("{sname}.Action"),
                "route",
                "time_out_expired",
                &[("key", back_key.into())],
            )],
        )],
        &[],
        &[],
    );
    Ok(Frame {
        action,
        audio: src.audio.clone(),
        tooltips,
        tooltips_old: "fe_career.tooltips".into(),
        back,
        watchdog,
        seconds: 10.0,
    })
}

pub fn finish(mut scr: Node, fr: Frame, mut kids: Vec<Node>, first_focus: &str) -> Result<Node> {
    let sname = scr.name().to_owned();
    let watchdog = format!("{sname}.watchdog");
    let tips = format!("{sname}.tooltips");
    let focus = format!("{sname}.{first_focus}");
    let reset = sig_with(
        &watchdog,
        "reset",
        "opened_screen",
        &[("duration", Attr::F32(fr.seconds))],
    );
    let Frame {
        mut action,
        audio,
        mut tooltips,
        tooltips_old,
        back,
        watchdog: wd,
        ..
    } = fr;
    *action.slot_mut(EVENTS)? = vec![
        event(
            "opened_screen",
            vec![reset, sig(&focus, "focus", "opened_screen")],
        ),
        event(
            "finishedTranIn",
            vec![
                sig(&watchdog, "stop", "finishedTranIn"),
                sig(&tips, "ttActivate", "finishedTranIn"),
                sig(&focus, "focus", "finishedTranIn"),
            ],
        ),
        event(
            "finishedTranOut",
            vec![
                sig(&watchdog, "stop", "finishedTranOut"),
                sig(&tips, "ttDeactive", "finishedTranOut"),
            ],
        ),
    ];
    rename(&mut tooltips, "tooltips")?;
    rename_refs(&mut tooltips, &tooltips_old, &tips);
    for k in kids.iter_mut().filter(|k| k.is_node()) {
        let kname = k.name().to_owned();
        let first = match first_focus.strip_prefix(&format!("{kname}.")) {
            Some(rest) => k.find_mut(rest).ok(),
            None if kname == first_focus => Some(k),
            None => None,
        };
        if let Some(first) = first {
            add_events(
                first,
                vec![event("focused", vec![sig(&watchdog, "stop", "focused")])],
            )?;
            break;
        }
    }
    let mut children = vec![action, tooltips, audio, back];
    children.append(&mut kids);
    children.push(wd);
    *scr.slot_mut(CHILDREN)? = children;
    fix_ids(&mut scr, None);
    Ok(scr)
}

pub fn chain_focus(screen: &str, kids: &mut [Node], rows: &[usize]) -> Result<()> {
    for w in rows.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (na, nb) = (kids[a].name().to_owned(), kids[b].name().to_owned());
        add_events(
            &mut kids[a],
            vec![event(
                "navDown",
                vec![sig(&format!("{screen}.{nb}"), "focus", "navDown")],
            )],
        )?;
        add_events(
            &mut kids[b],
            vec![event(
                "navUp",
                vec![sig(&format!("{screen}.{na}"), "focus", "navUp")],
            )],
        )?;
    }
    Ok(())
}
