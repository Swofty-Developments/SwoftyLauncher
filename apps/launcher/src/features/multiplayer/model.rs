use std::collections::HashSet;

use swofty_core::online::multiplayer::client::Local;
use swofty_core::online::multiplayer::matching::{self, Blocker, Need, Own, Plan, Unlisted};
use swofty_core::online::multiplayer::types::FixStep;
use swofty_core::online::multiplayer::types::{
    GameStatus, MemberStage, MemberState, Phase, Problem, Progress, RoomAddon, RoomContent, RoomSettings,
    RoomSummary, Screen, SessionState, SessionStep, Step, StepState,
};
use swofty_i18n::{fill, tr, trf, trn};

/// Where the game is, in the page header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameState {
    NotRunning,
    MainMenu,
    LanLobby,
    Busy,
}

impl GameState {
    pub(crate) fn of(running: bool, status: Option<&GameStatus>) -> Self {
        if !running {
            return Self::NotRunning;
        }
        match status.map(|s| s.screen) {
            Some(Screen::MainMenu) => Self::MainMenu,
            Some(Screen::LanLobby) => Self::LanLobby,
            Some(Screen::Other) | None => Self::Busy,
        }
    }

    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::NotRunning => "F1 25 isn't running",
            Self::MainMenu => "F1 25 is on the main menu",
            Self::LanLobby => "F1 25 is in a LAN lobby",
            Self::Busy => "F1 25 is busy",
        }
    }
}

/// The room row's second line: host and players.
pub(crate) fn room_meta(room: &RoomSummary) -> String {
    format!("{} \u{00b7} {}", room.host.name, room_meta_players(room))
}

pub(crate) fn room_meta_players(room: &RoomSummary) -> String {
    format!("{}/{} players", room.members, room.max_members)
}

/// This player's side as a room's status reads it: the own profile's digest and gameplay addons when that
/// profile reads, and what the player has to match a host with, which needs no profile.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OwnPrint {
    pub(crate) digest: Option<String>,
    pub(crate) addons: Vec<RoomAddon>,
    pub(crate) matching: Option<Own>,
}

impl OwnPrint {
    pub(crate) fn of(local: Option<&Local>, matching: Option<Own>) -> Self {
        Self {
            digest: local.map(Local::digest),
            addons: local
                .iter()
                .flat_map(|l| &l.fingerprint.addons)
                .map(|a| RoomAddon {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    ..RoomAddon::default()
                })
                .collect(),
            matching,
        }
    }
}

/// What joining a room takes, from a listing that names the host's versions.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Fit {
    /// The listing or this player's side doesn't say enough.
    Unknown,
    /// Joining installs `installs` addons from the Addon Browser and plays the host's addons, in the host's
    /// order, with the host's settings.
    Joins { installs: usize, replaces: Vec<String> },
    /// What stops the join, as the row and the View say it.
    Blocked(String),
}

pub(crate) fn room_fit(room: &RoomSummary, mine: Option<&OwnPrint>) -> Fit {
    let (Some(listing), Some(own)) = (room.addons.as_ref(), mine.and_then(|m| m.matching.as_ref())) else {
        return Fit::Unknown;
    };
    let Some(addons) = matching::listed_addons(listing) else {
        return Fit::Unknown;
    };
    let plan = matching::plan(
        &addons,
        listing.launcher.as_deref(),
        listing.game.as_deref(),
        listing.dlc,
        own,
    );
    fit_of(&plan)
}

pub(crate) fn fit_of(plan: &Plan) -> Fit {
    if let Some(first) = plan.blockers.first() {
        return Fit::Blocked(blocker_line(first));
    }
    Fit::Joins {
        installs: plan.installs.len(),
        replaces: replace_lines(plan),
    }
}

