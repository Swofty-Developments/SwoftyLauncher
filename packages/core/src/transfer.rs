//! Downloads to a file that survive a broken connection: a body cut off
//! part-way is asked for again from the bytes already written, with an HTTP
//! Range request, and the SHA-256 covers the whole file.

use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::{Error, Result, io_err};

/// Attempts in a row that may end without a new byte before a download gives up.
const TRIES: u32 = 4;
/// Attempts one download may make in all.
const MOST: u32 = 12;
/// The wait before the first retry; each retry in a row without a new byte
/// waits twice as long as the one before.
const BACKOFF: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_secs(2)
};

/// What a download that kept breaking off says to do.
pub const ADVICE: &str = "the connection kept breaking during the download, usually from antivirus HTTPS or web \
     scanning, a VPN or a proxy; turn those off or try another network, such as a phone hotspot, then try again";

/// A file to download.
pub(crate) struct Fetch<'a> {
    pub url: &'a str,
    pub size: u64,
    /// What lists `size`, such as "the registry", for the errors.
    pub lister: &'a str,
    /// The error for a body longer than `size`.
    pub too_long: &'a dyn Fn() -> String,
}

/// Why a request got no usable answer.
pub(crate) enum Opening {
    Call(ureq::Error),
    Refused(Error),
}

enum Attempt {
    Broke(String),
    Failed(Error),
}

/// Downloads `f` to `to` with `open`, which sends the request, with a
/// `Range` from the given byte when it isn't 0, and turns an HTTP error into
/// [`Opening::Refused`]. A connection or TLS failure is tried again, resuming
/// where the file stops when the server answers the Range. Returns the length
/// and SHA-256 of the file.
pub(crate) fn fetch(
    f: &Fetch,
    to: &Path,
    open: &mut dyn FnMut(u64) -> std::result::Result<ureq::http::Response<ureq::Body>, Opening>,
    progress: &dyn Fn(u64),
) -> Result<(u64, String)> {
    let mut file = std::fs::File::create(to).map_err(io_err(to))?;
    let mut got = Got {
        hash: Sha256::new(),
        len: 0,
        resumable: true,
    };
    let (mut quiet, mut tries) = (0, 0);
    loop {
        tries += 1;
        let before = got.len;
        let broke = match attempt(f, &mut file, to, &mut got, open, progress) {
            Ok(()) => break,
            Err(Attempt::Failed(e)) => return Err(e),
            Err(Attempt::Broke(e)) => e,
        };
        quiet = if got.len > before { 0 } else { quiet + 1 };
        if quiet >= TRIES || tries >= MOST {
            return Err(Error::Other(format!("{ADVICE}\n({}: {broke})", f.url)));
        }
        eprintln!(
            "swofty-core: {}: {broke}; trying again from byte {}",
            f.url,
            if got.resumable { got.len } else { 0 }
        );
        std::thread::sleep(BACKOFF * 2u32.pow(quiet.saturating_sub(1)));
    }
    file.sync_all().map_err(io_err(to))?;
    let digest = got.hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok((got.len, digest))
}

struct Got {
    hash: Sha256,
    len: u64,
    resumable: bool,
}

