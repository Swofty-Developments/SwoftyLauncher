//! The records of an addon's hud file as composer inputs, with the unique prefix of each record's names.

use super::model::HudElement;
use super::{Input, err};
use crate::{Error, Result};
use std::path::Path;
use swofty_addon::ops::HudFile;

pub(super) fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '_' };
        if c != '_' || !out.ends_with('_') {
            out.push(c);
        }
    }
    out.trim_matches('_').to_owned()
}

/// The prefix of a record's element and template node names.
pub fn prefix_for(addon: &str, key: &str, explicit: Option<&str>) -> Result<String> {
    let p = match explicit {
        Some(p) => p.to_owned(),
        None => {
            let tail = addon.rsplit('.').next().unwrap_or(addon);
            let a = slug(addon);
            if slug(tail) == slug(key) {
                a
            } else {
                format!("{a}_{}", slug(key))
            }
        }
    };
    let ok = !p.is_empty()
        && p.len() <= 24
        && p.starts_with(|c: char| c.is_ascii_lowercase())
        && p.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(p)
    } else {
        Err(err(format!(
            "{addon}/{key}: the prefix {p:?} must be 1 to 24 characters [a-z0-9_] starting with a letter; set `prefix` in the record"
        )))
    }
}

/// Records that several addons bring alike: the same key, the same `prefix`, an equal record and the
/// same bytes in every sprite. A family of addons that each must work alone (a season per addon, say)
/// can then carry one shared screen record without the board growing with every addon: all screen
/// records share the cap scene and each brings its own host, so a board holds
/// `(1 + records) x (cap + every record)` walk slots. The first in load order is composed; the
/// others are left out with a line. Records that share a prefix and differ in anything still stop
/// the build in [`compose_with`].
pub fn shared_records(inputs: Vec<Input>) -> Result<(Vec<Input>, Vec<String>)> {
    let mut kept: Vec<Input> = Vec::new();
    let mut lines = Vec::new();
    'next: for input in inputs {
        if let Some(p) = input.record.prefix.as_deref() {
            for k in &kept {
                if k.key == input.key
                    && k.record.prefix.as_deref() == Some(p)
                    && k.record == input.record
                    && same_sprites(k, &input)?
                {
                    lines.push(format!(
                        "{p}: {} brings the same record {} as {}; composed once",
                        input.addon, input.key, k.addon
                    ));
                    continue 'next;
                }
            }
        }
        kept.push(input);
    }
    Ok((kept, lines))
}

fn same_sprites(a: &Input, b: &Input) -> Result<bool> {
    for s in a.record.sprites.values() {
        let pa = a.dir.join(&s.png);
        let ra = std::fs::read(&pa).map_err(|e| Error::Io {
            path: pa.clone(),
            source: e,
        })?;
        match std::fs::read(b.dir.join(&s.png)) {
            Ok(rb) if rb == ra => {}
            _ => return Ok(false),
        }
    }
    Ok(true)
}

/// An addon's hud file: its text and what it holds.
pub fn read(dir: &Path, file: &str) -> Result<(String, HudFile)> {
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).map_err(|e| Error::Io {
        path: path.clone(),
        source: e,
    })?;
    let doc = HudFile::parse(&text).map_err(|e| err(format!("{path:?}: {e}")))?;
    Ok((text, doc))
}

/// The `hud.element` template-only record a `hud.widget` record `{ view_model, tmpl }` is.
pub fn widget_record(id: &str, value: &serde_json::Value) -> Result<serde_json::Value> {
    let o = value
        .as_object()
        .ok_or_else(|| err(format!("hud.widget {id}: a record is an object")))?;
    let view_model = match o.get("view_model") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => s,
        _ => {
            return Err(err(format!(
                "hud.widget {id}: view_model names the view-model path the widget binds"
            )));
        }
    };
    let file = match o.get("tmpl") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => s,
        None | Some(serde_json::Value::Null) => {
            return Err(err(format!(
                "hud.widget {id}: missing tmpl; name the .ui template file in the addon's folder"
            )));
        }
        Some(_) => {
            return Err(err(format!(
                "hud.widget {id}: tmpl is a .ui file in the addon's folder (a stock or archive path is not built yet)"
            )));
        }
    };
    Ok(serde_json::json!({ "template": { "file": file, "view_model": view_model } }))
}

/// The records of an addon's hud file `doc`, read from `file`, as inputs. Every `hud.widget` record is
/// lowered to its template-only `hud.element` record first, so one composer builds both.
pub fn inputs_of(addon: &str, dir: &Path, file: &str, doc: HudFile) -> Result<Vec<Input>> {
    let path = dir.join(file);
    let mut out = Vec::new();
    for (id, value) in &doc.widgets {
        let key = id.rsplit('/').next().unwrap_or(id).to_owned();
        if doc
            .elements
            .keys()
            .any(|e| e.rsplit('/').next().unwrap_or(e) == key)
        {
            return Err(err(format!(
                "{path:?}: hud.widget {id} and hud.element {id} share an id; rename one"
            )));
        }
        let lowered = widget_record(id, value).map_err(|e| err(format!("{path:?}: {e}")))?;
        let record =
            HudElement::parse(&lowered).map_err(|e| err(format!("{path:?}: hud.widget {id}: {e}")))?;
        out.push(Input {
            addon: addon.to_owned(),
            key,
            dir: dir.to_path_buf(),
            record,
        });
    }
    for (id, value) in doc.elements {
        let key = id.rsplit('/').next().unwrap_or(&id).to_owned();
        let record = HudElement::parse(&value).map_err(|e| err(format!("{path:?}: {id}: {e}")))?;
        out.push(Input {
            addon: addon.to_owned(),
            key,
            dir: dir.to_path_buf(),
            record,
        });
    }
    Ok(out)
}
