use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Disableable, Icon, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::launcher_saves;
use swofty_core::profile::VANILLA;
use swofty_core::vault::careers::{self, Brief, Diff, Listed, Row, State as CareerState};
use swofty_core::vault::cloud;
use swofty_core::vault::restore::Running;
use swofty_core::vault::{self, EventKind, Location, Notice, Reason, Snapshot, Vault};

use crate::addons::{DETAIL_W, FILTERS_W, SPLIT_AT, ago, facet_row};
use crate::library::plural;
use crate::pages::{GUTTER, fact, label, tag};
use crate::shell::{Page, Prompt, Shell};
use crate::theme::{Palette, SCROLLBAR_GAP, ScrollArea};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Profile,
    Status,
    Mode,
}

pub(crate) const VANILLA_NOTE: &str =
    "Plays on your Steam save. Careers here show after you play a profile that started from it.";
pub(crate) const NOTICE_TITLE: &str = "Your careers have their own saves now";
pub(crate) const NOTICE_BODY: &str = "Each profile now has its own save, starting from a copy of your Steam save. Playing from Steam still uses your Steam save, and Vanilla plays on it too.";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum StatusFilter {
    Present,
    Missing,
    Deleted,
}

impl StatusFilter {
    const ALL: [Self; 3] = [Self::Present, Self::Missing, Self::Deleted];

    fn label(self) -> &'static str {
        match self {
            Self::Present => "In your save",
            Self::Missing => "Missing",
            Self::Deleted => "Deleted in game",
        }
    }

    fn of(s: CareerState) -> Self {
        match s {
            CareerState::Present => Self::Present,
            CareerState::Missing => Self::Missing,
            CareerState::DeletedInGame => Self::Deleted,
        }
    }
}

#[derive(Default)]
pub(crate) struct SavesPage {
    pub view: Option<SavesView>,
    pub confirm: Option<String>,
    pub banner: Option<Banner>,
    pub game_running: bool,
    pub profile: Option<String>,
    pub career: Option<(String, String)>,
    statuses: BTreeSet<StatusFilter>,
    modes: BTreeSet<String>,
    search: Option<Entity<InputState>>,
    window: Option<AnyWindowHandle>,
    _search: Option<Subscription>,
    notice_done: bool,
}

impl SavesPage {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search careers"));
        let changed = cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        });
        Self {
            search: Some(search),
            window: Some(window.window_handle()),
            _search: Some(changed),
            ..Self::default()
        }
    }

    fn clear_search(&self, cx: &mut App) {
        let (Some(input), Some(window)) = (self.search.clone(), self.window) else {
            return;
        };
        cx.defer(move |cx| {
            window
                .update(cx, |_, window, cx| {
                    input.update(cx, |s, cx| s.set_value("", window, cx))
                })
                .ok();
        });
    }

    fn focus(&self) -> Option<&str> {
        self.career
            .as_ref()
            .map(|(p, _)| p.as_str())
            .or(self.profile.as_deref())
    }
}

pub(crate) enum Banner {
    Dropped {
        profile: String,
        source: String,
        snapshot: String,
        kept_at: i64,
        names: Vec<String>,
    },
    Hint(Notice),
}

struct Version {
    snap: Snapshot,
    held: Option<Vec<String>>,
    diff: Option<Diff>,
}

struct Entry {
    l: Listed,
    profile: String,
    place: Option<String>,
    needs: Vec<String>,
}

impl Entry {
    fn status(&self) -> StatusFilter {
        StatusFilter::of(self.l.state)
    }

    fn mode(&self) -> String {
        mode_name(self.l.mode.as_deref())
    }

    fn key(&self) -> (String, String) {
        (self.profile.clone(), self.l.career.clone())
    }

    fn matches(&self, query: &str) -> bool {
        query.is_empty()
            || [
                Some(self.l.title()),
                Some(self.l.label.clone()),
                Some(self.mode()),
                Some(self.profile.clone()),
                self.l.slot_name.clone(),
                self.l.driver.clone(),
                self.l.team.clone(),
            ]
            .into_iter()
            .flatten()
            .any(|t| t.to_lowercase().contains(query))
    }
}

struct ProfileData {
    profile: String,
    active: bool,
    loc: Option<Location>,
    launcher_off: bool,
    entries: Vec<Entry>,
    last_played: Option<i64>,
    has_save: bool,
    size: u64,
    snaps: Vec<Snapshot>,
    on: BTreeSet<String>,
}

impl ProfileData {
    fn vanilla(&self) -> bool {
        self.profile == VANILLA
    }

    fn name(&self) -> String {
        match self.vanilla() {
            true => "Vanilla · Steam save".to_owned(),
            false => self.profile.clone(),
        }
    }
}

pub(crate) struct SavesView {
    profiles: Vec<ProfileData>,
    versions: Vec<Version>,
    dir: PathBuf,
    usage: u64,
    cap: u64,
    cloud: Option<cloud::Status>,
    steam: Option<Location>,
    running: Running,
    names: BTreeMap<String, String>,
}

impl SavesView {
    pub(crate) fn career_count(&self) -> usize {
        self.profiles
            .iter()
            .flat_map(|c| &c.entries)
            .filter(|e| e.l.state == CareerState::Present)
            .count()
    }

    fn data(&self, profile: &str) -> Option<&ProfileData> {
        self.profiles.iter().find(|p| p.profile == profile)
    }

    fn entry(&self, key: &(String, String)) -> Option<&Entry> {
        self.data(&key.0)?.entries.iter().find(|e| e.l.career == key.1)
    }
}

pub(crate) fn source_of_profile(root: &Path, profile: &str) -> Option<String> {
    match profile == VANILLA {
        true => Location::active(root).map(|l| l.source),
        false => Some(Location::launcher(root, profile).source),
    }
}

fn profile_of_source(source: &str) -> String {
    match source.strip_prefix("launcher-") {
        Some(p) => p.to_owned(),
        None => VANILLA.to_owned(),
    }
}

fn whose(profile: &str) -> String {
    match profile == VANILLA {
        true => "your Steam save".to_owned(),
        false => format!("profile {profile}'s save"),
    }
}

impl SavesPage {
    pub(crate) fn banner_for(root: &Path, profile: &str) -> Option<Banner> {
        banner_from(root, source_of_profile(root, profile)?, profile.to_owned())
    }

    pub(crate) fn refresh_banner(&mut self, root: &Path, profile: &str) {
        self.banner = Self::banner_for(root, profile);
    }

    pub(crate) fn take_notice(&mut self, root: &Path) -> bool {
        if self.notice_done || launcher_saves::marker(root).is_none() {
            return false;
        }
        self.notice_done = true;
        let due = launcher_saves::notice_due(root);
        if due && let Err(e) = launcher_saves::mark_seen(root) {
            crate::app_log::action("record the saves notice").failed(&e);
        }
        due
    }
}

fn banner_from(root: &Path, source: String, profile: String) -> Option<Banner> {
    let v = Vault::open(root, &source).ok()?;
    let table = v.careers();
    let shown = table.drop_to_show();
    if let Some(newest) = shown.iter().max_by_key(|r| r.last_seen_at) {
        return Some(Banner::Dropped {
            profile,
            source,
            snapshot: newest.last_seen.clone(),
            kept_at: newest.last_seen_at,
            names: shown.iter().map(|r| row_name(r)).collect(),
        });
    }
    Notice::read(root).map(Banner::Hint)
}

