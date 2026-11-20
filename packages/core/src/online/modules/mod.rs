//! Server modules: signed files the server publishes and the launcher downloads at runtime.
//!
//! `GET /v1/modules` lists, per module name, the newest version that speaks the abi the launcher asks for.
//! [`ensure`] keeps a verified copy under `<data>/modules/<name>/<platform>/<version>/` and hands out a
//! [`Module`] only after the file's size, sha256 and Ed25519 signature check out against the key pinned in
//! [`crate::online::server::integrity`]. The file comes from the offer's `cdn_url` when it has one and that
//! copy verifies, else from the server. Every failure is an [`Unavailable`] with one sentence for the log or
//! the page; the launcher goes on without the module.

pub mod fetch;
pub mod library;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use self::fetch::OFFER_FILE;
pub use self::fetch::Retry;
#[cfg(test)]
use self::fetch::cdn_link;
use crate::game_folder::baseline::sha256_bytes;
use crate::online::server::api::{Api, ApiError};
use crate::online::server::integrity::{PUBLIC_KEY, decode, valid_version};

const SIGNED_PREFIX: &str = "swofty-module-v1";
const MAX_SIZE: u64 = 64 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const ENV_PREFIX: &str = "SWOFTY_MODULE_";
/// The version of a module [`local`] names.
pub const LOCAL_VERSION: &str = "local";
pub const LINUX: &str = "linux-x86_64";
pub const WINDOWS: &str = "windows-x86_64";
/// The platform of everything that loads inside the game, whatever the launcher runs on.
pub const GAME_PLATFORM: &str = WINDOWS;

/// An in-game plugin the server publishes: its module name, the addon id it loads under and the abi this
/// launcher speaks. The id is not a user addon: no profile, addon list or fingerprint has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plugin {
    pub name: &'static str,
    pub id: &'static str,
    pub abi: u32,
}

pub const TT_GAME: Plugin = Plugin {
    name: "tt-game",
    id: "swofty.tt",
    abi: 1,
};

/// Placed only for a game started with the session module's environment.
pub const MP_GAME: Plugin = Plugin {
    name: "mp-game",
    id: "swofty.mp",
    abi: 1,
};

/// The platform string of the running launcher.
pub fn platform() -> &'static str {
    if cfg!(windows) { WINDOWS } else { LINUX }
}

/// A verified file of a server module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub sha256: String,
}

impl Module {
    /// Whether this is a local build named by the environment, unsigned.
    pub fn is_local(&self) -> bool {
        self.version == LOCAL_VERSION
    }

    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

/// Why a module is not there for this session, in one sentence. A refusal carries the server's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub why: String,
    pub kind: Kind,
    /// The status of the server's refusal; 0 when the server did not answer one.
    pub status: u16,
}

/// What kept a signed file from the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// The server could not be reached or was busy, and no cached copy still verified.
    Unreachable,
    /// The server answered that there is none for this player.
    Refused,
    /// The offer or the file failed a check, or the copy could not be stored.
    Invalid,
}

impl Unavailable {
    fn new(why: impl Into<String>) -> Self {
        Self {
            why: why.into(),
            kind: Kind::Invalid,
            status: 0,
        }
    }

    /// The server's refusal, in its own words.
    pub(crate) fn answered(e: ApiError) -> Self {
        Self {
            status: e.status,
            ..Self::refused(e.message)
        }
    }

    pub(crate) fn refused(why: impl Into<String>) -> Self {
        Self {
            kind: Kind::Refused,
            ..Self::new(why)
        }
    }

    pub(crate) fn unreachable(why: impl Into<String>) -> Self {
        Self {
            kind: Kind::Unreachable,
            ..Self::new(why)
        }
    }
}

impl fmt::Display for Unavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.why)
    }
}

impl std::error::Error for Unavailable {}

impl From<String> for Unavailable {
    fn from(why: String) -> Self {
        Self::new(why)
    }
}

