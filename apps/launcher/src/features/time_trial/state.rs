use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::convert::Infallible;
use std::path::Path;
use swofty_core::online::server::api::{Api, ApiError};
use swofty_core::online::server::integrity;
use swofty_core::online::server::sign_in::Session;
use swofty_core::online::timetrial::barred::{self, Barred};
use swofty_core::online::timetrial::boards::{Group, Row, Run, RunBoard};
use swofty_i18n::{tr, trf};

use crate::services::library::Library;
use crate::shell::Shell;
use crate::shell::host::{Host, Hosted, reports_nothing};

pub(crate) enum Load<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Pick {
    pub content: String,
    pub track: String,
    pub weather: String,
    pub title: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Boards,
    Runs,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Accept,
    Deny,
    Ban,
}

pub(crate) struct TtPage {
    pub view: View,
    pub runs: Load<Vec<Run>>,
    pub run_boards: Vec<RunBoard>,
    pub run_seasons: Vec<String>,
    pub run_tracks: Vec<String>,
    pub run_status: &'static str,
    pub run_query: String,
    pub run_search: Option<Entity<InputState>>,
    run_sub: Option<Subscription>,
    run_generation: u64,
    pub open_lap: Option<String>,
    pub banning: bool,
    pub acting: bool,
    pub notice: Option<String>,
    pub reason: Option<Entity<InputState>>,
    pub catalog: Load<Vec<Group>>,
    pub pick: Option<Pick>,
    pub rows: Load<Vec<Row>>,
    pub query: String,
    pub search: Option<Entity<InputState>>,
    sub: Option<Subscription>,
    generation: u64,
    pub barred: Vec<Barred>,
    pub integrity_blocked: Option<String>,
    pub integrity_read: Option<std::time::Instant>,
}

impl Default for TtPage {
    fn default() -> Self {
        Self {
            view: View::Runs,
            runs: Load::Idle,
            run_boards: Vec::new(),
            run_seasons: Vec::new(),
            run_tracks: Vec::new(),
            run_status: "all",
            run_query: String::new(),
            run_search: None,
            run_sub: None,
            run_generation: 0,
            open_lap: None,
            banning: false,
            acting: false,
            notice: None,
            reason: None,
            catalog: Load::Idle,
            pick: None,
            rows: Load::Idle,
            query: String::new(),
            search: None,
            sub: None,
            generation: 0,
            barred: Vec::new(),
            integrity_blocked: None,
            integrity_read: None,
        }
    }
}

impl TtPage {
    /// The loaded runs that pass the status choice and the search text, newest first as loaded.
    pub(super) fn shown_runs(&self) -> Vec<&Run> {
        let Load::Ready(runs) = &self.runs else {
            return Vec::new();
        };
        runs.iter()
            .filter(|r| self.run_status == "all" || r.status == self.run_status)
            .filter(|r| r.matches(&self.run_query))
            .collect()
    }

    /// The seasons (`season`) or the tracks of the picked seasons that have runs, as key and label,
    /// sorted by label. A name two entries share gets its key after it.
    pub(super) fn choices(&self, season: bool) -> Vec<(String, String)> {
        let boards = self.run_boards.iter();
        if season {
            labelled(boards.map(|b| (b.content.as_str(), b.season.as_deref())))
        } else {
            labelled(
                boards
                    .filter(|b| self.run_seasons.is_empty() || self.run_seasons.contains(&b.content))
                    .map(|b| (b.track.as_str(), b.track_name.as_deref())),
            )
        }
    }

    pub(super) fn picked(&self, season: bool) -> &[String] {
        if season {
            &self.run_seasons
        } else {
            &self.run_tracks
        }
    }

    /// What the season or track filter button reads for the picks.
    pub(super) fn filter_label(&self, season: bool, choices: &[(String, String)]) -> String {
        let name = |key: &String| {
            choices
                .iter()
                .find(|(k, _)| k == key)
                .map_or(key.as_str(), |(_, label)| label.as_str())
                .to_owned()
        };
        match (season, self.picked(season)) {
            (true, []) => tr("All seasons").to_owned(),
            (false, []) => tr("All tracks").to_owned(),
            (true, [one]) => trf!("Season: {name}", name = name(one)),
            (false, [one]) => trf!("Track: {name}", name = name(one)),
            (true, many) => trf!("Season: {count} picked", count = many.len()),
            (false, many) => trf!("Track: {count} picked", count = many.len()),
        }
    }

    fn keep_listed_tracks(&mut self) {
        if self.run_boards.is_empty() {
            return;
        }
        let listed = self.choices(false);
        self.run_tracks.retain(|t| listed.iter().any(|(k, _)| k == t));
    }

