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

/// Where a step along the trail lands.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Landing {
    /// A view of the browser, scrolled to where it was left.
    View(View, Point<Pixels>),
    /// The Addons page, with this addon's panel open.
    Addons(String),
}

/// The view the browser shows and the views before it, each with the scroll it was left at.
#[derive(Clone, Debug, Default)]
pub(crate) struct Trail {
    view: View,
    steps: Vec<(View, Point<Pixels>)>,
}

impl Trail {
    pub(crate) fn view(&self) -> &View {
        &self.view
    }

    pub(crate) fn steps(&self) -> &[(View, Point<Pixels>)] {
        &self.steps
    }

    /// Every view but the grid has a Back.
    pub(crate) fn has_back(&self) -> bool {
        self.view != View::Grid
    }

    /// Shows `to`, remembering the current view, scrolled to `scrolled`, to come back to.
    /// A view already on the trail is gone back to instead.
    pub(crate) fn open(&mut self, to: View, scrolled: Point<Pixels>) -> Option<Landing> {
        if self.view == to {
            return None;
        }
        if let Some(i) = self.steps.iter().position(|(v, _)| *v == to) {
            return self.back_to(i);
        }
        let from = std::mem::replace(&mut self.view, to.clone());
        self.steps.push((from, scrolled));
        Some(Landing::View(to, Point::default()))
    }

    /// Goes back to the `i`th view on the trail, leaving the views after it.
    pub(crate) fn back_to(&mut self, i: usize) -> Option<Landing> {
        let (to, scrolled) = self.steps.get(i).cloned()?;
        self.steps.truncate(i);
        if let View::Installed(id) = to {
            *self = Self::default();
            return Some(Landing::Addons(id));
        }
        self.view = to.clone();
        Some(Landing::View(to, scrolled))
    }

    pub(crate) fn back(&mut self) -> Option<Landing> {
        match self.steps.len().checked_sub(1) {
            Some(i) => self.back_to(i),
            None => self.open(View::Grid, Point::default()),
        }
    }

    /// Opens the Creators list as a section of its own: back from it is All addons.
    pub(crate) fn creators(&mut self, scrolled: Point<Pixels>) -> Option<Landing> {
        if self.view == View::Creators {
            return None;
        }
        let grid = match (&self.view, self.steps.first()) {
            (View::Grid, _) => scrolled,
            (_, Some((View::Grid, at))) => *at,
            _ => Point::default(),
        };
        self.steps = vec![(View::Grid, grid)];
        self.view = View::Creators;
        Some(Landing::View(View::Creators, Point::default()))
    }

    /// Opens `to` from the Addons page, with `id` open there to come back to.
    pub(crate) fn open_from_addons(&mut self, id: &str, to: View) -> Landing {
        self.steps = vec![(View::Installed(id.to_owned()), Point::default())];
        self.view = to.clone();
        Landing::View(to, Point::default())
    }
}

impl Shell {
    pub(in crate::features::browser) fn browse(&mut self, to: View, cx: &mut Context<Self>) {
        let scrolled = self.browser.scroll.offset();
        if let Some(landing) = self.browser.trail.open(to, scrolled) {
            self.land(landing, cx);
        }
    }

    pub(super) fn browse_back_to(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(landing) = self.browser.trail.back_to(i) {
            self.land(landing, cx);
        }
    }

    pub(super) fn browse_creators(&mut self, cx: &mut Context<Self>) {
        let scrolled = self.browser.scroll.offset();
        if let Some(landing) = self.browser.trail.creators(scrolled) {
            self.land(landing, cx);
        }
    }

    pub(crate) fn browse_back(&mut self, cx: &mut Context<Self>) {
        if let Some(landing) = self.browser.trail.back() {
            self.land(landing, cx);
        }
    }

    pub(crate) fn browse_from_addons(&mut self, id: &str, to: View, cx: &mut Context<Self>) {
        let landing = self.browser.trail.open_from_addons(id, to);
        self.land(landing, cx);
        self.go(Page::Browser, cx);
    }

