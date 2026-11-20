use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::shell::notices::{Notification, Notify};
use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::*;
use swofty_core::addons::registry::Host;
use swofty_core::game_folder::baseline::Identity;
use swofty_core::install::Game;
use swofty_core::launcher::Progress;
use swofty_core::profile::Profile;
use swofty_core::store::Store;
use swofty_core::view::{self, Art, Effect, Event, Tone, View};
use swofty_i18n::trf;

use crate::services::game::Install;
use crate::services::library::{Library, SectionItem};
use crate::shell::Shell;
use crate::shell::host::{self as shell_host, Hosted, reports_nothing};
use crate::ui::theme::Palette;
use crate::ui::{badge, monogram};

mod intake;
mod page;

#[derive(Default)]
pub(crate) struct AddonViews {
    pub(crate) views: HashMap<String, Result<Arc<View>, String>>,
    pub(crate) profile: String,
    pub(crate) generation: u64,
    pub(crate) generations: HashMap<String, u64>,
    pub(crate) busy: HashSet<String>,
    pub(crate) open_row: Option<String>,
    /// A tick of the open section is running.
    ticking: bool,
    /// Live polls so far; every fourth one also ticks the open section.
    polls: u64,
}

pub(crate) type Shared = Arc<Mutex<Option<(f32, String)>>>;

pub(crate) struct Reporter(pub(crate) Shared);

impl Progress for Reporter {
    fn report(&self, done: f32, text: &str) {
        if let Ok(mut p) = self.0.lock() {
            *p = Some((done, text.to_owned()));
        }
    }
}

pub(crate) fn game(store: &Store, install: Option<&Install>) -> Option<Game> {
    let install = install?;
    let identity = Identity::detect_cached(&store.root, &install.dir, install.build_id).ok()?;
    Some(Game {
        dir: install.dir.clone(),
        user_reg: None,
        identity,
    })
}

fn host(store: &Store, addon: &str, profile: &Profile, game: Option<&Game>) -> Result<Host, String> {
    Host::new(store, addon, profile, game).map_err(|e| e.to_string())
}

pub(crate) fn icon(i: view::Icon) -> IconName {
    use view::Icon as I;
    match i {
        I::Activity => IconName::Activity,
        I::ArrowRightLeft => IconName::ArrowRightLeft,
        I::ChevronDown => IconName::ChevronDown,
        I::ChevronUp => IconName::ChevronUp,
        I::CircleAlert => IconName::CircleAlert,
        I::CircleCheck => IconName::CircleCheck,
        I::Copy => IconName::Copy,
        I::Eraser => IconName::Eraser,
        I::FolderOpen => IconName::FolderOpen,
        I::Gamepad => IconName::Gamepad2,
        I::Layers => IconName::Layers,
        I::Lock => IconName::Lock,
        I::Map => IconName::Map,
        I::Package => IconName::Package,
        I::Pencil => IconName::Pencil,
        I::Play => IconName::Play,
        I::Plus => IconName::Plus,
        I::Puzzle => IconName::Puzzle,
        I::Settings => IconName::Settings,
        I::ShieldCheck => IconName::ShieldCheck,
        I::Trash => IconName::Trash,
        I::Upload => IconName::Upload,
        I::Calendar => IconName::Calendar,
        I::Car => IconName::Car,
        I::Clock => IconName::Clock,
        I::CloudRain => IconName::CloudRain,
        I::Flag => IconName::Flag,
        I::Gauge => IconName::Gauge,
        I::History => IconName::RotateCcw,
        I::Info => IconName::Info,
        I::Refresh => IconName::RefreshCw,
        I::Save => IconName::Save,
        I::Search => IconName::Search,
        I::Timer => IconName::Timer,
        I::TrendingDown => IconName::TrendingDown,
        I::TrendingUp => IconName::TrendingUp,
        I::TriangleAlert => IconName::TriangleAlert,
        I::Trophy => IconName::Trophy,
        I::Undo => IconName::Undo2,
        I::User => IconName::User,
        I::Users => IconName::Users,
        I::Wallet => IconName::Wallet,
        I::Wrench => IconName::Wrench,
        I::Zap => IconName::Zap,
    }
}

pub(crate) fn tone(t: Tone, p: Palette) -> Hsla {
    match t {
        Tone::Neutral => p.text,
        Tone::Muted => p.text_muted,
        Tone::Accent => p.accent,
        Tone::Good => p.success,
        Tone::Warn => p.warning,
        Tone::Bad => p.error,
    }
}

pub(crate) fn art(a: &Art, size: f32, p: Palette) -> AnyElement {
    match a {
        Art::None => badge(IconName::Package, size, p).into_any_element(),
        Art::Monogram(text) => monogram(text, size, p).into_any_element(),
        Art::Image(path) => div()
            .size(px(size))
            .flex_shrink_0()
            .rounded(px(5.))
            .overflow_hidden()
            .border_1()
            .border_color(p.border_variant)
            .child(img(path.clone()).size_full().object_fit(ObjectFit::Cover))
            .into_any_element(),
    }
}

