use std::collections::HashSet;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::*;

use crate::shell::Shell;
use crate::ui::label;
use crate::ui::theme::Palette;

impl Shell {
    pub(crate) fn multiplayer_page(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let in_room = self.rooms.session.is_some();
        match self.rooms.state.clone() {
            Some(state) if in_room => self.room(&state, p, cx),
            _ => match self.room_viewed().cloned() {
                Some(room) => self.room_view(&room, p, cx),
                None => self.room_lobby(p, cx),
            },
        }
    }

    pub(super) fn game_status(&self, p: Palette) -> Div {
        h_flex()
            .gap(px(6.))
            .flex_shrink_0()
            .text_size(px(13.))
            .text_color(p.text_muted)
            .child(
                Icon::new(IconName::Gamepad2)
                    .size(px(15.))
                    .text_color(p.text_placeholder),
            )
            .child(self.room_game_state().text())
    }

    pub(super) fn section_head(&self, title: impl Into<SharedString>, p: Palette) -> Div {
        h_flex()
            .h(px(28.))
            .gap(px(10.))
            .items_center()
            .child(label(title, p))
    }

    pub(super) fn room_card(&self, p: Palette) -> Div {
        v_flex()
            .p(px(16.))
            .gap(px(12.))
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
    }

    pub(super) fn room_enabled_addons(&self) -> HashSet<String> {
        self.library
            .profile
            .addons
            .iter()
            .filter(|a| a.enabled)
            .map(|a| a.id.clone())
            .collect()
    }
}
