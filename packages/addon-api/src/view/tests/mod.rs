mod blocks;
mod event;
mod form;
mod selection;
mod table;

use super::*;

#[test]
fn a_row_menu_is_optional_in_json() {
    let plain: Row = serde_json::from_str(r#"{"id":"a","title":"A","subtitle":""}"#).unwrap();
    assert!(plain.menu.is_empty());
    assert!(!serde_json::to_string(&plain).unwrap().contains("menu"));
    let with: Row = serde_json::from_str(
        r#"{"id":"a","title":"A","subtitle":"","menu":[
                {"id":"remove:a","label":"Remove","style":"danger",
                 "kind":{"confirm":{"title":"Remove A?","text":"It is deleted."}}}]}"#,
    )
    .unwrap();
    assert_eq!(with.menu.len(), 1);
    assert_eq!(with.menu[0].style, Style::Danger);
    assert!(matches!(with.menu[0].kind, ButtonKind::Confirm { .. }));
}

#[test]
fn a_text_block_without_a_tone_is_neutral() {
    let view: View = serde_json::from_str(
        r#"{"body":{"blocks":[{"label":"LEGACY CAREER"},
            {"text":{"text":"Start-year tiles are under Settings > Legacy Career."}},
            {"label":"PHYSICS"},
            {"text":{"text":"Racecraft isn't installed or is too old.","tone":"warn"}}]},
            "subtitle":"Everything for the CRL packs in one place"}"#,
    )
    .unwrap();
    let Body::Blocks(blocks) = &view.body else {
        panic!("blocks");
    };
    assert_eq!(
        blocks[1],
        Block::Text {
            text: "Start-year tiles are under Settings > Legacy Career.".into(),
            tone: Tone::Neutral,
        }
    );
    assert!(matches!(blocks[3], Block::Text { tone: Tone::Warn, .. }));
}

#[test]
fn views_round_trip_through_json() {
    let view = View {
        subtitle: "1 in library".into(),
        actions: vec![Button {
            kind: ButtonKind::Pick {
                title: "Import".into(),
                extensions: vec!["serps".into(), "zip".into()],
                folders: true,
                multiple: true,
            },
            icon: Some(Icon::Upload),
            ..Button::new("import", "Import…")
        }],
        notices: vec![Notice {
            text: "Team Simplified is off in this profile.".into(),
            tone: Tone::Warn,
            action: Some(Button::new("enable", "Turn on")),
        }],
        body: Body::List {
            rows: vec![Row {
                id: "rm.alphatauri".into(),
                title: "Scuderia AlphaTauri".into(),
                subtitle: "RM Designs · 1.0".into(),
                art: Art::Monogram("AT".into()),
                tags: vec![Tag {
                    text: "Database".into(),
                    tone: Tone::Muted,
                }],
                trailing: Some(Control::Switch {
                    id: "rm.alphatauri".into(),
                    on: true,
                    enabled: true,
                }),
                detail: vec![Block::Stats(vec![Fact::new("Version", "1.0")])],
                menu: Vec::new(),
            }],
            reorder: true,
            search: None,
        },
        footer: Some("By Team Simplified.".into()),
        tick: false,
    };
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["body"]["list"]["rows"][0]["trailing"]["switch"]["on"], true);
    assert_eq!(json["actions"][0]["icon"], "upload");
    assert_eq!(serde_json::from_value::<View>(json).unwrap(), view);
    let event: Event = serde_json::from_str(r#"{"move":{"row":"a","to":2}}"#).unwrap();
    assert_eq!(
        event,
        Event::Move {
            row: "a".into(),
            to: 2
        }
    );
    assert_eq!(serde_json::to_value(Effect::Reload).unwrap(), "reload");
    assert_eq!(
        serde_json::from_str::<Effect>(r#"{"show_addon":"you.okayama"}"#).unwrap(),
        Effect::ShowAddon("you.okayama".into())
    );
}
