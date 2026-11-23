use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::game_folder::baseline::{sha256_bytes, sha256_file};
use crate::updates::transfer;
use crate::{Error, Result, io_err, with_retry};

use super::{ANTIVIRUS, EXTENSION_VERSION, REQUIRED, av_error, base_url, extension_dir, pin, virus_blocked};

/// Verified artifact paths inside the extension directory.
#[derive(Debug, Clone)]
pub struct Artifacts {
    pub dir: PathBuf,
    pub hook: PathBuf,
    pub stub: PathBuf,
    pub prelaunch: PathBuf,
    pub probe: PathBuf,
}

/// The two artifacts the manager runs: (prelaunch, probe).
#[cfg(windows)]
pub(super) const RUNNERS: [&str; 2] = ["f1op-prelaunch.exe", "f1op-probe.exe"];
#[cfg(not(windows))]
pub(super) const RUNNERS: [&str; 2] = ["f1op-prelaunch-linux-x86_64", "f1op-probe-linux-x86_64"];

fn verify_digest(path: &Path, name: &str, want: &str) -> Result<()> {
    let have = sha256_file(path).map_err(|e| match e {
        Error::Io { source, .. } if virus_blocked(&source) => Error::Other(ANTIVIRUS.to_owned()),
        e => e,
    })?;
    match_digest(name, &have, want)
}

fn match_digest(name: &str, have: &str, want: &str) -> Result<()> {
    if have != want {
        return Err(Error::Other(format!(
            "{name}: digest mismatch (have {have}, want {want}); refusing to use it"
        )));
    }
    Ok(())
}

fn verify_file(path: &Path, name: &str) -> Result<()> {
    let want = pin(name).ok_or_else(|| Error::Other(format!("no pinned digest for {name}")))?;
    verify_digest(path, name, want)
}

/// The files this platform needs, each with its pinned digest.
fn pinned_files() -> Result<Vec<(&'static str, &'static str)>> {
    REQUIRED
        .iter()
        .map(|name| {
            pin(name)
                .map(|digest| (*name, digest))
                .ok_or_else(|| Error::Other(format!("no pinned digest for {name}")))
        })
        .collect()
}

/// Downloads and copies do not carry the execute bit, and the runners need it
/// on Unix.
pub(super) fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path).map_err(io_err(path))?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms).map_err(io_err(path))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Re-verify every required artifact before use; tampering after install is
/// caught here.
pub fn artifacts(data: &Path) -> Result<Artifacts> {
    let dir = extension_dir(data);
    for name in REQUIRED {
        let path = dir.join(name);
        if !path.is_file() {
            return Err(Error::Other(format!(
                "offline play extension is incomplete: {} is missing",
                path.display()
            )));
        }
        verify_file(&path, name)?;
    }
    let [prelaunch, probe] = RUNNERS;
    let out = Artifacts {
        hook: dir.join("f1op_hook.dll"),
        stub: dir.join("f1op_stub.exe"),
        prelaunch: dir.join(prelaunch),
        probe: dir.join(probe),
        dir,
    };
    // Re-applied on every use so an older install without the bit heals
    // itself without being downloaded again.
    set_executable(&out.prelaunch)?;
    set_executable(&out.probe)?;
    Ok(out)
}

/// Where a release is assembled before it replaces the installed one. It sits
/// next to the extension directory so the final rename stays on one
/// filesystem.
pub(super) fn staging_dir(data: &Path) -> PathBuf {
    data.join("extensions/.offline-play.partial")
}

fn fresh_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(io_err(dir))?;
    }
    std::fs::create_dir_all(dir).map_err(io_err(dir))
}

