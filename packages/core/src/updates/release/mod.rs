mod download;
mod fetch;
mod install;
mod unpack;

#[cfg(test)]
mod tests;

use std::time::Duration;

use semver::Version;
use serde::Deserialize;

use crate::{Error, Result};

pub use download::{
    check_download, download, download_or, download_release, mirror_url, parse_sums, verifiable,
};
pub use fetch::{
    Failure, endpoint, fetch_json, fetch_latest, fetch_latest_from, fetch_manifest, fetch_release, get,
    get_text, in_minutes, refusal, request,
};
pub use install::{DATA_IN_INSTALL, INSTALL_MANIFEST, Install, STAGING_DIR, apply, refuse_data_root};
pub use unpack::{exe, extract, release_root};

pub const LATEST_URL: &str =
    "https://api.github.com/repos/Swofty-Developments/SwoftyLauncher/releases/latest";
pub const RELEASES_URL: &str = "https://github.com/Swofty-Developments/SwoftyLauncher/releases";
pub const CDN_HOST: &str = "https://cdn.swofty.net";
pub const MANIFEST_URL: &str = "https://cdn.swofty.net/launcher/latest.json";
pub const MANIFEST_SCHEMA: u32 = 1;
pub const SUMS_FILE: &str = "SHA256SUMS";
pub const PRETEND_ENV: &str = "SWOFTY_UPDATE_PRETEND_VERSION";
pub const FIXTURE_ENV: &str = "SWOFTY_TEST_GITHUB";
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn parse_version(tag: &str) -> Option<Version> {
    Version::parse(tag.trim().trim_start_matches('v')).ok()
}

pub fn current_version() -> Version {
    std::env::var(PRETEND_ENV)
        .ok()
        .and_then(|v| parse_version(&v))
        .unwrap_or_else(|| Version::parse(env!("CARGO_PKG_VERSION")).expect("the package version is semver"))
}

pub fn notes_url(tag: &str) -> String {
    format!("{RELEASES_URL}/tag/{tag}")
}

pub fn asset_name(tag: &str, os: &str, arch: &str) -> Option<String> {
    match (os, arch) {
        ("linux", "x86_64") => Some(format!("swofty-launcher-{tag}-linux-x86_64.tar.gz")),
        ("windows", "x86_64") => Some(format!("swofty-launcher-{tag}-windows-x86_64.zip")),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub assets: Vec<Asset>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    #[serde(default)]
    pub schema: u32,
    pub version: String,
    pub tag: String,
    #[serde(default)]
    pub published_at: String,
    #[serde(default)]
    pub notes_url: String,
    #[serde(default)]
    pub assets: Vec<ManifestAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

impl Manifest {
    pub fn release(self) -> std::result::Result<Release, String> {
        if self.schema != MANIFEST_SCHEMA {
            return Err(format!(
                "manifest schema {} is not {MANIFEST_SCHEMA}",
                self.schema
            ));
        }
        let tag = parse_version(&self.tag).ok_or_else(|| format!("{} is not a version", self.tag))?;
        if parse_version(&self.version) != Some(tag) {
            return Err(format!(
                "version {} does not match tag {}",
                self.version, self.tag
            ));
        }
        let mut assets = Vec::new();
        for a in self.assets.into_iter().filter(|a| a.name != SUMS_FILE) {
            let sha = a.sha256.trim().to_ascii_lowercase();
            if a.name.is_empty() || a.name.contains(['/', '\\']) {
                return Err(format!("{:?} is not an asset name", a.name));
            }
            if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("{}: {:?} is not a SHA-256", a.name, a.sha256));
            }
            if a.size == 0 || a.url.is_empty() {
                return Err(format!("{}: no size or no download address", a.name));
            }
            assets.push(Asset {
                name: a.name,
                size: a.size,
                browser_download_url: a.url,
                digest: Some(format!("sha256:{sha}")),
            });
        }
        if assets.is_empty() {
            return Err("manifest lists no downloads".into());
        }
        Ok(Release {
            tag_name: self.tag,
            html_url: self.notes_url,
            assets,
            draft: false,
            prerelease: false,
        })
    }
}

impl Release {
    pub fn version(&self) -> Option<Version> {
        parse_version(&self.tag_name)
    }

    pub fn notes_url(&self) -> String {
        if self.html_url.is_empty() {
            notes_url(&self.tag_name)
        } else {
            self.html_url.clone()
        }
    }

    pub fn asset(&self, os: &str, arch: &str) -> Option<&Asset> {
        let name = asset_name(&self.tag_name, os, arch)?;
        self.assets.iter().find(|a| a.name == name)
    }

    pub fn platform_asset(&self) -> Result<&Asset> {
        let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
        self.asset(os, arch).ok_or_else(|| {
            Error::Other(format!(
                "release {} has no download for {os} {arch}",
                self.tag_name
            ))
        })
    }

    pub fn sums(&self) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == SUMS_FILE)
    }
}
