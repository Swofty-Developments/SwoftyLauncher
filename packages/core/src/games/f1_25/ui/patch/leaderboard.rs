//! leaderboard_container.ui: the leaderboard tabs without their shoulder-button glyphs.

use super::f1_world::named_mut;
use crate::Result;
use crate::ui::node::*;

pub const FRIENDS_TAB_LABEL: &str = "lng_friends_leaderboard";

pub const FRIENDS_TAB_TEXT: &str = " ";

/// leaderboard_container.ui without its two shoulder-button glyphs, and with the tab controller's input off so the
/// bumpers (and the stick) no longer switch tabs. The tab buttons themselves are built by the game into the shared nav
/// scene from the screen model's view type list, which no data edit reaches.
pub fn leaderboard_glyphs(data: &[u8]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let screen = first_screen(p)?;
        for name in ["tooltip_tab_left", "tooltip_tab_right"] {
            let n = named_mut(screen, name)
                .ok_or_else(|| err(format!("leaderboard_container has no {name} node")))?;
            if n.type_code() != "UITL" {
                return Err(err(format!("{name} is not a tooltip element")));
            }
            set_bool(n, "visible", false)?;
        }
        let tc = named_mut(screen, "tab_controller")
            .ok_or_else(|| err("leaderboard_container has no tab_controller node"))?;
        if tc.type_code() != "UITC" {
            return Err(err("tab_controller is not a tab controller"));
        }
        set_bool(tc, "inputAllowed", false)?;
        Ok(())
    })
}