/// What stops a join, one sentence.
pub(crate) fn blocker_line(b: &Blocker) -> String {
    match b {
        Blocker::Launcher { host } => trf!(
            "The host plays on Swofty Launcher {version}; update to the same version to join",
            version = host
        ),
        Blocker::Game => tr("Your F1 25 is another version than the host's").to_owned(),
        Blocker::Dlc => tr("Your F1 25 content differs from the host's").to_owned(),
        Blocker::Builtin { name, version } => trf!(
            "The host's launcher has {name} {version} built in; update to the same version to join",
            name = name,
            version = version
        ),
        Blocker::Addon {
            name,
            version,
            why: Unlisted::NotListed,
        } => trf!(
            "{name} {version} isn't on the Addon Browser",
            name = name,
            version = version
        ),
        Blocker::Addon {
            name,
            version,
            why: Unlisted::OtherCopy,
        } => trf!(
            "The Addon Browser's {name} {version} isn't the host's copy",
            name = name,
            version = version
        ),
        Blocker::Addon {
            name,
            version,
            why: Unlisted::NeedsLauncher,
        } => trf!(
            "{name} {version} needs another Swofty Launcher",
            name = name,
            version = version
        ),
    }
}

/// One line per install that replaces a copy the player has.
pub(crate) fn replace_lines(plan: &Plan) -> Vec<String> {
    plan.installs
        .iter()
        .filter_map(|i| {
            let mine = i.replaces.as_ref()?;
            Some(trf!(
                "Installs {name} {version}, replacing your {mine}",
                name = i.name(),
                version = i.version.version,
                mine = mine
            ))
        })
        .collect()
}

/// What a room's one status line says, and whether it needs the player's attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoomStatus {
    Full,
    Differ,
    Match,
    /// Joining installs or turns on what differs.
    Fixable,
    /// Something the launcher can't change stops the join.
    Blocked,
    Unknown,
}

impl RoomStatus {
    /// `on` is the addons this player has on. A listing that names the host's versions decides by
    /// [`room_fit`]; without one, a room differs when one of the host's addons is off here or the host lacks
    /// one of this player's gameplay addons, as the room's View shows. A room whose host lists no addons
    /// differs when the digests do.
    pub(crate) fn of(room: &RoomSummary, on: &HashSet<String>, mine: Option<&OwnPrint>) -> Self {
        if room.members >= room.max_members {
            return Self::Full;
        }
        let same = room
            .digest
            .as_deref()
            .zip(mine.and_then(|m| m.digest.as_deref()))
            .is_some_and(|(h, m)| h.eq_ignore_ascii_case(m));
        if same {
            return Self::Match;
        }
        match room_fit(room, mine) {
            Fit::Blocked(_) => return Self::Blocked,
            Fit::Joins { .. } => return Self::Fixable,
            Fit::Unknown => {}
        }
        let (Some(_), Some(mine)) = (room.digest.as_deref(), mine.filter(|m| m.digest.is_some())) else {
            return Self::Unknown;
        };
        let Some(listed) = room.addons.as_ref().filter(|a| !a.list.is_empty()) else {
            return Self::Differ;
        };
        if missing(&addon_checks(&listed.list, on)) > 0 || !extra_addons(room, mine).is_empty() {
            Self::Differ
        } else {
            Self::Match
        }
    }

    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::Full => "Full",
            Self::Differ => "Your addons differ",
            Self::Match => "Your addons match",
            Self::Fixable => "Joining matches the host's addons",
            Self::Blocked => "Can't join",
            Self::Unknown => "",
        }
    }

    /// Whether a join can start: not for a full room or one something blocks.
    pub(crate) fn joinable(self) -> bool {
        !matches!(self, Self::Full | Self::Blocked)
    }
}

/// The room's status line: what joining installs, or what stops it, when the listing says.
pub(crate) fn room_line(room: &RoomSummary, status: RoomStatus, mine: Option<&OwnPrint>) -> String {
    match (status, room_fit(room, mine)) {
        (RoomStatus::Blocked, Fit::Blocked(why)) => trf!("Can't join: {why}", why = why),
        (RoomStatus::Fixable, Fit::Joins { installs, .. }) if installs > 0 => trn(
            installs,
            "Joining installs {count} addon from the Addon Browser",
            "Joining installs {count} addons from the Addon Browser",
        ),
        (status, _) => tr(status.text()).to_owned(),
    }
}

