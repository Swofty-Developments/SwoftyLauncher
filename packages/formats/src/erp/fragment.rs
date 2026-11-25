use crate::erp::{STORED, ZSTD, ZSTD_MAGIC};
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct Fragment {
    pub name: [u8; 4],
    pub offset: u64,
    pub size: u64,
    pub flags: u32,
    pub compression: u8,
    pub(super) payload: Payload,
    /// The archive and resource the stored bytes were read from, which a decode error names.
    pub(super) from: Option<std::sync::Arc<ReadFrom>>,
}

/// The archive file, when there is one, and the resource a fragment's stored bytes were read from.
#[derive(Debug)]
pub(super) struct ReadFrom {
    pub(super) path: Option<std::path::PathBuf>,
    pub(super) resource: String,
}

/// A fragment's bytes: as its archive stores them, or its data waiting for a [`Packer::settle`] that
/// compresses it. An archive is laid out from stored bytes only, so a pending fragment is compressed
/// before any write.
#[derive(Debug, Clone)]
pub(super) enum Payload {
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
            from: None,
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

    pub(super) fn stored(&self) -> Result<(u64, &[u8])> {
        match &self.payload {
            Payload::Stored { packed, raw } => Ok((*packed, raw)),
            Payload::Pending { .. } => Err(Error::Format(format!(
                "{}: a fragment's compression is still pending",
                self.name()
            ))),
        }
    }

    /// Whether the game's decompressor takes the fragment for zstd by its compression code: 3 to 0x3f, of which
    /// 0x20 to 0x2f name a dictionary this crate does not hold.
    pub fn zstd_coded(&self) -> bool {
        matches!(self.compression, 3..=0x1f | 0x30..=0x3f)
    }

    /// Decodes the stored bytes of a zstd-coded fragment through to the end of their frame, keeping no
    /// output, and fails unless they make exactly the declared size: the bytes the game's decompressor would
    /// otherwise spin on. `archive` and `resource` name where the fragment is written.
    pub(crate) fn check_frame(&self, archive: &std::path::Path, resource: &str) -> Result<()> {
        let Payload::Stored { raw, .. } = &self.payload else {
            return Ok(());
        };
        if !self.zstd_coded() {
            return Ok(());
        }
        let wrong = |reason: String| Error::Frame {
            path: Some(archive.to_path_buf()),
            resource: Some(resource.to_owned()),
            fragment: self.name(),
            packed: raw.len(),
            reason,
        };
        let made = frame_len(raw).map_err(|e| wrong(e.to_string()))?;
        match made == self.size {
            true => Ok(()),
            false => Err(wrong(format!(
                "it decodes to {made} byte(s), not the {} the index declares",
                self.size
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
            Payload::Stored { raw, .. } if self.is_zstd() => zstd::decode_all(raw.as_slice())
                .map(Into::into)
                .map_err(|e| Error::Frame {
                    path: self.from.as_ref().and_then(|f| f.path.clone()),
                    resource: self.from.as_ref().map(|f| f.resource.clone()),
                    fragment: self.name(),
                    packed: raw.len(),
                    reason: e.to_string(),
                }),
            Payload::Stored { raw, .. } => Ok(std::borrow::Cow::Borrowed(raw)),
        }
    }
}

/// How many bytes `raw` decodes to, read through the end of its frame, without keeping them.
pub(super) fn frame_len(raw: &[u8]) -> std::io::Result<u64> {
    use std::io::Read;
    let mut decoder = zstd::stream::read::Decoder::new(raw)?;
    let mut buf = vec![0u8; 1 << 16];
    let mut total = 0u64;
    loop {
        match decoder.read(&mut buf)? {
            0 => return Ok(total),
            n => total += n as u64,
        }
    }
}
