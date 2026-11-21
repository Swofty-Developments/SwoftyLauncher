use crate::ui::scale::{Hit, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::online::multiplayer::types::SessionState;
use swofty_i18n::{tr, trf};

use super::model::{StepCard, room_status};
use super::tracker::{Act, Banner, Focus, Here, Mark, Node, Note, Tracker};
use crate::shell::Shell;
use crate::ui::side_scroll;
use crate::ui::theme::Palette;

/// The narrowest a node gets before the row scrolls sideways. Seven nodes fit the room page at the launcher's
/// minimum window width up to about 150% interface scale.
const NODE_MIN: f32 = 84.;
const ICON: f32 = 20.;

impl Shell {
    /// What the launcher itself knows about the room's game, or what a previewed room says it knows.
    pub(super) fn room_here(&self) -> Here {
        let m = &self.rooms;
        if let Some(here) = m.preview.as_ref().and_then(|p| p.here) {
            return here;
        }
        Here {
            leaving: m.leaving,
            running: !self.game.processes.is_empty(),
            closing: m.game.closing.is_some(),
            building: self.game.build.is_some(),
            launching: m.game.launching || self.launching.launch.is_some(),
            held: m.game.held.is_some(),
            taken: m.game.ask_seq,
        }
    }

    /// The room strip's line under the room's name: what the room page's detail area says, else the session's
    /// headline.
    pub(super) fn room_strip_line(&self, state: &SessionState) -> String {
        let st = room_status(state);
        let headline = st.problem.map_or(st.progress.label, |problem| problem.text);
        let Some(card) = StepCard::of(state, self.rooms.leaving) else {
            return headline;
        };
        Tracker::of(&card, state, &self.room_here(), "")
            .focus
            .and_then(|f| f.title)
            .unwrap_or(headline)
    }

    /// The status card of a module that reports steps: the banner for what no step says, the row of steps, and
    /// the detail area for the step that matters now.
    pub(super) fn step_card(
        &self,
        card: &StepCard,
        headline: &str,
        state: &SessionState,
        subtitle: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let here = self.room_here();
        let banner = super::tracker::banner(card, state, here.leaving, headline, subtitle);
        let tracker = Tracker::of(card, state, &here, subtitle);
        if banner.is_none() && tracker.nodes.is_empty() {
            return None;
        }
        Some(
            self.room_card(p)
                .gap(px(16.))
                .children(banner.map(|b| banner_row(&b, p)))
                .when(!tracker.nodes.is_empty(), |d| {
                    d.child(self.step_row(&tracker.nodes, p, cx))
                })
                .children(tracker.focus.as_ref().map(|f| self.step_detail(f, state, p, cx))),
        )
    }

    fn step_row(&self, nodes: &[Node], p: Palette, cx: &mut Context<Self>) -> Div {
        let last = nodes.len().saturating_sub(1);
        let reached = |i: usize| {
            nodes
                .get(i)
                .is_some_and(|n| matches!(n.mark, Mark::Done | Mark::Warning))
        };
        let row = h_flex()
            .id("mp-steps")
            .w_full()
            .items_start()
            .overflow_x_scroll()
            .restrict_scroll_to_axis()
            .children(nodes.iter().enumerate().map(|(i, node)| {
                let before = (i > 0).then(|| {
                    if reached(i - 1) {
                        p.success
                    } else {
                        p.border_variant
                    }
                });
                let after = (i < last).then(|| if reached(i) { p.success } else { p.border_variant });
                step_node(node, before, after, p)
            }));
        side_scroll("mp-steps-more", row, &self.rooms.steps_scroll, p, cx)
    }

    fn step_detail(&self, focus: &Focus, state: &SessionState, p: Palette, cx: &mut Context<Self>) -> Div {
        let line = |text: String| div().text_size(px(13.)).text_color(p.text_muted).child(text);
        let title_color = match focus.mark {
            Mark::Failed => p.error,
            Mark::Warning => p.warning,
            _ => p.text,
        };
        let buttons: Vec<Button> = focus.acts.iter().map(|act| self.act_button(*act, cx)).collect();
        let race = focus.race.then(|| self.race_button(state, cx)).flatten();
        v_flex()
            .gap(px(4.))
            .pt(px(14.))
            .border_t_1()
            .border_color(p.border_variant)
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.text_placeholder)
                    .child(focus.caption.clone()),
            )
            .child(
                h_flex()
                    .gap(px(12.))
                    .items_start()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(3.))
                            .children(focus.title.clone().map(|t| {
                                div()
                                    .text_size(px(15.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(title_color)
                                    .child(t)
                            }))
                            .children(focus.lines.iter().cloned().map(line))
                            .children(focus.fixes.iter().map(|fix| bullet(fix, p)))
                            .children(focus.hint.map(|h| line(h.to_owned()))),
                    )
                    .children(race),
            )
            .children(focus.note.as_ref().map(|n| self.step_note(n, p, cx)))
            .when(!buttons.is_empty(), |d| {
                d.child(h_flex().pt(px(8.)).gap(px(8.)).flex_wrap().children(buttons))
            })
    }

    fn step_note(&self, note: &Note, p: Palette, cx: &mut Context<Self>) -> Div {
        h_flex()
            .mt(px(8.))
            .p(px(12.))
            .gap(px(10.))
            .items_start()
            .rounded(px(6.))
            .bg(p.warning.opacity(0.08))
            .border_1()
            .border_color(p.warning.opacity(0.4))
            .child(
                Icon::new(IconName::CircleAlert)
                    .size(px(15.))
                    .flex_shrink_0()
                    .text_color(p.warning),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text)
                            .child(note.text.clone()),
                    )
                    .children(
                        note.lines
                            .iter()
                            .map(|l| div().text_size(px(13.)).text_color(p.text_muted).child(l.clone())),
                    )
                    .children(note.fixes.iter().map(|fix| bullet(fix, p))),
            )
            .when(note.retry, |d| {
                d.child(
                    Button::new("mp-note-retry")
                        .outline()
                        .tap_sm()
                        .icon(IconName::RefreshCw)
                        .label(tr("Retry"))
                        .on_click(cx.listener(|this, _, _, cx| this.room_retry(cx))),
                )
            })
    }

    fn act_button(&self, act: Act, cx: &mut Context<Self>) -> Button {
        match act {
            Act::RestartGame => Button::new("mp-restart-game")
                .primary()
                .tap_sm()
                .icon(IconName::RefreshCw)
                .label(tr("Restart F1 25"))
                .on_click(cx.listener(|this, _, window, cx| {
                    super::Multiplayer::close_game(this, window, cx);
                    cx.notify();
                })),
            Act::Retry => Button::new("mp-retry")
                .primary()
                .tap_sm()
                .icon(IconName::RefreshCw)
                .label(tr("Retry"))
                .on_click(cx.listener(|this, _, _, cx| this.room_retry(cx))),
            Act::Leave => Button::new("mp-problem-leave")
                .outline()
                .tap_sm()
                .icon(IconName::LogOut)
                .label(tr("Leave"))
                .on_click(cx.listener(|this, _, _, cx| this.room_leave(cx))),
        }
    }
}

