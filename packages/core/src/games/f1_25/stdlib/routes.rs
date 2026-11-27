use std::path::Path;

use serde_json::Value;
use swofty_addon::manifest::RouteOp;
use swofty_addon::{Error, Result};

use super::data;
use crate::manifest::AddonManifest;

pub const REGISTRY: &str = "ui.route";

pub const FILES: &[&str] = &["routes_frontend_general.uir"];

pub fn lower(dir: &Path, m: &mut AddonManifest) -> Result<()> {
    let files = data::files(dir);
    if files.is_empty() {
        return Ok(());
    }
    let mut added = 0;
    for rel in files {
        let Ok(text) = std::fs::read_to_string(dir.join(&rel)) else {
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
            let op = route_of(&rel, key, value)?;
            if m.operations.routes.iter().any(|r| r.id == op.id) {
                return Err(Error::Manifest(format!(
                    "{rel}: ui.route {key} is declared twice in this addon"
                )));
            }
            m.operations.routes.push(op);
            added += 1;
        }
    }
    if added > 0 {
        m.validate()?;
    }
    Ok(())
}

fn route_of(rel: &str, key: &str, value: &Value) -> Result<RouteOp> {
    let bad = |why: String| Error::Manifest(format!("{rel}: ui.route {key}: {why}"));
    if value.get("extends").is_some() {
        return Err(bad("extends is not applied to routes yet".into()));
    }
    let file = match value.get("file") {
        Some(Value::String(f)) if !f.is_empty() => f.clone(),
        _ => return Err(bad("needs a file".into())),
    };
    if !FILES.contains(&file.as_str()) {
        return Err(bad(format!(
            "file {file} is not a route file the build clones groups in; known: {}",
            FILES.join(", ")
        )));
    }
    let clone_of = match value.get("clone_of") {
        Some(Value::String(c)) if !c.is_empty() => c.clone(),
        _ => return Err(bad("needs clone_of, the route group it copies".into())),
    };
    let opens = match value.get("opens") {
        Some(Value::String(o)) if !o.is_empty() => o.rsplit('/').next().unwrap_or(o).to_owned(),
        _ => return Err(bad("needs opens, the ui.screen it opens".into())),
    };
    Ok(RouteOp {
        id: key.to_owned(),
        file,
        clone_of,
        opens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addon(tag: &str, record: &str) -> (std::path::PathBuf, AddonManifest) {
        let dir = std::env::temp_dir().join(format!("swofty-uiroute-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("registry")).unwrap();
        std::fs::write(dir.join("registry/ui.json"), record).unwrap();
        let m = AddonManifest::parse(
            r#"{ "schema_version": 1, "id": "t.routes", "name": "R", "version": "1.0.0", "author": "a",
                 "compat": { "game": "f1_25" } }"#,
        )
        .unwrap();
        (dir, m)
    }

    #[test]
    fn a_route_record_lowers_into_the_manifest() {
        let (dir, mut m) = addon(
            "ok",
            r#"{ "ui.route": { "open_page": { "file": "routes_frontend_general.uir", "clone_of": "options", "opens": "page" } } }"#,
        );
        lower(&dir, &mut m).unwrap();
        let r = &m.operations.routes[0];
        assert_eq!(
            (r.id.as_str(), r.clone_of.as_str(), r.opens.as_str()),
            ("open_page", "options", "page")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn opens_may_name_the_screen_by_its_record_id() {
        let (dir, mut m) = addon(
            "ref",
            r#"{ "ui.route": { "open_page": { "file": "routes_frontend_general.uir", "clone_of": "mail_container", "opens": "t.routes/page" } } }"#,
        );
        lower(&dir, &mut m).unwrap();
        assert_eq!(m.operations.routes[0].opens, "page");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_cloned_group_is_named_by_the_record_key_alone() {
        let (dir, mut m) = addon(
            "key",
            r#"{ "ui.route": { "open_settings": { "file": "routes_frontend_general.uir", "clone_of": "mail_container", "opens": "t.routes/settings" } } }"#,
        );
        lower(&dir, &mut m).unwrap();
        let r = &m.operations.routes[0];
        assert_eq!((r.id.as_str(), r.opens.as_str()), ("open_settings", "settings"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_record_without_opens_or_clone_of_or_a_known_file_is_refused() {
        for (tag, body, want) in [
            (
                "op",
                r#""clone_of": "options", "opens": null, "file": "routes_frontend_general.uir""#,
                "opens",
            ),
            (
                "cl",
                r#""opens": "page", "file": "routes_frontend_general.uir""#,
                "clone_of",
            ),
            (
                "fi",
                r#""opens": "page", "clone_of": "options", "file": "routes.uir""#,
                "routes.uir",
            ),
        ] {
            let (dir, mut m) = addon(tag, &format!(r#"{{ "ui.route": {{ "r": {{ {body} }} }} }}"#));
            let e = lower(&dir, &mut m).unwrap_err().to_string();
            assert!(e.contains(want), "{e}");
            std::fs::remove_dir_all(&dir).ok();
        }
    }
}
