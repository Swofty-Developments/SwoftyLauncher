//! The integrity module: a closed-source native addon the Time Trial boards server publishes, which
//! appends an attestation to every lap while the player is signed in with Time Trial boards on.
//!
//! The server offers one module at a time at `GET /v1/tt/integrity`. [`ensure`] keeps a verified copy
//! under `<data>/timetrial/integrity/<version>/`, with the offer it came with and when the server last offered
//! it, and nothing is placed in the game folder unless the offer's signature checks out against the key pinned
//! here. The offer and the download are tried again on timeouts, connection errors and 5xx answers within
//! [`Retry`]'s budget. When the server's module cannot be had, the newest cached copy whose signature, hash and
//! age still check out is used. Every failure is an `Err` with one line of text for the log; the session then
//! hands the core [`BLOCKED_KEY`] and the core blocks Time Trial.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::baseline::sha256_bytes;
use crate::timetrial::api::{Api, ApiError};

/// The addon id the module loads under. It is not a user addon: no profile, addon list or fingerprint has it.
pub const PLUGIN_ID: &str = "swofty.integrity";
pub const DLL: &str = "swofty_integrity.dll";
/// Base64 of the Ed25519 public key whose private half signs published modules.
const PUBLIC_KEY: &str = "Y79gick8AdEJDzKoMDuiVdEV5UYuc2xgxJ+V7VGgZxg=";
const SIGNED_PREFIX: &str = "swofty-tt-integrity-v1";
const MAX_SIZE: u64 = 16 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
const KEEP: usize = 2;
const RECORD: &str = "offer.json";
const REASON_MAX: usize = 160;
/// A cached copy is used while the server last offered it at most this long ago: the server accepts a version
/// for 14 days after a newer one is published.
pub const CACHE_MAX_AGE_S: u64 = 14 * 24 * 60 * 60;
/// The [`super::SESSION_FILE`] key naming the module the session placed: `{"version", "dll"}`.
pub const MODULE_KEY: &str = "tt_module";
/// The [`super::SESSION_FILE`] key that blocks Time Trial in game: `{"reason"}`.
pub const BLOCKED_KEY: &str = "tt_blocked";
/// Names a locally built module to place for the session in place of the server's, for testing a build
/// before it is published. Unset in normal use.
pub const LOCAL_ENV: &str = "SWOFTY_TT_INTEGRITY_DLL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub version: String,
    pub dll: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Offer {
    version: String,
    sha256: String,
    size: u64,
    signature: String,
}

/// What is kept beside a cached module: its offer, to verify it again offline, and when the server last offered it.
#[derive(Serialize, Deserialize)]
struct Kept {
    #[serde(flatten)]
    offer: Offer,
    offered: u64,
}

/// A cached module that verified offline, and when the server last offered it (unix seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cached {
    pub module: Module,
    pub offered: u64,
}

/// How the offer and the download are tried again: one more try after each `backoff` wait, while the time since
/// the first try leaves the wait and `min_try` within `budget`. A try after a wait gets what is left of the budget
/// as its timeout, at most the call's usual one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Retry {
    pub backoff: Vec<Duration>,
    pub budget: Duration,
    pub min_try: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            backoff: vec![Duration::from_secs(1), Duration::from_secs(3)],
            budget: Duration::from_secs(18),
            min_try: Duration::from_secs(2),
        }
    }
}

/// Whether a failed call is worth another try: the server was not reached, timed out or answered 5xx.
pub fn retryable(e: &ApiError) -> bool {
    e.status == 0 || (500..600).contains(&e.status)
}

/// What a session did about the module, for the core and the launcher.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum State {
    /// Time Trial boards is off, the player is not signed in, the profile runs no core, or no build was installed.
    #[default]
    Off,
    /// This version was placed.
    Placed(String),
    /// None was placed, for this reason: Time Trial is blocked in game.
    Missing(String),
}

impl State {
    /// The state of a session's build: `fetched` is `None` when Time Trial boards is off or the player is not signed
    /// in, and `core` is whether the build places the loader at all (a vanilla profile does not).
    pub fn of(fetched: Option<&Result<Module, String>>, core: bool) -> Self {
        match fetched {
            Some(Ok(m)) if core => Self::Placed(m.version.clone()),
            Some(Err(why)) if core => Self::Missing(short(why)),
            _ => Self::Off,
        }
    }

    pub fn blocked(&self) -> Option<&str> {
        match self {
            Self::Missing(why) => Some(why),
            _ => None,
        }
    }
}

fn short(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(REASON_MAX) {
        Some((cut, _)) => format!("{}...", &text[..cut]),
        None => text.to_owned(),
    }
}

pub fn dir(data: &Path) -> PathBuf {
    data.join("timetrial").join("integrity")
}