    pub(super) fn waiting(&self) -> usize {
        match &self.runs {
            Load::Ready(runs) => runs.iter().filter(|r| r.pending()).count(),
            _ => 0,
        }
    }

    pub(super) fn visible(&self) -> Vec<&Row> {
        let Load::Ready(rows) = &self.rows else {
            return Vec::new();
        };
        swofty_core::online::timetrial::boards::by_driver(rows, &self.query)
    }
}

fn labelled<'a>(items: impl Iterator<Item = (&'a str, Option<&'a str>)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (key, name) in items {
        if out.iter().any(|(k, _)| k == key) {
            continue;
        }
        let name = name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(key);
        out.push((key.to_owned(), name.to_owned()));
    }
    let names: Vec<String> = out.iter().map(|(_, label)| label.to_lowercase()).collect();
    for (i, (key, label)) in out.iter_mut().enumerate() {
        if label != key && names.iter().filter(|n| **n == names[i]).count() > 1 {
            *label = format!("{label} ({key})");
        }
    }
    out.sort_by_cached_key(|(key, label)| (label.to_lowercase(), key.clone()));
    out
}

/// What the time trial page borrows from the shell: the data folder and who is signed in.
pub(crate) struct TtNeeds<'a> {
    pub(crate) data: &'a Path,
    pub(crate) session: Option<&'a Session>,
}

impl TtNeeds<'_> {
    fn api(&self) -> Option<Api> {
        self.session.map(|s| Api::new(&s.token))
    }

    pub(crate) fn admin(&self) -> bool {
        self.session.is_some_and(|s| s.admin)
    }
}

impl Hosted for TtPage {
    type Needs<'a> = TtNeeds<'a>;
    type Change = Infallible;
}

