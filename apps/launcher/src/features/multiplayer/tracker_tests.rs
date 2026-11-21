use swofty_core::online::multiplayer::client::state_of;
use swofty_core::online::multiplayer::types::SessionState;

use super::model::{StepCard, closed_subtitle, room_status};
use super::preview::{FIXTURES, fixture, room};
use super::tracker::{
    Act, Banner, Here, Mark, PlayWord, Tracker, banner, play_word, same_words, shows_closed, shows_players,
};

/// Everything the room page shows for one state, as the page works it out.
struct Page {
    subtitle: Option<&'static str>,
    banner: Option<Banner>,
    tracker: Tracker,
    play: Option<PlayWord>,
    players: bool,
}

fn page(state: &SessionState, here: Here) -> Page {
    let card = StepCard::of(state, here.leaving).expect("a module with steps");
    let subtitle =
        shows_closed(state, Some(&card)).then(|| closed_subtitle(state.room.is_some(), state.host));
    let st = room_status(state);
    let headline = st
        .problem
        .as_ref()
        .map_or(st.progress.label.clone(), |p| p.text.clone());
    let said = subtitle.unwrap_or("");
    Page {
        subtitle,
        banner: banner(&card, state, here.leaving, &headline, said),
        tracker: Tracker::of(&card, state, &here, said),
        play: play_word(true, here.leaving, Some(state)),
        players: shows_players(state),
    }
}

fn named(name: &str) -> (SessionState, Here) {
    let doc = fixture(name).unwrap_or_else(|| panic!("fixture {name}"));
    let state = state_of(doc).unwrap_or_else(|| panic!("fixture {name} reads"));
    let here = super::preview::room(true, Some(std::ffi::OsStr::new(name)))
        .and_then(|(_, p)| p.here)
        .unwrap_or_default();
    (state, here)
}

fn current(p: &Page) -> Option<&str> {
    p.tracker.nodes.iter().find(|n| n.current).map(|n| n.id.as_str())
}

fn title(p: &Page) -> Option<&str> {
    p.tracker.focus.as_ref().and_then(|f| f.title.as_deref())
}

fn acts(p: &Page) -> Vec<Act> {
    p.tracker
        .focus
        .as_ref()
        .map(|f| f.acts.clone())
        .unwrap_or_default()
}

/// Every text the page shows above and under the row, to check none says what another says.
fn texts(p: &Page) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    all.extend(p.subtitle.map(str::to_owned));
    if let Some(Banner::Over(t)) = &p.banner {
        all.push(t.clone());
    }
    if let Some(f) = &p.tracker.focus {
        all.extend(f.title.clone());
        all.extend(f.lines.iter().cloned());
        all.extend(f.hint.map(str::to_owned));
        if let Some(n) = &f.note {
            all.push(n.text.clone());
            all.extend(n.lines.iter().cloned());
        }
    }
    all
}

#[test]
fn every_fixture_reads_and_previews_by_name() {
    for name in FIXTURES {
        let (state, _) = named(name);
        assert!(!state.steps.is_empty(), "{name}");
        assert!(room(true, Some(std::ffi::OsStr::new(name))).is_some(), "{name}");
        assert!(
            room(false, Some(std::ffi::OsStr::new(name))).is_none(),
            "{name}: a release build"
        );
    }
    assert!(fixture("nope").is_none());
}

