use crate::bc7_enc::arith::{Bits, cmax, cmin, normalise, pairwise16, rint_to};
use crate::bc7_enc::{Block, W2, block_error};

#[derive(Default, Clone)]
pub(super) struct Mode5 {
    c0: [i64; 3],
    c1: [i64; 3],
    a0: i64,
    a1: i64,
    cidx: [usize; 16],
    aidx: [usize; 16],
}

pub(super) fn mode5_candidate(px: &Block, src: &[[f64; 3]; 16], wt: &[f64; 16]) -> (f64, Mode5) {
    let wsum = pairwise16(wt);
    let mut mean = [0.0f64; 3];
    for c in 0..3 {
        let mut s = 0.0f64;
        for t in 0..16 {
            s += src[t][c] * wt[t];
        }
        mean[c] = s / cmax(wsum, 1e-9);
    }
    let mut cov = [[0.0f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            let mut s = 0.0f64;
            for t in 0..16 {
                s += ((src[t][i] - mean[i]) * wt[t].sqrt()) * ((src[t][j] - mean[j]) * wt[t].sqrt());
            }
            cov[i][j] = s;
        }
    }
    let mut axis = [1.0f64; 3];
    for _ in 0..8 {
        let mut nx = [0.0f64; 3];
        for i in 0..3 {
            let mut s = 0.0f64;
            for j in 0..3 {
                s += cov[i][j] * axis[j];
            }
            nx[i] = s;
        }
        normalise(&mut nx);
        axis = nx;
    }
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in 0..16 {
        let mut s = 0.0f64;
        for c in 0..3 {
            s += (src[t][c] - mean[c]) * axis[c];
        }
        if wt[t] > 0.0 {
            lo = cmin(lo, s);
            hi = cmax(hi, s);
        }
    }
    if !lo.is_finite() {
        lo = 0.0;
    }
    if !hi.is_finite() {
        hi = 0.0;
    }
    let mut ends = [[0.0f64; 3]; 2];
    for c in 0..3 {
        ends[0][c] = mean[c] + lo * axis[c];
        ends[1][c] = mean[c] + hi * axis[c];
    }
    let mut have = false;
    let mut best_err = 0.0f64;
    let mut bq = [[0i64; 3]; 2];
    let mut bidx = [0usize; 16];
    for _ in 0..3 {
        let mut q = [[0i64; 3]; 2];
        let mut v = [[0i64; 3]; 2];
        for e in 0..2 {
            for c in 0..3 {
                q[e][c] = rint_to(ends[e][c] / 255.0 * 127.0, 0, 127);
                v[e][c] = (q[e][c] << 1) | (q[e][c] >> 6);
            }
        }
        let mut pal = [[0i64; 3]; 4];
        for i in 0..4 {
            for c in 0..3 {
                pal[i][c] = ((64 - W2[i]) * v[0][c] + W2[i] * v[1][c] + 32) >> 6;
            }
        }
        let mut idx = [0usize; 16];
        let mut per = [0.0f64; 16];
        for t in 0..16 {
            let mut best = -1i64;
            let mut pick = 0;
            for (i, p) in pal.iter().enumerate() {
                let mut e = 0i64;
                for c in 0..3 {
                    let d = p[c] - px[t][c];
                    e += d * d;
                }
                if best < 0 || e < best {
                    best = e;
                    pick = i;
                }
            }
            idx[t] = pick;
            per[t] = best as f64 * wt[t];
        }
        let err = pairwise16(&per);
        if !have || err < best_err {
            have = true;
            best_err = err;
            bq = q;
            bidx = idx;
        }
        let mut f = [0.0f64; 16];
        let mut g = [0.0f64; 16];
        for t in 0..16 {
            f[t] = W2[bidx[t]] as f64 / 64.0;
            g[t] = 1.0 - f[t];
        }
        let (mut pa, mut pb, mut pc) = ([0.0f64; 16], [0.0f64; 16], [0.0f64; 16]);
        for t in 0..16 {
            pa[t] = (wt[t] * g[t]) * g[t];
            pb[t] = (wt[t] * g[t]) * f[t];
            pc[t] = (wt[t] * f[t]) * f[t];
        }
        let (a, b, cc) = (pairwise16(&pa), pairwise16(&pb), pairwise16(&pc));
        let det = a * cc - b * b;
        let ok = det.abs() > 1e-9;
        let den = cmax(a + 2.0 * b + cc, 1e-9);
        for c in 0..3 {
            let (mut x0, mut x1) = (0.0f64, 0.0f64);
            for t in 0..16 {
                x0 += (wt[t] * g[t]) * src[t][c];
                x1 += (wt[t] * f[t]) * src[t][c];
            }
            let flat = (x0 + x1) / den;
            ends[0][c] = if ok { (cc * x0 - b * x1) / det } else { flat };
            ends[1][c] = if ok { (a * x1 - b * x0) / det } else { flat };
        }
    }
    let mut out = Mode5 {
        c0: bq[0],
        c1: bq[1],
        cidx: bidx,
        ..Mode5::default()
    };
    let (mut a0, mut a1) = (px[0][3], px[0][3]);
    for p in &px[1..] {
        a0 = a0.min(p[3]);
        a1 = a1.max(p[3]);
    }
    out.a0 = a0;
    out.a1 = a1;
    let mut apal = [0i64; 4];
    for i in 0..4 {
        apal[i] = ((64 - W2[i]) * a0 + W2[i] * a1 + 32) >> 6;
    }
    let mut cpal = [[0i64; 3]; 4];
    for i in 0..4 {
        for c in 0..3 {
            let e0 = (out.c0[c] << 1) | (out.c0[c] >> 6);
            let e1 = (out.c1[c] << 1) | (out.c1[c] >> 6);
            cpal[i][c] = ((64 - W2[i]) * e0 + W2[i] * e1 + 32) >> 6;
        }
    }
    let mut dec = [[0i64; 4]; 16];
    for t in 0..16 {
        let mut best = -1i64;
        let mut pick = 0;
        for (i, a) in apal.iter().enumerate() {
            let e = (a - px[t][3]).abs();
            if best < 0 || e < best {
                best = e;
                pick = i;
            }
        }
        out.aidx[t] = pick;
        for c in 0..3 {
            dec[t][c] = cpal[out.cidx[t]][c];
        }
        dec[t][3] = apal[pick];
    }
    (block_error(px, &dec), out)
}

pub(super) fn pack5(mut m: Mode5) -> [u8; 16] {
    if m.cidx[0] >= 2 {
        std::mem::swap(&mut m.c0, &mut m.c1);
        for i in &mut m.cidx {
            *i = 3 - *i;
        }
    }
    if m.aidx[0] >= 2 {
        std::mem::swap(&mut m.a0, &mut m.a1);
        for i in &mut m.aidx {
            *i = 3 - *i;
        }
    }
    let mut b = Bits::default();
    b.put(1 << 5, 6);
    b.put(0, 2);
    for c in 0..3 {
        b.put(m.c0[c] as u64, 7);
        b.put(m.c1[c] as u64, 7);
    }
    b.put(m.a0 as u64, 8);
    b.put(m.a1 as u64, 8);
    b.put(m.cidx[0] as u64, 1);
    for &i in &m.cidx[1..] {
        b.put(i as u64, 2);
    }
    b.put(m.aidx[0] as u64, 1);
    for &i in &m.aidx[1..] {
        b.put(i as u64, 2);
    }
    b.word
}