fn row_name(r: &Row) -> String {
    let mut out = r.name().to_owned();
    if let Some(p) = r.place() {
        out.push_str(&format!(", {p}"));
    }
    out
}

fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn mb_whole(bytes: u64) -> String {
    format!("{} MB", (bytes as f64 / (1024.0 * 1024.0)).round() as u64)
}

pub(crate) fn mode_name(mode: Option<&str>) -> String {
    match mode.unwrap_or("") {
        "" => "Career".to_owned(),
        "driver" | "driver-career" => "Driver Career".to_owned(),
        "myteam" | "my-team" => "My Team".to_owned(),
        "progressive" => "Progressive".to_owned(),
        "classic" | "classic-career" => "Classic Career".to_owned(),
        "1990" | "f1-1990" => "1990".to_owned(),
        other => {
            let mut c = other.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        }
    }
}

fn cloud_line(s: cloud::Status, steam_running: bool) -> String {
    match (s, steam_running) {
        (cloud::Status::Synced, _) => "synced".to_owned(),
        (cloud::Status::Waiting, true) => "waiting to upload, Steam is running".to_owned(),
        (cloud::Status::Waiting, false) => "waiting to upload, Steam uploads it when it starts".to_owned(),
        (s, _) => s.text().to_owned(),
    }
}

