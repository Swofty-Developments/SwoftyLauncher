use crate::ui::avatar;
use crate::ui::scale::{Hit, fixed, px};
use crate::ui::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::tr;

use super::{BAR_HEIGHT, Popover};
use crate::shell::Shell;

impl Shell {
    pub(in crate::shell) fn popover_layer(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let which = self.navigation.popover?;
        let panel = match which {
            Popover::Notices => self.notices_panel(p, cx),
            Popover::Account => self.account_panel(p, cx),
        };
        Some(
            div()
                .id("popover-layer")
                .absolute()
                .inset_0()
                .occlude()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.navigation.popover = None;
                    cx.notify();
                }))
                .child(div().absolute().top(fixed(BAR_HEIGHT)).right(px(8.)).child(panel))
                .into_any_element(),
        )
    }

    pub(super) fn panel(&self, p: Palette) -> Stateful<Div> {
        v_flex()
            .id("popover-panel")
            .w(px(340.))
            .rounded(px(8.))
            .border_1()
            .border_color(p.border)
            .bg(p.elevated)
            .overflow_hidden()
            .on_click(|_, _, cx| cx.stop_propagation())
    }

    pub(super) fn account_panel(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let name = self.account.display_name().unwrap_or_default();
        self.panel(p)
            .w(px(220.))
            .child(
                h_flex()
                    .gap(px(10.))
                    .p(px(12.))
                    .items_center()
                    .child(avatar(&name, 28., p).rounded_full())
                    .child(div().truncate().font_weight(FontWeight::MEDIUM).child(name)),
            )
            .child(
                Button::new("mp-logout")
                    .ghost()
                    .icon(IconName::LogOut)
                    .label(tr("Log out"))
                    .disabled(self.account.busy || self.game_busy())
                    .tooltip(tr("Close the game to sign in or out"))
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.navigation.popover = None;
                        crate::shell::account::Account::sign_out(this, cx);
                    })),
            )
            .into_any_element()
    }

    pub(super) fn notices_panel(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let status = self.notices();
        let history: Vec<crate::shell::notices::Entry> =
            cx.global::<crate::shell::notices::Notices>().entries().to_vec();
        let empty = status.is_empty() && history.is_empty();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (i, n) in status.into_iter().enumerate() {
            let page = n.page;
            rows.push(
                h_flex()
                    .id(SharedString::from(format!("notice-{i}")))
                    .gap(px(10.))
                    .p(px(12.))
                    .items_start()
                    .border_b_1()
                    .border_color(p.border_variant)
                    .when(page.is_some(), |d| {
                        d.cursor_pointer().hover(move |s| s.bg(p.hover))
                    })
                    .child(
                        Icon::new(n.icon)
                            .size(px(16.))
                            .text_color(p.accent)
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .child(n.text),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.navigation.popover = None;
                        if let Some(page) = page {
                            this.open_page(page, cx);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        for e in &history {
            rows.push(self.history_row(e, p));
        }
        let body: Vec<AnyElement> = if empty {
            vec![
                v_flex()
                    .items_center()
                    .gap(px(8.))
                    .py(px(26.))
                    .child(
                        Icon::new(IconName::Bell)
                            .size(px(22.))
                            .text_color(p.text_placeholder),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text_muted)
                            .child(tr("No notifications")),
                    )
                    .into_any_element(),
            ]
        } else {
            rows
        };
        self.panel(p)
            .child(
                h_flex()
                    .px(px(12.))
                    .py(px(6.))
                    .items_center()
                    .border_b_1()
                    .border_color(p.border_variant)
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text_muted)
                            .child(tr("NOTIFICATIONS")),
                    )
                    .when(!history.is_empty(), |d| {
                        d.child(
                            Button::new("clear-notices")
                                .ghost()
                                .tap_sm()
                                .label(tr("Clear all"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.update_global(|n: &mut crate::shell::notices::Notices, _| n.clear());
                                    this.navigation.popover = None;
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(v_flex().max_h(px(480.)).overflow_y_hidden().children(body))
            .into_any_element()
    }

    pub(super) fn history_row(&self, e: &crate::shell::notices::Entry, p: Palette) -> AnyElement {
        use crate::shell::notices::Kind;
        let (icon, tone) = match e.note.kind {
            Kind::Info => (IconName::Info, p.accent),
            Kind::Success => (IconName::CircleCheck, p.success),
            Kind::Warning => (IconName::TriangleAlert, p.warning),
            Kind::Error => (IconName::CircleX, p.error),
        };
        let action = e.note.action.clone();
        let time = div()
            .text_size(px(11.))
            .text_color(p.text_placeholder)
            .flex_shrink_0()
            .child(crate::ui::format::ago(e.at.elapsed().as_secs_f64()));
        let (head, body) = match e.note.title.clone() {
            Some(title) => (title, Some(e.note.message.clone())),
            None => (e.note.message.clone(), None),
        };
        h_flex()
            .id(SharedString::from(format!("history-{}", e.id)))
            .gap(px(12.))
            .px(px(14.))
            .py(px(12.))
            .items_start()
            .border_b_1()
            .border_color(p.border_variant)
            .child(
                div()
                    .pt(px(1.))
                    .child(Icon::new(icon).size(px(18.)).text_color(tone).flex_shrink_0()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.))
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .items_start()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .line_height(px(18.))
                                    .font_weight(if body.is_some() {
                                        FontWeight::SEMIBOLD
                                    } else {
                                        FontWeight::NORMAL
                                    })
                                    .child(head),
                            )
                            .child(time),
                    )
                    .children(body.map(|m| {
                        div()
                            .text_size(px(12.))
                            .line_height(px(17.))
                            .text_color(p.text_muted)
                            .child(m)
                    }))
                    .children(action.map(|(label, run)| {
                        h_flex().pt(px(6.)).child(
                            Button::new(SharedString::from(format!("history-action-{}", e.id)))
                                .primary()
                                .tap_sm()
                                .label(label)
                                .on_click(move |_, window, cx| run(window, cx)),
                        )
                    })),
            )
            .into_any_element()
    }

    pub(in crate::shell) fn popup_layer(&mut self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.navigation.popover == Some(Popover::Notices) {
            return None;
        }
        let now = std::time::Instant::now();
        let popups: Vec<crate::shell::notices::Entry> = cx
            .global::<crate::shell::notices::Notices>()
            .popups(now)
            .into_iter()
            .cloned()
            .collect();
        if popups.is_empty() {
            return None;
        }
        if !self.popup_timer {
            self.popup_timer = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1200))
                    .await;
                this.update(cx, |s, cx| {
                    s.popup_timer = false;
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        Some(
            v_flex()
                .id("popups")
                .absolute()
                .top(fixed(
                    BAR_HEIGHT
                        + 6.
                        + if self.dock.notch_beside {
                            crate::shell::download_dock::NOTCH_HEIGHT + 6.
                        } else {
                            0.
                        },
                ))
                .right(px(8.))
                .w(px(340.))
                .gap(px(6.))
                .children(popups.iter().map(|e| {
                    let id = e.id;
                    div()
                        .id(SharedString::from(format!("popup-{id}")))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.elevated)
                        .overflow_hidden()
                        .cursor_pointer()
                        .occlude()
                        .child(self.history_row(e, p))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.update_global(|n: &mut crate::shell::notices::Notices, _| n.dismiss_popup(id));
                            cx.notify();
                        }))
                }))
                .into_any_element(),
        )
    }
}
