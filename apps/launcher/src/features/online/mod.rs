mod api;
mod board;
mod images;
mod landing;
mod mock;
mod model;
mod profile;
mod season;
mod widgets;

use crate::shell::Shell;
use crate::shell::host::{Host, Hosted, reports_nothing};
use api::{BoardsApi, HttpBoards};
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use images::{Images, Job};
use model::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

pub(crate) enum Load<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(OnlineError),
}

impl<T> Load<T> {
    fn from(result: Result<T, OnlineError>) -> Self {
        match result {
            Ok(v) => Self::Ready(v),
            Err(e) => Self::Failed(e),
        }
    }
}

#[derive(Default)]
pub(crate) struct Seen {
    urls: Vec<String>,
    shell: Option<WeakEntity<Shell>>,
    armed: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Target {
    Season(String, String),
    Board(Box<TrackCard>, String),
    Profile(Driver),
}

pub(crate) struct Online {
    api: Arc<dyn BoardsApi>,
    watch_notice: Option<(bool, String)>,
    watch_lap: Option<String>,
    trail: Trail,
    overview: Load<Overview>,
    seasons: HashMap<String, Load<SeasonPage>>,
    profiles: HashMap<String, Load<Profile>>,
    board: Option<(BoardKey, Load<BoardPage>)>,
    category: Category,
    board_category: Option<Category>,
    more: bool,
    images: Images,
    seen: Rc<RefCell<Seen>>,
    wake: Option<Instant>,
    data: std::path::PathBuf,
    recents: Recents,
    filter_tracks: TrackFilter,
    search: Option<Entity<InputState>>,
    my_times: HashMap<BoardKey, u64>,
    feeds: HashMap<String, Feed>,
    handles: HashMap<String, UniformListScrollHandle>,
    me: Option<Me>,
    setup_open: Option<String>,
    setup_compare: bool,
    cursor: Option<usize>,
    items: Vec<Target>,
    epoch: u64,
}

impl Online {
    pub(crate) fn new(data: &std::path::Path) -> Self {
        let api: Arc<dyn BoardsApi> = match mock_mode() {
            Some(mode) => Arc::new(mock::MockBoards { mode }),
            None => Arc::new(HttpBoards::new()),
        };
        Self {
            api,
            watch_notice: None,
            watch_lap: None,
            trail: Trail::new(),
            overview: Load::Idle,
            seasons: HashMap::new(),
            profiles: HashMap::new(),
            board: None,
            category: Category::load(data),
            board_category: None,
            more: false,
            images: Images::new(data, &swofty_core::catalog::base_url()),
            seen: Rc::new(RefCell::new(Seen::default())),
            wake: None,
            data: data.to_path_buf(),
            recents: Recents::load(data),
            filter_tracks: TrackFilter::default(),
            search: None,
            my_times: HashMap::new(),
            feeds: HashMap::new(),
            handles: HashMap::new(),
            me: None,
            setup_open: None,
            setup_compare: false,
            cursor: None,
            items: Vec::new(),
            epoch: 0,
        }
    }
}

fn mock_mode() -> Option<mock::Mode> {
    if cfg!(debug_assertions) {
        mock::Mode::from_env()
    } else {
        None
    }
}

fn art_urls<'a>(drivers: impl IntoIterator<Item = &'a Driver>) -> Vec<String> {
    drivers.into_iter().filter_map(|d| d.avatar.clone()).collect()
}

pub(crate) struct OnlineNeeds<'a> {
    pub(crate) token: Option<&'a str>,
    /// The signed-in player's name, which picks their own profile and times off the boards.
    pub(crate) me: Option<String>,
    pub(crate) game_found: bool,
}

impl Hosted for Online {
    type Needs<'a> = OnlineNeeds<'a>;
    type Change = Infallible;
}

impl Host<Online> for Shell {
    fn split(&mut self) -> (&mut Online, OnlineNeeds<'_>) {
        (
            &mut self.online,
            OnlineNeeds {
                token: self.account.session.as_ref().map(|s| s.token.as_str()),
                me: self.account.display_name(),
                game_found: self.game.install.is_some(),
            },
        )
    }

    reports_nothing!();
}

