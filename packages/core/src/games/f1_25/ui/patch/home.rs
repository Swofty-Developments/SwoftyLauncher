//! fe_home.ui: the offline panel's launcher text and prompt, and the one-time popup over the home screen.

use swofty_formats::pkg::{Node, Val};

use super::{ACTION, MOD_MENU, MOD_MENU_OFFLINE, OFFLINE_PROMPT, POPUP_BODY, POPUP_TITLE};
use crate::Result;
use crate::ui::node::*;
use crate::ui::pages::STATE;

pub(super) const PANEL: &str = "fe_home.contextual_panels.offline_contextual_panel";

pub const OFFLINE_TEXT_PATH: &str = "#ui_menu_data.swofty_offline_text";

pub const OFFLINE_TEXT: &str = "Launched with Swofty Launcher";

pub fn popup_seen() -> String {
    format!("{STATE}.online_popup_seen")
}

pub struct Popup {
    pub title: String,
    pub body: String,
}

impl Popup {
    pub fn for_version(version: &str) -> Self {
        Self {
            title: format!("Swofty Launcher v{version}"),
            body: format!(
                "Online services are off while mods are loaded, so multiplayer is unavailable.\n\n\
                 New in v{version}:\n\
                 - Shorter loading screens\n\
                 - Time Trial sector times from the first flying lap\n\
                 - New standard libraries for addon makers"
            ),
        }
    }

    pub fn strings(&self) -> Vec<(String, String)> {
        vec![
            (POPUP_TITLE.into(), self.title.clone()),
            (POPUP_BODY.into(), self.body.clone()),
        ]
    }
}

pub(super) fn signal(to: &str, pin: &str, from: &str, attrs: &[(&str, &str)]) -> Node {
    let attrs: Vec<(&str, Attr)> = attrs.iter().map(|(k, v)| (*k, Attr::from(*v))).collect();
    sig_with(to, pin, from, &attrs)
}

pub(super) fn sig_to(s: &Node) -> (&str, &str) {
    let o = s.as_obj();
    let get = |k| {
        o.and_then(|o| o.get(k))
            .and_then(Node::as_val)
            .and_then(Val::as_str)
            .unwrap_or_default()
    };
    (get("to_object_name"), get("to_pin_name"))
}

pub(super) fn event_mut<'n>(n: &'n mut Node, name: &str) -> Result<&'n mut Vec<Node>> {
    let what = n.name().to_owned();
    n.slot_mut(EVENTS)?
        .iter_mut()
        .find(|e| e.o().ok().and_then(|o| o.str_at("eventName").ok()) == Some(name))
        .ok_or_else(|| err(format!("{what} has no {name} event")))?
        .o_mut()?
        .at_mut("signals")?
        .a_mut()
}

pub fn fe_home(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let screen = first_screen(p)?;
        if screen.name() != "fe_home" {
            return Err(err("the home screen file does not hold fe_home"));
        }
        let tooltips = screen.child_mut("tooltips")?;
        let mut tip = clone_node(tooltips.child("settings")?, "fe_home.tooltips", MOD_MENU)?;
        set_prop_value(&mut tip, "tooltipsName", Val::str(MOD_MENU))?;
        set_prop_value(tip.child_mut("text")?, "localise", Val::bool(false))?;
        tooltips.slot_mut(CHILDREN)?.push(tip);
        let mut listener = clone_node(screen.child("settings")?, "fe_home", "mod_menu_listener")?;
        set_prop_value(&mut listener, "action_name", Val::str(ACTION))?;
        let ev = listener
            .slot_mut(EVENTS)?
            .first_mut()
            .ok_or_else(|| err("the settings listener has no events"))?
            .o_mut()?;
        if ev.str_at("eventName")? != "pressed" {
            return Err(err("the settings listener does not start with pressed"));
        }
        *ev.at_mut("signals")?.a_mut()? = vec![
            signal("fe_home.Action", "route", "pressed", &[("key", MOD_MENU)]),
            signal("fe_home.audio_theme_trigger", "confirm", "pressed", &[]),
        ];
        fix_ids(&mut listener, Some("fe_home"));
        screen.slot_mut(CHILDREN)?.push(listener);
        Ok(())
    })
}

pub fn offline_panel(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let screen = first_screen(p)?;
        let cp = screen.child_mut("contextual_panels")?;
        let panel = cp.child_mut("offline_contextual_panel")?;
        let subtitle = panel.child_mut("subtitle")?;
        literal_text(subtitle, OFFLINE_TEXT)?;
        set_prop(subtitle, "text", prop(s(OFFLINE_TEXT), Some(OFFLINE_TEXT_PATH)))?;
        literal_text(panel.child_mut("tooltip_label")?, OFFLINE_PROMPT)?;
        set_prop_value(
            panel.child_mut("tooltip_extra_2")?,
            "tooltipsName",
            Val::str(MOD_MENU_OFFLINE),
        )?;
        let decision = cp.child_mut("contextual_decision")?;
        event_mut(decision, "offline")?
            .retain(|s| sig_to(s).0 != "fe_home.contextual_panels.extra_2_listener");
        event_mut(decision, "default")?.push(signal(PANEL, "show", "default", &[]));
        fix_ids(screen, None);
        Ok(())
    })
}

pub fn hide_panel(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let screen = first_screen(p)?;
        let dec = screen
            .child_mut("contextual_panels")?
            .child_mut("contextual_decision")?;
        for e in dec.slot_mut(EVENTS)? {
            e.o_mut()?.at_mut("signals")?.a_mut()?.retain(|s| {
                let (to, pin) = sig_to(s);
                !(to.ends_with("offline_contextual_panel") && pin == "show")
            });
        }
        Ok(())
    })
}
