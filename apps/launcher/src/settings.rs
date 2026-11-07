//! The Settings page: the launcher's own options, and where Swofty keeps its
//! files: the data folder's size by part, the room left on its drive and on the
//! drive F1 25 needs, and clearing old game session logs.

use std::time::SystemTime;

use crate::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::data_usage::{self, KEEP_FOR, KEEP_SESSIONS, Pruned, Usage};
use swofty_core::install::Source;
use swofty_core::session::UiMode;

use crate::pages::GUTTER;
use crate::shell::{Page, Shell};
use crate::theme::Palette;

/// How to move the data folder to another drive (the installer does it).
const MOVE_DOCS: &str = "https://f1.swofty.net/guide/installation#moving-the-data-folder";

#[derive(Default)]
pub struct Settings {
    pub usage: Option<Usage>,
    pub prunable: Pruned,
    pub measuring: bool,
    pub clearing: bool,
}

/// `bytes` the way Explorer shows them: GB from one gigabyte, else MB.
pub fn size(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{} GB", swofty_core::disk::gb(bytes))
    } else {
        format!("{} MB", bytes.div_ceil(1 << 20))
    }
}

/// Clears old session logs when the launcher starts, when the player turned
/// that on. Runs after an unfinished session was recovered; the newest sessions,
/// the last `KEEP_FOR` and anything written in the last hour always stay.
pub fn clear_logs_at_start(root: &std::path::Path) {
    let done = data_usage::prune_sessions(&root.join("sessions"), KEEP_SESSIONS, KEEP_FOR, SystemTime::now());
    if done.sessions > 0 {
        crate::app_log::note(&format!(
            "start: cleared {} old session logs ({})",
            done.sessions,
            size(done.bytes)
        ));
    }
}

