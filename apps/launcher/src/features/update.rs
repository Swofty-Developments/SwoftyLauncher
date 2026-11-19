use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use semver::Version;
use swofty_core::platform::app_control::Block;
use swofty_core::store::Store;
use swofty_core::updates::managed;
use swofty_core::updates::release::{self as update, Install, Release};
use swofty_core::updates::updater::{self, Catalog, Gate, Part, Phase, Plan, Ran, Track};

use crate::features::sections::{Reporter, Shared};
use crate::shell::host::{Host, Hosted};
use crate::shell::play_bar::link;
use crate::shell::{PlayState, Shell};
use crate::ui::theme::Palette;
use swofty_i18n::{tr, trf};

const TICK: Duration = Duration::from_secs(5);
const RESTART_DELAY: Duration = Duration::from_millis(1500);
pub(crate) const BUILDING: &str =
    "Swofty is still building your mods. Update when it finishes, or cancel the build.";
pub const BUTTON_WIDTH: f32 = 204.;
const MOVE_DOCS: &str = "https://f1.swofty.net/guide/installation#moving-your-addons-and-saves";

pub struct Updates {
    pub copy: Result<Install, String>,
    pub launcher: Track,
    pub addon: Catalog,
    pub addon_installed: Option<Version>,
    pub phase: Phase,
    pub checking: bool,
    pub adding: bool,
    pub progress: Option<(f32, String)>,
    pub restarting: Vec<(Part, Version)>,
    pub restarts: bool,
    pub expected: Option<String>,
    pub held: Option<String>,
    pub followed_up: bool,
    pub blocked_logged: Option<Version>,
}

impl Updates {
    pub fn new() -> Self {
        let copy = Install::current();
        if let Ok(i) = &copy
            && i.staging().exists()
        {
            let i = i.clone();
            std::thread::spawn(move || {
                for _ in 0..3 {
                    if i.cleanup() {
                        break;
                    }
                    std::thread::sleep(Duration::from_secs(5));
                }
            });
        }
        Self {
            copy,
            launcher: Track::default(),
            addon: Catalog::default(),
            addon_installed: None,
            phase: Phase::Idle,
            checking: false,
            adding: false,
            progress: None,
            restarting: Vec::new(),
            restarts: true,
            expected: std::env::var(updater::UPDATED_ENV).ok().filter(|v| !v.is_empty()),
            held: None,
            followed_up: false,
            blocked_logged: None,
        }
    }

    /// The updates at the launcher's start, with a line in launcher-app.log on whether this copy can update
    /// itself.
    pub fn started() -> Self {
        let u = Self::new();
        crate::app::app_log::scrubbed(&updater::copy_line(&std::env::current_exe(), &u.copy));
        u
    }

    fn launcher_offer(&self) -> Option<&Release> {
        self.copy
            .is_ok()
            .then(|| self.launcher.offer(Some(&update::current_version())))
            .flatten()
    }

    pub fn plan(&self) -> Plan {
        updater::plan(
            &self.addon,
            self.addon_installed.as_ref(),
            self.launcher_offer(),
            &update::current_version(),
            self.held.as_deref(),
        )
    }

    pub fn missing_addon(&self) -> Option<&Release> {
        self.addon.choose(&update::current_version(), None)
    }

    pub fn runs_from_release(&self) -> bool {
        !matches!(&self.copy, Err(reason) if reason.contains(NOT_RELEASE_FOLDER))
    }

    fn waiting_notice(&self) -> Option<String> {
        let target = self
            .launcher_offer()
            .and_then(Release::version)
            .unwrap_or_else(update::current_version);
        let c = self.addon.waiting(&target, self.addon_installed.as_ref())?;
        Some(trf!(
            "{addon} {version} needs Swofty Launcher {needs} and installs once that launcher is out. You have {current}.",
            addon = managed::NAME,
            version = c.version,
            needs = c.needs.as_ref()?,
            current = update::current_version()
        ))
    }

    fn blocked_notice(&self) -> Option<(Version, String, &str)> {
        let reason = self.copy.as_ref().err()?;
        let version = self.launcher.version()?;
        (version > update::current_version()).then(|| {
            (
                version,
                self.launcher.tag.clone().unwrap_or_default(),
                reason.as_str(),
            )
        })
    }

    pub(crate) fn gate(&self, busy: bool) -> Gate {
        updater::gate(&self.phase, !self.plan().is_empty(), busy)
    }

