//! The link that asks the EA app to launch F1 25, built from the content ids in the copy's installerdata.xml.

use std::path::Path;

use super::MARKER;

/// The `<contentID>` values in the install's installerdata.xml.
pub fn content_ids(installerdata: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = installerdata;
    while let Some(start) = rest.find("<contentID>") {
        rest = &rest[start + "<contentID>".len()..];
        let Some(end) = rest.find("</contentID>") else {
            break;
        };
        let id = rest[..end].trim();
        if !id.is_empty() && !out.iter().any(|o| o == id) {
            out.push(id.to_owned());
        }
        rest = &rest[end..];
    }
    out
}

pub(super) fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The link that asks the EA app to launch the game, with `args` for it.
pub fn launch_url(ids: &[String], args: &[String]) -> Option<String> {
    if ids.is_empty() {
        return None;
    }
    let offers: Vec<String> = ids.iter().map(|i| encode(i)).collect();
    let mut url = format!(
        "origin2://game/launch?offerIds={}&autoDownload=1",
        offers.join(",")
    );
    if !args.is_empty() {
        url.push_str("&cmdParams=");
        url.push_str(&encode(&args.join(" ")));
    }
    Some(url)
}

pub fn install_url(dir: &Path, args: &[String]) -> Result<String, String> {
    let xml = std::fs::read_to_string(dir.join(MARKER))
        .map_err(|e| format!("could not read {}: {e}", dir.join(MARKER).display()))?;
    launch_url(&content_ids(&xml), args)
        .ok_or_else(|| format!("{} names no content ID to launch", dir.join(MARKER).display()))
}
