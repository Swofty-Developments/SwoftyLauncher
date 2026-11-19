use std::collections::HashSet;

use swofty_core::online::multiplayer::client::Local;
use swofty_core::online::multiplayer::types::{
    GameStatus, MemberStage, MemberState, Phase, Problem, Progress, RoomAddon, RoomContent, RoomSettings,
    RoomSummary, Screen, SessionState, Step,
};
use swofty_i18n::trf;

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

/// This player's fingerprint as a room's status reads it: its digest and its gameplay addons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnPrint {
    pub(crate) digest: String,
    pub(crate) addons: Vec<RoomAddon>,
}

impl OwnPrint {
    pub(crate) fn of(local: &Local) -> Self {
        Self {
            digest: local.digest(),
            addons: local
                .fingerprint
                .addons
                .iter()
                .map(|a| RoomAddon {
                    id: a.id.clone(),
                    name: a.name.clone(),
                })
                .collect(),
        }
    }
}

/// What a room's one status line says, and whether it needs the player's attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoomStatus {
    Full,
    Differ,
    Match,
    Unknown,
}

impl RoomStatus {
    /// `on` is the addons this player has on. With the host's addons listed, a room differs when one of them
    /// is off here or the host lacks one of this player's gameplay addons, as the room's View shows; the
    /// load order, option values, stat ids and builds the digests also cover come from the host in a room
    /// launch. A room whose host lists no addons differs when the digests do.
    pub(crate) fn of(room: &RoomSummary, on: &HashSet<String>, mine: Option<&OwnPrint>) -> Self {
        if room.members >= room.max_members {
            return Self::Full;
        }
        let (Some(host), Some(mine)) = (room.digest.as_deref(), mine) else {
            return Self::Unknown;
        };
        if host.eq_ignore_ascii_case(&mine.digest) {
            return Self::Match;
        }
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
            Self::Unknown => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Working,
    Good,
    Blocked,
    Over,
}

pub(crate) const STEPS: [(Step, &str); 6] = [
    (Step::SigningIn, "Signing in"),
    (Step::Joining, "Joining"),
    (Step::Connecting, "Connecting"),
    (Step::StartingGame, "Starting F1 25"),
    (Step::Loading, "Loading"),
    (Step::InLobby, "In the lobby"),
];

pub(crate) fn step_number(step: Step) -> usize {
    STEPS.iter().position(|(s, _)| *s == step).unwrap_or(0)
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
    let (tone, hint) = match (&problem, progress.step, state.host, &state.phase) {
        (Some(_), ..) => (Tone::Blocked, None),
        (None, Step::InLobby, true, Phase::Racing) => (
            Tone::Good,
            Some("Mark the race over when everyone is back in the lobby."),
        ),
        (None, Step::InLobby, true, _) => (
            Tone::Good,
            Some("Pick tracks and settings in F1 25's lobby, then start the race there."),
        ),
        (None, Step::InLobby, false, _) => {
            (Tone::Good, Some("The host picks the tracks and starts the race."))
        }
        (None, ..) => (Tone::Working, None),
    };
    Status {
        tone,
        progress,
        problem,
        hint,
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
