//! The rooms card at the top of the Time Trial page's rail: the first open rooms, joining by code, and the way to
//! the Multiplayer page.

use crate::features::online::{open_link, preview_card};
use crate::shell::{Page, Shell};
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Disableable, h_flex, v_flex};
use gpui_kit::*;
use swofty_core::online::multiplayer::types::RoomSummary;
use swofty_i18n::{tr, trf};

use super::model::{card_rooms, refusal_text, room_meta_players};

impl Shell {
    /// None for an account without access, which must not learn the rooms exist.
    pub(crate) fn rooms_card(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.nav_mp() {
            return None;
        }
        let lobby = &self.rooms.lobby;
        let title = if lobby.rooms_loaded {
            trf!("OPEN ROOMS \u{00b7} {count}", count = lobby.rooms.len())
        } else {
            tr("OPEN ROOMS").to_owned()
        };
        let open = open_link("mp-card-open", tr("Open Multiplayer"), p)
            .on_click(cx.listener(|this, _, _, cx| this.open_page(Page::Multiplayer, cx)));
        let body: AnyElement = if let Some(why) = &self.rooms.refused {
            card_text(refusal_text(why), p).into_any_element()
        } else if !lobby.rooms_loaded && lobby.rooms_error.is_none() {
            card_text(tr("Loading rooms\u{2026}").to_owned(), p).into_any_element()
        } else if lobby.rooms.is_empty() {
            h_flex()
                .px(px(14.))
                .py(px(12.))
                .gap(px(10.))
                .items_center()
                .child(div().flex_1().text_size(px(13.)).text_color(p.text_muted).child(
                    match &lobby.rooms_error {
                        Some(_) => tr("The room list didn't load. It tries again in a few seconds."),
                        None => tr("No open rooms"),
                    },
                ))
                .child(
                    Button::new("mp-card-create")
                        .outline()
                        .tap_sm()
                        .label(tr("Create room"))
                        .disabled(self.rooms.busy())
                        .on_click(cx.listener(|this, _, window, cx| this.room_create_from_card(window, cx))),
                )
                .into_any_element()
        } else {
            v_flex()
                .children(
                    lobby.rooms[..card_rooms(lobby.rooms.len())]
                        .iter()
                        .map(|r| self.card_room(r, p, cx)),
                )
                .into_any_element()
        };
        Some(
            preview_card(title, open, p)
                .child(body)
                .child(
                    div()
                        .px(px(14.))
                        .py(px(12.))
                        .border_t_1()
                        .border_color(p.border_variant)
                        .child(self.code_join("mp-card-join-code", true, p, cx)),
                )
                .into_any_element(),
        )
    }

    fn card_room(&self, room: &RoomSummary, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let code = room.code.clone();
        h_flex()
            .h(px(46.))
            .px(px(14.))
            .gap(px(10.))
            .items_center()
            .border_b_1()
            .border_color(p.border_variant)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(room.name.clone()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(room_meta_players(room)),
            )
            .child(
                Button::new(SharedString::from(format!("mp-card-view-{}", room.code)))
                    .outline()
                    .tap_sm()
                    .label(tr("View"))
                    .disabled(self.rooms.busy())
                    .on_click(cx.listener(move |this, _, _, cx| this.room_open_from_card(code.clone(), cx))),
            )
            .into_any_element()
    }
}

fn card_text(text: String, p: Palette) -> Div {
    div()
        .px(px(14.))
        .py(px(14.))
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(text)
}
