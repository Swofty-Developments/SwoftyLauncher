use super::dto::{TrackDto, aids_of, input_of, setup_of};
use super::*;

#[test]
fn parses_the_overview() {
    let dto: OverviewDto = serde_json::from_str(
        r#"{"seasons":[{"id":"a/f1_2022","name":"F1 2022","cover":"https://x/icon","counts":{"tracks":22,"drivers":17,"laps":410},
          "top":[{"rank":1,"user":{"id":"1","name":"swofty","avatar":null},"score":1834.21}]}],
          "global":{"total":1,"rows":[{"rank":1,"user":{"id":"1","name":"swofty","avatar":null},"score":187.4,"boards":31,"wins":12,"podiums":20}]},
          "custom":{"tracks":[{"track":"a.h:hockenheim","name":"Hockenheimring","country":"de","layout":null,"boards":2,"drivers":4,"laps":9,
            "record":{"time_ms":58815,"user":{"id":"2","name":"d2","avatar":null},"content":"a/f1_2022","weather":"dry"}}]}}"#,
    )
    .unwrap();
    let card = season_card("https://s", &dto.seasons[0]);
    assert_eq!((card.tracks, card.drivers, card.top.len()), (22, 17, 1));
    let track = track_card(
        "https://s",
        "",
        dto.custom.tracks.into_iter().next().unwrap(),
        true,
    );
    assert_eq!(track.key.content, "a/f1_2022");
    assert_eq!(track.record.unwrap().time_ms, 58815);
}

#[test]
fn the_overview_and_a_profile_leave_out_unlisted_seasons() {
    let dto: OverviewDto = serde_json::from_str(
        r#"{"seasons":[{"id":"game/f1_2025","name":"F1 25 (stock)"},{"id":"swofty.classic-seasons-2020/f1_2020","name":"F1 2020 GP/TT Addon"},{"id":"swofty.swofty-seasons-2019/sw_2019","name":"SWOFTY 2019"}],
          "custom":{"tracks":[{"track":"mod.nycgp:nycgp","record":{"time_ms":1,"user":{"id":"2","name":"d2"},"content":"swofty.classic-seasons-2021/f1_2021","weather":"dry"}},
            {"track":"mod.lbcgp:lbcgp","record":{"time_ms":1,"user":{"id":"2","name":"d2"},"content":"game/f1_2025","weather":"dry"}}]}}"#,
    )
    .unwrap();
    let overview = overview_of("https://s", dto);
    assert_eq!(
        overview.seasons.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["game/f1_2025", "swofty.swofty-seasons-2019/sw_2019"]
    );
    assert_eq!(
        overview
            .custom
            .iter()
            .map(|t| t.key.track.as_str())
            .collect::<Vec<_>>(),
        ["mod.lbcgp:lbcgp"]
    );
    let dto: ProfileDto = serde_json::from_str(
        r#"{"user":{"id":"1","name":"d"},
          "seasons":[{"id":"swofty.swofty-seasons-2021/sw_2021","name":"SWOFTY 2021","rank":1,"score":40},{"id":"bydanix.f1-2012-season/ex_f1_2012","name":"F1 2012","rank":2,"score":9}],
          "bests":[{"content":"msyeett.classic-seasons-2015/f1_2015","track":"monza","weather":"dry","lap_id":"a","time_ms":1},{"content":"game/f1_2025","track":"monza","weather":"dry","lap_id":"b","time_ms":2}],
          "recent":[{"content":"swofty.classic-seasons-2017/f1_2017","lap_id":"c","time_ms":3},{"content":"swofty.swofty-seasons-2015/sw_2015","lap_id":"d","time_ms":4}]}"#,
    )
    .unwrap();
    let profile = profile_of(dto);
    assert_eq!(
        profile
            .seasons
            .iter()
            .map(|s| s.season.as_str())
            .collect::<Vec<_>>(),
        ["SWOFTY 2021"]
    );
    assert_eq!(
        profile
            .bests
            .iter()
            .map(|b| b.lap_id.as_str())
            .collect::<Vec<_>>(),
        ["b"]
    );
    assert_eq!(
        profile
            .recent
            .iter()
            .map(|b| b.lap_id.as_str())
            .collect::<Vec<_>>(),
        ["d"]
    );
}