/// How a row of a room's View marks an addon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Have,
    /// Joining installs it from the Addon Browser.
    Installs,
    Lacks,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewRow {
    pub(crate) name: String,
    pub(crate) mark: Mark,
    pub(crate) note: String,
}

/// A room's View: one sentence on the addons, and a row per addon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ViewAddons {
    pub(crate) summary: String,
    pub(crate) rows: Vec<ViewRow>,
}

/// The View's addons. A listing that names the host's versions shows what joining does about each of the
/// host's addons, as [`room_fit`] judges them; without one, each addon is checked against the ones `on` in
/// the player's profile, with the player's addons the host lacks after them.
pub(crate) fn view_addons(room: &RoomSummary, on: &HashSet<String>, mine: Option<&OwnPrint>) -> ViewAddons {
    if let Some(view) = fit_view(room, mine) {
        return view;
    }
    let listed = room.addons.as_ref().filter(|a| !a.list.is_empty());
    let checks = listed.map(|a| addon_checks(&a.list, on)).unwrap_or_default();
    let lacking = missing(&checks);
    let extra = mine.map(|m| extra_addons(room, m)).unwrap_or_default();
    let summary = if checks.is_empty() {
        tr("The host's launcher hasn't listed its addons.").to_owned()
    } else if lacking == 0 && extra.is_empty() {
        tr("You have every addon the host has on.").to_owned()
    } else if lacking == 0 {
        trn(
            extra.len(),
            "You have {count} addon on that the host doesn't have.",
            "You have {count} addons on that the host doesn't have.",
        )
    } else {
        fill(
            &trn(
                checks.len(),
                "You are missing or have off {lacking} of {count} addon.",
                "You are missing or have off {lacking} of {count} addons.",
            ),
            &[("lacking", lacking.to_string())],
        )
    };
    let rows = checks
        .into_iter()
        .map(|c| ViewRow {
            note: if c.have {
                tr("You have it")
            } else {
                tr("You don't have it on")
            }
            .to_owned(),
            mark: if c.have { Mark::Have } else { Mark::Lacks },
            name: c.name,
        })
        .chain(extra.into_iter().map(|name| ViewRow {
            name,
            mark: Mark::Lacks,
            note: tr("The host doesn't have it").to_owned(),
        }))
        .collect();
    ViewAddons { summary, rows }
}

fn fit_view(room: &RoomSummary, mine: Option<&OwnPrint>) -> Option<ViewAddons> {
    let listing = room.addons.as_ref()?;
    let own = mine?.matching.as_ref()?;
    let addons = matching::listed_addons(listing)?;
    let needs = matching::needs(&addons, listing.launcher.as_deref(), own);
    let rows: Vec<ViewRow> = addons
        .iter()
        .zip(&needs)
        .map(|(h, need)| {
            let (mark, note) = match need {
                Need::Have => (Mark::Have, tr("You have it").to_owned()),
                Need::Install(i) => (
                    Mark::Installs,
                    match &i.replaces {
                        Some(mine) => trf!(
                            "Joining installs {version}, replacing your {mine}",
                            version = i.version.version,
                            mine = mine
                        ),
                        None => trf!(
                            "Joining installs {version} from the Addon Browser",
                            version = i.version.version
                        ),
                    },
                ),
                Need::Blocked(b) => (Mark::Lacks, addon_blocker(b)),
            };
            ViewRow {
                name: h.name.clone(),
                mark,
                note,
            }
        })
        .collect();
    let installs = rows.iter().filter(|r| r.mark == Mark::Installs).count();
    let summary = match room_fit(room, mine) {
        Fit::Blocked(why) => trf!("Can't join: {why}", why = why),
        _ if installs > 0 => trn(
            installs,
            "Joining installs {count} addon from the Addon Browser.",
            "Joining installs {count} addons from the Addon Browser.",
        ),
        _ => tr("You have every addon the host has.").to_owned(),
    };
    Some(ViewAddons { summary, rows })
}

