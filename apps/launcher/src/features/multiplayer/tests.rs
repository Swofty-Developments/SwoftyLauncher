use std::collections::HashSet;
use std::time::Instant;

use swofty_core::online::multiplayer::types::{
    GameStatus, MemberStage, MemberState, Phase, PrintMatch, Problem, RelayStatus, Role, RoomAddon,
    RoomAddons, RoomSummary, Screen, SessionState, SessionStep, StepState, User,
};

use super::model::{
    GameState, OwnPrint, RoomStatus, Start, Tone, addon_checks, card_addons, content_check, content_word,
    exit_leaves, exit_notice, extra_addons, joined, member_word, missing, refusal, refusal_text,
    restart_ok_text, room_meta,
};
use super::model::{
    RoomFocus, StepCard, StepRow, card_rooms, closed_subtitle, default_room_name, fix_lines, game_after_room,
    leaves_on_exit, lobby_hint, repeats, room_focus, shows_ad, starting, step_title,
};
use super::state::{Lobby, room_sizes, unasked};
use swofty_core::online::multiplayer::types::RoomContent;

#[test]
fn a_problem_reads_with_or_without_its_detail() {
    let read = |json: &str| serde_json::from_str::<Problem>(json).unwrap();
    assert_eq!(
        read(r#"{"text":"Simplified doesn't match the host's.","retry":false}"#).detail,
        None
    );
    let said = read(
        r#"{"text":"Simplified doesn't match the host's.","retry":false,"detail":"Its build step made more."}"#,
    );
    assert_eq!(said.detail.as_deref(), Some("Its build step made more."));
    assert!(
        !serde_json::to_string(&read(r#"{"text":"x","retry":true}"#))
            .unwrap()
            .contains("detail")
    );
}

fn status(screen: Screen) -> GameStatus {
    GameStatus {
        screen,
        in_session: false,
        role: Role::None,
        configured: false,
        self_member: 0,
        base: 0,
        cover: None,
        room_failed: None,
        content: None,
    }
}

fn addon(id: &str, name: &str) -> RoomAddon {
    RoomAddon {
        id: id.into(),
        name: name.into(),
        ..RoomAddon::default()
    }
}

fn room(members: u32, max: u32, digest: Option<&str>) -> RoomSummary {
    RoomSummary {
        code: "F1-7KQ2MX".into(),
        name: "Sunday league".into(),
        host: User {
            id: "u1".into(),
            name: "swofty".into(),
            avatar: None,
        },
        members,
        max_members: max,
        category: "f1-2026".into(),
        digest: digest.map(str::to_owned),
        addons: Some(RoomAddons {
            count: 4,
            tracks: vec!["Okayama".into()],
            list: vec![
                addon("a.tracks", "Okayama"),
                addon("b.cars", "2026 Cars"),
                addon("c.hud", "Timing HUD"),
            ],
            ..RoomAddons::default()
        }),
        created_at: 0,
    }
}

fn session(phase: Phase, host: bool) -> SessionState {
    SessionState {
        phase,
        host,
        room: None,
        self_member: None,
        members: Vec::new(),
        mismatches: Vec::new(),
        lobby_connected: true,
        relay: None,
        base: None,
        bridge: None,
        game: None,
        last_error: None,
        launch: None,
        content: RoomContent::default(),
        steps: Vec::new(),
        progress: None,
        problem: None,
    }
}

fn member(connected: bool, stage: Option<MemberStage>) -> MemberState {
    MemberState {
        id: "u2".into(),
        name: "Max".into(),
        avatar: None,
        number: 2,
        ip: "10.77.0.2".into(),
        host: false,
        me: false,
        connected,
        fingerprint: PrintMatch::Matches,
        relay: RelayStatus::Ready,
        stage,
        traffic: None,
    }
}

#[test]
fn names_where_the_game_is() {
    assert_eq!(
        GameState::of(false, Some(&status(Screen::MainMenu))),
        GameState::NotRunning
    );
    assert_eq!(
        GameState::of(true, Some(&status(Screen::MainMenu))),
        GameState::MainMenu
    );
    assert_eq!(
        GameState::of(true, Some(&status(Screen::LanLobby))),
        GameState::LanLobby
    );
    assert_eq!(GameState::of(true, Some(&status(Screen::Other))), GameState::Busy);
    assert_eq!(GameState::of(true, None), GameState::Busy);
}

fn own(digest: &str, addons: &[(&str, &str)]) -> OwnPrint {
    OwnPrint {
        digest: Some(digest.into()),
        addons: addons.iter().map(|(id, name)| addon(id, name)).collect(),
        matching: None,
    }
}

fn ids(addons: &[(&str, &str)]) -> HashSet<String> {
    addons.iter().map(|(id, _)| (*id).to_owned()).collect()
}

#[test]
fn gives_each_room_one_status() {
    let all = [
        ("a.tracks", "Okayama"),
        ("b.cars", "2026 Cars"),
        ("c.hud", "Timing HUD"),
    ];
    let on = ids(&all);
    let of = |room: &RoomSummary, mine: &str| RoomStatus::of(room, &on, Some(&own(mine, &all)));
    assert_eq!(of(&room(8, 8, Some("sha256:a")), "sha256:a"), RoomStatus::Full);
    assert_eq!(
        of(&room(3, 8, Some("sha256:ABC")), "sha256:abc"),
        RoomStatus::Match
    );
    assert_eq!(
        of(&room(3, 8, Some("sha256:a")), "sha256:b"),
        RoomStatus::Match,
        "every listed addon on, the rest a room launch takes from the host"
    );
    let without_cars = ids(&[("a.tracks", "Okayama"), ("c.hud", "Timing HUD")]);
    assert_eq!(
        RoomStatus::of(
            &room(3, 8, Some("sha256:a")),
            &without_cars,
            Some(&own("sha256:b", &all))
        ),
        RoomStatus::Differ
    );
    assert_eq!(of(&room(3, 8, None), "sha256:b"), RoomStatus::Unknown);
    assert_eq!(
        RoomStatus::of(&room(3, 8, Some("sha256:a")), &on, None),
        RoomStatus::Unknown
    );
    let mut unlisted = room(3, 8, Some("sha256:a"));
    unlisted.addons = None;
    assert_eq!(of(&unlisted, "sha256:b"), RoomStatus::Differ);
    assert_eq!(of(&unlisted, "sha256:a"), RoomStatus::Match);
}

#[test]
fn a_room_differs_in_its_list_only_where_its_view_shows_a_difference() {
    let hosts = [
        ("kizi.ai-driver-behaviour", "AI Driver Behaviour"),
        ("swofty.core", "SwoftyMod"),
        ("swofty.lap1-ai", "Lap 1 Realistic AI"),
        ("swofty.swofty-seasons-2011", "SWOFTY 2011"),
        ("swofty.swofty-seasons-2011-real", "SWOFTY 2011 Real Liveries"),
        ("teamsimplified.mods", "Simplified for Swofty"),
    ];
    let mut room = room(1, 8, Some("sha256:dde0b6530ef0753d"));
    room.addons = Some(RoomAddons {
        count: hosts.len() as u32,
        tracks: Vec::new(),
        list: hosts.iter().map(|(id, name)| addon(id, name)).collect(),
        ..RoomAddons::default()
    });
    let mine = own("sha256:b860077ab11537ec", &hosts);
    assert_eq!(
        RoomStatus::of(&room, &ids(&hosts), Some(&mine)),
        RoomStatus::Match
    );
    assert!(extra_addons(&room, &mine).is_empty());

    let mut with_hud = hosts.to_vec();
    with_hud.push(("player.telemetry-hud", "Telemetry HUD"));
    let mine = own("sha256:43f821ef51b134cf", &with_hud);
    assert_eq!(
        RoomStatus::of(&room, &ids(&with_hud), Some(&mine)),
        RoomStatus::Differ
    );
    assert_eq!(extra_addons(&room, &mine), ["Telemetry HUD"]);

    let mut capped = room.clone();
    capped.addons.as_mut().unwrap().count += 1;
    assert!(
        extra_addons(&capped, &mine).is_empty(),
        "a list the server cut short says nothing of extra addons"
    );
    assert_eq!(
        RoomStatus::of(&capped, &ids(&with_hud), Some(&mine)),
        RoomStatus::Match
    );

    let without_season = ids(&hosts[..3]);
    let mine = own("sha256:43f821ef51b134cf", &hosts);
    assert_eq!(
        RoomStatus::of(&room, &without_season, Some(&mine)),
        RoomStatus::Differ
    );
}

#[test]
fn describes_a_room_with_every_addon() {
    assert_eq!(room_meta(&room(3, 8, None)), "swofty \u{00b7} 3/8 players");
    assert_eq!(
        card_addons(&room(3, 8, None)),
        ["Okayama", "2026 Cars", "Timing HUD"]
    );
    let mut old = room(3, 8, None);
    old.addons.as_mut().unwrap().list.clear();
    assert_eq!(
        card_addons(&old),
        ["Okayama"],
        "a server that lists no addons shows the tracks"
    );
    old.addons = None;
    assert!(card_addons(&old).is_empty());
}

#[test]
fn marks_each_room_addon_with_whether_the_player_has_it_on() {
    let listed = room(3, 8, None).addons.unwrap().list;
    let on: HashSet<String> = ["a.tracks".to_owned(), "c.hud".to_owned()].into();
    let checks = addon_checks(&listed, &on);
    assert_eq!(
        checks
            .iter()
            .map(|c| (c.name.as_str(), c.have))
            .collect::<Vec<_>>(),
        [("Okayama", true), ("2026 Cars", false), ("Timing HUD", true)]
    );
    assert_eq!(missing(&checks), 1);
}

#[test]
fn lists_each_player_by_what_their_game_is_doing() {
    assert_eq!(
        member_word(&member(true, Some(MemberStage::Starting))).0,
        "Starting the game"
    );
    assert_eq!(
        member_word(&member(true, Some(MemberStage::Loading))).0,
        "Loading"
    );
    assert_eq!(
        member_word(&member(true, Some(MemberStage::InLobby))).0,
        "In the lobby"
    );
    assert_eq!(
        member_word(&member(false, Some(MemberStage::InLobby))).0,
        "Disconnected"
    );
    assert_eq!(member_word(&member(true, None)).0, "Joining\u{2026}");
}

#[test]
fn only_a_game_in_the_lobby_counts_as_joined() {
    let mut s = session(Phase::InLobby, false);
    s.members = vec![
        member(true, Some(MemberStage::InLobby)),
        member(true, Some(MemberStage::Loading)),
        member(true, Some(MemberStage::Starting)),
        member(false, Some(MemberStage::InLobby)),
        member(true, None),
    ];
    assert_eq!(joined(&s), 1);
}

#[test]
fn leaves_the_room_when_the_rooms_game_exits() {
    assert!(exit_leaves(true, false, false));
    assert!(
        !exit_leaves(false, false, false),
        "a game that never ran is not an exit"
    );
    assert!(!exit_leaves(true, true, false), "a running game stays");
    assert!(
        !exit_leaves(true, false, true),
        "a launch or restore in flight stays"
    );
    assert_eq!(exit_notice(false), "F1 25 closed, so you left the room.");
    assert_eq!(exit_notice(true), "F1 25 closed, so your room closed.");
}

#[test]
fn the_servers_refusal_shows_until_a_load_succeeds() {
    let beta = || Some("needs the beta role".to_owned());
    assert_eq!(refusal(None, false, None), None);
    assert_eq!(refusal(None, false, beta()), beta());
    assert_eq!(
        refusal(beta(), false, None),
        beta(),
        "kept while another load runs"
    );
    assert_eq!(
        refusal(beta(), false, Some("not signed in".into())),
        Some("not signed in".into())
    );
    assert_eq!(refusal(beta(), true, None), None);
    assert_eq!(
        refusal_text("needs the beta role"),
        "Online sessions aren't available: needs the beta role"
    );
}

#[test]
fn the_restart_button_says_what_it_does() {
    use swofty_core::online::multiplayer::types::RoomSettings;
    assert_eq!(
        restart_ok_text(&Start::Create(RoomSettings::default())),
        "Create room and join"
    );
    assert_eq!(restart_ok_text(&Start::Join("F1-ABC123".into())), "Join room");
}

#[test]
fn a_member_without_the_f1_2026_content_is_marked_once_the_host_picked_it() {
    let max = member(true, Some(MemberStage::InLobby));
    let mut content = RoomContent::default();
    content.owns_f1_2026.insert("u2".into(), false);
    assert_eq!(
        content_word(&content, &max),
        None,
        "nothing before the host picked the 2026 cars"
    );
    content.host = Some("f1_2026".into());
    assert_eq!(
        content_word(&content, &max),
        Some(("No F1 2026 content", Tone::Blocked))
    );
    content.owns_f1_2026.insert("u2".into(), true);
    assert_eq!(content_word(&content, &max), None);
    content.owns_f1_2026.insert("u2".into(), false);
    assert_eq!(
        content_word(&content, &member(false, None)),
        None,
        "a disconnected member stays Disconnected"
    );
}

#[test]
fn the_cars_row_shows_this_players_f1_2026_answer() {
    let mut s = session(Phase::InLobby, false);
    let mut me = member(true, Some(MemberStage::InLobby));
    me.me = true;
    s.members = vec![me];
    assert_eq!(content_check(&s), None);
    s.content.host = Some("f1_2025".into());
    assert_eq!(content_check(&s), None, "the 2025 cars need no content");
    s.content.host = Some("f1_2026".into());
    assert_eq!(content_check(&s).map(|(owns, _)| owns), Some(None));
    s.content.owns_f1_2026.insert("u2".into(), false);
    assert_eq!(
        content_check(&s),
        Some((Some(false), "Your copy of F1 25 doesn't have the F1 2026 content"))
    );
}

#[test]
fn a_shown_page_polls_the_game_rooms_and_addons_at_once() {
    let now = Instant::now();
    let mut lobby = Lobby {
        game_at: Some(now),
        rooms_at: Some(now),
        print: Some(own("abc", &[])),
        ..Lobby::default()
    };
    assert!(!lobby.game_due(now) && !lobby.rooms_due(now) && !lobby.print_due());
    lobby.refresh();
    assert!(lobby.game_due(now) && lobby.rooms_due(now) && lobby.print_due());
}

#[test]
fn a_poll_already_running_is_not_due_again() {
    let now = Instant::now();
    let lobby = Lobby {
        game_polling: true,
        rooms_polling: true,
        print_busy: true,
        ..Lobby::default()
    };
    assert!(!lobby.game_due(now) && !lobby.rooms_due(now) && !lobby.print_due());
}

#[test]
fn a_game_read_reports_a_change_only_when_the_status_moved() {
    let now = Instant::now();
    let mut lobby = Lobby {
        game_polling: true,
        ..Lobby::default()
    };
    assert!(lobby.took_game(Some(status(Screen::MainMenu)), now));
    assert!(!lobby.game_polling && lobby.game_at == Some(now));
    assert!(!lobby.took_game(Some(status(Screen::MainMenu)), now));
    assert!(lobby.took_game(None, now));
}

#[test]
fn a_failed_room_list_keeps_the_rooms_shown_and_says_why() {
    let now = Instant::now();
    let mut lobby = Lobby::default();
    lobby.took_rooms(Ok(Vec::new()), now);
    assert!(lobby.rooms_loaded && lobby.rooms_error.is_none());
    lobby.rooms_polling = true;
    lobby.took_rooms(Err("offline".into()), now);
    assert!(lobby.rooms_loaded && !lobby.rooms_polling);
    assert_eq!(lobby.rooms_error.as_deref(), Some("offline"));
    lobby.took_rooms(Ok(Vec::new()), now);
    assert!(lobby.rooms_error.is_none());
}

#[test]
fn a_print_that_does_not_read_is_asked_for_again() {
    let mut lobby = Lobby {
        print_busy: true,
        ..Lobby::default()
    };
    lobby.took_print(Err("no game".into()));
    assert!(lobby.print_due());
    lobby.took_print(Ok(own("abc", &[])));
    assert!(!lobby.print_due());
}

#[test]
fn avatars_are_asked_for_once_and_only_from_the_web() {
    let asked: HashSet<String> = ["https://a/1.png".to_owned()].into();
    let mut wanted = unasked(
        vec![
            "https://a/1.png".into(),
            "https://a/2.png".into(),
            "https://a/2.png".into(),
            "file:///c.png".into(),
        ],
        &asked,
    );
    wanted.sort();
    assert_eq!(wanted, ["https://a/2.png"]);
}

#[test]
fn a_start_names_what_the_page_waits_on() {
    assert_eq!(starting(&Start::Create(Default::default())), "create");
    assert_eq!(starting(&Start::Join("ABCD".into())), "ABCD");
}

#[test]
fn a_room_offers_every_size_from_two_to_twenty_four_and_starts_at_the_servers_default() {
    use swofty_core::online::multiplayer::types::RoomSettings;
    let sizes: Vec<u8> = room_sizes().collect();
    assert_eq!(sizes.first(), Some(&2));
    assert_eq!(sizes.last(), Some(&24));
    assert_eq!(sizes.len(), 23);
    let default = RoomSettings::default().max_members;
    assert_eq!(default, 20, "the room server's default size");
    assert!(room_sizes().contains(&default));
}

#[test]
fn the_create_form_names_the_room_after_the_player() {
    assert_eq!(default_room_name(Some("Max")), "Max's room");
    assert_eq!(default_room_name(None), "Your room");
}

#[test]
fn the_ad_banner_gives_way_to_the_create_form() {
    assert!(shows_ad(false));
    assert!(!shows_ad(true), "the form's players menu would open under the ad");
}

fn step(id: &str, state: &str) -> SessionStep {
    serde_json::from_value(serde_json::json!({"id": id, "state": state, "detail": format!("{id} detail")}))
        .unwrap()
}

fn failed_step(id: &str, problem: serde_json::Value) -> SessionStep {
    let mut s = step(id, "failed");
    s.problem = Some(serde_json::from_value(problem).unwrap());
    s
}

fn stepped(phase: Phase, host: bool, steps: Vec<SessionStep>) -> SessionState {
    let mut s = session(phase, host);
    s.steps = steps;
    s
}

fn row<'a>(card: &'a StepCard, id: &str) -> &'a StepRow {
    card.rows.iter().find(|r| r.id == id).unwrap()
}

#[test]
fn a_session_with_steps_keeps_the_room_when_its_game_exits_and_one_without_them_leaves_it() {
    assert!(leaves_on_exit(None));
    assert!(
        leaves_on_exit(Some(&session(Phase::InLobby, false))),
        "a module without steps"
    );
    let failed = failed_step(
        "game",
        serde_json::json!({"code": "game_exited", "text": "F1 25 closed.", "retry": true}),
    );
    let state = stepped(
        Phase::WaitingForGame("game".into()),
        false,
        vec![step("account", "done"), failed, step("network", "pending")],
    );
    assert!(
        !leaves_on_exit(Some(&state)),
        "the game step fails and Retry starts it again"
    );
    let card = StepCard::of(&state, false).unwrap();
    let row = row(&card, "game");
    assert_eq!(row.state, StepState::Failed);
    assert_eq!(
        row.problem.as_ref().map(|p| (p.text.as_str(), p.retry)),
        Some(("F1 25 closed.", true))
    );
}

#[test]
fn a_module_without_steps_keeps_the_phase_and_progress_card() {
    assert_eq!(
        StepCard::of(&session(Phase::Connecting("joining".into()), false), false),
        None
    );
}

#[test]
fn each_step_is_titled_by_its_id_for_the_host_or_a_player() {
    assert_eq!(step_title("account", false), "Log in");
    assert_eq!(step_title("room", true), "Open the room");
    assert_eq!(step_title("room", false), "Join the room");
    assert_eq!(step_title("addons", false), "Match the host's addons");
    assert_eq!(step_title("game", true), "Start F1 25");
    assert_eq!(step_title("lobby", false), "Join the host's LAN lobby");
    assert_eq!(step_title("race", false), "Race");
    assert_eq!(
        step_title("voice_chat", false),
        "Voice chat",
        "a step a newer module adds reads as its id"
    );
}

#[test]
fn the_steps_show_in_order_with_the_active_ones_detail_only() {
    let s = stepped(
        Phase::Connecting("connecting".into()),
        false,
        vec![
            step("account", "done"),
            step("room", "done"),
            step("addons", "done"),
            step("build", "skipped"),
            step("game", "done"),
            step("network", "active"),
            step("lobby", "pending"),
            step("race", "warming_up"),
        ],
    );
    let card = StepCard::of(&s, false).unwrap();
    let ids: Vec<&str> = card.rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "account", "room", "addons", "build", "game", "network", "lobby", "race"
        ]
    );
    assert_eq!(row(&card, "network").state, StepState::Active);
    assert_eq!(row(&card, "network").detail.as_deref(), Some("Network detail"));
    assert_eq!(row(&card, "build").detail.as_deref(), Some("Build detail"));
    assert_eq!(row(&card, "account").detail, None, "a done step is only ticked");
    assert_eq!(row(&card, "lobby").detail, None);
    assert_eq!(
        row(&card, "race").state,
        StepState::Pending,
        "a state this launcher doesn't know"
    );
    assert!(
        card.rows
            .iter()
            .all(|r| !r.dimmed && r.problem.is_none() && r.hint.is_none())
    );
}

