use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::diagnostics::crash::StartFailure;
use swofty_core::diagnostics::known_problems::{self, Diagnosis};
use swofty_core::diagnostics::send_logs::{Outcome, RoomRun, Sent, Stage};
use swofty_core::session::SessionRecord;
use swofty_core::store::Store;
use swofty_i18n::{tr, trf};

mod auto_send;

pub(crate) use self::auto_send::GameEnd;
use self::auto_send::{AutoSend, CrashFacts, Job, Plan, RoomWatch};
use crate::dialogs::send_logs::{self, LogsSent};
use crate::services::game::{self, Install};
use crate::shell::Shell;
use crate::shell::host::{Host, Hosted};
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
    /// The send of this crash's logs the launcher made by itself, for a crash in a room.
    pub auto: Option<AutoSend>,
}

impl CrashView {
    /// The play bar's line under "Crashed".
    pub(crate) fn headline(&self) -> String {
        match &self.diagnosis {
            Some(d) => d.title.clone(),
            None => self.summary.clone(),
        }
    }

    /// The automatic send the modal shows, which names the room: only for a player with multiplayer access.
    pub(crate) fn auto_shown(&self, allowed: bool) -> Option<&AutoSend> {
        self.auto.as_ref().filter(|_| allowed)
    }
}

#[derive(Default)]
pub(crate) struct Crashes {
    pub(crate) latest: Option<(PathBuf, Result<String, StartFailure>)>,
    /// The newest crash's diagnosis, and whether its modal is open.
    pub(crate) view: Option<CrashView>,
    pub(crate) open: bool,
    pub(crate) dismissed: Option<PathBuf>,
    pub(crate) room_watch: RoomWatch,
    /// The automatic send on its way: the crash's folder and how far it got.
    sending: Option<(PathBuf, Arc<Mutex<Stage>>)>,
}

/// What the crash watch borrows from the shell: the data folder, the copy Play uses, whether F1 25 runs, the
/// room the launcher is in, whether the signed-in player has multiplayer access and whether a crash in a room
/// sends its logs by itself.
pub(crate) struct CrashNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) install: Option<&'a Install>,
    pub(crate) running: bool,
    pub(crate) room: Option<RoomRun>,
    pub(crate) allowed: bool,
    pub(crate) send_room_crash_logs: bool,
}

/// What one look at a finished session found.
struct Diagnosed {
    dir: PathBuf,
    summary: Result<String, StartFailure>,
    diagnosis: Option<Diagnosis>,
    crashed: bool,
    fresh: bool,
}

/// What the crash watch reports to the shell.
pub(crate) enum CrashChange {
    /// A game that ran in a room ended; the room's session hears how.
    GameEnded(GameEnd),
}

impl Hosted for Crashes {
    type Needs<'a> = CrashNeeds<'a>;
    type Change = CrashChange;
}

impl Crashes {
    pub(crate) fn shown(&self) -> Option<&CrashView> {
        self.view.as_ref().filter(|_| self.open)
    }

    /// Takes a session that crashed or ended and is not diagnosed yet; false for any other.
    fn begin(&mut self, s: &SessionRecord) -> bool {
        if !(s.crashed() || s.end.is_some()) || self.latest.as_ref().is_some_and(|(d, _)| *d == s.dir) {
            return false;
        }
        self.latest = Some((s.dir.clone(), Ok(String::new())));
        true
    }

    /// Takes a finished diagnosis: a crash, or an ended session with a known problem, shows in the
    /// play bar, and a fresh one opens the modal. `plan` says what the modal does about the logs; the room
    /// to send them for comes back when they go now.
    fn took(&mut self, found: Diagnosed, plan: Plan) -> Option<RoomRun> {
        let Diagnosed {
            dir,
            summary,
            diagnosis,
            crashed,
            fresh,
        } = found;
        let mut send = None;
        if let Ok(text) = &summary
            && (crashed || diagnosis.is_some())
        {
            let auto = match plan {
                Plan::Ask => None,
                Plan::Send(room) => {
                    send = Some(room);
                    Some(AutoSend::Sending)
                }
                Plan::Done(outcome) => Some(outcome.into()),
            };
            self.view = Some(CrashView {
                dir: dir.clone(),
                diagnosis,
                crashed,
                summary: text.clone(),
                auto,
            });
            self.open |= fresh;
        }
        self.latest = Some((dir, summary));
        send
    }

    /// Takes how the automatic send for the crash in `dir` went.
    fn sent(&mut self, dir: &Path, outcome: Outcome) {
        if self.sending.as_ref().is_some_and(|(d, _)| d == dir) {
            self.sending = None;
        }
        if let Some(view) = self.view.as_mut().filter(|v| v.dir == dir) {
            view.auto = Some(outcome.into());
        }
    }

