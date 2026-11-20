//! BC7 encoder for HUD and logo art: modes 5 and 6 only, alpha weighted, with edge colour bleed.
//!
//! A line for line port of `native/src/core/apply/content/bc7.hpp`, which in turn equals the
//! offline Python pipeline byte for byte. The sums add in the same order as the C++ (one at a
//! time, or eight lanes for sixteen values) and Rust never fuses multiply-adds, so the output
//! is bit identical. `tests/bc7_golden.rs` checks it against bytes the C++ produced.

#![allow(clippy::needless_range_loop)]

mod arith;
mod mode5;
mod mode6;

use crate::{Error, Result};

use self::arith::pairwise16;
use self::mode5::{mode5_candidate, pack5};
use self::mode6::{colour_block, decode6, flat_block};

type Block = [[i64; 4]; 16];

const W4: [i64; 16] = [0, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64];
const W2: [i64; 4] = [0, 21, 43, 64];

fn bleed(px: &mut Block) {
    let mut tot = 0i64;
    let mut mean = [0i64; 3];
    for p in px.iter() {
        tot += p[3];
        for c in 0..3 {
            mean[c] += p[c] * p[3];
        }
    }
    if tot <= 0 {
        return;
    }
    for m in &mut mean {
        *m = (*m + tot / 2) / tot;
    }
    for p in px.iter_mut() {
        if p[3] == 0 {
            p[..3].copy_from_slice(&mean);
        }
    }
}

fn gather(rgba: &[u8], w: usize, bx: usize, by: usize) -> Block {
    let mut px = [[0i64; 4]; 16];
    for y in 0..4 {
        for x in 0..4 {
            for c in 0..4 {
                px[y * 4 + x][c] = rgba[((by * 4 + y) * w + bx * 4 + x) * 4 + c] as i64;
            }
        }
    }
    px
}

fn block_error(px: &Block, dec: &Block) -> f64 {
    let mut per = [0.0f64; 16];
    for t in 0..16 {
        let mut rgb = 0.0f64;
        for c in 0..3 {
            let d = (px[t][c] - dec[t][c]) as f64;
            rgb += d * d;
        }
        let da = (px[t][3] - dec[t][3]) as f64;
        per[t] = rgb * px[t][3] as f64 / 255.0 + 4.0 * (da * da);
    }
    pairwise16(&per)
}

fn check_size(rgba: &[u8], w: usize, h: usize) -> Result<()> {
    if w == 0 || h == 0 || !w.is_multiple_of(4) || !h.is_multiple_of(4) {
        return Err(Error::Format(format!(
            "BC7 needs a size that is a multiple of 4, not {w}x{h}"
        )));
    }
    if rgba.len() != w * h * 4 {
        return Err(Error::Format(format!(
            "{w}x{h} RGBA is {} bytes, not {}",
            w * h * 4,
            rgba.len()
        )));
    }
    Ok(())
}

/// Encodes straight RGBA8 (`w * h * 4` bytes, row major) to BC7 mode 6 blocks, 16 bytes per 4x4
/// block, rows of blocks top to bottom. `w` and `h` must be multiples of 4; pad first otherwise.
/// Transparent texels take the opaque texels' mean colour before encoding, so edges do not
/// fringe under bilinear sampling. Equals `bc7::encode_rgba` of the native core.
pub fn encode_rgba(rgba: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    check_size(rgba, w, h)?;
    let mut out = Vec::with_capacity(w / 4 * (h / 4) * 16);
    for by in 0..h / 4 {
        for bx in 0..w / 4 {
            let mut bled = gather(rgba, w, bx, by);
            bleed(&mut bled);
            let flat = (1..16).all(|t| bled[t][..3] == bled[0][..3]);
            let word = if flat {
                flat_block(&bled)
            } else {
                colour_block(&bled)
            };
            out.extend_from_slice(&word);
        }
    }
    Ok(out)
}

/// [`encode_rgba`] for art with soft alpha: colour under fully transparent texels is zeroed first,
/// then each block switches to mode 5 (separate alpha) where that has the smaller alpha weighted
/// error. Equals `bc7::encode_logo` of the native core.
pub fn encode_logo(rgba: &[u8], w: usize, h: usize) -> Result<Vec<u8>> {
    check_size(rgba, w, h)?;
    let mut clean = rgba.to_vec();
    for p in clean.chunks_exact_mut(4) {
        if p[3] == 0 {
            p[..3].fill(0);
        }
    }
    let mut out = encode_rgba(&clean, w, h)?;
    for by in 0..h / 4 {
        for bx in 0..w / 4 {
            let px = gather(&clean, w, bx, by);
            let at = (by * (w / 4) + bx) * 16;
            let word: [u8; 16] = out[at..at + 16].try_into().expect("16 bytes");
            let stay = block_error(&px, &decode6(&word));
            let mut src = [[0.0f64; 3]; 16];
            let mut wt = [0.0f64; 16];
            for t in 0..16 {
                for c in 0..3 {
                    src[t][c] = px[t][c] as f64;
                }
                wt[t] = px[t][3] as f64 / 255.0;
            }
            let (err, m) = mode5_candidate(&px, &src, &wt);
            if err < stay {
                out[at..at + 16].copy_from_slice(&pack5(m));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