fn list(items: &[Brief]) -> String {
    items
        .iter()
        .map(|b| match &b.place {
            Some(p) => format!("{} ({p})", b.name),
            None => b.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn reason_label(r: Reason) -> &'static str {
    match r {
        Reason::BeforeSession => "Before Play",
        Reason::AfterSession => "After Play",
        Reason::Imported => "Imported from Steam",
        Reason::ImportedProfile => "Copied from another profile",
        Reason::LauncherStart => "Launcher start",
        Reason::ChangedOutside => "Changed outside the launcher",
        Reason::ForeignLaunch => "F1 25 started outside the launcher",
        Reason::ForeignExit => "F1 25 closed outside the launcher",
        Reason::BeforeRestore => "Before restore",
        Reason::Restored => "Restored",
        Reason::Migrated => "Old backup",
        Reason::Manual => "Backup",
        Reason::Other => "Saved",
    }
}

fn careers_held(held: &[String]) -> String {
    let shown = held.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    match held.len() {
        0 => "No careers".to_owned(),
        n if n > 3 => format!("{n} careers: {shown} and {} more", n - 3),
        n => format!("{}: {shown}", plural(n, "career", "careers")),
    }
}

fn card_careers(root: &Path, profile: &str, active: bool, table: &careers::Careers) -> Vec<Entry> {
    careers::read_listing(root, Some(profile))
        .into_iter()
        .filter(|l| !l.hidden)
        .filter(|l| match l.profile.as_deref() {
            Some(p) => p == profile,
            None => active,
        })
        .map(|l| {
            let row = table.find(&l.career);
            Entry {
                place: row.and_then(|r| r.place()).or_else(|| l.place()),
                needs: row
                    .map(|r| r.needs().map(str::to_owned).collect())
                    .unwrap_or_default(),
                profile: profile.to_owned(),
                l,
            }
        })
        .collect()
}

fn last_played(v: &Vault, snaps: &[Snapshot]) -> Option<i64> {
    let events = v
        .events()
        .into_iter()
        .filter(|e| matches!(e.kind, EventKind::SessionStart | EventKind::SessionEnd))
        .map(|e| e.at);
    let taken = snaps
        .iter()
        .filter(|s| matches!(s.reason, Reason::BeforeSession | Reason::AfterSession))
        .map(|s| s.at);
    events.chain(taken).max()
}

fn save_size(loc: &Location) -> u64 {
    loc.files()
        .unwrap_or_default()
        .iter()
        .filter_map(|f| std::fs::metadata(&f.path).ok())
        .map(|m| m.len())
        .sum()
}

impl Shell {
    pub(crate) fn refresh_saves(&mut self) {
        let root = self.store.root.clone();
        let active_name = self.library.profile.name.clone();
        self.saves.refresh_banner(&root, &active_name);
        let running = Running::now();
        let active_loc = Location::active(&root);
        let installed = self.store.addons().unwrap_or_default();
        let mut names = self.store.profile_names().unwrap_or_default();
        names.sort_by_key(|n| (*n != active_name, n != VANILLA, n.clone()));
        let profiles: Vec<ProfileData> = names
            .into_iter()
            .map(|name| {
                let vanilla = name == VANILLA;
                let loc = match vanilla {
                    true => active_loc.clone(),
                    false => Some(Location::launcher(&root, &name)),
                };
                let loaded = self.store.profile(&name).ok();
                let launcher_off = !vanilla && loaded.as_ref().is_some_and(|p| !p.launcher_saves);
                let vault = loc.as_ref().and_then(|l| Vault::of(&root, l).ok());
                let table = vault.as_ref().map(|v| v.careers()).unwrap_or_default();
                let entries = card_careers(&root, &name, name == active_name, &table);
                let snaps = vault.as_ref().and_then(|v| v.list().ok()).unwrap_or_default();
                let last_played = vault.as_ref().and_then(|v| last_played(v, &snaps));
                let on: BTreeSet<String> = match loaded
                    .as_ref()
                    .and_then(|p| swofty_core::resolver::resolve(p, &installed).ok())
                {
                    Some(plan) => plan.order.into_iter().collect(),
                    None => BTreeSet::new(),
                };
                ProfileData {
                    active: name == active_name,
                    has_save: loc.as_ref().is_some_and(launcher_saves::has_files),
                    size: loc.as_ref().map(save_size).unwrap_or(0),
                    profile: name,
                    loc,
                    launcher_off,
                    entries,
                    last_played,
                    snaps,
                    on,
                }
            })
            .collect();
        if self
            .saves
            .profile
            .as_ref()
            .is_some_and(|p| !profiles.iter().any(|d| &d.profile == p))
        {
            self.saves.profile = None;
        }
        if self.saves.career.as_ref().is_some_and(|(p, c)| {
            !profiles
                .iter()
                .any(|d| &d.profile == p && d.entries.iter().any(|e| &e.l.career == c))
        }) {
            self.saves.career = None;
        }
        let focus = self.saves.focus().map(str::to_owned);
        let data = focus
            .as_deref()
            .and_then(|f| profiles.iter().find(|d| d.profile == f));
        let steam = active_loc.clone().filter(|l| l.layout == vault::Layout::Steam);
        let (versions, dir, usage, cap) = match data
            .and_then(|d| d.loc.as_ref())
            .and_then(|l| Vault::of(&root, l).ok())
        {
            Some(v) => {
                let versions = data
                    .map(|d| d.snaps.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .map(|snap| {
                        let held = v.index_of(&snap).map(|i| {
                            vault::careers::in_save(&i)
                                .map(|c| match c.label.is_empty() {
                                    true => c.career.clone(),
                                    false => c.label.clone(),
                                })
                                .collect()
                        });
                        let diff = v.preview(&snap);
                        Version { snap, held, diff }
                    })
                    .collect();
                (versions, v.dir.clone(), v.usage(), v.policy.cap_bytes)
            }
            None => (Vec::new(), root.clone(), 0, 0),
        };
        let names = installed
            .iter()
            .map(|(id, m)| (id.clone(), m.name.clone()))
            .collect();
        self.saves.view = Some(SavesView {
            cloud: steam
                .as_ref()
                .and_then(|l| vault::save_status(l).ok())
                .map(|s| s.cloud)
                .filter(|c| *c != cloud::Status::Unknown),
            steam,
            profiles,
            versions,
            dir,
            usage,
            cap,
            running,
            names,
        });
    }

    pub(crate) fn open_saves(
        &mut self,
        profile: Option<String>,
        confirm: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.saves.confirm = confirm;
        if profile.is_some() {
            self.saves.profile = profile;
            self.saves.career = None;
        }
        self.open_page(Page::Careers, cx);
    }

    pub(crate) fn show_saves_notice(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let shell = shell.clone();
            alert
                .width(px(480.))
                .title(NOTICE_TITLE)
                .description(NOTICE_BODY)
                .ok_text("Show me")
                .cancel_text("OK")
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    shell
                        .update(cx, |s, cx| {
                            let active = s.library.profile.name.clone();
                            s.open_saves(Some(active), None, cx)
                        })
                        .ok();
                    true
                })
        });
    }

    fn ask_copy(
        &mut self,
        from: Location,
        from_name: String,
        to: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shell = cx.entity().downgrade();
        let text = format!(
            "{} replaces the save of profile {to}. The current save of {to} is kept as a version first, so you can restore it.",
            match from_name == VANILLA {
                true => "Your Steam save".to_owned(),
                false => format!("The save of profile {from_name}"),
            }
        );
        let steam = from_name == VANILLA;
        let title = match steam {
            true => format!("Import from Steam into {to}?"),
            false => format!("Copy careers to {to}?"),
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, from, to, from_name) = (shell.clone(), from.clone(), to.clone(), from_name.clone());
            alert
                .width(px(480.))
                .title(title.clone())
                .description(text.clone())
                .ok_text(if steam { "Import" } else { "Copy" })
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    shell
                        .update(cx, |s, cx| s.copy_save(&from, &from_name, &to, window, cx))
                        .ok();
                    true
                })
        });
    }

    fn copy_save(
        &mut self,
        from: &Location,
        from_name: &str,
        to: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.store.root.clone();
        let done = launcher_saves::copy_to(&root, from, to, Running::now(), vault::now());
        let note = match done {
            Ok((files, bytes, _)) => {
                crate::app_log::note(&format!(
                    "saves: copied {files} file(s), {bytes} bytes, from {from_name} into the launcher saves of profile {to}"
                ));
                Notification::success(format!("Copied the save of {from_name} to {to}."))
            }
            Err(e) => {
                crate::app_log::action("copy a save").failed(&e);
                Notification::warning(format!("The save was not copied: {e}"))
            }
        };
        window.push_notification(note, cx);
        self.refresh_saves();
        cx.notify();
    }

    fn ask_export(&mut self, profile: String, window: &mut Window, cx: &mut Context<Self>) {
        let shell = cx.entity().downgrade();
        let text = format!(
            "The save of profile {profile} replaces your Steam save. Your current Steam save is kept as a version first. Steam uploads the new save the next time it starts."
        );
        let title = format!("Export {profile} to Steam?");
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, profile) = (shell.clone(), profile.clone());
            alert
                .width(px(480.))
                .title(title.clone())
                .description(text.clone())
                .ok_text("Export")
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    shell.update(cx, |s, cx| s.export_save(&profile, window, cx)).ok();
                    true
                })
        });
    }

    fn export_save(&mut self, profile: &str, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.store.root.clone();
        let done = match Location::active(&root).filter(|l| l.layout == vault::Layout::Steam) {
            Some(steam) => launcher_saves::export(&root, profile, &steam, Running::now(), vault::now())
                .map_err(|e| e.to_string()),
            None => Err("no Steam save folder of F1 25 found".to_owned()),
        };
        let note = match done {
            Ok(r) => {
                crate::app_log::note(&r.log_line());
                Notification::success(format!("Exported the save of {profile} to Steam."))
            }
            Err(e) => {
                crate::app_log::action("export a save").failed(&e);
                Notification::warning(format!("The save was not exported: {e}"))
            }
        };
        window.push_notification(note, cx);
        self.refresh_saves();
        cx.notify();
    }

    fn saves_query(&self, cx: &App) -> String {
        self.saves
            .search
            .as_ref()
            .map(|s| s.read(cx).value().trim().to_lowercase())
            .unwrap_or_default()
    }

    fn career_passes(&self, e: &Entry, query: &str, skip: Option<Group>) -> bool {
        (skip == Some(Group::Profile) || self.saves.profile.as_ref().is_none_or(|p| *p == e.profile))
            && (skip == Some(Group::Status)
                || self.saves.statuses.is_empty()
                || self.saves.statuses.contains(&e.status()))
            && (skip == Some(Group::Mode)
                || self.saves.modes.is_empty()
                || self.saves.modes.contains(&e.mode()))
            && e.matches(query)
    }

    fn saves_filtering(&self, query: &str) -> bool {
        !query.is_empty()
            || self.saves.profile.is_some()
            || !self.saves.statuses.is_empty()
            || !self.saves.modes.is_empty()
    }

    fn target_profile<'a>(&self, view: &'a SavesView) -> Option<&'a ProfileData> {
        self.saves
            .focus()
            .and_then(|f| view.data(f))
            .or_else(|| view.profiles.iter().find(|d| d.active))
    }

    fn game_blocked(&self, view: &SavesView) -> bool {
        view.running.game || self.saves.game_running
    }

    pub(crate) fn careers_page(&mut self, p: Palette, _wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let subtitle = "Each profile has its own save. Vanilla plays on your Steam save.".to_owned();
        let query = self.saves_query(cx);
        let banner = self.saves_banner(true, p, cx);
        let split = self.content_width >= SPLIT_AT;
        let selected_gone = match (&self.saves.career, &self.saves.view) {
            (Some(k), Some(v)) => v.entry(k).is_none_or(|e| !self.career_passes(e, &query, None)),
            _ => false,
        };
        if selected_gone {
            self.saves.career = None;
        }
        let Some(view) = self.saves.view.as_ref() else {
            return v_flex()
                .size_full()
                .child(self.header(Page::Careers.label(), subtitle, div(), p))
                .children(banner)
                .child(
                    div()
                        .px(px(GUTTER))
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .child("No F1 25 save found yet."),
                )
                .into_any_element();
        };
        let actions = self.header_actions(view, cx);
        let nothing = view.profiles.iter().all(|d| d.entries.is_empty() && !d.has_save);
        let body = match nothing {
            true => div()
                .px(px(GUTTER))
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child("No careers yet. They appear after you play a profile.")
                .into_any_element(),
            false => {
                let list = self.career_list(view, &query, split, p, cx);
                let detail =
                    (split && self.saves.focus().is_some()).then(|| self.saves_detail(view, false, p, cx));
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .px(px(GUTTER))
                    .pb(px(20.))
                    .gap(px(20.))
                    .child(self.saves_filters(view, &query, p, cx))
                    .child(list)
                    .children(detail)
                    .into_any_element()
            }
        };
        v_flex()
            .size_full()
            .child(self.header(Page::Careers.label(), subtitle, actions, p))
            .children(banner)
            .child(body)
            .into_any_element()
    }

    fn header_actions(&self, view: &SavesView, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let target = self.target_profile(view);
        let blocked_game = self.game_blocked(view);
        let name = target.map(|t| t.profile.clone()).unwrap_or_default();
        let vanilla = target.is_none_or(|t| t.vanilla());
        let steam = view.steam.clone();
        let import_why = match () {
            _ if vanilla => {
                Some("Vanilla plays on your Steam save, so there is nothing to import into it.".to_owned())
            }
            _ if steam.is_none() => Some("No Steam save of F1 25 found.".to_owned()),
            _ if blocked_game => Some("Close F1 25 first.".to_owned()),
            _ => None,
        };
        let has_save = target.is_some_and(|t| t.has_save);
        let export_why = match () {
            _ if vanilla => Some("Vanilla plays on your Steam save already.".to_owned()),
            _ if steam.is_none() => Some("No Steam save of F1 25 found.".to_owned()),
            _ if !has_save => Some("This profile has no save yet.".to_owned()),
            _ if blocked_game => Some("Close F1 25 first.".to_owned()),
            _ if view.running.steam => Some("Close Steam first.".to_owned()),
            _ => None,
        };
        let (import_name, export_name) = (name.clone(), name);
        let import = Button::new("saves-import")
            .outline()
            .icon(IconName::Download)
            .label("Import from Steam")
            .disabled(import_why.is_some())
            .when_some(import_why, |b, t| b.tooltip(t))
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(steam) = steam.clone() {
                    this.ask_copy(steam, VANILLA.to_owned(), import_name.clone(), window, cx);
                }
            }));
        let export = Button::new("saves-export")
            .outline()
            .icon(IconName::Upload)
            .label("Export to Steam")
            .disabled(export_why.is_some())
            .when_some(export_why, |b, t| b.tooltip(t))
            .on_click(
                cx.listener(move |this, _, window, cx| this.ask_export(export_name.clone(), window, cx)),
            );
        h_flex().gap(px(8.)).child(import).child(export)
    }

    fn saves_filters(
        &self,
        view: &SavesView,
        query: &str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.saves_filtering(query);
        let count = |skip: Group, f: &dyn Fn(&Entry) -> bool| {
            view.profiles
                .iter()
                .flat_map(|d| &d.entries)
                .filter(|e| self.career_passes(e, query, Some(skip)) && f(e))
                .count()
        };
        let mut profile_rows: Vec<AnyElement> = Vec::new();
        {
            let on = self.saves.profile.is_none();
            profile_rows.push(
                facet_row(
                    "saves-profile-all",
                    on,
                    "All profiles",
                    count(Group::Profile, &|_| true),
                    p,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.saves.profile = None;
                    this.saves.career = None;
                    this.refresh_saves();
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        for d in &view.profiles {
            let name = d.profile.clone();
            let on = self.saves.profile.as_deref() == Some(name.as_str());
            let n = count(Group::Profile, &|e| e.profile == name);
            profile_rows.push(
                facet_row(format!("saves-profile-{name}"), on, d.name(), n, p)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.saves.profile = Some(name.clone());
                        this.saves.career = None;
                        this.refresh_saves();
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        let status_rows: Vec<AnyElement> = StatusFilter::ALL
            .into_iter()
            .map(|s| {
                let on = self.saves.statuses.contains(&s);
                let n = count(Group::Status, &|e| e.status() == s);
                facet_row(format!("saves-status-{s:?}"), on, s.label(), n, p)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.saves.statuses.remove(&s) {
                            this.saves.statuses.insert(s);
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let modes: BTreeSet<String> = view
            .profiles
            .iter()
            .flat_map(|d| &d.entries)
            .map(|e| e.mode())
            .collect();
        let mode_rows: Vec<AnyElement> = modes
            .into_iter()
            .map(|m| {
                let on = self.saves.modes.contains(&m);
                let n = count(Group::Mode, &|e| e.mode() == m);
                let key = m.clone();
                facet_row(format!("saves-mode-{m}"), on, m.clone(), n, p)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.saves.modes.remove(&key) {
                            this.saves.modes.insert(key.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let group = |title: &'static str, rows: Vec<AnyElement>| {
            v_flex()
                .gap(px(1.))
                .child(div().pb(px(4.)).child(label(title, p)))
                .children(rows)
        };
        let clear = active.then(|| {
            div()
                .id("saves-filters-clear")
                .text_size(px(12.))
                .text_color(p.accent)
                .cursor_pointer()
                .hover(move |s| s.text_color(p.accent_hover))
                .child("Clear")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.saves.profile = None;
                    this.saves.career = None;
                    this.saves.statuses.clear();
                    this.saves.modes.clear();
                    this.saves.clear_search(cx);
                    this.refresh_saves();
                    cx.notify();
                }))
        });
        v_flex()
            .w(px(FILTERS_W))
            .flex_shrink_0()
            .h_full()
            .gap(px(14.))
            .children(self.saves.search.as_ref().map(|s| {
                Input::new(s).cleanable(true).prefix(
                    Icon::new(IconName::Search)
                        .size(px(14.))
                        .text_color(p.text_placeholder),
                )
            }))
            .child(
                h_flex()
                    .pb(px(8.))
                    .mb(px(-6.))
                    .border_b_1()
                    .border_color(p.border_variant)
                    .justify_between()
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .text_color(p.text_muted)
                            .child(Icon::new(IconName::Funnel).size(px(13.)))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Filters"),
                            ),
                    )
                    .children(clear),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("saves-filters")
                    .gap(px(16.))
                    .pr(px(SCROLLBAR_GAP))
                    .pb(px(8.))
                    .child(group("PROFILE", profile_rows))
                    .child(group("STATUS", status_rows))
                    .when(!mode_rows.is_empty(), |d| d.child(group("MODE", mode_rows))),
            )
    }

    fn career_list(
        &self,
        view: &SavesView,
        query: &str,
        split: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows: Vec<&Entry> = view
            .profiles
            .iter()
            .flat_map(|d| &d.entries)
            .filter(|e| self.career_passes(e, query, None))
            .collect();
        let profile_only = self.saves.profile.as_deref().and_then(|f| view.data(f));
        let mut children: Vec<AnyElement> = Vec::new();
        if !split && self.saves.career.is_none() && profile_only.is_some() {
            children.push(self.saves_detail(view, true, p, cx));
        }
        for e in &rows {
            children.push(self.career_row(e, view, p, cx));
            if !split && self.saves.career.as_ref() == Some(&e.key()) {
                children.push(self.saves_detail(view, true, p, cx));
            }
        }
        let empty = |text: &'static str, clear: bool, cx: &mut Context<Self>| {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .py(px(40.))
                .rounded(px(8.))
                .border_1()
                .border_color(p.border_variant)
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(text)
                .when(clear, |d| {
                    d.child(
                        Button::new("saves-empty-clear")
                            .ghost()
                            .small()
                            .label("Clear filters")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.saves.profile = None;
                                this.saves.career = None;
                                this.saves.statuses.clear();
                                this.saves.modes.clear();
                                this.saves.clear_search(cx);
                                this.refresh_saves();
                                cx.notify();
                            })),
                    )
                })
                .into_any_element()
        };
        if rows.is_empty() {
            let never = profile_only.is_some_and(|d| d.entries.is_empty())
                && self.saves.statuses.is_empty()
                && self.saves.modes.is_empty()
                && query.is_empty();
            let body = match never {
                true => empty(
                    "No careers listed yet. They appear the first time you play this profile.",
                    false,
                    cx,
                ),
                false => empty("No careers match.", self.saves_filtering(query), cx),
            };
            if split {
                return div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(body)
                    .into_any_element();
            }
            return v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .gap(px(12.))
                .children(children)
                .child(body)
                .into_any_element();
        }
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(8.))
            .border_1()
            .border_color(p.border)
            .overflow_hidden()
            .child(
                v_flex()
                    .size_full()
                    .scroll_y("saves-list")
                    .child(div().px(px(12.)).pt(px(8.)).pb(px(6.)).child(label(
                        match self.saves.profile.as_deref() {
                            Some(f) => format!("CAREERS IN {}", f.to_uppercase()),
                            None => "CAREERS".to_owned(),
                        },
                        p,
                    )))
                    .children(children),
            )
            .into_any_element()
    }

    fn career_row(&self, e: &Entry, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let key = e.key();
        let selected = self.saves.career.as_ref() == Some(&key);
        let on = view.data(&e.profile).map(|d| &d.on);
        let needs: Vec<AnyElement> = e
            .needs
            .iter()
            .map(|id| {
                let off = on.is_some_and(|s| !s.is_empty() && !s.contains(id));
                let name = view.names.get(id).cloned().unwrap_or_else(|| id.clone());
                tag(
                    match off {
                        true => format!("{name} is off"),
                        false => name,
                    },
                    p,
                )
                .when(off, |t| t.text_color(p.error).border_color(p.error))
                .into_any_element()
            })
            .collect();
        let chip = match e.status() {
            StatusFilter::Present => None,
            StatusFilter::Missing => Some(("Missing", p.warning)),
            StatusFilter::Deleted => Some(("Deleted in game", p.text_placeholder)),
        };
        let off = e.status() != StatusFilter::Present;
        let id = SharedString::from(format!("career-{}-{}", e.profile, e.l.career));
        h_flex()
            .id(id)
            .relative()
            .min_h(px(52.))
            .px(px(12.))
            .py(px(6.))
            .gap(px(10.))
            .flex_shrink_0()
            .border_t_1()
            .border_color(p.border_variant)
            .cursor_pointer()
            .map(|d| match selected {
                true => d.bg(p.selected),
                false => d.hover(move |s| s.bg(p.hover)),
            })
            .when(selected, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(3.))
                        .bg(p.accent),
                )
            })
            .child(div().w(px(108.)).flex_shrink_0().child(tag(e.mode(), p)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_baseline()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .when(off, |d| d.text_color(p.text_muted))
                                    .child(e.l.title()),
                            )
                            .children(e.place.clone().map(|pl| {
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.))
                                    .text_color(p.text_placeholder)
                                    .child(pl)
                            })),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(tag(
                                match e.profile == VANILLA {
                                    true => "Vanilla".to_owned(),
                                    false => e.profile.clone(),
                                },
                                p,
                            ))
                            .children(e.l.last_played.map(|at| format!("played {}", ago(at as f64)))),
                    ),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .gap(px(4.))
                    .justify_end()
                    .children(needs)
                    .children(chip.map(|(t, c)| {
                        div()
                            .px(px(6.))
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(c)
                            .text_size(px(11.))
                            .text_color(c)
                            .child(t)
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.saves.career = match this.saves.career.as_ref() == Some(&key) {
                    true => None,
                    false => Some(key.clone()),
                };
                this.saves.confirm = None;
                this.refresh_saves();
                cx.notify();
            }))
            .into_any_element()
    }

    fn saves_detail(&self, view: &SavesView, inline: bool, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(d) = self.saves.focus().and_then(|f| view.data(f)) else {
            return div().w(px(DETAIL_W)).flex_shrink_0().into_any_element();
        };
        let entry = self.saves.career.as_ref().and_then(|k| view.entry(k));
        let close = (!inline && (entry.is_some() || self.saves.profile.is_some())).then(|| {
            Button::new("saves-detail-close")
                .ghost()
                .small()
                .icon(IconName::X)
                .tooltip("Close")
                .on_click(cx.listener(|this, _, _, cx| {
                    match this.saves.career.take() {
                        Some(_) => {}
                        None => this.saves.profile = None,
                    }
                    this.refresh_saves();
                    cx.notify();
                }))
        });
        let head = match entry {
            Some(e) => v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(4.))
                .child(
                    h_flex()
                        .gap(px(6.))
                        .child(tag(e.mode(), p))
                        .child(tag(d.name(), p)),
                )
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(e.l.title()),
                ),
            None => v_flex().flex_1().min_w_0().gap(px(4.)).child(
                h_flex()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(d.name()),
                    )
                    .when(d.active, |h| h.child(tag("Active", p)))
                    .when(d.launcher_off, |h| h.child(tag("Plays on the Steam save", p))),
            ),
        };
        let muted = |t: String| div().text_color(p.text_muted).child(t);
        let facts: Vec<Div> = match entry {
            Some(e) => {
                let mut f = vec![fact(
                    "Status",
                    muted(match e.status() {
                        StatusFilter::Present => "In your save".to_owned(),
                        _ => e.l.state.label().to_owned(),
                    }),
                    p,
                )];
                f.extend(e.l.driver.clone().map(|v| fact("Driver", div().child(v), p)));
                f.extend(e.l.team.clone().map(|v| fact("Team", div().child(v), p)));
                f.extend(e.place.clone().map(|v| fact("Season", div().child(v), p)));
                f.extend(e.l.last_played.map(|at| {
                    fact(
                        "Last played",
                        div().child(format!("{} · {}", ago(at as f64), vault::human(at))),
                        p,
                    )
                }));
                f.extend(
                    e.l.slot_name
                        .clone()
                        .filter(|n| !n.is_empty())
                        .map(|v| fact("Slot", div().child(v), p)),
                );
                if !e.needs.is_empty() {
                    let names: Vec<String> = e
                        .needs
                        .iter()
                        .map(|id| view.names.get(id).cloned().unwrap_or_else(|| id.clone()))
                        .collect();
                    f.push(fact("Needs", div().child(names.join(", ")), p));
                }
                f
            }
            None => {
                let mut f = vec![fact(
                    "Last played",
                    div().child(match d.last_played {
                        Some(at) => format!("{} · {}", ago(at as f64), vault::human(at)),
                        None => "Not played from this launcher yet".to_owned(),
                    }),
                    p,
                )];
                f.push(fact("Save size", div().child(mb(d.size)), p));
                f.push(fact(
                    "Careers",
                    div().child(plural(d.entries.len(), "career", "careers")),
                    p,
                ));
                if d.vanilla()
                    && let Some(s) = view.cloud
                {
                    f.push(fact(
                        "Steam Cloud",
                        div().child(cloud_line(s, view.running.steam)),
                        p,
                    ));
                }
                f
            }
        };
        let note = (entry.is_none() && d.vanilla()).then(|| {
            div()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(VANILLA_NOTE)
        });
        let empty = (entry.is_none() && d.entries.is_empty() && !d.vanilla()).then(|| {
            div()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child("No careers listed yet. They appear the first time you play this profile.")
        });
        let (label_text, copy_note) = match entry {
            Some(_) => (
                "Copy to profile…",
                Some("Copies the whole save of this profile, not only this career."),
            ),
            None => ("Copy careers to…", None),
        };
        let copy = self.copy_menu(d, view, "saves-detail-copy", label_text, cx);
        let blocker = vault::restore::blocker(
            d.loc.as_ref().map_or(vault::Layout::Launcher, |l| l.layout),
            Running {
                game: self.game_blocked(view),
                ..view.running
            },
        );
        let versions: Vec<AnyElement> = view
            .versions
            .iter()
            .map(|v| self.version_row(v, view, p, cx))
            .collect();
        let body = v_flex()
            .gap(px(16.))
            .child(h_flex().gap(px(8.)).items_start().child(head).children(close))
            .child(v_flex().children(facts))
            .children(note)
            .children(empty)
            .child(v_flex().gap(px(6.)).child(h_flex().child(copy)).children(
                copy_note.map(|t| div().text_size(px(12.)).text_color(p.text_placeholder).child(t)),
            ))
            .child(
                v_flex()
                    .gap(px(8.))
                    .child(label("VERSIONS", p))
                    .child(self.storage_line(view, p, cx))
                    .children(
                        blocker.map(|b| div().text_size(px(12.)).text_color(p.text_placeholder).child(b)),
                    )
                    .child(match versions.is_empty() {
                        true => div()
                            .text_size(px(13.))
                            .text_color(p.text_muted)
                            .child("No versions of this save yet.")
                            .into_any_element(),
                        false => v_flex()
                            .ml(px(5.))
                            .border_l_1()
                            .border_color(p.border)
                            .gap(px(8.))
                            .children(versions)
                            .into_any_element(),
                    }),
            );
        let key = SharedString::from(format!("saves-detail-{}", d.profile));
        if inline {
            return body
                .id(key)
                .flex_shrink_0()
                .px(px(20.))
                .pt(px(14.))
                .pb(px(18.))
                .bg(p.surface)
                .border_t_1()
                .border_color(p.border_variant)
                .into_any_element();
        }
        div()
            .w(px(DETAIL_W))
            .flex_shrink_0()
            .h_full()
            .rounded(px(8.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .child(body.size_full().p(px(18.)).scroll_y(key))
            .into_any_element()
    }

    fn copy_menu(
        &self,
        d: &ProfileData,
        view: &SavesView,
        id: &'static str,
        text: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let others: Vec<String> = view
            .profiles
            .iter()
            .filter(|o| !o.vanilla() && o.profile != d.profile)
            .map(|o| o.profile.clone())
            .collect();
        let can_copy = d.has_save
            && d.loc
                .as_ref()
                .is_some_and(|l| launcher_saves::supported(l.layout));
        let (from, from_name) = (d.loc.clone(), d.profile.clone());
        let shell = cx.entity().downgrade();
        let disabled = !can_copy || others.is_empty() || self.game_blocked(view);
        Button::new(id)
            .outline()
            .small()
            .icon(IconName::Copy)
            .label(text)
            .dropdown_caret(true)
            .disabled(disabled)
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for target in &others {
                    let (shell, from, from_name, to) =
                        (shell.clone(), from.clone(), from_name.clone(), target.clone());
                    menu = menu.item(PopupMenuItem::new(target.clone()).on_click(move |_, window, cx| {
                        let Some(from) = from.clone() else { return };
                        let (from_name, to) = (from_name.clone(), to.clone());
                        shell
                            .update(cx, |s, cx| s.ask_copy(from, from_name, to, window, cx))
                            .ok();
                    }));
                }
                menu
            })
            .into_any_element()
    }

    fn selected_layout(&self, view: &SavesView) -> vault::Layout {
        self.saves
            .focus()
            .and_then(|f| view.data(f))
            .and_then(|d| d.loc.as_ref().map(|l| l.layout))
            .or_else(|| view.versions.first().map(|v| v.snap.layout))
            .unwrap_or(vault::Layout::Launcher)
    }

    fn storage_line(&self, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> Div {
        let dir = view.dir.clone();
        h_flex()
            .gap(px(8.))
            .pb(px(6.))
            .items_center()
            .text_size(px(13.))
            .text_color(p.text_muted)
            .child(Icon::new(IconName::Save).size(px(14.)))
            .child(div().flex_1().min_w_0().child(format!(
                "Versions use {} of {}",
                mb(view.usage),
                mb_whole(view.cap)
            )))
            .child(
                Button::new("saves-storage-change")
                    .ghost()
                    .small()
                    .label("Change")
                    .on_click(cx.listener(|this, _, window, cx| this.cap_prompt(window, cx))),
            )
            .child(
                Button::new("saves-storage-open")
                    .ghost()
                    .small()
                    .icon(IconName::FolderOpen)
                    .label("Open folder")
                    .on_click(cx.listener(move |_, _, _, cx| crate::reveal::show(&dir, cx))),
            )
    }

    fn version_row(&self, v: &Version, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let s = &v.snap;
        let id = s.id.clone();
        let pinned = !s.pinned.is_empty();
        let own_pin = s.pinned.iter().any(|x| x == "pinned");
        let confirming = self.saves.confirm.as_deref() == Some(s.id.as_str());
        let layout = self.selected_layout(view);
        let blocker = vault::restore::blocker(
            layout,
            Running {
                game: view.running.game || self.saves.game_running,
                ..view.running
            },
        );
        let detail = [v.held.as_ref().map(|h| careers_held(h)), Some(mb(s.total()))]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let source = s.source.clone();
        let (restore_id, restore_source) = (id.clone(), source.clone());
        let (pin_id, pin_source) = (id.clone(), source);
        let ask_id = id.clone();
        let tooltip = blocker.clone();
        div()
            .relative()
            .pl(px(16.))
            .child(
                div()
                    .absolute()
                    .left(px(-5.))
                    .top(px(18.))
                    .size(px(9.))
                    .rounded_full()
                    .bg(match pinned {
                        true => p.accent,
                        false => p.border,
                    }),
            )
            .child(
                v_flex()
                    .gap(px(10.))
                    .p(px(12.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(match confirming {
                        true => p.accent,
                        false => p.border,
                    })
                    .child(
                        h_flex()
                            .gap(px(10.))
                            .items_center()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(2.))
                                    .child(
                                        h_flex()
                                            .gap(px(8.))
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_size(px(14.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(reason_label(s.reason)),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.))
                                                    .text_color(p.text_placeholder)
                                                    .child(format!(
                                                        "{} · {}",
                                                        ago(s.at as f64),
                                                        vault::human(s.at)
                                                    )),
                                            )
                                            .when(pinned, |d| {
                                                d.child(
                                                    Icon::new(IconName::Star)
                                                        .size(px(13.))
                                                        .text_color(p.accent),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(p.text_muted)
                                            .truncate()
                                            .child(detail),
                                    ),
                            )
                            .child(
                                Button::new(SharedString::from(format!("pin-{id}")))
                                    .ghost()
                                    .small()
                                    .icon(IconName::Star)
                                    .label(if own_pin { "Unpin" } else { "Pin" })
                                    .tooltip(if own_pin {
                                        "Let retention remove this version again"
                                    } else {
                                        "Keep this version; retention never removes it"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.pin_version(&pin_source, &pin_id, own_pin, cx)
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("restore-{id}")))
                                    .outline()
                                    .small()
                                    .icon(IconName::Undo2)
                                    .label("Restore")
                                    .disabled(blocker.is_some())
                                    .when_some(tooltip, |b, t| b.tooltip(t))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.saves.confirm =
                                            match this.saves.confirm.as_deref() == Some(ask_id.as_str()) {
                                                true => None,
                                                false => Some(ask_id.clone()),
                                            };
                                        cx.notify();
                                    })),
                            ),
                    )
                    .when(confirming, |d| {
                        d.child(self.restore_panel(
                            v,
                            layout,
                            blocker.clone(),
                            (restore_id, restore_source),
                            p,
                            cx,
                        ))
                    }),
            )
            .into_any_element()
    }

    pub(crate) fn saves_banner(&self, on_page: bool, p: Palette, cx: &mut Context<Self>) -> Option<Div> {
        let banner = self.saves.banner.as_ref()?;
        let (text, profile, source, snapshot, kept_at) = match banner {
            Banner::Dropped {
                profile,
                source,
                snapshot,
                kept_at,
                names,
            } => (
                format!(
                    "{} missing from {}: {}. {} last in the save on {}. Restoring that version brings {} back; the \
                     restore shows what else changes first.",
                    match names.len() {
                        1 => "1 career is".to_owned(),
                        n => format!("{n} careers are"),
                    },
                    whose(profile),
                    names.join("; "),
                    match names.len() {
                        1 => "It was",
                        _ => "They were",
                    },
                    vault::human(*kept_at),
                    match names.len() {
                        1 => "it",
                        _ => "them",
                    }
                ),
                profile.clone(),
                source.clone(),
                snapshot.clone(),
                *kept_at,
            ),
            Banner::Hint(n) => (
                n.headline(),
                profile_of_source(&n.source),
                n.source.clone(),
                n.snapshot.clone(),
                n.kept_at,
            ),
        };
        let hint = matches!(banner, Banner::Hint(_));
        let review = snapshot.clone();
        let _ = kept_at;
        let (review_profile, dismiss_source) = (profile, source);
        Some(
            h_flex()
                .px(px(GUTTER))
                .pb(px(12.))
                .gap(px(8.))
                .flex_shrink_0()
                .items_center()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(
                    Icon::new(IconName::CircleAlert)
                        .size(px(14.))
                        .text_color(p.warning),
                )
                .child(div().flex_1().min_w_0().child(text))
                .child(
                    Button::new("saves-banner-review")
                        .outline()
                        .small()
                        .label(match on_page {
                            true => "Review the restore",
                            false => "Open Career saves",
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_saves(Some(review_profile.clone()), Some(review.clone()), cx);
                        })),
                )
                .child(
                    Button::new("saves-banner-dismiss")
                        .ghost()
                        .small()
                        .label("Dismiss")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let root = this.store.root.clone();
                            if hint {
                                Notice::clear(&root);
                            }
                            if let Ok(v) = Vault::open(&root, &dismiss_source) {
                                v.dismiss_drop().ok();
                            }
                            this.refresh_saves();
                            cx.notify();
                        })),
                ),
        )
    }

    pub(crate) fn restore_version(
        &mut self,
        source: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.store.root.clone();
        let restored = Vault::open(&root, source).and_then(|v| {
            let snap = v.find(name)?;
            let loc = Location::of_source(&root, source)
                .or_else(|| Location::active(&root).filter(|l| l.source == source))
                .unwrap_or(Location {
                    source: snap.source.clone(),
                    layout: snap.layout,
                    dir: snap.save_dir.clone(),
                });
            vault::restore::restore(&v, name, &loc, vault::restore::Running::now(), vault::now())
        });
        let note = match restored {
            Ok(r) => {
                crate::app_log::note(&r.log_line());
                self.saves.confirm = None;
                Notification::success(match &r.before {
                    Some(b) => format!("Restored the save from {name}. The save it replaced is version {b}."),
                    None => format!("Restored the save from {name}."),
                })
            }
            Err(e) => {
                crate::app_log::action("restore a save").failed(&e);
                Notification::warning(format!("The save was not restored: {e}"))
            }
        };
        window.push_notification(note, cx);
        self.refresh_saves();
        cx.notify();
    }

    fn pin_version(&mut self, source: &str, id: &str, pinned: bool, cx: &mut Context<Self>) {
        let root = self.store.root.clone();
        let done = Vault::open(&root, source).and_then(|v| match pinned {
            true => v.unpin_why(id, "pinned", vault::now()),
            false => v.pin(id, "pinned", vault::now()),
        });
        if let Err(e) = done {
            crate::app_log::action("pin a version").failed(&e);
        }
        self.refresh_saves();
        cx.notify();
    }

    fn cap_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let initial = self
            .store
            .config()
            .vault_cap_mb
            .unwrap_or(vault::DEFAULT_CAP_MB)
            .to_string();
        self.prompt(
            Prompt {
                title: "Career saves storage",
                description: "The most space, in MB, the saved versions may take. The oldest unpinned versions go first."
                    .into(),
                initial,
                placeholder: "500",
                ok: "Save",
                browse: None,
                action: std::rc::Rc::new(|store, value| {
                    let mb: u64 = value.parse().map_err(|_| "Enter a number of MB.".to_owned())?;
                    if mb == 0 {
                        return Err("The limit must be at least 1 MB.".to_owned());
                    }
                    let mut c = store.config();
                    c.vault_cap_mb = Some(mb);
                    store.save_config(&c).map_err(|e| e.to_string())?;
                    Ok(format!("The career saves keep at most {mb} MB"))
                }),
            },
            window,
            cx,
        );
    }

    fn restore_panel(
        &self,
        v: &Version,
        layout: vault::Layout,
        blocker: Option<String>,
        (id, source): (String, String),
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let when = vault::human(v.snap.at);
        let line = |head: &'static str, tone: Hsla, text: String| {
            h_flex()
                .gap(px(8.))
                .items_start()
                .text_size(px(13.))
                .child(
                    div()
                        .w(px(100.))
                        .flex_shrink_0()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(tone)
                        .child(head),
                )
                .child(div().flex_1().min_w_0().text_color(p.text_muted).child(text))
        };
        let mut lines: Vec<Div> = Vec::new();
        match &v.diff {
            Some(d) => {
                lines.push(match d.get_back.is_empty() {
                    true => line("You get back", p.text_placeholder, "No careers.".into()),
                    false => line("You get back", p.success, list(&d.get_back)),
                });
                let mut lose = Vec::new();
                if !d.lose.is_empty() {
                    lose.push(format!("{} (not in that version)", list(&d.lose)));
                }
                for (now, then) in &d.progress {
                    lose.push(format!(
                        "progress in {}: {} goes back to {}",
                        now.name,
                        now.place.as_deref().unwrap_or("now"),
                        then.place.as_deref().unwrap_or("then")
                    ));
                }
                lines.push(match lose.is_empty() {
                    true => line("You lose", p.text_placeholder, "No careers.".into()),
                    false => line("You lose", p.error, lose.join("; ")),
                });
            }
            None => lines.push(line(
                "Careers",
                p.text_placeholder,
                "This version, or the save as it is, has no career list, so the launcher can't say which careers change."
                    .into(),
            )),
        }
        lines.push(line(
            "Also",
            p.text_placeholder,
            format!(
                "Settings, setups and everything else in the profile save go back to {when}. The save as it is now is kept as a version first."
            ),
        ));
        let close = match layout {
            vault::Layout::Steam => {
                "Exit Steam completely (Steam menu > Exit) and close F1 25 before you restore. Starting Steam afterwards uploads the restored save."
            }
            vault::Layout::Ea => "Close the EA app and F1 25 before you restore.",
            vault::Layout::Launcher | vault::Layout::LauncherEa => "Close F1 25 before you restore.",
        };
        v_flex()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(5.))
            .bg(p.surface)
            .border_1()
            .border_color(p.border_variant)
            .children(lines)
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(close),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(
                        Button::new("restore-confirm")
                            .primary()
                            .small()
                            .label("Restore this save")
                            .disabled(blocker.is_some())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.restore_version(&source, &id, window, cx)
                            })),
                    )
                    .child(
                        Button::new("restore-cancel")
                            .ghost()
                            .small()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.saves.confirm = None;
                                cx.notify();
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use swofty_core::vault::careers::{Brief, Row, Status};
    use swofty_core::vault::{self, Vault, cloud};

    use swofty_core::launcher_saves::{self, Marker};

    use super::{Banner, SavesPage, banner_from, cloud_line, list, mode_name, reason_label, whose};

    fn row(id: &str, status: Status, at: i64) -> Row {
        Row {
            career: id.into(),
            mode: None,
            label: format!("{id} label"),
            driver: None,
            team: None,
            season: Some(2016),
            round: Some(7),
            profile: None,
            addons: Vec::new(),
            first_seen: "a".into(),
            first_seen_at: 1,
            last_seen: format!("snap-{at}"),
            last_seen_at: at,
            last_played: None,
            status,
            found_at: None,
            dismissed: false,
        }
    }

    fn data(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("swofty-launcher-saves-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(d.join("career-vault/steam-1")).unwrap();
        d
    }

    #[test]
    fn the_banner_names_the_missing_careers_and_the_version_to_restore() {
        let d = data("drop");
        let table = vault::careers::Careers {
            version: 1,
            careers: vec![
                row("a", Status::MissingSince("later".into()), 10),
                row("b", Status::MissingSince("later".into()), 20),
                row("c", Status::Present, 30),
            ],
        };
        table.write(&d.join("career-vault/steam-1")).unwrap();
        match banner_from(&d, "steam-1".into(), "vanilla".into()) {
            Some(Banner::Dropped {
                snapshot,
                names,
                kept_at,
                profile,
                ..
            }) => {
                assert_eq!(profile, "vanilla");
                assert_eq!(snapshot, "snap-20");
                assert_eq!(kept_at, 20);
                assert_eq!(
                    names,
                    ["a label, season 2016, round 7", "b label, season 2016, round 7"]
                );
            }
            _ => panic!("expected a drop banner"),
        }
        Vault::open(&d, "steam-1").unwrap().dismiss_drop().unwrap();
        assert!(banner_from(&d, "steam-1".into(), "vanilla".into()).is_none());
    }

    #[test]
    fn no_careers_and_no_notice_means_no_banner() {
        let d = data("none");
        assert!(banner_from(&d, "steam-1".into(), "vanilla".into()).is_none());
    }

    #[test]
    fn mode_names_read_like_the_game() {
        assert_eq!(mode_name(Some("myteam")), "My Team");
        assert_eq!(mode_name(Some("progressive")), "Progressive");
        assert_eq!(mode_name(Some("sprint")), "Sprint");
        assert_eq!(mode_name(None), "Career");
    }

    #[test]
    fn a_waiting_save_says_when_steam_uploads_it() {
        assert_eq!(
            cloud_line(cloud::Status::Waiting, false),
            "waiting to upload, Steam uploads it when it starts"
        );
        assert_eq!(cloud_line(cloud::Status::Synced, true), "synced");
    }

    #[test]
    fn lists_name_the_place() {
        let b = |n: &str, p: Option<&str>| Brief {
            career: n.into(),
            name: n.into(),
            place: p.map(str::to_owned),
        };
        assert_eq!(
            list(&[b("A", Some("season 2016, round 7")), b("B", None)]),
            "A (season 2016, round 7), B"
        );
    }

    fn marker(profiles: &[&str], seen: bool) -> Marker {
        Marker {
            at: 1,
            launcher: "1.7.0".into(),
            profiles: profiles.iter().map(|p| p.to_string()).collect(),
            note: String::new(),
            seen,
        }
    }

    #[test]
    fn the_first_start_notice_shows_once_and_is_remembered() {
        let d = data("notice");
        let mut page = SavesPage::default();
        assert!(!page.take_notice(&d));
        swofty_core::write_json(&launcher_saves::marker_path(&d), &marker(&["default"], false)).unwrap();
        assert!(page.take_notice(&d));
        assert!(!page.take_notice(&d));
        assert!(launcher_saves::marker(&d).unwrap().seen);
        assert!(!SavesPage::default().take_notice(&d));
    }

    #[test]
    fn no_notice_when_the_import_copied_nothing_or_was_seen() {
        let d = data("notice-none");
        swofty_core::write_json(&launcher_saves::marker_path(&d), &marker(&[], false)).unwrap();
        assert!(!SavesPage::default().take_notice(&d));
        swofty_core::write_json(&launcher_saves::marker_path(&d), &marker(&["default"], true)).unwrap();
        assert!(!SavesPage::default().take_notice(&d));
    }

    #[test]
    fn versions_read_in_plain_words() {
        use swofty_core::vault::Reason;
        assert_eq!(reason_label(Reason::BeforeSession), "Before Play");
        assert_eq!(reason_label(Reason::Migrated), "Old backup");
        assert_eq!(reason_label(Reason::Imported), "Imported from Steam");
        assert_eq!(whose("vanilla"), "your Steam save");
        assert_eq!(whose("default"), "profile default's save");
    }

    #[test]
    fn a_profile_was_last_played_when_its_vault_saw_a_session() {
        use swofty_core::vault::{Event, EventKind};
        let d = data("played");
        let v = Vault::open(&d, "launcher-ids1610").unwrap();
        assert_eq!(super::last_played(&v, &[]), None);
        v.log(&Event::new(500, EventKind::SessionStart));
        v.log(&Event::new(900, EventKind::SessionEnd));
        v.log(&Event::new(950, EventKind::Pinned));
        assert_eq!(super::last_played(&v, &[]), Some(900));
    }

    #[test]
    fn career_states_map_to_the_filters() {
        use super::StatusFilter;
        use swofty_core::vault::careers::State;
        assert_eq!(StatusFilter::of(State::Present), StatusFilter::Present);
        assert_eq!(StatusFilter::of(State::Missing).label(), "Missing");
        assert_eq!(StatusFilter::of(State::DeletedInGame).label(), "Deleted in game");
    }
}