    /// The crash on screen when its automatic send failed, which a send from the modal then covers.
    fn uncovered(&self) -> Option<PathBuf> {
        self.shown()
            .filter(|v| v.auto.as_ref().is_some_and(|a| !a.covers()))
            .map(|v| v.dir.clone())
    }

    /// The player sent the logs from the modal after the automatic send for the crash in `dir` failed: its folder
    /// records the send, so the modal shows the logs as sent from then on and never offers or makes another.
    fn sent_by_hand(&mut self, dir: &Path, sent: &Sent) {
        let outcome = Outcome::Sent {
            parts: sent.parts,
            reference: sent.reference.clone(),
        };
        if let Err(e) = outcome.write(dir) {
            eprintln!("swofty-launcher: could not note the send in the session folder: {e}");
        }
        self.sent(dir, outcome);
    }

    /// How far the automatic send for the crash in `dir` got, while it is on its way.
    fn stage(&self, dir: &Path) -> Option<String> {
        let (_, stage) = self.sending.as_ref().filter(|(d, _)| d == dir)?;
        stage.lock().ok().map(|s| s.to_string())
    }

    /// Follows the room the running game is in, then diagnoses a session that crashed or ended, once per
    /// session and off the UI thread, and opens the crash modal for a fresh one. A fresh end of a game that
    /// ran in a room notes the step the room was at, is told to the room at once, and a crash there sends its
    /// logs by itself, unless the setting is off.
    pub(crate) fn watch<H: Host<Self>>(host: &mut H, session: Option<&SessionRecord>, cx: &mut Context<H>) {
        let (crashes, needs) = host.split();
        let live = needs.room.clone();
        crashes.room_watch.follow(session, needs.running, needs.room);
        if let Some(s) = session.filter(|s| crashes.begin(s)) {
            let session = s.clone();
            let crashed = s.crashed();
            let game_dir = needs.install.map(|i| i.dir.clone());
            let store = needs.store.clone();
            let fresh = s.ended.is_some_and(|e| game::now_secs() - e < FRESH_SECS);
            let in_room = fresh && crashes.room_watch.ended(s, live);
            let snapshot = if in_room {
                let at_end = swofty_core::online::multiplayer::client::snapshot();
                crashes.room_watch.at_step(s, at_end.as_ref());
                host.changed(CrashChange::GameEnded(GameEnd::of(&session)), cx);
                swofty_core::online::multiplayer::client::snapshot().or(at_end)
            } else {
                None
            };
            cx.spawn(async move |this, cx| {
                let dir = session.dir.clone();
                let (summary, diagnosis, facts) = cx
                    .background_executor()
                    .spawn(async move {
                        let facts = CrashFacts::read(&session, fresh);
                        if !crashed {
                            let stall = known_problems::stall(&session, game_dir.as_deref(), &store)
                                .or_else(|| known_problems::tyres(&session, game_dir.as_deref(), &store))
                                .or_else(|| known_problems::logos(&session, game_dir.as_deref(), &store));
                            return (Ok(String::new()), stall, facts);
                        }
                        let summary = game::crash_summary(&session, game_dir.as_deref());
                        let diagnosis = known_problems::crash(&session, game_dir.as_deref(), &store);
                        (summary, diagnosis, facts)
                    })
                    .await;
                this.update(cx, |host, cx| {
                    let (crashes, needs) = host.split();
                    let plan = facts.plan(
                        needs.allowed,
                        needs.send_room_crash_logs,
                        swofty_core::diagnostics::send_logs::available(),
                    );
                    let found = Diagnosed {
                        dir: dir.clone(),
                        summary,
                        diagnosis,
                        crashed,
                        fresh,
                    };
                    if let Some(room) = crashes.took(found, plan) {
                        Self::send(host, dir, room, snapshot, cx);
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    /// Sends the logs of the crash in `dir` off the UI thread, and shows how it went. `snapshot` is the room's
    /// state as the game ended, when the launcher was still in the room then.
    fn send<H: Host<Self>>(
        host: &mut H,
        dir: PathBuf,
        room: RoomRun,
        snapshot: Option<serde_json::Value>,
        cx: &mut Context<H>,
    ) {
        let (crashes, needs) = host.split();
        let stage = Arc::new(Mutex::new(Stage::Collecting));
        crashes.sending = Some((dir.clone(), stage.clone()));
        let job = Job {
            store: needs.store.clone(),
            session: dir.clone(),
            game: send_logs::game_info(needs.install),
            room,
            snapshot: snapshot.or_else(swofty_core::online::multiplayer::client::snapshot),
            stage,
        };
        crate::app::app_log::note(&format!(
            "crash: F1 25 crashed in a room; sending the logs of {} by themselves",
            dir.display()
        ));
        cx.spawn(async move |this, cx| {
            let outcome = cx.background_executor().spawn(async move { job.run() }).await;
            crate::app::app_log::note(&format!("crash: the automatic send {outcome:?}"));
            this.update(cx, |host, cx| {
                host.split().0.sent(&dir, outcome);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(250)).await;
                let Ok(sending) = this.update(cx, |host, cx| {
                    cx.notify();
                    host.split().0.sending.is_some()
                }) else {
                    break;
                };
                if !sending {
                    break;
                }
            }
        })
        .detach();
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

/// The line under the steps for logs the launcher sent by itself: on their way and how far they got, sent, or
/// not sent and why.
fn auto_note(auto: &AutoSend, stage: Option<String>, p: Palette) -> Div {
    let note = h_flex()
        .w_full()
        .gap(px(10.))
        .items_start()
        .p(px(12.))
        .rounded(px(8.))
        .bg(p.surface)
        .border_1()
        .border_color(p.border_variant)
        .text_size(px(13.))
        .line_height(px(19.));
    let text = |t: String| div().flex_1().min_w_0().child(t);
    match auto {
        AutoSend::Sending => note
            .text_color(p.text_muted)
            .child(div().pt(px(2.)).child(Spinner::new().with_size(px(15.))))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(tr(
                        "F1 25 crashed in your room, so the launcher is sending your logs to the developers…",
                    ))
                    .children(stage.map(|s| div().text_size(px(12.)).child(s))),
            ),
        AutoSend::Sent { reference } => note
            .child(
                Icon::new(IconName::CircleCheck)
                    .size(px(15.))
                    .mt(px(2.))
                    .text_color(p.success),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(div().text_color(p.text).child(tr(
                        "F1 25 crashed in your room, so your logs were already sent to the developers. You don't need to send them again.",
                    )))
                    .children(reference.as_ref().map(|r| {
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(trf!("Reference {reference}", reference = r))
                    })),
            ),
        AutoSend::Failed { why } => note
            .child(
                Icon::new(IconName::CircleAlert)
                    .size(px(15.))
                    .mt(px(2.))
                    .text_color(p.error),
            )
            .child(text(trf!(
                "F1 25 crashed in your room, but your logs couldn't be sent by themselves. {why}",
                why = why
            ))),
    }
}

impl Shell {
    /// Send Logs from the crash modal. After the automatic send for the crash on screen failed, a send from here
    /// stands for it.
    fn send_crash_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let uncovered = self.crashes.uncovered();
        let form = self.open_send_logs(window, cx);
        if let Some(dir) = uncovered {
            cx.subscribe(&form, move |shell, _, sent: &LogsSent, cx| {
                shell.crashes.sent_by_hand(&dir, &sent.0);
                cx.notify();
            })
            .detach();
        }
    }

    /// The full-window crash modal, while it is open for the newest crash.
    pub(crate) fn crash_overlay(&self, p: Palette, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let view = self.crashes.shown()?;
        let known = view.diagnosis.is_some();
        let auto = view.auto_shown(self.account.allowed());
        let covered = auto.is_some_and(AutoSend::covers);
        let (title, what, steps) = match &view.diagnosis {
            Some(d) => (d.title.clone(), d.what.clone(), d.steps.clone()),
            None if covered => (
                tr("We couldn't tell why it crashed").to_owned(),
                tr("Swofty Launcher looked at what the game left behind and didn't recognise this crash. Your logs are already with the developers. Join the Swofty Developments Discord so they can get back to you.").to_owned(),
                vec![tr("Join the support Discord and say what you were doing when it crashed, so the developers can reply to you there.").to_owned()],
            ),
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
        let send = cx.listener(|this, _, window, cx| this.send_crash_logs(window, cx));
        let buttons = h_flex()
            .w_full()
            .gap(px(10.))
            .justify_center()
            .flex_wrap()
            .map(|row| {
                if known {
                    row.when(!covered, |row| {
                        row.child(
                            Button::new("crash-send-logs")
                                .outline()
                                .h(px(38.))
                                .label(send_logs::label())
                                .on_click(send),
                        )
                    })
                    .child(
                        Button::new("crash-close")
                            .primary()
                            .h(px(38.))
                            .px(px(18.))
                            .label(tr("Got it"))
                            .on_click(close),
                    )
                } else {
                    row.when(!covered, |row| {
                        row.child(
                            Button::new("crash-send-logs")
                                .primary()
                                .h(px(38.))
                                .label(send_logs::label())
                                .on_click(send),
                        )
                    })
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
            .children(auto.map(|a| auto_note(a, self.crashes.stage(&view.dir), p)))
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use std::sync::{Arc, Mutex};
    use swofty_core::diagnostics::crash::StartFailure;

    use swofty_core::diagnostics::send_logs::{Outcome, RoomRun, Sent, Stage};
    use swofty_core::session::SessionRecord;

    use super::{AutoSend, CrashView, Crashes, Diagnosed, Plan};

    fn found(dir: &str, summary: Result<String, StartFailure>, crashed: bool, fresh: bool) -> Diagnosed {
        Diagnosed {
            dir: dir.into(),
            summary,
            diagnosis: None,
            crashed,
            fresh,
        }
    }

    fn room() -> RoomRun {
        RoomRun {
            code: Some("K7Q2PX".into()),
            launched: true,
            ..RoomRun::default()
        }
    }

    fn session(dir: &str, exit_code: Option<i32>) -> SessionRecord {
        SessionRecord {
            dir: PathBuf::from(dir),
            request_id: None,
            started: Some(1.0),
            ended: Some(2.0),
            exit_code,
            end: None,
        }
    }

    const ACCESS_VIOLATION: i32 = 0xC000_0005_u32 as i32;

    #[test]
    fn each_crashed_session_is_diagnosed_once() {
        let mut crashes = Crashes::default();
        let s = session("s1", Some(ACCESS_VIOLATION));
        assert!(crashes.begin(&s));
        assert!(!crashes.begin(&s));
        assert!(crashes.begin(&session("s2", Some(ACCESS_VIOLATION))));
    }

    #[test]
    fn the_modal_names_the_room_only_for_a_player_with_multiplayer_access() {
        let view = CrashView {
            dir: PathBuf::from("s1"),
            diagnosis: None,
            crashed: true,
            summary: String::new(),
            auto: Some(AutoSend::Sent {
                reference: Some("1311204".into()),
            }),
        };
        assert_eq!(view.auto_shown(true), view.auto.as_ref());
        assert_eq!(view.auto_shown(false), None);
    }

    #[test]
    fn a_session_still_running_is_not_diagnosed() {
        let mut crashes = Crashes::default();
        assert!(!crashes.begin(&session("s1", Some(0))));
        assert!(crashes.latest.is_none());
    }

    #[test]
    fn a_fresh_crash_opens_the_modal_and_an_old_one_only_the_play_bar() {
        let mut crashes = Crashes::default();
        crashes.took(found("s1", Ok("exit c0000005".into()), true, false), Plan::Ask);
        assert!(crashes.view.is_some() && !crashes.open);
        assert!(crashes.shown().is_none());
        crashes.took(found("s2", Ok("exit c0000005".into()), true, true), Plan::Ask);
        assert_eq!(crashes.shown().map(|v| v.dir.clone()), Some(PathBuf::from("s2")));
    }

    #[test]
    fn a_clean_end_with_no_known_problem_shows_nothing() {
        let mut crashes = Crashes::default();
        crashes.took(found("s1", Ok(String::new()), false, true), Plan::Ask);
        assert!(crashes.view.is_none() && !crashes.open);
        assert_eq!(crashes.latest.map(|(d, _)| d), Some(PathBuf::from("s1")));
    }

    #[test]
    fn a_game_that_never_started_keeps_the_failure_for_the_play_bar() {
        let mut crashes = Crashes::default();
        crashes.took(found("s1", Err(StartFailure { code: 5 }), true, true), Plan::Ask);
        assert!(crashes.view.is_none());
        assert!(matches!(crashes.latest, Some((_, Err(StartFailure { code: 5 })))));
    }

    #[test]
    fn a_crash_in_a_room_shows_its_logs_on_their_way_then_sent() {
        let mut crashes = Crashes::default();
        let send = crashes.took(
            found("s1", Ok("exit c0000005".into()), true, true),
            Plan::Send(room()),
        );
        assert_eq!(send, Some(room()));
        assert_eq!(
            crashes.shown().and_then(|v| v.auto.clone()),
            Some(AutoSend::Sending)
        );
        crashes.sent(
            "s0".as_ref(),
            Outcome::Failed {
                why: "not this crash".into(),
            },
        );
        assert_eq!(
            crashes.shown().and_then(|v| v.auto.clone()),
            Some(AutoSend::Sending)
        );
        crashes.sent(
            "s1".as_ref(),
            Outcome::Sent {
                parts: 1,
                reference: Some("1311204".into()),
            },
        );
        assert_eq!(
            crashes.shown().and_then(|v| v.auto.clone()),
            Some(AutoSend::Sent {
                reference: Some("1311204".into())
            })
        );
    }

    #[test]
    fn a_crash_sent_before_shows_how_it_went_and_is_not_sent_again() {
        let mut crashes = Crashes::default();
        let send = crashes.took(
            found("s1", Ok("exit c0000005".into()), true, true),
            Plan::Done(Outcome::interrupted()),
        );
        assert_eq!(send, None);
        let auto = crashes.shown().and_then(|v| v.auto.clone()).unwrap();
        assert!(!auto.covers(), "a send that never finished gives Send Logs back");
    }

    #[test]
    fn a_crash_outside_a_room_shows_no_note() {
        let mut crashes = Crashes::default();
        assert_eq!(
            crashes.took(found("s1", Ok("exit c0000005".into()), true, true), Plan::Ask),
            None
        );
        assert_eq!(crashes.shown().map(|v| v.auto.clone()), Some(None));
    }

    #[test]
    fn a_send_from_the_modal_after_the_automatic_one_failed_stands_for_it() {
        let dir = std::env::temp_dir().join(format!("swofty-crash-by-hand-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let mut crashes = Crashes::default();
        let failed = Outcome::Failed {
            why: "Couldn't reach the log server: offline".into(),
        };
        crashes.took(
            found(dir.to_str().unwrap(), Ok("exit c0000005".into()), true, true),
            Plan::Done(failed),
        );
        assert_eq!(crashes.uncovered(), Some(dir.clone()));
        let sent = Sent {
            parts: 2,
            reference: Some("1311204".into()),
        };
        crashes.sent_by_hand(&dir, &sent);
        assert_eq!(
            crashes.shown().and_then(|v| v.auto.clone()),
            Some(AutoSend::Sent {
                reference: Some("1311204".into())
            })
        );
        assert_eq!(crashes.uncovered(), None);
        assert_eq!(
            Outcome::read(&dir),
            Some(Outcome::Sent {
                parts: 2,
                reference: Some("1311204".into())
            })
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_a_crash_whose_automatic_send_failed_takes_a_send_from_the_modal() {
        let mut crashes = Crashes::default();
        crashes.took(found("s1", Ok("exit c0000005".into()), true, true), Plan::Ask);
        assert_eq!(crashes.uncovered(), None, "a crash outside a room");
        crashes.took(
            found("s2", Ok("exit c0000005".into()), true, true),
            Plan::Send(room()),
        );
        assert_eq!(crashes.uncovered(), None, "a send on its way");
        crashes.took(
            found("s3", Ok("exit c0000005".into()), true, true),
            Plan::Done(Outcome::Sent {
                parts: 1,
                reference: None,
            }),
        );
        assert_eq!(crashes.uncovered(), None, "a crash already sent");
        crashes.open = false;
        crashes.took(
            found("s4", Ok("exit c0000005".into()), true, false),
            Plan::Done(Outcome::interrupted()),
        );
        assert_eq!(crashes.uncovered(), None, "a modal that is not open");
    }

    #[test]
    fn the_modal_shows_how_far_the_automatic_send_got_until_it_ends() {
        let mut crashes = Crashes::default();
        crashes.took(
            found("s1", Ok("exit c0000005".into()), true, true),
            Plan::Send(room()),
        );
        let waiting = Stage::Waiting { secs: 30 };
        crashes.sending = Some(("s1".into(), Arc::new(Mutex::new(waiting.clone()))));
        assert_eq!(crashes.stage("s1".as_ref()), Some(waiting.to_string()));
        assert_eq!(crashes.stage("s0".as_ref()), None);
        crashes.sent(
            "s0".as_ref(),
            Outcome::Failed {
                why: "not this crash".into(),
            },
        );
        assert!(crashes.sending.is_some());
        crashes.sent(
            "s1".as_ref(),
            Outcome::Sent {
                parts: 1,
                reference: None,
            },
        );
        assert!(crashes.sending.is_none());
        assert_eq!(crashes.stage("s1".as_ref()), None);
    }

    #[test]
    fn a_game_that_never_started_sends_nothing() {
        let mut crashes = Crashes::default();
        let send = crashes.took(
            found("s1", Err(StartFailure { code: 5 }), true, true),
            Plan::Send(room()),
        );
        assert_eq!(send, None);
        assert!(crashes.view.is_none());
    }
}
