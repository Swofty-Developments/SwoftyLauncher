//! JSON-lines helper for selection dialogs. This sample deletes no files.
use std::io::{BufRead, Write};

use serde_json::{Value, json};
use swofty_addon::helper::VIEW_SELECTION_DIALOG;
use swofty_addon::view::{Button, ButtonKind, Event, SelectionDialog, SelectionOption, Style, View};

fn choice(value: &str, label: &str, group: Option<&str>) -> SelectionOption {
    SelectionOption {
        value: value.into(),
        label: label.into(),
        group: group.map(str::to_owned),
        help: None,
    }
}

fn actions(drivers: &[String]) -> Vec<Button> {
    [
        (
            "delete",
            "Delete recordings…",
            Style::Danger,
            SelectionDialog {
                title: "Delete recordings?".into(),
                text:
                    "Choose recordings to delete. This sample reports the selection and never deletes files."
                        .into(),
                options: vec![
                    choice("q1", "Qualifying 1", None),
                    choice("q2", "Qualifying 2", None),
                    choice("q3", "Qualifying 3", None),
                ],
                multiple: true,
                apply: Some("Delete selected".into()),
                ..Default::default()
            },
        ),
        (
            "drivers",
            "Drivers…",
            Style::Outline,
            SelectionDialog {
                title: "Drivers".into(),
                options: vec![
                    choice("ada", "Ada", Some("Analytical")),
                    choice("alan", "Alan", Some("Analytical")),
                    choice("grace", "Grace", Some("Cobol")),
                ],
                selected: drivers.to_vec(),
                multiple: true,
                allow_empty: true,
                ..Default::default()
            },
        ),
        (
            "export",
            "Export…",
            Style::Outline,
            SelectionDialog {
                title: "Export recording".into(),
                options: vec![
                    choice("raw", "Raw CSV data", None),
                    choice("visuals", "Visuals", None),
                    choice("both", "Data and visuals", None),
                ],
                selected: vec!["raw".into()],
                apply: Some("Export".into()),
                ..Default::default()
            },
        ),
    ]
    .into_iter()
    .map(|(id, label, style, spec)| {
        let mut b = Button::new(id, label);
        b.style = style;
        b.kind = ButtonKind::Selection(spec);
        b
    })
    .collect()
}

fn main() {
    let mut supported = false;
    let mut drivers = vec!["ada".into(), "alan".into()];
    let mut output = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let Ok(call) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = call.get("id") else { continue };
        let result = match call["method"].as_str() {
            Some("hello") => {
                supported = call["params"]["capabilities"]
                    .as_array()
                    .is_some_and(|caps| caps.iter().any(|c| c == VIEW_SELECTION_DIALOG));
                Ok(json!({"api":1}))
            }
            Some("view") => {
                let view = json!({"subtitle":"Selection dialogs", "actions":if supported { serde_json::to_value(actions(&drivers)).unwrap() } else { json!([]) }, "body":{"blocks":[{"text":{"text":if supported { "Choose recordings, drivers or export contents in a popup." } else { "This launcher has no view.selection_dialog capability. Keep using page controls on older launchers." }}}]}});
                serde_json::from_value::<View>(view.clone()).expect("valid example view");
                Ok(view)
            }
            Some("event") => match serde_json::from_value::<Event>(call["params"]["event"].clone()) {
                Ok(Event::Selected { id, values }) => {
                    let action = actions(&drivers).into_iter().find(|b| b.id == id);
                    match action.map(|b| b.kind) {
                        Some(ButtonKind::Selection(spec)) if spec.accepts(&values) => {
                            if id == "drivers" {
                                drivers.clone_from(&values);
                            }
                            Ok(json!({"notice":format!("{id}: {}", values.join(", "))}))
                        }
                        _ => Err("The selection is no longer available".to_owned()),
                    }
                }
                Ok(_) => Ok(json!("none")),
                Err(e) => Err(e.to_string()),
            },
            _ => Ok(Value::Null),
        };
        let reply = match result {
            Ok(result) => json!({"id":id,"result":result}),
            Err(error) => json!({"id":id,"error":error}),
        };
        if writeln!(output, "{reply}").and_then(|()| output.flush()).is_err() {
            break;
        }
    }
}
