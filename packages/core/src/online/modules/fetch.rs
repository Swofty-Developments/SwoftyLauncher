//! The one way the launcher gets a signed file the server publishes, for the server modules and the integrity
//! module alike. Each kind says how its offer is asked for, picked and signed through [`Source`]; the steps
//! are the same for all of them:
//!
//! 1. ask for the offer; with no cached copy to fall back on, again after a short wait while the server can't
//!    be reached or is busy ([`Retry`]);
//! 2. check the offer's fields and its Ed25519 signature against the pinned key;
//! 3. use the cached copy when it has the offered size and sha256;
//! 4. else download it from the offer's `cdn_url`, when it has one the launcher follows and the bytes there
//!    verify, else from the server;
//! 5. when the server can't be reached for the offer or the download, the newest cached copy whose stored
//!    offer and bytes still verify.
//!
//! The offer is kept as `offer.json` beside each cached copy so step 5 can verify it again offline.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ring::signature::{ED25519, UnparsedPublicKey};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::Unavailable;
use crate::game_folder::baseline::sha256_bytes;
use crate::online::server::api::{Api, ApiError};
use crate::online::server::integrity::{decode, valid_version};

pub(crate) const OFFER_FILE: &str = "offer.json";
const CDN_TIMEOUT: Duration = Duration::from_secs(60);
const CDN_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const KEEP: usize = 2;

/// The fields of an offer the fetch itself needs.
pub(crate) trait Signed: Serialize + DeserializeOwned + Clone {
    fn version(&self) -> &str;
    fn sha256(&self) -> &str;
    fn size(&self) -> u64;
    /// The name the file is kept under in its version's folder.
    fn file(&self) -> &str;
    fn cdn_url(&self) -> Option<&str>;
}

/// One kind of signed file.
pub(crate) trait Source {
    type Offer: Signed;
    /// The folder that holds one folder per cached version.
    fn base(&self, data: &Path) -> PathBuf;
    /// The server's answer about what it offers.
    fn ask(&self, api: &Api, timeout: Duration) -> Result<Value, ApiError>;
    /// The offer in that answer, or why there is none.
    fn pick(&self, answer: Value) -> Result<Self::Offer, Unavailable>;
    /// Why the server's refusal to answer means there is no file, for an error that is not [`unreachable`].
    fn refused(&self, e: ApiError) -> Unavailable {
        Unavailable::answered(e)
    }
    /// Everything about an offer that can be judged without the file, the signature included. It is also
    /// what a stored offer must pass for its cached copy to be used offline.
    fn check(&self, key: &[u8], offer: &Self::Offer) -> Result<(), Unavailable>;
    fn download(&self, api: &Api, offer: &Self::Offer, timeout: Duration) -> Result<Vec<u8>, ApiError>;
    fn download_timeout(&self) -> Duration;
    /// How long after the server last offered a version its cached copy may still stand in for the server's.
    fn max_age(&self) -> Option<Duration> {
        None
    }
}

/// A verified file: its version, where it is kept and its sha256, and when the server's copy could not be had,
/// why (the file is then the newest cached copy that still verifies).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
    pub fallback: Option<String>,
    pub origin: Origin,
}

/// Where the bytes of a [`Fetched`] file came from this time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    Server,
    Cdn,
    Cache,
}

/// How the offer and the download are asked for again: one more try after each `backoff` wait, while the time
/// since the first try leaves room for the wait and `min_try` within `budget`. A later try gets what is left of
/// the budget as its timeout, at most the call's usual one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retry {
    pub backoff: Vec<Duration>,
    pub budget: Duration,
    pub min_try: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            backoff: vec![Duration::from_secs(1), Duration::from_secs(2)],
            budget: Duration::from_secs(12),
            min_try: Duration::from_secs(2),
        }
    }
}

impl Retry {
    /// One try only.
    pub fn none() -> Self {
        Self {
            backoff: Vec::new(),
            budget: Duration::ZERO,
            min_try: Duration::ZERO,
        }
    }
}