/// Which drive is short of room and what that means, as Play and the Settings
/// page both say it, or `None` when both drives have enough.
pub fn low_space_warning(room: data_usage::Room) -> Option<String> {
    let need = size(data_usage::GAME_NEEDS);
    let game = room.game_low().map(|free| {
        if free < data_usage::GAME_NEEDS {
            format!(
                "Only {} free on {}. F1 25 won't start with less than {need} free there.",
                size(free),
                data_usage::GAME_DRIVE
            )
        } else {
            format!(
                "Only {} free on {}, and F1 25 needs {need} to start.",
                size(free),
                data_usage::GAME_DRIVE
            )
        }
    });
    let data = room.data_low().map(|free| {
        format!(
            "Only {} free on the drive with the data folder: installing or building addons may fail.",
            size(free)
        )
    });
    let lines: Vec<String> = game.into_iter().chain(data).collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

impl Shell {
    /// Measures the data folder again, off the UI thread.
    pub(crate) fn settings_measure(&mut self, cx: &mut Context<Self>) {
        if self.settings.measuring {
            return;
        }
        self.settings.measuring = true;
        let root = self.store.root.clone();
        cx.spawn(async move |this, cx| {
            let (usage, prunable) = cx
                .background_executor()
                .spawn(async move {
                    let usage = data_usage::measure(&root);
                    let old = data_usage::prunable(
                        &root.join("sessions"),
                        KEEP_SESSIONS,
                        KEEP_FOR,
                        SystemTime::now(),
                    );
                    (usage, old)
                })
                .await;
            this.update(cx, |s, cx| {
                s.settings.usage = Some(usage);
                s.settings.prunable = prunable;
                s.settings.measuring = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn confirm_clear_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.processes.is_empty() {
            window.push_notification(
                Notification::error("Close F1 25 first: it may still be writing its session log"),
                cx,
            );
            return;
        }
        let old = self.settings.prunable;
        if old.sessions == 0 {
            return;
        }
        let shell = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let shell = shell.clone();
            alert
                .width(px(480.))
                .title(format!("Delete {} old sessions ({})?", old.sessions, size(old.bytes)))
                .description(format!(
                    "Their logs are deleted for good. The newest {KEEP_SESSIONS} sessions and everything from the last {} days stay.",
                    KEEP_FOR.as_secs() / 86_400
                ))
                .ok_text("Delete sessions")
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    shell.update(cx, |s, cx| s.settings_clear_logs(window, cx)).ok();
                    true
                })
        });
    }

    fn settings_clear_logs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.processes.is_empty() {
            window.push_notification(
                Notification::error("Close F1 25 first: it may still be writing its session log"),
                cx,
            );
            return;
        }
        self.settings.clearing = true;
        let sessions = self.store.root.join("sessions");
        cx.spawn_in(window, async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    data_usage::prune_sessions(&sessions, KEEP_SESSIONS, KEEP_FOR, SystemTime::now())
                })
                .await;
            this.update_in(cx, |s, window, cx| {
                s.settings.clearing = false;
                crate::app_log::note(&format!(
                    "settings: cleared {} old session logs ({})",
                    done.sessions,
                    size(done.bytes)
                ));
                window.push_notification(
                    Notification::success(format!(
                        "Cleared {} old session logs and freed {}",
                        done.sessions,
                        size(done.bytes)
                    )),
                    cx,
                );
                s.settings_measure(cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn settings_page(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        if self.settings.usage.is_none() {
            self.settings_measure(cx);
        }
        let refresh = Button::new("settings-refresh")
            .outline()
            .icon(IconName::RefreshCw)
            .label("Refresh")
            .disabled(self.settings.measuring)
            .on_click(cx.listener(|this, _, _, cx| this.settings_measure(cx)));
        v_flex()
            .size_full()
            .child(self.header(
                Page::Settings.label(),
                "Launcher options, and where Swofty keeps its files".to_owned(),
                refresh,
                p,
            ))
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        v_flex()
                            .px(px(GUTTER))
                            .pb(px(24.))
                            .gap(px(16.))
                            .max_w(px(820.))
                            .child(self.settings_storage(p, cx))
                            .child(self.settings_general(p, cx)),
                    ),
            )
            .into_any_element()
    }

    fn settings_storage(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let root = self.store.root.clone();
        let mut rows: Vec<AnyElement> = Vec::new();
        let open = {
            let root = root.clone();
            Button::new("settings-open-data")
                .outline()
                .small()
                .icon(IconName::FolderOpen)
                .label("Open")
                .on_click(move |_, _, cx| cx.open_with_system(&crate::reveal::real_path(&root)))
        };
        rows.push(row(
            "Data folder",
            muted(root.display().to_string(), p),
            Some(open.into_any_element()),
            p,
        ));
        match &self.settings.usage {
            None => rows.push(row("Size", muted("Measuring…".to_owned(), p), None, p)),
            Some(u) => {
                let mut free = format!("{} free on this drive", u.free.map_or("?".to_owned(), size));
                if let Some(pf) = u.profile_free.filter(|pf| Some(*pf) != u.free) {
                    free += &format!(", {} on {}", size(pf), data_usage::GAME_DRIVE);
                }
                rows.push(row(
                    "Size",
                    muted(format!("{} in total · {free}", size(u.total)), p),
                    None,
                    p,
                ));
                let short = low_space_warning(data_usage::Room {
                    game: u.profile_free,
                    data: u.free,
                });
                if let Some(short) = short {
                    rows.push(
                        div()
                            .px(px(16.))
                            .py(px(10.))
                            .border_t_1()
                            .border_color(p.border_variant)
                            .text_size(px(13.))
                            .text_color(p.warning)
                            .child(format!(
                                "{short} Clear old logs, remove addons you don't use, or move the data folder."
                            ))
                            .into_any_element(),
                    );
                }
                let mut parts: Vec<(String, u64)> =
                    u.parts.iter().map(|x| (x.name.to_owned(), x.bytes)).collect();
                parts.push(("Other".to_owned(), u.other));
                parts.sort_by_key(|x| std::cmp::Reverse(x.1));
                let max = parts.first().map_or(1, |x| x.1.max(1));
                for (name, bytes) in parts.into_iter().filter(|x| x.1 > 0) {
                    let share = bytes as f32 / max as f32;
                    rows.push(row(
                        "",
                        h_flex()
                            .gap(px(12.))
                            .w_full()
                            .child(div().w(px(130.)).text_size(px(13.)).child(name))
                            .child(
                                div()
                                    .flex_1()
                                    .h(px(6.))
                                    .rounded_full()
                                    .bg(p.border_variant)
                                    .child(div().h_full().w(relative(share)).rounded_full().bg(p.accent)),
                            )
                            .child(
                                div()
                                    .w(px(70.))
                                    .text_size(px(13.))
                                    .text_color(p.text_muted)
                                    .child(size(bytes)),
                            )
                            .into_any_element(),
                        None,
                        p,
                    ));
                }
                let old = self.settings.prunable;
                let clear = Button::new("settings-clear-logs")
                    .outline()
                    .small()
                    .icon(IconName::Eraser)
                    .label(if old.sessions == 0 {
                        "Nothing to clear".to_owned()
                    } else {
                        format!("Clear {} old ({})", old.sessions, size(old.bytes))
                    })
                    .disabled(old.sessions == 0 || self.settings.clearing)
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_clear_logs(window, cx)));
                rows.push(row(
                    "Session logs",
                    muted(
                        format!(
                            "{} game sessions kept. Clearing keeps the newest {KEEP_SESSIONS} and everything from the \
                             last {} days.",
                            u.sessions,
                            KEEP_FOR.as_secs() / 86_400
                        ),
                        p,
                    ),
                    Some(clear.into_any_element()),
                    p,
                ));
            }
        }
        let how = Button::new("settings-move-data")
            .outline()
            .small()
            .icon(IconName::ExternalLink)
            .label("How to move it")
            .on_click(|_, _, cx| cx.open_url(MOVE_DOCS));
        rows.push(row(
            "Another drive",
            muted(
                format!(
                    "To keep addons {}, run the Swofty installer again and pick a folder under \"Keep addons, \
                     profiles and backups in\". It moves everything and the game keeps finding it.",
                    if cfg!(windows) { "off drive C" } else { "on another drive" }
                ),
                p,
            ),
            Some(how.into_any_element()),
            p,
        ));
        card("Storage", rows, p)
    }

    fn settings_general(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let start_label = match self.install.as_ref().map(|i| i.source) {
            Some(Source::EaApp) => "Start the EA app automatically",
            _ => "Start Steam automatically",
        };
        let start = Switch::new("settings-start-store")
            .checked(self.start_store)
            .small()
            .on_click(cx.listener(|this, on: &bool, window, cx| this.set_start_store(*on, window, cx)));
        let developer = Switch::new("settings-developer")
            .checked(self.developer_mode)
            .small()
            .on_click(cx.listener(|this, on: &bool, window, cx| this.set_developer(*on, window, cx)));
        let auto_clear = Switch::new("settings-auto-clear-logs")
            .checked(self.clear_old_session_logs)
            .small()
            .on_click(
                cx.listener(|this, on: &bool, window, cx| this.set_clear_old_session_logs(*on, window, cx)),
            );
        let tv = Switch::new("settings-tv-mode")
            .checked(self.ui_mode.is_some_and(UiMode::is_tv))
            .small()
            .on_click(cx.listener(|this, on: &bool, window, cx| {
                this.set_ui_mode(if *on { UiMode::Tv } else { UiMode::Pc }, window, cx)
            }));
        card(
            "General",
            vec![
                row(
                    "TV mode",
                    muted(
                        "Scales the whole launcher up and widens the scrollbars, for handhelds and couch play"
                            .to_owned(),
                        p,
                    ),
                    Some(tv.into_any_element()),
                    p,
                ),
                row(
                    start_label,
                    muted("When you press Play".to_owned(), p),
                    Some(start.into_any_element()),
                    p,
                ),
                row(
                    "Developer mode",
                    muted("Shows the Tools page with the swofty dev actions".to_owned(), p),
                    Some(developer.into_any_element()),
                    p,
                ),
                row(
                    "Clear old logs",
                    muted(
                        format!(
                            "Off by default. When on, the launcher clears old logs as it starts, as Clear old does: keeps the newest {KEEP_SESSIONS} sessions \
                             and the last {} days",
                            KEEP_FOR.as_secs() / 86_400
                        ),
                        p,
                    ),
                    Some(auto_clear.into_any_element()),
                    p,
                ),
            ],
            p,
        )
    }
}

