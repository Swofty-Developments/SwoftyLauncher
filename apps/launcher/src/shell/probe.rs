use crate::dialogs::crash::{CrashChange, CrashNeeds, Crashes};
use crate::dialogs::game_edited::Edited;
use crate::services::game::{self, Copies, Install, Owner, Probe};
use crate::shell::notices::{Notification, Notify};
use gpui_kit::*;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use swofty_core::game_folder::busy::Busy;
use swofty_core::game_folder::recovery::{self, Blocked};
use swofty_core::platform::process::GameProcess;
use swofty_core::session::SessionRecord;

use super::host::Host;
use super::launch_alerts::{launch_headline, show_integrity_missing, show_launch_failed, show_mods_blocked};
use super::{Page, Shell};
use swofty_i18n::{tr, trf};

pub(crate) struct GameWatch {
    pub(crate) install: Option<Install>,
    pub(crate) copies: Vec<Install>,
    pub(crate) missing_copies: Vec<PathBuf>,
    pub(crate) detected: bool,
    pub(crate) rediscover: bool,
    pub(crate) processes: Vec<GameProcess>,
    pub(crate) owners: Vec<Owner>,
    pub(crate) session: Option<SessionRecord>,
    pub(crate) build: Option<Busy>,
    pub(crate) blocked: Option<Blocked>,
}

impl GameWatch {
    pub(crate) fn new() -> Self {
        Self {
            install: None,
            copies: Vec::new(),
            missing_copies: Vec::new(),
            detected: false,
            rediscover: true,
            processes: Vec::new(),
            owners: Vec::new(),
            session: None,
            build: None,
            blocked: None,
        }
    }

    /// Takes the copies a probe found. True when Play's copy or the list of copies changed.
    pub(crate) fn take_copies(&mut self, found: Copies) -> bool {
        let changed = !self.detected || self.install != found.active || self.copies != found.all;
        self.install = found.active;
        crate::app::app_log::set_game(self.install.as_ref().map(|i| i.dir.as_path()));
        self.copies = found.all;
        self.missing_copies = found.missing;
        self.detected = true;
        changed
    }

    pub(crate) fn take_processes(&mut self, build: Option<Busy>, processes: Vec<GameProcess>) {
        self.build = build;
        self.processes = processes;
    }

    /// Whether the next probe should look for copies again, clearing the request.
    pub(crate) fn take_rediscover(&mut self) -> bool {
        std::mem::take(&mut self.rediscover)
    }

    /// The copy addon panels work with: the one a running game belongs to,
    /// else the one Play uses.
    pub(crate) fn panel_install(&self) -> Option<Install> {
        self.owners
            .iter()
            .find_map(|o| match o {
                Owner::Other(i) => Some(i.clone()),
                _ => None,
            })
            .or_else(|| self.install.clone())
    }

    pub(crate) fn attribute(&mut self) {
        self.owners = self
            .processes
            .iter()
            .map(|g| game::owner(g, self.install.as_ref(), &self.copies))
            .collect();
    }
}

/// Times shown as "5 min ago" stay current without a repaint every probe.
const REPAINT: Duration = Duration::from_secs(30);

impl Shell {
    pub(super) fn apply(&mut self, probe: Probe, window: &mut Window, cx: &mut Context<Self>) {
        let started = Instant::now();
        self.apply_probe(probe, window, cx);
        self.note_slow("probe", self.page_name(), started);
    }