/// What stops one addon, as its row in the View says it.
fn addon_blocker(b: &Blocker) -> String {
    match b {
        Blocker::Addon {
            version,
            why: Unlisted::NotListed,
            ..
        } => trf!("{version} isn't on the Addon Browser", version = version),
        Blocker::Addon {
            why: Unlisted::OtherCopy,
            ..
        } => tr("The Addon Browser's copy isn't the host's").to_owned(),
        Blocker::Addon {
            why: Unlisted::NeedsLauncher,
            ..
        } => tr("Needs another Swofty Launcher").to_owned(),
        other => blocker_line(other),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Working,
    Good,
    Blocked,
    Over,
}

pub(crate) const STEPS: [Step; 6] = [
    Step::SigningIn,
    Step::Joining,
    Step::Connecting,
    Step::StartingGame,
    Step::Loading,
    Step::InLobby,
];

pub(crate) fn step_number(step: Step) -> usize {
    STEPS.iter().position(|s| *s == step).unwrap_or(0)
}

/// A step's name on the status card of a session that reports no steps.
pub(crate) fn step_name(step: Step) -> &'static str {
    match step {
        Step::SigningIn => tr("Signing in"),
        Step::Joining => tr("Joining"),
        Step::Connecting => tr("Connecting"),
        Step::StartingGame => tr("Starting F1 25"),
        Step::Loading => tr("Loading"),
        Step::InLobby => tr("In the lobby"),
    }
}

/// What the status area shows: the step, how far along, and the problem when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Status {
    pub(crate) tone: Tone,
    pub(crate) progress: Progress,
    pub(crate) problem: Option<Problem>,
    pub(crate) hint: Option<&'static str>,
}

pub(crate) fn room_status(state: &SessionState) -> Status {
    let progress = state.step();
    if let Phase::Ended(why) = &state.phase {
        return Status {
            tone: Tone::Over,
            progress: Progress {
                label: crate::ui::format::capitalised(why),
                ..progress
            },
            problem: None,
            hint: None,
        };
    }
    let problem = state.problem.clone();
    let (tone, hint) = match (&problem, progress.step) {
        (Some(_), _) => (Tone::Blocked, None),
        (None, Step::InLobby) => (Tone::Good, Some(lobby_hint(state.host, &state.phase))),
        (None, _) => (Tone::Working, None),
    };
    Status {
        tone,
        progress,
        problem,
        hint,
    }
}

/// What the player does next once everyone is in F1 25's lobby.
pub(crate) fn lobby_hint(host: bool, phase: &Phase) -> &'static str {
    match (host, phase) {
        (true, Phase::Racing) => tr("Mark the race over when everyone is back in the lobby."),
        (true, _) => tr("Pick tracks and settings in F1 25's lobby, then start the race there."),
        (false, _) => tr("The host picks the tracks and starts the race."),
    }
}

/// A step's title, by the id the module names it with. A step this launcher doesn't know shows its id.
pub(crate) fn step_title(id: &str, host: bool) -> String {
    let title = match (id, host) {
        ("account", _) => tr("Log in"),
        ("room", true) => tr("Open the room"),
        ("room", false) => tr("Join the room"),
        ("addons", true) => tr("Check your addons"),
        ("addons", false) => tr("Match the host's addons"),
        ("build", _) => tr("Build the game"),
        ("game", _) => tr("Start F1 25"),
        ("network", _) => tr("Connect to the players"),
        ("lobby", true) => tr("Open the LAN lobby"),
        ("lobby", false) => tr("Join the host's LAN lobby"),
        ("race", _) => tr("Race"),
        (other, _) => return crate::ui::format::capitalised(&other.replace('_', " ")),
    };
    title.to_owned()
}