/// One row per state the module sends: the step the detail area speaks for, its title, whether a warning or
/// closed text shows, the buttons, and the play button.
#[test]
fn each_state_renders_one_story() {
    struct Row {
        name: &'static str,
        current: Option<&'static str>,
        title: Option<&'static str>,
        warned: bool,
        closed: bool,
        acts: &'static [Act],
        play: Option<PlayWord>,
        players: bool,
    }
    let starting = Some(PlayWord::Busy("Starting…"));
    let table = [
        Row {
            name: "launching",
            current: Some("game"),
            title: Some("Starting F1 25 for the room"),
            warned: false,
            closed: false,
            acts: &[],
            play: starting,
            players: true,
        },
        Row {
            name: "restart-needed",
            current: Some("build"),
            title: Some("F1 25 has to restart for the room"),
            warned: false,
            closed: false,
            acts: &[Act::RestartGame],
            play: starting,
            players: true,
        },
        Row {
            name: "restarting",
            current: Some("build"),
            title: Some("Restarting F1 25 for the room"),
            warned: false,
            closed: false,
            acts: &[],
            play: starting,
            players: true,
        },
        Row {
            name: "lobby",
            current: Some("race"),
            title: Some("Waiting for the race to start"),
            warned: false,
            closed: false,
            acts: &[],
            play: starting,
            players: true,
        },
        Row {
            name: "lobby-host",
            current: Some("race"),
            title: Some("Waiting for the race to start"),
            warned: false,
            closed: false,
            acts: &[],
            play: starting,
            players: true,
        },
        Row {
            name: "racing",
            current: Some("race"),
            title: Some("Racing"),
            warned: false,
            closed: false,
            acts: &[],
            play: starting,
            players: true,
        },
        Row {
            name: "addons-fixable",
            current: Some("build"),
            title: Some("Building the game for the room"),
            warned: true,
            closed: false,
            acts: &[],
            play: starting,
            players: true,
        },
        Row {
            name: "addons-installing",
            current: Some("addons"),
            title: Some("Matching the host's addons"),
            warned: false,
            closed: false,
            acts: &[],
            play: starting,
            players: true,
        },
        Row {
            name: "game-crashed",
            current: Some("game"),
            title: Some("F1 25 crashed."),
            warned: false,
            closed: false,
            acts: &[Act::Retry],
            play: Some(PlayWord::Needed),
            players: true,
        },
        Row {
            name: "host-closed",
            current: Some("room"),
            title: Some("The host closed the room."),
            warned: false,
            closed: true,
            acts: &[],
            play: None,
            players: false,
        },
        Row {
            name: "relay-lost",
            current: Some("network"),
            title: Some("The connection to the relay dropped."),
            warned: false,
            closed: false,
            acts: &[Act::Retry],
            play: Some(PlayWord::Needed),
            players: true,
        },
        Row {
            name: "finished",
            current: None,
            title: None,
            warned: false,
            closed: true,
            acts: &[],
            play: None,
            players: false,
        },
    ];
    assert_eq!(table.len(), FIXTURES.len(), "a row per fixture");
    for row in table {
        let (state, here) = named(row.name);
        let p = page(&state, here);
        let name = row.name;
        assert_eq!(
            current(&p),
            row.current,
            "{name}: the step the detail area speaks for"
        );
        assert_eq!(title(&p), row.title, "{name}: the detail area's title");
        let warned = p.tracker.focus.as_ref().is_some_and(|f| f.note.is_some());
        assert_eq!(warned, row.warned, "{name}: a warning beside the step");
        assert_eq!(p.subtitle.is_some(), row.closed, "{name}: the header says closed");
        assert_eq!(acts(&p), row.acts, "{name}: the buttons");
        assert_eq!(p.play, row.play, "{name}: the play button");
        assert_eq!(p.players, row.players, "{name}: the player list");
        let all = texts(&p);
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(!same_words(a, b), "{name}: {a:?} is said twice");
            }
        }
    }
}

#[test]
fn a_session_working_normally_shows_no_warning_and_nothing_closed() {
    for name in [
        "launching",
        "restart-needed",
        "restarting",
        "lobby",
        "racing",
        "addons-installing",
    ] {
        let (state, here) = named(name);
        let p = page(&state, here);
        assert!(p.banner.is_none(), "{name}: no line above the row");
        assert!(p.subtitle.is_none(), "{name}: no closed text");
        let focus = p.tracker.focus.as_ref().unwrap();
        assert!(focus.note.is_none(), "{name}: no warning");
        assert!(
            !matches!(focus.mark, Mark::Failed | Mark::Warning),
            "{name}: the step it is on is fine"
        );
        assert!(
            p.tracker
                .nodes
                .iter()
                .all(|n| !matches!(n.mark, Mark::Failed | Mark::Warning)),
            "{name}"
        );
    }
}

