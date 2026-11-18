use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use crate::erp::fragment::{Fragment, Payload};
use crate::erp::{ZSTD_LEVEL, ZSTD_WINDOW_LOG};
use crate::{Error, Result};

pub fn compress(data: &[u8]) -> Result<Vec<u8>> {
    Packer::default().compress(data)
}

pub fn compress_at(data: &[u8], level: i32) -> Result<Vec<u8>> {
    Packer::default().compress_at(data, level)
}

/// [`Packer::compress_many`] without a folder of frames to reuse.
pub fn compress_many(inputs: &[&[u8]], level: i32) -> Result<Vec<Vec<u8>>> {
    Packer::default().compress_many(inputs, level)
}

/// How the launcher compresses the fragments it changes: with the folder of frames it reuses between
/// builds, and whether compression waits for a [`Packer::settle`], which compresses every fragment
/// pending at one level together. Deferred or not, a fragment ends as the same bytes.
#[derive(Debug, Clone, Default)]
pub struct Packer {
    cache: Option<PathBuf>,
    defer: bool,
}

impl Packer {
    /// A packer that reuses the frames in `dir` and keeps the ones it writes there.
    pub fn cached(dir: impl Into<PathBuf>) -> Self {
        Self {
            cache: Some(dir.into()),
            defer: false,
        }
    }

    /// This packer, leaving each zstd fragment it is given pending until a [`Packer::settle`] when `defer`.
    pub fn deferring(mut self, defer: bool) -> Self {
        self.defer = defer;
        self
    }

    pub fn defers(&self) -> bool {
        self.defer
    }

    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        self.compress_at(data, ZSTD_LEVEL)
    }

    pub fn compress_at(&self, data: &[u8], level: i32) -> Result<Vec<u8>> {
        match self.cache.as_deref().filter(|_| data.len() >= PACKED_MIN) {
            Some(dir) => compress_cached(dir, data, level),
            None => encode(data, level),
        }
    }

    /// Compresses each of `inputs` at `level` as [`Packer::compress_at`] does, several at a time, and
    /// returns them in the same order. Each frame is the same bytes whichever thread wrote it.
    pub fn compress_many(&self, inputs: &[&[u8]], level: i32) -> Result<Vec<Vec<u8>>> {
        let threads = parallelism().min(inputs.len());
        if threads <= 1 {
            return inputs.iter().map(|data| self.compress_at(data, level)).collect();
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done: Vec<std::sync::Mutex<Option<Result<Vec<u8>>>>> =
            inputs.iter().map(|_| std::sync::Mutex::new(None)).collect();
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    loop {
                        let at = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(data) = inputs.get(at) else { break };
                        *done[at].lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(self.compress_at(data, level));
                    }
                });
            }
        });
        done.into_iter()
            .map(|slot| {
                slot.into_inner()
                    .unwrap_or_else(|e| e.into_inner())
                    .expect("every input is compressed")
            })
            .collect()
    }

    /// Puts `data` in `f` with its size: compressed at `level` when `zstd`, else as it is. A deferring
    /// packer leaves a zstd fragment pending, and its readers see `data` until [`Packer::settle`].
    pub fn put(&self, f: &mut Fragment, data: &[u8], zstd: bool, level: i32) -> Result<()> {
        f.payload = match zstd {
            false => Payload::Stored {
                packed: data.len() as u64,
                raw: data.to_vec(),
            },
            true if self.defer => Payload::Pending {
                data: data.to_vec(),
                level,
            },
            true => {
                let raw = self.compress_at(data, level)?;
                Payload::Stored {
                    packed: raw.len() as u64,
                    raw,
                }
            }
        };
        f.size = data.len() as u64;
        Ok(())
    }

    /// Compresses every pending fragment of `fragments`, one [`Packer::compress_many`] per level, and
    /// returns how many it compressed.
    pub fn settle<'a>(&self, fragments: impl IntoIterator<Item = &'a mut Fragment>) -> Result<usize> {
        let mut levels: BTreeMap<i32, Vec<&'a mut Fragment>> = BTreeMap::new();
        for f in fragments {
            if let Some((_, level)) = f.pending() {
                levels.entry(level).or_default().push(f);
            }
        }
        let mut settled = 0;
        for (level, frags) in levels {
            let packed = {
                let inputs: Vec<&[u8]> = frags
                    .iter()
                    .filter_map(|f| f.pending())
                    .map(|(data, _)| data)
                    .collect();
                self.compress_many(&inputs, level)?
            };
            settled += frags.len();
            for (f, raw) in frags.into_iter().zip(packed) {
                f.payload = Payload::Stored {
                    packed: raw.len() as u64,
                    raw,
                };
            }
        }
        Ok(settled)
    }
}

/// How many compressions run at once, and how many workers one large frame is split across: half the
/// cores, at most four, so a build leaves the rest of the PC responsive.
fn parallelism() -> usize {
    static N: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *N.get_or_init(|| std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4)))
}