pub(crate) fn section_key(s: &SectionItem) -> String {
    format!("{}/{}", s.addon, s.spec.id)
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

pub(crate) struct ViewsNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) library: &'a Library,
    /// The game copy the panels read, when one is found.
    pub(crate) install: Option<Install>,
    /// The game folders are known, so helper sections can load before they open.
    pub(crate) detected: bool,
    /// The section open on screen.
    pub(crate) open: Option<usize>,
}

impl Hosted for AddonViews {
    type Needs<'a> = ViewsNeeds<'a>;
    type Change = Infallible;
}

impl shell_host::Host<AddonViews> for Shell {
    fn split(&mut self) -> (&mut AddonViews, ViewsNeeds<'_>) {
        let open = self.open_section();
        let install = self.game.panel_install();
        (
            &mut self.addon_views,
            ViewsNeeds {
                store: &self.store,
                library: &self.library,
                install,
                detected: self.game.detected,
                open,
            },
        )
    }

    reports_nothing!();
}

impl AddonViews {
    pub(crate) fn refresh<H: shell_host::Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        Self::fetch(host, None, cx);
    }

    pub(crate) fn fetch<H: shell_host::Host<Self>>(host: &mut H, only: Option<usize>, cx: &mut Context<H>) {
        let (views, needs) = host.split();
        let store = needs.store.clone();
        let library = needs.library;
        let registry = library.registry.clone();
        let profile = library.profile.clone();
        let install = needs.install;
        let open = needs.open;
        let keys: HashSet<String> = library.sections.iter().map(section_key).collect();
        if views.profile != profile.name {
            views.profile = profile.name.clone();
            views.views.clear();
        }
        views.views.retain(|k, _| keys.contains(k));
        let prefetch = needs.detected;
        let sections: Vec<(String, String)> = library
            .sections
            .iter()
            .enumerate()
            .filter(|(i, _)| only.is_none_or(|o| o == *i))
            .filter(|(i, s)| {
                open == Some(*i) || prefetch || !registry.get(&s.addon).is_some_and(|e| e.helper)
            })
            .map(|(_, s)| (s.addon.clone(), s.spec.id.clone()))
            .collect();
        if sections.is_empty() {
            return;
        }
        views.generation += 1;
        let generation = views.generation;
        cx.spawn(async move |this, cx| {
            let views: Vec<(String, Result<View, String>)> = cx
                .background_executor()
                .spawn(async move {
                    let game = game(&store, install.as_ref());
                    sections
                        .into_iter()
                        .filter_map(|(addon, section)| {
                            let view = registry
                                .get(&addon)
                                .ok_or_else(|| trf!("{addon} is gone", addon = addon))
                                .and_then(|e| {
                                    let host = self::host(&store, &addon, &profile, game.as_ref())?;
                                    let view = match &e.plugin {
                                        Some(plugin) => {
                                            let view = plugin.try_view(&host.cx(), &section);
                                            if view.is_none() {
                                                plugin.mark_stale(Some(&section));
                                            }
                                            view
                                        }
                                        None => Some(e.addon.view(&host.cx(), &section)),
                                    };
                                    Ok(view.map(|v| v.map_err(|e| e.to_string())))
                                });
                            match view {
                                Ok(None) => None,
                                Ok(Some(view)) => Some((format!("{addon}/{section}"), view)),
                                Err(e) => Some((format!("{addon}/{section}"), Err(e))),
                            }
                        })
                        .collect()
                })
                .await;
            this.update(cx, |host, cx| {
                host.split().0.took_views(generation, views);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn live_poll<H: shell_host::Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let views = host.split().0;
        views.polls += 1;
        let ticks = views.polls.is_multiple_of(4);
        Self::refresh_stale(host, cx);
        if ticks {
            Self::tick(host, cx);
        }
    }

    fn refresh_stale<H: shell_host::Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (views, needs) = host.split();
        let (open, library) = (needs.open, needs.library);
        let mut due = Vec::new();
        let mut seen = HashSet::new();
        for s in &library.sections {
            if !seen.insert(s.addon.clone()) {
                continue;
            }
            let Some(plugin) = library.registry.get(&s.addon).and_then(|e| e.plugin.clone()) else {
                continue;
            };
            if plugin.busy() {
                continue;
            }
            let Some(stale) = plugin.take_stale() else {
                continue;
            };
            for (i, t) in library.sections.iter().enumerate() {
                let shown = open == Some(i) || views.views.contains_key(&section_key(t));
                if t.addon == s.addon && shown && stale.covers(&t.spec.id) {
                    due.push(i);
                }
            }
        }
        for i in due {
            Self::fetch(host, Some(i), cx);
        }
    }

    fn tick<H: shell_host::Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (views, needs) = host.split();
        let Some(i) = needs.open else {
            return;
        };
        let Some(s) = needs.library.sections.get(i) else {
            return;
        };
        let ticks = matches!(views.views.get(&section_key(s)), Some(Ok(v)) if v.tick);
        let Some(plugin) = needs
            .library
            .registry
            .get(&s.addon)
            .and_then(|e| e.plugin.clone())
        else {
            return;
        };
        if !ticks || views.ticking || !plugin.ready() {
            return;
        }
        views.ticking = true;
        let store = needs.store.clone();
        let profile = needs.library.profile.clone();
        let install = needs.install;
        let (addon, section) = (s.addon.clone(), s.spec.id.clone());
        cx.spawn(async move |this, cx| {
            let failed = cx
                .background_executor()
                .spawn(async move {
                    let game = game(&store, install.as_ref());
                    let Ok(host) = self::host(&store, &addon, &profile, game.as_ref()) else {
                        return false;
                    };
                    if let Some(Err(e)) = plugin.try_tick(&host.cx(), &section) {
                        eprintln!("swofty-launcher: {addon} tick: {e}");
                    }
                    plugin.failure().is_some()
                })
                .await;
            this.update(cx, |host, cx| {
                host.split().0.ticking = false;
                if failed {
                    Self::fetch(host, Some(i), cx);
                }
                Self::refresh_stale(host, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Takes built views, skipping any a newer fetch has already replaced.
    pub(crate) fn took_views(&mut self, generation: u64, views: Vec<(String, Result<View, String>)>) {
        for (key, view) in views {
            if self.generations.get(&key).is_some_and(|g| *g > generation) {
                continue;
            }
            self.generations.insert(key.clone(), generation);
            self.views.insert(key, view.map(Arc::new));
        }
    }
}

impl Shell {
    /// Sends `startup` to the first section of each addon that asks for it.
    pub(crate) fn run_startup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut seen = std::collections::BTreeSet::new();
        let targets: Vec<(String, String)> = self
            .library
            .sections
            .iter()
            .filter(|s| seen.insert(s.addon.clone()))
            .filter(|s| {
                self.library.addons.iter().any(|a| {
                    a.manifest.id == s.addon && a.manifest.launcher.as_ref().is_some_and(|l| l.startup)
                })
            })
            .map(|s| (s.addon.clone(), s.spec.id.clone()))
            .collect();
        for (addon, section) in targets {
            let control = format!("{addon}/{section}/startup");
            self.send(&addon, &section, control, Event::Startup {}, window, cx);
        }
    }

    pub(crate) fn send(
        &mut self,
        addon: &str,
        section: &str,
        control: String,
        event: Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.library.registry.get(addon).cloned() else {
            return;
        };
        let store = self.store.clone();
        let profile = self.library.profile.clone();
        let install = self.game.panel_install();
        let (addon, section) = (addon.to_owned(), section.to_owned());
        let feed = crate::dialogs::tasks::Feed::default();
        let sink = crate::dialogs::tasks::Sink(feed.clone());
        self.addon_views.busy.insert(control.clone());
        self.follow_tasks(control.clone(), feed.clone(), cx);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let id = addon.clone();
            let effect = cx
                .background_executor()
                .spawn(async move {
                    let game = game(&store, install.as_ref());
                    let host = host(&store, &addon, &profile, game.as_ref())?;
                    entry
                        .addon
                        .event(&host.cx().with_tasks(&sink), &section, event)
                        .map_err(|e| e.to_string())
                })
                .await;
            this.update_in(cx, |s, window, cx| {
                s.addon_views.busy.remove(&control);
                s.settled(&control, &effect);
                let effect = s.finish_tasks(&control, &feed, effect, cx);
                s.effect(&id, effect, window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn effect(
        &mut self,
        addon: &str,
        effect: Result<Effect, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            Ok(Effect::None) => cx.notify(),
            Ok(Effect::Reload) => self.reload(cx),
            Ok(Effect::Notice(text)) => {
                window.notify(Notification::info(text), cx);
                self.reload(cx);
            }
            Ok(Effect::SetAddonEnabled(on)) => self.set_addon_enabled(addon, on, window, cx),
            Ok(Effect::Open(path)) => {
                cx.open_with_system(&path);
                cx.notify();
            }
            Ok(Effect::ShowAddon(shown)) => {
                if let Some(text) = shown.notice() {
                    window.notify(Notification::info(text.to_owned()), cx);
                }
                self.reload(cx);
                self.show_addon(shown.id(), cx);
            }
            Err(e) => {
                crate::app::app_log::action("addon action")
                    .addon(addon)
                    .failed(&e);
                window.notify(Notification::error(e), cx);
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AddonViews;
    use core::prelude::v1::test;

    #[test]
    fn an_older_fetch_does_not_replace_a_newer_view() {
        let mut views = AddonViews::default();
        views.took_views(2, vec![("t.addon/main".into(), Err("new".into()))]);
        views.took_views(1, vec![("t.addon/main".into(), Err("old".into()))]);
        assert!(matches!(views.views.get("t.addon/main"), Some(Err(e)) if e == "new"));
        views.took_views(3, vec![("t.addon/main".into(), Err("newer".into()))]);
        assert!(matches!(views.views.get("t.addon/main"), Some(Err(e)) if e == "newer"));
    }
}
