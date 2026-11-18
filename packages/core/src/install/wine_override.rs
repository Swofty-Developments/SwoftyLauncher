//! The DLL override in the Wine prefix's user.reg that makes the game load the launcher's version.dll: added
//! to its section, and taken out leaving the rest of the section as it was.

use super::{OVERRIDE_HEADER, OVERRIDE_VALUE};

pub(super) fn remove_section(text: &str, header: &str) -> Option<String> {
    let start = text.find(header)?;
    let from = if start > 0 && text.as_bytes()[start - 1] == b'\n' {
        start - 1
    } else {
        start
    };
    let end = text[start + header.len()..]
        .find("\n[")
        .map_or(text.len(), |i| start + header.len() + i);
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..from]);
    out.push_str(&text[end..]);
    if !text[end..].is_empty() && from == start {
        out = format!("{}{}", &text[..from], &text[end + 1..]);
    }
    if text.ends_with('\n') && !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    Some(out)
}

pub(super) fn override_section(text: &str) -> Option<std::ops::Range<usize>> {
    let start = text.find(OVERRIDE_HEADER)?;
    let end = text[start..].find("\n[").map_or(text.len(), |i| start + i);
    Some(start..end)
}

pub(super) fn version_value(line: &str) -> Option<&str> {
    let (name, value) = line.split_once('=')?;
    let name = name.trim().trim_matches('"').trim_start_matches('*');
    name.eq_ignore_ascii_case("version")
        .then(|| value.trim().trim_matches('"'))
}

/// Whether a prefix's user.reg has F1_25.exe load a native version.dll
/// first. A section for F1_25.exe without a version.dll value leaves Wine
/// on its builtin one.
pub fn has_override(text: &str) -> bool {
    override_section(text).is_some_and(|s| {
        text[s]
            .lines()
            .skip(1)
            .filter_map(version_value)
            .any(|v| v.starts_with('n'))
    })
}

/// user.reg with the version.dll override, in F1_25.exe's section when the
/// prefix has one; None when that section already sets version.dll.
pub fn add_override(text: &str) -> Option<String> {
    let Some(s) = override_section(text) else {
        let mut out = text.to_owned();
        if !out.ends_with('\n') && !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("\n{OVERRIDE_HEADER}\n{OVERRIDE_VALUE}\n"));
        return Some(out);
    };
    if text[s.clone()]
        .lines()
        .skip(1)
        .any(|l| version_value(l).is_some())
    {
        return None;
    }
    let mut at = text[s.clone()].find('\n').map_or(s.end, |i| s.start + i + 1);
    while text[at..s.end].starts_with('#') {
        at = text[at..s.end].find('\n').map_or(s.end, |i| at + i + 1);
    }
    let sep = if at == text.len() && !text.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    Some(format!("{}{sep}{OVERRIDE_VALUE}\n{}", &text[..at], &text[at..]))
}

/// user.reg without the version.dll override, and without F1_25.exe's
/// section when nothing else is set in it; None when the section sets other
/// DLLs but not version.dll.
pub(super) fn remove_override(text: &str) -> Option<String> {
    let s = override_section(text)?;
    let lines = || text[s.clone()].lines().skip(1).map(str::trim);
    if !lines().any(|l| !l.is_empty() && !l.starts_with('#') && version_value(l).is_none()) {
        return remove_section(text, OVERRIDE_HEADER);
    }
    if !lines().any(|l| version_value(l).is_some()) {
        return None;
    }
    let kept: String = text[s.clone()]
        .split_inclusive('\n')
        .enumerate()
        .filter(|(i, l)| *i == 0 || version_value(l).is_none())
        .map(|(_, l)| l)
        .collect();
    Some(format!("{}{kept}{}", &text[..s.start], &text[s.end..]))
}
