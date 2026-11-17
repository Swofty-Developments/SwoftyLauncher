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
fn image_blocks_parse_inside_cards_and_default_to_cover() {
    let card: Block = serde_json::from_value(serde_json::json!({ "card": { "blocks": [
            { "image": { "path": "/c/helmet-ab12.png", "width": 240, "height": 135, "caption": "Helmet" } },
            { "image": { "path": "/d/suit.png", "width": 96, "height": 96, "fit": "contain" } }
        ] } }))
    .unwrap();
    let Block::Card(card) = &card else { panic!("card") };
    let Block::Image(helmet) = &card.blocks[0] else {
        panic!("image")
    };
    assert_eq!(
        (
            helmet.path.to_str(),
            helmet.width,
            helmet.height,
            helmet.caption.as_deref(),
            helmet.fit
        ),
        (Some("/c/helmet-ab12.png"), 240., 135., Some("Helmet"), Fit::Cover)
    );
    let Block::Image(suit) = &card.blocks[1] else {
        panic!("image")
    };
    assert_eq!((suit.fit, suit.caption.is_none()), (Fit::Contain, true));
    let json = serde_json::to_value(Block::Card(card.clone())).unwrap();
    assert_eq!(json["card"]["blocks"][1]["image"]["fit"], "contain");
    assert!(json["card"]["blocks"][1]["image"].get("caption").is_none());
}

#[test]
fn dashboard_blocks_parse_from_the_documented_json() {
    let view: View = serde_json::from_value(serde_json::json!({
            "subtitle": "s", "tick": true,
            "body": { "blocks": [
                { "hero": { "title": "Scuderia Swofty", "subtitle": "My Team",
                    "art": { "monogram": "S" },
                    "pills": [ { "text": "LIVE", "tone": "good", "dot": true } ],
                    "facts": [ { "label": "Next race", "value": "Monaco" } ],
                    "buttons": [ { "id": "backup", "label": "Back up now", "icon": "save" } ] } },
                { "tiles": [
                    { "label": "Cash", "value": "$12.4M", "delta": "+$2.0M", "hint": "since Bahrain" },
                    { "label": "RP", "value": "340", "progress": 0.68, "tone": "accent" },
                    { "label": "P", "value": "P4", "delta": "-1" },
                    { "label": "Q", "value": "1", "delta": "1", "delta_tone": "warn" } ] },
                { "grid": { "columns": [2, 1], "blocks": [
                    { "card": { "title": "Facilities", "blocks": [
                        { "meters": [ { "label": "Wind tunnel", "value": 3, "max": 4, "text": "Phase 3 of 4", "tone": "good" } ] } ] } },
                    { "card": { "pills": [ { "text": "Offline", "tone": "muted" } ] } } ] } },
                { "grid": { "blocks": [] } },
                { "pills": [ { "text": "In Team HQ", "tone": "accent", "icon": "car" } ] }
            ] }
        }))
        .unwrap();
    assert!(view.tick);
    let Body::Blocks(blocks) = &view.body else {
        panic!("blocks body");
    };
    let Block::Hero(hero) = &blocks[0] else {
        panic!("hero");
    };
    assert_eq!(hero.art, Art::Monogram("S".into()));
    assert!(hero.pills[0].dot);
    assert_eq!(hero.buttons[0].icon, Some(Icon::Save));
    let Block::Tiles(tiles) = &blocks[1] else {
        panic!("tiles");
    };
    let tones: Vec<Tone> = tiles.iter().map(Tile::delta_tone).collect();
    assert_eq!(tones, [Tone::Good, Tone::Muted, Tone::Bad, Tone::Warn]);
    assert_eq!(tiles[1].progress, Some(0.68));
    let Block::Grid(grid) = &blocks[2] else {
        panic!("grid");
    };
    assert_eq!(grid.columns.weights(), [2., 1.]);
    let Block::Card(card) = &grid.blocks[0] else {
        panic!("card");
    };
    let Block::Meters(meters) = &card.blocks[0] else {
        panic!("meters");
    };
    assert_eq!(meters[0].fraction(), 0.75);
    assert_eq!(meters[0].text(), "Phase 3 of 4");
    let Block::Grid(plain) = &blocks[3] else {
        panic!("grid");
    };
    assert_eq!(plain.columns.weights(), [1., 1.]);
    assert_eq!(Columns::Count(9).weights().len(), 4);
    let again: View = serde_json::from_value(serde_json::to_value(&view).unwrap()).unwrap();
    assert_eq!(again, view);
    let bare = serde_json::to_value(View { tick: false, ..view }).unwrap();
    assert!(bare.get("tick").is_none());
}

