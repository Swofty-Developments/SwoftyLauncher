//! Downloading a release asset and checking it against the release's SHA-256 sums.

use std::collections::BTreeMap;
use std::path::Path;

use crate::updates::transfer;
use crate::{Error, Result};

use super::fetch::{Failure, answered, builder};
use super::{Asset, CDN_HOST, RELEASES_URL, Release, SUMS_FILE};

pub fn parse_sums(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.trim().split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !name.is_empty())
                .then(|| (name.to_owned(), hash.to_ascii_lowercase()))
        })
        .collect()
}

pub fn check_download(
    asset: &Asset,
    len: u64,
    sha256: &str,
    sums: Option<&BTreeMap<String, String>>,
) -> Result<()> {
    if len != asset.size {
        return Err(Error::Other(format!(
            "{}: downloaded {len} bytes, the release lists {}",
            asset.name, asset.size
        )));
    }
    let expected = match sums {
        Some(sums) => Some(
            sums.get(&asset.name)
                .cloned()
                .ok_or_else(|| Error::Other(format!("{SUMS_FILE} has no entry for {}", asset.name)))?,
        ),
        None => asset
            .digest
            .as_deref()
            .and_then(|d| d.strip_prefix("sha256:"))
            .map(str::to_ascii_lowercase),
    };
    match expected {
        Some(e) if e != sha256 => Err(Error::Other(format!(
            "{}: SHA-256 is {sha256}, expected {e}",
            asset.name
        ))),
        _ => Ok(()),
    }
}

pub fn mirror_url(release: &Release, asset: &Asset) -> Option<String> {
    asset
        .browser_download_url
        .starts_with(CDN_HOST)
        .then(|| format!("{RELEASES_URL}/download/{}/{}", release.tag_name, asset.name))
}

pub fn download_release(
    release: &Release,
    asset: &Asset,
    to: &Path,
    progress: &dyn Fn(u64),
) -> Result<(u64, String)> {
    download_or(asset, mirror_url(release, asset).as_deref(), to, progress)
}

pub fn download_or(
    asset: &Asset,
    mirror: Option<&str>,
    to: &Path,
    progress: &dyn Fn(u64),
) -> Result<(u64, String)> {
    let first = download(asset, to, progress);
    match (first, mirror) {
        (Err(first), Some(url)) => {
            let other = Asset {
                browser_download_url: url.to_owned(),
                ..asset.clone()
            };
            download(&other, to, progress).map_err(|e| Error::Other(format!("{e} ({first})")))
        }
        (first, _) => first,
    }
}

pub fn verifiable(release: &Release, asset: &Asset) -> bool {
    release.sums().is_some() || asset.digest.as_deref().is_some_and(|d| d.starts_with("sha256:"))
}

/// Downloads `asset` to `to` and returns its length and SHA-256. A download
/// the connection breaks off resumes from the bytes it has.
pub fn download(asset: &Asset, to: &Path, progress: &dyn Fn(u64)) -> Result<(u64, String)> {
    let url = &asset.browser_download_url;
    let too_long = || {
        format!(
            "{}: larger than the {} bytes the release lists",
            asset.name, asset.size
        )
    };
    let fetch = transfer::Fetch {
        url,
        size: asset.size,
        lister: "the release",
        too_long: &too_long,
        resume: None,
        log: None,
    };
    let mut open = |from: u64| {
        let left = asset.size.saturating_sub(from);
        let body_timeout = crate::catalog::body_timeout(left);
        let request = || {
            let mut request = builder(url, false, None)
                .config()
                .timeout_recv_body(Some(body_timeout))
                .build()
                .header("Accept-Encoding", "identity");
            if from > 0 {
                request = request.header("Range", format!("bytes={from}-"));
            }
            request.call()
        };
        let (result, attempts) = transfer::send(request);
        let note = transfer::attempts_note(attempts);
        let response = result.map_err(transfer::Opening::Call)?;
        answered(url, response).map_err(|f| {
            transfer::Opening::Refused(match f {
                Failure::Failed(text) => Error::Other(format!("{text}{note}")),
                wait => wait.into(),
            })
        })
    };
    transfer::fetch(&fetch, to, &mut open, progress)
}