fn muted(text: String, p: Palette) -> AnyElement {
    div()
        .text_size(px(13.))
        .text_color(p.text_muted)
        .child(text)
        .into_any_element()
}

fn row(title: &'static str, body: AnyElement, action: Option<AnyElement>, p: Palette) -> AnyElement {
    h_flex()
        .px(px(16.))
        .py(px(10.))
        .gap(px(12.))
        .items_center()
        .border_t_1()
        .border_color(p.border_variant)
        .when(!title.is_empty(), |d| {
            d.child(div().w(px(150.)).flex_shrink_0().text_size(px(13.)).child(title))
        })
        .child(div().flex_1().min_w_0().child(body))
        .when_some(action, |d, a| d.child(div().flex_shrink_0().child(a)))
        .into_any_element()
}

fn card(title: &'static str, rows: Vec<AnyElement>, p: Palette) -> AnyElement {
    v_flex()
        .rounded(px(6.))
        .border_1()
        .border_color(p.border)
        .child(
            div()
                .px(px(16.))
                .pt(px(14.))
                .pb(px(12.))
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title),
        )
        .children(rows)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{low_space_warning, size};
    use swofty_core::data_usage::{GAME_DRIVE, Room};

    const GB: u64 = 1 << 30;

    #[test]
    fn sizes_read_like_explorer() {
        assert_eq!(size(0), "0 MB");
        assert_eq!(size(1), "1 MB");
        assert_eq!(size(5 << 20), "5 MB");
        assert_eq!(size(3 << 29), "1.5 GB");
    }

    #[test]
    fn play_warns_only_when_a_drive_is_short() {
        let room = |game, data| Room { game, data };
        assert_eq!(low_space_warning(room(Some(50 * GB), Some(50 * GB))), None);
        assert_eq!(low_space_warning(room(None, None)), None);
        let refused = low_space_warning(room(Some(GB), Some(GB))).unwrap();
        assert!(
            refused.starts_with(&format!(
                "Only 1.0 GB free on {GAME_DRIVE}. F1 25 won't start with less than 2.0 GB free there."
            )),
            "{refused}"
        );
        assert!(
            !refused.contains("data folder"),
            "one drive is named once: {refused}"
        );
        let tight = low_space_warning(room(Some(5 * GB / 2), Some(50 * GB))).unwrap();
        assert!(tight.contains("and F1 25 needs 2.0 GB to start"), "{tight}");
        let data = low_space_warning(room(Some(50 * GB), Some(GB))).unwrap();
        assert!(
            data.starts_with("Only 1.0 GB free on the drive with the data folder"),
            "{data}"
        );
    }
}
