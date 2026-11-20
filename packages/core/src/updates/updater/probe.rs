//! The parts that update, the launcher and its managed addon, and the probe of a part's latest release tag
//! from its releases page.

use std::time::Duration;

use crate::updates::managed;
use crate::updates::release::{self, Failure, Release};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Part {
    Addon,
    Launcher,
}

impl Part {
    pub fn name(self) -> &'static str {
        match self {
            Self::Addon => managed::NAME,
            Self::Launcher => "Swofty Launcher",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::Addon => "Simplified",
            Self::Launcher => "Launcher",
        }
    }

    pub fn releases_url(self) -> &'static str {
        match self {
            Self::Addon => managed::RELEASES_URL,
            Self::Launcher => release::RELEASES_URL,
        }
    }

    pub fn api_url(self) -> &'static str {
        match self {
            Self::Addon => managed::LATEST_URL,
            Self::Launcher => release::LATEST_URL,
        }
    }

    pub fn ready(self, release: &Release) -> bool {
        match self {
            Self::Addon => managed::ready(release),
            Self::Launcher => release
                .platform_asset()
                .is_ok_and(|a| release::verifiable(release, a)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Probe {
    Tag(String),
    NoRelease,
    Refused(Failure),
}

pub(super) fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn tag_from_location(location: &str) -> Option<String> {
    let (_, rest) = location.split_once("/releases/tag/")?;
    let tag = unescape(rest.split(['?', '#', '/']).next()?);
    (!tag.is_empty()).then_some(tag)
}

pub fn read_probe(url: &str, status: u16, location: Option<&str>) -> Probe {
    if !(300..400).contains(&status) {
        return Probe::Refused(Failure::Failed(format!(
            "{url}: expected a redirect to the latest release, got HTTP {status}"
        )));
    }
    location
        .and_then(tag_from_location)
        .map_or(Probe::NoRelease, Probe::Tag)
}

pub fn probe(part: Part, timeout: Duration) -> Probe {
    let url = format!("{}/latest", part.releases_url());
    match release::request(&url, true, Some(timeout)) {
        Ok(r) => read_probe(
            &url,
            r.status().as_u16(),
            r.headers().get("location").and_then(|v| v.to_str().ok()),
        ),
        Err(f) => Probe::Refused(f),
    }
}
