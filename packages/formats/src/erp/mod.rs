use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::bytes::Reader;
use crate::{Error, Result};
use md5::{Digest, Md5};

pub const MAGIC: &[u8; 4] = b"ERPK";
pub const HEADER_LEN: usize = 0x30;
pub const ZSTD: u8 = 0x11;
pub const STORED: u8 = 0x91;
pub const ZSTD_LEVEL: i32 = 19;
/// The window every frame is written with, whatever the level: 1 MiB, the
/// largest the game's own archives use. The game's stream decoder sizes its
/// buffer from a frame's window and content size, inside a fixed heap, so a
/// level-19 window (8 MiB) on every fragment runs that heap out.
pub const ZSTD_WINDOW_LOG: u32 = 20;
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
/// The waits between tries of an archive's open or replace that Windows
/// refused for now: an antivirus holds a file it scans for a moment.
const BACKOFF_MS: [u64; 6] = [25, 50, 100, 200, 400, 800];
/// The fewest index bytes a resource entry takes: its length, name length,
/// kind, the shorter type argument, fragment count and hash.
const MIN_ENTRY_LEN: usize = 4 + 2 + 16 + 4 + 1 + 16;

#[derive(Debug, Clone)]
pub struct Fragment {
    pub name: [u8; 4],
    pub offset: u64,
    pub size: u64,
    pub flags: u32,
    pub compression: u8,
    payload: Payload,
}

/// A fragment's bytes: as its archive stores them, or its data waiting for a [`Packer::settle`] that
/// compresses it. An archive is laid out from stored bytes only, so a pending fragment is compressed
/// before any write.
#[derive(Debug, Clone)]
enum Payload {
    /// The bytes and how many of them the archive holds; `raw` stays empty until they are loaded.
    Stored {
        packed: u64,
        raw: Vec<u8>,
    },
    Pending {
        data: Vec<u8>,
        level: i32,
    },
}

impl Fragment {
    /// A fragment stored as `raw`, at offset 0 until an archive lays it out.
    pub fn new(name: [u8; 4], size: u64, flags: u32, compression: u8, raw: Vec<u8>) -> Self {
        Self {
            name,
            offset: 0,
            size,
            flags,
            compression,
            payload: Payload::Stored {
                packed: raw.len() as u64,
                raw,
            },
        }
    }

    /// The bytes as the archive stores them; none while compression is pending.
    pub fn raw(&self) -> Option<&[u8]> {
        match &self.payload {
            Payload::Stored { raw, .. } => Some(raw),
            Payload::Pending { .. } => None,
        }
    }

    /// How many bytes the archive stores; none while compression is pending.
    pub fn packed(&self) -> Option<u64> {
        match self.payload {
            Payload::Stored { packed, .. } => Some(packed),
            Payload::Pending { .. } => None,
        }
    }

    /// The data a pending compression takes and the level it runs at; none once the fragment is stored.
    pub fn pending(&self) -> Option<(&[u8], i32)> {
        match &self.payload {
            Payload::Stored { .. } => None,
            Payload::Pending { data, level } => Some((data, *level)),
        }
    }

    /// Whether the fragment holds a zstd frame: by its compression, or by its bytes when the compression
    /// names neither zstd nor stored. A pending fragment is one.
    pub fn is_zstd(&self) -> bool {
        match &self.payload {
            Payload::Stored { raw, .. } => {
                self.compression == ZSTD || self.compression != STORED && raw.starts_with(&ZSTD_MAGIC)
            }
            Payload::Pending { .. } => true,
        }
    }

    fn stored(&self) -> Result<(u64, &[u8])> {
        match &self.payload {
            Payload::Stored { packed, raw } => Ok((*packed, raw)),
            Payload::Pending { .. } => Err(Error::Format(format!(
                "{}: a fragment's compression is still pending",
                self.name()
            ))),
        }
    }

    pub fn name(&self) -> String {
        String::from_utf8_lossy(&self.name)
            .trim_end_matches('\0')
            .to_owned()
    }

    pub fn data(&self) -> Result<Vec<u8>> {
        self.data_cow().map(std::borrow::Cow::into_owned)
    }