#[test]
fn a_failed_step_carries_its_problem_and_dims_the_steps_after_it() {
    let mut s = stepped(
        Phase::Connecting("connecting".into()),
        false,
        vec![
            step("account", "done"),
            failed_step(
                "network",
                serde_json::json!({"code": "member_unreachable", "text": "Max can't be reached.",
                                   "detail": "Their router drops the relay."}),
            ),
            step("lobby", "pending"),
            step("race", "pending"),
        ],
    );
    s.problem = Some(Problem {
        text: "Max can't be reached.".into(),
        retry: true,
        detail: None,
    });
    let card = StepCard::of(&s, false).unwrap();
    let failed = row(&card, "network");
    assert_eq!(failed.state, StepState::Failed);
    let problem = failed.problem.as_ref().unwrap();
    assert_eq!(problem.text, "Max can't be reached.");
    assert_eq!(problem.detail.as_deref(), Some("Their router drops the relay."));
    assert!(problem.retry, "the session's problem says trying again helps");
    assert_eq!(failed.detail.as_deref(), Some("Network detail"));
    assert!(!row(&card, "account").dimmed && !failed.dimmed);
    assert!(row(&card, "lobby").dimmed && row(&card, "race").dimmed);

    s.steps[1] = failed_step(
        "network",
        serde_json::json!({"code": "turn_refused", "text": "The relay refused us.", "retry": false}),
    );
    let card = StepCard::of(&s, false).unwrap();
    assert!(
        !row(&card, "network").problem.as_ref().unwrap().retry,
        "the step's own answer wins"
    );

    s.steps[1] = step("network", "failed");
    s.problem = None;
    let card = StepCard::of(&s, false).unwrap();
    let bare = row(&card, "network").problem.as_ref().unwrap();
    assert!(
        !bare.retry && !bare.text.is_empty(),
        "a failure without a problem still offers Leave"
    );
}

