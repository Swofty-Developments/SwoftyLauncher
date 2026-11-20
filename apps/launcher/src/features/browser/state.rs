use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crate::shell::notices::Notification;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use swofty_core::catalog::publishers::{self, Creator, Profile};
use swofty_core::catalog::stars::Stars;
use swofty_core::catalog::{self, Addon, Trouble, Update};
use swofty_i18n::{tr, trf, trn};

use super::authors::Trail;
use super::*;
use crate::services::library::Library;
use crate::shell::Shell;
use crate::shell::host::{Host, Hosted};
use crate::ui::scale::px;
use swofty_core::game_folder::busy::Busy;
use swofty_core::store::Store;

pub(crate) struct Browser {
    pub(crate) addons: Vec<Addon>,
    pub(crate) updates: Vec<Update>,
    pub(crate) toasts: Vec<Notification>,
    pub(super) icons: HashMap<String, Arc<Image>>,
    pub(super) search: Entity<InputState>,
    pub(super) window: AnyWindowHandle,
    pub(super) trail: Trail,
    pub(super) scroll: ScrollHandle,
    pub(super) grid_scroll: ListState,
    pub(super) grid_cache: Option<GridCache>,
    pub(super) catalog_revision: u64,
    pub(super) facet_counts: RefCell<HashMap<Facet, usize>>,
    pub(super) needs_cache: RefCell<HashMap<String, AddonNeeds>>,
    pub(super) grid_layout: (Arc<[usize]>, usize, bool, u32),
    pub(crate) smooth: crate::ui::scroll::SmoothScroll,
    pub(super) details: HashMap<String, Addon>,
    pub(super) loading: BTreeSet<String>,
    pub(super) asked: HashSet<String>,
    pub(super) history: HashSet<String>,
    pub(super) working: HashMap<String, Job>,
    pub(super) queue: crate::services::download_queue::Queue<Request>,
    pub(super) failed: HashMap<String, String>,
    pub(super) refused: BTreeSet<String>,
    pub(super) batch: Option<Batch>,
    pub(super) held: BTreeMap<String, (semver::Version, String)>,
    /// The version of each addon the play bar doesn't offer ([`catalog::source::skipped`]).
    pub(super) skipped: BTreeMap<String, semver::Version>,
    pub(super) ticking: bool,
    pub(super) stars: Stars,
    pub(super) sort: Sort,
    pub(super) search_sort: Option<Sort>,
    pub(super) landing: bool,
    pub(super) tab: Tab,
    pub(super) facets: BTreeSet<Facet>,
    pub(super) hide_code: bool,
    pub(super) activity_open: bool,
    pub(super) creators: Vec<Creator>,
    pub(super) profiles: HashMap<String, Profile>,
    pub(super) profile_loading: BTreeSet<String>,
    pub(super) profile_failed: HashMap<String, String>,
    pub(super) author_sort: Sort,
    pub(super) avatars: HashMap<String, Arc<Image>>,
    pub(super) media: HashMap<String, Arc<Image>>,
    pub(super) media_pick: HashMap<String, usize>,
    pub(super) _search: Subscription,
    pub(super) feed: Feed,
}