#[test]
fn meters_default_to_a_hundred_and_clamp() {
    let m: Meter = serde_json::from_str(r#"{"value": 40}"#).unwrap();
    assert_eq!((m.fraction(), m.text()), (0.4, "40 / 100".to_owned()));
    let over: Meter = serde_json::from_str(r#"{"value": 12, "min": 2, "max": 7}"#).unwrap();
    assert_eq!(over.fraction(), 1.0);
    let flat: Meter = serde_json::from_str(r#"{"value": 1, "min": 5, "max": 5}"#).unwrap();
    assert_eq!(flat.fraction(), 0.0);
}

#[test]
fn forms_parse_every_field_kind() {
    let form: Form = serde_json::from_value(serde_json::json!({
            "id": "finance", "columns": 2, "apply": "Apply",
            "fields": [
                { "number": { "id": "cash", "label": "Cash", "value": 12400000, "min": 0, "max": 2147483647, "step": 100000, "unit": "$" } },
                { "slider": { "id": "rp", "value": 340, "min": 0, "max": 500, "step": 10 } },
                { "text": { "id": "name", "value": "Swofty", "placeholder": "Team", "max_len": 32 } },
                { "select": { "id": "weather", "value": "dry", "options": [ { "value": "dry", "label": "Dry" }, "Light rain", { "heading": "Wet" }, { "value": "storm", "label": "Storm", "image": "/c/storm-1.png" } ] } },
                { "toggle": { "id": "lock", "value": true, "disabled": true } }
            ]
        }))
        .unwrap();
    let ids: Vec<&str> = form.fields.iter().map(Field::id).collect();
    assert_eq!(ids, ["cash", "rp", "name", "weather", "lock"]);
    let values: Vec<serde_json::Value> = form.fields.iter().map(Field::value).collect();
    assert_eq!(
        values,
        [
            serde_json::json!(12400000),
            serde_json::json!(340),
            serde_json::json!("Swofty"),
            serde_json::json!("dry"),
            serde_json::json!(true)
        ]
    );
    assert!(values[0].is_i64());
    assert!(form.fields[4].disabled() && !form.fields[0].disabled());
    let Field::Select(select) = &form.fields[3] else {
        panic!("select");
    };
    assert_eq!(select.options[1].value(), "Light rain");
    assert_eq!(select.options[0].label(), "Dry");
    assert!(select.options[2].is_heading() && !select.options[0].is_heading());
    assert_eq!(
        (select.options[2].label(), select.options[2].value()),
        ("Wet", "")
    );
    assert_eq!(
        select.options[3].image(),
        Some(std::path::Path::new("/c/storm-1.png"))
    );
    assert!(select.options[0].image().is_none() && select.options[2].image().is_none());
    assert!(!form.live && !form.disabled);
    let again: Form = serde_json::from_value(serde_json::to_value(&form).unwrap()).unwrap();
    assert_eq!(again, form);
}

#[test]
fn select_fields_can_ask_for_search() {
    let field: Field = serde_json::from_value(serde_json::json!(
            { "select": { "id": "helmet", "value": "h2", "search": true,
                          "options": [ { "value": "h1", "label": "Classic Red" }, { "value": "h2", "label": "Carbon" } ] } }
        ))
        .unwrap();
    let Field::Select(select) = &field else {
        panic!("select");
    };
    assert!(select.search);
    assert_eq!(field.value(), serde_json::json!("h2"));
    assert_eq!(serde_json::to_value(&field).unwrap()["select"]["search"], true);
    let plain: Field = serde_json::from_value(
        serde_json::json!({ "select": { "id": "garage", "options": ["Front", "Back"] } }),
    )
    .unwrap();
    let Field::Select(select) = &plain else {
        panic!("select");
    };
    assert!(!select.search);
    assert!(
        serde_json::to_value(&plain).unwrap()["select"]
            .get("search")
            .is_none()
    );
}

#[test]
fn number_fields_clamp_round_and_send_integers() {
    let cash = NumberField {
        min: Some(0.),
        max: Some(1000.),
        step: Some(10.),
        ..NumberField::default()
    };
    assert_eq!(cash.clamp(123.), 120.);
    assert_eq!(cash.clamp(-5.), 0.);
    assert_eq!(cash.clamp(5000.), 1000.);
    assert!(cash.json(120.).is_i64());
    let rate = NumberField {
        step: Some(0.05),
        ..NumberField::default()
    };
    assert_eq!(rate.clamp(0.333), 0.35);
    assert_eq!(rate.json(0.35), serde_json::json!(0.35));
    assert_eq!(rate.text(0.3), "0.30");
    assert!(NumberField::default().json(3.).is_i64());
    assert_eq!(NumberField::default().json(2.5), serde_json::json!(2.5));
}

#[test]
fn tables_keep_the_old_shape_and_add_sorting_search_and_edits() {
    let old: Block =
        serde_json::from_str(r#"{"table":{"columns":["File","Op"],"rows":[["a.erp","patch"]]}}"#).unwrap();
    let Block::Table(old_table) = &old else {
        panic!("table");
    };
    assert!(old_table.is_plain());
    assert_eq!(
        *old_table,
        Table::plain(
            vec!["File".into(), "Op".into()],
            vec![vec!["a.erp".into(), "patch".into()]]
        )
    );
    assert_eq!(
        serde_json::to_value(&old).unwrap(),
        serde_json::json!({"table":{"columns":["File","Op"],"rows":[["a.erp","patch"]]}})
    );
    let t: Table = serde_json::from_value(serde_json::json!({
        "id": "drivers",
        "columns": [
            "Driver",
            { "key": "team", "title": "Team" },
            { "key": "pace", "title": "Pace", "align": "right", "width": 96, "sort": true }
        ],
        "rows": [
            { "id": "ver", "cells": [
                "Max Verstappen",
                { "text": "Red Bull", "tone": "muted" },
                { "text": "95", "bar": 0.95, "edit": { "number": { "min": 0, "max": 100 } } } ] },
            ["Lando Norris", "McLaren", "1,093"],
            ["Oscar Piastri", "McLaren", { "text": "$12.4M", "sort": 12400000 }]
        ],
        "sort": { "column": "pace", "desc": true },
        "search": { "placeholder": "Search drivers", "event": true },
        "compact": true,
        "empty": "No drivers match"
    }))
    .unwrap();
    assert!(!t.is_plain());
    assert_eq!(t.id(), "drivers");
    let keys: Vec<&str> = t.columns.iter().map(Column::key).collect();
    assert_eq!(keys, ["Driver", "team", "pace"]);
    assert_eq!(t.columns[2].spec().align, Align::Right);
    assert_eq!(t.columns[2].spec().width, Some(96.));
    assert!(t.columns[2].spec().sort && !t.columns[0].spec().sort);
    assert_eq!(t.rows[0].id(0), "ver");
    assert_eq!(t.rows[1].id(1), "1");
    assert_eq!(t.rows[0].cells()[1].spec().tone, Some(Tone::Muted));
    assert!(matches!(t.rows[0].cells()[2].spec().edit, Some(Field::Number(_))));
    assert_eq!(t.rows[1].cells()[2].sort_key(), SortKey::Number(1093.));
    assert_eq!(t.rows[2].cells()[2].sort_key(), SortKey::Number(12_400_000.));
    assert_eq!(
        t.rows[0].cells()[0].sort_key(),
        SortKey::Text("max verstappen".into())
    );
    assert_eq!(
        SortKey::Number(2.).compare(&SortKey::Number(10.)),
        std::cmp::Ordering::Less
    );
    let search = t.search.clone().unwrap();
    assert!(search.event && search.filter && search.id.is_none());
    assert_eq!(
        t.sort,
        Some(Sort {
            column: "pace".into(),
            desc: true
        })
    );
    let again: Table = serde_json::from_value(serde_json::to_value(&t).unwrap()).unwrap();
    assert_eq!(again, t);
}

#[test]
fn table_rows_take_pictures_switches_spans_and_a_detail_panel() {
    let t: Table = serde_json::from_value(serde_json::json!({
            "id": "drivers",
            "columns": ["On", "Driver", "Nationality"],
            "row_height": 72,
            "rows": [
                { "id": "rai", "subtitle": "kimi_raikkonen",
                  "cells": [
                    { "text": "On", "edit": { "toggle": { "value": true } } },
                    { "text": "Kimi RÄIKKÖNEN", "image": "/cache/kimi.png",
                      "spans": [ { "text": "Kimi " }, { "text": "RÄIKKÖNEN", "bold": true } ] },
                    { "text": "Finnish", "image": { "path": "/cache/flags/FI.png", "width": 20, "height": 14 } } ],
                  "detail": [ { "facts": [ { "label": "Car number", "value": "#7" } ] } ] },
                ["Off", "Nico Rosberg", "German"]
            ]
        }))
        .unwrap();
    assert!(!t.is_plain() && t.has_detail());
    assert_eq!(t.row_height, Some(72.));
    let (kimi, nico) = (&t.rows[0], &t.rows[1]);
    assert!(matches!(
        kimi.cells()[0].spec().edit,
        Some(Field::Toggle(ToggleField { value: true, .. }))
    ));
    let name = kimi.cells()[1].spec();
    assert_eq!(
        name.image.as_ref().unwrap().path(),
        std::path::Path::new("/cache/kimi.png")
    );
    assert_eq!(name.image.as_ref().unwrap().size(64.), (64., 64.));
    assert!(!name.spans[0].bold && name.spans[1].bold);
    assert_eq!(kimi.cells()[2].spec().image.unwrap().size(64.), (20., 14.));
    // The text still searches and sorts; the panel is titled after the first cell that
    // isn't a switch.
    assert_eq!(name.text, "Kimi RÄIKKÖNEN");
    assert_eq!(kimi.title(), "Kimi RÄIKKÖNEN");
    assert_eq!(nico.title(), "Off");
    assert_eq!(kimi.subtitle(), Some("kimi_raikkonen"));
    assert_eq!(kimi.detail().len(), 1);
    assert!(nico.detail().is_empty() && nico.subtitle().is_none());
    let again: Table = serde_json::from_value(serde_json::to_value(&t).unwrap()).unwrap();
    assert_eq!(again, t);
    // Rows without the new keys serialize as before.
    assert_eq!(
        serde_json::to_value(Table::plain(vec!["A".into()], vec![vec!["1".into()]])).unwrap(),
        serde_json::json!({ "columns": ["A"], "rows": [["1"]] })
    );
}

#[test]
fn colour_fields_send_upper_case_hex_and_leave_the_default_hex_out() {
    let form: Form = serde_json::from_value(serde_json::json!({ "id": "livery", "fields": [
            { "colour": { "id": "team", "label": "Team colour", "value": "#e10600" } },
            { "colour": { "id": "accent", "value": "0af", "hex": false } },
            { "colour": { "id": "unset" } }
        ] }))
    .unwrap();
    let values: Vec<serde_json::Value> = form.fields.iter().map(Field::value).collect();
    assert_eq!(
        values,
        [
            serde_json::json!("#E10600"),
            serde_json::json!("#00AAFF"),
            serde_json::Value::Null
        ]
    );
    let Field::Colour(team) = &form.fields[0] else {
        panic!("colour")
    };
    assert!(team.hex && team.rgb() == Some([0xE1, 0x06, 0x00]));
    assert_eq!(form.fields[0].label(), Some("Team colour"));
    let json = serde_json::to_value(&form).unwrap();
    assert!(json["fields"][0]["colour"].get("hex").is_none());
    assert_eq!(json["fields"][1]["colour"]["hex"], false);
    let again: Form = serde_json::from_value(json).unwrap();
    assert_eq!(again, form);
}

#[test]
fn parse_colour_takes_six_or_three_digits_and_refuses_the_rest() {
    assert_eq!(parse_colour("#E8002D"), Some([0xE8, 0x00, 0x2D]));
    assert_eq!(parse_colour(" e8002d "), Some([0xE8, 0x00, 0x2D]));
    assert_eq!(parse_colour("#fA0"), Some([0xFF, 0xAA, 0x00]));
    assert_eq!(parse_colour("abc"), Some([0xAA, 0xBB, 0xCC]));
    for bad in [
        "",
        "#",
        "#12",
        "#1234",
        "#12345",
        "#1234567",
        "#GGGGGG",
        "##123456",
        "+12345",
        "red",
        "#12 456",
        "#E8002Dé",
    ] {
        assert_eq!(parse_colour(bad), None, "{bad}");
    }
    assert_eq!(colour_hex([0x00, 0xa1, 0x9c]), "#00A19C");
}

#[test]
fn colour_cells_and_transparent_pictures_parse() {
    let row: TableRow = serde_json::from_value(serde_json::json!({ "id": "ferrari", "cells": [
            { "text": "Ferrari", "image": { "path": "/cache/logos/ferrari.png", "width": 28, "height": 28,
                                            "fit": "contain", "transparent": true } },
            { "text": "#E8002D", "edit": { "colour": {} } },
            { "text": "#E8002D", "edit": { "colour": { "hex": false } } }
        ] }))
    .unwrap();
    let logo = row.cells()[0].spec().image.unwrap();
    assert_eq!(
        (logo.fit(), logo.transparent(), logo.size(32.)),
        (Fit::Contain, true, (28., 28.))
    );
    let Some(Field::Colour(cell)) = row.cells()[1].spec().edit else {
        panic!("colour cell")
    };
    assert!(cell.hex && cell.value.is_none());
    assert!(matches!(
        row.cells()[2].spec().edit,
        Some(Field::Colour(ColourField { hex: false, .. }))
    ));
    let block: Block = serde_json::from_value(serde_json::json!(
        { "image": { "path": "/c/logo.png", "width": 96, "height": 96, "transparent": true } }
    ))
    .unwrap();
    assert!(matches!(&block, Block::Image(i) if i.transparent));
    let again: TableRow = serde_json::from_value(serde_json::to_value(&row).unwrap()).unwrap();
    assert_eq!(again, row);
}

#[test]
fn pictures_without_the_new_keys_serialize_as_before() {
    let sized = serde_json::json!({ "path": "/cache/flags/FI.png", "width": 20.0, "height": 14.0 });
    let image: CellImage = serde_json::from_value(sized.clone()).unwrap();
    assert_eq!((image.fit(), image.transparent()), (Fit::Cover, false));
    assert_eq!(serde_json::to_value(&image).unwrap(), sized);
    let plain: CellImage = serde_json::from_value(serde_json::json!("/cache/kimi.png")).unwrap();
    assert!(!plain.transparent() && plain.fit() == Fit::Cover);
    let block = serde_json::json!({ "image": { "path": "/c/helmet.png", "width": 240.0, "height": 135.0, "fit": "cover" } });
    let parsed: Block = serde_json::from_value(block.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), block);
}

#[test]
fn search_matches_every_word_ignoring_case() {
    let row = ["Max Verstappen", "Red Bull Racing"];
    assert!(Search::matches("max", row));
    assert!(Search::matches("RED max", row));
    assert!(!Search::matches("max ferrari", row));
    assert!(Search::matches("  ", row));
    let list: Body =
        serde_json::from_str(r#"{"list":{"rows":[],"reorder":false,"search":{"filter":false}}}"#).unwrap();
    assert!(matches!(
        list,
        Body::List {
            search: Some(Search { filter: false, .. }),
            ..
        }
    ));
    let plain: Body = serde_json::from_str(r#"{"list":{"rows":[],"reorder":false}}"#).unwrap();
    assert!(!serde_json::to_string(&plain).unwrap().contains("search"));
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
