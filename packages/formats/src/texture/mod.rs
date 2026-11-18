mod side;

use std::path::Path;

use crate::erp::Resource;
use crate::{Error, Result};

pub use side::{SideFile, SideLevel, side_file};

const BC7: u32 = 0x46;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Bc1,
    Bc3,
    Bc5,
    Bc7,
}

impl Codec {
    pub fn of(format: u32) -> Option<Codec> {
        match format {
            52 | 54 => Some(Codec::Bc1),
            55 | 57 => Some(Codec::Bc3),
            65 => Some(Codec::Bc5),
            69 | BC7 => Some(Codec::Bc7),
            _ => None,
        }
    }

    pub fn block_bytes(self) -> usize {
        match self {
            Codec::Bc1 => 8,
            Codec::Bc3 | Codec::Bc5 | Codec::Bc7 => 16,
        }
    }

    pub fn level_bytes(self, width: u32, height: u32) -> usize {
        width.div_ceil(4) as usize * height.div_ceil(4) as usize * self.block_bytes()
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub layers: u32,
}

impl Header {
    pub fn codec(&self) -> Option<Codec> {
        Codec::of(self.format)
    }
}

pub fn header(surface: &Resource) -> Result<Header> {
    let head = surface
        .fragments
        .first()
        .ok_or_else(|| Error::Format(format!("{}: not a surface", surface.name)))?
        .data()?;
    let word = |at: usize| {
        head.get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes")))
    };
    let (Some(format), Some(width), Some(height)) = (word(8), word(12), word(16)) else {
        return Err(Error::Format(format!("{}: short surface header", surface.name)));
    };
    Ok(Header {
        format,
        width,
        height,
        levels: word(24).unwrap_or(1).clamp(1, 16),
        layers: word(28).unwrap_or(1).max(1),
    })
}

pub fn decode_pixels(codec: Codec, width: u32, height: u32, data: &[u8]) -> Result<Vec<u8>> {
    let (bw, bh) = (width.div_ceil(4) as usize, height.div_ceil(4) as usize);
    let bytes = codec.block_bytes();
    if data.len() < bw * bh * bytes {
        return Err(Error::Format(format!(
            "{width}x{height} {codec:?} needs {} bytes, got {}",
            bw * bh * bytes,
            data.len()
        )));
    }
    let pitch = bw * 16;
    let mut padded = vec![0; pitch * bh * 4];
    let mut rg = [0u8; 32];
    for (i, block) in data.chunks_exact(bytes).take(bw * bh).enumerate() {
        let dst = &mut padded[i / bw * 4 * pitch + i % bw * 16..];
        match codec {
            Codec::Bc1 => bcdec_rs::bc1(block, dst, pitch),
            Codec::Bc3 => bcdec_rs::bc3(block, dst, pitch),
            Codec::Bc7 => bcdec_rs::bc7(block, dst, pitch),
            Codec::Bc5 => {
                bcdec_rs::bc5(block, &mut rg, 8, false);
                for y in 0..4 {
                    for x in 0..4 {
                        let s = y * 8 + x * 2;
                        dst[y * pitch + x * 4..][..4].copy_from_slice(&[rg[s], rg[s + 1], 0, 255]);
                    }
                }
            }
        }
    }
    let row = width as usize * 4;
    if row != pitch {
        for y in 1..height as usize {
            padded.copy_within(y * pitch..y * pitch + row, y * row);
        }
    }
    padded.truncate(row * height as usize);
    Ok(padded)
}

fn unsupported(surface: &Resource, h: &Header) -> Error {
    Error::Format(format!(
        "{}: unsupported surface format {:#x}",
        surface.name, h.format
    ))
}

fn pixels(surface: &Resource) -> Result<std::borrow::Cow<'_, [u8]>> {
    surface
        .fragments
        .get(1)
        .ok_or_else(|| Error::Format(format!("{}: not a surface", surface.name)))?
        .data_cow()
}

pub fn decode(surface: &Resource) -> Result<Image> {
    let h = header(surface)?;
    let codec = h.codec().ok_or_else(|| unsupported(surface, &h))?;
    let rgba = decode_pixels(codec, h.width, h.height, &pixels(surface)?)
        .map_err(|e| Error::Format(format!("{}: {e}", surface.name)))?;
    Ok(Image {
        width: h.width,
        height: h.height,
        rgba,
    })
}

pub fn decode_near(surface: &Resource, root: Option<&Path>, want: u32) -> Result<Image> {
    let h = header(surface)?;
    let codec = h.codec().ok_or_else(|| unsupported(surface, &h))?;
    let side = match root {
        Some(_) => side_file(surface)?,
        None => None,
    };
    let mut sizes: Vec<(u32, u32, Option<usize>)> = Vec::new();
    if let Some(side) = &side {
        let n = side.levels.len().min(8) as u32;
        for k in 0..n {
            let shift = n - k;
            sizes.push((h.width << shift, h.height << shift, Some(k as usize)));
        }
    }
    let (mut w, mut hh) = (h.width, h.height);
    for _ in 0..h.levels {
        sizes.push((w, hh, None));
        w = (w / 2).max(1);
        hh = (hh / 2).max(1);
    }
    let (width, height, level) = sizes
        .iter()
        .rev()
        .find(|(w, h, _)| (*w).max(*h) >= want)
        .or_else(|| sizes.first())
        .copied()
        .ok_or_else(|| Error::Format(format!("{}: no levels", surface.name)))?;
    let data: std::borrow::Cow<'_, [u8]>;
    let selected = match (level, &side, root) {
        (Some(k), Some(side), Some(root)) => {
            data = std::borrow::Cow::Owned(side.read(root, k)?);
            data.as_ref()
        }
        _ => {
            data = pixels(surface)?;
            let (mut w, mut hh, mut at) = (h.width, h.height, 0);
            while (w, hh) != (width, height) {
                at += codec.level_bytes(w, hh);
                w = (w / 2).max(1);
                hh = (hh / 2).max(1);
            }
            data.get(at..).ok_or(Error::Eof(at))?
        }
    };
    let rgba = decode_pixels(codec, width, height, selected)
        .map_err(|e| Error::Format(format!("{}: {e}", surface.name)))?;
    Ok(Image { width, height, rgba })
}

#[cfg(test)]
mod tests;
