//! The trail of views the browser walks back along, and loading a publisher's profile.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::*;
use swofty_core::catalog::Trouble;
use swofty_core::catalog::publishers::{self, Profile};
use swofty_i18n::{trf, trn};

use super::View;
use crate::features::browser::*;
use crate::shell::host::Host;
use crate::shell::{Page, Shell};

impl Shell {
    /// Shows `to`, remembering the current view and its scroll to come back to.
    /// A view already on the trail is gone back to instead.
    pub(in crate::features::browser) fn browse(&mut self, to: View, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        if b.view == to {
            return;
        }
        if let Some(i) = b.trail.iter().position(|(v, _)| *v == to) {
            self.browse_back_to(i, cx);
            return;
        }
        let from = std::mem::replace(&mut b.view, to.clone());
        b.trail.push((from, b.scroll.offset()));
        b.smooth.cancel();
        b.scroll.set_offset(Point::default());
        b.activity_open = false;
        Browser::load_view(self, &to, cx);
        cx.notify();
    }

    pub(super) fn browse_back_to(&mut self, i: usize, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        let Some((view, offset)) = b.trail.get(i).cloned() else {
            return;
        };
        b.trail.truncate(i);
        b.activity_open = false;
        if let View::Installed(id) = view {
            b.view = View::Grid;
            b.trail.clear();
            b.smooth.cancel();
            b.scroll.set_offset(Point::default());
            self.navigation.open_addon = Some(id);
            self.go(Page::Addons, cx);
            return;
        }
        b.view = view.clone();
        b.smooth.cancel();
        b.scroll.set_offset(offset);
        Browser::load_view(self, &view, cx);
        cx.notify();
    }

    /// Opens the Creators list as a section of its own: back from it is All addons.
    pub(super) fn browse_creators(&mut self, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        if b.view == View::Creators {
            return;
        }
        let grid = match (&b.view, b.trail.first()) {
            (View::Grid, _) => b.scroll.offset(),
            (_, Some((View::Grid, offset))) => *offset,
            _ => Point::default(),
        };
        b.trail = vec![(View::Grid, grid)];
        b.view = View::Creators;
        b.smooth.cancel();
        b.scroll.set_offset(Point::default());
        b.activity_open = false;
        cx.notify();
    }

    pub(super) fn browse_back(&mut self, cx: &mut Context<Self>) {
        match self.browser.trail.len().checked_sub(1) {
            Some(i) => self.browse_back_to(i, cx),
            None => self.browse(View::Grid, cx),
        }
    }

    /// Opens `to` in the browser from the Addons page, with `id` open there to come back to.
    pub(crate) fn browse_from_addons(&mut self, id: &str, to: View, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        b.trail = vec![(View::Installed(id.to_owned()), Point::default())];
        b.view = to.clone();
        b.smooth.cancel();
        b.scroll.set_offset(Point::default());
        b.activity_open = false;
        Browser::load_view(self, &to, cx);
        self.go(Page::Browser, cx);
    }
}

impl Browser {
    pub(in crate::features::browser) fn load_view<H: Host<Self>>(
        host: &mut H,
        view: &View,
        cx: &mut Context<H>,
    ) {
        match view {
            View::Addon(id) => Self::fetch_details(host, id, cx),
            View::Author(id) => Self::fetch_profile(host, id, cx),
            _ => {}
        }
    }

    pub(in crate::features::browser) fn fetch_profile<H: Host<Self>>(
        host: &mut H,
        id: &str,
        cx: &mut Context<H>,
    ) {
        let (b, needs) = host.split();
        if !b.profile_loading.insert(id.to_owned()) {
            return;
        }
        let id = id.to_owned();
        let dir = avatar_dir(&needs.store.root);
        let known: BTreeSet<String> = b.avatars.keys().cloned().collect();
        let started = Instant::now();
        cx.spawn(async move |this, cx| {
            let job = id.clone();
            let (result, faces) = cx
                .background_executor()
                .spawn(async move {
                    let result = publishers::fetch_profile(&job);
                    let faces = match &result {
                        Ok(pr) => images(&dir, pr.avatar.iter(), &known),
                        Err(_) => Vec::new(),
                    };
                    (result, faces)
                })
                .await;
            this.update(cx, |host, cx| {
                let (b, needs) = host.split();
                b.take_profile(&needs.store.root, id, result, faces, started.elapsed());
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn take_profile(
        &mut self,
        root: &Path,
        id: String,
        result: Result<Profile, Trouble>,
        faces: Vec<(String, Arc<Image>)>,
        took: Duration,
    ) {
        self.profile_loading.remove(&id);
        match result {
            Ok(pr) => {
                let detail = format!(
                    "{} · {}",
                    trn(pr.addons.len(), "{count} addon", "{count} addons"),
                    trn(pr.links.len(), "{count} link", "{count} links")
                );
                self.record(
                    root,
                    Op::Profile,
                    true,
                    trf!("Fetched {name}'s profile", name = pr.name),
                    detail,
                    took,
                );
                self.profile_failed.remove(&id);
                self.profiles.insert(id, pr);
            }
            Err(e) => {
                self.record(
                    root,
                    Op::Profile,
                    false,
                    trf!("Couldn't fetch publisher {id}", id = id),
                    e.to_string(),
                    took,
                );
                self.profile_failed.insert(id, e.to_string());
            }
        }
        self.avatars.extend(faces);
    }
}
