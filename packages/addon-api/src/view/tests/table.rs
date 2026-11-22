use super::*;

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
fn memory_cell_pictures_round_trip_beside_the_text_fallback() {
    let value = serde_json::json!({
        "text": "Ferrari", "image_data": {
            "png": "aW1hZ2U=", "width": 28.0, "height": 20.0,
            "fit": "contain", "transparent": true
        }
    });
    let cell: Cell = serde_json::from_value(value.clone()).unwrap();
    let spec = cell.spec();
    assert_eq!(spec.text, "Ferrari");
    let data = spec.image_data.unwrap();
    assert_eq!(data.size(32.), (28., 20.));
    assert_eq!(data.fit, Fit::Contain);
    assert!(data.transparent);
    assert_eq!(serde_json::to_value(cell).unwrap(), value);
    let plain: Cell = serde_json::from_value(serde_json::json!({"text":"Ferrari"})).unwrap();
    assert!(plain.spec().image_data.is_none());
    assert!(serde_json::to_value(plain).unwrap().get("image_data").is_none());
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