    /// Takes a finished check; comes back with the managed addon's release when it is missing and
    /// nothing else is running.
    fn took_check(&mut self, addon: Catalog, launcher: Track, installed: Option<Version>) -> Option<Release> {
        log_change(Part::Addon, &self.addon.latest.error, &addon.latest.error);
        log_change(Part::Addon, &self.addon.error, &addon.error);
        log_change(Part::Launcher, &self.launcher.error, &launcher.error);
        (self.addon, self.launcher, self.addon_installed, self.checking) =
            (addon, launcher, installed, false);
        if self.phase == Phase::Idle
            && let Some(offer) = self.launcher_offer()
            && self.expected.as_deref() == Some(offer.tag_name.as_str())
            && let Some(text) = updater::did_not_take(self.expected.as_deref(), &update::current_version())
        {
            crate::app::app_log::note(&format!("update: {text}"));
            self.phase = Phase::Failed(text);
        }
        let blocked = self
            .blocked_notice()
            .map(|(version, _, reason)| (updater::blocked_line(&version, reason), version));
        if let Some((line, version)) = blocked
            && self.blocked_logged.as_ref() != Some(&version)
        {
            crate::app::app_log::scrubbed(&line);
            self.blocked_logged = Some(version);
        }
        if self.addon_installed.is_none() && !self.adding && self.phase == Phase::Idle {
            return self.missing_addon().cloned();
        }
        None
    }

    /// The plan to run on its own after a launcher update restarted, once, when nothing holds it.
    fn follow_up(&mut self, busy: bool) -> Option<Plan> {
        let plan = self.plan();
        if self.followed_up
            || !updater::follow_up(self.expected.is_some(), &plan)
            || self.gate(busy) != Gate::Update
        {
            return None;
        }
        self.followed_up = true;
        Some(plan)
    }
}

fn log_change(part: Part, before: &Option<String>, after: &Option<String>) {
    if before != after
        && let Some(e) = after
    {
        eprintln!("swofty-launcher: {} update check: {e}", part.name());
        crate::app::app_log::note(&format!("update: {} update check: {e}", part.name()));
    }
}

fn blocked_text(version: &Version, reason: &str) -> String {
    trf!(
        "Swofty Launcher {version} is available. {reason}. You have {current}.",
        version = version,
        reason = crate::ui::format::capitalised(reason),
        current = update::current_version()
    )
}

pub(crate) const NOT_RELEASE_FOLDER: &str = "does not run from a release folder";

impl Shell {
    pub(crate) fn update_gate(&self, state: &PlayState) -> Gate {
        self.updates.gate(holds_updates(state))
    }

