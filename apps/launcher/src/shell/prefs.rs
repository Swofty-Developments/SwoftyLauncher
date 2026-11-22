use crate::shell::notices::{Notification, Notify};
use gpui_kit::*;
use std::collections::BTreeSet;
use swofty_core::session::{Config, LaunchMode};

use super::Shell;
use swofty_i18n::trf;

pub(crate) struct Prefs {
    pub(crate) developer_mode: bool,
    /// `clear_old_session_logs` from config.json: opt-in, old session logs are cleared at start.
    pub(crate) clear_old_session_logs: bool,
    /// `time_trial_boards` from config.json: on unless turned off, uploads completed Time Trial laps.
    pub(crate) time_trial_boards: bool,
    pub(crate) use_profiles: bool,
    /// `time_trial_ingame` from config.json: on unless turned off, shows the boards inside Time Trial.
    pub(crate) time_trial_ingame: bool,
    /// `home_video` from config.json: on unless turned off, shows the latest video card on the F1 25 home screen.
    pub(crate) home_video: bool,
    /// `update_off_addons` from config.json: opt-in, the play bar also updates addons that are off.
    pub(crate) update_off_addons: bool,
    /// `send_room_crash_logs` from config.json: on unless turned off, a crash in a room sends its logs by itself.
    pub(crate) send_room_crash_logs: bool,
    /// `updates.check` from config.json, read when the library loads.
    pub(crate) update_check: bool,
    pub(crate) start_store: bool,
    pub(crate) vr_profiles: BTreeSet<String>,
}

impl Prefs {
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            developer_mode: config.developer,
            clear_old_session_logs: config.clear_old_session_logs,
            time_trial_boards: config.time_trial_boards,
            use_profiles: config.use_profiles,
            time_trial_ingame: config.time_trial_ingame,
            home_video: config.home_video,
            update_off_addons: config.update_off_addons,
            send_room_crash_logs: config.send_room_crash_logs,
            update_check: config.updates.check,
            start_store: config.launch.start_store,
            vr_profiles: config.vr_profiles.clone(),
        }
    }

    /// The crash-in-a-room setting Settings shows: only to a player with multiplayer access. It keeps its value
    /// while hidden.
    pub(crate) fn room_crash_logs_shown(&self, allowed: bool) -> Option<bool> {
        allowed.then_some(self.send_room_crash_logs)
    }
}

impl Shell {
    pub(crate) fn set_launcher_saves(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.library.profile.name.clone();
        match self.store.update_profile(&name, |p| {
            p.launcher_saves = on;
            Ok(())
        }) {
            Ok(()) => {
                let ea = self
                    .game
                    .install
                    .as_ref()
                    .is_some_and(|i| i.source == swofty_core::install::Source::EaApp);
                crate::app::app_log::note(&format!(
                    "saves: profile {name} now plays on {}",
                    match (on, ea) {
                        (true, _) => "its launcher saves",
                        (false, true) => "the EA app copy's own saves",
                        (false, false) => "Steam's saves",
                    }
                ));
                self.library.profile.launcher_saves = on;
                window.notify(
                    Notification::info(match (on, ea) {
                        (true, _) => trf!("{name} plays on its own launcher saves.", name = name),
                        (false, true) => trf!("{name} plays on the EA app copy's own saves.", name = name),
                        (false, false) => trf!("{name} plays on your Steam saves.", name = name),
                    }),
                    cx,
                );
            }
            Err(e) => {
                crate::app::app_log::action("save profile").failed(&e);
                window.notify(Notification::error(e.to_string()), cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_start_store(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.launch.start_store = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.start_store = config.launch.start_store;
        cx.notify();
    }

    pub(crate) fn set_clear_old_session_logs(
        &mut self,
        on: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut config = self.store.config();
        config.clear_old_session_logs = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.clear_old_session_logs = config.clear_old_session_logs;
        cx.notify();
    }

    pub(crate) fn set_use_profiles(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.use_profiles = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.use_profiles = config.use_profiles;
        if !on {
            let active = self.library.profile.name.clone();
            if let Some(name) = crate::dialogs::profiles::implicit_profile(&self.library.profiles, &active) {
                self.switch_profile(&name, window, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn set_time_trial_boards(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.time_trial_boards = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.time_trial_boards = config.time_trial_boards;
        cx.notify();
    }

    pub(crate) fn set_time_trial_ingame(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.time_trial_ingame = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.time_trial_ingame = config.time_trial_ingame;
        cx.notify();
    }

    pub(crate) fn set_home_video(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.home_video = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.home_video = config.home_video;
        cx.notify();
    }

    pub(crate) fn set_update_off_addons(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.update_off_addons = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.update_off_addons = config.update_off_addons;
        cx.notify();
    }

    pub(crate) fn set_send_room_crash_logs(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.send_room_crash_logs = on;
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.send_room_crash_logs = config.send_room_crash_logs;
        cx.notify();
    }

    pub(crate) fn set_launch_mode(&mut self, mode: LaunchMode, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.set_launch_mode(&self.library.profile.name, mode);
        if let Err(e) = self.store.save_config(&config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
        self.prefs.vr_profiles = config.vr_profiles;
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, Prefs};

    #[test]
    fn the_crash_in_a_room_setting_shows_only_with_multiplayer_access_and_keeps_its_value() {
        let mut config = Config::default();
        assert_eq!(
            Prefs::from_config(&config).room_crash_logs_shown(true),
            Some(true)
        );
        assert_eq!(Prefs::from_config(&config).room_crash_logs_shown(false), None);
        config.send_room_crash_logs = false;
        let prefs = Prefs::from_config(&config);
        assert_eq!(prefs.room_crash_logs_shown(false), None);
        assert!(!prefs.send_room_crash_logs);
        assert_eq!(prefs.room_crash_logs_shown(true), Some(false));
    }
}