#[test]
fn the_headline_shows_above_the_steps_only_for_what_no_step_says() {
    use super::tracker::{Banner, banner};
    let steps = || vec![step("account", "done"), step("room", "active")];
    let working = stepped(Phase::Connecting("joining".into()), false, steps());
    let card = StepCard::of(&working, false).unwrap();
    assert_eq!(banner(&card, &working, false, "Joining", ""), None);
    assert_eq!(
        banner(&card, &working, true, "Joining", ""),
        Some(Banner::Leaving),
        "leaving"
    );

    let mut stray = working.clone();
    stray.problem = Some(Problem {
        text: "The room server is slow.".into(),
        retry: true,
        detail: None,
    });
    let card = StepCard::of(&stray, false).unwrap();
    assert_eq!(
        banner(&card, &stray, false, "The room server is slow.", ""),
        None,
        "a problem no step carries shows beside the active step"
    );

    let ended = stepped(Phase::Ended("the host closed the room".into()), false, steps());
    let card = StepCard::of(&ended, false).unwrap();
    assert!(card.rows.is_empty());
    assert_eq!(
        banner(&card, &ended, false, "The host closed the room", "Room closed"),
        Some(Banner::Over("The host closed the room".into()))
    );

    let failed = stepped(
        Phase::Failed("the room is full".into()),
        false,
        vec![
            step("account", "done"),
            failed_step(
                "room",
                serde_json::json!({"code": "room_full", "text": "The room is full."}),
            ),
        ],
    );
    let card = StepCard::of(&failed, false).unwrap();
    assert_eq!(
        card.rows.len(),
        2,
        "an ended session still shows where it stopped"
    );
    assert_eq!(
        banner(&card, &failed, false, "The room is full.", "The room didn't open"),
        None
    );
}