    fn start_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.play_state(), PlayState::Building) {
            window.notify(Notification::error(BUILDING), cx);
            return;
        }
        if matches!(
            self.play_state(),
            PlayState::Running | PlayState::Elsewhere(_) | PlayState::Launching | PlayState::Preparing
        ) {
            window.notify(Notification::error(tr("Close F1 25 before updating.")), cx);
            return;
        }
        let plan = self.updates.plan();
        if plan.is_empty() {
            return;
        }
        let task = Updates::run_plan(self, plan, true, cx);
        cx.spawn_in(window, async move |this, cx| {
            let ran = task.await;
            this.update_in(cx, |s, window, cx| {
                if let Some(e) = Updates::finish(s, ran, cx) {
                    window.notify(Notification::error(e).title(tr("Update failed")), cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn retry_addon(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.updates.held = None;
        self.updates.phase = Phase::Idle;
        self.start_updates(window, cx);
    }

    pub(crate) fn update_status(
        &self,
        gate: Gate,
        row: Div,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let dot = |c: Hsla| div().size(px(7.)).rounded_full().bg(c).flex_shrink_0();
        let text = |t: String| div().min_w_0().truncate().child(t);
        let u = &self.updates;
        match gate {
            Gate::Update => {
                let plan = u.plan();
                let changes: Vec<String> = plan
                    .versions()
                    .iter()
                    .map(|(part, v)| {
                        let from = match part {
                            Part::Addon => u.addon_installed.as_ref().map(ToString::to_string),
                            Part::Launcher => Some(update::current_version().to_string()),
                        };
                        match from {
                            Some(from) => format!("{} {from} → {v}", part.name()),
                            None => format!("{} {v}", part.name()),
                        }
                    })
                    .collect();
                let notes = plan.steps().first().map(|(_, r)| r.notes_url());
                Some(
                    row.child(dot(p.accent))
                        .child(text(trf!(
                            "Update required: {changes}. The launcher restarts afterwards.",
                            changes = changes.join(" and ")
                        )))
                        .when_some(notes, |d, notes| {
                            d.child(
                                link("update-notes", "Release notes", p)
                                    .flex_shrink_0()
                                    .on_click(move |_, _, cx| cx.open_url(&notes)),
                            )
                        }),
                )
            }
            Gate::Installing => Some(row.child(dot(p.warning)).child(text(match u.restarts {
                true => trf!(
                    "Updating {versions}. Keep the launcher open; it restarts when it's done.",
                    versions = updater::describe(&u.plan().versions())
                ),
                false => trf!(
                    "Updating {versions}.",
                    versions = updater::describe(&u.plan().versions())
                ),
            }))),
            Gate::Restarting => Some(row.child(dot(p.success)).child(text(trf!(
                "Updated {versions}. Restarting the launcher…",
                versions = updater::describe(&u.restarting)
            )))),
            Gate::Retry | Gate::Play => {
                let Phase::Failed(error) = &u.phase else {
                    return None;
                };
                if gate == Gate::Play && !u.plan().is_empty() {
                    return None;
                }
                let tip = SharedString::from(error.clone());
                Some(
                    row.text_color(p.error)
                        .child(Icon::new(IconName::CircleAlert).size(px(16.)))
                        .child(
                            div()
                                .id("update-failed")
                                .min_w_0()
                                .truncate()
                                .child(error.clone())
                                .tooltip(move |window, cx| {
                                    let tip = tip.clone();
                                    Tooltip::element(move |_, _| div().max_w(px(480.)).child(tip.clone()))
                                        .build(window, cx)
                                }),
                        )
                        .when(gate == Gate::Retry, |d| {
                            d.child(
                                link("play-anyway", "Play without updating", p)
                                    .flex_shrink_0()
                                    .on_click(cx.listener(|this, _, window, cx| this.play(window, cx))),
                            )
                        })
                        .when(gate == Gate::Play && u.held.is_some(), |d| {
                            d.child(
                                link("update-addon-again", "Try again", p)
                                    .flex_shrink_0()
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.retry_addon(window, cx)),
                                    ),
                            )
                        }),
                )
            }
        }
    }

    pub(crate) fn update_button(&self, gate: Gate, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let u = &self.updates;
        let summary = updater::summary(&u.plan().versions());
        let (progress, detail) = match &u.progress {
            Some((done, text)) => (Some(*done), text.clone()),
            None => (None, summary.clone()),
        };
        let (icon, label, sub, bg, fg, active) = match gate {
            Gate::Installing => (
                IconName::LoaderCircle,
                match progress {
                    Some(done) => trf!("Updating {percent}", percent = crate::ui::format::percent(done)),
                    None => tr("Updating…").into(),
                },
                detail,
                p.elevated,
                p.text,
                false,
            ),
            Gate::Restarting => (
                IconName::LoaderCircle,
                "Restarting…".into(),
                updater::summary(&u.restarting),
                p.elevated,
                p.text,
                false,
            ),
            Gate::Retry => (
                IconName::RotateCw,
                "Retry update".into(),
                summary,
                p.accent,
                p.on_accent,
                true,
            ),
            Gate::Update | Gate::Play => (
                IconName::CircleArrowUp,
                "Update".into(),
                summary,
                p.accent,
                p.on_accent,
                true,
            ),
        };
        div()
            .id("play")
            .relative()
            .overflow_hidden()
            .h(px(52.))
            .w(px(BUTTON_WIDTH))
            .flex_shrink_0()
            .rounded(px(9.))
            .bg(bg)
            .text_color(fg)
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .when(!active, |d| d.border_1().border_color(p.border))
            .when(active, |d| {
                d.cursor_pointer()
                    .hover(move |s| s.bg(p.accent_hover))
                    .on_click(cx.listener(|this, _, window, cx| this.start_updates(window, cx)))
            })
            .map(|d| match icon {
                IconName::LoaderCircle => d.child(Spinner::new().icon(icon).with_size(px(18.))),
                _ => d.child(Icon::new(icon).size(px(18.))),
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .truncate()
                            .text_color(if active { p.on_accent } else { p.text_muted })
                            .when(active, |d| d.opacity(0.85))
                            .child(sub),
                    ),
            )
            .when_some(progress.filter(|_| gate == Gate::Installing), |d, done| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .bottom_0()
                        .h(px(4.))
                        .w(relative(done.clamp(0., 1.)))
                        .bg(p.accent),
                )
            })
            .into_any_element()
    }

    pub(crate) fn launcher_notices(&self) -> Vec<String> {
        if !self.prefs.update_check || !self.updates.runs_from_release() {
            return Vec::new();
        }
        match self.updates.blocked_notice() {
            Some((version, _, reason)) => vec![blocked_text(&version, reason)],
            None => self.updates.waiting_notice().into_iter().collect(),
        }
    }

    pub(crate) fn update_bar(&self, p: Palette) -> Option<impl IntoElement> {
        if !self.prefs.update_check || !self.updates.runs_from_release() {
            return None;
        }
        let bar = h_flex()
            .id("update-bar")
            .flex_shrink_0()
            .px(px(16.))
            .py(px(8.))
            .gap(px(12.))
            .bg(p.surface)
            .text_size(px(13.));
        let Some((version, tag, reason)) = self.updates.blocked_notice() else {
            let notice = self.updates.waiting_notice()?;
            return Some(
                bar.text_color(p.text_muted)
                    .child(Icon::new(IconName::Info).size(px(16.)))
                    .child(div().flex_1().min_w_0().truncate().child(notice)),
            );
        };
        let notes = update::notes_url(&tag);
        let fix = reason == update::DATA_IN_INSTALL;
        Some(
            bar.child(
                Icon::new(IconName::CircleArrowUp)
                    .size(px(16.))
                    .text_color(p.accent),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(blocked_text(&version, reason)),
            )
            .when(fix, |d| {
                d.child(
                    link("update-move-data", "Moving your addons and saves", p)
                        .flex_shrink_0()
                        .on_click(|_, _, cx| cx.open_url(MOVE_DOCS)),
                )
            })
            .child(
                link("update-release", "Release page", p)
                    .flex_shrink_0()
                    .on_click(move |_, _, cx| cx.open_url(&notes)),
            ),
        )
    }
}

/// The play states an update waits for.
fn holds_updates(state: &PlayState) -> bool {
    matches!(
        state,
        PlayState::Running
            | PlayState::Elsewhere(_)
            | PlayState::Launching
            | PlayState::Preparing
            | PlayState::Restoring
            | PlayState::Detecting
            | PlayState::Checking
            | PlayState::Building
    )
}

pub(crate) struct UpdatesNeeds<'a> {
    pub(crate) store: &'a Store,
    /// The data folder is moving; nothing checks until it is done.
    pub(crate) locked: bool,
    /// The game or a launch holds updates back.
    pub(crate) busy: bool,
}