/// Whether the server gave no answer, as opposed to refusing: not reached, timed out, rate limited or busy.
pub(crate) fn unreachable(e: &ApiError) -> bool {
    matches!(e.status, 0 | 408 | 429) || e.status >= 500
}

fn retryable(e: &ApiError) -> bool {
    matches!(e.status, 0 | 408) || e.status >= 500
}

/// `call` with the `usual` timeout, then again as `retry` allows while it fails with a [`retryable`] error.
fn with_retry<T>(
    what: &str,
    retry: &Retry,
    started: Instant,
    usual: Duration,
    out: &dyn Fn(&str),
    mut call: impl FnMut(Duration) -> Result<T, ApiError>,
) -> Result<T, ApiError> {
    let tries = retry.backoff.len() + 1;
    let mut timeout = usual;
    for n in 0.. {
        let e = match call(timeout) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        let Some(wait) = retry.backoff.get(n).copied().filter(|_| retryable(&e)) else {
            return Err(e);
        };
        let left = retry.budget.saturating_sub(started.elapsed());
        if left < wait + retry.min_try {
            return Err(e);
        }
        out(&format!(
            "{what} failed ({}); trying again in {} s (try {} of {tries})",
            e.message,
            wait.as_millis() as f64 / 1000.0,
            n + 2
        ));
        std::thread::sleep(wait);
        timeout = usual.min(left - wait);
    }
    unreachable!("the loop returns")
}

/// The verified file `source` offers, by the steps in the module documentation. `what` names it in the lines
/// `out` gets for each try again.
pub(crate) fn ensure<S: Source>(
    data: &Path,
    api: &Api,
    source: &S,
    key: &[u8],
    retry: &Retry,
    what: &str,
    out: &dyn Fn(&str),
) -> Result<Fetched, Unavailable> {
    let started = Instant::now();
    let base = source.base(data);
    let kept = cached(source, &base, key);
    let none = Retry::none();
    let retry = if kept.is_some() { &none } else { retry };
    let offline = |e: ApiError| match kept.clone() {
        Some(mut found) => {
            found.fallback = Some(e.message);
            Ok(found)
        }
        None => Err(Unavailable::unreachable(e.message)),
    };
    let answer = match with_retry(
        &format!("{what}: asking for the offer"),
        retry,
        started,
        CALL,
        out,
        |t| source.ask(api, t),
    ) {
        Ok(v) => v,
        Err(e) if unreachable(&e) => return offline(e),
        Err(e) => return Err(source.refused(e)),
    };
    let offer = source.pick(answer)?;
    source.check(key, &offer)?;
    let folder = base.join(offer.version());
    let path = folder.join(offer.file());
    let mut origin = Origin::Cache;
    if !matches(&path, &offer) {
        origin = Origin::Cdn;
        let bytes = match from_cdn(&offer) {
            Some(bytes) => bytes,
            None => match with_retry(
                &format!("{what}: downloading it"),
                retry,
                started,
                source.download_timeout(),
                out,
                |t| source.download(api, &offer, t),
            ) {
                Ok(bytes) => {
                    origin = Origin::Server;
                    bytes
                }
                Err(e) if unreachable(&e) => return offline(e),
                Err(e) => return Err(Unavailable::answered(e)),
            },
        };
        if bytes.len() as u64 != offer.size() {
            return Err(format!(
                "the download is {} bytes, the offer says {}",
                bytes.len(),
                offer.size()
            )
            .into());
        }
        if sha256_bytes(&bytes) != offer.sha256() {
            return Err("the download does not have the offered sha256".into());
        }
        store(&folder, offer.file(), &bytes).map_err(|e| format!("storing the file: {e}"))?;
    }
    let noted = serde_json::to_vec_pretty(&offer)
        .map_err(std::io::Error::other)
        .and_then(|doc| store(&folder, OFFER_FILE, &doc));
    if let Err(e) = noted {
        return Err(format!("storing the file: {e}").into());
    }
    prune(&base, offer.version());
    Ok(Fetched {
        version: offer.version().to_owned(),
        path,
        sha256: offer.sha256().to_owned(),
        fallback: None,
        origin,
    })
}