#[test]
fn a_failed_steps_detail_that_repeats_its_problem_is_left_out() {
    assert!(repeats("The room closed", "The room closed."));
    assert!(repeats("the room closed.", "The Room Closed"));
    assert!(!repeats("The room closed after being idle", "The room closed."));
    let closed = |detail: &str| {
        let room: SessionStep = serde_json::from_value(serde_json::json!({
            "id": "room", "state": "failed", "detail": detail,
            "problem": {"code": "room_closed", "text": "The room closed.", "retry": false,
                        "detail": "the room closed"},
        }))
        .unwrap();
        stepped(
            Phase::Ended("the room closed".into()),
            true,
            vec![step("account", "done"), room, step("addons", "pending")],
        )
    };
    let card = StepCard::of(&closed("the room closed"), false).unwrap();
    let failed = row(&card, "room");
    assert_eq!(failed.problem.as_ref().unwrap().text, "The room closed.");
    assert_eq!(failed.detail, None, "the detail says what the problem says");
    assert_eq!(failed.problem.as_ref().unwrap().detail, None);
    let card = StepCard::of(&closed("lobby: refused with HTTP 404"), false).unwrap();
    assert_eq!(
        row(&card, "room").detail.as_deref(),
        Some("Lobby: refused with HTTP 404"),
        "a detail that adds something stays"
    );
}