    fn land(&mut self, landing: Landing, cx: &mut Context<Self>) {
        let b = &mut self.browser;
        b.activity_open = false;
        b.smooth.cancel();
        match landing {
            Landing::Addons(id) => {
                b.scroll.set_offset(Point::default());
                self.navigation.open_addon = Some(id);
                self.go(Page::Addons, cx);
            }
            Landing::View(view, scrolled) => {
                b.scroll.set_offset(scrolled);
                Browser::load_view(self, &view, cx);
                cx.notify();
            }
        }
    }
}

impl Browser {
    pub(crate) fn has_back(&self) -> bool {
        self.trail.has_back()
    }

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

#[cfg(test)]
mod tests {
    use gpui_kit::{Pixels, Point, point, px};

    use super::{Landing, Trail};
    use crate::features::browser::authors::View;

    fn down(y: f32) -> Point<Pixels> {
        point(px(0.), px(y))
    }

    fn addon(id: &str) -> View {
        View::Addon(id.to_owned())
    }

    #[test]
    fn back_from_an_addon_returns_to_the_grid() {
        let mut t = Trail::default();
        assert!(!t.has_back());
        assert_eq!(
            t.open(addon("a"), down(0.)),
            Some(Landing::View(addon("a"), down(0.)))
        );
        assert!(t.has_back());
        assert_eq!(t.back(), Some(Landing::View(View::Grid, down(0.))));
        assert_eq!(t.view(), &View::Grid);
        assert!(t.steps().is_empty());
        assert!(!t.has_back());
        assert_eq!(t.back(), None);
    }

    #[test]
    fn each_back_scrolls_the_view_it_returns_to_where_it_was_left() {
        let mut t = Trail::default();
        t.open(addon("a"), down(0.));
        t.open(View::Author("x".to_owned()), down(1200.));
        t.open(addon("b"), down(480.));
        assert_eq!(
            t.back(),
            Some(Landing::View(View::Author("x".to_owned()), down(480.)))
        );
        assert_eq!(t.back(), Some(Landing::View(addon("a"), down(1200.))));
        assert_eq!(t.back(), Some(Landing::View(View::Grid, down(0.))));
    }

    #[test]
    fn opening_a_view_on_the_trail_goes_back_to_it() {
        let mut t = Trail::default();
        t.open(addon("a"), down(0.));
        t.open(View::Author("x".to_owned()), down(900.));
        assert_eq!(
            t.open(addon("a"), down(300.)),
            Some(Landing::View(addon("a"), down(900.)))
        );
        assert_eq!(t.steps().len(), 1);
        assert_eq!(t.open(addon("a"), down(0.)), None);
    }

    #[test]
    fn back_from_a_page_opened_on_the_addons_page_returns_there_with_its_panel_open() {
        let mut t = Trail::default();
        assert_eq!(
            t.open_from_addons("mine", addon("mine")),
            Landing::View(addon("mine"), down(0.))
        );
        t.open(View::Author("x".to_owned()), down(640.));
        assert_eq!(t.back(), Some(Landing::View(addon("mine"), down(640.))));
        assert_eq!(t.back(), Some(Landing::Addons("mine".to_owned())));
        assert_eq!(t.view(), &View::Grid);
        assert!(t.steps().is_empty());
    }

    #[test]
    fn back_from_creators_returns_to_the_grid_where_it_was_left() {
        let mut t = Trail::default();
        t.open(addon("a"), down(330.));
        t.open(View::Author("x".to_owned()), down(200.));
        assert_eq!(
            t.creators(down(50.)),
            Some(Landing::View(View::Creators, down(0.)))
        );
        assert_eq!(t.creators(down(50.)), None);
        assert_eq!(t.back(), Some(Landing::View(View::Grid, down(330.))));
    }

    #[test]
    fn a_crumb_goes_back_past_every_view_after_it() {
        let mut t = Trail::default();
        t.open(addon("a"), down(0.));
        t.open(View::Author("x".to_owned()), down(700.));
        t.open(addon("b"), down(20.));
        assert_eq!(t.back_to(1), Some(Landing::View(addon("a"), down(700.))));
        assert_eq!(t.steps().len(), 1);
        assert_eq!(t.back_to(5), None);
    }
}
