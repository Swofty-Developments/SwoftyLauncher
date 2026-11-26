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
    get: &mut Get,
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
            let body = download_verified(&url, name, digest, get, sleep).map_err(|e| match e {
                Fetched::Failed(f, attempts) => Error::Other(format!(
                    "offline play {version}: {name}{}: {f}",
                    transfer::attempts_note(attempts)
                )),
                Fetched::Mismatch(e) => Error::Other(format!("offline play {version}: {url}: {e}")),
            })?;
            keep(&to, &body)
        })
        .and_then(|()| install_files(data, &dir, files))
        .map_err(av_error)?;
    std::fs::remove_dir_all(&dir).ok();
    Ok(())
}

/// Fetches a file into the bytes already held of it.
pub(super) type Get<'a> = dyn FnMut(&str, &mut Vec<u8>) -> std::result::Result<(), Failure> + 'a;

/// Attempts one file may make in all, however much each one delivers.
const MOST_ATTEMPTS: u32 = 20;

/// Why a file did not arrive intact.
enum Fetched {
    Failed(Failure, u32),
    Mismatch(Error),
}

/// A file that matches `digest`. One that was joined from several partial
/// downloads and does not match is fetched again from its first byte before
/// the mismatch counts, so a bad splice cannot fail an install the host could
/// serve.
fn download_verified(
    url: &str,
    name: &str,
    digest: &str,
    get: &mut Get,
    sleep: &dyn Fn(Duration),
) -> std::result::Result<Vec<u8>, Fetched> {
    let mut last = None;
    for _ in 0..2 {
        let (body, attempts) = download_retrying(url, get, sleep).map_err(|(f, n)| Fetched::Failed(f, n))?;
        match match_digest(name, &sha256_bytes(&body), digest) {
            Ok(()) => return Ok(body),
            Err(e) if attempts == 1 => return Err(Fetched::Mismatch(e)),
            Err(e) => {
                eprintln!("swofty-core: {name}: {e}; downloading it again from the start");
                last = Some(e);
            }
        }
    }
    Err(Fetched::Mismatch(last.expect("two tries ran")))
}

/// `get` for `url`, tried again with the shared backoff while it fails in a
/// way the next try can get past. `get` appends to the body and is called with
/// what earlier tries delivered, so it can ask for the rest. The wait counts
/// tries in a row that brought no new byte; one that did starts it over. The
/// failure carries the attempts made.
fn download_retrying(
    url: &str,
    get: &mut Get,
    sleep: &dyn Fn(Duration),
) -> std::result::Result<(Vec<u8>, u32), (Failure, u32)> {
    let mut body = Vec::new();
    let (mut attempts, mut streak) = (0, 0);
    loop {
        attempts += 1;
        let before = body.len();
        let failure = match get(url, &mut body) {
            Ok(()) => return Ok((body, attempts)),
            Err(f) => f,
        };
        streak = if body.len() > before { 1 } else { streak + 1 };
        let wait = transfer::retry_delay(streak).filter(|_| failure.transient && attempts < MOST_ATTEMPTS);
        let Some(wait) = wait else {
            return Err((failure, attempts));
        };
        eprintln!(
            "swofty-core: {}; trying again in {} ms{}",
            failure.detail,
            wait.as_millis(),
            if body.is_empty() {
                String::new()
            } else {
                format!(" from byte {}", body.len())
            }
        );
        sleep(wait);
    }
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

/// The reason given when a download keeps being cut off part-way, or goes quiet,
/// on a connection that does open.
pub(super) const CUT_OFF: &str = "the download kept being cut off before it finished; check for a VPN or proxy, antivirus \
     web filtering or DNS trouble on this PC or network, turn it off or try another network such as a phone \
     hotspot, then try again";

/// How long a body may go without a byte before the connection counts as dead.
const STALL: Duration = if cfg!(test) {
    Duration::from_millis(400)
} else {
    Duration::from_secs(30)
};

/// Why a download brought no file, and whether trying again can get past it.
#[derive(Debug)]
pub(super) struct Failure {
    pub(super) detail: String,
    pub(super) blocked: bool,
    pub(super) cut_off: bool,
    pub(super) transient: bool,
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.blocked {
            write!(f, "{BLOCKED}\n({})", self.detail)
        } else if self.cut_off {
            write!(f, "{CUT_OFF}\n({})", self.detail)
        } else {
            f.write_str(&self.detail)
        }
    }
}