#[test]
fn a_closed_room_says_so_in_place_of_its_live_details() {
    assert_eq!(closed_subtitle(true, true), "Room closed");
    assert_eq!(closed_subtitle(true, false), "Room closed");
    assert_eq!(closed_subtitle(false, true), "The room didn't open");
    assert_eq!(closed_subtitle(false, false), "You didn't join the room");
}

#[test]
fn a_game_still_starting_when_the_room_ends_keeps_starting_without_it() {
    assert_eq!(
        game_after_room(true, true),
        Some("F1 25 keeps starting without the room.")
    );
    assert_eq!(game_after_room(true, false), None);
    assert_eq!(game_after_room(false, true), None, "a live room says nothing");
}

#[test]
fn the_race_step_says_what_to_do_once_everyone_is_in_the_lobby() {
    let steps = || {
        vec![
            step("lobby", "done"),
            serde_json::from_value(serde_json::json!({"id": "race", "state": "pending"})).unwrap(),
        ]
    };
    let host = StepCard::of(&stepped(Phase::InLobby, true, steps()), false).unwrap();
    assert_eq!(row(&host, "race").hint, Some(lobby_hint(true, &Phase::InLobby)));
    assert_eq!(row(&host, "lobby").hint, None);
    let racing = StepCard::of(&stepped(Phase::Racing, true, steps()), false).unwrap();
    assert_eq!(
        row(&racing, "race").hint,
        Some("Mark the race over when everyone is back in the lobby.")
    );
    let player = StepCard::of(&stepped(Phase::InLobby, false, steps()), false).unwrap();
    assert_eq!(
        row(&player, "race").hint,
        Some("The host picks the tracks and starts the race.")
    );
    let leaving = StepCard::of(&stepped(Phase::InLobby, false, steps()), true).unwrap();
    assert_eq!(row(&leaving, "race").hint, None);
    let starting = StepCard::of(&stepped(Phase::Connecting("x".into()), false, steps()), false).unwrap();
    assert_eq!(row(&starting, "race").hint, None);
}

#[test]
fn a_warning_shows_its_problem_and_fix_plan_without_stopping_the_steps_after_it() {
    let mut warned = step("addons", "done");
    warned.warning = true;
    warned.problem = Some(
        serde_json::from_value(serde_json::json!({
            "code": "addons_differ", "text": "Timing HUD doesn't match the host's.", "retry": false,
            "detail": "Timing HUD (t.hud) is 1.0.0; the host has 1.2.0.",
            "fix_plan": [{"action": "install", "id": "t.hud", "name": "Timing HUD", "version": "1.2.0", "sha256": null}],
        }))
        .unwrap(),
    );
    let mut s = stepped(
        Phase::InLobby,
        false,
        vec![
            step("room", "done"),
            warned,
            step("game", "done"),
            step("lobby", "done"),
            serde_json::from_value(serde_json::json!({"id": "race", "state": "pending"})).unwrap(),
        ],
    );
    s.problem = Some(Problem {
        text: "Timing HUD doesn't match the host's.".into(),
        retry: false,
        detail: None,
    });
    let card = StepCard::of(&s, false).unwrap();
    let row = row(&card, "addons");
    assert!(row.warning);
    assert_eq!(row.state, StepState::Done);
    assert_eq!(
        row.problem.as_ref().map(|p| p.text.as_str()),
        Some("Timing HUD doesn't match the host's.")
    );
    assert_eq!(row.fixes, ["Install Timing HUD 1.2.0 from the Addon Browser"]);
    assert_eq!(row.detail.as_deref(), Some("Addons detail"));
    assert!(card.rows.iter().all(|r| !r.dimmed));
    assert_eq!(
        card.rows.last().unwrap().hint,
        Some("The host picks the tracks and starts the race."),
        "the session goes on past a warning"
    );
}

