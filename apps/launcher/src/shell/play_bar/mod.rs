pub(crate) mod save_area;
pub(crate) mod unsupported_build;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf};

use crate::services::game::{self};

pub(crate) const HEIGHT: f32 = 72.;
use crate::shell::{PlayState, Shell};
use crate::ui::theme::Palette;
use swofty_core::install::Source;

mod button;
mod game_info;
mod status;

pub(crate) fn store_badge(source: Source, p: Palette) -> Div {
    let (text, color) = match source {
        Source::Steam => ("STEAM", p.accent),
        Source::EaApp => ("EA APP", p.warning),
        Source::Unknown => (tr("FOLDER"), p.text_muted),
    };
    div()
        .flex_shrink_0()
        .px(px(5.))
        .rounded(px(4.))
        .bg(color.opacity(0.14))
        .text_size(px(10.))
        .line_height(px(16.))
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(text)
}

pub(crate) fn link(id: &'static str, label: &'static str, p: Palette) -> Stateful<Div> {
    div()
        .id(id)
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(p.text_muted)
        .underline()
        .hover(move |s| s.text_color(p.text))
        .child(label)
}

fn status_row() -> Div {
    h_flex()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .gap(px(9.))
        .text_size(px(13.))
}

fn launch_failed(row: Div, text: impl std::fmt::Display, p: Palette) -> Div {
    let full = text.to_string();
    let text = SharedString::from(trf!("Launch failed: {cause}", cause = game::cause_first(&full)));
    let tip = SharedString::from(trf!("Launch failed: {cause}", cause = full));
    row.text_color(p.error)
        .child(Icon::new(IconName::CircleAlert).size(px(16.)))
        .child(
            div()
                .id("launch-failed")
                .min_w_0()
                .truncate()
                .child(text)
                .tooltip(move |window, cx| {
                    let tip = tip.clone();
                    Tooltip::element(move |_, _| div().max_w(px(480.)).child(tip.clone())).build(window, cx)
                }),
        )
}

impl Shell {
    fn footer_links(&self, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let tip = crate::dialogs::send_logs::label();
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(18.))
            .child(
                h_flex()
                    .id("send-logs")
                    .gap(px(6.))
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .hover(move |s| s.text_color(p.text))
                    .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                    .child(Icon::new(crate::dialogs::send_logs::icon()).size(px(13.)))
                    .child(crate::dialogs::send_logs::short_label())
                    .on_click(cx.listener(|this, _, window, cx| this.send_logs(window, cx))),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(concat!("v", env!("CARGO_PKG_VERSION"))),
            )
    }

    pub(crate) fn play_bar(&self, p: Palette, wide: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.play_state();
        // With a room open on the Online page and the game ready, the room's button replaces Play.
        let ready = matches!(state, PlayState::Ready | PlayState::Crashed(_));
        let room = self.room_button(&state, ready, p, cx);
        let lobby_only = ready && room.is_some();
        h_flex()
            .h(px(HEIGHT))
            .flex_shrink_0()
            .bg(p.chrome)
            .border_t_1()
            .border_color(p.border_variant)
            .child(crate::ui::game_art(56., 56.).m(px(8.)))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .pl(px(16.))
                    .pr(px(16.))
                    .gap(px(20.))
                    .child(self.game_info(p, wide, cx))
                    .child(self.status(&state, p, cx))
                    .children(self.save_area(&state, p))
                    .children(room)
                    .child(self.footer_links(p, cx))
                    .when(!lobby_only, |d| d.child(self.big_button(&state, p, cx))),
            )
    }
}
