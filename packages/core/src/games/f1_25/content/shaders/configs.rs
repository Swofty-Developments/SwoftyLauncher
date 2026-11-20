//! Personal and addon shader configs: listing, reading, writing and naming them, and the config each profile has active.

use std::ops::Range;
use std::path::{Path, PathBuf};

use crate::profile::Profile;
use crate::store::Store;
use crate::{Error, Result, io_err};

use super::{ConfigEntry, ConfigRef, active_profile, shader_addons, starter};

pub(super) const EXTENSION: &str = "swfx";

pub(super) fn escape(name: &str) -> String {
    let mut out = String::from('"');
    for c in name.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn unescape(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('\'') {
        return Some(rest[..rest.find('\'')?].to_owned());
    }
    let mut chars = value.strip_prefix('"')?.chars();
    let mut out = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(out),
            '\\' => out.push(match chars.next()? {
                'n' => '\n',
                't' => '\t',
                c => c,
            }),
            c => out.push(c),
        }
    }
}

/// The top-level `name = "…"` line: its byte range without the line break, and the name.
pub(super) fn name_line(text: &str) -> Option<(Range<usize>, String)> {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let t = body.trim_start();
        if t.starts_with('[') {
            return None;
        }
        if let Some(rest) = t.strip_prefix("name")
            && let Some(value) = rest.trim_start().strip_prefix('=')
            && let Some(name) = unescape(value)
        {
            return Some((at..at + body.len(), name));
        }
        at += line.len();
    }
    None
}

pub(super) fn name_of(text: &str, fallback: &str) -> String {
    name_line(text)
        .map(|(_, n)| n)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| fallback.to_owned())
}

/// `text` with its `name` set; only that line changes.
pub(super) fn with_name(text: &str, name: &str) -> String {
    let line = format!("name = {}", escape(name));
    match name_line(text) {
        Some((range, _)) => format!("{}{line}{}", &text[..range.start], &text[range.end..]),
        None => format!("{line}\n{text}"),
    }
}

pub(super) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "config".into()
    } else {
        out.chars().take(48).collect()
    }
}

pub(super) fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

pub(super) fn personal_path(store: &Store, slug: &str) -> PathBuf {
    store.shaders_dir().join(format!("{slug}.{EXTENSION}"))
}

pub(super) fn unique_slug(store: &Store, base: &str) -> String {
    let mut candidate = base.to_owned();
    let mut n = 2;
    while personal_path(store, &candidate).exists() {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

pub(super) struct Personal {
    slug: String,
    name: String,
    path: PathBuf,
}

pub(super) fn personal(store: &Store) -> Vec<Personal> {
    let mut out: Vec<Personal> = std::fs::read_dir(store.shaders_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let slug = path.file_name()?.to_str()?.strip_suffix(".swfx")?.to_owned();
            valid_slug(&slug).then(|| {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                Personal {
                    name: name_of(&text, &slug),
                    slug,
                    path,
                }
            })
        })
        .collect();
    out.sort_by_key(|p| (p.name.to_lowercase(), p.slug.clone()));
    out
}

pub(super) fn name_taken(store: &Store, name: &str) -> bool {
    personal(store).iter().any(|p| p.name.eq_ignore_ascii_case(name))
}

pub(super) fn free_name(store: &Store, name: &str) -> String {
    let mut candidate = name.to_owned();
    let mut n = 2;
    while name_taken(store, &candidate) {
        candidate = format!("{name} ({n})");
        n += 1;
    }
    candidate
}

/// Personal configs first (by name), then addon configs in load order.
pub fn list(store: &Store) -> Vec<ConfigEntry> {
    let mut out: Vec<ConfigEntry> = personal(store)
        .into_iter()
        .map(|p| ConfigEntry {
            r#ref: ConfigRef::Personal(p.slug),
            name: p.name,
            source: "Personal".into(),
            editable: true,
            path: p.path,
        })
        .collect();
    let profile = active_profile(store);
    for m in shader_addons(store, profile.as_ref()) {
        for decl in &m.shaders {
            let path = store.addons_dir().join(&m.id).join(&decl.config);
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            out.push(ConfigEntry {
                r#ref: ConfigRef::Addon {
                    addon: m.id.clone(),
                    file: decl.stem().to_owned(),
                },
                name: name_of(&text, decl.stem()),
                source: m.name.clone(),
                editable: false,
                path,
            });
        }
    }
    out
}