#[test]
fn a_fix_plan_reads_as_one_sentence_per_change() {
    let plan: Vec<swofty_core::online::multiplayer::types::FixStep> =
        serde_json::from_value(serde_json::json!([
            {"action": "install", "id": "t.cars", "name": "2026 Cars", "version": "2.4.1", "sha256": "ab"},
            {"action": "disable", "id": "t.hud", "name": "Telemetry HUD"},
            {"action": "set_option", "addon": "t.ai", "option": "laps", "value": 5},
            {"action": "set_option", "addon": "t.ai", "option": "mode", "value": "full"},
            {"action": "set_option", "addon": "t.ai", "option": "wet", "value": null},
            {"action": "reorder", "order": ["t.cars", "t.ai"]},
            {"action": "manual", "line": "Update F1 25 to the host's version."},
        ]))
        .unwrap();
    assert_eq!(
        fix_lines(&plan),
        [
            "Install 2026 Cars 2.4.1 from the Addon Browser",
            "Turn off Telemetry HUD",
            "Set laps in t.ai to 5",
            "Set mode in t.ai to full",
            "Set wet in t.ai back to its default",
            "Put your addons in the host's load order",
            "Update F1 25 to the host's version.",
        ]
    );
}

#[test]
fn the_multiplayer_page_shows_the_room_until_its_player_steps_out_to_the_list() {
    assert_eq!(room_focus(true, false, None), RoomFocus::InRoom);
    assert_eq!(room_focus(true, false, Some("KX7P2Q")), RoomFocus::InRoom);
    assert_eq!(room_focus(true, true, None), RoomFocus::List);
    assert_eq!(
        room_focus(false, false, Some("KX7P2Q")),
        RoomFocus::Viewing("KX7P2Q".into())
    );
    assert_eq!(
        room_focus(false, true, Some("KX7P2Q")),
        RoomFocus::Viewing("KX7P2Q".into())
    );
    assert_eq!(room_focus(false, false, None), RoomFocus::List);
}

#[test]
fn the_rooms_card_lists_the_first_three_rooms() {
    assert_eq!(card_rooms(0), 0);
    assert_eq!(card_rooms(2), 2);
    assert_eq!(card_rooms(9), 3);
}

mod room_fit {
    use std::collections::HashSet;
    use std::time::{Duration, Instant};

    use swofty_core::catalog::{Addon, Version};
    use swofty_core::online::multiplayer::matching::{Have, Own, Progress};
    use swofty_core::online::multiplayer::types::{
        HostPrint, Phase, RoomAddon, RoomAddons, SessionStep, StepState,
    };

    use super::super::fix::{FixView, RoomFix, SETTLE, patch};
    use super::super::model::{Fit, Mark, OwnPrint, RoomStatus, StepCard, room_fit, room_line, view_addons};
    use super::{room, stepped};

