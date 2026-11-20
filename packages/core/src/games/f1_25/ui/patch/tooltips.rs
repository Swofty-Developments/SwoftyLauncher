//! The tooltips dictionary: the launcher's tooltip for its own settings.

use swofty_formats::pkg::Obj;

use super::{ACTION, MOD_MENU, MOD_MENU_OFFLINE, OFFLINE_PROMPT};
use crate::Result;
use crate::ui::node::*;

pub const PROMPT_LABEL: &str = "Addon Settings";

pub(super) const ACTION_PREFIX: &str = "#input_data.action_sets.menu_actions.actions.";

pub(super) fn tooltip_dict(p: &mut swofty_formats::pkg::Pkg) -> Result<&mut Obj> {
    root_obj(p)?
        .at_mut("tool_tips")?
        .o_mut()?
        .at_mut("Contents")?
        .o_mut()?
        .at_mut("tooltips_dictionary")?
        .o_mut()?
        .at_mut("Contents")?
        .o_mut()
}

pub(super) fn add_tooltip(
    data: &[u8],
    name: &str,
    src: &str,
    label: &str,
    action: Option<&str>,
    priority: Option<&str>,
) -> Result<Vec<u8>> {
    edit(data, |p| {
        let d = tooltip_dict(p)?;
        if d.get(name).is_some() {
            return Err(err(format!("the tooltips already have {name}")));
        }
        let mut e = d.at(src)?.clone();
        let eo = e.o_mut()?;
        set_str(eo, "Name", name)?;
        let c = eo.at_mut("Contents")?.o_mut()?;
        let mut set = |key: &str, v: &str| set_str(c.at_mut(key)?.o_mut()?, "Value", v);
        if let Some(action) = action {
            set("action_path", &format!("{ACTION_PREFIX}{action}"))?;
        }
        set("text", label)?;
        if let Some(priority) = priority {
            set("priority", priority)?;
        }
        d.set(name, e);
        Ok(())
    })
}

pub fn tooltips(data: &[u8], adjust: &str) -> Result<Vec<u8>> {
    let d = add_tooltip(
        data,
        MOD_MENU,
        "settings_fe_home",
        PROMPT_LABEL,
        Some(ACTION),
        Some("5"),
    )?;
    let d = add_tooltip(&d, MOD_MENU_OFFLINE, "delete", OFFLINE_PROMPT, Some(ACTION), None)?;
    add_tooltip(&d, adjust, "change_driver", "Adjust", None, None)
}