    pub(crate) fn data_cow(&self) -> Result<std::borrow::Cow<'_, [u8]>> {
        match &self.payload {
            Payload::Pending { data, .. } => Ok(std::borrow::Cow::Borrowed(data)),
            Payload::Stored { raw, .. } if self.is_zstd() => Ok(zstd::decode_all(raw.as_slice())?.into()),
            Payload::Stored { raw, .. } => Ok(std::borrow::Cow::Borrowed(raw)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub name: String,
    pub kind: [u8; 16],
    pub unk: Vec<u8>,
    pub fragments: Vec<Fragment>,
    pub hash: [u8; 16],
}

impl Resource {
    pub fn kind(&self) -> String {
        String::from_utf8_lossy(&self.kind)
            .trim_end_matches('\0')
            .to_owned()
    }

    pub fn content_hash(&self) -> Result<[u8; 16]> {
        let mut h = Md5::new();
        for f in &self.fragments {
            h.update(f.data()?);
        }
        Ok(h.finalize().into())
    }

    pub fn like(name: &str, template: &Resource, data: &[Vec<u8>]) -> Result<Self> {
        Self::like_with(&Packer::default(), name, template, data)
    }

    /// A resource named `name` with `template`'s kind and fragments holding `data`, compressed by `packer`.
    pub fn like_with(packer: &Packer, name: &str, template: &Resource, data: &[Vec<u8>]) -> Result<Self> {
        check_name(name)?;
        if data.len() != template.fragments.len() {
            return Err(Error::Format(format!(
                "{name}: {} fragment(s) given, a {} like {} has {}",
                data.len(),
                template.kind(),
                template.name,
                template.fragments.len()
            )));
        }
        let mut res = Resource {
            name: name.to_owned(),
            kind: template.kind,
            unk: template.unk.clone(),
            fragments: Vec::with_capacity(data.len()),
            hash: [0; 16],
        };
        for (t, d) in template.fragments.iter().zip(data) {
            let mut f = Fragment::new(t.name, 0, t.flags, t.compression, Vec::new());
            fill(packer, &mut f, d)?;
            res.fragments.push(f);
        }
        res.hash = res.content_hash()?;
        Ok(res)
    }

    pub fn has_stats(&self) -> bool {
        matches!(&self.fragments[..], [_, s, ..] if s.size == crate::bxml::STATS_LEN as u64)
    }

    pub fn stale_stats(&self) -> Result<bool> {
        match &self.fragments[..] {
            [data, stats, ..] if self.has_stats() => {
                Ok(crate::bxml::stale_stats(&data.data()?, &stats.data()?))
            }
            _ => Ok(false),
        }
    }

    // Rewrites stale TSMD stats from fragment 0, as the game's own tools write them;
    // true when it did. Fails when fragment 0 starts as binary XML but does not parse.
    pub fn repair_stats(&mut self) -> Result<bool> {
        self.repair_stats_with(&Packer::default())
    }

    /// [`Resource::repair_stats`], compressing the new stats with `packer`.
    pub fn repair_stats_with(&mut self, packer: &Packer) -> Result<bool> {
        if !self.stale_stats()? {
            return Ok(false);
        }
        let stats = self.fragments[1].data()?;
        let version = u32::from_le_bytes(stats[4..8].try_into().expect("4 bytes"));
        let doc = crate::bxml::Document::parse(&self.fragments[0].data()?)?;
        self.set_fragment_with(packer, 1, &doc.string_stats(version))?;
        Ok(true)
    }

    pub fn set_fragment(&mut self, index: usize, data: &[u8]) -> Result<()> {
        self.set_fragment_with(&Packer::default(), index, data)
    }

    /// Puts `data` in fragment `index`, compressed by `packer` when the fragment is zstd, and rehashes.
    pub fn set_fragment_with(&mut self, packer: &Packer, index: usize, data: &[u8]) -> Result<()> {
        let f = self
            .fragments
            .get_mut(index)
            .ok_or_else(|| Error::Format(format!("{}: no fragment {index}", self.name)))?;
        fill(packer, f, data)?;
        self.hash = self.content_hash()?;
        Ok(())
    }
}

fn fill(packer: &Packer, f: &mut Fragment, data: &[u8]) -> Result<()> {
    let zstd = f.compression == ZSTD;
    packer.put(f, data, zstd, ZSTD_LEVEL)
}

pub fn check_name(name: &str) -> Result<()> {
    if name.is_empty() || name.contains('\0') || name.len() >= u16::MAX as usize {
        return Err(Error::Format(format!(
            "{name:?} is not a resource name: it must be 1 to {} bytes without NUL",
            u16::MAX - 1
        )));
    }
    Ok(())
}

pub fn kind(name: &str) -> Result<[u8; 16]> {
    let b = name.as_bytes();
    if b.is_empty() || b.len() >= 16 || b.contains(&0) {
        return Err(Error::Format(format!(
            "{name:?} is not a resource type: it must be 1 to 15 bytes without NUL"
        )));
    }
    let mut out = [0; 16];
    out[..b.len()].copy_from_slice(b);
    Ok(out)
}

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

fn worked() {
    WORK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
const PACKED_MIN: usize = 16 << 10;

type Encoder = zstd::zstd_safe::CCtx<'static>;

/// Compression contexts kept between fragments, by level. Setting one up
/// starts a worker thread and allocates its window, which cost as much as
/// compressing a small fragment. A reused context keeps its parameters, so it
/// writes the same bytes as a new one.
static ENCODERS: std::sync::Mutex<Vec<(i32, Encoder)>> = std::sync::Mutex::new(Vec::new());

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

fn encode(data: &[u8], level: i32) -> Result<Vec<u8>> {
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

fn compress_cached(dir: &Path, data: &[u8], level: i32) -> Result<Vec<u8>> {
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

#[derive(Debug, Clone)]
pub struct Erp {
    pub header: [u8; HEADER_LEN],
    pub version: u32,
    pub resources: Vec<Resource>,
}

impl Erp {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let (mut erp, data_off) = Self::parse_index(data)?;
        for res in &mut erp.resources {
            for f in &mut res.fragments {
                let (packed, _) = f.stored()?;
                let raw = Reader::new(data, data_off.saturating_add(f.offset as usize))
                    .take(packed as usize)?
                    .to_vec();
                f.payload = Payload::Stored { packed, raw };
            }
        }
        Ok(erp)
    }

    pub fn read_matching(path: &Path, want: impl Fn(&str) -> bool) -> Result<Vec<Resource>> {
        let file = ErpFile::open(path)?;
        file.erp
            .resources
            .iter()
            .filter(|r| want(&r.name))
            .map(|r| file.load(r))
            .collect()
    }

    fn parse_index(data: &[u8]) -> Result<(Self, usize)> {
        let mut r = Reader::new(data, 0);
        let header: [u8; HEADER_LEN] = r.array()?;
        if &header[..4] != MAGIC {
            return Err(Error::Magic {
                offset: 0,
                expected: "ERPK",
            });
        }
        let version = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes"));
        let data_off = u64::from_le_bytes(header[0x20..0x28].try_into().expect("8 bytes")) as usize;
        let count = r.u32()?;
        let _fragments = r.u32()?;
        let fit = data.len().saturating_sub(r.pos) / MIN_ENTRY_LEN;
        let mut resources = Vec::with_capacity((count as usize).min(fit));
        for _ in 0..count {
            let _entry_len = r.u32()?;
            let name_len = r.u16()? as usize;
            let name = String::from_utf8_lossy(r.take(name_len)?)
                .trim_end_matches('\0')
                .to_owned();
            let kind = r.array()?;
            let unk = r.take(if version >= 4 { 6 } else { 4 })?.to_vec();
            let n = r.u8()?;
            let mut fragments = Vec::with_capacity(n as usize);
            for _ in 0..n {
                fragments.push(Fragment {
                    name: r.array()?,
                    offset: r.u64()?,
                    size: r.u64()?,
                    flags: r.u32()?,
                    compression: r.u8()?,
                    payload: Payload::Stored {
                        packed: r.u64()?,
                        raw: Vec::new(),
                    },
                });
            }
            resources.push(Resource {
                name,
                kind,
                unk,
                fragments,
                hash: r.array()?,
            });
        }
        if r.pos != data_off {
            return Err(Error::Format(format!(
                "index ends at {:#x}, data starts at {data_off:#x}",
                r.pos
            )));
        }
        Ok((
            Self {
                header,
                version,
                resources,
            },
            data_off,
        ))
    }

    pub fn find(&self, name: &str) -> Result<&Resource> {
        let mut hits = self.resources.iter().filter(|r| r.name == name);
        match (hits.next(), hits.next()) {
            (Some(r), None) => Ok(r),
            (None, _) => Err(Error::Format(format!("{name}: not found"))),
            _ => Err(Error::Format(format!("{name}: more than one resource"))),
        }
    }

    pub fn find_mut(&mut self, name: &str) -> Result<&mut Resource> {
        self.find(name)?;
        Ok(self
            .resources
            .iter_mut()
            .find(|r| r.name == name)
            .expect("found above"))
    }

    /// The archive's bytes, after compressing any fragment still pending.
    pub fn to_bytes(&mut self) -> Result<Vec<u8>> {
        Packer::default().settle(self.resources.iter_mut().flat_map(|r| r.fragments.iter_mut()))?;
        let mut out = self.layout()?;
        for res in &self.resources {
            for f in &res.fragments {
                out.extend_from_slice(f.stored()?.1);
            }
        }
        Ok(out)
    }

    fn layout(&mut self) -> Result<Vec<u8>> {
        let mut index = Vec::new();
        let mut fragments = 0u32;
        let mut offset = 0u64;
        for res in &mut self.resources {
            let mut e = Vec::new();
            let name_len = res.name.len() + 1;
            e.extend_from_slice(&(name_len as u16).to_le_bytes());
            e.extend_from_slice(res.name.as_bytes());
            e.push(0);
            e.extend_from_slice(&res.kind);
            e.extend_from_slice(&res.unk);
            e.push(res.fragments.len() as u8);
            for f in &mut res.fragments {
                let (packed, _) = f.stored()?;
                f.offset = offset;
                offset += packed;
                e.extend_from_slice(&f.name);
                e.extend_from_slice(&f.offset.to_le_bytes());
                e.extend_from_slice(&f.size.to_le_bytes());
                e.extend_from_slice(&f.flags.to_le_bytes());
                e.push(f.compression);
                e.extend_from_slice(&packed.to_le_bytes());
                fragments += 1;
            }
            e.extend_from_slice(&res.hash);
            index.extend_from_slice(&(e.len() as u32).to_le_bytes());
            index.extend_from_slice(&e);
        }
        let info_len = 8 + index.len() as u64;
        let mut header = self.header;
        header[0x10..0x18].copy_from_slice(&(HEADER_LEN as u64).to_le_bytes());
        header[0x18..0x20].copy_from_slice(&info_len.to_le_bytes());
        header[0x20..0x28].copy_from_slice(&(HEADER_LEN as u64 + info_len).to_le_bytes());
        let mut out = Vec::with_capacity(HEADER_LEN + info_len as usize + offset as usize);
        out.extend_from_slice(&header);
        out.extend_from_slice(&(self.resources.len() as u32).to_le_bytes());
        out.extend_from_slice(&fragments.to_le_bytes());
        out.extend_from_slice(&index);
        Ok(out)
    }
}

pub enum Entry<'a> {
    Copy(&'a Resource),
    Owned(Resource),
}

#[derive(Debug, Clone)]
pub struct ErpFile {
    pub path: std::path::PathBuf,
    pub erp: Erp,
    pub data_off: u64,
}

impl ErpFile {
    pub fn open(path: &Path) -> Result<Self> {
        let mut file = open(path)?;
        let mut index = vec![0; HEADER_LEN];
        file.read_exact(&mut index).map_err(crate::io_at(path))?;
        if &index[..4] != MAGIC {
            return Err(Error::Magic {
                offset: 0,
                expected: "ERPK",
            });
        }
        let data_off = u64::from_le_bytes(index[0x20..0x28].try_into().expect("8 bytes"));
        if data_off < HEADER_LEN as u64 {
            return Err(Error::Format(format!(
                "{}: data starts at {data_off:#x}, inside the {HEADER_LEN:#x}-byte header",
                path.display()
            )));
        }
        let len = file.metadata().map_err(crate::io_at(path))?.len();
        if data_off > len {
            return Err(crate::io_at(path)(std::io::ErrorKind::UnexpectedEof.into()));
        }
        index.resize(data_off as usize, 0);
        file.read_exact(&mut index[HEADER_LEN..])
            .map_err(crate::io_at(path))?;
        Ok(Self {
            path: path.to_path_buf(),
            erp: Erp::parse_index(&index)?.0,
            data_off,
        })
    }

    pub fn load(&self, res: &Resource) -> Result<Resource> {
        let mut file = open(&self.path)?;
        let len = file.metadata().map_err(crate::io_at(&self.path))?.len();
        let mut out = res.clone();
        for f in &mut out.fragments {
            let (packed, _) = f.stored()?;
            let end = self
                .data_off
                .checked_add(f.offset)
                .and_then(|at| at.checked_add(packed));
            if end.is_none_or(|end| end > len) {
                return Err(crate::io_at(&self.path)(std::io::ErrorKind::UnexpectedEof.into()));
            }
            file.seek(SeekFrom::Start(self.data_off + f.offset))
                .map_err(crate::io_at(&self.path))?;
            let mut raw = vec![0; packed as usize];
            file.read_exact(&mut raw).map_err(crate::io_at(&self.path))?;
            f.payload = Payload::Stored { packed, raw };
        }
        Ok(out)
    }

    /// Writes `entries` to `dst`, copying stock resources from this archive and compressing any owned
    /// fragment still pending first.
    pub fn write(&self, entries: Vec<Entry<'_>>, dst: &Path) -> Result<()> {
        let mut spans = Vec::new();
        let mut resources = Vec::with_capacity(entries.len());
        for e in entries {
            match e {
                Entry::Copy(r) => {
                    let mut r = r.clone();
                    for f in &mut r.fragments {
                        let (packed, _) = f.stored()?;
                        spans.push(Some((self.data_off + f.offset, packed)));
                        f.payload = Payload::Stored {
                            packed,
                            raw: Vec::new(),
                        };
                    }
                    resources.push(r);
                }
                Entry::Owned(r) => {
                    spans.extend(r.fragments.iter().map(|_| None));
                    resources.push(r);
                }
            }
        }
        Packer::default().settle(resources.iter_mut().flat_map(|r| r.fragments.iter_mut()))?;
        let mut erp = Erp {
            header: self.erp.header,
            version: self.erp.version,
            resources,
        };
        let head = erp.layout()?;
        let tmp = dst.with_extension("erp.tmp");
        let mut src = open(&self.path)?;
        let mut out = std::io::BufWriter::with_capacity(
            1 << 20,
            retry(|| std::fs::File::create(&tmp)).map_err(crate::io_at(&tmp))?,
        );
        std::io::Write::write_all(&mut out, &head)?;
        let mut buf = vec![0; 1 << 20];
        let mut run: Option<(u64, u64)> = None;
        let frags = erp.resources.iter().flat_map(|r| &r.fragments);
        for (f, span) in frags.zip(spans) {
            match (span, run.as_mut()) {
                (Some((at, len)), Some((start, run_len))) if *start + *run_len == at => *run_len += len,
                (Some(span), _) => {
                    if let Some(done) = run.replace(span) {
                        self.copy_span(&mut src, done, &mut buf, &mut out)?;
                    }
                }
                (None, _) => {
                    if let Some(done) = run.take() {
                        self.copy_span(&mut src, done, &mut buf, &mut out)?;
                    }
                    std::io::Write::write_all(&mut out, f.stored()?.1)?;
                }
            }
        }
        if let Some(done) = run {
            self.copy_span(&mut src, done, &mut buf, &mut out)?;
        }
        std::io::Write::flush(&mut out)?;
        drop(out);
        drop(src);
        if let Err(e) = replace(&tmp, dst) {
            std::fs::remove_file(&tmp).ok();
            return Err(crate::io_at(dst)(e));
        }
        worked();
        Ok(())
    }

    /// Copies `len` stock bytes at `at` of this archive to `out`. Adjacent stock fragments arrive as one span,
    /// so a run of untouched resources costs one seek and reads as large as `buf`.
    fn copy_span(
        &self,
        src: &mut std::fs::File,
        (at, mut left): (u64, u64),
        buf: &mut [u8],
        out: &mut impl std::io::Write,
    ) -> Result<()> {
        src.seek(SeekFrom::Start(at)).map_err(crate::io_at(&self.path))?;
        while left > 0 {
            let n = left.min(buf.len() as u64) as usize;
            src.read_exact(&mut buf[..n]).map_err(crate::io_at(&self.path))?;
            out.write_all(&buf[..n])?;
            left -= n as u64;
        }
        Ok(())
    }
}

fn transient(e: &std::io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    cfg!(windows)
        && matches!(
            e.raw_os_error(),
            Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION)
        )
}

fn retrying<T>(
    transient: &dyn Fn(&std::io::Error) -> bool,
    wait: &mut dyn FnMut(Duration),
    mut f: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut waits = BACKOFF_MS.iter();
    loop {
        match f() {
            Err(e) if transient(&e) => match waits.next() {
                Some(ms) => wait(Duration::from_millis(*ms)),
                None => return Err(e),
            },
            done => return done,
        }
    }
}

fn retry<T>(f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    retrying(&transient, &mut std::thread::sleep, f)
}

fn open(path: &Path) -> Result<std::fs::File> {
    retry(|| std::fs::File::open(path)).map_err(crate::io_at(path))
}

/// Moves `tmp` over `dst`, clearing a read-only `dst` first, and tries again
/// for a moment while Windows refuses it, as it does while an antivirus scans
/// either file. Nothing of ours may hold `dst` open.
pub fn replace(tmp: &Path, dst: &Path) -> std::io::Result<()> {
    retry(|| {
        writable(dst);
        std::fs::rename(tmp, dst)
    })
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn writable(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path)
        && meta.permissions().readonly()
    {
        let mut perms = meta.permissions();
        perms.set_readonly(false);
        std::fs::set_permissions(path, perms).ok();
    }
}

#[cfg(not(windows))]
fn writable(_: &Path) {}

#[cfg(test)]
mod tests;
