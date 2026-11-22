//! The integrity module: a closed-source native addon the Time Trial boards server publishes, which
//! appends an attestation to every lap while the player is signed in with Time Trial boards on.
//!
//! The server offers one module at a time at `GET /v1/tt/integrity`. [`ensure`] gets it the way every signed
//! server file is got ([`crate::online::modules::fetch`]): the offer, asked for again while the server is out
//! of reach or busy; the cached copy under `<data>/timetrial/integrity/<version>/` when it has the offered
//! hash; else the offer's `cdn_url`, else the server's file; and when the server can't be reached, the newest
//! cached copy whose kept offer and bytes still verify and that the server offered within [`CACHE_MAX_AGE`].
//! Nothing is placed in the game folder unless the signature checks out against the key pinned here. Every
//! failure is an [`Unavailable`] with one line of text for the log; the session then hands the core
//! [`BLOCKED_KEY`] ([`write_session`]) and the core blocks Time Trial in game.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::api::{Api, ApiError};
use crate::game_folder::baseline::sha256_bytes;
use crate::online::modules::fetch::{self, Retry};
pub use crate::online::modules::{Kind, Unavailable};

/// The addon id the module loads under. It is not a user addon: no profile, addon list or fingerprint has it.
pub const PLUGIN_ID: &str = "swofty.integrity";
pub const DLL: &str = "swofty_integrity.dll";
/// Base64 of the Ed25519 public key whose private half signs published modules.
pub(crate) const PUBLIC_KEY: &str = "Y79gick8AdEJDzKoMDuiVdEV5UYuc2xgxJ+V7VGgZxg=";
const SIGNED_PREFIX: &str = "swofty-tt-integrity-v1";
const MAX_SIZE: u64 = 16 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// A cached copy stands in for the server's while the server offered it at most this long ago: the boards
/// stop taking a version's seals some time after a newer one is published.
pub const CACHE_MAX_AGE: Duration = Duration::from_secs(14 * 24 * 60 * 60);
/// Names a locally built module to place for the session in place of the server's, for testing a build
/// before it is published. Unset in normal use.
pub const LOCAL_ENV: &str = "SWOFTY_TT_INTEGRITY_DLL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub version: String,
    pub dll: PathBuf,
    pub sha256: String,
    /// Why the server's copy could not be had, when this is the newest cached copy in its place.
    pub fallback: Option<String>,
    /// Where the bytes came from this time.
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Offer {
    version: String,
    sha256: String,
    size: u64,
    signature: String,
    /// Where the same bytes are served without the sign-in; the size, sha256 and signature are checked the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cdn_url: Option<String>,
}

pub fn dir(data: &Path) -> PathBuf {
    data.join("timetrial").join("integrity")
}

/// The file [`LOCAL_ENV`] names, when it is set: `None` when it is unset or empty, else that file as a
/// module with the version `local`, or why it cannot be used. No server is asked and no signature is checked.
pub fn local() -> Option<Result<Module, Unavailable>> {
    let path = std::env::var_os(LOCAL_ENV).filter(|p| !p.is_empty())?;
    Some(local_at(Path::new(&path)).map_err(Unavailable::from))
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
        fallback: None,
        source: Source::Env,
    })
}

/// The verified module for this session, by the steps in the module documentation. `out` gets a line for
/// each try again and for a cached copy used in place of the server's.
pub fn ensure(data: &Path, api: &Api, out: &dyn Fn(&str)) -> Result<Module, Unavailable> {
    let key = decode(PUBLIC_KEY).ok_or("the pinned integrity key is not valid base64")?;
    ensure_retrying(data, api, &key, &Retry::default(), out)
}

#[cfg(test)]
pub(crate) fn ensure_with(data: &Path, api: &Api, key: &[u8]) -> Result<Module, Unavailable> {
    let quick = Retry {
        backoff: vec![Duration::from_millis(20), Duration::from_millis(20)],
        budget: Duration::from_secs(6),
        min_try: Duration::from_millis(500),
    };
    ensure_retrying(data, api, key, &quick, &|_| {})
}