    fn sha(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn version(v: &str, sha256: &str) -> Version {
        serde_json::from_value(serde_json::json!({
            "version": v, "size": 3 << 20, "sha256": sha256, "url": format!("http://localhost/{v}"),
        }))
        .unwrap()
    }

    fn listed(id: &str, name: &str, versions: Vec<Version>) -> Addon {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": name, "author": "a", "versions": versions, "latest": versions.first(),
        }))
        .unwrap()
    }

    fn entry(id: &str, name: &str, v: &str, registry: Option<char>) -> RoomAddon {
        RoomAddon {
            id: id.into(),
            name: name.into(),
            version: Some(v.into()),
            registry: registry.map(sha),
            order: None,
            builtin: false,
        }
    }

    fn hosting(list: Vec<RoomAddon>) -> swofty_core::online::multiplayer::types::RoomSummary {
        let mut r = room(2, 8, Some("sha256:host"));
        r.addons = Some(RoomAddons {
            count: list.len() as u32,
            list,
            launcher: Some("1.10.14".into()),
            game: Some(sha('e')),
            ..RoomAddons::default()
        });
        r
    }

    fn mine(digest: Option<&str>, have: &[(&str, &str, Option<char>)], catalog: Vec<Addon>) -> OwnPrint {
        OwnPrint {
            digest: digest.map(str::to_owned),
            addons: Vec::new(),
            matching: Some(Own {
                launcher: "1.10.14".into(),
                exe_sha256: Some(sha('e')),
                dlc: None,
                have: have
                    .iter()
                    .map(|(id, v, from)| {
                        let h = Have {
                            version: semver::Version::parse(v).unwrap(),
                            from: from.map(sha),
                        };
                        ((*id).to_owned(), h)
                    })
                    .collect(),
                catalog,
            }),
        }
    }

    fn status(room: &swofty_core::online::multiplayer::types::RoomSummary, me: &OwnPrint) -> RoomStatus {
        RoomStatus::of(room, &HashSet::new(), Some(me))
    }

    #[test]
    fn the_same_digest_matches_and_joins() {
        let r = hosting(vec![entry("x.a", "A", "1.0.0", Some('a'))]);
        let me = mine(Some("sha256:host"), &[("x.a", "1.0.0", Some('a'))], Vec::new());
        assert_eq!(status(&r, &me), RoomStatus::Match);
        assert!(status(&r, &me).joinable());
    }

    #[test]
    fn a_profile_that_fails_its_own_dependency_check_still_gets_a_status_and_can_join() {
        let seasons = listed("x.s12", "SWOFTY 2012", vec![version("1.0.6", &sha('b'))]);
        let r = hosting(vec![
            entry("x.pc", "Progressive Career", "1.0.0", Some('a')),
            entry("x.s12", "SWOFTY 2012", "1.0.6", Some('b')),
        ]);
        let me = mine(
            None,
            &[("x.pc", "1.0.0", Some('a')), ("x.s12", "1.0.6", Some('b'))],
            vec![seasons],
        );
        assert_eq!(status(&r, &me), RoomStatus::Fixable);
        assert_eq!(
            room_line(&r, RoomStatus::Fixable, Some(&me)),
            "Joining matches the host's addons"
        );
        assert!(status(&r, &me).joinable());
    }

    #[test]
    fn differences_the_addon_browser_fixes_say_how_many_addons_joining_installs() {
        let catalog = vec![
            listed("x.a", "A", vec![version("1.0.0", &sha('a'))]),
            listed("x.b", "B", vec![version("2.0.0", &sha('b'))]),
            listed(
                "x.dm",
                "Driver Manager",
                vec![version("0.1.4", &sha('d')), version("0.1.3", &sha('c'))],
            ),
        ];
        let r = hosting(vec![
            entry("x.a", "A", "1.0.0", Some('a')),
            entry("x.b", "B", "2.0.0", Some('b')),
            entry("x.dm", "Driver Manager", "0.1.3", Some('c')),
        ]);
        let me = mine(
            Some("sha256:mine"),
            &[("x.dm", "0.1.4", Some('d')), ("x.extra", "1.0.0", None)],
            catalog,
        );
        assert_eq!(status(&r, &me), RoomStatus::Fixable);
        assert_eq!(
            room_line(&r, RoomStatus::Fixable, Some(&me)),
            "Joining installs 3 addons from the Addon Browser"
        );
        let Fit::Joins { installs, replaces } = room_fit(&r, Some(&me)) else {
            panic!("joins");
        };
        assert_eq!(installs, 3);
        assert_eq!(replaces, ["Installs Driver Manager 0.1.3, replacing your 0.1.4"]);
    }

    #[test]
    fn something_the_launcher_cannot_fix_blocks_the_join_and_names_it() {
        let r = hosting(vec![entry("x.private", "Private Pack", "1.0.0", None)]);
        let me = mine(Some("sha256:mine"), &[], Vec::new());
        assert_eq!(status(&r, &me), RoomStatus::Blocked);
        assert!(!status(&r, &me).joinable());
        assert_eq!(
            room_line(&r, RoomStatus::Blocked, Some(&me)),
            "Can't join: Private Pack 1.0.0 isn't on the Addon Browser"
        );
        let mut other_game = hosting(Vec::new());
        other_game.addons.as_mut().unwrap().game = Some(sha('f'));
        other_game.addons.as_mut().unwrap().count = 0;
        other_game.addons.as_mut().unwrap().list = vec![entry("x.a", "A", "1.0.0", Some('a'))];
        let me = mine(Some("sha256:mine"), &[("x.a", "1.0.0", Some('a'))], Vec::new());
        assert_eq!(
            room_line(&other_game, RoomStatus::Blocked, Some(&me)),
            "Can't join: Your F1 25 is another version than the host's"
        );
        let mut other_launcher = hosting(vec![entry("x.a", "A", "1.0.0", Some('a'))]);
        other_launcher.addons.as_mut().unwrap().launcher = Some("1.10.12".into());
        assert_eq!(status(&other_launcher, &me), RoomStatus::Blocked);
    }

    #[test]
    fn a_listing_without_versions_keeps_the_old_status_and_a_full_room_cannot_be_joined() {
        let mut old = hosting(vec![RoomAddon {
            id: "x.a".into(),
            name: "A".into(),
            ..RoomAddon::default()
        }]);
        let me = mine(None, &[], Vec::new());
        assert_eq!(room_fit(&old, Some(&me)), Fit::Unknown);
        assert_eq!(status(&old, &me), RoomStatus::Unknown);
        assert!(status(&old, &me).joinable(), "as before, Join stays");
        old.members = 8;
        assert_eq!(status(&old, &me), RoomStatus::Full);
        assert!(!RoomStatus::Full.joinable());
    }

    fn print(digest: &str) -> HostPrint {
        HostPrint {
            digest: digest.into(),
            ..HostPrint::default()
        }
    }

    #[test]
    fn the_room_launch_waits_for_the_match_and_a_failure_waits_for_retry() {
        let t0 = Instant::now();
        let mut fix = RoomFix::default();
        let host = print("sha256:h1");
        assert!(!fix.holds(None, t0), "an older module sends no host print");
        assert!(!fix.due(None, false));
        assert!(fix.holds(Some(&host), t0));
        assert!(fix.due(Some(&host), false));
        assert!(!fix.due(Some(&host), true), "not once the room's game started");
        let progress = fix.begin(&host.digest);
        assert!(!fix.due(Some(&host), false), "one runs");
        *progress.lock().unwrap() = Progress {
            done: 1,
            total: 3,
            bytes: 5 << 20,
            total_bytes: 12 << 20,
            name: "SWOFTY 2012".into(),
            version: "1.0.6".into(),
            replacing: None,
        };
        let Some(FixView::Running(p)) = fix.view() else {
            panic!("running");
        };
        assert_eq!(p.done, 1);
        fix.finish(Err("the download's SHA-256 is not the listed one".into()), t0);
        assert!(fix.holds(Some(&host), t0 + SETTLE));
        assert!(!fix.due(Some(&host), false), "a failure waits for Retry");
        assert_eq!(
            fix.view(),
            Some(FixView::Failed(
                "the download's SHA-256 is not the listed one".into()
            ))
        );
        assert!(fix.retry());
        assert!(!fix.retry());
        assert!(fix.due(Some(&host), false));
        fix.begin(&host.digest);
        fix.finish(Ok(()), t0);
        assert!(
            fix.holds(Some(&host), t0),
            "the session takes the room profile first"
        );
        assert!(!fix.holds(Some(&host), t0 + SETTLE));
        assert!(
            fix.holds(Some(&print("sha256:h2")), t0 + Duration::from_secs(5)),
            "the host changed"
        );
        assert!(fix.due(Some(&print("sha256:h2")), false));
        assert_eq!(fix.view(), None, "nothing to show once it is done");
    }

    #[test]
    fn the_addons_step_shows_the_match_progress_and_a_failure_with_retry() {
        let steps = vec![
            serde_json::from_value::<SessionStep>(serde_json::json!({"id": "room", "state": "done"}))
                .unwrap(),
            serde_json::from_value::<SessionStep>(serde_json::json!({
                "id": "addons", "state": "done", "warning": true,
                "problem": {"code": "addons_differ", "text": "3 addons differ.", "retry": false}}))
            .unwrap(),
            serde_json::from_value::<SessionStep>(serde_json::json!({"id": "build", "state": "pending"}))
                .unwrap(),
        ];
        let state = stepped(Phase::Connecting("joining".into()), false, steps);
        let progress = Progress {
            done: 1,
            total: 3,
            bytes: 5 << 20,
            total_bytes: 12 << 20,
            name: "Driver Manager".into(),
            version: "0.1.3".into(),
            replacing: Some("0.1.4".into()),
        };
        let mut running = state.clone();
        patch(&mut running, &FixView::Running(progress.clone()));
        let card = StepCard::of(&running, false).unwrap();
        let row = card.rows.iter().find(|r| r.id == "addons").unwrap();
        assert_eq!(row.state, StepState::Active);
        assert!(row.problem.is_none() && !row.warning);
        assert_eq!(
            row.detail.as_deref(),
            Some("Installs Driver Manager 0.1.3, replacing your 0.1.4: 1 of 3 addons, 5 of 12 MB")
        );
        let fresh = Progress {
            replacing: None,
            name: "SWOFTY 2012".into(),
            version: "1.0.6".into(),
            ..progress
        };
        let mut running = state.clone();
        patch(&mut running, &FixView::Running(fresh));
        let card = StepCard::of(&running, false).unwrap();
        let row = card.rows.iter().find(|r| r.id == "addons").unwrap();
        assert_eq!(
            row.detail.as_deref(),
            Some("Installing SWOFTY 2012 1.0.6: 1 of 3 addons, 5 of 12 MB")
        );
        let mut failed = state;
        patch(
            &mut failed,
            &FixView::Failed("SWOFTY 2012 1.0.6: no space left".into()),
        );
        let card = StepCard::of(&failed, false).unwrap();
        let row = card.rows.iter().find(|r| r.id == "addons").unwrap();
        assert_eq!(row.state, StepState::Failed);
        let problem = row.problem.as_ref().unwrap();
        assert!(problem.retry);
        assert_eq!(
            problem.text,
            "Couldn't match the host's addons: SWOFTY 2012 1.0.6: no space left"
        );
        assert!(card.rows.iter().find(|r| r.id == "build").unwrap().dimmed);
    }

    #[test]
    fn the_view_marks_each_host_addon_by_what_joining_does_about_it() {
        let catalog = vec![
            listed("x.s12", "SWOFTY 2012", vec![version("1.0.6", &sha('b'))]),
            listed(
                "x.dm",
                "Driver Manager",
                vec![version("0.1.4", &sha('d')), version("0.1.3", &sha('c'))],
            ),
        ];
        let r = hosting(vec![
            entry("x.pc", "Progressive Career", "1.0.0", Some('a')),
            entry("x.s12", "SWOFTY 2012", "1.0.6", Some('b')),
            entry("x.dm", "Driver Manager", "0.1.3", Some('c')),
        ]);
        let me = mine(
            None,
            &[("x.pc", "1.0.0", Some('a')), ("x.dm", "0.1.4", Some('d'))],
            catalog,
        );
        let view = view_addons(&r, &HashSet::new(), Some(&me));
        assert_eq!(view.summary, "Joining installs 2 addons from the Addon Browser.");
        let rows: Vec<(&str, Mark, &str)> = view
            .rows
            .iter()
            .map(|r| (r.name.as_str(), r.mark, r.note.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Progressive Career", Mark::Have, "You have it"),
                (
                    "SWOFTY 2012",
                    Mark::Installs,
                    "Joining installs 1.0.6 from the Addon Browser"
                ),
                (
                    "Driver Manager",
                    Mark::Installs,
                    "Joining installs 0.1.3, replacing your 0.1.4"
                ),
            ]
        );

        let blocked = hosting(vec![
            entry("x.pc", "Progressive Career", "1.0.0", Some('a')),
            entry("x.private", "Private Pack", "1.0.0", None),
        ]);
        let view = view_addons(&blocked, &HashSet::new(), Some(&me));
        assert_eq!(
            view.summary,
            "Can't join: Private Pack 1.0.0 isn't on the Addon Browser"
        );
        assert_eq!(
            view.rows[0].mark,
            Mark::Have,
            "off in the own profile, on in the room's"
        );
        assert_eq!(view.rows[1].mark, Mark::Lacks);
        assert_eq!(view.rows[1].note, "1.0.0 isn't on the Addon Browser");

        let all = hosting(vec![entry("x.pc", "Progressive Career", "1.0.0", Some('a'))]);
        let view = view_addons(&all, &HashSet::new(), Some(&me));
        assert_eq!(view.summary, "You have every addon the host has.");
    }

    #[test]
    fn a_view_without_versions_checks_the_addons_on_in_the_profile_as_before() {
        let old = hosting(vec![
            RoomAddon {
                id: "x.a".into(),
                name: "A".into(),
                ..RoomAddon::default()
            },
            RoomAddon {
                id: "x.b".into(),
                name: "B".into(),
                ..RoomAddon::default()
            },
        ]);
        let on: HashSet<String> = ["x.a".to_owned()].into();
        let view = view_addons(&old, &on, None);
        assert_eq!(view.summary, "You are missing or have off 1 of 2 addons.");
        let marks: Vec<Mark> = view.rows.iter().map(|r| r.mark).collect();
        assert_eq!(marks, [Mark::Have, Mark::Lacks]);
    }
}
