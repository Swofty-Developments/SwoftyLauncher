use super::*;

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