static WORK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A count that grows as fragments are compressed and archives written, so a
/// caller can tell a long build that is working from one that is stuck.
pub fn work() -> u64 {
    WORK.load(std::sync::atomic::Ordering::Relaxed)
}

pub(super) fn worked() {
    WORK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub(super) const PACKED_MIN: usize = 16 << 10;

pub(super) type Encoder = zstd::zstd_safe::CCtx<'static>;

/// Compression contexts kept between fragments, by level. Setting one up
/// starts a worker thread and allocates its window, which cost as much as
/// compressing a small fragment. A reused context keeps its parameters, so it
/// writes the same bytes as a new one.
pub(super) static ENCODERS: std::sync::Mutex<Vec<(i32, Encoder)>> = std::sync::Mutex::new(Vec::new());

fn zstd_error(code: usize) -> Error {
    Error::Format(format!("zstd: {}", zstd::zstd_safe::get_error_name(code)))
}

/// The settings `zstd -c -<level> --zstd=wlog=20 --stream-size=<n>` compresses
/// with: a checksum, the content size, a 1 MiB window, and the command line's
/// multithreaded path, whose output differs from the single-threaded encoder's
/// on large inputs. That path cuts a frame into jobs by size alone, so its
/// output is the same for any number of workers; more workers only finish a
/// frame of several jobs sooner.
fn encoder(level: i32) -> Result<Encoder> {
    use zstd::zstd_safe::CParameter;
    let mut ctx =
        Encoder::try_create().ok_or_else(|| Error::Format("zstd: no memory for an encoder".into()))?;
    for p in [
        CParameter::CompressionLevel(level),
        CParameter::WindowLog(ZSTD_WINDOW_LOG),
        CParameter::ContentSizeFlag(true),
        CParameter::ChecksumFlag(true),
        CParameter::NbWorkers(parallelism() as u32),
    ] {
        ctx.set_parameter(p).map_err(zstd_error)?;
    }
    Ok(ctx)
}

pub(super) fn encode(data: &[u8], level: i32) -> Result<Vec<u8>> {
    use zstd::zstd_safe::{InBuffer, OutBuffer, ResetDirective, zstd_sys::ZSTD_EndDirective};
    worked();
    let pooled = {
        let mut pool = ENCODERS.lock().unwrap_or_else(|e| e.into_inner());
        pool.iter()
            .position(|(l, _)| *l == level)
            .map(|i| pool.swap_remove(i).1)
    };
    let mut ctx = match pooled {
        Some(ctx) => ctx,
        None => encoder(level)?,
    };
    ctx.reset(ResetDirective::SessionOnly).map_err(zstd_error)?;
    ctx.set_pledged_src_size(Some(data.len() as u64))
        .map_err(zstd_error)?;
    let mut out: Vec<u8> = Vec::with_capacity(zstd::compress_bound(data.len()) + 64);
    let mut input = InBuffer::around(data);
    // The input goes in first and the frame ends after, as the command line
    // does with a pledged size, so the bytes match the command line's.
    while input.pos() < data.len() {
        if out.capacity() - out.len() < (1 << 16) {
            out.reserve(1 << 16);
        }
        let at = out.len();
        ctx.compress_stream2(
            &mut OutBuffer::around_pos(&mut out, at),
            &mut input,
            ZSTD_EndDirective::ZSTD_e_continue,
        )
        .map_err(zstd_error)?;
    }
    let mut end = InBuffer::around(&[]);
    loop {
        let at = out.len();
        let left = ctx
            .compress_stream2(
                &mut OutBuffer::around_pos(&mut out, at),
                &mut end,
                ZSTD_EndDirective::ZSTD_e_end,
            )
            .map_err(zstd_error)?;
        if left == 0 {
            break;
        }
        out.reserve(left.max(1 << 16));
    }
    ENCODERS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push((level, ctx));
    Ok(out)
}

pub(super) fn compress_cached(dir: &Path, data: &[u8], level: i32) -> Result<Vec<u8>> {
    let digest: String = Md5::digest(data).iter().map(|b| format!("{b:02x}")).collect();
    let name = format!(
        "{digest}-{}-{level}-w{ZSTD_WINDOW_LOG}-{}",
        data.len(),
        zstd::zstd_safe::version_number()
    );
    let path = dir.join(&name[..2]).join(&name);
    if let Ok(raw) = std::fs::read(&path)
        && zstd::decode_all(raw.as_slice()).is_ok_and(|back| back == data)
    {
        if let Ok(f) = std::fs::File::options().write(true).open(&path) {
            f.set_modified(std::time::SystemTime::now()).ok();
        }
        return Ok(raw);
    }
    let raw = encode(data, level)?;
    static TMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = TMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{n}.tmp", std::process::id()));
    let stored = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&tmp, &raw))
        .and_then(|()| std::fs::rename(&tmp, &path));
    if stored.is_err() {
        std::fs::remove_file(&tmp).ok();
    }
    Ok(raw)
}