    pub(super) fn page_name(&self) -> &'static str {
        match self.navigation.page {
            Page::Section(_) => "panel",
            page => page.nav_label(),
        }
    }

    pub(super) fn note_slow(&mut self, what: &str, page: &str, started: Instant) {
        let took = started.elapsed();
        if took > Duration::from_millis(100) && !self.slow_logged {
            self.slow_logged = true;
            crate::app::app_log::note(&format!(
                "ui: {what} took {} ms on the UI thread while on the {page} page",
                took.as_millis()
            ));
        }
    }

    pub(super) fn apply_probe(&mut self, mut probe: Probe, window: &mut Window, cx: &mut Context<Self>) {
        let before = (
            self.requests,
            self.game.processes.clone(),
            self.game.session.clone(),
        );
        let was_live = self.launching.launch.is_some();
        self.requests = probe.requests;
        let root = self.store.root.clone();
        if let Some(banner) = probe.banner.take()
            && probe.banner_profile == self.library.profile.name
        {
            self.saves.banner = banner;
            self.saves.banner_profile = probe.banner_profile.clone();
        }
        self.saves.game_running = !probe.processes.is_empty();
        if self
            .saves
            .refresh_due(self.navigation.page == Page::Careers, probe.saves_print)
        {
            crate::features::career_saves::SavesPage::refresh(self, cx);
        }
        if probe.saves_marker && self.saves.take_notice(&root) {
            self.show_saves_notice(window, cx);
        }
        self.announce_reports(&probe, window, cx);
        self.apply_install(probe.install.take(), probe.merged, cx);
        if self
            .launching
            .cancel_settled(self.game.build.is_some(), probe.busy.is_some())
        {
            window.notify(
                Notification::info(tr(
                    "The game files are back as they were. Press Play to build again.",
                ))
                .title(tr("Build cancelled")),
                cx,
            );
        }
        self.game
            .take_processes(probe.busy.take(), std::mem::take(&mut probe.processes));
        self.sync_shaders(false, cx);
        self.game.attribute();
        self.finish_launch(&probe);
        Crashes::watch(self, probe.session.as_ref(), cx);
        self.game.session = probe.session;
        self.db_poll(cx);
        let live = was_live || self.launching.launch.is_some();
        if live
            || before
                != (
                    self.requests,
                    self.game.processes.clone(),
                    self.game.session.clone(),
                )
            || self.painted.elapsed() >= REPAINT
        {
            cx.notify();
        }
    }

    fn announce_reports(&mut self, probe: &Probe, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(n) = probe.recovered.as_ref().filter(|n| n.at > self.seen.recovered) {
            self.seen.recovered = n.at;
            let mut text: Vec<String> = n.lines.iter().take(4).cloned().collect();
            if n.lines.len() > text.len() {
                text.push(trf!(
                    "and {count} more; the launch log has them all",
                    count = n.lines.len() - text.len()
                ));
            }
            window.notify(Notification::info(text.join("\n")).title(recovery::HEADLINE), cx);
        }
        if let Some(r) = probe
            .mods_blocked
            .as_ref()
            .filter(|r| r.at > self.seen.mods_blocked)
        {
            self.seen.mods_blocked = r.at;
            show_mods_blocked(r, cx.entity().downgrade(), window, cx);
        }
        if let Some(r) = probe.not_stock.as_ref().filter(|r| r.at > self.seen.not_stock) {
            self.seen.not_stock = r.at;
            self.launching.game_edited = Some(Edited::NotStock(r.clone()));
            self.show_game_edited(window, cx);
        }
        if let Some(b) = probe
            .blocked
            .as_ref()
            .filter(|_| probe.blocked_at.is_some_and(|at| at > self.seen.blocked))
        {
            self.seen.blocked = probe.blocked_at.expect("blocked file timestamp");
            self.game.blocked = Some(b.clone());
            self.launching.game_edited = Some(Edited::Leftovers(b.clone()));
            self.show_game_edited(window, cx);
        }
        if let Some(f) = probe
            .launch_failed
            .as_ref()
            .filter(|f| f.at > self.seen.launch_failed)
        {
            self.seen.launch_failed = f.at;
            self.launching.notice = Some(launch_headline(f));
            show_launch_failed(f, window, cx);
        }
        if let Some(n) = probe
            .integrity_missing
            .as_ref()
            .filter(|n| n.at > self.seen.integrity_missing)
        {
            self.seen.integrity_missing = n.at;
            show_integrity_missing(n, window, cx);
        }
    }

    fn apply_install(&mut self, found: Option<Copies>, merged: bool, cx: &mut Context<Self>) {
        if let Some(found) = found {
            let changed = self.game.take_copies(found);
            if self.navigation.page == Page::Careers && (changed || merged) {
                crate::features::career_saves::SavesPage::refresh(self, cx);
            }
            if merged {
                self.reload(cx);
            } else if changed {
                crate::features::sections::AddonViews::refresh(self, cx);
                self.db_profile_changed(cx);
            }
        } else if merged {
            self.reload(cx);
        }
    }

    fn finish_launch(&mut self, probe: &Probe) {
        if let Some(failed) = self.launching.follow(&mut self.game, &self.store, probe) {
            self.rooms.launch_ended(failed);
        }
    }
}

impl Host<Crashes> for Shell {
    fn split(&mut self) -> (&mut Crashes, CrashNeeds<'_>) {
        let room = self.rooms.crash_room();
        (
            &mut self.crashes,
            CrashNeeds {
                store: &self.store,
                install: self.game.install.as_ref(),
                running: !self.game.processes.is_empty(),
                room,
                allowed: self.account.allowed(),
                send_room_crash_logs: self.prefs.send_room_crash_logs,
            },
        )
    }

    fn changed(&mut self, change: CrashChange, _: &mut Context<Self>) {
        match change {
            CrashChange::GameEnded(end) => {
                self.rooms
                    .game_exited(end.exit_code, end.stopped_by_launcher, end.crashed)
            }
        }
    }
}
