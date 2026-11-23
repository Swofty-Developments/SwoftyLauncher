use std::path::{Path, PathBuf};

use crate::game_folder::baseline::sha256_file;
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
    fetch_release(data, &base_url(), EXTENSION_VERSION, &pinned_files()?)?;
    artifacts(data)
}

/// Download every file into the staging directory, verify each one, and only
/// then replace the installed extension. Any failure leaves the installed copy
/// as it was and names the file and the reason.
pub(super) fn fetch_release(data: &Path, base: &str, version: &str, files: &[(&str, &str)]) -> Result<()> {
    // Earlier launchers downloaded into this folder and could leave it behind.
    std::fs::remove_dir_all(data.join("extensions/.download")).ok();
    let staging = staging_dir(data);
    fresh_dir(&staging)?;
    let done = files
        .iter()
        .try_for_each(|(name, digest)| {
            let url = format!("{}/v{version}/{name}", base.trim_end_matches('/'));
            let to = staging.join(name);
            download(&url, &to).map_err(|e| Error::Other(format!("offline play {version}: {e}")))?;
            verify_digest(&to, name, digest)
                .map_err(|e| Error::Other(format!("offline play {version}: {url}: {e}")))
        })
        .and_then(|()| commit(data, &staging, files))
        .map_err(av_error);
    std::fs::remove_dir_all(&staging).ok();
    done
}

/// The reason given when the host could not be reached or its TLS failed. The
/// common case is something on the PC or network answering on 443 in its place.
pub(super) const BLOCKED: &str = "the download was blocked on this PC or network, typically by antivirus HTTPS or web \
     scanning, a VPN or proxy, a network filter or a Wi-Fi login page; turning that off or trying another \
     network fixes it";

/// A connection, DNS or TLS failure, as opposed to a reply from the host. A
/// reply that isn't TLS (rustls `InvalidContentType`) arrives as `Io`.
fn blocked(e: &ureq::Error) -> bool {
    matches!(
        e,
        ureq::Error::Io(_)
            | ureq::Error::Rustls(_)
            | ureq::Error::Tls(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::ConnectProxyFailed(_)
            | ureq::Error::Timeout(_)
    )
}

pub(super) fn download(url: &str, to: &Path) -> std::result::Result<(), String> {
    let resp = ureq::get(url)
        .config()
        .timeout_connect(Some(std::time::Duration::from_secs(30)))
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|e| {
            let what = format!("could not download {url}: {e}");
            if blocked(&e) {
                format!("{BLOCKED}\n({what})")
            } else {
                what
            }
        })?;
    let status = resp.status();
    if status == 404 {
        return Err(format!(
            "{url}: HTTP {status}: the release on the host has no such file"
        ));
    }
    if !status.is_success() {
        return Err(format!("{url}: HTTP {status}"));
    }
    let expected = resp.body().content_length();
    let mut body = Vec::new();
    let read = std::io::Read::read_to_end(&mut resp.into_body().into_reader(), &mut body);
    if let Err(e) = read {
        return Err(format!(
            "{url}: the download was cut off after {} bytes: {e}",
            body.len()
        ));
    }
    if let Some(n) = expected
        && body.len() as u64 != n
    {
        return Err(format!(
            "{url}: the download was cut off ({} of {n} bytes)",
            body.len()
        ));
    }
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(to, &body).map_err(|e| {
        if virus_blocked(&e) {
            ANTIVIRUS.to_owned()
        } else {
            format!("{}: {e}", to.display())
        }
    })
}
