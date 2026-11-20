use std::path::Path;

use crate::GAME_EXE;
use crate::install::PROXY;

use super::{Event, norm};

/// The Code Integrity or Defender events in `wevtutil qe /f:xml` output that
/// name version.dll, F1_25.exe or the game folder.
pub fn events(xml: &str, game: &Path) -> Vec<Event> {
    let folder = norm(&game.to_string_lossy()).to_lowercase();
    let tail = folder
        .split_once(':')
        .map_or(folder.as_str(), |(_, t)| t)
        .to_owned();
    xml.split("<Event xmlns")
        .skip(1)
        .filter_map(|chunk| {
            let lower = chunk.to_lowercase();
            let proxy = lower.contains(&PROXY.to_lowercase());
            let game = lower.contains(&GAME_EXE.to_lowercase()) || (tail.len() > 1 && lower.contains(&tail));
            if !proxy && !game {
                return None;
            }
            let id = tag_text(chunk, "EventID")?.trim().parse().ok()?;
            let data = data_fields(chunk);
            let field = |names: &[&str]| {
                data.iter()
                    .find(|(n, v)| names.contains(&n.as_str()) && !v.is_empty())
                    .map(|(_, v)| v.clone())
            };
            Some(Event {
                id,
                at: attr(chunk, "SystemTime").and_then(clock),
                file: field(&["filename", "filenamebuffer", "path"]),
                process: field(&["processname", "processnamebuffer"]),
                policy: field(&["policyname"]),
                threat: field(&["threatname"]),
                proxy,
            })
        })
        .collect()
}

fn tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = xml.find(&format!("<{tag}"))?;
    let body = &xml[open + tag.len() + 1..];
    let body = &body[body.find('>')? + 1..];
    Some(&body[..body.find('<')?])
}

pub(super) fn attr<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let rest = &xml[xml.find(&format!("{name}="))? + name.len() + 1..];
    let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
    let rest = &rest[1..];
    Some(&rest[..rest.find(quote)?])
}

fn clock(stamp: &str) -> Option<String> {
    let time = stamp.split_once('T')?.1.get(..8)?;
    Some(format!("{time} UTC"))
}

/// The `<Data Name='…'>…</Data>` pairs, names lowercased without spaces.
fn data_fields(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<Data Name=") {
        rest = &rest[at + "<Data Name=".len()..];
        let Some(quote) = rest.chars().next().filter(|c| *c == '\'' || *c == '"') else {
            continue;
        };
        let Some(end) = rest[1..].find(quote) else { break };
        let name: String = rest[1..1 + end]
            .chars()
            .filter(|c| !c.is_whitespace())
            .flat_map(char::to_lowercase)
            .collect();
        rest = &rest[end + 2..];
        let value = if rest.trim_start().starts_with("/>") {
            String::new()
        } else {
            let Some(open) = rest.find('>') else { break };
            let body = &rest[open + 1..];
            let Some(close) = body.find("</Data>") else { break };
            unescape(&body[..close])
        };
        out.push((name, value));
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
