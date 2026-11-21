//! The room page's session steps as one row of nodes with one detail area under it, and what the header, the
//! play button and the player list say beside them. Pure: the page draws what these functions return.

use swofty_core::online::multiplayer::types::{Phase, Problem, SessionState, StepState};
use swofty_i18n::{tr, trf};

use super::model::{StepCard, StepRow, repeats};

/// What the launcher itself knows about the room's game, beside the session's state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Here {
    pub(crate) leaving: bool,
    /// F1 25 runs.
    pub(crate) running: bool,
    /// The launcher is closing F1 25 so the room can start it again.
    pub(crate) closing: bool,
    /// The launcher builds the game for the room.
    pub(crate) building: bool,
    /// The launcher is starting F1 25 for the room.
    pub(crate) launching: bool,
    /// A prompt Play opened holds the room's launch until the player answers it.
    pub(crate) held: bool,
    /// The last launch ask the launcher took up.
    pub(crate) taken: u32,
}

impl Here {
    /// The session asked for F1 25 and the launcher hasn't taken that ask up yet.
    fn ask_waiting(&self, state: &SessionState) -> bool {
        state.launch.as_ref().is_some_and(|a| a.seq != self.taken)
    }

    /// F1 25 is open with an ask waiting, so the launcher can't start it for the room until it closes.
    pub(crate) fn restart_due(&self, state: &SessionState) -> bool {
        self.running && !self.closing && !self.leaving && !state.phase.is_over() && self.ask_waiting(state)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    Done,
    Active,
    Warning,
    Failed,
    Pending,
}

impl Mark {
    fn of(row: &StepRow) -> Self {
        match row.state {
            _ if row.warning => Self::Warning,
            StepState::Done => Self::Done,
            StepState::Active => Self::Active,
            StepState::Failed => Self::Failed,
            StepState::Pending | StepState::Skipped | StepState::Unknown => Self::Pending,
        }
    }

    /// The node's state in words, for its tooltip, so no state rests on an icon or a colour alone.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Self::Done => tr("Done"),
            Self::Active => tr("In progress"),
            Self::Warning => tr("Done, with a warning"),
            Self::Failed => tr("Stopped"),
            Self::Pending => tr("Not started"),
        }
    }
}

/// One node of the row. Skipped steps get none.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Node {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) mark: Mark,
    /// After a failed step: the session stopped before it.
    pub(crate) dimmed: bool,
    /// The detail area speaks for this node.
    pub(crate) current: bool,
}

/// A button the detail area offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Act {
    RestartGame,
    Retry,
    Leave,
}

/// Something the session warns of beside the step it is on, with its own Retry when trying again helps.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Note {
    pub(crate) text: String,
    pub(crate) lines: Vec<String>,
    pub(crate) fixes: Vec<String>,
    pub(crate) retry: bool,
}

/// The detail area under the row: the step that matters now.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Focus {
    pub(crate) mark: Mark,
    /// "Step 4 of 7 · Build the game".
    pub(crate) caption: String,
    /// What is happening, or what went wrong. None when the page header already says it.
    pub(crate) title: Option<String>,
    pub(crate) lines: Vec<String>,
    pub(crate) fixes: Vec<String>,
    pub(crate) hint: Option<&'static str>,
    pub(crate) note: Option<Note>,
    pub(crate) acts: Vec<Act>,
    /// The host's race button goes with this step.
    pub(crate) race: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tracker {
    pub(crate) nodes: Vec<Node>,
    pub(crate) focus: Option<Focus>,
}

/// Module waits that only say what the step's title says.
const RESTATED: &[&str] = &[
    "signing in",
    "connecting to the room",
    "starting F1 25 for the room",
    "restart F1 25 for the room",
    "F1 25 is starting",
    "connecting to the other players",
    "starting the bridge",
    "loading F1 25 into the room's lobby",
    "waiting for the race to start",
];

fn restated(detail: &str) -> bool {
    RESTATED.iter().any(|r| repeats(detail, r))
}

/// Whether two sentences say the same, also with or without a leading "the".
pub(crate) fn same_words(a: &str, b: &str) -> bool {
    let bare = |s: &str| {
        let s = s.trim();
        s.strip_prefix("The ")
            .or_else(|| s.strip_prefix("the "))
            .unwrap_or(s)
            .to_owned()
    };
    repeats(&bare(a), &bare(b))
}

fn push_new(lines: &mut Vec<String>, line: Option<String>, said: &[&str]) {
    let Some(line) = line.filter(|l| !l.trim().is_empty()) else {
        return;
    };
    if said.iter().any(|s| same_words(&line, s)) || lines.iter().any(|l| same_words(l, &line)) {
        return;
    }
    lines.push(line);
}