/// One row of the room's step list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepRow {
    pub(crate) id: String,
    pub(crate) title: String,
    /// A state this launcher doesn't know reads as pending.
    pub(crate) state: StepState,
    /// A done step with a problem the session goes on with ([`SessionStep::warns`]).
    pub(crate) warning: bool,
    /// Shown under an active, failed, skipped or warning step.
    pub(crate) detail: Option<String>,
    /// The step comes after a failed one, so the session stopped before it.
    pub(crate) dimmed: bool,
    /// What stopped a failed step, which the card's problem buttons act on, or what a warning step warns of.
    pub(crate) problem: Option<Problem>,
    /// What the problem's fix plan asks the player to change, one sentence each.
    pub(crate) fixes: Vec<String>,
    /// What the player does next, under the race step once everyone is in the lobby.
    pub(crate) hint: Option<&'static str>,
}

/// The steps of a module that reports them. The room page draws them as one row ([`super::tracker::Tracker`]).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepCard {
    /// Empty once the session ended without a step failing.
    pub(crate) rows: Vec<StepRow>,
}

impl StepCard {
    /// `None` from a module that reports no steps, whose card shows its phase and progress as before.
    pub(crate) fn of(state: &SessionState, leaving: bool) -> Option<Self> {
        if state.steps.is_empty() {
            return None;
        }
        let failed = state.steps.iter().position(|s| s.state == StepState::Failed);
        let warned = state.steps.iter().any(SessionStep::warns);
        let stray = failed.is_none() && !warned && state.problem.is_some();
        let over = state.phase.is_over();
        let lobby = matches!(state.phase, Phase::InLobby | Phase::Racing);
        let hint = (failed.is_none() && !stray && lobby && !leaving && !over)
            .then(|| lobby_hint(state.host, &state.phase));
        let rows = if over && failed.is_none() {
            Vec::new()
        } else {
            state
                .steps
                .iter()
                .enumerate()
                .map(|(i, step)| StepRow::of(step, state, failed.is_some_and(|f| i > f), hint))
                .collect()
        };
        Some(Self { rows })
    }
}

impl StepRow {
    fn of(step: &SessionStep, state: &SessionState, dimmed: bool, hint: Option<&'static str>) -> Self {
        let shown = match step.state {
            StepState::Unknown => StepState::Pending,
            known => known,
        };
        let warning = step.warns();
        let problem = match (shown, &step.problem) {
            (StepState::Failed, _) => Some(failure(step, state.problem.as_ref())),
            (_, Some(p)) if warning => Some(Problem {
                text: p.text.clone(),
                retry: false,
                detail: p.detail.clone(),
            }),
            _ => None,
        }
        .map(|mut p| {
            p.detail = p.detail.take().filter(|d| !repeats(d, &p.text));
            p
        });
        let fixes = step
            .problem
            .as_ref()
            .filter(|_| problem.is_some())
            .map(|p| fix_lines(&p.fix_plan))
            .unwrap_or_default();
        let detail = (warning || matches!(shown, StepState::Active | StepState::Failed | StepState::Skipped))
            .then(|| step.detail.as_deref().map(crate::ui::format::capitalised))
            .flatten()
            .filter(|d| problem.as_ref().is_none_or(|p| !repeats(d, &p.text)));
        Self {
            id: step.id.clone(),
            title: step_title(&step.id, state.host),
            state: shown,
            warning,
            detail,
            dimmed,
            problem,
            fixes,
            hint: hint.filter(|_| step.id == "race"),
        }
    }
}

/// Whether `detail` says the same as `text`, ignoring case and a closing full stop.
pub(crate) fn repeats(detail: &str, text: &str) -> bool {
    let plain = |s: &str| s.trim().trim_end_matches('.').trim_end().to_lowercase();
    plain(detail) == plain(text)
}

/// The room page's subtitle once the session is over, in place of the room's live details: whether the room
/// `opened` for this player, and whether they host it.
pub(crate) fn closed_subtitle(opened: bool, host: bool) -> &'static str {
    match (opened, host) {
        (true, _) => tr("Room closed"),
        (false, true) => tr("The room didn't open"),
        (false, false) => tr("You didn't join the room"),
    }
}

/// What the room page says once the session is over while F1 25 is still `starting`.
pub(crate) fn game_after_room(over: bool, starting: bool) -> Option<&'static str> {
    (over && starting).then(|| tr("F1 25 keeps starting without the room."))
}

