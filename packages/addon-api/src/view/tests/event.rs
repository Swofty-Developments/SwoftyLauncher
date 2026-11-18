use super::*;

#[test]
fn show_addon_takes_an_id_or_an_id_with_a_notice() {
    let old: Effect = serde_json::from_str(r#"{"show_addon":"you.okayama"}"#).unwrap();
    let Effect::ShowAddon(shown) = &old else {
        panic!("show_addon")
    };
    assert_eq!((shown.id(), shown.notice()), ("you.okayama", None));
    assert_eq!(
        serde_json::to_value(&old).unwrap(),
        serde_json::json!({ "show_addon": "you.okayama" })
    );
    let new: Effect =
        serde_json::from_str(r#"{"show_addon":{"id":"you.okayama","notice":"Imported Okayama"}}"#).unwrap();
    assert_eq!(
        new,
        Effect::ShowAddon(Shown::new("you.okayama", "Imported Okayama"))
    );
    let Effect::ShowAddon(shown) = &new else {
        panic!("show_addon")
    };
    assert_eq!(
        (shown.id(), shown.notice()),
        ("you.okayama", Some("Imported Okayama"))
    );
    assert_eq!(
        serde_json::from_value::<Effect>(serde_json::to_value(&new).unwrap()).unwrap(),
        new
    );
    let bare: Effect = serde_json::from_str(r#"{"show_addon":{"id":"you.okayama"}}"#).unwrap();
    let Effect::ShowAddon(shown) = &bare else {
        panic!("show_addon")
    };
    assert_eq!((shown.id(), shown.notice()), ("you.okayama", None));
}

#[test]
fn tabs_block_and_tab_event_match_the_documented_json() {
    let b: Block = serde_json::from_value(serde_json::json!({ "tabs": { "id": "page",
            "items": [ { "id": "overview", "label": "Overview", "icon": "gauge" },
                       { "id": "team", "label": "Team", "badge": "2", "disabled": true } ],
            "active": "overview" } }))
    .unwrap();
    let Block::Tabs(t) = &b else { panic!("tabs") };
    assert_eq!(t.active.as_deref(), Some("overview"));
    assert_eq!(
        (t.items[1].badge.as_deref(), t.items[1].disabled, t.items[0].icon),
        (Some("2"), true, Some(Icon::Gauge))
    );
    assert_eq!(
        serde_json::to_value(Event::Tab {
            id: "page".into(),
            tab: "team".into()
        })
        .unwrap(),
        serde_json::json!({ "tab": { "id": "page", "tab": "team" } })
    );
}

#[test]
fn submit_and_search_events_match_the_documented_json() {
    let submit: Event = serde_json::from_value(serde_json::json!({ "submit": {
            "form": "finance", "values": { "cash": 15000000, "lock": false }, "changed": ["cash"] } }))
    .unwrap();
    let Event::Submit {
        form,
        row,
        values,
        changed,
    } = &submit
    else {
        panic!("submit");
    };
    assert_eq!(
        (form.as_str(), row, changed.as_slice()),
        ("finance", &None, &["cash".to_owned()][..])
    );
    assert_eq!(values["cash"], 15000000);
    assert!(!serde_json::to_string(&submit).unwrap().contains("row"));
    let cell: Event = serde_json::from_str(
        r#"{"submit":{"form":"drivers","row":"ver","values":{"pace":96},"changed":["pace"]}}"#,
    )
    .unwrap();
    assert!(matches!(cell, Event::Submit { row: Some(ref r), .. } if r == "ver"));
    assert_eq!(
        serde_json::to_value(Event::Search {
            id: "drivers".into(),
            query: "ver".into()
        })
        .unwrap(),
        serde_json::json!({ "search": { "id": "drivers", "query": "ver" } })
    );
    let icons: Vec<Icon> =
        serde_json::from_str(r#"["refresh","undo","cloud-rain","trending-up","triangle-alert"]"#).unwrap();
    assert_eq!(
        icons,
        [
            Icon::Refresh,
            Icon::Undo,
            Icon::CloudRain,
            Icon::TrendingUp,
            Icon::TriangleAlert
        ]
    );
}