impl From<&str> for Unavailable {
    fn from(why: &str) -> Self {
        Self::new(why)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Offer {
    name: String,
    platform: String,
    version: String,
    abi: u32,
    sha256: String,
    size: u64,
    signature: String,
    file: String,
    /// Where the same bytes are served without the sign-in; the size, sha256 and signature are checked the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cdn_url: Option<String>,
}

#[derive(Deserialize)]
struct Refusal {
    name: String,
    why: String,
}

#[derive(Deserialize)]
struct Listing {
    #[serde(default)]
    modules: Vec<Offer>,
    #[serde(default)]
    refused: Vec<Refusal>,
}

/// `<data>/modules/<name>/<platform>`, the folder that holds one folder per cached version.
pub fn dir(data: &Path, name: &str, platform: &str) -> PathBuf {
    data.join("modules").join(name).join(platform)
}

/// The environment variable that names a local build of `name`: `SWOFTY_MODULE_` and the name in upper case
/// with `_` for `-`.
pub fn local_env(name: &str) -> String {
    format!("{ENV_PREFIX}{}", name.to_ascii_uppercase().replace('-', "_"))
}

/// The file [`local_env`] names, when it is set: `None` when it is unset or empty, else that file as a module
/// with the version `local`, or why it cannot be used. No server is asked and no signature is checked. Only
/// debug builds read the variable; a release build always takes the server's signed module.
pub fn local(name: &str) -> Option<Result<Module, Unavailable>> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let path = std::env::var_os(local_env(name)).filter(|p| !p.is_empty())?;
    Some(local_at(name, Path::new(&path)))
}

fn local_at(name: &str, path: &Path) -> Result<Module, Unavailable> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_SIZE || path.file_name().is_none() {
        return Err(format!("{} is not a module file under 64 MiB", path.display()).into());
    }
    Ok(Module {
        name: name.to_owned(),
        version: LOCAL_VERSION.to_owned(),
        path: path.to_path_buf(),
        sha256: sha256_bytes(&bytes),
    })
}

/// The verified module `name` for `platform` that speaks `abi`: the local build when [`local_env`] names
/// one, else the cached copy when it has the hash the server offers, else a download. When the server can't
/// be reached or the download is refused as busy, the newest cached copy of that abi that still verifies.
/// The listing and the download are asked for again as [`Retry::default`] allows; `out` gets a line for each
/// try again and for a cached copy used in place of the server's.
pub fn ensure(
    data: &Path,
    api: &Api,
    name: &str,
    platform: &str,
    abi: u32,
    out: &dyn Fn(&str),
) -> Result<Module, Unavailable> {
    if let Some(found) = local(name) {
        return found;
    }
    let key = decode(PUBLIC_KEY).ok_or("the pinned module key is not valid base64")?;
    ensure_retrying(data, api, name, platform, abi, &key, &Retry::default(), out)
}

#[cfg(test)]
pub(crate) fn ensure_with(
    data: &Path,
    api: &Api,
    name: &str,
    platform: &str,
    abi: u32,
    key: &[u8],
) -> Result<Module, Unavailable> {
    ensure_retrying(data, api, name, platform, abi, key, &Retry::none(), &|_| {})
}

#[allow(clippy::too_many_arguments)]
fn ensure_retrying(
    data: &Path,
    api: &Api,
    name: &str,
    platform: &str,
    abi: u32,
    key: &[u8],
    retry: &Retry,
    out: &dyn Fn(&str),
) -> Result<Module, Unavailable> {
    if !valid_name(name) || !valid_name(platform) {
        return Err("the module name is not a plain name".into());
    }
    let source = Server { name, platform, abi };
    let f = fetch::ensure(data, api, &source, key, retry, &format!("module {name}"), out)?;
    if let Some(why) = &f.fallback {
        out(&format!(
            "module {name}: the server's copy could not be had ({why}); using the cached copy {}, verified offline",
            f.version
        ));
    }
    Ok(Module {
        name: name.to_owned(),
        version: f.version,
        path: f.path,
        sha256: f.sha256,
    })
}

/// One server module, as [`fetch`] asks for it.
struct Server<'a> {
    name: &'a str,
    platform: &'a str,
    abi: u32,
}

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
        &self.file
    }
    fn cdn_url(&self) -> Option<&str> {
        self.cdn_url.as_deref()
    }
}