/// The file [`LOCAL_ENV`] names, when it is set: `None` when it is unset or empty, else that file as a
/// module with the version `local`, or why it cannot be used. No server is asked and no signature is checked.
pub fn local() -> Option<Result<Module, String>> {
    let path = std::env::var_os(LOCAL_ENV).filter(|p| !p.is_empty())?;
    Some(local_at(Path::new(&path)))
}

fn local_at(path: &Path) -> Result<Module, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_SIZE || !bytes.starts_with(b"MZ") {
        return Err(format!("{} is not a Windows DLL under 16 MiB", path.display()));
    }
    Ok(Module {
        version: "local".to_owned(),
        dll: path.to_path_buf(),
        sha256: sha256_bytes(&bytes),
    })
}

/// The verified module for this session: the cached copy when it has the offered hash, else a download, else
/// the newest cached copy that verifies offline. `out` gets a line for each try again and for a cached copy used.
pub fn ensure(data: &Path, api: &Api, out: &dyn Fn(&str)) -> Result<Module, String> {
    let key = decode(PUBLIC_KEY).ok_or("the pinned integrity key is not valid base64")?;
    ensure_with(
        data,
        api,
        &key,
        &Retry::default(),
        crate::timetrial::queue::unix_now(),
        out,
    )
}

fn ensure_with(
    data: &Path,
    api: &Api,
    key: &[u8],
    retry: &Retry,
    now: u64,
    out: &dyn Fn(&str),
) -> Result<Module, String> {
    let why = match fetch(data, api, key, retry, now, out) {
        Ok(m) => return Ok(m),
        Err(why) => why,
    };
    let Some(c) = cached_with(data, key, now) else {
        return Err(why);
    };
    out(&format!(
        "integrity module: the server's copy could not be had ({why}); using the cached copy {}, verified offline, last offered {} ago",
        c.module.version,
        age(now.saturating_sub(c.offered))
    ));
    Ok(c.module)
}

fn age(secs: u64) -> String {
    match secs {
        0..3600 => format!("{} min", secs / 60),
        3600..172_800 => format!("{} h", secs / 3600),
        _ => format!("{} days", secs / 86_400),
    }
}

fn seconds(d: Duration) -> String {
    format!("{}", d.as_millis() as f64 / 1000.0)
}

/// `call` with the call's `usual` timeout, and again as [`Retry`] allows while it fails with a [`retryable`] error.
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
    for n in 1.. {
        let e = match call(timeout) {
            Ok(v) => return Ok(v),
            Err(e) => e,
        };
        let Some(wait) = retry.backoff.get(n - 1).copied().filter(|_| retryable(&e)) else {
            return Err(e);
        };
        let left = retry.budget.saturating_sub(started.elapsed());
        if left < wait + retry.min_try {
            return Err(e);
        }
        out(&format!(
            "integrity module: {what} failed ({}); trying again in {} s (try {} of {tries})",
            e.message,
            seconds(wait),
            n + 1
        ));
        std::thread::sleep(wait);
        timeout = usual.min(left - wait);
    }
    unreachable!("the loop returns")
}

fn check(offer: &Offer) -> Result<(), String> {
    if !valid_version(&offer.version) {
        return Err("the offered version is not a plain version string".into());
    }
    if offer.sha256.len() != 64
        || !offer
            .sha256
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("the offered hash is not a lowercase sha256".into());
    }
    if offer.size == 0 || offer.size > MAX_SIZE {
        return Err(format!(
            "the offered size of {} bytes is out of range",
            offer.size
        ));
    }
    Ok(())
}

fn fetch(
    data: &Path,
    api: &Api,
    key: &[u8],
    retry: &Retry,
    now: u64,
    out: &dyn Fn(&str),
) -> Result<Module, String> {
    let started = Instant::now();
    let offer = with_retry(
        "asking the server for the module",
        retry,
        started,
        Duration::from_secs(4),
        out,
        |t| api.integrity(t),
    )
    .map_err(|e| e.message)?;
    let offer: Offer = serde_json::from_value(offer).map_err(|e| format!("the offer is not valid: {e}"))?;
    check(&offer)?;
    verify(key, &offer)?;
    let folder = dir(data).join(&offer.version);
    let dll = folder.join(DLL);
    let module = Module {
        version: offer.version.clone(),
        dll: dll.clone(),
        sha256: offer.sha256.clone(),
    };
    if std::fs::read(&dll).is_ok_and(|b| b.len() as u64 == offer.size && sha256_bytes(&b) == offer.sha256) {
        keep(&folder, &offer, now);
        prune(data, &offer.version);
        return Ok(module);
    }
    let bytes = with_retry(
        "downloading the module",
        retry,
        started,
        DOWNLOAD_TIMEOUT,
        out,
        |t| api.integrity_file(&offer.version, offer.size, t),
    )
    .map_err(|e| e.message)?;
    if bytes.len() as u64 != offer.size {
        return Err(format!(
            "the download is {} bytes, the offer says {}",
            bytes.len(),
            offer.size
        ));
    }
    if sha256_bytes(&bytes) != offer.sha256 {
        return Err("the download does not have the offered sha256".into());
    }
    store(&folder, &dll, &bytes).map_err(|e| format!("storing the module: {e}"))?;
    keep(&folder, &offer, now);
    prune(data, &offer.version);
    Ok(module)
}

