//! The installed addons and the profiles that run them, as `addons.json` and the summary's Addons section.

use serde_json::{Value, json};

use super::REDACTED;
use super::collect::Sources;
use super::redact::{redact_json, secret_name};
use crate::manifest::AddonManifest;

impl Sources<'_> {
    pub(super) fn addons_json(&self) -> Value {
        let store = self.store;
        let manifests = store.addons();
        let installed = match &manifests {
            Ok(addons) => Value::Array(
                addons
                    .values()
                    .map(|m| {
                        json!({
                            "id": m.id,
                            "name": m.name,
                            "version": m.version.to_string(),
                            "builtin": store.is_builtin(&m.id),
                            "source": self.addon_source(m),
                            "category": m.category,
                            "kind": m.kind,
                            "origin": m.origin,
                            "experimental_features": m.experimental_features,
                        })
                    })
                    .collect(),
            ),
            Err(e) => json!({ "error": e.to_string() }),
        };
        let name = |id: &str| {
            manifests
                .as_ref()
                .ok()
                .and_then(|a| a.get(id))
                .map(|m| m.name.clone())
        };
        let active = store.active_profile();
        let profiles = match store.profile_names() {
        Ok(names) => Value::Array(
            names
                .iter()
                .map(|n| match store.profile(n) {
                    Ok(p) => json!({
                        "name": n,
                        "active": *n == active,
                        "addons": p.addons.iter().map(|a| {
                            let mut entry = json!({ "id": a.id, "name": name(&a.id), "enabled": a.enabled });
                            if let Some(options) = p.options.get(&a.id) {
                                entry["options"] = json!(options);
                            }
                            entry
                        }).collect::<Vec<_>>(),
                    }),
                    Err(e) => json!({ "name": n, "error": e.to_string() }),
                })
                .collect(),
        ),
        Err(e) => json!({ "error": e.to_string() }),
    };
        let unreadable: Vec<Value> = store
            .unreadable_addons()
            .iter()
            .map(|u| json!({ "id": u.id, "name": u.name, "version": u.version, "reason": u.reason }))
            .collect();
        let mut out = json!({ "installed": installed, "profiles": profiles });
        if !unreadable.is_empty() {
            out["unreadable"] = Value::Array(unreadable);
        }
        redact_json(&mut out);
        out
    }

    /// Where an addon came from, as the Addons page tags it.
    fn addon_source(&self, m: &AddonManifest) -> String {
        let store = self.store;
        let tool = m.origin.as_ref().map(|o| o.tool.as_str());
        if store.is_builtin(&m.id) || m.id == crate::updates::managed::ID {
            "built in".into()
        } else if tool == Some(crate::dbedit::package::ORIGIN_TOOL)
            || crate::dbedit::package::is_package(store, &m.id)
        {
            "DB Editor".into()
        } else if tool == Some(crate::updates::managed::ID) {
            match m.origin.as_ref().and_then(|o| o.file.as_deref()) {
                Some(file) => format!("imported from {file}"),
                None => "imported".into(),
            }
        } else {
            "installed".into()
        }
    }

    /// The active profile's addons by name: those on in load order, then the rest,
    /// each with its source and the settings changed from their defaults.
    pub(super) fn addon_lines(&self) -> String {
        let store = self.store;
        let addons = match store.addons() {
            Ok(a) => a,
            Err(e) => return format!("  could not list the addons: {e}\n"),
        };
        let active = store.active_profile();
        let p = match store.profile(&active) {
            Ok(p) => p,
            Err(e) => {
                let mut s = format!("  Active profile: {active} (could not be read: {e})\n");
                for m in addons.values() {
                    s.push_str(&format!("  {} {} ({})\n", m.name, m.version, m.id));
                }
                return s;
            }
        };
        let row = |id: &str| match addons.get(id) {
            Some(m) => {
                let mut row = format!("{} {} ({}), {}\n", m.name, m.version, m.id, self.addon_source(m));
                let stored = p.options.get(id);
                let declared = m.options.iter().map(|o| (o.id.as_str(), Some(o)));
                let stale = stored
                    .into_iter()
                    .flat_map(|v| v.keys())
                    .filter(|k| !m.options.iter().any(|o| o.id == **k))
                    .map(|k| (k.as_str(), None));
                for (option, o) in declared.chain(stale) {
                    let Some(value) = stored.and_then(|v| v.get(option)) else {
                        continue;
                    };
                    let shown = |v: &Value| match secret_name(option) {
                        true => REDACTED.to_owned(),
                        false => setting_text(v),
                    };
                    let label = o.and_then(|o| o.label.as_deref()).unwrap_or(option);
                    let default = o.map_or_else(String::new, |o| format!(" (default {})", shown(&o.default)));
                    row.push_str(&format!("          {label}: {}{default}\n", shown(value)));
                }
                row
            }
            None => format!("{id} (not installed)\n"),
        };
        let on: Vec<&str> = p.enabled().collect();
        let mut s = format!(
            "  Active profile: {active}, {} of {} addons on\n",
            on.len(),
            addons.len()
        );
        s.push_str("  On, in load order (a later addon wins where two change the same thing):\n");
        for (i, id) in on.iter().enumerate() {
            s.push_str(&format!("  {:>4}. {}", i + 1, row(id)));
        }
        let off: Vec<&str> = addons
            .keys()
            .map(String::as_str)
            .filter(|id| !on.contains(id))
            .collect();
        if !off.is_empty() {
            s.push_str("  Off:\n");
            for id in off {
                s.push_str(&format!("        {}", row(id)));
            }
        }
        let others: Vec<String> = store
            .profile_names()
            .unwrap_or_default()
            .into_iter()
            .filter(|n| *n != active)
            .map(|n| match store.profile(&n) {
                Ok(o) => format!("{n} ({} on)", o.enabled().count()),
                Err(_) => n,
            })
            .collect();
        let unreadable = store.unreadable_addons();
        if !unreadable.is_empty() {
            s.push_str("  Can't be used:\n");
            for u in &unreadable {
                s.push_str(&format!("        {}\n", u.line()));
            }
        }
        if !others.is_empty() {
            s.push_str(&format!("  Other profiles: {}\n", others.join(", ")));
        }
        s
    }
}

fn setting_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