#[test]
fn the_restart_says_what_happens_instead_of_the_modules_warning() {
    let (state, here) = named("restart-needed");
    let p = page(&state, here);
    let all = texts(&p).join(" | ");
    assert!(!all.contains("has to restart for this session"), "{all}");
    assert!(
        !all.contains("Restart F1 25 for the room"),
        "the module's wait reads as an order: {all}"
    );
    let focus = p.tracker.focus.unwrap();
    assert_eq!(
        focus.lines,
        ["Restarting closes F1 25, so unsaved progress is lost."]
    );

    let closing = Here {
        closing: true,
        ..here
    };
    let p = page(&state, closing);
    assert_eq!(title(&p), Some("Restarting F1 25 for the room"));
    assert!(acts(&p).is_empty(), "the launcher is already closing it");
    assert_eq!(p.tracker.focus.unwrap().lines, ["Closing F1 25"]);

    let launched = Here {
        running: false,
        launching: true,
        taken: 2,
        ..here
    };
    let p = page(&state, launched);
    assert_eq!(title(&p), Some("Restarting F1 25 for the room"));

    let building = Here {
        running: false,
        building: true,
        taken: 2,
        ..here
    };
    assert_eq!(
        page(&state, building).tracker.focus.unwrap().lines,
        ["Building the game"]
    );
}

#[test]
fn an_ask_the_launcher_took_up_is_not_a_restart_to_press() {
    let (state, here) = named("launching");
    assert!(
        !here.restart_due(&state),
        "taken up: the room's own game is starting"
    );
    let untaken = Here { taken: 0, ..here };
    assert!(
        untaken.restart_due(&state),
        "F1 25 runs and the ask waits for it to close"
    );
    assert!(
        !Here {
            leaving: true,
            ..untaken
        }
        .restart_due(&state)
    );
}

#[test]
fn a_failed_step_stops_the_row_there_and_offers_what_helps() {
    let (state, here) = named("game-crashed");
    let p = page(&state, here);
    let after: Vec<bool> = p
        .tracker
        .nodes
        .iter()
        .skip_while(|n| n.id != "game")
        .skip(1)
        .map(|n| n.dimmed)
        .collect();
    assert!(!after.is_empty() && after.iter().all(|d| *d));
    let focus = p.tracker.focus.unwrap();
    assert_eq!(focus.mark, Mark::Failed);
    assert!(
        focus
            .lines
            .iter()
            .any(|l| l == "Retry starts F1 25 again for the room.")
    );
    assert!(focus.caption.starts_with("Step 5 of 8"), "{}", focus.caption);
    assert_eq!(
        page(
            &state,
            Here {
                running: true,
                ..here
            }
        )
        .tracker
        .focus
        .unwrap()
        .acts,
        [Act::Retry],
        "no launch ask waits, so nothing to restart"
    );
}

#[test]
fn skipped_steps_get_no_node() {
    let (mut state, here) = named("lobby");
    state.steps[3].state = swofty_core::online::multiplayer::types::StepState::Skipped;
    let p = page(&state, here);
    assert!(p.tracker.nodes.iter().all(|n| n.id != "build"));
    assert_eq!(p.tracker.nodes.len(), 7);
}

#[test]
fn the_closed_room_says_why_once() {
    let (state, here) = named("host-closed");
    let p = page(&state, here);
    assert_eq!(p.subtitle, Some("Room closed"));
    assert!(p.banner.is_none(), "the failed step says why");
    assert!(acts(&p).is_empty(), "Back to rooms in the header is the way on");

    let mut generic = state.clone();
    generic.steps[1].problem.as_mut().unwrap().text = "The room closed.".into();
    let p = page(&generic, here);
    assert_eq!(title(&p), None, "the header already says the room closed");

    let (state, here) = named("finished");
    let p = page(&state, here);
    assert_eq!(p.banner, Some(Banner::Over("The host ended the session".into())));
    assert!(p.tracker.nodes.is_empty());
    let mut plain = state.clone();
    plain.phase = swofty_core::online::multiplayer::types::Phase::Ended("the room closed".into());
    assert_eq!(page(&plain, here).banner, None, "the header says it");
}

#[test]
fn leaving_shows_one_line_and_no_buttons() {
    let (state, here) = named("game-crashed");
    let p = page(
        &state,
        Here {
            leaving: true,
            ..here
        },
    );
    assert_eq!(p.banner, Some(Banner::Leaving));
    assert!(p.tracker.focus.is_none());
    assert!(p.tracker.nodes.iter().all(|n| !n.current));
    assert_eq!(p.play, Some(PlayWord::Busy("Leaving…")));
}