impl Failure {
    /// A request that got no answer. A connection, DNS or TLS failure leads with
    /// [`BLOCKED`]; one that broke or timed out on the way is tried again, and one
    /// that went quiet before its headers arrived ends with [`CUT_OFF`].
    pub(super) fn call(url: &str, e: &ureq::Error) -> Self {
        let quiet = matches!(
            e,
            ureq::Error::Timeout(ureq::Timeout::PerRead | ureq::Timeout::RecvResponse)
        );
        Self {
            detail: format!("could not download {url}: {e}"),
            blocked: transfer::transport(e) && !quiet,
            cut_off: quiet,
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
            cut_off: false,
            transient: transfer::retryable_status(status),
        }
    }

    /// A body that stopped before its end, or went quiet for [`STALL`]. The
    /// bytes it delivered stay with the caller, and the next try asks for the
    /// rest.
    pub(super) fn cut_off(detail: String, e: Option<&std::io::Error>) -> Self {
        let inner = e.and_then(|e| e.get_ref()?.downcast_ref::<ureq::Error>());
        Self {
            detail,
            blocked: false,
            cut_off: true,
            transient: e.is_none_or(transfer::broken) && !inner.is_some_and(final_failure),
        }
    }

    /// A server that answered a request for the rest of a file with something
    /// else; the next try asks for the whole file.
    fn restart(detail: String) -> Self {
        Self {
            detail,
            blocked: false,
            cut_off: false,
            transient: true,
        }
    }
}

/// A failure the next try would meet again: the connection was refused.
fn final_failure(e: &ureq::Error) -> bool {
    matches!(e, ureq::Error::Io(e) if e.kind() == std::io::ErrorKind::ConnectionRefused)
}

/// The first byte of a `Content-Range: bytes <first>-<last>/<size>`.
fn range_start(range: &str) -> Option<u64> {
    let (span, _) = range.trim().strip_prefix("bytes ")?.split_once('/')?;
    span.split_once('-')?.0.trim().parse().ok()
}

/// Appends the file at `url` to `body`. A `body` that already holds the start
/// of the file is continued with a Range request from its end; a server that
/// answers with the whole file instead has `body` replaced by it. A body that
/// breaks off leaves what arrived in `body`.
pub(super) fn download(url: &str, body: &mut Vec<u8>) -> std::result::Result<(), Failure> {
    let from = body.len();
    let mut request = ureq::get(url);
    if from > 0 {
        request = request.header("Range", format!("bytes={from}-"));
    }
    let resp = request
        .config()
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_per_read(Some(STALL))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|e| Failure::call(url, &e))?;
    let status = resp.status().as_u16();
    if status == 416 && from > 0 {
        body.clear();
        return Err(Failure::restart(format!(
            "{url}: the server would not continue at byte {from}; starting over"
        )));
    }
    if !resp.status().is_success() {
        return Err(Failure::status(url, status));
    }
    if status == 206 {
        let start = resp
            .headers()
            .get("content-range")
            .and_then(|h| h.to_str().ok())
            .and_then(range_start);
        if from == 0 || start != Some(from as u64) {
            body.clear();
            return Err(Failure::restart(format!(
                "{url}: the server sent another part of the file than byte {from} on; starting over"
            )));
        }
    } else {
        body.clear();
    }
    let kept = body.len();
    let expected = resp.body().content_length();
    let read = std::io::Read::read_to_end(&mut resp.into_body().into_reader(), body);
    if let Err(e) = read {
        return Err(Failure::cut_off(
            format!("{url}: the download was cut off after {} bytes: {e}", body.len()),
            Some(&e),
        ));
    }
    if let Some(n) = expected
        && (body.len() - kept) as u64 != n
    {
        return Err(Failure::cut_off(
            format!(
                "{url}: the download was cut off ({} of {} bytes)",
                body.len() - kept,
                n
            ),
            None,
        ));
    }
    Ok(())
}
