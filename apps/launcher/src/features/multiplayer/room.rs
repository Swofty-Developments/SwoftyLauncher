use crate::ui::scale::{Hit, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::online::multiplayer::types::{
    MemberState, Phase, Problem, RoomContent, RoomSummary, SessionState,
};
use swofty_i18n::{tr, trf};

use super::model::*;
use crate::shell::Shell;
use crate::ui::tag;
use crate::ui::theme::{Palette, ScrollArea};
use crate::ui::tokens::space::GUTTER;

impl Shell {
    pub(super) fn room_view(&mut self, room: &RoomSummary, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let on = self.room_enabled_addons();
        let view = view_addons(room, &on, self.rooms.lobby.print.as_ref());
        let subtitle = room_meta(room);
        let actions = h_flex().gap(px(8.)).items_center().child(
            Button::new("mp-view-back")
                .outline()
                .icon(IconName::ArrowLeft)
                .label(tr("Back to rooms"))
                .on_click(cx.listener(|this, _, _, cx| this.room_close_view(cx))),
        );
        let summary = view.summary;
        let rows: Vec<AnyElement> = view
            .rows
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let (icon, tone) = match row.mark {
                    Mark::Have => (IconName::Check, p.success),
                    Mark::Installs => (IconName::Download, p.text_muted),
                    Mark::Lacks => (IconName::X, p.error),
                };
                h_flex()
                    .px(px(16.))
                    .py(px(10.))
                    .gap(px(12.))
                    .items_center()
                    .when(i > 0, |d| d.border_t_1().border_color(p.border_variant))
                    .child(Icon::new(icon).size(px(16.)).text_color(tone))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(14.))
                            .child(row.name),
                    )
                    .child(div().text_size(px(13.)).text_color(p.text_muted).child(row.note))
                    .into_any_element()
            })
            .collect();
        let ad = self.page_ad(false, self.layout.content_width - 2. * GUTTER, p);
        v_flex()
            .size_full()
            .child(self.header(room.name.clone(), subtitle, actions, p))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("mp-room-view")
                    .px(px(GUTTER))
                    .pt(px(4.))
                    .pb(px(24.))
                    .gap(px(16.))
                    .child(div().text_size(px(13.)).text_color(p.text_muted).child(summary))
                    .child(div().text_size(px(13.)).text_color(p.text_muted).child(trf!(
                        "Press Play {host}'s Lobby in the bar below to start F1 25 in this room.",
                        host = room.host.name
                    )))
                    .when(!rows.is_empty(), |d| {
                        d.child(
                            v_flex()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(p.border)
                                .overflow_hidden()
                                .children(rows),
                        )
                    })
                    .children(ad),
            )
            .into_any_element()
    }

    pub(super) fn status_card(
        &self,
        st: &Status,
        state: &SessionState,
        subtitle: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let starting =
            self.rooms.game.launching || self.launching.launch.is_some() || self.game.build.is_some();
        let after = game_after_room(state.phase.is_over(), starting)
            .map(|text| div().text_size(px(13.)).text_color(p.text_muted).child(text));
        if let Some(card) = StepCard::of(state, self.rooms.leaving) {
            let headline = st
                .problem
                .as_ref()
                .map_or(st.progress.label.as_str(), |problem| problem.text.as_str());
            return match self.step_card(&card, headline, state, subtitle, p, cx) {
                Some(card) => Some(card.children(after)),
                None => after.map(|after| self.room_card(p).child(after)),
            };
        }
        let over = state.phase.is_over();
        let current = step_number(st.progress.step);
        let steps = h_flex()
            .gap(px(14.))
            .flex_wrap()
            .children(STEPS.iter().enumerate().map(|(i, step)| {
                div()
                    .text_size(px(12.))
                    .text_color(if i == current && !over {
                        p.text
                    } else if i < current && !over {
                        p.text_muted
                    } else {
                        p.text_placeholder
                    })
                    .when(i == current && !over, |d| d.font_weight(FontWeight::SEMIBOLD))
                    .child(step_name(*step))
            }));
        let fraction = f32::from(st.progress.percent) / 100.0;
        let bar = div()
            .h(px(6.))
            .w_full()
            .rounded_full()
            .bg(p.border_variant)
            .child(
                div()
                    .h_full()
                    .w(relative(fraction))
                    .rounded_full()
                    .bg(if st.tone == Tone::Blocked {
                        p.warning
                    } else {
                        p.accent
                    }),
            );
        Some(
            self.room_card(p)
                .gap(px(14.))
                .child(self.status_head(st, state, p, cx))
                .when(!over, |d| d.child(bar).child(steps))
                .children(after),
        )
    }

    /// The headline row of a module that reports no steps: the session's state, its hint or problem, and the
    /// buttons for them.
    fn status_head(&self, st: &Status, state: &SessionState, p: Palette, cx: &mut Context<Self>) -> Div {
        let m = &self.rooms;
        let (icon, color) = match st.tone {
            Tone::Working => (IconName::LoaderCircle, p.accent),
            Tone::Blocked => (IconName::CircleAlert, p.warning),
            Tone::Good => (IconName::CircleCheck, p.success),
            Tone::Over => (IconName::Info, p.text_placeholder),
        };
        let over = state.phase.is_over();
        let headline = match &st.problem {
            Some(problem) => problem.text.clone(),
            None => st.progress.label.clone(),
        };
        let fixes = self.problem_buttons(st.problem.as_ref(), state, cx);
        let reopened = fixes.reopen.is_some();
        h_flex()
            .gap(px(12.))
            .items_center()
            .map(|d| match icon {
                IconName::LoaderCircle => d.child(Spinner::new().icon(icon).with_size(px(22.)).color(color)),
                _ => d.child(Icon::new(icon).size(px(22.)).text_color(color)),
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(div().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(
                        if m.leaving && !over {
                            tr("Leaving the room…").to_owned()
                        } else {
                            headline
                        },
                    ))
                    .children(
                        st.hint
                            .filter(|_| !m.leaving)
                            .map(|h| div().text_size(px(13.)).text_color(p.text_muted).child(h)),
                    )
                    .children(
                        st.problem
                            .as_ref()
                            .and_then(|problem| problem.detail.clone().filter(|d| !repeats(d, &problem.text)))
                            .filter(|_| !m.leaving)
                            .map(|d| div().text_size(px(13.)).text_color(p.text_muted).child(d)),
                    )
                    .children(reopened.then(|| reopen_note(p))),
            )
            .children(self.race_button(state, cx))
            .children(fixes.reopen)
            .children(fixes.retry_or_leave)
    }

    /// What a problem offers: restarting F1 25 when the session's game is open, and trying again or leaving.
    fn problem_buttons(
        &self,
        problem: Option<&Problem>,
        state: &SessionState,
        cx: &mut Context<Self>,
    ) -> ProblemButtons {
        let m = &self.rooms;
        let over = state.phase.is_over();
        let retry_or_leave = problem.filter(|_| !m.leaving && !over).map(|problem| {
            if problem.retry {
                Button::new("mp-retry")
                    .primary()
                    .tap_sm()
                    .icon(IconName::RefreshCw)
                    .label(tr("Retry"))
                    .on_click(cx.listener(|this, _, _, cx| this.room_retry(cx)))
            } else {
                Button::new("mp-problem-leave")
                    .primary()
                    .tap_sm()
                    .icon(IconName::LogOut)
                    .label(tr("Leave"))
                    .on_click(cx.listener(|this, _, _, cx| this.room_leave(cx)))
            }
        });
        let running = !self.game.processes.is_empty();
        let reopen = (problem.is_some()
            && state.launch.is_some()
            && running
            && m.game.closing.is_none()
            && !over
            && !m.leaving)
            .then(|| {
                Button::new("mp-restart-game")
                    .primary()
                    .tap_sm()
                    .label(tr("Restart F1 25"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        super::Multiplayer::close_game(this, window, cx);
                        cx.notify();
                    }))
            });
        ProblemButtons {
            reopen,
            retry_or_leave,
        }
    }

    /// The host's button that hides the room from the list while they race, and shows it again after.
    pub(super) fn race_button(&self, state: &SessionState, cx: &mut Context<Self>) -> Option<Button> {
        let over = state.phase.is_over();
        let shown = state.host
            && !over
            && !self.rooms.leaving
            && matches!(state.phase, Phase::InLobby | Phase::Racing);
        shown.then(|| {
            let started = state.room.as_ref().is_some_and(|r| r.started);
            Button::new("mp-race")
                .outline()
                .tap_sm()
                .icon(IconName::Flag)
                .label(if started {
                    tr("Race over")
                } else {
                    tr("Race started")
                })
                .tooltip(if started {
                    tr("Show the room in the list again")
                } else {
                    tr("Hide the room from the list while you race")
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(s) = &this.rooms.session {
                        s.mark_race(!started);
                    }
                    cx.notify();
                }))
        })
    }

    pub(super) fn room(&mut self, state: &SessionState, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let m = &self.rooms;
        let over = state.phase.is_over();
        let closed = super::tracker::shows_closed(state, StepCard::of(state, m.leaving).as_ref());
        let listed = super::tracker::shows_players(state);
        let room = state.room.clone();
        let title = room
            .as_ref()
            .map_or_else(|| "Multiplayer".to_owned(), |r| r.name.clone());
        let subtitle = match room.as_ref() {
            _ if closed => closed_subtitle(room.is_some(), state.host).to_owned(),
            None => tr("Setting up the room\u{2026}").to_owned(),
            Some(r) => {
                let mut parts = vec![
                    trf!("{count} in the lobby", count = joined(state)),
                    trf!(
                        "{joined}/{max} players",
                        joined = state.members.len(),
                        max = r.max_members
                    ),
                    if r.public { tr("Listed") } else { tr("Private") }.to_owned(),
                ];
                if state.host {
                    parts.push(tr("You're the host").into());
                }
                parts.join(" \u{00b7} ")
            }
        };
        let code = room.as_ref().filter(|_| !over).map(|r| r.code.clone());
        let actions = h_flex()
            .gap(px(8.))
            .items_center()
            .when_some(code, |d, code| {
                let copy = code.clone();
                d.child(
                    Button::new("mp-copy-code")
                        .outline()
                        .icon(if m.copied.is_some() {
                            IconName::Check
                        } else {
                            IconName::Copy
                        })
                        .label(if m.copied.is_some() {
                            "Copied".to_owned()
                        } else {
                            code
                        })
                        .tooltip(tr("Copy the invite code"))
                        .on_click(cx.listener(move |this, _, _, cx| this.room_copy_code(copy.clone(), cx))),
                )
            })
            .when_some(m.steward_url(), |d, url| {
                d.child(
                    Button::new("mp-steward")
                        .outline()
                        .icon(IconName::ExternalLink)
                        .label(tr("Open stewarding"))
                        .tooltip(tr("Open League Stewarding in your browser"))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
            .child(
                Button::new("mp-leave")
                    .outline()
                    .icon(IconName::LogOut)
                    .label(if m.leaving {
                        tr("Leaving…")
                    } else if over {
                        tr("Back to rooms")
                    } else {
                        tr("Leave")
                    })
                    .disabled(m.leaving)
                    .on_click(cx.listener(|this, _, _, cx| this.room_leave(cx))),
            );
        let st = room_status(state);
        let status = self.status_card(&st, state, &subtitle, p, cx);
        let players: Vec<AnyElement> = state
            .members
            .iter()
            .enumerate()
            .map(|(i, member)| self.member_row(member, &state.content, i == 0, p))
            .collect();
        let cars = content_check(state).filter(|_| listed).map(|(owns, text)| {
            v_flex()
                .gap(px(8.))
                .child(self.section_head("CARS".to_owned(), p))
                .child(
                    h_flex()
                        .px(px(16.))
                        .py(px(10.))
                        .gap(px(12.))
                        .items_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(p.border)
                        .child(
                            Icon::new(match owns {
                                Some(true) => IconName::Check,
                                Some(false) => IconName::X,
                                None => IconName::Info,
                            })
                            .size(px(16.))
                            .text_color(match owns {
                                Some(true) => p.success,
                                Some(false) => p.error,
                                None => p.text_muted,
                            }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(14.))
                                .child(tr("F1 2026 cars")),
                        )
                        .child(div().text_size(px(13.)).text_color(p.text_muted).child(text)),
                )
        });
        let ad = self.page_ad(false, self.layout.content_width - 2. * GUTTER, p);
        v_flex()
            .size_full()
            .child(self.header(title, subtitle, actions, p))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("mp-room")
                    .px(px(GUTTER))
                    .pt(px(4.))
                    .pb(px(24.))
                    .gap(px(24.))
                    .children(status)
                    .children(cars)
                    .when(listed, |d| {
                        d.child(
                            v_flex()
                                .gap(px(8.))
                                .child(self.section_head(
                                    trf!("PLAYERS \u{00b7} {count}", count = state.members.len()),
                                    p,
                                ))
                                .child(if players.is_empty() {
                                    div()
                                        .text_size(px(13.))
                                        .text_color(p.text_muted)
                                        .child(tr("Nobody is here yet."))
                                        .into_any_element()
                                } else {
                                    v_flex()
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(p.border)
                                        .overflow_hidden()
                                        .children(players)
                                        .into_any_element()
                                }),
                        )
                    })
                    .children(ad),
            )
            .into_any_element()
    }

    pub(super) fn member_row(
        &self,
        m: &MemberState,
        content: &RoomContent,
        first: bool,
        p: Palette,
    ) -> AnyElement {
        let (word, tone) = content_word(content, m).unwrap_or_else(|| member_word(m));
        h_flex()
            .px(px(16.))
            .py(px(12.))
            .gap(px(12.))
            .when(!first, |d| d.border_t_1().border_color(p.border_variant))
            .child(self.face(&m.name, m.avatar.as_deref(), 32., p))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .truncate()
                            .child(m.name.clone()),
                    )
                    .when(m.host, |d| d.child(tag("Host", p)))
                    .when(m.me, |d| d.child(tag("You", p))),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(match tone {
                        Tone::Good => p.success,
                        Tone::Blocked => p.warning,
                        _ => p.text_muted,
                    })
                    .child(word),
            )
            .into_any_element()
    }
}

struct ProblemButtons {
    reopen: Option<Button>,
    retry_or_leave: Option<Button>,
}

fn reopen_note(p: Palette) -> Div {
    div()
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(tr("F1 25 is open. It restarts for this session."))
}