fn ensure_retrying(
    data: &Path,
    api: &Api,
    key: &[u8],
    retry: &Retry,
    out: &dyn Fn(&str),
) -> Result<Module, Unavailable> {
    let f = fetch::ensure(data, api, &Integrity, key, retry, "integrity module", out)?;
    if let Some(why) = &f.fallback {
        out(&format!(
            "integrity module: the server's copy could not be had ({why}); using the cached copy {}, verified offline",
            f.version
        ));
    }
    Ok(Module {
        version: f.version,
        dll: f.path,
        sha256: f.sha256,
        fallback: f.fallback,
        source: match f.origin {
            fetch::Origin::Server => Source::Server,
            fetch::Origin::Cdn => Source::Cdn,
            fetch::Origin::Cache => Source::Cache,
        },
    })
}

/// The integrity module, as [`fetch`] asks for it.
struct Integrity;

impl fetch::Signed for Offer {
    fn version(&self) -> &str {
        &self.version
    }
    fn sha256(&self) -> &str {
        &self.sha256
    }
    fn size(&self) -> u64 {
        self.size
    }
    fn file(&self) -> &str {
        DLL
    }
    fn cdn_url(&self) -> Option<&str> {
        self.cdn_url.as_deref()
    }
}

impl fetch::Source for Integrity {
    type Offer = Offer;

    fn base(&self, data: &Path) -> PathBuf {
        dir(data)
    }

    fn ask(&self, api: &Api, timeout: Duration) -> Result<Value, ApiError> {
        api.integrity(timeout)
    }

    fn pick(&self, answer: Value) -> Result<Offer, Unavailable> {
        serde_json::from_value(answer).map_err(|e| format!("the offer is not valid: {e}").into())
    }

    fn check(&self, key: &[u8], offer: &Offer) -> Result<(), Unavailable> {
        fetch::plain(&offer.version, &offer.sha256, offer.size, MAX_SIZE)?;
        fetch::signed_by(key, &signed_message(offer), &offer.signature)
    }

    fn download(&self, api: &Api, offer: &Offer, timeout: Duration) -> Result<Vec<u8>, ApiError> {
        api.integrity_file(&offer.version, offer.size, timeout)
    }

    fn download_timeout(&self) -> Duration {
        DOWNLOAD_TIMEOUT
    }

    fn max_age(&self) -> Option<Duration> {
        Some(CACHE_MAX_AGE)
    }
}

fn signed_message(offer: &Offer) -> Vec<u8> {
    let mut message = Vec::new();
    message.extend_from_slice(SIGNED_PREFIX.as_bytes());
    message.push(0);
    message.extend_from_slice(offer.version.as_bytes());
    message.push(0);
    message.extend_from_slice(offer.sha256.as_bytes());
    message
}

/// What the session did about the integrity module, as the `integrity` field of its laps' upload meta, in the
/// server's terms: what happened, the version loaded or tried, where its bytes came from and free text for review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub state: Status,
    pub version: Option<String>,
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The module was placed for the session.
    Loaded,
    /// The server has no module to offer.
    NoOffer,
    /// The offer or the file could not be had, and no cached copy still verified.
    FetchFailed,
    /// The offer's signature, or the file's size or sha256, did not verify.
    BadOffer,
    /// The server did not take the launcher's sign-in.
    SignedOut,
    /// The server refused it for another reason, in its words in `detail`.
    Refused,
}

/// Where the bytes of the placed module came from; `env` is [`LOCAL_ENV`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Server,
    Cdn,
    Cache,
    Env,
}

/// The longest reason a [`State`] or [`Notice`] carries, in characters.
const WHY_MAX: usize = 200;

