use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crate::scale::px;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use swofty_core::catalog::publishers::{self, Creator, Profile};
use swofty_core::catalog::stars::Stars;
use swofty_core::catalog::{self, Addon, Trouble, Update};
use swofty_i18n::trn;

use crate::shell::Shell;

use super::authors::View;
use super::*;

pub(crate) struct Browser {
    pub(crate) addons: Vec<Addon>,
    pub(crate) updates: Vec<Update>,
    pub(crate) toasts: Vec<Notification>,
    pub(super) icons: HashMap<String, Arc<Image>>,
    pub(super) checked: Option<Instant>,
    pub(super) fetched_at: Option<SystemTime>,
    pub(super) live: bool,
    pub(super) saved: bool,
    pub(super) fetching: bool,
    pub(super) trouble: Option<Trouble>,
    pub(super) search: Entity<InputState>,
    pub(super) window: AnyWindowHandle,
    pub(super) view: View,
    pub(super) trail: Vec<(View, Point<Pixels>)>,
    pub(super) scroll: ScrollHandle,
    pub(super) grid_scroll: ListState,
    pub(super) grid_cache: Option<GridCache>,
    pub(super) rows: rows::Rows,
    pub(super) catalog_revision: u64,
    pub(super) facet_counts: RefCell<HashMap<Facet, usize>>,
    pub(super) needs_cache: RefCell<HashMap<String, AddonNeeds>>,
    pub(super) grid_layout: (Arc<[usize]>, usize, bool, u32),
    pub(crate) smooth: crate::scroll::SmoothScroll,
    pub(super) details: HashMap<String, Addon>,
    pub(super) loading: BTreeSet<String>,
    pub(super) asked: HashSet<String>,
    pub(super) history: HashSet<String>,
    pub(super) working: HashMap<String, Job>,
    pub(super) queue: crate::download_queue::Queue<(Addon, catalog::Version, String)>,
    pub(super) failed: HashMap<String, String>,
    pub(super) refused: BTreeSet<String>,
    pub(super) batch: Option<Batch>,
    pub(super) held: BTreeMap<String, (semver::Version, String)>,
    pub(super) ticking: bool,
    pub(super) stars: Stars,
    pub(super) syncing: bool,
    pub(super) sync_again: bool,
    pub(super) star_gen: u64,
    pub(super) sort: Sort,
    pub(super) search_sort: Option<Sort>,
    pub(super) tab: Tab,
    pub(super) facets: BTreeSet<Facet>,
    pub(super) hide_code: bool,
    pub(super) activity: VecDeque<Activity>,
    pub(super) activity_open: bool,
    pub(super) creators: Vec<Creator>,
    pub(super) profiles: HashMap<String, Profile>,
    pub(super) profile_loading: BTreeSet<String>,
    pub(super) profile_failed: HashMap<String, String>,
    pub(super) author_sort: Sort,
    pub(super) avatars: HashMap<String, Arc<Image>>,
    pub(super) _search: Subscription,
}

impl Browser {
    pub(crate) fn new(root: &Path, window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let search = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search names, authors, author: or category:"));
        let changed = cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        });
        let cached = catalog::cached(root);
        Self {
            saved: cached.is_some(),
            fetched_at: cached.as_ref().and_then(|_| catalog::cached_at(root)),
            addons: cached.unwrap_or_default(),
            updates: Vec::new(),
            toasts: Vec::new(),
            icons: HashMap::new(),
            checked: None,
            live: false,
            fetching: false,
            trouble: None,
            search,
            window: window.window_handle(),
            view: View::Grid,
            trail: Vec::new(),
            scroll: ScrollHandle::new(),
            grid_scroll: ListState::new(0, ListAlignment::Top, px(200.)),
            grid_layout: Default::default(),
            rows: Default::default(),
            grid_cache: None,
            catalog_revision: 0,
            facet_counts: RefCell::default(),
            needs_cache: RefCell::default(),
            smooth: Default::default(),
            details: HashMap::new(),
            loading: BTreeSet::new(),
            asked: HashSet::new(),
            history: HashSet::new(),
            working: HashMap::new(),
            queue: {
                let mut queue = crate::download_queue::Queue::default();
                queue.set_limit(swofty_core::store::Store::new(root).config().download_jobs());
                queue
            },
            failed: HashMap::new(),
            refused: BTreeSet::new(),
            batch: None,
            held: BTreeMap::new(),
            ticking: false,
            stars: Stars::load(root),
            syncing: false,
            sync_again: false,
            star_gen: 0,
            sort: Sort::Stars,
            search_sort: None,
            tab: None,
            facets: BTreeSet::new(),
            hide_code: false,
            activity: VecDeque::new(),
            activity_open: false,
            creators: publishers::cached(root).unwrap_or_default(),
            profiles: HashMap::new(),
            profile_loading: BTreeSet::new(),
            profile_failed: HashMap::new(),
            author_sort: Sort::Stars,
            avatars: HashMap::new(),
            _search: changed,
        }
    }

    pub(super) fn stale(&self) -> bool {
        !self.fetching && self.checked.is_none_or(|t| t.elapsed() >= STALE)
    }

    pub(super) fn subtitle(&self) -> String {
        let base = catalog::base_url();
        let n = trn(self.addons.len(), "{count} addon", "{count} addons");
        let age = self.fetched_at.map(since_time);
        if self.fetching && self.fetched_at.is_none() {
            return format!("Checking {}…", host(&base));
        }
        let mut text = match (&self.trouble, self.saved) {
            (None, _) => format!("{n} from {}", host(&base)),
            (Some(_), true) if !self.live => format!("Offline · showing the {n} saved last time"),
            (Some(_), true) => format!("Offline · showing {n} from {}", host(&base)),
            (Some(_), false) => "Offline · no addons saved yet".to_owned(),
        };
        if let Some(age) = age.filter(|_| self.saved) {
            text.push_str(&format!(" · updated {age}"));
        }
        if self.fetching {
            text.push_str(" · checking…");
        }
        text
    }

    pub(crate) fn has(&self, id: &str) -> bool {
        self.find(id).is_some()
    }

    pub(super) fn find(&self, id: &str) -> Option<&Addon> {
        self.addons.iter().find(|a| a.id == id)
    }

    pub(super) fn stars_of(&self, a: &Addon) -> u64 {
        self.stars.count(&a.id, a.stars)
    }

    pub(super) fn clear_search(&self, cx: &mut App) {
        let (input, window) = (self.search.clone(), self.window);
        cx.defer(move |cx| {
            window
                .update(cx, |_, window, cx| {
                    input.update(cx, |s, cx| s.set_value("", window, cx))
                })
                .ok();
        });
    }
}