fn banner_row(banner: &Banner, p: Palette) -> Div {
    let (text, leaving) = match banner {
        Banner::Leaving => (tr("Leaving the room…").to_owned(), true),
        Banner::Over(text) => (text.clone(), false),
    };
    h_flex()
        .gap(px(12.))
        .items_center()
        .map(|d| {
            if leaving {
                d.child(
                    Spinner::new()
                        .icon(IconName::LoaderCircle)
                        .with_size(px(20.))
                        .color(p.accent),
                )
            } else {
                d.child(
                    Icon::new(IconName::Info)
                        .size(px(20.))
                        .text_color(p.text_placeholder),
                )
            }
        })
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text),
        )
}

fn bullet(fix: &str, p: Palette) -> Div {
    h_flex()
        .gap(px(6.))
        .items_start()
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(div().flex_shrink_0().child("\u{2022}"))
        .child(div().flex_1().min_w_0().child(fix.to_owned()))
}

/// One node: its mark between the line from the step before and the line to the step after, and its title under
/// it in at most two lines. The tooltip gives the whole title and its state in words.
fn step_node(node: &Node, before: Option<Hsla>, after: Option<Hsla>, p: Palette) -> Stateful<Div> {
    let (icon, color) = match node.mark {
        _ if node.dimmed => (IconName::Circle, p.text_placeholder.opacity(0.6)),
        Mark::Done => (IconName::CircleCheck, p.success),
        Mark::Active => (IconName::LoaderCircle, p.accent),
        Mark::Warning => (IconName::CircleAlert, p.warning),
        Mark::Failed => (IconName::CircleX, p.error),
        Mark::Pending => (IconName::Circle, p.text_placeholder),
    };
    let title_color = match node.mark {
        _ if node.dimmed => p.text_placeholder,
        _ if node.current && node.mark == Mark::Failed => p.error,
        _ if node.current && node.mark == Mark::Warning => p.warning,
        _ if node.current => p.text,
        Mark::Done | Mark::Warning => p.text_muted,
        Mark::Active | Mark::Failed | Mark::Pending => p.text_placeholder,
    };
    let tip = trf!("{title}: {state}", title = node.title, state = node.mark.word());
    let segment = |color: Option<Hsla>| {
        div()
            .flex_1()
            .h(px(2.))
            .rounded_full()
            .when_some(color, |d, c| d.bg(c))
    };
    v_flex()
        .id(ElementId::Name(format!("mp-step-{}", node.id).into()))
        .flex_1()
        .min_w(px(NODE_MIN))
        .gap(px(6.))
        .items_center()
        .child(
            h_flex()
                .w_full()
                .h(px(ICON + 8.))
                .items_center()
                .child(segment(before))
                .child(
                    div()
                        .flex_shrink_0()
                        .mx(px(6.))
                        .size(px(ICON + 8.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .when(node.current, |d| d.bg(color.opacity(0.14)))
                        .map(|d| match node.mark {
                            Mark::Active if !node.dimmed => {
                                d.child(Spinner::new().icon(icon).with_size(px(ICON)).color(color))
                            }
                            _ => d.child(Icon::new(icon).size(px(ICON)).text_color(color)),
                        }),
                )
                .child(segment(after)),
        )
        .child(
            div()
                .w_full()
                .px(px(4.))
                .text_center()
                .text_size(px(12.))
                .line_height(px(16.))
                .line_clamp(2)
                .text_color(title_color)
                .when(node.current, |d| d.font_weight(FontWeight::SEMIBOLD))
                .child(node.title.clone()),
        )
        .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
}