fn keep(folder: &Path, offer: &Offer, now: u64) {
    let kept = Kept {
        offer: offer.clone(),
        offered: now,
    };
    crate::write_json_shared(&folder.join(RECORD), &kept).ok();
}

/// The newest cached module whose kept offer verifies against the pinned key, whose bytes have the offered size
/// and hash, and that the server offered at most [`CACHE_MAX_AGE_S`] before `now`.
pub fn cached(data: &Path, now: u64) -> Option<Cached> {
    cached_with(data, &decode(PUBLIC_KEY)?, now)
}

fn cached_with(data: &Path, key: &[u8], now: u64) -> Option<Cached> {
    std::fs::read_dir(dir(data))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            valid_version(&name).then(|| verified(&e.path(), &name, key, now))?
        })
        .max_by_key(|c| c.offered)
}

fn verified(folder: &Path, version: &str, key: &[u8], now: u64) -> Option<Cached> {
    let kept: Kept = crate::read_json(&folder.join(RECORD)).ok()?;
    let fresh = now.saturating_sub(kept.offered) <= CACHE_MAX_AGE_S
        && kept.offered <= now.saturating_add(CACHE_MAX_AGE_S);
    if kept.offer.version != version || !fresh {
        return None;
    }
    check(&kept.offer).ok()?;
    verify(key, &kept.offer).ok()?;
    let dll = folder.join(DLL);
    let bytes = std::fs::read(&dll).ok()?;
    (bytes.len() as u64 == kept.offer.size && sha256_bytes(&bytes) == kept.offer.sha256).then(|| Cached {
        module: Module {
            version: kept.offer.version.clone(),
            dll,
            sha256: kept.offer.sha256.clone(),
        },
        offered: kept.offered,
    })
}

/// Hand the core the session's [`State`] through [`super::SESSION_FILE`]: [`MODULE_KEY`] when a module was
/// placed, [`BLOCKED_KEY`] when none was, neither when off. Other keys stay as they are.
pub fn write_session(game: &Path, state: &State) -> std::io::Result<()> {
    super::edit_session(game, |doc| {
        doc.remove(MODULE_KEY);
        doc.remove(BLOCKED_KEY);
        match state {
            State::Off => {}
            State::Placed(version) => {
                doc.insert(MODULE_KEY.into(), json!({"version": version, "dll": DLL}));
            }
            State::Missing(why) => {
                doc.insert(BLOCKED_KEY.into(), json!({"reason": why}));
            }
        }
    })
}

/// Keep, in the data folder, the reason Time Trial is blocked for the running session, for the launcher; the note
/// goes when the state is not [`State::Missing`].
pub fn note(data: &Path, state: &State) {
    let path = note_path(data);
    match state.blocked() {
        Some(why) => {
            crate::write_json_shared(&path, &json!({"reason": why})).ok();
        }
        None => {
            std::fs::remove_file(path).ok();
        }
    }
}

fn note_path(data: &Path) -> PathBuf {
    data.join("timetrial").join("integrity-blocked.json")
}

/// [`note`]'s reason, when one is kept.
pub fn noted(data: &Path) -> Option<String> {
    let doc: Value = crate::read_json(&note_path(data)).ok()?;
    Some(
        doc.get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    )
}

fn verify(key: &[u8], offer: &Offer) -> Result<(), String> {
    let signature = decode(&offer.signature).ok_or("the signature is not valid base64")?;
    let mut message = Vec::new();
    message.extend_from_slice(SIGNED_PREFIX.as_bytes());
    message.push(0);
    message.extend_from_slice(offer.version.as_bytes());
    message.push(0);
    message.extend_from_slice(offer.sha256.as_bytes());
    UnparsedPublicKey::new(&ED25519, key)
        .verify(&message, &signature)
        .map_err(|_| "the signature does not match the pinned key".to_owned())
}

/// A version becomes a folder name, so it is plain characters only.
pub fn valid_version(v: &str) -> bool {
    (1..=32).contains(&v.len())
        && !v.starts_with('.')
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn store(folder: &Path, dll: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let tmp = folder.join(format!("{DLL}.{}.tmp", std::process::id()));
    let done = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, dll));
    if done.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    done
}

