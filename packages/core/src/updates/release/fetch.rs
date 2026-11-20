//! Asking GitHub and the CDN about releases, and the refusals they answer with.

use std::time::Duration;

use crate::updates::transfer;
use crate::{Error, Result};

#[cfg(debug_assertions)]
use super::{CDN_HOST, FIXTURE_ENV};
use super::{LATEST_URL, MANIFEST_URL, Manifest, Release, now_secs};

pub(super) fn user_agent() -> String {
    format!(
        "swofty-launcher/{} (+https://github.com/Swofty-Developments/SwoftyLauncher)",
        env!("CARGO_PKG_VERSION")
    )
}

pub fn endpoint(url: &str) -> String {
    #[cfg(debug_assertions)]
    if let Some(base) = std::env::var(FIXTURE_ENV).ok().filter(|b| !b.is_empty()) {
        let base = base.trim_end_matches('/');
        for host in ["https://api.github.com", "https://github.com"] {
            if let Some(rest) = url.strip_prefix(host) {
                return format!("{base}{rest}");
            }
        }
        if let Some(rest) = url.strip_prefix(CDN_HOST) {
            return format!("{base}/cdn{rest}");
        }
    }
    url.to_owned()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Failure {
    Wait(u64, String),
    Failed(String),
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Wait(_, text) | Self::Failed(text) => f.write_str(text),
        }
    }
}

impl From<Failure> for Error {
    fn from(f: Failure) -> Self {
        Error::Other(f.to_string())
    }
}

pub fn in_minutes(secs: u64) -> String {
    match secs.div_ceil(60) {
        0 | 1 => "a minute".into(),
        n => format!("{n} minutes"),
    }
}

pub fn refusal(
    url: &str,
    status: u16,
    retry_after: Option<&str>,
    remaining: Option<&str>,
    reset: Option<&str>,
    now: u64,
) -> Failure {
    let number = |v: Option<&str>| v.and_then(|v| v.trim().parse::<u64>().ok());
    let wait = number(retry_after).or_else(|| {
        (remaining.map(str::trim) == Some("0"))
            .then(|| number(reset).map(|at| at.saturating_sub(now)))
            .flatten()
    });
    match wait {
        Some(secs) => Failure::Wait(
            secs.max(1),
            format!(
                "{url}: GitHub's rate limit is reached; try again in {}",
                in_minutes(secs)
            ),
        ),
        None if status == 429 => Failure::Wait(
            60,
            format!("{url}: GitHub's rate limit is reached; try again in a minute"),
        ),
        None => Failure::Failed(format!("{url}: HTTP {status}")),
    }
}

pub(super) fn builder(
    url: &str,
    head: bool,
    timeout: Option<Duration>,
) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
    let target = endpoint(url);
    let builder = if head {
        ureq::head(&target)
    } else {
        ureq::get(&target)
    }
    .header("User-Agent", user_agent())
    .header("Accept", "application/vnd.github+json, application/octet-stream")
    .config()
    .timeout_global(timeout)
    .timeout_connect(Some(Duration::from_secs(15)))
    .timeout_recv_response(Some(Duration::from_secs(30)))
    .http_status_as_error(false);
    let config = if head { builder.max_redirects(0) } else { builder };
    config.build()
}

pub(super) fn answered(
    url: &str,
    response: ureq::http::Response<ureq::Body>,
) -> std::result::Result<ureq::http::Response<ureq::Body>, Failure> {
    let status = response.status().as_u16();
    if status < 400 {
        return Ok(response);
    }
    let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok());
    Err(refusal(
        url,
        status,
        header("retry-after"),
        header("x-ratelimit-remaining"),
        header("x-ratelimit-reset"),
        now_secs(),
    ))
}

pub fn request(
    url: &str,
    head: bool,
    timeout: Option<Duration>,
) -> std::result::Result<ureq::http::Response<ureq::Body>, Failure> {
    let (result, attempts) = transfer::send(|| builder(url, head, timeout).call());
    let note = transfer::attempts_note(attempts);
    let response = result.map_err(|e| Failure::Failed(format!("{url}: {e}{note}")))?;
    answered(url, response).map_err(|f| match f {
        Failure::Failed(text) => Failure::Failed(format!("{text}{note}")),
        wait => wait,
    })
}

pub fn get(url: &str, timeout: Option<Duration>) -> Result<ureq::Body> {
    Ok(request(url, false, timeout)?.into_body())
}

pub fn get_text(url: &str, timeout: Duration) -> Result<String> {
    get(url, Some(timeout))?
        .read_to_string()
        .map_err(|e| Error::Other(format!("{url}: {e}")))
}

pub fn fetch_json<T: serde::de::DeserializeOwned>(
    url: &str,
    timeout: Duration,
) -> std::result::Result<T, Failure> {
    let text = request(url, false, Some(timeout))?
        .into_body()
        .read_to_string()
        .map_err(|e| Failure::Failed(format!("{url}: {e}")))?;
    serde_json::from_str(&text).map_err(|e| Failure::Failed(format!("{url}: {e}")))
}

pub fn fetch_release(url: &str, timeout: Duration) -> std::result::Result<Release, Failure> {
    fetch_json(url, timeout)
}

pub fn fetch_manifest(url: &str, timeout: Duration) -> std::result::Result<Release, Failure> {
    fetch_json::<Manifest>(url, timeout)?
        .release()
        .map_err(|why| Failure::Failed(format!("{url}: {why}")))
}

pub fn fetch_latest_from(
    manifest_url: &str,
    github_url: &str,
    timeout: Duration,
) -> std::result::Result<Release, Failure> {
    let cdn = match fetch_manifest(manifest_url, timeout) {
        Ok(release) => return Ok(release),
        Err(cdn) => cdn,
    };
    fetch_release(github_url, timeout).map_err(|github| match github {
        Failure::Failed(text) => Failure::Failed(format!("{text} ({cdn})")),
        wait => wait,
    })
}

pub fn fetch_latest(timeout: Duration) -> Result<Release> {
    Ok(fetch_latest_from(MANIFEST_URL, LATEST_URL, timeout)?)
}