/// The usual timeout of the offer request; the source's own call caps it again.
const CALL: Duration = Duration::from_secs(4);

/// The newest cached version under `base` whose stored offer passes `source`'s checks, was offered within
/// `source`'s [`Source::max_age`], and whose file has the offered size and sha256. The stored offer is written
/// again each time the server offers that version, so its modified time is when it was last offered.
pub(crate) fn cached<S: Source>(source: &S, base: &Path, key: &[u8]) -> Option<Fetched> {
    let fresh = |kept: &Path| {
        source.max_age().is_none_or(|max| {
            std::fs::metadata(kept)
                .and_then(|m| m.modified())
                .is_ok_and(|at| at.elapsed().map_or(true, |age| age <= max))
        })
    };
    let mut found: Vec<(Vec<u64>, Fetched)> = std::fs::read_dir(base)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let folder = entry.path();
            let kept = folder.join(OFFER_FILE);
            let offer: S::Offer = serde_json::from_slice(&std::fs::read(&kept).ok()?).ok()?;
            if entry.file_name().to_str() != Some(offer.version())
                || source.check(key, &offer).is_err()
                || !fresh(&kept)
            {
                return None;
            }
            let path = folder.join(offer.file());
            matches(&path, &offer).then(|| {
                (
                    order(offer.version()),
                    Fetched {
                        version: offer.version().to_owned(),
                        path,
                        sha256: offer.sha256().to_owned(),
                        fallback: None,
                        origin: Origin::Cache,
                    },
                )
            })
        })
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.version.cmp(&b.1.version)));
    found.pop().map(|(_, m)| m)
}

/// Whether `signature` (base64) is the pinned `key`'s over `message`.
pub(crate) fn signed_by(key: &[u8], message: &[u8], signature: &str) -> Result<(), Unavailable> {
    let signature = decode(signature).ok_or("the signature is not valid base64")?;
    UnparsedPublicKey::new(&ED25519, key)
        .verify(message, &signature)
        .map_err(|_| "the signature does not match the pinned key".into())
}

/// The checks every offer's own fields get: a plain version, a lowercase sha256 and a size within `max`.
pub(crate) fn plain(version: &str, sha256: &str, size: u64, max: u64) -> Result<(), Unavailable> {
    if !valid_version(version) {
        return Err("the offered version is not a plain version string".into());
    }
    if sha256.len() != 64 || !sha256.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return Err("the offered hash is not a lowercase sha256".into());
    }
    if size == 0 || size > max {
        return Err(format!("the offered size of {size} bytes is out of range").into());
    }
    Ok(())
}

/// The offered file from its `cdn_url`, when the offer has one the launcher follows and the bytes there have
/// the offered size and sha256. No sign-in is sent. `None` sends the launcher to the server instead.
fn from_cdn<O: Signed>(offer: &O) -> Option<Vec<u8>> {
    let url = offer.cdn_url().filter(|url| cdn_link(url))?;
    let mut response = ureq::get(url)
        .header("User-Agent", "swofty-launcher")
        .header("Accept-Encoding", "identity")
        .config()
        .timeout_global(Some(CDN_TIMEOUT))
        .timeout_connect(Some(CDN_CONNECT_TIMEOUT))
        .max_redirects(0)
        .build()
        .call()
        .ok()?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(offer.size() + 1)
        .read_to_vec()
        .ok()?;
    (bytes.len() as u64 == offer.size() && sha256_bytes(&bytes) == offer.sha256()).then_some(bytes)
}

/// A CDN link the launcher follows: https, or plain http on this machine.
pub(crate) fn cdn_link(url: &str) -> bool {
    url.starts_with("https://")
        || (url.starts_with("http://") && crate::online::server::http::bearer_allowed(url))
}

