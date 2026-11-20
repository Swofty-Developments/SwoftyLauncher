//! One part's latest release as the launcher follows it: when to probe its tag again and when to fetch the
//! release itself.

use semver::Version;

use super::probe::{Part, Probe, probe};
use super::{CHECK_INTERVAL, RELEASE_RECHECK};
use crate::updates::managed;
use crate::updates::release::{self, Failure, Release};

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub tag: Option<String>,
    pub release: Option<Release>,
    pub probe_at: u64,
    pub fetch_at: u64,
    pub error: Option<String>,
}

impl Track {
    pub fn probe_due(&self, now: u64) -> bool {
        now >= self.probe_at
    }

    pub fn version(&self) -> Option<Version> {
        release::parse_version(self.tag.as_deref()?)
    }

    pub fn newer_than(&self, installed: Option<&Version>) -> bool {
        self.version().is_some_and(|v| managed::newer(installed, &v))
    }

    pub fn on_probe(&mut self, probe: Probe, now: u64) {
        let interval = CHECK_INTERVAL.as_secs();
        self.probe_at = now + interval;
        self.error = None;
        match probe {
            Probe::Tag(tag) => self.tag = Some(tag),
            Probe::NoRelease => {
                self.tag = None;
                self.release = None;
            }
            Probe::Refused(Failure::Wait(secs, text)) => {
                self.probe_at = now + secs.max(interval);
                self.error = Some(text);
            }
            Probe::Refused(Failure::Failed(text)) => self.error = Some(text),
        }
    }

    pub fn on_manifest(&mut self, part: Part, release: Release, now: u64) {
        self.probe_at = now + CHECK_INTERVAL.as_secs();
        self.error = None;
        if part.ready(&release) {
            self.tag = Some(release.tag_name.clone());
            self.release = Some(release);
        } else {
            self.error = Some(format!("release {} is not complete yet", release.tag_name));
        }
    }

    pub fn fetch_due(&self, installed: Option<&Version>, now: u64) -> bool {
        self.newer_than(installed)
            && self.release.as_ref().map(|r| &r.tag_name) != self.tag.as_ref()
            && now >= self.fetch_at
    }

    pub fn on_fetch(&mut self, part: Part, fetched: std::result::Result<Release, Failure>, now: u64) {
        self.fetch_at = now + RELEASE_RECHECK.as_secs();
        match fetched {
            Ok(r) if part.ready(&r) && r.version().is_some_and(|v| self.version().is_none_or(|t| v >= t)) => {
                self.tag = Some(r.tag_name.clone());
                self.release = Some(r);
            }
            Ok(r) => self.error = Some(format!("release {} is not complete yet", r.tag_name)),
            Err(Failure::Wait(secs, text)) => {
                self.fetch_at = now + secs.max(CHECK_INTERVAL.as_secs());
                self.error = Some(text);
            }
            Err(Failure::Failed(text)) => self.error = Some(text),
        }
    }

    pub fn offer(&self, installed: Option<&Version>) -> Option<&Release> {
        self.release.as_ref().filter(|r| {
            Some(&r.tag_name) == self.tag.as_ref()
                && r.version().is_some_and(|v| managed::newer(installed, &v))
        })
    }

    /// Asks the network about `part`: the CDN's manifest for the launcher, else a probe of the latest
    /// release and, when `fetch` allows it and one is due, the release itself.
    pub fn check(&mut self, part: Part, installed: Option<&Version>, fetch: bool, now: u64) {
        if part == Part::Launcher
            && let Ok(release) = release::fetch_manifest(release::MANIFEST_URL, release::CHECK_TIMEOUT)
        {
            self.on_manifest(part, release, now);
            return;
        }
        self.on_probe(probe(part, release::CHECK_TIMEOUT), now);
        if fetch && self.fetch_due(installed, now) {
            self.on_fetch(
                part,
                release::fetch_release(part.api_url(), release::CHECK_TIMEOUT),
                now,
            );
        }
    }
}

/// [`Track::check`].
pub fn check(part: Part, track: &mut Track, installed: Option<&Version>, fetch: bool, now: u64) {
    track.check(part, installed, fetch, now);
}