#[test]
fn a_tracks_addon_is_read_and_a_stock_track_in_a_season_box_has_none() {
    let custom: TrackDto = serde_json::from_str(
        r#"{"track":"twilight.bahrain-reverse:sakhir_reverse","name":"Bahrain Reverse","addon":"twilight.bahrain-reverse","country":"bh","laps":3}"#,
    )
    .unwrap();
    let card = track_card("https://s", "game/f1_2025", custom, true);
    assert_eq!(card.addon.as_deref(), Some("twilight.bahrain-reverse"));
    let listed: TrackDto = serde_json::from_str(
        r#"{"track":"mod.lisgp:lisgp","name":"Portuguese Grand Prix","track_addon":"mod.lisgp"}"#,
    )
    .unwrap();
    assert_eq!(
        track_card("https://s", "", listed, true).addon.as_deref(),
        Some("mod.lisgp")
    );
    let stock: TrackDto =
        serde_json::from_str(r#"{"track":"bahrain","name":"Bahrain International Circuit"}"#).unwrap();
    let season = track_card("https://s", "swofty.swofty-seasons-2015/sw_2015", stock, false);
    assert_eq!(season.addon, None);
    let board: BoardDto = serde_json::from_str(
        r#"{"content":"swofty.swofty-seasons-2015/sw_2015","track":"mod.lemans:le_mans_2017","weather":"dry","name":"SWOFTY 2015","track_name":"Le Mans","custom":true,"track_addon":"mod.lemans"}"#,
    )
    .unwrap();
    assert_eq!(board.track_addon.as_deref(), Some("mod.lemans"));
    assert_eq!(
        TrackCard::bare(
            BoardKey {
                content: "swofty.swofty-seasons-2015/sw_2015".into(),
                track: "monaco".into(),
                weather: "dry".into(),
            },
            "Monaco".into(),
        )
        .addon,
        None
    );
}

#[test]
fn relative_art_is_pointed_at_the_server() {
    assert_eq!(
        absolute("https://s", Some("/v1/x.svg".into())).as_deref(),
        Some("https://s/v1/x.svg")
    );
    assert_eq!(
        absolute("https://s", Some("https://c/a.png".into())).as_deref(),
        Some("https://c/a.png")
    );
    assert_eq!(absolute("https://s", None), None);
    assert_eq!(absolute("https://s", Some(String::new())), None);
}

#[test]
fn encodes_the_slash_and_colon() {
    assert_eq!(encode("a.b/c_d"), "a.b%2Fc_d");
    assert_eq!(encode("addon:track"), "addon%3Atrack");
}

#[test]
fn an_unreachable_server_is_a_network_error() {
    let e = HttpBoards::with_server("http://127.0.0.1:1")
        .overview(&Category::default())
        .unwrap_err();
    assert_eq!(e.status, 0);
    assert!(e.message.contains("couldn't reach"));
}

#[test]
fn the_local_servers_real_board_and_empty_overview_parse() {
    let board: BoardDto = serde_json::from_str(r##"{"content": "swofty.swofty-seasons-2016/sw_2016", "track": "suzuka", "weather": "dry", "name": "swofty.swofty-seasons-2016/sw_2016", "track_name": "Suzuka Circuit", "country": "jp", "custom": false, "track_addon": null, "key": "swofty.swofty-seasons-2016/sw_2016|suzuka|dry", "layout": null, "filters": {"assists": null, "device": null, "ranked": false}, "devices": [], "total": 1, "limit": 50, "offset": 0, "record": null, "rows": [{"rank": 1, "lap_id": "11c96143c8aa631d", "user": {"id": "42", "name": "Swofty", "avatar": null}, "time_ms": 88106, "sectors_ms": [31583, 38881, 17641], "car": {"team": "sw16_ferrari"}, "assists": {"brk": 0, "abs": 1, "trc": 2, "drl": 2, "gbx": 3, "ers": 0, "input": 3}, "set_at": "2026-11-10T07:09:29Z", "status": "unranked", "gap_ms": 0, "gap_pct": 0.0, "car_label": "sw16_ferrari", "input": null, "verification": "unverified", "has_replay": true}], "history": [{"lap_id": "421f3a7eece3a234", "time_ms": 88117, "set_at": "2026-11-10T06:19:01Z", "user": {"id": "42", "name": "Swofty", "avatar": null}, "improved_ms": null}, {"lap_id": "11c96143c8aa631d", "time_ms": 88106, "set_at": "2026-11-10T07:09:29Z", "user": {"id": "42", "name": "Swofty", "avatar": null}, "improved_ms": 11}]}"##).unwrap();
    assert_eq!(board.rows.len(), 1);
    assert_eq!(board.rows[0].time_ms, 88106);
    assert_eq!(row(board.rows.into_iter().next().unwrap()).rank, 1);
    let overview: OverviewDto = serde_json::from_str(r##"{"generated": "2026-11-10T08:11:54Z", "scoring": {"min_field": 3, "pace_scale": 0.015, "pace_share": 0.7, "full_field": 10}, "seasons": [], "global": {"total": 0, "rows": []}, "custom": {"tracks": []}}"##).unwrap();
    assert!(overview.seasons.is_empty() && overview.global.rows.is_empty());
}

#[test]
fn a_setup_object_keeps_known_numbers_and_the_compound() {
    let v: serde_json::Value = serde_json::from_str(
        r#"{"front_wing":25,"brake_bias":56,"tyre_pressure_fl":22.5,"tyre_compound":"soft","changed":true,"mystery":1}"#,
    )
    .unwrap();
    let s = setup_of(&v).unwrap();
    assert_eq!(s.get("front_wing"), Some(25.));
    assert_eq!(s.get("tyre_pressure_fl"), Some(22.5));
    assert_eq!(s.compound.as_deref(), Some("soft"));
    assert!(s.changed);
    assert!(setup_of(&serde_json::json!({})).is_none());
    assert!(setup_of(&serde_json::Value::Null).is_none());
}

#[test]
fn a_ranking_page_and_me_parse() {
    let dto: RankingDto = serde_json::from_str(
        r#"{"season":"all","name":"All","total":312,"limit":50,"offset":0,"next_offset":50,
           "rows":[{"rank":1,"user":{"id":"1","name":"swofty","avatar":null},"score":187.4,"boards":31,"wins":12,"podiums":20}]}"#,
    )
    .unwrap();
    assert_eq!((dto.total, dto.next_offset, dto.rows.len()), (312, Some(50), 1));
    let me: MeDto = serde_json::from_str(
        r#"{"id":"1","name":"swofty","avatar":null,"global":{"rank":3,"of":40,"score":187.4,"boards":31,"wins":12,"podiums":20}}"#,
    )
    .unwrap();
    assert_eq!(me.global.unwrap().rank, 3);
    let last: RankingDto = serde_json::from_str(r#"{"total":3,"next_offset":null,"rows":[]}"#).unwrap();
    assert_eq!(last.next_offset, None);
}

#[test]
fn a_rows_assists_and_input_are_read_from_the_server_shape() {
    let v: serde_json::Value =
        serde_json::from_str(r#"{"trc":2,"abs":1,"gbx":0,"mystery":5,"input_device":"wheel"}"#).unwrap();
    let aids = aids_of(Some(&v)).unwrap();
    assert_eq!(aids.level(Aid::TractionControl), Some(2));
    assert_eq!(aids.level(Aid::Gearbox), Some(0));
    assert_eq!(input_of(None, Some(&v)), Some(InputDevice::Wheel));
    let direct = serde_json::json!("pad");
    assert_eq!(input_of(Some(&direct), None), Some(InputDevice::Controller));
    assert!(aids_of(Some(&serde_json::json!({}))).is_none());
}