/// A dotted version as numbers, so `1.10` sorts after `1.9`.
fn order(version: &str) -> Vec<u64> {
    version.split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

fn matches<O: Signed>(path: &Path, offer: &O) -> bool {
    std::fs::read(path).is_ok_and(|b| b.len() as u64 == offer.size() && sha256_bytes(&b) == offer.sha256())
}

fn store(folder: &Path, file: &str, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let tmp = folder.join(format!("{file}.{}.tmp", std::process::id()));
    let done = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, folder.join(file)));
    if done.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    done
}

/// Keep `current` and the newest other version; the rest go.
fn prune(base: &Path, current: &str) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    let mut others: Vec<(std::time::SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| valid_version(n) && n != current)
        })
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    others.sort_by_key(|o| std::cmp::Reverse(o.0));
    for (_, path) in others.into_iter().skip(KEEP - 1) {
        std::fs::remove_dir_all(path).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_error(status: u16) -> ApiError {
        ApiError {
            status,
            message: format!("HTTP {status}"),
            retry_after: None,
        }
    }

    fn quick() -> Retry {
        Retry {
            backoff: vec![Duration::from_millis(10), Duration::from_millis(10)],
            budget: Duration::from_secs(5),
            min_try: Duration::from_millis(1),
        }
    }

    #[test]
    fn a_failed_call_is_tried_again_with_a_line_each_and_a_refusal_is_not() {
        let lines = std::cell::RefCell::new(Vec::new());
        let mut calls = 0;
        let got = with_retry(
            "asking",
            &quick(),
            Instant::now(),
            CALL,
            &|l| lines.borrow_mut().push(l.to_owned()),
            |_| {
                calls += 1;
                if calls < 3 {
                    Err(api_error(if calls == 1 { 0 } else { 503 }))
                } else {
                    Ok(7)
                }
            },
        );
        assert_eq!((got.unwrap(), calls), (7, 3));
        let lines = lines.into_inner();
        assert_eq!(lines.len(), 2);
        assert!(
            lines[0].starts_with("asking failed (HTTP 0); trying again in 0.01 s (try 2 of 3)"),
            "{lines:?}"
        );

        let mut calls = 0;
        let got: Result<(), _> = with_retry("asking", &quick(), Instant::now(), CALL, &|_| {}, |_| {
            calls += 1;
            Err(api_error(404))
        });
        assert_eq!((got.unwrap_err().status, calls), (404, 1));

        let mut calls = 0;
        let got: Result<(), _> = with_retry("asking", &quick(), Instant::now(), CALL, &|_| {}, |_| {
            calls += 1;
            Err(api_error(429))
        });
        assert_eq!((got.unwrap_err().status, calls), (429, 1));
    }

    #[test]
    fn tries_end_with_the_last_wait_or_the_budget() {
        let mut calls = 0;
        let got: Result<(), _> = with_retry("asking", &quick(), Instant::now(), CALL, &|_| {}, |_| {
            calls += 1;
            Err(api_error(0))
        });
        assert!(got.is_err());
        assert_eq!(calls, 3);

        let tight = Retry {
            budget: Duration::from_millis(5),
            ..quick()
        };
        let mut calls = 0;
        let _: Result<(), _> = with_retry("asking", &tight, Instant::now(), CALL, &|_| {}, |_| {
            calls += 1;
            Err(api_error(0))
        });
        assert_eq!(calls, 1);

        let mut timeouts = Vec::new();
        let _: Result<(), _> = with_retry("asking", &quick(), Instant::now(), CALL, &|_| {}, |t| {
            timeouts.push(t);
            Err(api_error(0))
        });
        assert_eq!(timeouts[0], CALL);
        assert!(timeouts[1..].iter().all(|t| *t <= CALL));
    }

    #[test]
    fn the_default_tries_stay_within_the_budget() {
        let r = Retry::default();
        assert_eq!(r.backoff.len(), 2);
        assert!(r.budget <= Duration::from_secs(12));
        assert!(Retry::none().backoff.is_empty());
    }
}