fn short(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(WHY_MAX) {
        Some(_) => {
            let cut = text.char_indices().nth(WHY_MAX - 3).map_or(0, |(at, _)| at);
            format!("{}...", &text[..cut])
        }
        None => text.to_owned(),
    }
}

impl State {
    /// The state of a session that asked for the module and got `found`, from [`local`] when `local`.
    pub fn of(found: &Result<Module, Unavailable>, local: bool) -> Self {
        match found {
            Ok(m) => Self {
                state: Status::Loaded,
                version: (!local).then(|| m.version.clone()),
                source: Some(if local { Source::Env } else { m.source }),
                detail: m.fallback.as_deref().map(short),
            },
            Err(e) => Self {
                state: match (e.kind, e.status) {
                    (Kind::Unreachable, _) => Status::FetchFailed,
                    (Kind::Invalid, _) => Status::BadOffer,
                    (Kind::Refused, 404) => Status::NoOffer,
                    (Kind::Refused, 401) => Status::SignedOut,
                    (Kind::Refused, _) => Status::Refused,
                },
                version: None,
                source: None,
                detail: Some(short(&e.why)),
            },
        }
    }
}

impl State {
    /// Why Time Trial is blocked in game for a session in this state: every state but [`Status::Loaded`] blocks it.
    pub fn blocked(&self) -> Option<&str> {
        (self.state != Status::Loaded).then(|| self.detail.as_deref().unwrap_or_default())
    }
}

/// The [`SESSION_FILE`](crate::online::timetrial::SESSION_FILE) key naming the module the session placed:
/// `{"version", "dll"}`.
pub const MODULE_KEY: &str = "tt_module";
/// The [`SESSION_FILE`](crate::online::timetrial::SESSION_FILE) key that blocks Time Trial in game: `{"reason"}`.
pub const BLOCKED_KEY: &str = "tt_blocked";

/// Hand the core what the session did about the module through
/// [`SESSION_FILE`](crate::online::timetrial::SESSION_FILE): [`MODULE_KEY`] when one was placed, [`BLOCKED_KEY`]
/// when the session asked for one and none was placed, and neither when it did not ask (`None`). Other keys stay
/// as they are.
pub fn write_session(game: &Path, state: Option<&State>) -> std::io::Result<()> {
    crate::online::timetrial::edit_session(game, |doc| {
        doc.remove(MODULE_KEY);
        doc.remove(BLOCKED_KEY);
        let Some(state) = state else {
            return;
        };
        match state.blocked() {
            None => doc.insert(
                MODULE_KEY.into(),
                json!({"version": state.version.as_deref().unwrap_or("local"), "dll": DLL}),
            ),
            Some(why) => doc.insert(BLOCKED_KEY.into(), json!({"reason": why})),
        };
    })
}

/// The note a session that placed no module leaves in the data folder. The launcher shows it once, as a
/// dialog, when a Play leaves a newer one than it has shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    pub at: f64,
    pub kind: Kind,
    pub why: String,
}

impl Notice {
    pub fn path(data: &Path) -> PathBuf {
        data.join("state").join("integrity-missing.json")
    }

    pub fn read(data: &Path) -> Option<Self> {
        crate::read_json(&Self::path(data)).ok()
    }

    /// Leave the note when the session placed no module, for the reason `missing`; clear it when it placed one
    /// or did not ask for it (`None`).
    pub fn keep(data: &Path, missing: Option<&Unavailable>) -> crate::Result<()> {
        let path = Self::path(data);
        let Some(Unavailable { kind, why, .. }) = missing else {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(crate::io_err(&path)(e)),
                _ => Ok(()),
            };
        };
        let at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64());
        let notice = Self {
            at,
            kind: *kind,
            why: short(why),
        };
        crate::write_json_shared(&path, &notice)
    }
}

