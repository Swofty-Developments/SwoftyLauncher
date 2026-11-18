use super::*;

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
