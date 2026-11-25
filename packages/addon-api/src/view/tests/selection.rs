use super::*;
use serde_json::json;

fn dialog() -> SelectionDialog {
    serde_json::from_value(json!({
        "title":"Drivers", "multiple":true,
        "options":[
            {"value":"17","label":"Driver A","group":"Team A","help":"Car 17"},
            {"value":"21","label":"Driver B","group":"Team A"}
        ], "selected":["17"]
    }))
    .unwrap()
}

#[test]
fn selection_buttons_and_events_round_trip_without_changing_old_buttons() {
    let spec = dialog();
    let button = json!({"id":"drivers","label":"Drivers…","kind":{"selection":spec}});
    let parsed: Button = serde_json::from_value(button.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap()["kind"], button["kind"]);
    let event = json!({"selected":{"id":"drivers","values":["17","21"]}});
    assert_eq!(
        serde_json::to_value(serde_json::from_value::<Event>(event.clone()).unwrap()).unwrap(),
        event
    );
    let old: Button = serde_json::from_value(json!({"id":"old","label":"Old"})).unwrap();
    assert_eq!(old.kind, ButtonKind::Event);
    let confirm: Button = serde_json::from_value(
        json!({"id":"delete","label":"Delete","kind":{"confirm":{"title":"Delete?","text":"Warning"}}}),
    )
    .unwrap();
    assert!(matches!(confirm.kind, ButtonKind::Confirm { .. }));
}

#[test]
fn selections_validate_identity_cardinality_and_duplicates() {
    let mut spec = dialog();
    assert!(spec.validate().is_ok());
    assert!(spec.accepts(&spec.selected)); // Confirming defaults must still act.
    assert!(!spec.accepts(&[]));
    spec.allow_empty = true;
    assert!(spec.accepts(&[]));
    assert!(!spec.accepts(&["missing".into()]));
    assert!(!spec.accepts(&["17".into(), "17".into()]));
    spec.multiple = false;
    assert!(!spec.accepts(&["17".into(), "21".into()]));
    spec.selected.push("21".into());
    assert!(spec.validate().is_err());
    spec.multiple = true;
    assert!(spec.validate().is_ok());
    spec.options[1].value = "17".into();
    assert!(spec.validate().is_err());
}

#[test]
fn unknown_or_duplicate_defaults_are_rejected_but_an_unpicked_dialog_can_open() {
    let mut spec = dialog();
    spec.selected = vec!["missing".into()];
    assert!(spec.validate().is_err());
    spec.selected = vec!["17".into(), "17".into()];
    assert!(spec.validate().is_err());
    spec.selected.clear();
    assert!(spec.validate().is_ok());
    assert!(!spec.accepts(&[]));
    spec.options[0].value.clear();
    assert!(spec.validate().is_err());
}

#[test]
fn selections_cap_options_and_text() {
    let mut spec = dialog();
    spec.options = (0..SelectionDialog::MAX_OPTIONS)
        .map(|i| SelectionOption {
            value: i.to_string(),
            label: format!("Driver {i}"),
            group: None,
            help: None,
        })
        .collect();
    spec.selected.clear();
    assert!(spec.validate().is_ok());
    spec.options.push(SelectionOption {
        value: "extra".into(),
        label: "Extra".into(),
        group: None,
        help: None,
    });
    assert!(spec.validate().is_err());
    spec.options.pop();
    spec.options[0].label = "x".repeat(SelectionDialog::MAX_FIELD_CHARS + 1);
    assert!(spec.validate().is_err());
    spec.options[0].label = "x".repeat(SelectionDialog::MAX_FIELD_CHARS);
    assert!(spec.validate().is_ok());
    spec.text = "x".repeat(SelectionDialog::MAX_TEXT_CHARS + 1);
    assert!(spec.validate().is_err());
}
