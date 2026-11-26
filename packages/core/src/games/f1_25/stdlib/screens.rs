use std::path::Path;

use serde_json::Value;
use swofty_addon::manifest::ScreenOp;
use swofty_addon::{Error, Result};

use super::data;
use crate::manifest::AddonManifest;

pub const REGISTRY: &str = "ui.screen";

pub const FLOWS: &[&str] = &[
    "flow_frontend",
    "flow_options",
    "flow_race",
    "flow_career_hub",
    "flow_f1_world",
];

pub fn lower(dir: &Path, m: &mut AddonManifest) -> Result<()> {
    let files = data::files(dir);
    if files.is_empty() {
        return Ok(());
    }
    let mut added = 0;
    for rel in files {
        let path = dir.join(&rel);
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(Value::Object(doc)) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(Value::Object(records)) = doc.get(REGISTRY) else {
            continue;
        };
        for (key, value) in records {
            if key.contains('/') || key.starts_with('$') {
                continue;
            }
            let op = screen_of(&rel, key, value)?;
            if let Some(other) = m.operations.screens.iter().find(|s| s.name == op.name) {
                return Err(Error::Manifest(format!(
                    "{rel}: ui.screen {key} clashes with the screen {} the addon already declares",
                    other.name
                )));
            }
            m.operations.screens.push(op);
            added += 1;
        }
    }
    if added > 0 {
        m.validate()?;
    }
    Ok(())
}

fn screen_of(rel: &str, key: &str, value: &Value) -> Result<ScreenOp> {
    let bad = |why: String| Error::Manifest(format!("{rel}: ui.screen {key}: {why}"));
    if value.get("extends").is_some() {
        return Err(bad("extends is not applied to screens yet".into()));
    }
    let file = match value.get("file") {
        Some(Value::String(f)) if !f.is_empty() => f.clone(),
        Some(Value::String(_)) | Some(Value::Null) | None => return Err(bad("needs a file".into())),
        Some(_) => {
            return Err(bad(
                "file must be a path in the addon's folder; stock and archive paths are not supported".into(),
            ));
        }
    };
    let flow = match value.get("flow") {
        Some(Value::String(f)) => f.clone(),
        _ => return Err(bad("needs a flow".into())),
    };
    if !FLOWS.contains(&flow.as_str()) {
        return Err(bad(format!(
            "flow {flow} is not a flow archive the game has; known: {}",
            FLOWS.join(", ")
        )));
    }
    let anchor = match value.get("anchor") {
        None | Some(Value::Null) => None,
        Some(Value::String(a)) => Some(a.clone()),
        Some(_) => return Err(bad("anchor must be a string".into())),
    };
    let view_model = match value.get("view_model") {
        None | Some(Value::Null) => false,
        Some(Value::String(p)) => !p.is_empty(),
        Some(_) => return Err(bad("view_model must be a path string".into())),
    };
    Ok(ScreenOp {
        name: key.to_owned(),
        file,
        page: None,
        flow,
        anchor,
        view_model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addon(tag: &str, files: &[(&str, &str)], screens: &str) -> (std::path::PathBuf, AddonManifest) {
        let dir = std::env::temp_dir().join(format!("swofty-uiscreen-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        for (rel, text) in files {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let m = AddonManifest::parse(&format!(
            r#"{{ "schema_version": 1, "id": "t.screens", "name": "S", "version": "1.0.0", "author": "a",
                 "compat": {{ "game": "f1_25" }}, "operations": {{ "screens": [{screens}] }} }}"#
        ))
        .unwrap();
        (dir, m)
    }

    #[test]
    fn a_record_becomes_the_screen_the_manifest_would_declare() {
        let (dir, mut m) = addon(
            "lowered",
            &[(
                "registry/ui.json",
                r#"{ "ui.screen": { "settings": { "file": "ui/settings.ui", "flow": "flow_frontend",
                    "anchor": "addon_settings", "view_model": "mod.settings" },
                    "plain": { "file": "ui/plain.ui", "flow": "flow_options" },
                    "other/layer": { "flow": "flow_race" } } }"#,
            )],
            "",
        );
        lower(&dir, &mut m).unwrap();
        let s = &m.operations.screens;
        assert_eq!(s.len(), 2);
        let first = s.iter().find(|s| s.name == "settings").unwrap();
        assert_eq!(
            first,
            &ScreenOp {
                name: "settings".into(),
                file: "ui/settings.ui".into(),
                page: None,
                flow: "flow_frontend".into(),
                anchor: Some("addon_settings".into()),
                view_model: true,
            }
        );
        let plain = s.iter().find(|s| s.name == "plain").unwrap();
        assert!(!plain.view_model && plain.anchor.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_record_that_clashes_with_a_manifest_screen_is_refused() {
        let (dir, mut m) = addon(
            "clash",
            &[(
                "registry/ui.json",
                r#"{ "ui.screen": { "page": { "file": "ui/a.ui", "flow": "flow_frontend" } } }"#,
            )],
            r#"{ "name": "page", "file": "ui/p.ui" }"#,
        );
        let e = lower(&dir, &mut m).unwrap_err().to_string();
        assert!(e.contains("clashes") && e.contains("page"), "{e}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_bad_flow_is_refused_naming_the_known_flows() {
        let (dir, mut m) = addon(
            "flow",
            &[(
                "registry/ui.json",
                r#"{ "ui.screen": { "x": { "file": "ui/a.ui", "flow": "flow_nothing" } } }"#,
            )],
            "",
        );
        let e = lower(&dir, &mut m).unwrap_err().to_string();
        assert!(e.contains("flow_nothing") && e.contains("flow_frontend"), "{e}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_path_that_leaves_the_addon_folder_is_refused() {
        let (dir, mut m) = addon(
            "path",
            &[(
                "registry/ui.json",
                r#"{ "ui.screen": { "x": { "file": "../a.ui", "flow": "flow_frontend" } } }"#,
            )],
            "",
        );
        assert!(lower(&dir, &mut m).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn screens_of_two_addons_coexist_and_a_shared_name_is_reported() {
        let rec = |name: &str| {
            format!(r#"{{ "ui.screen": {{ "{name}": {{ "file": "ui/a.ui", "flow": "flow_frontend" }} }} }}"#)
        };
        let (d1, mut a) = addon("two-a", &[("registry/ui.json", &rec("first"))], "");
        let (d2, mut b) = addon("two-b", &[("registry/ui.json", &rec("second"))], "");
        b.id = "t.other".into();
        lower(&d1, &mut a).unwrap();
        lower(&d2, &mut b).unwrap();
        let both = crate::addons::settings::screens([&a, &b]).unwrap();
        let names: Vec<&str> = both.iter().map(|(_, s)| s.name.as_str()).collect();
        assert_eq!(names, ["first", "second"]);
        let (d3, mut c) = addon("two-c", &[("registry/ui.json", &rec("first"))], "");
        c.id = "t.third".into();
        lower(&d3, &mut c).unwrap();
        let e = crate::addons::settings::screens([&a, &c])
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("first") && e.contains("t.screens") && e.contains("t.third"),
            "{e}"
        );
        for d in [d1, d2, d3] {
            std::fs::remove_dir_all(d).ok();
        }
    }
}
