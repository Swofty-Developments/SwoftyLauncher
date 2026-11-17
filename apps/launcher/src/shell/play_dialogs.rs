use crate::services::game::{self};
use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::px;
use crate::ui::theme::palette;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::*;
use swofty_core::platform::process::stop_game_in;

use super::Shell;
use super::launch_alerts::offline_terms;
use swofty_i18n::tr;

impl Shell {
    /// First launch: the offline play terms, shown as a dialog on Play.
    /// Accepting records consent and launches; declining changes nothing.
    pub(super) fn confirm_missing_rows(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let shell = shell.clone();
            alert
                .width(px(520.))
                .title(tr("Careers that need addons"))
                .description(text.clone())
                .ok_text(tr("Play anyway"))
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    window.close_dialog(cx);
                    shell
                        .update(cx, |s, cx| {
                            s.launching.play_anyway = true;
                            s.play(window, cx);
                        })
                        .ok();
                    false
                })
        });
    }

    pub(crate) fn confirm_offline_play(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let store = self.store.clone();
        let p = palette(cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, store) = (shell.clone(), store.clone());
            alert
                .width(px(560.))
                .title(tr("Modded play runs offline"))
                .child(offline_terms(p))
                .ok_text(tr("Accept and launch"))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    if let Err(e) = swofty_core::offline::record_accept(&store.root) {
                        crate::app::app_log::action("accept offline play").failed(&e);
                        window.notify(Notification::error(e.to_string()), cx);
                        return false;
                    }
                    window.close_dialog(cx);
                    shell.update(cx, |s, cx| s.play(window, cx)).ok();
                    false
                })
        });
    }

    pub(crate) fn restore_game_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(install) = self.game.install.clone() else {
            return;
        };
        let store = self.store.clone();
        self.launching.notice = None;
        self.game.blocked = None;
        self.launching.game_edited = None;
        self.launching.restoring = true;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { game::restore(&store, install.dir) })
                .await;
            this.update_in(cx, |s, window, cx| {
                s.launching.restoring = false;
                match result {
                    Ok(text) => {
                        window.notify(Notification::success(text).title(tr("Game folder restored")), cx)
                    }
                    Err(e) => {
                        crate::app::app_log::action("restore the game folder").failed(&e);
                        window.notify(Notification::error(e).title(tr("Restore failed")), cx)
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn cancel_build(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let data = self.store.root.clone();
        let this = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (data, this) = (data.clone(), this.clone());
            alert
                .title(tr("Cancel the build?"))
                .description(tr(
                    "Swofty stops after the file it is on and puts the game files back. \
                     Your profile and addons stay as they are, and the next Play builds again.",
                ))
                .ok_text(tr("Cancel build"))
                .cancel_text(tr("Keep building"))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    match swofty_core::game_folder::busy::request_cancel(&data) {
                        Ok(()) => {
                            this.update(cx, |s, cx| {
                                s.launching.cancelling = true;
                                cx.notify();
                            })
                            .ok();
                        }
                        Err(e) => {
                            crate::app::app_log::action("cancel the build").failed(&e);
                            window.notify(Notification::error(e.to_string()), cx)
                        }
                    }
                    true
                })
        });
    }

    pub(crate) fn confirm_stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dir) = self.game.install.as_ref().map(|i| i.dir.clone()) else {
            return;
        };
        let data = self.store.root.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (dir, data) = (dir.clone(), data.clone());
            alert
                .title(tr("Stop F1 25?"))
                .description(tr("The game closes immediately. Unsaved progress is lost."))
                .ok_text(tr("Stop game"))
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    swofty_core::session::request_stop(&data);
                    if let Err(e) = stop_game_in(&dir) {
                        crate::app::app_log::action("stop f1 25").failed(&e);
                        window.notify(Notification::error(e.to_string()), cx);
                    }
                    true
                })
        });
    }
}
