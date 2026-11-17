//! The trail of views the browser walks back along, and loading a publisher's profile.

use std::collections::BTreeSet;
use std::time::Instant;

use gpui_kit::*;
use swofty_core::catalog::publishers::{self};
use swofty_i18n::{trf, trn};

use super::View;
use crate::features::browser::*;
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
        self.load_view(&to, cx);
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
        self.load_view(&view, cx);
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
        self.load_view(&to, cx);
        self.go(Page::Browser, cx);
    }

    pub(super) fn load_view(&mut self, view: &View, cx: &mut Context<Self>) {
        match view {
            View::Addon(id) => self.fetch_details(id, cx),
            View::Author(id) => self.fetch_profile(id, cx),
            _ => {}
        }
    }

    pub(super) fn fetch_profile(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.browser.profile_loading.insert(id.to_owned()) {
            return;
        }
        let id = id.to_owned();
        let dir = avatar_dir(&self.store.root);
        let known: BTreeSet<String> = self.browser.avatars.keys().cloned().collect();
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
            this.update(cx, |s, cx| {
                s.browser.profile_loading.remove(&id);
                let took = started.elapsed();
                match result {
                    Ok(pr) => {
                        let detail = format!(
                            "{} · {}",
                            trn(pr.addons.len(), "{count} addon", "{count} addons"),
                            trn(pr.links.len(), "{count} link", "{count} links")
                        );
                        s.record(
                            Op::Profile,
                            true,
                            trf!("Fetched {name}'s profile", name = pr.name),
                            detail,
                            took,
                        );
                        s.browser.profile_failed.remove(&id);
                        s.browser.profiles.insert(id, pr);
                    }
                    Err(e) => {
                        s.record(
                            Op::Profile,
                            false,
                            trf!("Couldn't fetch publisher {id}", id = id),
                            e.to_string(),
                            took,
                        );
                        s.browser.profile_failed.insert(id, e.to_string());
                    }
                }
                s.browser.avatars.extend(faces);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}