/// Verify a fully assembled release in the staging directory, then swap it in
/// for the installed one. Until the swap the installed copy is untouched; a
/// crash between the two renames leaves no extension directory, which reads
/// as "not installed" and is fetched again.
fn commit(data: &Path, staging: &Path, files: &[(&str, &str)]) -> Result<()> {
    for (name, digest) in files {
        verify_digest(&staging.join(name), name, digest)?;
    }
    for name in RUNNERS {
        let path = staging.join(name);
        if path.is_file() {
            set_executable(&path)?;
        }
    }
    let dir = extension_dir(data);
    let old = data.join("extensions/.offline-play.old");
    if old.exists() {
        std::fs::remove_dir_all(&old).map_err(io_err(&old))?;
    }
    if dir.exists() {
        with_retry(&dir, || std::fs::rename(&dir, &old))?;
    }
    if let Err(e) = with_retry(&dir, || std::fs::rename(staging, &dir)) {
        if old.exists() {
            std::fs::rename(&old, &dir).ok();
        }
        return Err(e);
    }
    std::fs::remove_dir_all(&old).ok();
    Ok(())
}

/// Verify a downloaded release folder against the pins and install it.
pub fn install(data: &Path, src: &Path) -> Result<Artifacts> {
    install_files(data, src, &pinned_files()?)?;
    artifacts(data)
}

pub(super) fn install_files(data: &Path, src: &Path, files: &[(&str, &str)]) -> Result<()> {
    let staging = staging_dir(data);
    fresh_dir(&staging)?;
    let done = files
        .iter()
        .try_for_each(|(name, _)| {
            let from = src.join(name);
            if !from.is_file() {
                return Err(Error::Other(format!("{name}: not found in {}", src.display())));
            }
            let to = staging.join(name);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(io_err(parent.to_path_buf()))?;
            }
            std::fs::copy(&from, &to).map(|_| ()).map_err(io_err(&to))
        })
        .and_then(|()| commit(data, &staging, files))
        .map_err(av_error);
    std::fs::remove_dir_all(&staging).ok();
    done
}

/// Download the pinned release from the artifact host and install it. The
/// digests decide what is trusted; the host is only a transport.
pub fn fetch(data: &Path) -> Result<Artifacts> {
    fetch_release(
        data,
        &base_url(),
        EXTENSION_VERSION,
        &pinned_files()?,
        &mut download,
        &std::thread::sleep,
    )?;
    artifacts(data)
}

/// Where downloaded files that match their pins wait for the rest of the
/// release. A download that breaks off keeps them, so the next one fetches
/// only what is still missing.
pub(super) fn download_dir(data: &Path) -> PathBuf {
    data.join("extensions/.offline-play.download")
}

/// Bring every file into the download folder, kept from an earlier try when it
/// still matches its pin, then verify them all and only then replace the
/// installed extension. Any failure leaves the installed copy as it was and
/// names the file and the reason.
pub(super) fn fetch_release(
    data: &Path,
    base: &str,
    version: &str,
    files: &[(&str, &str)],
    get: &mut dyn FnMut(&str) -> std::result::Result<Vec<u8>, Failure>,
    sleep: &dyn Fn(Duration),
) -> Result<()> {
    // Earlier launchers downloaded into this folder and could leave it behind.
    std::fs::remove_dir_all(data.join("extensions/.download")).ok();
    let dir = download_dir(data);
    files
        .iter()
        .try_for_each(|(name, digest)| {
            let to = dir.join(name);
            if verify_digest(&to, name, digest).is_ok() {
                return Ok(());
            }
            let url = format!("{}/v{version}/{name}", base.trim_end_matches('/'));
            let body = download_retrying(&url, get, sleep).map_err(|(f, attempts)| {
                Error::Other(format!(
                    "offline play {version}: {name}{}: {f}",
                    transfer::attempts_note(attempts)
                ))
            })?;
            match_digest(name, &sha256_bytes(&body), digest)
                .map_err(|e| Error::Other(format!("offline play {version}: {url}: {e}")))?;
            keep(&to, &body)
        })
        .and_then(|()| install_files(data, &dir, files))
        .map_err(av_error)?;
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// `get` for `url`, tried again with the shared backoff while it fails in a
/// way the next try can get past. The failure carries the attempts made.
fn download_retrying(
    url: &str,
    get: &mut dyn FnMut(&str) -> std::result::Result<Vec<u8>, Failure>,
    sleep: &dyn Fn(Duration),
) -> std::result::Result<Vec<u8>, (Failure, u32)> {
    let last = std::cell::RefCell::new(String::new());
    let (result, attempts) = transfer::retrying(
        || {
            get(url).inspect_err(|f| {
                last.replace(f.detail.clone());
            })
        },
        |r| r.as_ref().is_err_and(|f| f.transient),
        |wait| {
            eprintln!(
                "swofty-core: {}; trying again in {} ms",
                last.borrow(),
                wait.as_millis()
            );
            sleep(wait);
        },
    );
    result.map_err(|f| (f, attempts))
}

/// Write `body` under a temporary name, flush it to disk and only then give it
/// its own name, so a file found under its own name is never half written.
fn keep(to: &Path, body: &[u8]) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(io_err(parent))?;
    }
    let mut part = to.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let kept = std::fs::File::create(&part)
        .and_then(|mut f| {
            std::io::Write::write_all(&mut f, body)?;
            f.sync_all()
        })
        .map_err(io_err(&part))
        .and_then(|()| with_retry(to, || std::fs::rename(&part, to)));
    if kept.is_err() {
        std::fs::remove_file(&part).ok();
    }
    kept
}