impl Tracker {
    /// `card` is the step card after the launcher's own overlays; `subtitle` is what the page header says, so the
    /// detail area doesn't say it again.
    pub(crate) fn of(card: &StepCard, state: &SessionState, here: &Here, subtitle: &str) -> Self {
        let shown: Vec<&StepRow> = card
            .rows
            .iter()
            .filter(|r| r.state != StepState::Skipped)
            .collect();
        let at = focus_index(&shown, &state.phase);
        let nodes = shown
            .iter()
            .enumerate()
            .map(|(i, r)| Node {
                id: r.id.clone(),
                title: r.title.clone(),
                mark: Mark::of(r),
                dimmed: r.dimmed,
                current: Some(i) == at && !here.leaving,
            })
            .collect();
        let focus = at
            .filter(|_| !here.leaving)
            .map(|i| focus(shown[i], i, shown.len(), &shown, state, here, subtitle));
        Self { nodes, focus }
    }
}

/// The step the detail area speaks for: a failed one, else the active one, else the last one shown.
fn focus_index(shown: &[&StepRow], phase: &Phase) -> Option<usize> {
    let find = |state: StepState| shown.iter().position(|r| r.state == state);
    find(StepState::Failed)
        .or_else(|| find(StepState::Active))
        .or_else(|| (!phase.is_over()).then(|| shown.len().checked_sub(1)).flatten())
}

fn focus(
    row: &StepRow,
    at: usize,
    count: usize,
    shown: &[&StepRow],
    state: &SessionState,
    here: &Here,
    subtitle: &str,
) -> Focus {
    let over = state.phase.is_over();
    let live = !over && !here.leaving;
    let mark = Mark::of(row);
    let caption = trf!(
        "Step {n} of {count} \u{00b7} {title}",
        n = at + 1,
        count = count,
        title = row.title
    );
    let mut lines = Vec::new();
    let mut acts = Vec::new();
    let mut note = None;
    let title = match mark {
        Mark::Failed | Mark::Warning => {
            let problem = row.problem.clone().unwrap_or_else(|| Problem {
                text: row.title.clone(),
                retry: false,
                detail: None,
            });
            let title = (!same_words(&problem.text, subtitle)).then(|| problem.text.clone());
            let said = [problem.text.as_str(), subtitle];
            push_new(&mut lines, problem.detail.clone(), &said);
            push_new(&mut lines, row.detail.clone(), &said);
            if mark == Mark::Failed && live {
                if here.restart_due(state) {
                    acts.push(Act::RestartGame);
                }
                if problem.retry {
                    push_new(
                        &mut lines,
                        game_gone(state, &row.id).then(|| retry_starts().to_owned()),
                        &said,
                    );
                    acts.push(Act::Retry);
                } else {
                    acts.push(Act::Leave);
                }
            }
            title
        }
        Mark::Active | Mark::Done | Mark::Pending => {
            let title = doing(row, state, here);
            push_new(&mut lines, launcher_line(row, state, here), &[title.as_str()]);
            push_new(
                &mut lines,
                row.detail.clone().filter(|d| !restated(d)),
                &[title.as_str(), subtitle],
            );
            if here.restart_due(state) {
                acts.push(Act::RestartGame);
            }
            if live {
                note = aside(row, shown, state);
            }
            Some(title)
        }
    };
    let hint = row
        .hint
        .filter(|_| live && (state.host || state.phase != Phase::Racing));
    Focus {
        mark,
        caption,
        title,
        lines,
        fixes: if mark == Mark::Failed || mark == Mark::Warning {
            row.fixes.clone()
        } else {
            Vec::new()
        },
        hint,
        note,
        acts,
        race: row.id == "race" && races(state, here),
    }
}

/// Whether the host's race button shows: while the room's game is in its lobby or racing.
pub(crate) fn races(state: &SessionState, here: &Here) -> bool {
    state.host && !here.leaving && matches!(state.phase, Phase::InLobby | Phase::Racing)
}

fn retry_starts() -> &'static str {
    tr("Retry starts F1 25 again for the room.")
}

/// The failed step's problem says the game is gone, so Retry starts it again.
fn game_gone(state: &SessionState, id: &str) -> bool {
    state
        .steps
        .iter()
        .find(|s| s.id == id)
        .and_then(|s| s.problem.as_ref())
        .is_some_and(|p| matches!(p.code.as_str(), "game_exited" | "game_crashed"))
}

/// A warning step's problem, or the session's problem no step carries, beside the step the session is on. The
/// module's own sentence for a restart is left out: the restart's title says it.
fn aside(row: &StepRow, shown: &[&StepRow], state: &SessionState) -> Option<Note> {
    if let Some(warned) = shown.iter().find(|r| r.warning && r.id != row.id) {
        let problem = warned.problem.as_ref()?;
        let mut lines = Vec::new();
        push_new(&mut lines, problem.detail.clone(), &[problem.text.as_str()]);
        return Some(Note {
            text: problem.text.clone(),
            lines,
            fixes: warned.fixes.clone(),
            retry: false,
        });
    }
    let restart = state.launch.as_ref().is_some_and(|a| a.restart);
    let failed = shown.iter().any(|r| r.state == StepState::Failed);
    let problem = state.problem.as_ref().filter(|_| !restart && !failed)?;
    let mut lines = Vec::new();
    push_new(&mut lines, problem.detail.clone(), &[problem.text.as_str()]);
    Some(Note {
        text: problem.text.clone(),
        lines,
        fixes: Vec::new(),
        retry: problem.retry,
    })
}

