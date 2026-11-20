use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use crate::erp::fragment::Payload;
use crate::erp::packer::{Packer, worked};
use crate::erp::resource::Resource;
use crate::erp::{BACKOFF_MS, Erp, HEADER_LEN, MAGIC};
use crate::{Error, Result};

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

pub(super) fn transient(e: &std::io::Error) -> bool {
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    cfg!(windows)
        && matches!(
            e.raw_os_error(),
            Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION)
        )
}

pub(super) fn retrying<T>(
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

pub(super) fn open(path: &Path) -> Result<std::fs::File> {
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
