use std::collections::HashSet;
use std::time::Instant;

use swofty_core::online::multiplayer::types::{
    GameStatus, MemberStage, MemberState, Phase, PrintMatch, Problem, RelayStatus, Role, RoomAddon,
    RoomAddons, RoomSummary, Screen, SessionState, User,
};

use super::model::starting;
use super::model::{
    GameState, OwnPrint, RoomStatus, Start, Tone, addon_checks, card_addons, content_check, content_word,
    exit_leaves, exit_notice, extra_addons, joined, member_word, missing, refusal, refusal_text,
    restart_ok_text, room_meta,
};
use super::state::{Lobby, unasked};
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
        digest: digest.into(),
        addons: addons.iter().map(|(id, name)| addon(id, name)).collect(),
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
