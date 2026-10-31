use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Disableable, Icon, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::launcher_saves;
use swofty_core::profile::VANILLA;
use swofty_core::vault::careers::{Brief, Diff, Row, Status};
use swofty_core::vault::cloud;
use swofty_core::vault::restore::Running;
use swofty_core::vault::{self, Location, Notice, Reason, Snapshot, Vault};

use crate::addons::ago;
use crate::library::plural;
use crate::pages::{GUTTER, tag};
use crate::shell::{Page, Prompt, Shell};
use crate::theme::{Palette, ScrollArea};

pub(crate) const VANILLA_NOTE: &str =
    "Plays on your Steam save. Careers here show after you play a profile that started from it.";
pub(crate) const NOTICE_TITLE: &str = "Your careers have their own saves now";
pub(crate) const NOTICE_BODY: &str = "Each profile now has its own save, starting from a copy of your Steam save. Playing from Steam still uses your Steam save, and Vanilla plays on it too.";

#[derive(Default)]
pub(crate) struct SavesPage {
    pub view: Option<SavesView>,
    pub confirm: Option<String>,
    pub banner: Option<Banner>,
    pub selected: Option<String>,
    pub game_running: bool,
    notice_done: bool,
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

struct Card {
    profile: String,
    active: bool,
    loc: Option<Location>,
    launcher_off: bool,
    careers: Vec<Row>,
    last_played: Option<i64>,
    has_save: bool,
    snaps: Vec<Snapshot>,
}

impl SavesView {
    pub(crate) fn career_count(&self) -> usize {
        self.cards
            .iter()
            .flat_map(|c| &c.careers)
            .filter(|r| r.status == Status::Present)
            .count()
    }
}

impl Card {
    fn vanilla(&self) -> bool {
        self.profile == VANILLA
    }
}

pub(crate) struct SavesView {
    cards: Vec<Card>,
    selected: String,
    versions: Vec<Version>,
    dir: PathBuf,
    usage: u64,
    cap: u64,
    cloud: Option<cloud::Status>,
    steam: Option<Location>,
    running: Running,
    on: BTreeSet<String>,
    names: BTreeMap<String, String>,
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

fn shown_career(r: &Row) -> bool {
    matches!(
        r.status,
        Status::Present | Status::MissingSince(_) | Status::DeletedInGame
    )
}

fn card_careers(v: &Vault) -> Vec<Row> {
    v.careers().careers.into_iter().filter(shown_career).collect()
}

impl Shell {
    pub(crate) fn refresh_saves(&mut self) {
        let root = self.store.root.clone();
        let active_name = self.library.profile.name.clone();
        self.saves.refresh_banner(&root, &active_name);
        let running = Running::now();
        let active_loc = Location::active(&root);
        let mut names = self.store.profile_names().unwrap_or_default();
        names.sort_by_key(|n| (*n != active_name, n != VANILLA, n.clone()));
        let cards: Vec<Card> = names
            .into_iter()
            .map(|name| {
                let vanilla = name == VANILLA;
                let loc = match vanilla {
                    true => active_loc.clone(),
                    false => Some(Location::launcher(&root, &name)),
                };
                let launcher_off = !vanilla && self.store.profile(&name).is_ok_and(|p| !p.launcher_saves);
                let vault = loc.as_ref().and_then(|l| Vault::of(&root, l).ok());
                let careers: Vec<Row> = vault.as_ref().map(card_careers).unwrap_or_default();
                let snaps = vault.as_ref().and_then(|v| v.list().ok()).unwrap_or_default();
                let last_played = careers.iter().filter_map(|r| r.last_played).max().or_else(|| {
                    snaps
                        .iter()
                        .find(|s| s.reason == Reason::AfterSession)
                        .map(|s| s.at)
                });
                Card {
                    active: name == active_name,
                    has_save: loc.as_ref().is_some_and(launcher_saves::has_files),
                    profile: name,
                    loc,
                    launcher_off,
                    careers,
                    last_played,
                    snaps,
                }
            })
            .collect();
        let selected = self
            .saves
            .selected
            .clone()
            .filter(|s| cards.iter().any(|c| &c.profile == s))
            .or_else(|| cards.first().map(|c| c.profile.clone()));
        let Some(selected) = selected else {
            self.saves.view = None;
            return;
        };
        self.saves.selected = Some(selected.clone());
        let Some(card) = cards.iter().find(|c| c.profile == selected) else {
            self.saves.view = None;
            return;
        };
        let steam = active_loc.clone().filter(|l| l.layout == vault::Layout::Steam);
        let (versions, dir, usage, cap) = match card.loc.as_ref().and_then(|l| Vault::of(&root, l).ok()) {
            Some(v) => {
                let versions = card
                    .snaps
                    .iter()
                    .cloned()
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
        let installed = self.store.addons().unwrap_or_default();
        let on: BTreeSet<String> = match self
            .store
            .profile(&self.library.profile.name)
            .ok()
            .and_then(|p| swofty_core::resolver::resolve(&p, &installed).ok())
        {
            Some(plan) => plan.order.into_iter().collect(),
            None => self.library.profile.enabled().map(str::to_owned).collect(),
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
            selected,
            cards,
            versions,
            dir,
            usage,
            cap,
            running,
            on,
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
            self.saves.selected = profile;
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

    pub(crate) fn careers_page(&mut self, p: Palette, _wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let subtitle = "Each profile has its own save. Vanilla plays on your Steam save.".into();
        let banner = self.saves_banner(true, p, cx);
        let body = match self.saves.view.as_ref() {
            None => v_flex()
                .gap(px(6.))
                .py(px(40.))
                .items_center()
                .text_color(p.text_muted)
                .text_size(px(13.))
                .child(Icon::new(IconName::Save).size(px(20.)))
                .child("No F1 25 save found yet.")
                .into_any_element(),
            Some(view) => self.saves_body(view, p, cx),
        };
        v_flex()
            .size_full()
            .child(self.header(Page::Careers.label(), subtitle, div(), p))
            .children(banner)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("career-saves")
                    .px(px(GUTTER))
                    .pt(px(4.))
                    .pb(px(24.))
                    .child(body),
            )
            .into_any_element()
    }

    fn saves_body(&self, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let cards: Vec<AnyElement> = view
            .cards
            .iter()
            .map(|c| self.saves_card(c, view, p, cx))
            .collect();
        let versions: Vec<AnyElement> = view
            .versions
            .iter()
            .map(|v| self.version_row(v, view, p, cx))
            .collect();
        v_flex()
            .gap(px(12.))
            .children(cards)
            .child(v_flex().gap(px(2.)).pt(px(14.)).pb(px(6.)).child(
                div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(
                    match view.selected == VANILLA {
                        true => "Versions of your Steam save".to_owned(),
                        false => format!("Versions of {}", view.selected),
                    },
                ),
            ))
            .child(self.storage_line(view, p, cx))
            .child(match versions.is_empty() {
                true => div()
                    .p(px(16.))
                    .rounded(px(6.))
                    .border_1()
                    .border_dashed()
                    .border_color(p.border)
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
            })
            .into_any_element()
    }

    fn selected_layout(&self, view: &SavesView) -> vault::Layout {
        view.cards
            .iter()
            .find(|c| c.profile == view.selected)
            .and_then(|c| c.loc.as_ref().map(|l| l.layout))
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

    fn saves_card(&self, c: &Card, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let selected = c.profile == view.selected;
        let running = view.running;
        let game_blocked = running.game || self.saves.game_running;
        let name = c.profile.clone();
        let others: Vec<String> = view
            .cards
            .iter()
            .filter(|o| !o.vanilla() && o.profile != c.profile)
            .map(|o| o.profile.clone())
            .collect();
        let careers: Vec<AnyElement> = c
            .careers
            .iter()
            .map(|r| self.career_line(r, c, view, p))
            .collect();
        let can_copy = c.has_save
            && c.loc
                .as_ref()
                .is_some_and(|l| launcher_saves::supported(l.layout));
        let copy_from = c.loc.clone();
        let copy_name = name.clone();
        let shell = cx.entity().downgrade();
        let copy_btn = Button::new(SharedString::from(format!("copy-{name}")))
            .outline()
            .small()
            .icon(IconName::Copy)
            .label("Copy careers to…")
            .dropdown_caret(true)
            .disabled(!can_copy || others.is_empty() || game_blocked)
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for target in &others {
                    let (shell, from, from_name, to) = (
                        shell.clone(),
                        copy_from.clone(),
                        copy_name.clone(),
                        target.clone(),
                    );
                    menu = menu.item(PopupMenuItem::new(target.clone()).on_click(move |_, window, cx| {
                        let Some(from) = from.clone() else { return };
                        let (from_name, to) = (from_name.clone(), to.clone());
                        shell
                            .update(cx, |s, cx| s.ask_copy(from, from_name, to, window, cx))
                            .ok();
                    }));
                }
                menu
            });
        let steam_loc = view.steam.clone();
        let import_name = name.clone();
        let import_btn = Button::new(SharedString::from(format!("import-{name}")))
            .outline()
            .small()
            .icon(IconName::Download)
            .label("Import from Steam")
            .disabled(steam_loc.is_none() || game_blocked)
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(steam) = steam_loc.clone() {
                    this.ask_copy(steam, VANILLA.to_owned(), import_name.clone(), window, cx);
                }
            }));
        let export_name = name.clone();
        let export_blocked = game_blocked || running.steam;
        let export_btn = Button::new(SharedString::from(format!("export-{name}")))
            .outline()
            .small()
            .icon(IconName::Upload)
            .label("Export to Steam")
            .disabled(view.steam.is_none() || !c.has_save || export_blocked)
            .on_click(
                cx.listener(move |this, _, window, cx| this.ask_export(export_name.clone(), window, cx)),
            );
        let versions_name = name.clone();
        let versions_btn = Button::new(SharedString::from(format!("versions-{name}")))
            .small()
            .icon(IconName::Clock)
            .label("Versions")
            .map(|b| match selected {
                true => b.primary(),
                false => b.outline(),
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.saves.selected = Some(versions_name.clone());
                this.saves.confirm = None;
                this.refresh_saves();
                cx.notify();
            }));
        let waiting = match (game_blocked, running.steam) {
            (true, _) => Some("Close F1 25 to copy, import or export."),
            (false, true) => Some("Close Steam to export."),
            _ => None,
        };
        let cloud = match (c.vanilla(), view.cloud) {
            (true, Some(s)) => Some(format!("Steam Cloud: {}", cloud_line(s, running.steam))),
            _ => None,
        };
        v_flex()
            .gap(px(12.))
            .p(px(14.))
            .rounded(px(6.))
            .border_1()
            .border_color(match selected {
                true => p.accent,
                false => p.border,
            })
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(c.profile.clone()),
                    )
                    .when(c.vanilla(), |d| d.child(tag("Steam save", p)))
                    .when(c.active, |d| d.child(tag("Active", p)))
                    .when(c.launcher_off, |d| d.child(tag("Plays on the Steam save", p)))
                    .child(div().flex_1())
                    .child(div().text_size(px(12.)).text_color(p.text_placeholder).child(
                        match c.last_played {
                            Some(at) => format!("Last played {}", ago(at as f64)),
                            None => "Not played yet".to_owned(),
                        },
                    )),
            )
            .when(c.vanilla(), |d| {
                d.child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .child(VANILLA_NOTE),
                )
            })
            .child(match careers.is_empty() {
                true => div()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(match c.vanilla() {
                        true => "No careers known yet",
                        false => "Careers show here after you play this profile",
                    })
                    .into_any_element(),
                false => v_flex().gap(px(6.)).children(careers).into_any_element(),
            })
            .children(cloud.map(|t| {
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(Icon::new(IconName::Cloud).size(px(14.)))
                    .child(t)
            }))
            .child(
                h_flex()
                    .gap(px(8.))
                    .flex_wrap()
                    .items_center()
                    .child(copy_btn)
                    .when(!c.vanilla(), |d| d.child(import_btn).child(export_btn))
                    .child(div().flex_1())
                    .child(versions_btn),
            )
            .children(waiting.map(|t| div().text_size(px(12.)).text_color(p.text_placeholder).child(t)))
            .into_any_element()
    }

    fn career_line(&self, r: &Row, c: &Card, view: &SavesView, p: Palette) -> AnyElement {
        let who = [r.driver.as_deref(), r.team.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let detail = [Some(who).filter(|w| !w.is_empty()), r.place()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let off: Vec<String> = match c.active {
            true => r
                .needs()
                .filter(|id| !view.on.contains(*id))
                .map(|id| view.names.get(id).cloned().unwrap_or_else(|| id.to_owned()))
                .collect(),
            false => Vec::new(),
        };
        h_flex()
            .gap(px(8.))
            .items_center()
            .flex_wrap()
            .child(tag(mode_name(r.mode.as_deref()), p))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(r.name().to_owned()),
            )
            .when(!detail.is_empty(), |d| {
                d.child(div().text_size(px(13.)).text_color(p.text_muted).child(detail))
            })
            .when_some(r.last_played, |d, at| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child(format!("played {}", ago(at as f64))),
                )
            })
            .when(r.status.is_missing(), |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.warning)
                        .child("missing from the save"),
                )
            })
            .when(r.status == Status::DeletedInGame, |d| {
                d.child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child("deleted in game"),
                )
            })
            .children(off.into_iter().map(|n| {
                tag(format!("{n} is off"), p)
                    .text_color(p.error)
                    .border_color(p.error)
            }))
            .into_any_element()
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
        let restore_label = match (&blocker, view.running.game || self.saves.game_running) {
            (None, _) => "Restore",
            (Some(_), true) => "Close F1 25 to restore",
            (Some(_), false) => "Close Steam to restore",
        };
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
                                    .label(restore_label)
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
}