impl Browser {
    pub(crate) fn new(root: &Path, window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx).placeholder(tr("Search names, authors, author: or category:"))
        });
        let changed = cx.subscribe(&search, |_, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Change) {
                cx.notify();
            }
        });
        let cached = catalog::cached(root);
        let feed = Feed {
            saved: cached.is_some(),
            fetched_at: cached.as_ref().and_then(|_| catalog::cached_at(root)),
            ..Feed::default()
        };
        Self {
            addons: cached.unwrap_or_default(),
            updates: Vec::new(),
            toasts: Vec::new(),
            icons: HashMap::new(),
            search,
            window: window.window_handle(),
            trail: Trail::default(),
            scroll: ScrollHandle::new(),
            grid_scroll: ListState::new(0, ListAlignment::Top, px(200.)),
            grid_layout: Default::default(),
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
                let mut queue = crate::services::download_queue::Queue::default();
                queue.set_limit(swofty_core::store::Store::new(root).config().download_jobs());
                queue
            },
            failed: HashMap::new(),
            refused: BTreeSet::new(),
            batch: None,
            held: BTreeMap::new(),
            skipped: catalog::source::skipped(root),
            ticking: false,
            stars: Stars::load(root),
            sort: Sort::Stars,
            search_sort: None,
            landing: true,
            tab: None,
            facets: BTreeSet::new(),
            hide_code: false,
            activity_open: false,
            creators: publishers::cached(root).unwrap_or_default(),
            profiles: HashMap::new(),
            profile_loading: BTreeSet::new(),
            profile_failed: HashMap::new(),
            author_sort: Sort::Stars,
            avatars: HashMap::new(),
            media: HashMap::new(),
            media_pick: HashMap::new(),
            _search: changed,
            feed,
        }
    }

    pub(super) fn on_landing(&self, searching: bool) -> bool {
        self.landing && self.tab.is_none() && !searching && self.facets.is_empty() && !self.hide_code
    }

    pub(super) fn stale(&self) -> bool {
        !self.feed.fetching && self.feed.checked.is_none_or(|t| t.elapsed() >= STALE)
    }

    pub(super) fn subtitle(&self) -> String {
        let base = catalog::base_url();
        let host = host(&base);
        let count = self.addons.len();
        let addons = trn(count, "{count} addon", "{count} addons");
        if self.feed.fetching && self.feed.fetched_at.is_none() {
            return trf!("Checking {host}…", host = host);
        }
        let mut text = match (&self.feed.trouble, self.feed.saved) {
            (None, _) => trf!("{addons} from {host}", addons = addons, host = host),
            (Some(_), true) if !self.feed.live => trn(
                count,
                "Offline · showing the {count} addon saved last time",
                "Offline · showing the {count} addons saved last time",
            ),
            (Some(_), true) => trf!(
                "Offline · showing {addons} from {host}",
                addons = addons,
                host = host
            ),
            (Some(_), false) => tr("Offline · no addons saved yet").to_owned(),
        };
        if let Some(age) = self.feed.fetched_at.map(since_time).filter(|_| self.feed.saved) {
            text.push_str(&format!(" · {}", trf!("updated {when}", when = age)));
        }
        if self.feed.fetching {
            text.push_str(&format!(" · {}", tr("checking…")));
        }
        text
    }

    pub(crate) fn has(&self, id: &str) -> bool {
        self.find(id).is_some()
    }

    pub(crate) fn find(&self, id: &str) -> Option<&Addon> {
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

/// What fetching the catalog and syncing stars is doing, and what they did.
#[derive(Default)]
pub(crate) struct Feed {
    pub(super) fetching: bool,
    pub(super) checked: Option<Instant>,
    pub(super) fetched_at: Option<SystemTime>,
    pub(super) live: bool,
    pub(super) saved: bool,
    pub(super) trouble: Option<Trouble>,
    pub(super) syncing: bool,
    pub(super) sync_again: bool,
    pub(super) star_gen: u64,
    pub(super) activity: VecDeque<Activity>,
}

impl Feed {
    pub(super) fn begin_fetch(&mut self) -> bool {
        !std::mem::replace(&mut self.fetching, true)
    }

    pub(super) fn fetched(&mut self, trouble: Option<Trouble>, listed: bool) {
        self.fetching = false;
        self.checked = Some(Instant::now());
        self.trouble = trouble;
        if listed {
            self.live = true;
            self.saved = true;
            self.fetched_at = Some(SystemTime::now());
        }
    }

    pub(super) fn begin_sync(&mut self) -> bool {
        if self.syncing {
            self.sync_again = true;
            return false;
        }
        self.syncing = true;
        true
    }

    pub(super) fn synced(&mut self) -> bool {
        self.syncing = false;
        std::mem::take(&mut self.sync_again)
    }

    pub(super) fn next_star(&mut self) -> u64 {
        self.star_gen += 1;
        self.star_gen
    }

    pub(super) fn settled(&self, ticket: u64) -> bool {
        self.star_gen == ticket
    }

    pub(super) fn note(&mut self, activity: Activity) {
        self.activity.push_front(activity);
        self.activity.truncate(ACTIVITY_KEPT);
    }
}

pub(crate) struct BrowserNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) library: &'a Library,
    /// The data folder is moving; nothing fetches or installs until it is done.
    pub(crate) locked: bool,
    /// The game folder build running now, which holds the addons it builds.
    pub(crate) building: Option<&'a Busy>,
}

pub(crate) enum BrowserChange {
    /// An install finished or failed, so the installed addons changed on disk.
    Installed,
    /// An addon installed outside an update batch: its notice offers to turn it on.
    TurnOn { id: String, note: Notification },
}

impl Hosted for Browser {
    type Needs<'a> = BrowserNeeds<'a>;
    type Change = BrowserChange;
}

impl Host<Browser> for Shell {
    fn split(&mut self) -> (&mut Browser, BrowserNeeds<'_>) {
        let locked = self.data_move_locked();
        (
            &mut self.browser,
            BrowserNeeds {
                store: &self.store,
                library: &self.library,
                locked,
                building: self.game.build.as_ref(),
            },
        )
    }

    fn changed(&mut self, change: BrowserChange, cx: &mut Context<Self>) {
        match change {
            BrowserChange::Installed => self.reload(cx),
            BrowserChange::TurnOn { id, note } => {
                let shell = cx.entity().downgrade();
                let note = note.action(tr("Turn on"), move |window, cx| {
                    shell
                        .update(cx, |s, cx| s.set_addon_enabled(&id, true, window, cx))
                        .ok();
                });
                self.browser.toasts.push(note);
            }
        }
    }
}
