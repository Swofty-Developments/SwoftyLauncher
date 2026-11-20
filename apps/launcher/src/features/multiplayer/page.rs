use std::collections::HashSet;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::*;

use super::model::{RoomFocus, shows_ad};

/// The drivers of the global ranking the Multiplayer page's rail lists above the player's own place.
const RAIL_RANKS: usize = 5;
use crate::features::ads::Unit;
use crate::features::online::{Online, RankFit};
use crate::shell::Shell;
use crate::shell::tabs::{OnlineTab, rooms_label};
use crate::ui::label;
use crate::ui::theme::Palette;
use swofty_i18n::tr;

impl Shell {
    /// The room page, while the Multiplayer page shows a room rather than the list of rooms.
    pub(crate) fn room_page(&mut self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        match self.rooms.focus() {
            RoomFocus::InRoom => {
                let state = self.rooms.state.clone()?;
                Some(self.room(&state, p, cx))
            }
            RoomFocus::Viewing(_) => {
                let room = self.room_viewed()?.clone();
                Some(self.room_view(&room, p, cx))
            }
            RoomFocus::List => None,
        }
    }

    /// The room the Multiplayer page's trail names while it shows the room page.
    pub(crate) fn room_crumb(&self) -> Option<String> {
        match self.rooms.focus() {
            RoomFocus::InRoom => Some(
                self.rooms
                    .state
                    .as_ref()
                    .and_then(|s| s.room.as_ref())
                    .map_or_else(|| tr("Room").to_owned(), |r| r.name.clone()),
            ),
            RoomFocus::Viewing(_) => self.room_viewed().map(|r| r.name.clone()),
            RoomFocus::List => None,
        }
    }

    /// The Multiplayer page: the room page while it shows a room, else the open rooms with the ad under them, and
    /// the Time Trial card above a short global ranking in the rail.
    pub(crate) fn multiplayer_page(
        &mut self,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.online_prepare(window, cx);
        let crumbs = self.crumb_bar(OnlineTab::Multiplayer, self.room_crumb(), p, cx);
        let body = match self.room_page(p, cx) {
            Some(room) => room,
            None => {
                let width = self.online_main_width();
                let column = self.room_column(width, p, cx);
                let ad = self.page_ad(self.rooms.form_open(), width, p);
                let card = self.trial_card(p, cx);
                Online::rank_ensure(self, "all", cx);
                let ranking = self.rank_compact("all", tr("GLOBAL RANKING"), RAIL_RANKS, p, cx);
                let main = v_flex().gap(px(20.)).child(column).children(ad);
                self.online_frame("online-rooms", main, Some(card), ranking, RankFit::Own)
            }
        };
        v_flex()
            .size_full()
            .child(crumbs)
            .child(v_flex().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// The Multiplayer tab's name with the player's room or how many rooms are open.
    pub(crate) fn rooms_tab_label(&self) -> String {
        let lobby = &self.rooms.lobby;
        rooms_label(
            self.rooms.in_room(),
            lobby.rooms_loaded.then_some(lobby.rooms.len()),
        )
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

    /// The page's ad banner, `width` wide, while ads are on and it fits.
    pub(crate) fn page_ad(&self, form_open: bool, width: f32, p: Palette) -> Option<AnyElement> {
        shows_ad(form_open)
            .then(|| self.ad_banner(Unit::Online, width, p))
            .flatten()
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