/// What a fix plan asks the player to change to match the host, one sentence per step.
pub(crate) fn fix_lines(plan: &[FixStep]) -> Vec<String> {
    plan.iter()
        .map(|fix| match fix {
            FixStep::Install { name, version, .. } => trf!(
                "Install {name} {version} from the Addon Browser",
                name = name,
                version = version
            ),
            FixStep::Disable { name, .. } => trf!("Turn off {name}", name = name),
            FixStep::SetOption {
                addon,
                option,
                value: Some(value),
            } => trf!(
                "Set {option} in {addon} to {value}",
                option = option,
                addon = addon,
                value = value.as_str().map_or_else(|| value.to_string(), str::to_owned)
            ),
            FixStep::SetOption { addon, option, .. } => trf!(
                "Set {option} in {addon} back to its default",
                option = option,
                addon = addon
            ),
            FixStep::Reorder { .. } => tr("Put your addons in the host's load order").to_owned(),
            FixStep::Manual { line } => line.clone(),
        })
        .collect()
}

/// A failed step's problem as the card's problem buttons read it: whether trying again helps comes from the step,
/// else from the session's problem.
fn failure(step: &SessionStep, session: Option<&Problem>) -> Problem {
    let retry = |p: Option<bool>| p.or(session.map(|s| s.retry)).unwrap_or(false);
    match &step.problem {
        Some(p) => Problem {
            text: p.text.clone(),
            retry: retry(p.retry),
            detail: p.detail.clone(),
        },
        None => session.cloned().unwrap_or_else(|| Problem {
            text: tr("This step didn't finish.").to_owned(),
            retry: false,
            detail: None,
        }),
    }
}

/// F1 25 closing while the player is in a room that started it leaves the room: `seen` is that the room's
/// game has been running, `busy` that a launch, build or restore is still going.
pub(crate) fn exit_leaves(seen: bool, running: bool, busy: bool) -> bool {
    seen && !running && !busy
}

/// The restart dialog's button: what pressing it does.
pub(super) fn restart_ok_text(start: &Start) -> &'static str {
    match start {
        Start::Create(_) => "Create room and join",
        Start::Join(_) => "Join room",
    }
}

/// Whether the room leaves once its game exited: only with a session that reports no steps. One that does fails
/// its `game` step and waits there for Retry, which starts the game again, or Leave.
pub(crate) fn leaves_on_exit(state: Option<&SessionState>) -> bool {
    state.is_none_or(|s| s.steps.is_empty())
}

/// The name the create form starts with: the player's room, or "Your room" while the player's name isn't known.
pub(crate) fn default_room_name(player: Option<&str>) -> String {
    player.map_or_else(
        || tr("Your room").to_owned(),
        |name| trf!("{name}'s room", name = name),
    )
}

pub(crate) fn exit_notice(host: bool) -> &'static str {
    if host {
        "F1 25 closed, so your room closed."
    } else {
        "F1 25 closed, so you left the room."
    }
}

/// What the page shows in place of its list while the session module isn't loaded: the sentence the last
/// load failed with, kept while another load runs, gone once one succeeds.
pub(crate) fn refusal(shown: Option<String>, loaded: bool, failed: Option<String>) -> Option<String> {
    if loaded {
        return None;
    }
    failed.or(shown)
}

pub(crate) fn refusal_text(why: &str) -> String {
    trf!("Online sessions aren't available: {why}", why = why)
}

/// One member's state as the room lists it: what their game is doing. Only a game in the lobby counts as joined.
pub(crate) fn member_word(m: &MemberState) -> (&'static str, Tone) {
    if !m.connected {
        return ("Disconnected", Tone::Blocked);
    }
    match m.stage {
        Some(MemberStage::InLobby) => (MemberStage::InLobby.word(), Tone::Good),
        Some(stage) => (stage.word(), Tone::Working),
        None => ("Joining\u{2026}", Tone::Working),
    }
}