/// What the step is doing, in plain words.
fn doing(row: &StepRow, state: &SessionState, here: &Here) -> String {
    let host = state.host;
    let restart = state.launch.as_ref().is_some_and(|a| a.restart);
    let text = match row.id.as_str() {
        "account" => tr("Logging in"),
        "room" if host => tr("Opening the room"),
        "room" => tr("Joining the room"),
        "addons" if host => tr("Checking your addons"),
        "addons" => tr("Matching the host's addons"),
        "build" | "game" if here.restart_due(state) => tr("F1 25 has to restart for the room"),
        "build" | "game"
            if here.closing || (restart && (here.building || here.launching || here.ask_waiting(state))) =>
        {
            tr("Restarting F1 25 for the room")
        }
        "build" | "game" if here.building => tr("Building the game for the room"),
        "build" | "game" if here.launching => tr("Starting F1 25 for the room"),
        "build" => tr("Getting F1 25 ready for the room"),
        "game" => tr("Starting F1 25 for the room"),
        "network" => tr("Connecting to the players"),
        "lobby" if host => tr("Opening the LAN lobby"),
        "lobby" => tr("Joining the host's LAN lobby"),
        "race" if state.phase == Phase::Racing => tr("Racing"),
        "race" if row.state == StepState::Done => tr("Racing"),
        "race" => tr("Waiting for the race to start"),
        _ => return row.title.clone(),
    };
    text.to_owned()
}

/// What the launcher is doing for the build and game steps that the step's title doesn't already say.
fn launcher_line(row: &StepRow, state: &SessionState, here: &Here) -> Option<String> {
    if !matches!(row.id.as_str(), "build" | "game") {
        return None;
    }
    let line = if here.restart_due(state) {
        tr("Restarting closes F1 25, so unsaved progress is lost.")
    } else if here.closing {
        tr("Closing F1 25")
    } else if here.building && state.launch.as_ref().is_some_and(|a| a.restart) {
        tr("Building the game")
    } else if here.held && here.ask_waiting(state) {
        tr("Answer the open dialog to go on.")
    } else {
        return None;
    };
    Some(line.to_owned())
}

/// The line above the row, only for what no step says: leaving, or a session over without a failed step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Banner {
    Leaving,
    Over(String),
}

/// `text` is the session's own headline; `subtitle` what the page header says.
pub(crate) fn banner(
    card: &StepCard,
    state: &SessionState,
    leaving: bool,
    text: &str,
    subtitle: &str,
) -> Option<Banner> {
    let over = state.phase.is_over();
    let failed = card.rows.iter().any(|r| r.state == StepState::Failed);
    if leaving && !over {
        return Some(Banner::Leaving);
    }
    (over && !failed && !same_words(text, subtitle)).then(|| Banner::Over(text.to_owned()))
}

/// Whether the page header says the session is over: never while a step is active.
pub(crate) fn shows_closed(state: &SessionState, card: Option<&StepCard>) -> bool {
    let active = card.is_some_and(|c| c.rows.iter().any(|r| r.state == StepState::Active));
    state.phase.is_over() && !active
}

/// Whether the room lists its players and their cars: not once it is over, when each player's word is stale.
pub(crate) fn shows_players(state: &SessionState) -> bool {
    !state.phase.is_over()
}

/// The play button while a session runs. Its words are neutral: the play bar shows on every page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlayWord {
    /// The session is starting or leaving: a disabled button with a spinner.
    Busy(&'static str),
    /// The session waits for the player on its page: the button opens it.
    Needed,
}

impl PlayWord {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Busy(word) => word,
            Self::Needed => tr("Action needed"),
        }
    }
}

/// `busy` is that a session is starting or running.
pub(crate) fn play_word(busy: bool, leaving: bool, state: Option<&SessionState>) -> Option<PlayWord> {
    if !busy || state.is_some_and(|s| s.phase.is_over()) {
        return None;
    }
    if leaving {
        return Some(PlayWord::Busy(tr("Leaving…")));
    }
    let stopped = state.is_some_and(|s| {
        if s.steps.is_empty() {
            s.problem.is_some()
        } else {
            s.steps.iter().any(|st| st.state == StepState::Failed)
        }
    });
    Some(if stopped {
        PlayWord::Needed
    } else {
        PlayWord::Busy(tr("Starting…"))
    })
}