impl fetch::Source for Server<'_> {
    type Offer = Offer;

    fn base(&self, data: &Path) -> PathBuf {
        dir(data, self.name, self.platform)
    }

    fn ask(&self, api: &Api, timeout: Duration) -> Result<serde_json::Value, ApiError> {
        api.modules(self.platform, &[(self.name, self.abi)], timeout)
    }

    fn pick(&self, answer: serde_json::Value) -> Result<Offer, Unavailable> {
        let listing: Listing =
            serde_json::from_value(answer).map_err(|e| format!("the module listing is not valid: {e}"))?;
        let Some(offer) = listing.modules.into_iter().find(|m| m.name == self.name) else {
            return Err(listing
                .refused
                .into_iter()
                .find(|r| r.name == self.name && !r.why.trim().is_empty())
                .map_or_else(
                    || Unavailable::refused("the server did not offer it"),
                    |r| Unavailable::refused(r.why.trim()),
                ));
        };
        if offer.platform != self.platform || offer.abi != self.abi {
            return Err("the offer is for another platform or abi".into());
        }
        Ok(offer)
    }

    fn refused(&self, e: ApiError) -> Unavailable {
        if e.status == 404 {
            return Unavailable::answered(ApiError {
                message: "the server does not offer modules".into(),
                ..e
            });
        }
        Unavailable::answered(e)
    }

    fn check(&self, key: &[u8], offer: &Offer) -> Result<(), Unavailable> {
        if offer.name != self.name || offer.platform != self.platform || offer.abi != self.abi {
            return Err("the offer is for another module, platform or abi".into());
        }
        check(key, offer)
    }

    fn download(&self, api: &Api, offer: &Offer, timeout: Duration) -> Result<Vec<u8>, ApiError> {
        api.module_file(self.name, self.platform, &offer.version, offer.size, timeout)
    }

    fn download_timeout(&self) -> Duration {
        DOWNLOAD_TIMEOUT
    }
}

/// Everything about an offer that can be judged without the file: its fields are plain and its signature
/// is the pinned key's.
fn check(key: &[u8], offer: &Offer) -> Result<(), Unavailable> {
    if !valid_name(&offer.name) || !valid_name(&offer.platform) {
        return Err("the offered name is not a plain name".into());
    }
    if !valid_version(&offer.version) {
        return Err("the offered version is not a plain version string".into());
    }
    if !valid_version(&offer.file) || offer.file == OFFER_FILE {
        return Err("the offered file name is not a plain file name".into());
    }
    fetch::plain(&offer.version, &offer.sha256, offer.size, MAX_SIZE)?;
    fetch::signed_by(key, &signed_message(offer), &offer.signature)
}

fn signed_message(offer: &Offer) -> Vec<u8> {
    [
        SIGNED_PREFIX,
        &offer.name,
        &offer.platform,
        &offer.version,
        &offer.abi.to_string(),
        &offer.sha256,
    ]
    .join("\0")
    .into_bytes()
}

/// A module or platform name becomes a folder name and part of a URL: lowercase words joined by dashes.
fn valid_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

/// Add `module` to a build's placed files and plugin list so the loader loads it as the addon `id`.
pub fn place(module: &Module, id: &str, loader: &mut BTreeMap<String, PathBuf>, plugins: &mut Vec<String>) {
    place_file(id, &module.file_name(), &module.path, loader, plugins);
}

/// Add the plugin file at `source` to a build as `<addons>/<id>/<file>`.
pub fn place_file(
    id: &str,
    file: &str,
    source: &Path,
    loader: &mut BTreeMap<String, PathBuf>,
    plugins: &mut Vec<String>,
) {
    let rel = format!("{id}/{file}");
    loader.insert(
        format!(
            "{}/{}/{rel}",
            crate::install::LOADER_DIR,
            crate::build::ADDONS_DIR
        ),
        source.to_path_buf(),
    );
    plugins.push(format!("{}/{rel}", crate::build::ADDONS_DIR));
}

#[cfg(test)]
pub(crate) mod tests;
