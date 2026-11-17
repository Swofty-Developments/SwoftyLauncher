use std::path::PathBuf;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::crash::StartFailure;
use swofty_core::known_problems::Diagnosis;
use swofty_i18n::{tr, trf};

use crate::dialogs::send_logs;
use crate::shell::Shell;
use crate::ui::Overlay;
use crate::ui::discord::invite_button;
use crate::ui::theme::Palette;

/// A crash newer than this opens the modal by itself; an older one only shows
/// in the play bar.
pub(crate) const FRESH_SECS: f64 = 15. * 60.;

/// What the crash modal shows for one crashed session, or one that ended stuck.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CrashView {
    pub dir: PathBuf,
    pub diagnosis: Option<Diagnosis>,
    pub crashed: bool,
    /// The exit code and last exception, as the play bar shows them.
    pub summary: String,
}

impl CrashView {
    /// The play bar's line under "Crashed".
    pub(crate) fn headline(&self) -> String {
        match &self.diagnosis {
            Some(d) => d.title.clone(),
            None => self.summary.clone(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Crashes {
    pub(crate) latest: Option<(PathBuf, Result<String, StartFailure>)>,
    /// The newest crash's diagnosis, and whether its modal is open.
    pub(crate) view: Option<CrashView>,
    pub(crate) open: bool,
    pub(crate) dismissed: Option<PathBuf>,
}

impl Crashes {
    pub(crate) fn shown(&self) -> Option<&CrashView> {
        self.view.as_ref().filter(|_| self.open)
    }
}

fn badge(p: Palette) -> Div {
    div()
        .size(px(56.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(p.error.opacity(0.14))
        .border_1()
        .border_color(p.error.opacity(0.45))
        .child(
            Icon::new(IconName::TriangleAlert)
                .size(px(26.))
                .text_color(p.error),
        )
}

fn step(n: usize, text: String, p: Palette) -> Div {
    h_flex()
        .gap(px(12.))
        .items_start()
        .child(
            div()
                .size(px(22.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(p.accent.opacity(0.16))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.accent)
                .child(n.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pt(px(2.))
                .text_size(px(13.))
                .line_height(px(19.))
                .text_color(p.text)
                .child(text),
        )
}

impl Shell {
    /// The full-window crash modal, while it is open for the newest crash.
    pub(crate) fn crash_overlay(&self, p: Palette, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let view = self.crashes.shown()?;
        let known = view.diagnosis.is_some();
        let (title, what, steps) = match &view.diagnosis {
            Some(d) => (d.title.clone(), d.what.clone(), d.steps.clone()),
            None => (
                tr("We couldn't tell why it crashed").to_owned(),
                tr("Swofty Launcher looked at what the game left behind and didn't recognise this crash. Send your logs to the developers, then join the Swofty Developments Discord so they can get back to you.").to_owned(),
                vec![
                    trf!(
                        "Press {button} and say what you were doing when it crashed.",
                        button = send_logs::label()
                    ),
                    tr("Join the support Discord, so the developers can reply to you there.").to_owned(),
                ],
            ),
        };
        let close = cx.listener(|this, _, _, cx| {
            this.crashes.open = false;
            cx.notify();
        });
        let send = cx.listener(|this, _, window, cx| this.send_logs(window, cx));
        let buttons = h_flex()
            .w_full()
            .gap(px(10.))
            .justify_center()
            .flex_wrap()
            .map(|row| {
                if known {
                    row.child(
                        Button::new("crash-send-logs")
                            .outline()
                            .h(px(38.))
                            .label(send_logs::label())
                            .on_click(send),
                    )
                    .child(
                        Button::new("crash-close")
                            .primary()
                            .h(px(38.))
                            .px(px(18.))
                            .label(tr("Got it"))
                            .on_click(close),
                    )
                } else {
                    row.child(
                        Button::new("crash-send-logs")
                            .primary()
                            .h(px(38.))
                            .label(send_logs::label())
                            .on_click(send),
                    )
                    .child(invite_button(
                        "crash-discord",
                        tr("Join the support Discord"),
                        38.,
                        16.,
                    ))
                    .child(
                        Button::new("crash-close")
                            .ghost()
                            .h(px(38.))
                            .label(tr("Close"))
                            .on_click(close),
                    )
                }
            });
        let card = v_flex()
            .w_full()
            .max_w(px(620.))
            .gap(px(18.))
            .items_center()
            .child(badge(p))
            .child(
                v_flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.error)
                            .child(if view.crashed {
                                tr("F1 25 crashed")
                            } else {
                                tr("F1 25 got stuck")
                            }),
                    )
                    .child(
                        div()
                            .text_center()
                            .text_size(px(22.))
                            .line_height(px(28.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_center()
                            .text_size(px(14.))
                            .line_height(px(21.))
                            .text_color(p.text_muted)
                            .child(what),
                    ),
            )
            .child(
                v_flex()
                    .w_full()
                    .gap(px(12.))
                    .p(px(16.))
                    .rounded(px(10.))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border_variant)
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text_muted)
                            .child(if known {
                                tr("HOW TO FIX IT")
                            } else {
                                tr("WHAT TO DO")
                            }),
                    )
                    .children(steps.into_iter().enumerate().map(|(i, s)| step(i + 1, s, p))),
            )
            .child(buttons)
            .when(!view.summary.is_empty(), |c| {
                c.child(
                    div()
                        .text_center()
                        .text_size(px(11.))
                        .text_color(p.text_placeholder)
                        .child(view.summary.clone()),
                )
            });
        Some(
            Overlay::new("crash-modal")
                .opaque(p.background)
                .below_title_bar()
                .scrolls()
                .child(v_flex().w(px(668.)).px(px(24.)).py(px(32.)).child(card)),
        )
    }
}