/// A version becomes a folder name, so it is plain characters only.
pub fn valid_version(v: &str) -> bool {
    (1..=32).contains(&v.len())
        && !v.starts_with('.')
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Standard base64, padded or not.
pub(crate) fn decode(text: &str) -> Option<Vec<u8>> {
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
    crate::online::modules::place_file(PLUGIN_ID, DLL, &module.dll, loader, plugins);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::online::modules::fetch::Source as _;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use std::io::{Read, Write};
    use std::sync::{Arc, Mutex};

    const BYTES: &[u8] = b"MZ not really a dll";

    pub(crate) fn pair() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }

    pub(crate) fn b64(bytes: &[u8]) -> String {
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

    pub(crate) fn offer(key: &Ed25519KeyPair, version: &str, bytes: &[u8]) -> serde_json::Value {
        let sha = sha256_bytes(bytes);
        serde_json::json!({
            "version": version, "sha256": sha, "size": bytes.len(),
            "signature": sign(key, version, &sha),
            "url": format!("/v1/tt/integrity/file?version={version}"),
        })
    }

    pub(crate) type Route = (&'static str, &'static str, Vec<u8>);

    pub(crate) fn serve(
        routes: Vec<Route>,
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
                let (status, body) = routes.iter().find(|(p, _, _)| path.starts_with(p)).map_or(
                    ("404 Not Found", br#"{"error":"nothing here"}"#.to_vec()),
                    |(_, s, b)| (*s, b.clone()),
                );
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

    pub(crate) fn json(v: &serde_json::Value) -> Vec<u8> {
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
    ) -> (Result<Module, Unavailable>, PathBuf, Vec<String>) {
        let d = data(name);
        let (url, seen, t) = serve(routes, requests);
        let got = ensure_with(&d, &Api::with_server(&url, "tok"), key.public_key().as_ref());
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
            cdn_url: None,
        };
        Integrity.check(&decode(PUBLIC_KEY).unwrap(), &offer).unwrap();
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
        let mut kept: Vec<String> = std::fs::read_dir(dir(&d).join("2026.11.10.1"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        kept.sort();
        assert_eq!(kept, ["offer.json", DLL]);
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
        let m = ensure_with(&d, &Api::with_server(&url, "tok"), key.public_key().as_ref()).unwrap();
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
        let m = ensure_with(&d, &Api::with_server(&url, "tok"), key.public_key().as_ref()).unwrap();
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
        assert!(got.unwrap_err().why.contains("signature"));
        assert_eq!(seen.len(), 1);
        assert!(!dir(&d).exists());
    }

    #[test]
    fn a_signature_over_another_version_is_refused() {
        let key = pair();
        let mut o = offer(&key, "v1", BYTES);
        o["version"] = "v2".into();
        let (got, ..) = run("swapped", &key, vec![("/v1/tt/integrity", "200 OK", json(&o))], 1);
        assert!(got.unwrap_err().why.contains("signature"));
    }

    #[test]
    fn a_download_with_the_wrong_hash_is_not_stored() {
        let key = pair();
        let routes = vec![
            ("/v1/tt/integrity/file", "200 OK", b"MZ not really a dlL".to_vec()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES))),
        ];
        let (got, d, _) = run("badhash", &key, routes, 2);
        assert!(got.unwrap_err().why.contains("sha256"));
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
        assert!(got.unwrap_err().why.contains("out of range"));
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
            assert!(got.unwrap_err().why.contains("version"), "{v}");
            assert_eq!(seen.len(), 1);
            assert!(!dir(&d).exists());
        }
        assert!(valid_version("2026.11.10.1"));
        assert!(valid_version("a_b-c"));
    }

    #[test]
    fn nothing_is_stored_when_offline_or_unpublished() {
        let d = data("offline");
        let e = ensure(&d, &Api::with_server("http://127.0.0.1:1", "tok"), &|_| {}).unwrap_err();
        assert!(e.why.contains("couldn't reach"));
        assert_eq!(e.kind, Kind::Unreachable);
        assert!(!dir(&d).exists());
        let key = pair();
        let routes = vec![(
            "/v1/tt/integrity",
            "404 Not Found",
            br#"{"error":"no integrity module is published"}"#.to_vec(),
        )];
        let (got, d, _) = run("none", &key, routes, 1);
        let e = got.unwrap_err();
        assert_eq!(
            (e.why.as_str(), e.kind),
            ("no integrity module is published", Kind::Refused)
        );
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
        assert!(e.why.contains("pinned key"));
        assert_eq!(e.kind, Kind::Invalid);
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
        ensure_with(&d, &Api::with_server(&url, "tok"), key.public_key().as_ref()).unwrap();
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
            fallback: None,
            source: Source::Server,
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

    fn keep_copy(d: &Path, key: &Ed25519KeyPair, version: &str, bytes: &[u8], offered: u64) {
        let folder = dir(d).join(version);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(DLL), bytes).unwrap();
        let kept = folder.join("offer.json");
        std::fs::write(&kept, json(&offer(key, version, bytes))).unwrap();
        let at = std::time::UNIX_EPOCH + Duration::from_secs(offered);
        std::fs::File::options()
            .write(true)
            .open(&kept)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    fn never_answers() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for c in listener.incoming() {
                held.push(c);
            }
        });
        url
    }

    #[test]
    fn an_offer_that_times_out_falls_back_to_the_cached_copy() {
        let key = pair();
        let d = data("timeout-cached");
        let now = crate::online::timetrial::queue::unix_now();
        keep_copy(&d, &key, "v1", BYTES, now - 60);
        let m = ensure_with(
            &d,
            &Api::with_server(&never_answers(), "tok"),
            key.public_key().as_ref(),
        )
        .unwrap();
        assert_eq!(
            (m.version.as_str(), m.dll.clone()),
            ("v1", dir(&d).join("v1").join(DLL))
        );
        std::fs::remove_dir_all(&d).ok();
    }

    fn hangs_up() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for c in listener.incoming() {
                drop(c);
            }
        });
        url
    }

    fn offline(d: &Path, key: &Ed25519KeyPair) -> Result<Module, Unavailable> {
        ensure_with(
            d,
            &Api::with_server(&hangs_up(), "tok"),
            key.public_key().as_ref(),
        )
    }

    #[test]
    fn a_busy_server_is_asked_again_for_the_offer() {
        let key = pair();
        let d = data("busy");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let flaky = format!("http://{}", listener.local_addr().unwrap());
        let offered = json(&offer(&key, "v1", BYTES));
        let server = std::thread::spawn(move || {
            let replies: [(&str, Vec<u8>); 3] = [
                ("503 Service Unavailable", Vec::new()),
                ("200 OK", offered),
                ("200 OK", BYTES.to_vec()),
            ];
            let mut paths = Vec::new();
            for (status, body) in replies {
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
                paths.push(line.split(' ').nth(1).unwrap_or("").to_owned());
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                c.write_all(head.as_bytes()).ok();
                c.write_all(&body).ok();
            }
            paths
        });
        let m = ensure_with(&d, &Api::with_server(&flaky, "tok"), key.public_key().as_ref()).unwrap();
        assert_eq!(m.version, "v1");
        assert_eq!(std::fs::read(&m.dll).unwrap(), BYTES);
        assert_eq!(
            server.join().unwrap(),
            [
                "/v1/tt/integrity",
                "/v1/tt/integrity",
                "/v1/tt/integrity/file?version=v1"
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_newest_verified_cached_copy_is_used_when_the_server_is_out_of_reach() {
        let key = pair();
        let d = data("cached");
        let now = crate::online::timetrial::queue::unix_now();
        keep_copy(&d, &key, "v1", b"MZ old build", now - 7200);
        keep_copy(&d, &key, "v2", BYTES, now - 3600);
        let m = offline(&d, &key).unwrap();
        assert_eq!(
            (m.version.as_str(), m.dll.clone()),
            ("v2", dir(&d).join("v2").join(DLL))
        );
        assert_eq!(m.sha256, sha256_bytes(BYTES));
        assert!(
            m.fallback
                .as_deref()
                .is_some_and(|why| why.contains("couldn't reach"))
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_cached_copy_is_used_only_while_it_still_verifies() {
        let key = pair();
        let now = crate::online::timetrial::queue::unix_now();

        let d = data("cached-tampered");
        keep_copy(&d, &key, "v1", BYTES, now - 60);
        std::fs::write(dir(&d).join("v1").join(DLL), b"MZ patched").unwrap();
        assert!(offline(&d, &key).unwrap_err().why.contains("couldn't reach"));
        std::fs::remove_dir_all(&d).ok();

        let d = data("cached-other-key");
        keep_copy(&d, &pair(), "v1", BYTES, now - 60);
        assert!(offline(&d, &key).is_err());
        std::fs::remove_dir_all(&d).ok();

        let d = data("cached-renamed");
        keep_copy(&d, &key, "v1", BYTES, now - 60);
        std::fs::rename(dir(&d).join("v1"), dir(&d).join("v9")).unwrap();
        assert!(offline(&d, &key).is_err());
        std::fs::remove_dir_all(&d).ok();

        let d = data("cached-old");
        keep_copy(&d, &key, "v1", BYTES, now - 15 * 24 * 3600);
        assert!(offline(&d, &key).is_err());
        std::fs::remove_dir_all(&d).ok();

        let d = data("cached-no-offer");
        keep_copy(&d, &key, "v1", BYTES, now - 60);
        std::fs::remove_file(dir(&d).join("v1").join("offer.json")).unwrap();
        assert!(offline(&d, &key).is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    fn cdn(body: Vec<u8>) -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!(
            "http://{}/cdn/swofty_integrity.dll",
            listener.local_addr().unwrap()
        );
        let t = std::thread::spawn(move || {
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
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            c.write_all(head.as_bytes()).ok();
            c.write_all(&body).ok();
            String::from_utf8_lossy(&got).into_owned()
        });
        (url, t)
    }

    #[test]
    fn an_offered_cdn_copy_is_downloaded_without_the_sign_in() {
        let key = pair();
        let (link, cdn_t) = cdn(BYTES.to_vec());
        let mut offered = offer(&key, "v1", BYTES);
        offered["cdn_url"] = link.into();
        let routes = vec![("/v1/tt/integrity", "200 OK", json(&offered))];
        let (got, d, seen) = run("cdn", &key, routes, 1);
        let m = got.unwrap();
        assert_eq!(std::fs::read(&m.dll).unwrap(), BYTES);
        assert_eq!(seen, ["/v1/tt/integrity"]);
        let asked = cdn_t.join().unwrap();
        assert!(asked.starts_with("GET /cdn/swofty_integrity.dll "), "{asked}");
        assert!(!asked.to_ascii_lowercase().contains("authorization"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_cdn_copy_that_does_not_match_the_signed_hash_falls_back_to_the_servers_download() {
        let key = pair();
        let mut tampered = BYTES.to_vec();
        tampered[0] ^= 1;
        for (name, link, cdn_t) in [
            ("cdn-tampered", cdn(tampered)),
            ("cdn-short", cdn(BYTES[..BYTES.len() - 1].to_vec())),
        ]
        .map(|(name, (link, t))| (name, link, Some(t)))
        .into_iter()
        .chain([(
            "cdn-down",
            "http://127.0.0.1:1/cdn/swofty_integrity.dll".to_owned(),
            None,
        )]) {
            let mut offered = offer(&key, "v1", BYTES);
            offered["cdn_url"] = link.into();
            let routes = vec![
                ("/v1/tt/integrity/file", "200 OK", BYTES.to_vec()),
                ("/v1/tt/integrity", "200 OK", json(&offered)),
            ];
            let (got, d, seen) = run(name, &key, routes, 2);
            let m = got.unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(std::fs::read(&m.dll).unwrap(), BYTES, "{name}");
            assert_eq!(
                seen,
                ["/v1/tt/integrity", "/v1/tt/integrity/file?version=v1"],
                "{name}"
            );
            if let Some(t) = cdn_t {
                t.join().unwrap();
            }
            std::fs::remove_dir_all(&d).ok();
        }
    }

    #[test]
    fn an_offer_that_times_out_with_nothing_cached_leaves_the_note_for_the_launcher() {
        let key = pair();
        let d = data("timeout-empty");
        let found = ensure_with(
            &d,
            &Api::with_server(&never_answers(), "tok"),
            key.public_key().as_ref(),
        );
        let Err(missing) = &found else {
            panic!("{found:?}");
        };
        assert_eq!(missing.kind, Kind::Unreachable);
        let why = &missing.why;
        assert!(why.contains("couldn't reach") && why.contains("timeout"), "{why}");
        assert_eq!(State::of(&found, false).state, Status::FetchFailed);
        assert_eq!(Notice::read(&d), None);
        Notice::keep(&d, Some(missing)).unwrap();
        let note = Notice::read(&d).unwrap();
        assert_eq!((note.kind, note.why.as_str()), (Kind::Unreachable, why.as_str()));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        assert!(now - note.at < 60.0);
        Notice::keep(&d, None).unwrap();
        assert_eq!(Notice::read(&d), None);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_upload_meta_says_where_the_module_came_from_in_the_servers_words() {
        let key = pair();
        let wire = |found: &Result<Module, Unavailable>, local: bool| {
            serde_json::to_value(State::of(found, local)).unwrap()
        };
        let served = vec![
            ("/v1/tt/integrity/file", "200 OK", BYTES.to_vec()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES))),
        ];
        let (got, d, _) = run("meta-server", &key, served.clone(), 2);
        assert_eq!(
            wire(&got, false),
            serde_json::json!({"state": "loaded", "version": "v1", "source": "server"})
        );
        let (url, _, t) = serve(
            vec![("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES)))],
            1,
        );
        let again = ensure_with(&d, &Api::with_server(&url, "tok"), key.public_key().as_ref());
        t.join().unwrap();
        assert_eq!(
            wire(&again, false),
            serde_json::json!({"state": "loaded", "version": "v1", "source": "cache"})
        );
        let kept = offline(&d, &key);
        let detail = wire(&kept, false)["detail"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(detail.contains("couldn't reach"), "{detail}");
        assert_eq!(
            wire(&kept, false),
            serde_json::json!({"state": "loaded", "version": "v1", "source": "cache", "detail": detail})
        );
        std::fs::remove_dir_all(&d).ok();

        let (link, cdn_t) = cdn(BYTES.to_vec());
        let mut offered = offer(&key, "v2", BYTES);
        offered["cdn_url"] = link.into();
        let (got, d, _) = run(
            "meta-cdn",
            &key,
            vec![("/v1/tt/integrity", "200 OK", json(&offered))],
            1,
        );
        cdn_t.join().unwrap();
        assert_eq!(
            wire(&got, false),
            serde_json::json!({"state": "loaded", "version": "v2", "source": "cdn"})
        );
        std::fs::remove_dir_all(&d).ok();

        let local = d.with_extension("local.dll");
        std::fs::write(&local, BYTES).unwrap();
        let found = local_at(&local).map_err(Unavailable::from);
        assert_eq!(
            wire(&found, true),
            serde_json::json!({"state": "loaded", "version": null, "source": "env"})
        );
        std::fs::remove_file(&local).ok();

        for (name, status, body, state) in [
            (
                "meta-404",
                "404 Not Found",
                r#"{"error":"no integrity module is published"}"#,
                "no_offer",
            ),
            (
                "meta-401",
                "401 Unauthorized",
                r#"{"error":"sign in with Discord in the launcher first"}"#,
                "signed_out",
            ),
            (
                "meta-403",
                "403 Forbidden",
                r#"{"error":"banned","reason":"x"}"#,
                "refused",
            ),
        ] {
            let (got, d, _) = run(
                name,
                &key,
                vec![("/v1/tt/integrity", status, body.as_bytes().to_vec())],
                1,
            );
            let meta = wire(&got, false);
            assert_eq!(
                (meta["state"].as_str(), &meta["version"], &meta["source"]),
                (Some(state), &Value::Null, &Value::Null),
                "{meta}"
            );
            std::fs::remove_dir_all(&d).ok();
        }
        assert_eq!(
            wire(&Err(Unavailable::unreachable("timeout: global")), false),
            serde_json::json!({"state": "fetch_failed", "version": null, "source": null, "detail": "timeout: global"})
        );
        assert_eq!(
            wire(&Err("the signature does not match the pinned key".into()), false),
            serde_json::json!({"state": "bad_offer", "version": null, "source": null, "detail": "the signature does not match the pinned key"})
        );
        let long = wire(&Err("x".repeat(1000).into()), false);
        assert_eq!(long["detail"].as_str().map(|d| d.chars().count()), Some(200));
    }

    #[test]
    fn the_session_keys_tell_the_core_whether_time_trial_is_blocked() {
        let game = data("ttjson");
        let file = game.join(crate::online::timetrial::SESSION_FILE);
        let read = || -> Value { serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap() };
        let placed = |source| Module {
            version: "v1".into(),
            dll: PathBuf::from(DLL),
            sha256: String::new(),
            fallback: None,
            source,
        };
        let missing = Unavailable {
            why: "couldn't reach the server".into(),
            kind: Kind::Unreachable,
            status: 0,
        };
        write_session(&game, None).unwrap();
        assert!(!file.exists());
        crate::online::timetrial::write_session_keys(&game, None, false).unwrap();
        write_session(&game, Some(&State::of(&Err(missing), false))).unwrap();
        assert_eq!(
            read(),
            json!({"tt_boards": true, "tt_blocked": {"reason": "couldn't reach the server"}})
        );
        write_session(&game, Some(&State::of(&Ok(placed(Source::Cache)), false))).unwrap();
        assert_eq!(read()["tt_module"], json!({"version": "v1", "dll": DLL}));
        assert!(read().get("tt_blocked").is_none());
        write_session(&game, Some(&State::of(&Ok(placed(Source::Env)), true))).unwrap();
        assert_eq!(read()["tt_module"], json!({"version": "local", "dll": DLL}));
        let refused = Unavailable {
            why: "no module".into(),
            kind: Kind::Refused,
            status: 404,
        };
        let state = State::of(&Err(refused), false);
        assert_eq!(state.blocked(), Some("no module"));
        crate::online::timetrial::clear_session_keys(&game);
        write_session(&game, None).unwrap();
        assert!(!file.exists());
        std::fs::remove_dir_all(game).ok();
    }

    #[test]
    fn a_download_keeps_its_offer_for_a_later_offline_start() {
        let key = pair();
        let routes = vec![
            ("/v1/tt/integrity/file", "200 OK", BYTES.to_vec()),
            ("/v1/tt/integrity", "200 OK", json(&offer(&key, "v1", BYTES))),
        ];
        let (got, d, _) = run("kept", &key, routes, 2);
        let fetched = got.unwrap();
        assert_eq!(fetched.fallback, None);
        assert!(dir(&d).join("v1").join("offer.json").is_file());
        let kept = offline(&d, &key).unwrap();
        assert_eq!(
            (kept.version, kept.dll, kept.sha256),
            (fetched.version, fetched.dll, fetched.sha256)
        );
        std::fs::remove_dir_all(&d).ok();
    }
}
