use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::timetrial::boards::{PendingLap, format_time};
use swofty_i18n::tr;

use super::state::{Load, Verdict, View};
use crate::pages::GUTTER;
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use crate::ui::{EmptyState, selectable_row};

pub(super) fn flag(p: Palette) -> Div {
    div()
        .flex_shrink_0()
        .child(Icon::new(IconName::CircleX).size(px(16.)).text_color(p.error))
}

impl Shell {
    pub(super) fn tt_tabs(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let waiting = match &self.tt.pending {
            Load::Ready(l) => l.len(),
            _ => 0,
        };
        let tab = |id: &'static str, label: String, view: View| {
            let on = self.tt.view == view;
            Button::new(id)
                .small()
                .when_else(on, |b| b.primary(), |b| b.ghost())
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tt.view = view;
                    this.tt.open_lap = None;
                    this.tt.banning = false;
                    this.tt.notice = None;
                    cx.notify();
                }))
        };
        let _ = p;
        h_flex()
            .gap(px(6.))
            .px(px(GUTTER))
            .pb(px(10.))
            .child(tab("tt-tab-boards", tr("Boards").to_owned(), View::Boards))
            .child(tab(
                "tt-tab-pending",
                format!("{} ({waiting})", tr("Pending review")),
                View::Pending,
            ))
            .into_any_element()
    }

    pub(super) fn tt_pending(
        &mut self,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let laps: Vec<PendingLap> = match &self.tt.pending {
            Load::Ready(l) => l.clone(),
            Load::Failed(e) => {
                return div()
                    .px(px(GUTTER))
                    .text_size(px(13.))
                    .text_color(p.error)
                    .child(e.clone())
                    .into_any_element();
            }
            _ => {
                return div()
                    .px(px(GUTTER))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(tr("Loading pending laps…"))
                    .into_any_element();
            }
        };
        if laps.is_empty() {
            return EmptyState::new(IconName::ShieldCheck, tr("No laps are waiting for review"))
                .into_any_element();
        }
        let mut rows = Vec::new();
        for (i, lap) in laps.iter().enumerate() {
            let id = lap.lap_id.clone();
            let head = selectable_row(
                SharedString::from(format!("tt-pend-{id}")),
                self.tt.open_lap.as_deref() == Some(&id),
                p,
            )
            .cursor_pointer()
            .child(flag(p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(14.))
                    .text_color(p.text)
                    .child(lap.driver.name.clone()),
            )
            .child(
                div()
                    .w(px(220.))
                    .flex_shrink_0()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(match lap.track_name.is_empty() {
                        true => lap.track.clone(),
                        false => lap.track_name.clone(),
                    }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(14.))
                    .child(format_time(lap.time_ms)),
            )
            .on_click({
                let id = id.clone();
                cx.listener(move |this, _, _, cx| this.tt_select_lap(&id, cx))
            });
            let open = self.tt.open_lap.as_deref() == Some(&id);
            let panel = open.then(|| self.tt_actions(&id, &lap.reasons, p, window, cx));
            let _ = i;
            rows.push(v_flex().child(head).children(panel).into_any_element());
        }
        div()
            .flex_1()
            .min_h_0()
            .scroll_y("tt-pending")
            .px(px(GUTTER))
            .pb(px(16.))
            .child(v_flex().gap(px(2.)).children(rows))
            .into_any_element()
    }

    pub(super) fn tt_actions(
        &mut self,
        lap: &str,
        reasons: &[String],
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.tt.acting;
        let reason = self.tt_reason(window, cx);
        let typed = reason.read(cx).value().trim().to_owned();
        let id = lap.to_owned();
        let mut panel = v_flex().gap(px(8.)).px(px(12.)).pb(px(10.)).pt(px(4.));
        if !reasons.is_empty() {
            panel = panel.child(div().text_size(px(12.)).text_color(p.text_muted).child(format!(
                "{}: {}",
                tr("Flagged"),
                reasons.join(", ")
            )));
        }
        let url = format!("{}/lap/{}", swofty_core::timetrial::boards_url(), lap);
        let buttons = if self.tt.banning {
            let (ban, back) = (id.clone(), reason.clone());
            h_flex()
                .gap(px(8.))
                .items_center()
                .child(div().w(px(280.)).child(Input::new(&reason).small()))
                .child(
                    Button::new("tt-ban-confirm")
                        .small()
                        .danger()
                        .label(tr("Confirm ban"))
                        .disabled(busy || typed.is_empty())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let text = back.read(cx).value().trim().to_owned();
                            back.update(cx, |s, cx| s.set_value("", window, cx));
                            this.tt_moderate(ban.clone(), Verdict::Ban, text, cx);
                        })),
                )
                .child(
                    Button::new("tt-ban-cancel")
                        .small()
                        .ghost()
                        .label(tr("Cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tt.banning = false;
                            cx.notify();
                        })),
                )
        } else {
            let (accept, deny) = (id.clone(), id.clone());
            h_flex()
                .gap(px(8.))
                .child(
                    Button::new("tt-watch")
                        .small()
                        .outline()
                        .icon(IconName::Play)
                        .label(tr("Watch Replay"))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
                .child(
                    Button::new("tt-accept")
                        .small()
                        .primary()
                        .label(tr("Accept"))
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tt_moderate(accept.clone(), Verdict::Accept, String::new(), cx)
                        })),
                )
                .child(
                    Button::new("tt-deny")
                        .small()
                        .outline()
                        .label(tr("Deny"))
                        .disabled(busy)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.tt_moderate(deny.clone(), Verdict::Deny, String::new(), cx)
                        })),
                )
                .child(
                    Button::new("tt-ban")
                        .small()
                        .danger()
                        .label(tr("Ban driver"))
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.tt.banning = true;
                            cx.notify();
                        })),
                )
        };
        panel = panel.child(buttons);
        if let Some(note) = &self.tt.notice {
            panel = panel.child(div().text_size(px(12.)).text_color(p.error).child(note.clone()));
        }
        panel.into_any_element()
    }
}
