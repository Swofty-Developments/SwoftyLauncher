use std::collections::HashSet;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::*;

use super::model::shows_ad;
use crate::features::ads::Unit;
use crate::shell::Shell;
use crate::ui::label;
use crate::ui::theme::Palette;
use crate::ui::tokens::space::GUTTER;
use swofty_i18n::tr;

impl Shell {
    pub(crate) fn multiplayer_page(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let in_room = self.rooms.in_room();
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

    /// The page's ad banner, while ads are on and it fits.
    pub(super) fn page_ad(&self, form_open: bool, p: Palette) -> Option<AnyElement> {
        if !shows_ad(form_open) {
            return None;
        }
        let width = self.layout.content_width - 2. * GUTTER;
        self.ad_banner(Unit::Online, width, p).or_else(|| {
            let ad = self
                .ad_stand_in(p)
                .filter(|_| Self::ad_fits(Unit::Online, width))?;
            Some(h_flex().w_full().justify_center().child(ad).into_any_element())
        })
    }

    /// A box the size of the page's ad, where it goes, while a debug build previews ads that are off.
    fn ad_stand_in(&self, p: Palette) -> Option<AnyElement> {
        let size = Unit::Online.size();
        self.rooms.ad_stand_in.then(|| {
            v_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(p.text_placeholder)
                        .child(tr("Advertisement")),
                )
                .child(
                    div()
                        .w(size.width)
                        .h(size.height)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .border_1()
                        .border_color(p.border)
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child(format!(
                            "{} \u{00d7} {}",
                            f32::from(size.width),
                            f32::from(size.height)
                        )),
                )
                .into_any_element()
        })
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