/// The reason given when the host could not be reached or its TLS failed. The
/// common case is something on the PC or network answering on 443 in its place.
pub(super) const BLOCKED: &str = "the download was blocked on this PC or network, typically by antivirus HTTPS or web \
     scanning, a VPN or proxy, a network filter or a Wi-Fi login page; turning that off or trying another \
     network fixes it";

/// Why a download brought no file, and whether trying again can get past it.
#[derive(Debug)]
pub(super) struct Failure {
    pub(super) detail: String,
    pub(super) blocked: bool,
    pub(super) transient: bool,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.blocked {
            write!(f, "{BLOCKED}\n({})", self.detail)
        } else {
            f.write_str(&self.detail)
        }
    }
}

impl Failure {
    /// A request that got no answer. A connection, DNS or TLS failure leads with
    /// [`BLOCKED`]; one that broke or timed out on the way is tried again.
    pub(super) fn call(url: &str, e: &ureq::Error) -> Self {
        Self {
            detail: format!("could not download {url}: {e}"),
            blocked: transfer::transport(e),
            transient: transfer::retryable_error(e) && !final_failure(e),
        }
    }

    /// An answer that is not the file.
    pub(super) fn status(url: &str, status: u16) -> Self {
        let detail = if status == 404 {
            format!("{url}: HTTP {status}: the release on the host has no such file")
        } else {
            format!("{url}: HTTP {status}")
        };
        Self {
            detail,
            blocked: false,
            transient: transfer::retryable_status(status),
        }
    }

    /// A body that stopped before its end.
    pub(super) fn cut_off(detail: String, e: Option<&std::io::Error>) -> Self {
        let inner = e.and_then(|e| e.get_ref()?.downcast_ref::<ureq::Error>());
        Self {
            detail,
            blocked: false,
            transient: e.is_none_or(transfer::broken) && !inner.is_some_and(final_failure),
        }
    }
}

/// A failure the next try would meet again: the connection was refused, or the
/// file was still unfinished when the whole timeout ran out.
fn final_failure(e: &ureq::Error) -> bool {
    match e {
        ureq::Error::Timeout(ureq::Timeout::Global) => true,
        ureq::Error::Io(e) => e.kind() == std::io::ErrorKind::ConnectionRefused,
        _ => false,
    }
}

pub(super) fn download(url: &str) -> std::result::Result<Vec<u8>, Failure> {
    let resp = ureq::get(url)
        .config()
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_global(Some(Duration::from_secs(300)))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|e| Failure::call(url, &e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(Failure::status(url, status.as_u16()));
    }
    let expected = resp.body().content_length();
    let mut body = Vec::new();
    let read = std::io::Read::read_to_end(&mut resp.into_body().into_reader(), &mut body);
    if let Err(e) = read {
        return Err(Failure::cut_off(
            format!("{url}: the download was cut off after {} bytes: {e}", body.len()),
            Some(&e),
        ));
    }
    if let Some(n) = expected
        && body.len() as u64 != n
    {
        return Err(Failure::cut_off(
            format!("{url}: the download was cut off ({} of {n} bytes)", body.len()),
            None,
        ));
    }
    Ok(body)
}
