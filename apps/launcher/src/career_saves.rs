use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Icon, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::vault::careers::{Brief, Diff, Row, Status};
use swofty_core::vault::cloud;
use swofty_core::vault::{self, Location, Notice, Snapshot, Vault};

use crate::addons::ago;
use crate::library::plural;
use crate::pages::{GUTTER, label, tag};
use crate::shell::{Page, Prompt, Shell};
use crate::theme::{Palette, ScrollArea};

#[derive(Default)]
pub(crate) struct SavesPage {
    pub view: Option<SavesView>,
    pub confirm: Option<String>,
    pub banner: Option<Banner>,
}

pub(crate) enum Banner {
    Dropped {
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

pub(crate) struct SavesView {
    source: String,
    layout: vault::Layout,
    dir: PathBuf,
    versions: Vec<Version>,
    careers: Vec<Row>,
    usage: u64,
    cap: u64,
    cloud: Option<cloud::Status>,
    steam_running: bool,
    on: BTreeSet<String>,
    names: BTreeMap<String, String>,
}

fn source_of(root: &std::path::Path) -> Option<String> {
    if let Some(l) = Location::active(root) {
        return Some(l.source);
    }
    match Vault::sources(root).as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

impl SavesPage {
    pub(crate) fn banner_for(root: &std::path::Path) -> Option<Banner> {
        banner_from(root, source_of(root)?)
    }
}

fn banner_from(root: &std::path::Path, source: String) -> Option<Banner> {
    let v = Vault::open(root, &source).ok()?;
    let table = v.careers();
    let shown = table.drop_to_show();
    if let Some(newest) = shown.iter().max_by_key(|r| r.last_seen_at) {
        return Some(Banner::Dropped {
            source,
            snapshot: newest.last_seen.clone(),
            kept_at: newest.last_seen_at,
            names: shown.iter().map(|r| row_name(r)).collect(),
        });
    }
    Notice::read(root).map(Banner::Hint)
}

impl SavesPage {
    pub(crate) fn refresh_banner(&mut self, root: &std::path::Path) {
        self.banner = Self::banner_for(root);
    }
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

impl SavesView {
    pub(crate) fn career_count(&self) -> usize {
        self.careers
            .iter()
            .filter(|r| r.status == Status::Present)
            .count()
    }
}

impl Shell {
    pub(crate) fn refresh_saves(&mut self) {
        let root = self.store.root.clone();
        self.saves.refresh_banner(&root);
        let Some(source) = source_of(&root) else {
            self.saves.view = None;
            return;
        };
        let Ok(v) = Vault::open(&root, &source) else {
            self.saves.view = None;
            return;
        };
        let active = Location::active(&root).filter(|l| l.source == source);
        let versions: Vec<Version> = v
            .list()
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
        let layout = match (&active, versions.first()) {
            (Some(l), _) => l.layout,
            (None, Some(s)) => s.snap.layout,
            (None, None) => return self.saves.view = None,
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
            source,
            layout,
            dir: v.dir.clone(),
            cloud: active
                .as_ref()
                .and_then(|l| vault::save_status(l).ok())
                .map(|s| s.cloud),
            versions,
            careers: v.careers().careers,
            usage: v.usage(),
            cap: v.policy.cap_bytes,
            steam_running: vault::restore::Running::now().steam,
            on,
            names,
        });
    }

    pub(crate) fn open_saves(&mut self, confirm: Option<String>, cx: &mut Context<Self>) {
        self.saves.confirm = confirm;
        self.open_page(Page::Careers, cx);
    }

    pub(crate) fn saves_banner(&self, on_page: bool, p: Palette, cx: &mut Context<Self>) -> Option<Div> {
        let banner = self.saves.banner.as_ref()?;
        let (text, source, snapshot, kept_at) = match banner {
            Banner::Dropped {
                source,
                snapshot,
                kept_at,
                names,
            } => (
                format!(
                    "{} missing from F1 25's save: {}. {} last in the save on {}. Restoring that version brings {} back; the \
                     restore shows what else changes first.",
                    match names.len() {
                        1 => "1 career is".to_owned(),
                        n => format!("{n} careers are"),
                    },
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
                source.clone(),
                snapshot.clone(),
                *kept_at,
            ),
            Banner::Hint(n) => (n.headline(), n.source.clone(), n.snapshot.clone(), n.kept_at),
        };
        let hint = matches!(banner, Banner::Hint(_));
        let review = snapshot.clone();
        let _ = (source, kept_at);
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
                            this.open_saves(Some(review.clone()), cx);
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
                            if let Some(s) = source_of(&root)
                                && let Ok(v) = Vault::open(&root, &s)
                            {
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
            let loc = Location::active(&root)
                .filter(|l| l.source == source)
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

    pub(crate) fn careers_page(&mut self, p: Palette, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let subtitle =
            "Versions of F1 25's save, and the careers in them. Every Play keeps one you can go back to."
                .into();
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
                .child("Versions appear here after the launcher has seen a profile save.")
                .into_any_element(),
            Some(view) => self.saves_body(view, wide, p, cx),
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

    fn saves_body(&self, view: &SavesView, wide: bool, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let heading = |t: &'static str, sub: String| {
            v_flex()
                .gap(px(2.))
                .pt(px(18.))
                .pb(px(10.))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t),
                )
                .child(div().text_size(px(13.)).text_color(p.text_muted).child(sub))
        };
        let shown: Vec<&Row> = view.careers.iter().collect();
        let careers: AnyElement = match shown.is_empty() {
            true => div()
                .p(px(16.))
                .rounded(px(6.))
                .border_1()
                .border_dashed()
                .border_color(p.border)
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child("No careers yet. They show up after a launcher session that loaded a career.")
                .into_any_element(),
            false => div()
                .grid()
                .grid_cols(if wide { 3 } else { 2 })
                .gap(px(12.))
                .children(shown.iter().map(|r| self.career_card(r, view, p)))
                .into_any_element(),
        };
        let versions: Vec<AnyElement> = view
            .versions
            .iter()
            .map(|v| self.version_row(v, view, p, cx))
            .collect();
        v_flex()
            .child(self.storage_line(view, p, cx))
            .child(heading(
                "Careers",
                format!(
                    "{} the launcher has seen in your save.",
                    plural(view.careers.len(), "career", "careers")
                ),
            ))
            .child(careers)
            .child(heading(
                "Versions",
                format!(
                    "{} kept, newest first. Restore puts the whole profile save back.",
                    plural(view.versions.len(), "version", "versions")
                ),
            ))
            .child(v_flex().gap(px(8.)).children(versions))
            .into_any_element()
    }

    fn storage_line(&self, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> Div {
        let dir = view.dir.clone();
        let cloud = match (view.cloud, view.layout) {
            (_, vault::Layout::Ea) => "The EA app copy has no Steam Cloud.".to_owned(),
            (Some(s), _) => {
                let mut t = format!("Steam Cloud: {}", cloud_line(s, view.steam_running));
                if view.steam_running {
                    t.push_str(". Close Steam to restore now.");
                }
                t
            }
            (None, _) => "Steam Cloud: unknown".to_owned(),
        };
        v_flex()
            .gap(px(6.))
            .p(px(12.))
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .text_size(px(13.))
                    .child(Icon::new(IconName::Save).size(px(14.)).text_color(p.text_muted))
                    .child(div().flex_1().min_w_0().child(format!(
                        "Vault: {}, {} of {}.",
                        plural(view.versions.len(), "version", "versions"),
                        mb(view.usage),
                        mb(view.cap)
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
                    ),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(Icon::new(IconName::Cloud).size(px(14.)))
                    .child(cloud),
            )
    }

    fn career_card(&self, r: &Row, view: &SavesView, p: Palette) -> AnyElement {
        let (chip, tone) = match &r.status {
            Status::Present => ("In your save".to_owned(), p.success),
            Status::MissingSince(s) => (
                format!(
                    "Missing since {}",
                    vault::human(
                        view.versions
                            .iter()
                            .find(|v| &v.snap.id == s)
                            .map_or(r.found_at.unwrap_or(r.last_seen_at), |v| v.snap.at)
                    )
                ),
                p.warning,
            ),
            Status::DeletedInGame => ("Deleted in game".to_owned(), p.text_placeholder),
            Status::Archived => ("Archived".to_owned(), p.text_placeholder),
        };
        let who = [r.driver.as_deref(), r.team.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        let needs: Vec<AnyElement> = r
            .needs()
            .map(|id| {
                let off = !view.on.contains(id);
                let name = view.names.get(id).cloned().unwrap_or_else(|| id.to_owned());
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
        v_flex()
            .gap(px(10.))
            .p(px(14.))
            .rounded(px(6.))
            .border_1()
            .border_color(match r.status.is_missing() {
                true => p.warning,
                false => p.border,
            })
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(tag(mode_name(r.mode.as_deref()), p))
                    .child(div().flex_1())
                    .child(div().text_size(px(12.)).text_color(tone).child(chip)),
            )
            .child(
                v_flex()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(r.name().to_owned()),
                    )
                    .when(!who.is_empty(), |d| {
                        d.child(div().text_size(px(13.)).text_color(p.text_muted).child(who))
                    })
                    .when_some(r.place(), |d, place| {
                        d.child(div().text_size(px(13.)).text_color(p.text_muted).child(place))
                    })
                    .when_some(r.last_played, |d, at| {
                        d.child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_placeholder)
                                .child(format!("Last played {}", ago(at as f64))),
                        )
                    }),
            )
            .when(!needs.is_empty(), |d| {
                d.child(
                    v_flex()
                        .gap(px(4.))
                        .child(label("NEEDS", p))
                        .child(h_flex().gap(px(4.)).flex_wrap().children(needs)),
                )
            })
            .into_any_element()
    }

    fn version_row(&self, v: &Version, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let s = &v.snap;
        let id = s.id.clone();
        let pinned = !s.pinned.is_empty();
        let own_pin = s.pinned.iter().any(|x| x == "pinned");
        let confirming = self.saves.confirm.as_deref() == Some(s.id.as_str());
        let held = match &v.held {
            Some(h) if h.is_empty() => "No careers".to_owned(),
            Some(h) => {
                let shown = h.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                match h.len() > 3 {
                    true => format!("{} careers: {shown} and {} more", h.len(), h.len() - 3),
                    false => format!("{}: {shown}", plural(h.len(), "career", "careers")),
                }
            }
            None => "Careers not known for this version".to_owned(),
        };
        let cloud_text = match s.layout {
            vault::Layout::Ea => "local only".to_owned(),
            vault::Layout::Steam => s.cloud().text().to_owned(),
        };
        let (restore_id, source) = (id.clone(), view.source.clone());
        let (pin_id, pin_source) = (id.clone(), view.source.clone());
        let ask_id = id.clone();
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
                                            .child(capital(s.reason.text())),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(p.text_placeholder)
                                            .child(format!("{} · {}", ago(s.at as f64), vault::human(s.at))),
                                    )
                                    .when(pinned, |d| {
                                        d.child(Icon::new(IconName::Star).size(px(13.)).text_color(p.accent))
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(p.text_muted)
                                    .truncate()
                                    .child(format!(
                                        "{} · {} · {} · Steam Cloud {}",
                                        match &s.profile {
                                            Some(pr) => format!("profile {pr}"),
                                            None => "no profile".to_owned(),
                                        },
                                        held,
                                        mb(s.total()),
                                        cloud_text
                                    )),
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
                            .label("Restore this save")
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
                d.child(self.restore_panel(v, view, restore_id, source, p, cx))
            })
            .into_any_element()
    }

    fn restore_panel(
        &self,
        v: &Version,
        view: &SavesView,
        id: String,
        source: String,
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
        let close = match view.layout {
            vault::Layout::Steam => {
                "Exit Steam completely (Steam menu > Exit) and close F1 25 before you restore. Starting Steam afterwards uploads the restored save."
            }
            vault::Layout::Ea => "Close the EA app and F1 25 before you restore.",
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

fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use swofty_core::vault::careers::{Brief, Row, Status};
    use swofty_core::vault::{self, Vault, cloud};

    use super::{Banner, banner_from, cloud_line, list, mode_name};

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
        match banner_from(&d, "steam-1".into()) {
            Some(Banner::Dropped {
                snapshot,
                names,
                kept_at,
                ..
            }) => {
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
        assert!(banner_from(&d, "steam-1".into()).is_none());
    }

    #[test]
    fn no_careers_and_no_notice_means_no_banner() {
        let d = data("none");
        assert!(banner_from(&d, "steam-1".into()).is_none());
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
}
