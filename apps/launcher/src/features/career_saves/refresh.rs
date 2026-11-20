use std::convert::Infallible;

use gpui_kit::*;
use swofty_core::store::Store;

use super::Banner;
use super::build::build_saves;
use super::state::{Built, SavesPage};
use crate::services::game::Install;
use crate::shell::host::{Host, Hosted, reports_nothing};
use crate::shell::{Page, Shell};

pub(crate) struct SavesNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) install: Option<&'a Install>,
    pub(crate) profile: &'a str,
}

impl Hosted for SavesPage {
    type Needs<'a> = SavesNeeds<'a>;
    type Change = Infallible;
}

impl Host<SavesPage> for Shell {
    fn split(&mut self) -> (&mut SavesPage, SavesNeeds<'_>) {
        (
            &mut self.saves,
            SavesNeeds {
                store: &self.store,
                install: self.game.install.as_ref(),
                profile: &self.library.profile.name,
            },
        )
    }

    reports_nothing!();
}

impl SavesPage {
    pub(crate) fn refresh<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (saves, needs) = host.split();
        let generation = saves.next_generation();
        saves.loading = true;
        let store = needs.store.clone();
        let install = needs.install.cloned();
        let name = needs.profile.to_owned();
        let game_running = saves.game_running;
        cx.spawn(async move |this, cx| {
            let built = cx
                .background_executor()
                .spawn(async move { build_saves(&store, install.as_ref(), &name, game_running) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.took(generation, built) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn refresh_banner<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (saves, needs) = host.split();
        let generation = saves.next_generation();
        let root = needs.store.root.clone();
        let name = needs.profile.to_owned();
        let name_done = name.clone();
        cx.spawn(async move |this, cx| {
            let banner = cx
                .background_executor()
                .spawn(async move { SavesPage::banner_for(&root, &name) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.took_banner(generation, name_done, banner) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn next_generation(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// Takes a built view; false when a newer refresh started since, so this one is dropped.
    pub(super) fn took(&mut self, generation: u64, built: Built) -> bool {
        if self.generation != generation {
            return false;
        }
        self.apply(built);
        true
    }

    fn took_banner(&mut self, generation: u64, profile: String, banner: Option<Banner>) -> bool {
        if self.generation != generation {
            return false;
        }
        self.banner_profile = profile;
        self.banner = banner;
        true
    }
}

impl Shell {
    pub(crate) fn open_saves(&mut self, confirm: Option<String>, cx: &mut Context<Self>) {
        self.saves.confirm = confirm;
        self.open_page(Page::Careers, cx);
    }
}