/// A member's word when the host picked the F1 2026 cars and their game says it doesn't have that content.
pub(crate) fn content_word(content: &RoomContent, m: &MemberState) -> Option<(&'static str, Tone)> {
    (m.connected && content.needs_f1_2026() && content.owns_f1_2026.get(&m.id) == Some(&false))
        .then_some(("No F1 2026 content", Tone::Blocked))
}

/// This player's row for the host's F1 2026 cars: whether their game has the content, `None` until the host
/// picked them.
pub(crate) fn content_check(state: &SessionState) -> Option<(Option<bool>, &'static str)> {
    if !state.content.needs_f1_2026() {
        return None;
    }
    let me = state.members.iter().find(|m| m.me)?;
    let owns = state.content.owns_f1_2026.get(&me.id).copied();
    Some((
        owns,
        match owns {
            Some(true) => "You have the F1 2026 content",
            Some(false) => "Your copy of F1 25 doesn't have the F1 2026 content",
            None => "F1 25 hasn't said yet whether you have it",
        },
    ))
}

pub(crate) fn joined(state: &SessionState) -> usize {
    state
        .members
        .iter()
        .filter(|m| m.connected && m.stage == Some(MemberStage::InLobby))
        .count()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AddonCheck {
    pub(crate) name: String,
    pub(crate) have: bool,
}

/// The room's addons, each with whether this player has it on.
pub(crate) fn addon_checks(addons: &[RoomAddon], on: &HashSet<String>) -> Vec<AddonCheck> {
    addons
        .iter()
        .map(|a| AddonCheck {
            name: a.name.clone(),
            have: on.contains(&a.id),
        })
        .collect()
}

pub(crate) fn missing(checks: &[AddonCheck]) -> usize {
    checks.iter().filter(|c| !c.have).count()
}

/// The names of this player's gameplay addons the host doesn't have, once the server lists every addon the
/// host has on.
pub(crate) fn extra_addons(room: &RoomSummary, mine: &OwnPrint) -> Vec<String> {
    let Some(listed) = room
        .addons
        .as_ref()
        .filter(|a| !a.list.is_empty() && a.list.len() == a.count as usize)
    else {
        return Vec::new();
    };
    mine.addons
        .iter()
        .filter(|a| !listed.list.iter().any(|h| h.id == a.id))
        .map(|a| a.name.clone())
        .collect()
}

/// The names a room card shows: every addon the host has on when the server lists them, else its tracks.
pub(crate) fn card_addons(room: &RoomSummary) -> Vec<String> {
    match &room.addons {
        Some(a) if !a.list.is_empty() => a.list.iter().map(|x| x.name.clone()).collect(),
        Some(a) => a.tracks.clone(),
        None => Vec::new(),
    }
}

/// Whether a page of the online rooms shows its ad, a banner in the page's content: not while the create form is
/// open, whose players menu would open under the ad's webview. The header has no place for it, as the Online
/// tab's links sit over the middle of the header.
pub(crate) fn shows_ad(form_open: bool) -> bool {
    !form_open
}

/// What the Multiplayer page shows: the room this launcher is in, a room looked at before joining it, or the list
/// of open rooms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RoomFocus {
    InRoom,
    Viewing(String),
    List,
}

/// `away` is a player in a room who stepped out to the list; the room keeps running and the list links back to it.
pub(crate) fn room_focus(in_room: bool, away: bool, viewing: Option<&str>) -> RoomFocus {
    match (in_room, viewing) {
        (true, _) if away => RoomFocus::List,
        (true, _) => RoomFocus::InRoom,
        (false, Some(code)) => RoomFocus::Viewing(code.to_owned()),
        (false, None) => RoomFocus::List,
    }
}

/// How many rooms the Time Trial page's rooms card lists out of `total`.
pub(crate) fn card_rooms(total: usize) -> usize {
    total.min(3)
}

#[derive(Clone)]
pub(super) enum Start {
    Create(RoomSettings),
    Join(String),
}

/// What a start is waiting on, as the page's busy state names it: "create", or the code it joins.
pub(super) fn starting(start: &Start) -> String {
    match start {
        Start::Create(_) => "create".into(),
        Start::Join(code) => code.clone(),
    }
}