impl Online {
    pub(crate) fn enter<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        if matches!(host.split().0.overview, Load::Idle) {
            Self::reload(host, cx);
        }
    }

    pub(crate) fn reload<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (o, needs) = host.split();
        o.epoch += 1;
        let epoch = o.epoch;
        o.overview = Load::Loading;
        o.seasons.clear();
        o.profiles.clear();
        o.feeds.clear();
        o.api.set_token(needs.token.map(str::to_owned));
        let me_api = o.api.clone();
        cx.spawn(async move |this, cx| {
            let me = cx.background_executor().spawn(async move { me_api.me() }).await;
            this.update(cx, |host, cx| {
                host.split().0.me = me.ok().flatten();
                cx.notify();
            })
            .ok();
        })
        .detach();
        let api = o.api.clone();
        let cat = o.category;
        cx.spawn(async move |this, cx| {
            let overview = cx
                .background_executor()
                .spawn(async move { api.overview(&cat) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.epoch != epoch {
                    return;
                }
                if let Ok(o) = &overview {
                    let urls = art_urls(
                        o.global
                            .iter()
                            .map(|r| &r.driver)
                            .chain(o.seasons.iter().flat_map(|s| &s.top)),
                    );
                    Self::prefetch(host, urls, cx);
                }
                let me = host.split().1.me;
                if let (Ok(o), Some(me)) = (&overview, me)
                    && let Some(r) = o.global.iter().find(|r| r.driver.name == me)
                {
                    let id = r.driver.id.clone();
                    Self::load_profile(host, &id, cx);
                }
                if let (true, Ok(o)) = (cfg!(debug_assertions), &overview)
                    && let Ok(start) = std::env::var("SWOFTY_ONLINE_START")
                    && let Some(first) = o.seasons.first()
                {
                    let (id, name) = (first.id.clone(), first.name.clone());
                    host.split().0.overview = Load::from(overview.clone());
                    Self::go(host, Target::Season(id.clone(), name), cx);
                    let board = match start.as_str() {
                        "board" => Some(TrackCard::bare(
                            BoardKey {
                                content: id,
                                track: "bahrain".into(),
                                weather: "dry".into(),
                            },
                            "Bahrain".into(),
                        )),
                        "custom" => o.custom.first().cloned(),
                        _ => None,
                    };
                    if let Some(card) = board {
                        let name = card.name.clone();
                        Self::go(host, Target::Board(Box::new(card), name), cx);
                    }
                    return;
                }
                host.split().0.overview = Load::from(overview);
                cx.notify();
            })
            .ok();
        })
        .detach();
        match o.trail.current().clone() {
            Screen::Landing => {}
            Screen::Season(id) => Self::load_season(host, &id, cx),
            Screen::Board(key) => Self::load_board(host, key, 0, cx),
            Screen::Profile(id) => Self::load_profile(host, &id, cx),
        }
        cx.notify();
    }

    fn prefetch<H: Host<Self>>(host: &mut H, urls: Vec<String>, cx: &mut Context<H>) {
        let jobs = host.split().0.images.prefetch(urls, Instant::now());
        Self::run(host, jobs, cx);
    }

    pub(crate) fn pump<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let o = host.split().0;
        let visible = {
            let mut seen = o.seen.borrow_mut();
            seen.armed = false;
            std::mem::take(&mut seen.urls)
        };
        let now = Instant::now();
        let plan = o.images.plan(visible, now);
        Self::run(host, plan.jobs, cx);
        if let Some(wait) = plan.wake {
            Self::wake(host, now + wait, cx);
        }
    }

    fn run<H: Host<Self>>(host: &mut H, jobs: Vec<Job>, cx: &mut Context<H>) {
        if jobs.is_empty() {
            return;
        }
        let (o, needs) = host.split();
        let dir = o.images.dir();
        let token = needs.token.map(str::to_owned);
        for job in jobs {
            let (dir, token) = (dir.clone(), token.clone());
            cx.spawn(async move |this, cx| {
                let url = job.url().to_owned();
                let done = cx
                    .background_executor()
                    .spawn(async move { job.run(&dir, token.as_deref()) })
                    .await;
                this.update(cx, |host, cx| {
                    host.split().0.images.finish(url, done, Instant::now());
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    fn wake<H: Host<Self>>(host: &mut H, at: Instant, cx: &mut Context<H>) {
        let o = host.split().0;
        if o.wake.is_some_and(|w| w <= at) {
            return;
        }
        o.wake = Some(at);
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(at.saturating_duration_since(Instant::now()))
                .await;
            this.update(cx, |host, cx| {
                let o = host.split().0;
                if o.wake == Some(at) {
                    o.wake = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn rank_ensure<H: Host<Self>>(host: &mut H, scope: &str, cx: &mut Context<H>) {
        let feed = host.split().0.feeds.get(scope);
        if feed.is_none_or(|f| !f.started && !f.loading) {
            Self::rank_load(host, scope, 0, cx);
        }
    }

    pub(crate) fn rank_load<H: Host<Self>>(host: &mut H, scope: &str, offset: u32, cx: &mut Context<H>) {
        let o = host.split().0;
        let feed = o.feeds.entry(scope.to_owned()).or_default();
        if feed.loading {
            return;
        }
        feed.loading = true;
        let (api, key, epoch) = (o.api.clone(), scope.to_owned(), o.epoch);
        let cat = o.category;
        cx.spawn(async move |this, cx| {
            let k = key.clone();
            let page = cx
                .background_executor()
                .spawn(async move { api.rankings(&k, &cat, offset) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.epoch != epoch {
                    return;
                }
                if let Ok(p) = &page {
                    let urls = art_urls(p.rows.iter().map(|r| &r.driver));
                    Self::prefetch(host, urls, cx);
                }
                let feed = host.split().0.feeds.entry(key).or_default();
                match page {
                    Ok(p) => feed.apply(offset, p),
                    Err(e) => feed.failed(e.message),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn load_season<H: Host<Self>>(host: &mut H, id: &str, cx: &mut Context<H>) {
        let o = host.split().0;
        if matches!(o.seasons.get(id), Some(Load::Ready(_) | Load::Loading)) {
            return;
        }
        o.seasons.insert(id.to_owned(), Load::Loading);
        Self::rank_ensure(host, id, cx);
        let o = host.split().0;
        let (api, key, epoch) = (o.api.clone(), id.to_owned(), o.epoch);
        let cat = o.category;
        cx.spawn(async move |this, cx| {
            let k = key.clone();
            let page = cx
                .background_executor()
                .spawn(async move { api.season(&k, &cat) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.epoch != epoch {
                    return;
                }
                if let Ok(p) = &page {
                    let urls = art_urls(
                        p.ranking
                            .iter()
                            .map(|r| &r.driver)
                            .chain(p.tracks.iter().flat_map(|t| t.podium.iter().map(|e| &e.driver)))
                            .chain(p.season.top.iter()),
                    );
                    Self::prefetch(host, urls, cx);
                }
                host.split().0.seasons.insert(key, Load::from(page));
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn load_board<H: Host<Self>>(host: &mut H, key: BoardKey, offset: u32, cx: &mut Context<H>) {
        let o = host.split().0;
        if offset == 0 {
            o.board = Some((key.clone(), Load::Loading));
        } else {
            if o.more {
                return;
            }
            o.more = true;
        }
        let (api, filter, epoch) = (o.api.clone(), o.board_category(), o.epoch);
        cx.spawn(async move |this, cx| {
            let k = key.clone();
            let page = cx
                .background_executor()
                .spawn(async move { api.board(&k, &filter, offset) })
                .await;
            this.update(cx, |host, cx| {
                let o = host.split().0;
                o.more = false;
                if o.epoch != epoch || o.board.as_ref().map(|b| &b.0) != Some(&key) {
                    return;
                }
                if let Ok(p) = &page {
                    let urls = art_urls(
                        p.rows
                            .iter()
                            .map(|r| &r.driver)
                            .chain(p.history.iter().map(|h| &h.driver)),
                    );
                    Self::prefetch(host, urls, cx);
                }
                match (offset, page, host.split().0.board.as_mut()) {
                    (0, page, Some(slot)) => slot.1 = Load::from(page),
                    (_, Ok(next), Some((_, Load::Ready(cur)))) => {
                        cur.total = next.total;
                        cur.rows.extend(next.rows);
                    }
                    _ => {}
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn load_profile<H: Host<Self>>(host: &mut H, id: &str, cx: &mut Context<H>) {
        let o = host.split().0;
        if matches!(o.profiles.get(id), Some(Load::Ready(_) | Load::Loading)) {
            return;
        }
        o.profiles.insert(id.to_owned(), Load::Loading);
        let (api, key, epoch) = (o.api.clone(), id.to_owned(), o.epoch);
        let cat = o.category;
        cx.spawn(async move |this, cx| {
            let k = key.clone();
            let page = cx
                .background_executor()
                .spawn(async move { api.profile(&k, &cat) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.epoch != epoch {
                    return;
                }
                if let Ok(p) = &page {
                    Self::prefetch(host, art_urls([&p.driver]), cx);
                }
                let (o, needs) = host.split();
                if let Ok(p) = &page
                    && needs.me.as_deref() == Some(p.driver.name.as_str())
                {
                    o.my_times = p.bests.iter().map(|b| (b.key.clone(), b.time_ms)).collect();
                }
                o.profiles.insert(key, Load::from(page));
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn go<H: Host<Self>>(host: &mut H, target: Target, cx: &mut Context<H>) {
        let o = host.split().0;
        o.cursor = None;
        match target {
            Target::Season(id, name) => {
                o.recents.visit(&id, &name);
                o.recents.save(&o.data);
                o.filter_tracks = TrackFilter::default();
                o.trail.push(Screen::Season(id.clone()), name);
                Self::load_season(host, &id, cx);
            }
            Target::Board(card, label) => {
                let key = card.key.clone();
                o.trail.push(Screen::Board(key.clone()), label);
                o.board_category = None;
                Self::load_board(host, key, 0, cx);
            }
            Target::Profile(driver) => {
                o.trail
                    .push(Screen::Profile(driver.id.clone()), driver.name.clone());
                Self::load_profile(host, &driver.id, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn open_recent<H: Host<Self>>(host: &mut H, id: String, name: String, cx: &mut Context<H>) {
        let o = host.split().0;
        o.cursor = None;
        o.recents.visit(&id, &name);
        o.recents.save(&o.data);
        o.filter_tracks = TrackFilter::default();
        o.trail.jump(1);
        o.trail.push(Screen::Season(id.clone()), name);
        Self::load_season(host, &id, cx);
        cx.notify();
    }

    pub(crate) fn back<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let o = host.split().0;
        o.cursor = None;
        o.trail.back();
        Self::reenter(host, cx);
    }

    pub(crate) fn jump<H: Host<Self>>(host: &mut H, depth: usize, cx: &mut Context<H>) {
        let o = host.split().0;
        o.cursor = None;
        o.trail.jump(depth);
        Self::reenter(host, cx);
    }

    fn reenter<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let o = host.split().0;
        match o.trail.current().clone() {
            Screen::Board(key) if o.board.as_ref().map(|b| &b.0) != Some(&key) => {
                Self::load_board(host, key, 0, cx);
            }
            Screen::Season(id) => Self::load_season(host, &id, cx),
            Screen::Profile(id) => Self::load_profile(host, &id, cx),
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn set_category<H: Host<Self>>(host: &mut H, cat: Category, cx: &mut Context<H>) {
        let o = host.split().0;
        if o.category == cat {
            return;
        }
        o.category = cat;
        cat.save(&o.data);
        o.board_category = None;
        o.board = None;
        o.overview = Load::Idle;
        Self::reload(host, cx);
    }

    pub(crate) fn set_board_category<H: Host<Self>>(host: &mut H, cat: Category, cx: &mut Context<H>) {
        let o = host.split().0;
        o.board_category = (cat != o.category).then_some(cat);
        if let Screen::Board(key) = o.trail.current().clone() {
            Self::load_board(host, key, 0, cx);
        }
    }

    pub(crate) fn board_category(&self) -> Category {
        self.board_category.unwrap_or(self.category)
    }

    /// Ask the running game to play lap `id` as the ghost in Time Trial.
    pub(crate) fn add_to_game<H: Host<Self>>(host: &mut H, id: String, cx: &mut Context<H>) {
        let (o, needs) = host.split();
        let (Some(api), true) = (
            needs.token.map(swofty_core::online::server::api::Api::new),
            needs.game_found,
        ) else {
            return;
        };
        o.watch_notice = Some((true, swofty_i18n::tr("Adding the replay to the game…").to_owned()));
        o.watch_lap = Some(id.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            let lap = id.clone();
            let done = cx
                .background_executor()
                .spawn(async move {
                    swofty_core::online::timetrial::game_replay::add(
                        &api,
                        &swofty_core::data::paths::data_dir(),
                        &lap,
                    )
                })
                .await;
            this.update(cx, |host, cx| {
                let o = host.split().0;
                o.watch_notice = Some(match done {
                    Ok(()) => (
                        true,
                        swofty_i18n::tr("Added. Open Theatre in the game to watch it.").to_owned(),
                    ),
                    Err(e) => {
                        crate::app::app_log::action("add a lap's replay to the game").failed(&e);
                        (
                            false,
                            if e == swofty_core::online::timetrial::game_replay::NO_REPLAY {
                                swofty_i18n::tr("This lap has no game replay").to_owned()
                            } else {
                                e
                            },
                        )
                    }
                });
                o.watch_lap = Some(id);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn watch_notice(&self) -> Option<&(bool, String)> {
        self.watch_notice.as_ref()
    }

    /// The notice of "Add to game" for lap `id`, if that was the last lap it ran for.
    pub(crate) fn replay_notice(&self, id: &str) -> Option<&(bool, String)> {
        self.watch_notice
            .as_ref()
            .filter(|_| self.watch_lap.as_deref() == Some(id))
    }

    pub(crate) fn has_cursor(&self) -> bool {
        self.cursor.is_some()
    }

    pub(crate) fn key<H: Host<Self>>(host: &mut H, key: &str, cx: &mut Context<H>) -> bool {
        let o = host.split().0;
        let n = o.items.len();
        if n == 0 {
            return key == "escape" && o.trail.depth() > 1 && {
                Self::back(host, cx);
                true
            };
        }
        let at = o.cursor;
        match key {
            "down" | "right" => o.cursor = Some(at.map_or(0, |i| (i + 1).min(n - 1))),
            "up" | "left" => o.cursor = Some(at.map_or(0, |i| i.saturating_sub(1))),
            "enter" => {
                if let Some(t) = at.and_then(|i| o.items.get(i)).cloned() {
                    Self::go(host, t, cx);
                }
                return true;
            }
            "escape" | "backspace" => {
                if o.trail.depth() > 1 {
                    Self::back(host, cx);
                    return true;
                }
                return false;
            }
            _ => return false,
        }
        cx.notify();
        true
    }
}

impl Shell {
    pub(crate) fn online_page(
        &mut self,
        p: crate::ui::theme::Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use crate::ui::scale::px;
        use gpui_kit::component::v_flex;
        Online::enter(self, cx);
        self.online
            .seen
            .borrow_mut()
            .shell
            .get_or_insert_with(|| cx.entity().downgrade());
        if self.online.search.is_none() {
            self.online.search =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder(swofty_i18n::tr("Search tracks"))));
        }
        let crumbs = self.crumb_bar(p, cx);
        let banner = self.sign_in_banner(p, cx);
        let body = match self.online.trail.current().clone() {
            Screen::Landing => self.online_landing(p, cx),
            Screen::Season(id) => self.online_season(&id, p, cx),
            Screen::Board(_) => self.online_board(p, cx),
            Screen::Profile(id) => self.online_profile(&id, p, cx),
        };
        let _ = px;
        v_flex()
            .size_full()
            .child(crumbs)
            .children(banner)
            .child(v_flex().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::Online;
    use super::model::{AssistFilter, Category};

    fn online() -> Online {
        Online::new(&std::env::temp_dir().join(format!("swofty-launcher-online-{}", std::process::id())))
    }

    #[test]
    fn the_replay_notice_shows_only_on_the_lap_last_sent_to_the_game() {
        let mut o = online();
        let added = (true, "Added".to_owned());
        o.watch_notice = Some(added.clone());
        o.watch_lap = Some("lap-1".into());
        assert_eq!(o.replay_notice("lap-1"), Some(&added));
        assert_eq!(o.replay_notice("lap-2"), None);
        assert_eq!(o.watch_notice(), Some(&added));
    }

    #[test]
    fn a_board_follows_the_page_category_until_it_picks_its_own() {
        let mut o = online();
        assert_eq!(o.board_category(), o.category);
        let own = Category {
            assists: AssistFilter::None,
            ..o.category
        };
        o.board_category = Some(own);
        assert_eq!(o.board_category(), own);
    }
}