fn attempt(
    f: &Fetch,
    file: &mut std::fs::File,
    to: &Path,
    got: &mut Got,
    open: &mut dyn FnMut(u64) -> std::result::Result<ureq::http::Response<ureq::Body>, Opening>,
    progress: &dyn Fn(u64),
) -> std::result::Result<(), Attempt> {
    let from = if got.resumable { got.len } else { 0 };
    let response = match open(from) {
        Ok(r) => r,
        Err(Opening::Call(e)) if transport(&e) => return Err(Attempt::Broke(e.to_string())),
        Err(Opening::Call(e)) => return Err(Attempt::Failed(Error::Other(format!("{}: {e}", f.url)))),
        Err(Opening::Refused(e)) => return Err(Attempt::Failed(e)),
    };
    let headers = response.headers();
    let header = |name: &str| headers.get(name).and_then(|h| h.to_str().ok());
    let partial = response.status().as_u16() == 206;
    if partial && (from == 0 || resumes_at(header("content-range"), f.size) != Some(from)) {
        return Err(Attempt::Failed(Error::Other(format!(
            "{}: the server sent another part of the file than byte {from} on",
            f.url
        ))));
    }
    if !partial && got.len > 0 {
        file.set_len(0)
            .and_then(|()| file.seek(SeekFrom::Start(0)).map(drop))
            .map_err(|e| Attempt::Failed(io_err(to)(e)))?;
        got.hash = Sha256::new();
        got.len = 0;
        progress(0);
    }
    let encoded = headers.contains_key("content-encoding");
    got.resumable = !encoded;
    let left = f.size - got.len;
    let sent = header("content-length").and_then(|h| h.trim().parse::<u64>().ok());
    if let Some(sent) = sent.filter(|s| !encoded && *s != left) {
        let after = if partial {
            format!(" after byte {from}")
        } else {
            String::new()
        };
        return Err(Attempt::Failed(Error::Other(format!(
            "{}: the server sends {sent} bytes{after}, {} lists {}",
            f.url, f.lister, f.size
        ))));
    }
    let mut body = response.into_body();
    let mut reader = body.with_config().limit(left + 1).reader();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(n) => n,
            Err(e) if broken(&e) => return Err(Attempt::Broke(e.to_string())),
            Err(e) => return Err(Attempt::Failed(Error::Other(format!("{}: {e}", f.url)))),
        };
        if n == 0 {
            break;
        }
        got.len += n as u64;
        if got.len > f.size {
            return Err(Attempt::Failed(Error::Other((f.too_long)())));
        }
        got.hash.update(&buf[..n]);
        file.write_all(&buf[..n])
            .map_err(|e| Attempt::Failed(io_err(to)(e)))?;
        progress(got.len);
    }
    if got.len < f.size && !encoded {
        return Err(Attempt::Broke(format!(
            "the connection closed after {} of {} bytes",
            got.len, f.size
        )));
    }
    Ok(())
}

/// The first byte of a `Content-Range: bytes <first>-<last>/<size>` that
/// runs to the end of a `size`-byte file.
fn resumes_at(range: Option<&str>, size: u64) -> Option<u64> {
    let (span, total) = range?.trim().strip_prefix("bytes ")?.split_once('/')?;
    let (first, last) = span.split_once('-')?;
    let (first, last) = (
        first.trim().parse::<u64>().ok()?,
        last.trim().parse::<u64>().ok()?,
    );
    let whole = total.trim() == "*" || total.trim().parse::<u64>().ok() == Some(size);
    (whole && last + 1 == size && first <= last).then_some(first)
}

/// A failure of the connection or its TLS, as opposed to an answer from the
/// server.
pub(crate) fn transport(e: &ureq::Error) -> bool {
    matches!(
        e,
        ureq::Error::Io(_)
            | ureq::Error::Rustls(_)
            | ureq::Error::Tls(_)
            | ureq::Error::Timeout(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::ConnectProxyFailed(_)
    )
}

/// A read of a body that failed on the connection or its TLS. ureq hands its
/// own errors, such as a body over its limit, through `io::Error`.
pub(crate) fn broken(e: &std::io::Error) -> bool {
    match e.get_ref().and_then(|inner| inner.downcast_ref::<ureq::Error>()) {
        Some(inner) => transport(inner),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_tls_and_connection_failures_as_transport() {
        let decrypt = std::io::Error::new(std::io::ErrorKind::InvalidData, rustls::Error::DecryptError);
        assert!(broken(&decrypt));
        assert_eq!(decrypt.to_string(), "cannot decrypt peer's message");
        let reset = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        assert!(broken(&reset));
        let eof = ureq::Error::Io(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)).into_io();
        assert!(broken(&eof));
        assert!(broken(&ureq::Error::Timeout(ureq::Timeout::RecvBody).into_io()));
        assert!(!broken(&ureq::Error::BodyExceedsLimit(10).into_io()));

        assert!(transport(&ureq::Error::Rustls(rustls::Error::DecryptError)));
        assert!(transport(&ureq::Error::ConnectionFailed));
        assert!(transport(&ureq::Error::HostNotFound));
        assert!(!transport(&ureq::Error::StatusCode(404)));
        assert!(!transport(&ureq::Error::BodyExceedsLimit(10)));
    }

    #[test]
    fn reads_where_a_range_answer_starts() {
        assert_eq!(resumes_at(Some("bytes 100-999/1000"), 1000), Some(100));
        assert_eq!(resumes_at(Some(" bytes 0-999/* "), 1000), Some(0));
        assert_eq!(resumes_at(Some("bytes 100-998/1000"), 1000), None);
        assert_eq!(resumes_at(Some("bytes 100-999/2000"), 1000), None);
        assert_eq!(resumes_at(Some("bytes */1000"), 1000), None);
        assert_eq!(resumes_at(Some("items 100-999/1000"), 1000), None);
        assert_eq!(resumes_at(None, 1000), None);
    }
}
