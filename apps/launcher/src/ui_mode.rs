use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use swofty_core::session::UiMode;

use crate::shell::Shell;

impl Shell {
    pub(crate) fn start_ui_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.ui_mode.unwrap_or_default();
        crate::scale::apply(mode, window, cx);
        if !self.store.config().needs_onboarding() {
            return;
        }
        let wizard = crate::onboarding::Onboarding::new(mode, cx);
        window.focus(&wizard.focus, cx);
        self.onboarding = Some(wizard);
    }

    pub(crate) fn set_ui_mode(&mut self, mode: UiMode, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.ui_mode = Some(mode);
        self.save_config_toast(&config, window, cx);
        self.ui_mode = config.ui_mode;
        crate::scale::apply(mode, window, cx);
        cx.notify();
    }

    pub(crate) fn save_config_toast(
        &mut self,
        config: &swofty_core::session::Config,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = self.store.save_config(config) {
            crate::app_log::action("save settings").failed(&e);
            window.push_notification(Notification::error(e.to_string()), cx);
        }
    }
}