#[test]
fn a_problem_no_step_carries_shows_beside_the_step_not_above_the_row() {
    let (mut state, here) = named("lobby");
    state.problem = Some(swofty_core::online::multiplayer::types::Problem {
        text: "Having trouble reaching the other players.".into(),
        retry: true,
        detail: None,
    });
    let p = page(&state, here);
    assert!(p.banner.is_none());
    let note = p.tracker.focus.unwrap().note.unwrap();
    assert_eq!(note.text, "Having trouble reaching the other players.");
    assert!(note.retry);
}

#[test]
fn the_hint_says_what_to_do_in_the_lobby_and_nothing_stale_while_racing() {
    let (state, here) = named("lobby");
    let p = page(&state, here);
    assert_eq!(
        p.tracker.focus.unwrap().hint,
        Some("The host picks the tracks and starts the race.")
    );
    let (state, here) = named("lobby-host");
    let focus = page(&state, here).tracker.focus.unwrap();
    assert!(focus.race, "the host's race button");
    let (mut state, here) = named("racing");
    assert!(page(&state, here).tracker.focus.unwrap().race);
    state.host = false;
    let focus = page(&state, here).tracker.focus.unwrap();
    assert_eq!(focus.hint, None, "a joiner racing has nothing to do on the page");
    assert!(!focus.race);
}

#[test]
fn the_play_button_follows_the_session_in_neutral_words() {
    let (crashed, _) = named("game-crashed");
    let (lobby, _) = named("lobby");
    let (closed, _) = named("host-closed");
    let table: [(bool, bool, Option<&SessionState>, Option<PlayWord>); 7] = [
        (false, false, None, None),
        (true, false, None, Some(PlayWord::Busy("Starting…"))),
        (true, false, Some(&lobby), Some(PlayWord::Busy("Starting…"))),
        (true, false, Some(&crashed), Some(PlayWord::Needed)),
        (true, true, Some(&crashed), Some(PlayWord::Busy("Leaving…"))),
        (true, false, Some(&closed), None),
        (true, true, Some(&closed), None),
    ];
    for (busy, leaving, state, want) in table {
        assert_eq!(play_word(busy, leaving, state), want, "{busy} {leaving}");
    }
    for word in [
        PlayWord::Busy("Starting…"),
        PlayWord::Busy("Leaving…"),
        PlayWord::Needed,
    ] {
        let label = word.label().to_lowercase();
        for secret in ["room", "lobby", "lan", "multiplayer"] {
            assert!(!label.contains(secret), "{label}");
        }
    }
    let mut legacy = lobby.clone();
    legacy.steps.clear();
    assert_eq!(
        play_word(true, false, Some(&legacy)),
        Some(PlayWord::Busy("Starting…"))
    );
    legacy.problem = Some(swofty_core::online::multiplayer::types::Problem {
        text: "x".into(),
        retry: true,
        detail: None,
    });
    assert_eq!(play_word(true, false, Some(&legacy)), Some(PlayWord::Needed));
}

#[test]
fn the_header_never_says_closed_while_a_step_is_active() {
    let (mut state, here) = named("lobby");
    state.phase = swofty_core::online::multiplayer::types::Phase::Ended("the room closed".into());
    let card = StepCard::of(&state, here.leaving).unwrap();
    assert!(card.rows.is_empty(), "over without a failed step shows no rows");
    assert!(shows_closed(&state, Some(&card)));
    let (live, _) = named("lobby");
    let mut card = StepCard::of(&live, false).unwrap();
    assert!(!shows_closed(&live, Some(&card)));
    let mut over = live.clone();
    over.phase = swofty_core::online::multiplayer::types::Phase::Failed("x".into());
    card.rows.iter_mut().for_each(|r| {
        if r.id == "addons" {
            r.state = swofty_core::online::multiplayer::types::StepState::Active;
        }
    });
    assert!(
        !shows_closed(&over, Some(&card)),
        "an overlay's active step keeps the live header"
    );
}

#[test]
fn same_words_ignores_a_leading_the_case_and_a_full_stop() {
    assert!(same_words("The room closed.", "Room closed"));
    assert!(same_words("room closed", "The Room Closed."));
    assert!(!same_words("The host closed the room.", "Room closed"));
}