pub(super) fn entry(store: &Store, r: &ConfigRef) -> Result<ConfigEntry> {
    match r {
        ConfigRef::Personal(slug) if valid_slug(slug) => {
            let path = personal_path(store, slug);
            if !path.is_file() {
                return Err(Error::Other(format!("no personal shader config {slug}")));
            }
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            Ok(ConfigEntry {
                r#ref: r.clone(),
                name: name_of(&text, slug),
                source: "Personal".into(),
                editable: true,
                path,
            })
        }
        _ => list(store)
            .into_iter()
            .find(|e| &e.r#ref == r)
            .ok_or_else(|| Error::Other(format!("no shader config {r}"))),
    }
}

pub fn read(store: &Store, r: &ConfigRef) -> Result<String> {
    let e = entry(store, r)?;
    std::fs::read_to_string(&e.path).map_err(io_err(&e.path))
}

pub(super) fn write_text(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    }
    let tmp = path.with_extension("swfx.tmp");
    std::fs::write(&tmp, text).map_err(io_err(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io_err(path))
}

/// Personal configs only.
pub fn save(store: &Store, r: &ConfigRef, text: &str) -> Result<()> {
    match r {
        ConfigRef::Personal(_) => write_text(&entry(store, r)?.path, text),
        ConfigRef::Addon { .. } => Err(Error::Other(
            "an addon's shader config is read-only; copy it to your own configs to edit it".into(),
        )),
    }
}

pub(super) fn write_new(store: &Store, name: &str, text: &str) -> Result<ConfigRef> {
    let slug = unique_slug(store, &slug(name));
    write_text(&personal_path(store, &slug), &with_name(text, name))?;
    Ok(ConfigRef::Personal(slug))
}

/// `None` starts from the starter template.
pub fn create(store: &Store, name: &str, text: Option<&str>) -> Result<ConfigRef> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Other("a shader config needs a name".into()));
    }
    if name_taken(store, name) {
        return Err(Error::Other(format!(
            "you already have a shader config named {name}"
        )));
    }
    let text = match text {
        Some(t) => t,
        None => starter().unwrap_or("name = \"\"\n"),
    };
    write_new(store, name, text)
}

pub fn copy_to_personal(store: &Store, r: &ConfigRef) -> Result<ConfigRef> {
    let text = read(store, r)?;
    let name = name_of(&text, "Shader config");
    let name = if name_taken(store, &name) {
        let mut candidate = format!("{name} (copy)");
        let mut n = 2;
        while name_taken(store, &candidate) {
            candidate = format!("{name} (copy {n})");
            n += 1;
        }
        candidate
    } else {
        name
    };
    write_new(store, &name, &text)
}

pub(super) fn repoint(store: &Store, from: &ConfigRef, to: Option<&ConfigRef>) -> Result<()> {
    let from = from.to_string();
    for name in store.profile_names()? {
        if name == crate::profile::VANILLA {
            continue;
        }
        let Ok(p) = store.profile(&name) else { continue };
        if p.shader.as_deref() == Some(from.as_str()) {
            store.update_profile(&name, |p| {
                p.shader = to.map(ToString::to_string);
                Ok(())
            })?;
        }
    }
    Ok(())
}

pub fn rename(store: &Store, r: &ConfigRef, name: &str) -> Result<ConfigRef> {
    let ConfigRef::Personal(old) = r else {
        return Err(Error::Other("an addon's shader config cannot be renamed".into()));
    };
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Other("a shader config needs a name".into()));
    }
    let e = entry(store, r)?;
    if !e.name.eq_ignore_ascii_case(name) && name_taken(store, name) {
        return Err(Error::Other(format!(
            "you already have a shader config named {name}"
        )));
    }
    let text = with_name(&read(store, r)?, name);
    let base = slug(name);
    let new = if &base == old || personal_path(store, &base) == e.path {
        old.clone()
    } else {
        unique_slug(store, &base)
    };
    write_text(&personal_path(store, &new), &text)?;
    let renamed = ConfigRef::Personal(new);
    if &renamed != r {
        std::fs::remove_file(&e.path).map_err(io_err(&e.path))?;
        repoint(store, r, Some(&renamed))?;
    }
    Ok(renamed)
}

pub fn delete(store: &Store, r: &ConfigRef) -> Result<()> {
    if !matches!(r, ConfigRef::Personal(_)) {
        return Err(Error::Other(
            "an addon's shader config cannot be deleted; remove the addon".into(),
        ));
    }
    let e = entry(store, r)?;
    std::fs::remove_file(&e.path).map_err(io_err(&e.path))?;
    repoint(store, r, None)
}

pub fn import(store: &Store, path: &Path) -> Result<ConfigRef> {
    let text = std::fs::read_to_string(path).map_err(io_err(path))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported config");
    let name = free_name(store, &name_of(&text, stem));
    write_new(store, &name, &text)
}

pub fn export(store: &Store, r: &ConfigRef, path: &Path) -> Result<()> {
    let text = read(store, r)?;
    std::fs::write(path, text).map_err(io_err(path))
}

pub fn active(store: &Store, profile: &Profile) -> Option<ConfigRef> {
    let r: ConfigRef = profile.shader.as_deref()?.parse().ok()?;
    entry(store, &r).ok().map(|_| r)
}

pub fn set_active(store: &Store, profile: &mut Profile, r: Option<ConfigRef>) -> Result<()> {
    profile.shader = r.map(|r| r.to_string());
    store.save_profile(profile)
}

/// The config an enabled addon forces, with the addon's name. A shader-only addon is enabled
/// whenever it is installed; of several forcing addons the last in load order wins.
pub fn forced(store: &Store, profile: &Profile) -> Option<(ConfigRef, String)> {
    if profile.is_vanilla() {
        return None;
    }
    let enabled: Vec<&str> = profile.enabled().collect();
    shader_addons(store, Some(profile))
        .into_iter()
        .filter(|m| m.is_shader_only() || enabled.contains(&m.id.as_str()))
        .filter_map(|m| {
            let decl = m.shaders.iter().find(|d| d.force)?;
            Some((
                ConfigRef::Addon {
                    addon: m.id.clone(),
                    file: decl.stem().to_owned(),
                },
                m.name.clone(),
            ))
        })
        .next_back()
}
