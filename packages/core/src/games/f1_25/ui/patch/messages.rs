//! The runtime and HUD message lists: the messages the launcher shows through the game's own message screens
//! and toasts.

use swofty_formats::pkg::Node;

use super::{ONCE_ATTR, POPUP_BODY, POPUP_TITLE};
use crate::Result;
use crate::ui::node::*;

pub(super) const ONLINE_MESSAGES: &[&str] = &[
    "online_services_generic_exception_online",
    "online_services_generic_exception_online_with_code",
    "online_services_generic_exception_offline",
    "online_services_generic_exception_offline_with_code",
];

pub(super) const BUTTON: &str = "lng_continue";

pub const MESSAGE_ID: &str = "swofty_message";

pub(super) const MESSAGE_TEMPLATE: &str = "online_services_EA_not_logged_in";

pub(super) const ATTRIBUTE_ROW: &str = "#ui_messages.default_queue.multichoice_message.user_attributes";

pub(super) fn attribute_token(attribute: &str) -> Node {
    obj(vec![(
        "VALUE",
        obj(vec![
            ("localise", boolv(false)),
            ("value", s("")),
            ("name", s("VALUE")),
            ("dbEntry", s(&format!("{ATTRIBUTE_ROW}[{attribute}].Value"))),
        ]),
    )])
}

pub(super) fn attribute(name: &str) -> Node {
    obj(vec![
        ("localise", boolv(false)),
        ("value", s("")),
        ("name", s(name)),
        ("dbEntry", s("")),
    ])
}

pub(super) fn by_message_id<'n>(list: &'n [Node], id: &str) -> Option<&'n Node> {
    list.iter()
        .find(|m| m.as_obj().and_then(|o| o.str_at("id").ok()) == Some(id))
}

pub const CONFIRM_ID: &str = "swofty_confirm";

pub const CONFIRM_YES: &str = "load_anyway";

pub const CONFIRM_NO: &str = "back";

pub const CONFIRM_YES_LABEL: &str = "lng_swofty_load_anyway";

pub const CONFIRM_NO_LABEL: &str = "lng_swofty_back";

fn option(label: &str, response: &str, default: bool) -> Node {
    obj(vec![
        ("value", s(label)),
        ("response", s(response)),
        ("defaultSelection", boolv(default)),
    ])
}

fn text_message(list: &[Node], id: &str, options: Vec<Node>) -> Result<Option<Node>> {
    if by_message_id(list, id).is_some() {
        return Ok(None);
    }
    let Some(template) = by_message_id(list, MESSAGE_TEMPLATE) else {
        return Ok(None);
    };
    let mut m = template.clone();
    let o = m.o_mut()?;
    set_str(o, "id", id)?;
    set_str(o, "title", "lng_value_tag")?;
    set_str(o, "messageBody", "lng_value_tag")?;
    o.set("titleTokens", attribute_token("title"));
    o.set("bodyTokens", attribute_token("message"));
    *o.at_mut("attributes")?.a_mut()? = vec![attribute("title"), attribute("message")];
    *o.at_mut("options")?.a_mut()? = options;
    Ok(Some(m))
}

pub(super) fn runtime_message(list: &[Node]) -> Result<Option<Node>> {
    text_message(list, MESSAGE_ID, vec![option("lng_msg_ok", "ok", true)])
}

pub(super) fn confirm_message(list: &[Node]) -> Result<Option<Node>> {
    text_message(
        list,
        CONFIRM_ID,
        vec![
            option(CONFIRM_YES_LABEL, CONFIRM_YES, false),
            option(CONFIRM_NO_LABEL, CONFIRM_NO, true),
        ],
    )
}

pub fn messages(data: &[u8], text: bool) -> Result<Vec<u8>> {
    edit(data, |p| {
        let list = root_obj(p)?.at_mut("messages")?.a_mut()?;
        if let Some(added) = runtime_message(list)? {
            list.push(added);
        }
        if let Some(added) = confirm_message(list)? {
            list.push(added);
        }
        let mut done = Vec::new();
        for m in root_obj(p)?.at_mut("messages")?.a_mut()? {
            let m = m.o_mut()?;
            let id = m.str_at("id")?.to_owned();
            if !ONLINE_MESSAGES.contains(&id.as_str()) {
                continue;
            }
            if text {
                set_str(m, "title", POPUP_TITLE)?;
                set_str(m, "messageBody", POPUP_BODY)?;
                m.drop_key("titleTokens");
                m.drop_key("bodyTokens");
            }
            *m.at_mut("options")?.a_mut()? = vec![obj(vec![
                ("value", s(BUTTON)),
                ("response", s("cancel")),
                ("defaultSelection", boolv(true)),
            ])];
            m.at_mut("attributes")?.a_mut()?.push(obj(vec![
                ("localise", boolv(false)),
                ("value", s("true")),
                ("name", s(ONCE_ATTR)),
                ("dbEntry", s("")),
            ]));
            done.push(id);
        }
        done.sort();
        let mut want: Vec<&str> = ONLINE_MESSAGES.to_vec();
        want.sort();
        if done != want {
            return Err(err(format!(
                "the online services messages changed: found {done:?}"
            )));
        }
        Ok(())
    })
}

pub const TOAST_PREFIX: &str = "swofty_toast_";

pub const TOAST_SECONDS: &[u32] = &[2, 3, 4, 5, 6, 8, 10, 12, 15, 20, 30];

pub(super) const TOAST_TEMPLATE: &str = "teammate_entering_pit";

pub(super) const TOAST_FLAG: &str = "#overrides.osd.race_flags.generic";

pub(super) fn toast_attribute(name: &str, value: Node) -> Node {
    obj(vec![
        ("localise", boolv(false)),
        ("value", value),
        ("name", s(name)),
        ("dbEntry", s("")),
    ])
}

pub fn hud_messages(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let list = root_obj(p)?.at_mut("messages")?.a_mut()?;
        let Some(template) = by_message_id(list, TOAST_TEMPLATE).cloned() else {
            return Err(err(format!("the HUD messages have no {TOAST_TEMPLATE}")));
        };
        for &n in TOAST_SECONDS {
            let id = format!("{TOAST_PREFIX}{n}");
            if by_message_id(list, &id).is_some() {
                continue;
            }
            let mut m = template.clone();
            let o = m.o_mut()?;
            set_str(o, "id", &id)?;
            *o.at_mut("attributes")?.a_mut()? = vec![
                toast_attribute("message", s("")),
                toast_attribute("sub_message", s("")),
                toast_attribute("msg_duration", f32v(n as f32)),
                toast_attribute("flag", s(TOAST_FLAG)),
                toast_attribute("enabled_in_spectator", boolv(true)),
            ];
            list.push(m);
        }
        Ok(())
    })
}
