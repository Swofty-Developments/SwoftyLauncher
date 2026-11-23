//! The status line beside Play, and the save area under it.

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf, trn};

use crate::services::game::{self, Owner};
use crate::ui::format::{ago, duration};

use super::{launch_failed, link, status_row};
use crate::shell::{PlayState, Shell};
use crate::ui::theme::Palette;
use swofty_core::install::Source;

impl Shell {
    pub(super) fn status(&self, state: &PlayState, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let dot = |c: Hsla| div().size(px(7.)).rounded_full().bg(c).flex_shrink_0();
        let row = status_row();
        if self.launching.game_edited.is_some() {
            return row
                .text_color(p.warning)
                .child(Icon::new(IconName::TriangleAlert).size(px(16.)))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(tr("Your game files were edited")),
                )
                .child(
                    link("game-edited-what", tr("What to do"), p)
                        .on_click(cx.listener(|this, _, window, cx| this.show_game_edited(window, cx))),
                )
                .when_some(self.launch_log(), |d, log| {
                    d.child(
                        link("launch-log", tr("View log"), p)
                            .on_click(move |_, _, cx| cx.open_with_system(&log)),
                    )
                });
        }
        if let Some(notice) = &self.launching.notice {
            return launch_failed(row, notice, p).when_some(self.launch_log(), |d, log| {
                d.child(
                    link("launch-log", tr("View log"), p).on_click(move |_, _, cx| cx.open_with_system(&log)),
                )
            });
        }
        if let Some(row) = self.update_status(self.update_gate(state), status_row(), p, cx) {
            return row;
        }
        if let Some(row) = self.addon_update_status(state, status_row(), p, cx) {
            return row;
        }
        let rebuild = !self.library.profile.is_vanilla() && !self.library.rebuild.is_empty();
        match state {
            PlayState::Running => row,
            PlayState::Elsewhere(owner) => {
                let text = match owner {
                    Owner::Other(i) => match i.source {
                        Source::Steam => tr("F1 25 from Steam is running"),
                        Source::EaApp => tr("F1 25 from the EA app is running"),
                        Source::Unknown => tr("F1 25 from another folder is running"),
                    },
                    _ => tr(
                        "F1 25 is running from a folder the launcher doesn't know · close it to play this copy",
                    ),
                };
                let row = row
                    .text_color(p.text_muted)
                    .child(dot(p.warning))
                    .child(div().min_w_0().truncate().child(text));
                match owner {
                    Owner::Other(i) => {
                        let dir = i.dir.clone();
                        row.child(link("switch-to-running", tr("Switch to it"), p).on_click(
                            cx.listener(move |this, _, window, cx| this.use_copy(dir.clone(), window, cx)),
                        ))
                    }
                    _ => row,
                }
            }
            PlayState::Restoring => row.child(dot(p.warning)).child(tr("Restoring the game files…")),
            PlayState::Building => {
                let Some(b) = &self.game.build else {
                    return row;
                };
                let text = match self.launching.cancelling {
                    true => tr("Cancelling the build…").to_owned(),
                    false => format!(
                        "{} for {} · {}",
                        crate::ui::format::capitalised(&b.progress()),
                        b.profile,
                        duration((b.minutes() * 60).max(1))
                    ),
                };
                row.child(dot(p.warning))
                    .child(div().min_w_0().truncate().child(text))
                    .when_some(self.wrapper_log(), |d, log| {
                        d.child(
                            link("launch-log", tr("Log"), p)
                                .on_click(move |_, _, cx| cx.open_with_system(&log)),
                        )
                    })
            }
            PlayState::Launching => row
                .child(dot(p.warning))
                .child(format!(
                    "{} {}",
                    match self.launching.launch.as_ref().and_then(|l| l.stage.as_deref()) {
                        Some(stage) => stage.to_owned(),
                        None if !self.prefs.use_profiles && self.launch_mode().is_vr() => {
                            tr("Starting in VR…").to_owned()
                        }
                        None if !self.prefs.use_profiles => tr("Starting…").to_owned(),
                        None if self.launch_mode().is_vr() => trf!(
                            "Starting with profile {name} in VR…",
                            name = self.library.profile.name
                        ),
                        None => trf!("Starting with profile {name}…", name = self.library.profile.name),
                    },
                    self.launching
                        .launch
                        .as_ref()
                        .map(|l| duration(l.since.elapsed().as_secs()))
                        .unwrap_or_default()
                ))
                .when_some(self.launch_log(), |d, log| {
                    d.child(
                        link("launch-log", tr("Log"), p).on_click(move |_, _, cx| cx.open_with_system(&log)),
                    )
                }),
            PlayState::Crashed(session) => {
                let dir = session.dir.clone();
                let ending = self
                    .crashes
                    .latest
                    .as_ref()
                    .filter(|(d, _)| *d == session.dir)
                    .map(|(_, e)| e);
                let open =
                    link("crash-log", tr("Open logs"), p).on_click(move |_, _, cx| cx.open_with_system(&dir));
                let dismiss = link("dismiss", tr("Dismiss"), p).on_click(cx.listener(|this, _, _, cx| {
                    this.crashes.dismissed = this.game.session.as_ref().map(|s| s.dir.clone());
                    cx.notify();
                }));
                if let Some(Err(failure)) = ending {
                    return launch_failed(row, failure, p).child(open).child(dismiss);
                }
                let view = self.crashes.view.as_ref().filter(|v| v.dir == session.dir);
                let details = view.map(|_| {
                    link("crash-details", tr("What happened"), p).on_click(cx.listener(|this, _, _, cx| {
                        this.crashes.open = true;
                        cx.notify();
                    }))
                });
                let summary = view
                    .map(|v| v.headline())
                    .or_else(|| ending.and_then(|e| e.as_ref().ok()).cloned())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| tr("reading the log…").into());
                let crashed = trf!(
                    "Crashed {when}",
                    when = session
                        .ended
                        .map(|e| ago(game::now_secs() - e))
                        .unwrap_or_default()
                );
                row.items_start()
                    .child(div().mt(px(6.)).child(dot(p.error)))
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                h_flex()
                                    .gap(px(12.))
                                    .child(
                                        div()
                                            .text_color(p.error)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(crashed),
                                    )
                                    .children(details)
                                    .child(open)
                                    .child(dismiss),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.text_muted)
                                    .truncate()
                                    .child(summary),
                            ),
                    )
            }
            PlayState::NotInstalled => row.text_color(p.text_muted).child(dot(p.text_placeholder)).child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(tr("Install it with Steam or the EA app, or locate its folder")),
            ),
            PlayState::Detecting => row.text_color(p.text_muted).child(tr("Detecting F1 25…")),
            PlayState::Checking => row.text_color(p.text_muted).child(tr("Checking conflicts…")),
            PlayState::Preparing => row.child(dot(p.warning)).child(tr("Checking addons…")),
            PlayState::Ready => {
                if let Some(n) = self.game.install.as_ref().and_then(|i| {
                    crate::shell::play_bar::unsupported_build::notice(&i.dir.join(swofty_core::GAME_EXE))
                }) {
                    return row
                        .items_start()
                        .child(div().mt(px(6.)).child(dot(p.warning)))
                        .child(
                            v_flex()
                                .min_w_0()
                                .gap(px(2.))
                                .child(div().min_w_0().truncate().child(n.headline))
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(p.text_muted)
                                        .truncate()
                                        .child(n.detail),
                                ),
                        );
                }
                if rebuild {
                    let changes = trn(
                        self.library.rebuild.len(),
                        "{count} changed setting",
                        "{count} changed settings",
                    );
                    let text = match self.prefs.use_profiles {
                        true => trf!(
                            "Ready · rebuilds {profile} for {changes} on launch",
                            profile = self.library.profile.name,
                            changes = changes
                        ),
                        false => trf!("Ready · rebuilds for {changes} on launch", changes = changes),
                    };
                    return row
                        .text_color(p.text_muted)
                        .child(dot(p.warning))
                        .child(div().min_w_0().truncate().child(text));
                }
                if let Some(refused) = self.saves_refused() {
                    let dir = refused.dir.clone();
                    return row
                        .child(dot(p.warning))
                        .child(div().min_w_0().truncate().child(tr(
                            "Launcher saves could not be used, so F1 25 closed before loading a save",
                        )))
                        .child(
                            link(
                                "steam-once",
                                super::save_area::play_once_label(
                                    self.game.install.as_ref().map(|i| i.source),
                                ),
                                p,
                            )
                            .on_click(cx.listener(|this, _, window, cx| {
                                crate::shell::Launching::play_with_steam_saves(this, window, cx)
                            })),
                        )
                        .child(
                            link("saves-log", tr("Open logs"), p)
                                .on_click(move |_, _, cx| cx.open_with_system(&dir)),
                        )
                        .child(link("saves-dismiss", tr("Dismiss"), p).on_click(cx.listener(
                            |this, _, _, cx| {
                                this.crashes.dismissed = this.game.session.as_ref().map(|s| s.dir.clone());
                                cx.notify();
                            },
                        )));
                }
                if let Some(lost) = self.saves.banner.as_ref().and_then(|b| b.lost()) {
                    let names = lost.names().join("; ");
                    let (source, modes) = (lost.source().to_owned(), lost.modes().to_vec());
                    let (generation, at) = (lost.generation(), lost.kept_at());
                    return row
                        .child(dot(p.warning))
                        .child(div().min_w_0().truncate().child(trf!(
                            "Careers missing from the save the game read: {names}. Saved copy: {at}",
                            names = names,
                            at = swofty_core::saves::vault::human(at)
                        )))
                        .child(link("career-restore", tr("Restore"), p).on_click(cx.listener(
                            move |this, _, window, cx| {
                                for mode in &modes {
                                    this.restore_mode(&source, mode, generation, window, cx);
                                }
                            },
                        )))
                        .child(
                            link("career-open", tr("Open Career saves"), p)
                                .on_click(cx.listener(|this, _, _, cx| this.open_saves(None, cx))),
                        );
                }
                if self.saves_await_import() {
                    return row.text_color(p.text_muted).child(dot(p.warning)).child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(crate::features::career_saves::empty_save_short()),
                    );
                }
                row
            }
        }
    }

    pub(super) fn save_area(&self, state: &PlayState, p: Palette) -> Option<AnyElement> {
        if !matches!(
            state,
            PlayState::Ready | PlayState::Crashed(_) | PlayState::Preparing
        ) {
            return None;
        }
        let install = self.game.install.as_ref()?;
        let blocked = swofty_core::build_support::blocked(&install.dir.join(swofty_core::GAME_EXE));
        let line = crate::shell::play_bar::save_area::line(
            &self.library.profile,
            install.source,
            blocked.as_deref(),
            self.launching.steam_saves_once,
        );
        let colour = if line.warn { p.warning } else { p.text_muted };
        Some(
            v_flex()
                .id("save-area")
                .flex_shrink_0()
                .max_w(px(330.))
                .min_w_0()
                .gap(px(1.))
                .text_size(px(12.))
                .text_color(colour)
                .child(div().min_w_0().truncate().child(line.headline))
                .when_some(line.reason, |d, reason| {
                    d.child(div().min_w_0().truncate().text_color(p.text_muted).child(reason))
                })
                .into_any_element(),
        )
    }

    pub(super) fn saves_await_import(&self) -> bool {
        self.saves
            .view
            .as_ref()
            .is_some_and(|v| v.awaiting_import_for(&self.library.profile.name))
    }

    pub(super) fn wrapper_log(&self) -> Option<std::path::PathBuf> {
        let log = self.store.root.join("state/wrapper-run.log");
        log.is_file().then_some(log)
    }

    pub(super) fn launch_log(&self) -> Option<std::path::PathBuf> {
        let log = self.store.root.join("state/launcher-run.log");
        log.is_file().then_some(log)
    }
}
