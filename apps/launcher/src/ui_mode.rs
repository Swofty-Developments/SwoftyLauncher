use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{WindowExt, v_flex};
use gpui_kit::*;
use swofty_core::session::UiMode;

use crate::scale::px;
use crate::shell::Shell;
use crate::theme::palette;

impl Shell {
    pub(crate) fn start_ui_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::scale::apply(self.ui_mode.unwrap_or_default(), window, cx);
        if self.ui_mode.is_none() {
            self.ask_ui_mode(window, cx);
        }
    }

    pub(crate) fn set_ui_mode(&mut self, mode: UiMode, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.ui_mode = Some(mode);
        if let Err(e) = self.store.save_config(&config) {
            crate::app_log::action("save settings").failed(&e);
            window.push_notification(Notification::error(e.to_string()), cx);
        }
        self.ui_mode = config.ui_mode;
        crate::scale::apply(mode, window, cx);
        cx.notify();
    }

    fn ask_ui_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let p = palette(cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (ok, cancel) = (shell.clone(), shell.clone());
            alert
                .width(px(560.))
                .title("PC or TV mode?")
                .keyboard(false)
                .close_button(false)
                .child(
                    v_flex()
                        .gap(px(8.))
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .child(
                            "TV mode makes everything larger and the scrollbars wider, for handhelds and \
                         playing from the couch. PC mode is the normal layout. You can change this \
                         later in Settings.",
                        ),
                )
                .ok_text("TV mode")
                .cancel_text("PC mode")
                .cancel_variant(ButtonVariant::Primary)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    ok.update(cx, |s, cx| s.set_ui_mode(UiMode::Tv, window, cx)).ok();
                    true
                })
                .on_cancel(move |_, window, cx| {
                    cancel
                        .update(cx, |s, cx| s.set_ui_mode(UiMode::Pc, window, cx))
                        .ok();
                    true
                })
        });
    }
}