impl TtPage {
    pub(crate) fn open<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        Self::load_barred(host, cx);
        Self::load_integrity(host, cx);
        if host.split().1.admin() {
            Self::load_runs(host, cx);
        } else {
            host.split().0.view = View::Boards;
        }
        if matches!(host.split().0.catalog, Load::Idle | Load::Failed(_)) {
            Self::load_catalog(host, cx);
        }
    }

    /// Whether the running session blocks Time Trial in game: the note the wrapper keeps when the session placed
    /// no integrity module.
    pub(super) fn load_integrity<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        page.integrity_read = Some(std::time::Instant::now());
        let data = needs.data.to_path_buf();
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(async move { integrity::Notice::read(&data).map(|n| n.why) })
                .await;
            this.update(cx, |host, cx| {
                let page = host.split().0;
                if page.integrity_blocked != got {
                    page.integrity_blocked = got;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The barred list, from the cache while it is fresh, else from the server.
    pub(super) fn load_barred<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let data = host.split().1.data.to_path_buf();
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(
                    async move { barred::current(&data, swofty_core::online::timetrial::queue::unix_now()) },
                )
                .await;
            this.update(cx, |host, cx| {
                if let Some(list) = got {
                    host.split().0.barred = list;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn load_catalog<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        let Some(api) = needs.api() else {
            return;
        };
        page.catalog = Load::Loading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx.background_executor().spawn(async move { api.catalog() }).await;
            this.update(cx, |host, cx| {
                host.split().0.catalog = load(got);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn load_runs<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        let Some(api) = needs.api().filter(|_| needs.admin()) else {
            return;
        };
        let (generation, seasons, tracks) = page.begin_runs();
        cx.spawn(async move |this, cx| {
            let (runs, boards) = cx
                .background_executor()
                .spawn(async move { (api.runs(&seasons, &tracks), api.run_boards()) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.take_runs(generation, runs, boards) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Turn one season or track filter on or off and load the runs again.
    pub(super) fn toggle_filter<H: Host<Self>>(host: &mut H, season: bool, value: &str, cx: &mut Context<H>) {
        host.split().0.toggle(season, value);
        Self::load_runs(host, cx);
        cx.notify();
    }

    pub(super) fn clear_filter<H: Host<Self>>(host: &mut H, season: bool, cx: &mut Context<H>) {
        host.split().0.picks_mut(season).clear();
        Self::load_runs(host, cx);
        cx.notify();
    }

    pub(super) fn run_search<H: Host<Self>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> Entity<InputState> {
        if let Some(state) = &host.split().0.run_search {
            return state.clone();
        }
        let state = cx.new(|cx| {
            InputState::new(window, cx).placeholder(swofty_i18n::tr("Search driver, season, track or lap id"))
        });
        let sub = cx.subscribe(&state, |host, state, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                host.split().0.run_query = state.read(cx).value().to_string();
                cx.notify();
            }
        });
        let page = host.split().0;
        page.run_sub = Some(sub);
        page.run_search = Some(state.clone());
        state
    }

    pub(super) fn select_lap<H: Host<Self>>(host: &mut H, lap: &str, cx: &mut Context<H>) {
        let page = host.split().0;
        page.open_lap = Some(lap.to_owned());
        page.banning = false;
        page.notice = None;
        cx.notify();
    }

    pub(super) fn moderate<H: Host<Self>>(
        host: &mut H,
        lap: String,
        verdict: Verdict,
        reason: String,
        cx: &mut Context<H>,
    ) {
        let (page, needs) = host.split();
        let Some(api) = needs.api().filter(|_| needs.admin() && !page.acting) else {
            return;
        };
        page.acting = true;
        page.notice = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let done = cx
                .background_executor()
                .spawn(async move {
                    match verdict {
                        Verdict::Accept => api.accept_lap(&lap),
                        Verdict::Deny => api.deny_lap(&lap),
                        Verdict::Ban => api.ban_driver(&lap, &reason),
                    }
                })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.take_moderated(done) {
                    Self::load_runs(host, cx);
                    Self::reload(host, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn reason<H: Host<Self>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> Entity<InputState> {
        if let Some(state) = &host.split().0.reason {
            return state.clone();
        }
        let state =
            cx.new(|cx| InputState::new(window, cx).placeholder(swofty_i18n::tr("Reason for the ban")));
        cx.subscribe(&state, |_, _, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        host.split().0.reason = Some(state.clone());
        state
    }

    pub(super) fn pick<H: Host<Self>>(host: &mut H, pick: Pick, cx: &mut Context<H>) {
        host.split().0.pick = Some(pick);
        Self::reload(host, cx);
    }

    pub(super) fn reload<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (page, needs) = host.split();
        let (Some(api), Some(pick)) = (needs.api(), page.pick.clone()) else {
            return;
        };
        let generation = page.begin_rows();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(async move { api.board(&pick.content, &pick.track, &pick.weather, None) })
                .await;
            this.update(cx, |host, cx| {
                if host.split().0.take_rows(generation, got) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn search<H: Host<Self>>(
        host: &mut H,
        window: &mut Window,
        cx: &mut Context<H>,
    ) -> Entity<InputState> {
        if let Some(state) = &host.split().0.search {
            return state.clone();
        }
        let state = cx.new(|cx| InputState::new(window, cx).placeholder(swofty_i18n::tr("Search drivers")));
        let sub = cx.subscribe(&state, |host, state, ev: &InputEvent, cx| {
            if matches!(ev, InputEvent::Change) {
                host.split().0.query = state.read(cx).value().to_string();
                cx.notify();
            }
        });
        let page = host.split().0;
        page.sub = Some(sub);
        page.search = Some(state.clone());
        state
    }

    fn picks_mut(&mut self, season: bool) -> &mut Vec<String> {
        if season {
            &mut self.run_seasons
        } else {
            &mut self.run_tracks
        }
    }

    /// Turns one season or track pick on or off. Picked tracks that no picked season has are dropped.
    fn toggle(&mut self, season: bool, value: &str) {
        let list = self.picks_mut(season);
        match list.iter().position(|v| v == value) {
            Some(i) => {
                list.remove(i);
            }
            None => list.push(value.to_owned()),
        }
        if season {
            self.keep_listed_tracks();
        }
    }

    /// Starts loading the runs for the picks: the generation the answer has to match, and the picks.
    fn begin_runs(&mut self) -> (u64, Vec<String>, Vec<String>) {
        self.run_generation += 1;
        if !matches!(self.runs, Load::Ready(_)) {
            self.runs = Load::Loading;
        }
        (
            self.run_generation,
            self.run_seasons.clone(),
            self.run_tracks.clone(),
        )
    }

    /// Takes loaded runs unless a newer load started since. The first run opens when none is.
    fn take_runs(
        &mut self,
        generation: u64,
        runs: Result<Vec<Run>, ApiError>,
        boards: Result<Vec<RunBoard>, ApiError>,
    ) -> bool {
        if self.run_generation != generation {
            return false;
        }
        self.runs = match runs {
            Ok(laps) => {
                if self.open_lap.is_none() {
                    self.open_lap = laps.first().map(|r| r.lap_id.clone());
                }
                Load::Ready(laps)
            }
            Err(e) => Load::Failed(e.message),
        };
        if let Ok(boards) = boards {
            self.run_boards = boards;
        }
        true
    }

    /// Takes a verdict's result. True when the runs and the board have to load again.
    fn take_moderated(&mut self, done: Result<(), ApiError>) -> bool {
        self.acting = false;
        match done {
            Ok(()) => {
                self.banning = false;
                true
            }
            Err(e) => {
                self.notice = Some(e.message);
                false
            }
        }
    }

    fn begin_rows(&mut self) -> u64 {
        self.generation += 1;
        self.rows = Load::Loading;
        self.generation
    }

    fn take_rows(&mut self, generation: u64, got: Result<Vec<Row>, ApiError>) -> bool {
        if self.generation != generation {
            return false;
        }
        self.rows = load(got);
        true
    }
}

fn load<T>(got: Result<T, ApiError>) -> Load<T> {
    match got {
        Ok(v) => Load::Ready(v),
        Err(e) => Load::Failed(e.message),
    }
}

impl TtPage {
    /// The barred addons `library`'s active profile has enabled, at the versions installed.
    pub(super) fn barred_in(&self, library: &Library) -> Vec<&Barred> {
        let registry = &library.registry;
        barred::in_profile(&self.barred, &library.profile, |id| {
            registry.get(id).map(|e| e.addon.manifest().version.to_string())
        })
    }
}

impl Shell {
    pub(crate) fn tt_admin(&self) -> bool {
        self.account.session.as_ref().is_some_and(|s| s.admin)
    }
}

impl Host<TtPage> for Shell {
    fn split(&mut self) -> (&mut TtPage, TtNeeds<'_>) {
        (
            &mut self.tt,
            TtNeeds {
                data: &self.store.root,
                session: self.account.session.as_ref(),
            },
        )
    }

    reports_nothing!();
}

#[cfg(test)]
mod tests {
    use swofty_core::online::server::api::ApiError;
    use swofty_core::online::timetrial::barred::Barred;
    use swofty_core::online::timetrial::boards::Run;

    use super::{Load, RunBoard, TtPage};
    use crate::services::test_library::TestLibrary;

    fn error(message: &str) -> ApiError {
        ApiError {
            status: 500,
            message: message.to_owned(),
            retry_after: None,
        }
    }

    fn run(lap: &str) -> Run {
        serde_json::from_value(serde_json::json!({
            "lap_id": lap,
            "driver": { "name": "Driver" },
            "time_ms": 90_000,
            "status": "pending",
            "date": "2026-11-01",
        }))
        .unwrap()
    }

    fn board(content: &str, season: &str, track: &str, name: &str) -> RunBoard {
        RunBoard {
            content: content.to_owned(),
            season: Some(season.to_owned()).filter(|s| !s.is_empty()),
            track: track.to_owned(),
            track_name: Some(name.to_owned()).filter(|n| !n.is_empty()),
            laps: 1,
        }
    }

    fn page() -> TtPage {
        TtPage {
            run_boards: vec![
                board(
                    "swofty.swofty-seasons-2016/f1_2016",
                    "SWOFTY 2016",
                    "silverstone",
                    "Silverstone",
                ),
                board("game/f1_25", "F1 25", "silverstone", "Silverstone"),
                board("game/f1_25", "F1 25", "austria_reverse", ""),
                board("mod.fuji/season", "Fuji", "mod.fuji", "Fuji Speedway"),
                board("other.fuji/season", "", "other.fuji", "Fuji Speedway"),
                board("game/f1_25", "F1 25", "bahrain", "Bahrain"),
            ],
            ..TtPage::default()
        }
    }

    #[test]
    fn seasons_are_sorted_by_name_once_each() {
        let labels: Vec<String> = page().choices(true).into_iter().map(|(_, l)| l).collect();
        assert_eq!(labels, ["F1 25", "Fuji", "other.fuji/season", "SWOFTY 2016"]);
    }

    #[test]
    fn tracks_sharing_a_name_show_their_code() {
        let tracks = page().choices(false);
        let labels: Vec<&str> = tracks.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(
            labels,
            [
                "austria_reverse",
                "Bahrain",
                "Fuji Speedway (mod.fuji)",
                "Fuji Speedway (other.fuji)",
                "Silverstone"
            ]
        );
    }

    #[test]
    fn tracks_follow_the_picked_seasons() {
        let mut tt = page();
        tt.run_seasons = vec!["swofty.swofty-seasons-2016/f1_2016".to_owned()];
        let keys: Vec<String> = tt.choices(false).into_iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["silverstone"]);
    }

    #[test]
    fn a_season_pick_drops_tracks_it_does_not_have() {
        let mut tt = page();
        tt.run_tracks = vec!["bahrain".to_owned(), "silverstone".to_owned()];
        tt.run_seasons = vec!["swofty.swofty-seasons-2016/f1_2016".to_owned()];
        tt.keep_listed_tracks();
        assert_eq!(tt.run_tracks, ["silverstone"]);
    }

    #[test]
    fn the_button_names_the_picks() {
        let mut tt = page();
        assert_eq!(tt.filter_label(true, &tt.choices(true)), "All seasons");
        assert_eq!(tt.filter_label(false, &tt.choices(false)), "All tracks");
        tt.run_seasons = vec!["swofty.swofty-seasons-2016/f1_2016".to_owned()];
        assert_eq!(tt.filter_label(true, &tt.choices(true)), "Season: SWOFTY 2016");
        tt.run_seasons.clear();
        tt.run_tracks = vec!["mod.fuji".to_owned()];
        assert_eq!(
            tt.filter_label(false, &tt.choices(false)),
            "Track: Fuji Speedway (mod.fuji)"
        );
        tt.run_tracks
            .extend(["bahrain".to_owned(), "silverstone".to_owned()]);
        assert_eq!(tt.filter_label(false, &tt.choices(false)), "Track: 3 picked");
    }

    #[test]
    fn a_season_toggle_drops_the_tracks_it_does_not_have_and_toggles_back() {
        let mut tt = page();
        tt.toggle(false, "bahrain");
        tt.toggle(false, "silverstone");
        tt.toggle(true, "swofty.swofty-seasons-2016/f1_2016");
        assert_eq!(tt.run_tracks, ["silverstone"]);
        tt.toggle(true, "swofty.swofty-seasons-2016/f1_2016");
        assert!(tt.run_seasons.is_empty());
    }

    #[test]
    fn runs_from_an_older_load_are_dropped_and_the_first_run_opens() {
        let mut tt = TtPage::default();
        let (old, ..) = tt.begin_runs();
        let (new, ..) = tt.begin_runs();
        assert!(matches!(tt.runs, Load::Loading));
        assert!(!tt.take_runs(old, Ok(vec![run("old")]), Ok(Vec::new())));
        assert!(matches!(tt.runs, Load::Loading));
        assert!(tt.take_runs(new, Ok(vec![run("a"), run("b")]), Err(error("boards down"))));
        assert_eq!(tt.open_lap.as_deref(), Some("a"));
        assert!(matches!(&tt.runs, Load::Ready(r) if r.len() == 2));
        tt.begin_runs();
        assert!(
            matches!(tt.runs, Load::Ready(_)),
            "loaded runs stay up while they reload"
        );
    }

    #[test]
    fn a_verdict_reloads_when_it_lands_and_says_why_when_it_does_not() {
        let mut tt = TtPage {
            acting: true,
            banning: true,
            ..TtPage::default()
        };
        assert!(!tt.take_moderated(Err(error("not allowed"))));
        assert_eq!(tt.notice.as_deref(), Some("not allowed"));
        assert!(!tt.acting && tt.banning);
        assert!(tt.take_moderated(Ok(())));
        assert!(!tt.banning);
    }

    #[test]
    fn a_board_from_an_older_pick_is_dropped() {
        let mut tt = TtPage::default();
        let old = tt.begin_rows();
        let new = tt.begin_rows();
        assert!(!tt.take_rows(old, Ok(Vec::new())));
        assert!(matches!(tt.rows, Load::Loading));
        assert!(tt.take_rows(new, Err(error("gone"))));
        assert!(matches!(&tt.rows, Load::Failed(m) if m == "gone"));
    }

    fn barred(id: &str, below: Option<&str>) -> Barred {
        Barred {
            id: id.to_owned(),
            name: String::new(),
            reason: String::new(),
            below: below.map(str::to_owned),
        }
    }

    #[test]
    fn the_barred_notice_names_addons_on_in_the_profile_below_their_fixed_version() {
        let library = TestLibrary::new("tt-barred")
            .addon("t.old", "1.0.0")
            .addon("t.fixed", "2.0.0")
            .addon("t.off", "1.0.0")
            .profile("Main", &["t.old", "t.fixed"]);
        let mut tt = page();
        tt.barred = vec![
            barred("t.old", Some("2.0.0")),
            barred("t.fixed", Some("2.0.0")),
            barred("t.off", None),
        ];
        let ids: Vec<&str> = tt
            .barred_in(&library.read())
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(ids, ["t.old"]);
    }
}