pub(crate) enum UpdatesChange {
    /// The managed addon was installed or updated, so the installed addons changed on disk.
    Installed,
}

impl Hosted for Updates {
    type Needs<'a> = UpdatesNeeds<'a>;
    type Change = UpdatesChange;
}

impl Host<Updates> for Shell {
    fn split(&mut self) -> (&mut Updates, UpdatesNeeds<'_>) {
        let locked = self.data_move_locked();
        let busy = holds_updates(&self.play_state());
        (
            &mut self.updates,
            UpdatesNeeds {
                store: &self.store,
                locked,
                busy,
            },
        )
    }

    fn changed(&mut self, change: UpdatesChange, cx: &mut Context<Self>) {
        match change {
            UpdatesChange::Installed => self.reload(cx),
        }
    }
}

impl Updates {
    pub(crate) fn watch<H: Host<Self>>(cx: &mut Context<H>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                if this.update(cx, |host, cx| Self::tick(host, cx)).is_err() {
                    break;
                }
                cx.background_executor().timer(TICK).await;
            }
        })
    }

    fn tick<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (u, needs) = host.split();
        let now = update::now_secs();
        let check_launcher = needs.store.config().updates.check;
        let addon_due = u.addon.probe_due(now);
        let launcher_due = check_launcher && u.launcher.probe_due(now);
        if u.checking
            || matches!(u.phase, Phase::Installing | Phase::Restarting)
            || !(addon_due || launcher_due)
            || needs.locked
        {
            return;
        }
        u.checking = true;
        let (mut addon, mut launcher) = (u.addon.clone(), u.launcher.clone());
        let can_install = u.copy.is_ok();
        let store = needs.store.clone();
        cx.spawn(async move |this, cx| {
            let (addon, launcher, installed) = cx
                .background_executor()
                .spawn(async move {
                    let installed = managed::installed(&store);
                    let current = update::current_version();
                    if launcher_due {
                        launcher.check(Part::Launcher, Some(&current), can_install, now);
                    }
                    if addon_due {
                        addon.check(&store, installed.as_ref(), &current, now);
                    }
                    (addon, launcher, installed)
                })
                .await;
            this.update(cx, |host, cx| Self::checked(host, addon, launcher, installed, cx))
                .ok();
        })
        .detach();
    }

    fn checked<H: Host<Self>>(
        host: &mut H,
        addon: Catalog,
        launcher: Track,
        installed: Option<Version>,
        cx: &mut Context<H>,
    ) {
        if let Some(release) = host.split().0.took_check(addon, launcher, installed) {
            Self::add_missing(host, release, cx);
        }
        let (u, needs) = host.split();
        if let Some(plan) = u.follow_up(needs.busy) {
            let task = Self::run_plan(host, plan, false, cx);
            cx.spawn(async move |this, cx| {
                let ran = task.await;
                this.update(cx, |host, cx| Self::finish(host, ran, cx)).ok();
            })
            .detach();
        }
        cx.notify();
    }

    fn add_missing<H: Host<Self>>(host: &mut H, release: Release, cx: &mut Context<H>) {
        let (u, needs) = host.split();
        u.adding = true;
        let store = needs.store.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let launcher = update::current_version();
                    managed::install_release(&store, &release, &launcher, &Reporter(Shared::default()))
                })
                .await;
            this.update(cx, |host, cx| {
                let u = host.split().0;
                u.adding = false;
                match result {
                    Ok(version) => {
                        eprintln!("swofty-launcher: installed {} {version}", managed::ID);
                        crate::app::app_log::note(&format!("update: installed {} {version}", managed::ID));
                        u.addon_installed = Some(version);
                        host.changed(UpdatesChange::Installed, cx);
                    }
                    Err(e) => {
                        eprintln!("swofty-launcher: could not install {}: {e}", managed::ID);
                        crate::app::app_log::action("update")
                            .addon(managed::ID)
                            .failed(&e);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn run_plan<H: Host<Self>>(
        host: &mut H,
        plan: Plan,
        restarts: bool,
        cx: &mut Context<H>,
    ) -> Task<Ran> {
        let u = host.split().0;
        u.phase = Phase::Installing;
        u.restarts = restarts || plan.launcher.is_some();
        let shared: Shared = Arc::new(Mutex::new(Some((0., "Starting the update".into()))));
        let reporter = Reporter(shared.clone());
        Self::follow_progress(host, shared, cx);
        let (u, needs) = host.split();
        let store = needs.store.clone();
        let copy = u.copy.clone().ok();
        cx.background_executor().spawn(async move {
            let current = update::current_version();
            let ran = updater::run(&plan, &current, &reporter, |part, release, launcher, progress| {
                updater::install(&store, copy.as_ref(), part, release, launcher, progress)
            });
            if let Ok(mut p) = reporter.0.lock() {
                *p = None;
            }
            ran
        })
    }

    pub(crate) fn finish<H: Host<Self>>(host: &mut H, ran: Ran, cx: &mut Context<H>) -> Option<String> {
        let u = host.split().0;
        u.progress = None;
        if let Some(version) = ran.addon() {
            u.addon_installed = Some(version.clone());
        }
        if let Some(tag) = ran.failed_tag(Part::Addon) {
            u.held = Some(tag.to_owned());
        }
        let error = ran.error();
        if let Some(e) = &error {
            crate::app::app_log::action("update").failed(e);
        }
        if ran.launcher().is_some() || (u.restarts && ran.restart()) {
            crate::app::app_log::note(&updater::installed_line(&ran.done));
            u.phase = Phase::Restarting;
            u.restarting = ran.done.clone();
            let launcher = ran.launcher().map(|v| format!("v{v}"));
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(RESTART_DELAY).await;
                this.update(cx, |host, cx| Self::restart(host, launcher, cx)).ok();
            })
            .detach();
            cx.notify();
            return None;
        }
        u.phase = match &error {
            Some(e) => Phase::Failed(trf!("Update failed: {e}", e = e)),
            None => Phase::Idle,
        };
        if ran.addon().is_some() {
            host.changed(UpdatesChange::Installed, cx);
        }
        cx.notify();
        error
    }

    fn follow_progress<H: Host<Self>>(host: &mut H, shared: Shared, cx: &mut Context<H>) {
        host.split().0.progress = shared.lock().ok().and_then(|p| p.clone());
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(100)).await;
                let now = shared.lock().ok().and_then(|p| p.clone());
                let done = now.is_none();
                if this
                    .update(cx, |host, cx| {
                        if now.is_some() {
                            host.split().0.progress = now;
                        }
                        cx.notify();
                    })
                    .is_err()
                    || done
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn restart<H: Host<Self>>(host: &mut H, launcher: Option<String>, cx: &mut Context<H>) {
        let u = host.split().0;
        let exe = match &u.copy {
            Ok(install) => Ok(install.launcher()),
            Err(_) => std::env::current_exe(),
        };
        let mut command = match exe {
            Ok(exe) => std::process::Command::new(exe),
            Err(e) => {
                crate::app::app_log::note(&format!("update: could not find the launcher to restart: {e}"));
                u.phase = Phase::Failed(trf!(
                    "Updated, but could not restart: {e}. Close and reopen the launcher.",
                    e = e
                ));
                cx.notify();
                return;
            }
        };
        command.env_remove(update::PRETEND_ENV);
        match &launcher {
            Some(tag) => command.env(updater::UPDATED_ENV, tag),
            None => command.env_remove(updater::UPDATED_ENV),
        };
        let program = Path::new(command.get_program()).to_path_buf();
        let spawned = command.spawn();
        crate::app::app_log::scrubbed(&updater::restart_line(
            &program,
            spawned.as_ref().map(std::process::Child::id),
        ));
        match spawned {
            Ok(_) => cx.quit(),
            Err(e) => {
                u.phase = Phase::Failed(match Block::of(&program, &e) {
                    Some(b) => trf!("Updated, but could not restart: {e}", e = b.text()),
                    None => trf!(
                        "Updated, but could not restart: {e}. Close and reopen the launcher.",
                        e = e
                    ),
                });
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    fn blocked() -> Updates {
        let mut u = Updates::new();
        u.copy = Err(update::DATA_IN_INSTALL.into());
        u.launcher.tag = Some("v999.0.0".into());
        u
    }

    #[test]
    fn a_data_folder_inside_the_install_shows_one_notice_and_no_update() {
        let u = blocked();
        let (version, tag, reason) = u.blocked_notice().unwrap();
        assert_eq!(version, Version::new(999, 0, 0));
        assert_eq!(tag, "v999.0.0");
        assert_eq!(reason, update::DATA_IN_INSTALL);
        assert!(u.plan().is_empty());
        assert_eq!(updater::gate(&u.phase, !u.plan().is_empty(), false), Gate::Play);
    }

    #[test]
    fn a_source_build_is_not_a_release_copy_but_a_data_conflict_is() {
        let mut u = blocked();
        assert!(u.runs_from_release());
        u.copy = Err("this copy does not run from a release folder".into());
        assert!(!u.runs_from_release());
    }

    #[test]
    fn no_notice_when_the_launcher_is_current() {
        let mut u = blocked();
        u.launcher.tag = Some("v0.0.1".into());
        assert!(u.blocked_notice().is_none());
    }

    #[test]
    fn an_update_waits_while_the_game_runs_or_gets_ready() {
        for busy in [
            PlayState::Running,
            PlayState::Launching,
            PlayState::Checking,
            PlayState::Building,
        ] {
            assert!(holds_updates(&busy));
        }
        for free in [PlayState::Ready, PlayState::NotInstalled] {
            assert!(!holds_updates(&free));
        }
    }

    #[test]
    fn a_check_with_nothing_new_ends_the_check_and_installs_nothing() {
        let mut u = Updates::new();
        u.checking = true;
        assert!(
            u.took_check(Catalog::default(), Track::default(), Some(Version::new(1, 0, 0)))
                .is_none()
        );
        assert!(!u.checking);
        assert_eq!(u.addon_installed, Some(Version::new(1, 0, 0)));
        assert!(u.follow_up(false).is_none());
    }
}