/// Keep `current` and the newest other version; the rest go.
fn prune(data: &Path, current: &str) {
    let Ok(entries) = std::fs::read_dir(dir(data)) else {
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

/// Standard base64, padded or not.
fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for b in text.trim_end_matches('=').bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Add `module` to a build's placed files and plugin list so the loader loads it as [`PLUGIN_ID`].
pub fn place(
    module: &Module,
    loader: &mut std::collections::BTreeMap<String, PathBuf>,
    plugins: &mut Vec<String>,
) {
    let rel = format!("{PLUGIN_ID}/{DLL}");
    loader.insert(
        format!(
            "{}/{}/{rel}",
            crate::install::LOADER_DIR,
            crate::build::ADDONS_DIR
        ),
        module.dll.clone(),
    );
    plugins.push(format!("{}/{rel}", crate::build::ADDONS_DIR));
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    const BYTES: &[u8] = b"MZ not really a dll";

    fn pair() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    fn b64(bytes: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in bytes.chunks(3) {
            let n = c
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..=c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
            out.extend(std::iter::repeat_n('=', 3 - c.len()));
        }
        out
    }

    fn sign(key: &Ed25519KeyPair, version: &str, sha: &str) -> String {
        let msg = format!("{SIGNED_PREFIX}\0{version}\0{sha}");
        b64(key.sign(msg.as_bytes()).as_ref())
    }

    fn offer(key: &Ed25519KeyPair, version: &str, bytes: &[u8]) -> serde_json::Value {
        let sha = sha256_bytes(bytes);
        serde_json::json!({
            "version": version, "sha256": sha, "size": bytes.len(),
            "signature": sign(key, version, &sha),
            "url": format!("/v1/tt/integrity/file?version={version}"),
        })
    }

    type Route = (&'static str, &'static str, Vec<u8>);

    const DROP: &str = "drop";
    const NOW: u64 = 1_800_000_000;

    fn fast() -> Retry {
        Retry {
            backoff: vec![Duration::from_millis(10), Duration::from_millis(10)],
            budget: Duration::from_secs(10),
            min_try: Duration::from_millis(10),
        }
    }

    fn serve(
        mut routes: Vec<Route>,
        requests: usize,
    ) -> (String, Arc<Mutex<Vec<String>>>, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = seen.clone();
        let t = std::thread::spawn(move || {
            for _ in 0..requests {
                let (mut c, _) = listener.accept().unwrap();
                let mut got = Vec::new();
                let mut buf = [0u8; 4096];
                while !got.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = c.read(&mut buf).unwrap();
                    if n == 0 {
                        break;
                    }
                    got.extend_from_slice(&buf[..n]);
                }
                let line = String::from_utf8_lossy(&got)
                    .lines()
                    .next()
                    .unwrap_or("")
                    .to_owned();
                let path = line.split(' ').nth(1).unwrap_or("").to_owned();
                log.lock().unwrap().push(path.clone());
                let at = routes.iter().position(|(p, _, _)| path.starts_with(p));
                let (status, body) = at
                    .map_or(("404 Not Found", br#"{"error":"nothing here"}"#.to_vec()), |i| {
                        (routes[i].1, routes[i].2.clone())
                    });
                if let Some(i) = at
                    && routes[i + 1..].iter().any(|r| r.0 == routes[i].0)
                {
                    routes.remove(i);
                }
                if status == DROP {
                    continue;
                }
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                c.write_all(head.as_bytes()).ok();
                c.write_all(&body).ok();
            }
        });
        (url, seen, t)
    }

    fn json(v: &serde_json::Value) -> Vec<u8> {
        v.to_string().into_bytes()
    }

    fn data(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("swofty-integ-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        d
    }

    fn run(
        name: &str,
        key: &Ed25519KeyPair,
        routes: Vec<Route>,
        requests: usize,
    ) -> (Result<Module, String>, PathBuf, Vec<String>) {
        let d = data(name);
        let (url, seen, t) = serve(routes, requests);
        let got = ensure_with(
            &d,
            &Api::with_server(&url, "tok"),
            key.public_key().as_ref(),
            &fast(),
            NOW,
            &|_| {},
        );
        t.join().unwrap();
        let seen = seen.lock().unwrap().clone();
        (got, d, seen)
    }

    #[test]
    fn the_pinned_key_is_an_ed25519_public_key() {
        assert_eq!(decode(PUBLIC_KEY).unwrap().len(), 32);
        assert_eq!(decode("aGk=").unwrap(), b"hi");
        assert_eq!(decode("aGk").unwrap(), b"hi");
        assert!(decode("a$k=").is_none());
    }

    #[test]
    fn the_pinned_key_accepts_the_servers_signature_format() {
        let offer = Offer {
            version: "kat".to_owned(),
            sha256: "0".repeat(64),
            size: 1,
            signature:
                "zZcTrXukQHWeNKBjlHN+WEge6rdUdGhnGofCPjCQVXy4FyEs2Fs7H0Vugik9LDAeaHmV09oJP3/zQGEYwO2vBw=="
                    .to_owned(),
        };
        verify(&decode(PUBLIC_KEY).unwrap(), &offer).unwrap();
    }

    #[test]
    fn downloads_verifies_and_stores_a_signed_module() {
        let key = pair();
        let routes = vec![
            (
                "/v1/tt/integrity/file?version=2026.11.10.1",
                "200 OK",
                BYTES.to_vec(),
            ),
            (
                "/v1/tt/integrity",
                "200 OK",
                json(&offer(&key, "2026.11.10.1", BYTES)),
            ),
        ];
        let (got, d, seen) = run("good", &key, routes, 2);
        let m = got.unwrap();
        assert_eq!(m.version, "2026.11.10.1");
        assert_eq!(m.sha256, sha256_bytes(BYTES));
        assert_eq!(m.dll, dir(&d).join("2026.11.10.1").join(DLL));
        assert_eq!(std::fs::read(&m.dll).unwrap(), BYTES);
        assert_eq!(
            seen,
            ["/v1/tt/integrity", "/v1/tt/integrity/file?version=2026.11.10.1"]
        );
        assert_eq!(
            std::fs::read_dir(dir(&d).join("2026.11.10.1")).unwrap().count(),
            2
        );
        let kept: Value = crate::read_json(&dir(&d).join("2026.11.10.1").join(RECORD)).unwrap();
        assert_eq!(kept["offered"], NOW);
        assert_eq!(kept["sha256"], sha256_bytes(BYTES));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_cached_copy_with_the_offered_hash_is_not_downloaded_again() {
        let key = pair();
        let d = data("cache");
        let folder = dir(&d).join("v1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(DLL), BYTES).unwrap();
        let (url, seen, t) = serve(
            vec![("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES)))],
            1,
        );
        let m = ensure_with(
            &d,
            &Api::with_server(&url, "tok"),
            key.public_key().as_ref(),
            &fast(),
            NOW,
            &|_| {},
        )
        .unwrap();
        t.join().unwrap();
        assert_eq!(std::fs::read(m.dll).unwrap(), BYTES);
        assert_eq!(*seen.lock().unwrap(), ["/v1/tt/integrity"]);
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_cached_copy_with_other_bytes_is_replaced() {
        let key = pair();
        let d = data("stale");
        let folder = dir(&d).join("v1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(DLL), b"tampered").unwrap();
        let routes = vec![
            ("/v1/tt/integrity/file", "200 OK", BYTES.to_vec()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES))),
        ];
        let (url, _, t) = serve(routes, 2);
        let m = ensure_with(
            &d,
            &Api::with_server(&url, "tok"),
            key.public_key().as_ref(),
            &fast(),
            NOW,
            &|_| {},
        )
        .unwrap();
        t.join().unwrap();
        assert_eq!(std::fs::read(m.dll).unwrap(), BYTES);
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_bad_signature_is_refused_before_any_download() {
        let key = pair();
        let other = pair();
        let routes = vec![("/v1/tt/integrity", "200 OK", json(&offer(&other, "v1", BYTES)))];
        let (got, d, seen) = run("badsig", &key, routes, 1);
        assert!(got.unwrap_err().contains("signature"));
        assert_eq!(seen.len(), 1);
        assert!(!dir(&d).exists());
    }

    #[test]
    fn a_signature_over_another_version_is_refused() {
        let key = pair();
        let mut o = offer(&key, "v1", BYTES);
        o["version"] = "v2".into();
        let (got, ..) = run("swapped", &key, vec![("/v1/tt/integrity", "200 OK", json(&o))], 1);
        assert!(got.unwrap_err().contains("signature"));
    }

    #[test]
    fn a_download_with_the_wrong_hash_is_not_stored() {
        let key = pair();
        let routes = vec![
            ("/v1/tt/integrity/file", "200 OK", b"MZ not really a dlL".to_vec()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES))),
        ];
        let (got, d, _) = run("badhash", &key, routes, 2);
        assert!(got.unwrap_err().contains("sha256"));
        assert!(!dir(&d).join("v1").join(DLL).exists());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_download_longer_than_offered_is_refused() {
        let key = pair();
        let routes = vec![
            ("/v1/tt/integrity/file", "200 OK", [BYTES, b" and more"].concat()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES))),
        ];
        let (got, d, _) = run("long", &key, routes, 2);
        assert!(got.is_err());
        assert!(!dir(&d).join("v1").join(DLL).exists());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn an_offer_over_the_size_limit_is_refused_unfetched() {
        let key = pair();
        let mut o = offer(&key, "v1", BYTES);
        o["size"] = (MAX_SIZE + 1).into();
        let (got, _, seen) = run("big", &key, vec![("/v1/tt/integrity", "200 OK", json(&o))], 1);
        assert!(got.unwrap_err().contains("out of range"));
        assert_eq!(seen.len(), 1);
    }

    #[test]
    fn a_version_that_is_not_a_plain_name_is_refused() {
        let key = pair();
        for v in ["../x", ".hidden", "a/b", "a\\b", "", "x y", &"v".repeat(33)] {
            let (got, d, seen) = run(
                "trav",
                &key,
                vec![("/v1/tt/integrity", "200 OK", json(&offer(&key, v, BYTES)))],
                1,
            );
            assert!(got.unwrap_err().contains("version"), "{v}");
            assert_eq!(seen.len(), 1);
            assert!(!dir(&d).exists());
        }
        assert!(valid_version("2026.11.10.1"));
        assert!(valid_version("a_b-c"));
    }

    #[test]
    fn nothing_is_stored_when_offline_or_unpublished() {
        let d = data("offline");
        let e = ensure_with(
            &d,
            &Api::with_server("http://127.0.0.1:1", "tok"),
            &decode(PUBLIC_KEY).unwrap(),
            &fast(),
            NOW,
            &|_| {},
        )
        .unwrap_err();
        assert!(e.contains("couldn't reach"));
        assert!(!dir(&d).exists());
        let key = pair();
        let routes = vec![(
            "/v1/tt/integrity",
            "404 Not Found",
            br#"{"error":"no integrity module is published"}"#.to_vec(),
        )];
        let (got, d, _) = run("none", &key, routes, 1);
        assert_eq!(got.unwrap_err(), "no integrity module is published");
        assert!(!dir(&d).exists());
    }

    #[test]
    fn the_pinned_key_does_not_accept_a_test_signature() {
        let key = pair();
        let d = data("pinned");
        let (url, _, t) = serve(
            vec![("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES)))],
            1,
        );
        let e = ensure(&d, &Api::with_server(&url, "tok"), &|_| {}).unwrap_err();
        t.join().unwrap();
        assert!(e.contains("pinned key"));
    }

    #[cfg(unix)]
    #[test]
    fn older_versions_are_pruned_to_two() {
        let key = pair();
        let d = data("prune");
        for (i, v) in ["a1", "a2", "a3"].iter().enumerate() {
            let folder = dir(&d).join(v);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(DLL), b"old").unwrap();
            let when = std::time::SystemTime::now() - Duration::from_secs(1000 - 100 * i as u64);
            std::fs::File::open(&folder).unwrap().set_modified(when).unwrap();
        }
        let routes = vec![
            ("/v1/tt/integrity/file", "200 OK", BYTES.to_vec()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "b1", BYTES))),
        ];
        let (url, _, t) = serve(routes, 2);
        ensure_with(
            &d,
            &Api::with_server(&url, "tok"),
            key.public_key().as_ref(),
            &fast(),
            NOW,
            &|_| {},
        )
        .unwrap();
        t.join().unwrap();
        let mut left: Vec<String> = std::fs::read_dir(dir(&d))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["a3", "b1"]);
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_local_build_is_used_as_it_is_and_a_file_that_is_no_dll_is_refused() {
        let d = data("local");
        std::fs::create_dir_all(&d).unwrap();
        let dll = d.join(DLL);
        std::fs::write(&dll, BYTES).unwrap();
        let m = local_at(&dll).unwrap();
        assert_eq!(
            (m.version.as_str(), &m.dll, m.sha256),
            ("local", &dll, sha256_bytes(BYTES))
        );
        std::fs::write(&dll, b"not a dll").unwrap();
        assert!(local_at(&dll).unwrap_err().contains("not a Windows DLL"));
        assert!(local_at(&d.join("missing.dll")).unwrap_err().contains("reading"));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn placing_the_module_adds_one_file_and_one_plugin() {
        let m = Module {
            version: "v1".into(),
            dll: PathBuf::from("/data/timetrial/integrity/v1").join(DLL),
            sha256: String::new(),
        };
        let mut loader = std::collections::BTreeMap::new();
        let mut plugins = vec!["addons/other/o.dll".to_owned()];
        place(&m, &mut loader, &mut plugins);
        assert_eq!(
            loader.into_iter().collect::<Vec<_>>(),
            [(
                "swofty/addons/swofty.integrity/swofty_integrity.dll".to_owned(),
                m.dll.clone()
            )]
        );
        assert_eq!(
            plugins,
            [
                "addons/other/o.dll",
                "addons/swofty.integrity/swofty_integrity.dll"
            ]
        );
    }

    fn lines() -> (Arc<Mutex<Vec<String>>>, impl Fn(&str)) {
        let said = Arc::new(Mutex::new(Vec::new()));
        let log = said.clone();
        (said, move |l: &str| log.lock().unwrap().push(l.to_owned()))
    }

    fn api_error(status: u16) -> ApiError {
        ApiError {
            status,
            message: format!("HTTP {status}"),
            retry_after: None,
        }
    }

    #[test]
    fn timeouts_connection_errors_and_5xx_are_tried_again_and_nothing_else() {
        for status in [0, 500, 502, 503, 504] {
            assert!(retryable(&api_error(status)), "{status}");
        }
        for status in [200, 400, 401, 403, 404, 408, 429] {
            assert!(!retryable(&api_error(status)), "{status}");
        }
    }

    #[test]
    fn the_default_tries_stay_within_twenty_seconds() {
        let r = Retry::default();
        assert_eq!(r.backoff.len(), 2);
        assert!(r.budget <= Duration::from_secs(20));
        assert!(r.backoff.iter().sum::<Duration>() + r.min_try <= r.budget);
    }

    #[test]
    fn a_failed_offer_and_download_are_tried_again_with_a_line_each() {
        let key = pair();
        let routes = vec![
            ("/v1/tt/integrity/file", "503 Service Unavailable", b"{}".to_vec()),
            ("/v1/tt/integrity/file", "200 OK", BYTES.to_vec()),
            ("/v1/tt/integrity", "502 Bad Gateway", b"{}".to_vec()),
            ("/v1/tt/integrity", DROP, Vec::new()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES))),
        ];
        let d = data("retry");
        let (url, seen, t) = serve(routes, 5);
        let (said, out) = lines();
        let m = ensure_with(
            &d,
            &Api::with_server(&url, "tok"),
            key.public_key().as_ref(),
            &Retry {
                backoff: vec![Duration::from_millis(10); 3],
                ..fast()
            },
            NOW,
            &out,
        )
        .unwrap();
        t.join().unwrap();
        assert_eq!(std::fs::read(m.dll).unwrap(), BYTES);
        assert_eq!(
            *seen.lock().unwrap(),
            [
                "/v1/tt/integrity",
                "/v1/tt/integrity",
                "/v1/tt/integrity",
                "/v1/tt/integrity/file?version=v1",
                "/v1/tt/integrity/file?version=v1"
            ]
        );
        let said = said.lock().unwrap();
        assert_eq!(said.len(), 3, "{said:?}");
        assert!(
            said[0].contains("asking the server for the module failed") && said[0].contains("try 2 of 4")
        );
        assert!(said[1].contains("try 3 of 4"));
        assert!(said[2].contains("downloading the module failed") && said[2].contains("try 2 of 4"));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn tries_end_after_the_last_wait() {
        let key = pair();
        let routes = vec![
            ("/v1/tt/integrity", "503 Service Unavailable", b"{}".to_vec()),
            ("/v1/tt/integrity", "503 Service Unavailable", b"{}".to_vec()),
            ("/v1/tt/integrity", "503 Service Unavailable", b"{}".to_vec()),
        ];
        let (got, d, seen) = run("tries", &key, routes, 3);
        assert!(got.is_err());
        assert_eq!(seen.len(), 3);
        assert!(!dir(&d).exists());
    }

    #[test]
    fn no_try_starts_past_the_budget() {
        let calls = std::cell::Cell::new(0);
        let retry = Retry {
            backoff: vec![Duration::from_millis(50); 5],
            budget: Duration::from_millis(120),
            min_try: Duration::from_millis(20),
        };
        let started = Instant::now();
        let got: Result<(), ApiError> =
            with_retry("x", &retry, started, Duration::from_secs(4), &|_| {}, |t| {
                calls.set(calls.get() + 1);
                assert!(t <= Duration::from_secs(4));
                if calls.get() > 1 {
                    assert!(t <= Duration::from_millis(120));
                }
                Err(api_error(0))
            });
        assert!(got.is_err());
        assert_eq!(calls.get(), 2);
        assert!(started.elapsed() < Duration::from_millis(400));
    }

    #[test]
    fn a_4xx_is_not_tried_again() {
        let key = pair();
        let routes = vec![(
            "/v1/tt/integrity",
            "401 Unauthorized",
            br#"{"error":"signed out"}"#.to_vec(),
        )];
        let (got, _, seen) = run("unauth", &key, routes, 1);
        assert_eq!(got.unwrap_err(), "signed out");
        assert_eq!(seen.len(), 1);
    }

    fn cache(d: &Path, key: &Ed25519KeyPair, version: &str, bytes: &[u8], offered: u64) {
        let folder = dir(d).join(version);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(DLL), bytes).unwrap();
        let o: Offer = serde_json::from_value(offer(key, version, bytes)).unwrap();
        keep(&folder, &o, offered);
    }

    fn offline(d: &Path, key: &Ed25519KeyPair, now: u64) -> (Result<Module, String>, Vec<String>) {
        let (said, out) = lines();
        let got = ensure_with(
            d,
            &Api::with_server("http://127.0.0.1:1", "tok"),
            key.public_key().as_ref(),
            &fast(),
            now,
            &out,
        );
        let said = said.lock().unwrap().clone();
        (got, said)
    }

    #[test]
    fn the_newest_verified_cached_copy_is_used_when_the_server_is_out_of_reach() {
        let key = pair();
        let d = data("fallback");
        cache(&d, &key, "v1", b"MZ one", NOW - 86_400);
        cache(&d, &key, "v2", BYTES, NOW - 3 * 86_400);
        cache(&d, &key, "v3", b"MZ three", NOW - 2 * 86_400);
        let (got, said) = offline(&d, &key, NOW);
        let m = got.unwrap();
        assert_eq!(m.version, "v1");
        assert_eq!(m.sha256, sha256_bytes(b"MZ one"));
        assert!(
            said.iter().filter(|l| l.contains("trying again")).count() == 2,
            "{said:?}"
        );
        let last = said.last().unwrap();
        assert!(
            last.contains("using the cached copy v1") && last.contains("24 h ago"),
            "{last}"
        );
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_cached_copy_past_the_age_limit_is_not_used() {
        let key = pair();
        let d = data("aged");
        cache(&d, &key, "v1", BYTES, NOW - CACHE_MAX_AGE_S);
        assert_eq!(offline(&d, &key, NOW).0.unwrap().version, "v1");
        let (got, _) = offline(&d, &key, NOW + 1);
        assert!(got.unwrap_err().contains("couldn't reach"));
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_cached_copy_that_does_not_verify_is_not_used() {
        let key = pair();
        let other = pair();
        let d = data("unverified");
        cache(&d, &other, "v1", BYTES, NOW);
        cache(&d, &key, "v2", BYTES, NOW);
        std::fs::write(dir(&d).join("v2").join(DLL), b"MZ tampered").unwrap();
        cache(&d, &key, "v3", BYTES, NOW);
        std::fs::remove_file(dir(&d).join("v3").join(RECORD)).unwrap();
        let folder = dir(&d).join("v4");
        cache(&d, &key, "v5", BYTES, NOW);
        std::fs::rename(dir(&d).join("v5"), &folder).unwrap();
        assert!(offline(&d, &key, NOW).0.is_err());
        assert!(cached_with(&d, key.public_key().as_ref(), NOW).is_none());
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn a_cached_hit_keeps_when_the_server_offered_it() {
        let key = pair();
        let d = data("offered");
        cache(&d, &key, "v1", BYTES, NOW - 10 * 86_400);
        let (url, _, t) = serve(
            vec![("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES)))],
            1,
        );
        ensure_with(
            &d,
            &Api::with_server(&url, "tok"),
            key.public_key().as_ref(),
            &fast(),
            NOW,
            &|_| {},
        )
        .unwrap();
        t.join().unwrap();
        assert_eq!(
            cached_with(&d, key.public_key().as_ref(), NOW).unwrap().offered,
            NOW
        );
        std::fs::remove_dir_all(d).ok();
    }

    #[test]
    fn the_session_state_follows_the_fetch_and_the_build() {
        let m = Module {
            version: "v1".into(),
            dll: PathBuf::from(DLL),
            sha256: String::new(),
        };
        assert_eq!(State::of(None, true), State::Off);
        assert_eq!(State::of(Some(&Ok(m.clone())), true), State::Placed("v1".into()));
        assert_eq!(State::of(Some(&Ok(m)), false), State::Off);
        assert_eq!(
            State::of(Some(&Err("timeout".into())), true),
            State::Missing("timeout".into())
        );
        assert_eq!(State::of(Some(&Err("timeout".into())), false), State::Off);
        let long = "x".repeat(400);
        let State::Missing(why) = State::of(Some(&Err(long)), true) else {
            panic!("not missing");
        };
        assert_eq!(why.len(), REASON_MAX + 3);
        assert_eq!(State::Missing("a".into()).blocked(), Some("a"));
        assert_eq!(State::Placed("a".into()).blocked(), None);
    }

    #[test]
    fn the_session_keys_go_into_tt_json_and_come_back_out() {
        let game = data("ttjson");
        let file = game.join(crate::timetrial::SESSION_FILE);
        write_session(&game, &State::Off).unwrap();
        assert!(!file.exists());
        crate::timetrial::write_session_keys(&game, Some("n"), false).unwrap();
        write_session(&game, &State::Missing("couldn't reach the server".into())).unwrap();
        let doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(
            doc,
            json!({"tt_boards": true, "tt_nonce": "n", "tt_blocked": {"reason": "couldn't reach the server"}})
        );
        write_session(&game, &State::Placed("v1".into())).unwrap();
        let doc: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(doc["tt_module"], json!({"version": "v1", "dll": DLL}));
        assert!(doc.get("tt_blocked").is_none());
        crate::timetrial::clear_session_keys(&game);
        write_session(&game, &State::Off).unwrap();
        assert!(!file.exists());
        std::fs::remove_dir_all(game).ok();
    }

    #[test]
    fn the_launcher_note_is_kept_only_while_blocked() {
        let d = data("note");
        note(&d, &State::Missing("timeout".into()));
        assert_eq!(noted(&d).as_deref(), Some("timeout"));
        note(&d, &State::Placed("v1".into()));
        assert_eq!(noted(&d), None);
        note(&d, &State::Off);
        assert_eq!(noted(&d), None);
        std::fs::remove_dir_all(d).ok();
    }
}
