use crate::bc7_enc::arith::{Bits, cmax, cmin, interp, normalise, pairwise16, rint_to};
use crate::bc7_enc::{Block, W4};

struct Endpoint {
    q: [i64; 4],
    v: [i64; 4],
    p: i64,
}

fn endpoint(c: &[i64; 4]) -> Endpoint {
    let mut best_err = -1i64;
    let mut out = Endpoint {
        q: [0; 4],
        v: [0; 4],
        p: 0,
    };
    for pb in 0..2i64 {
        let (mut qq, mut vv, mut err) = ([0i64; 4], [0i64; 4], 0i64);
        for k in 0..4 {
            qq[k] = ((c[k] - pb + 1) >> 1).clamp(0, 127);
            vv[k] = (qq[k] << 1) | pb;
            err += (vv[k] - c[k]) * (vv[k] - c[k]);
        }
        if best_err < 0 || err < best_err {
            best_err = err;
            out = Endpoint { q: qq, v: vv, p: pb };
        }
    }
    out
}

fn pack6(e0: &[i64; 4], e1: &[i64; 4], p0: i64, p1: i64, ix: &[i64; 16]) -> [u8; 16] {
    let mut b = Bits::default();
    b.put(1 << 6, 7);
    for c in 0..4 {
        b.put(e0[c] as u64, 7);
        b.put(e1[c] as u64, 7);
    }
    b.put(p0 as u64, 1);
    b.put(p1 as u64, 1);
    b.put(ix[0] as u64, 3);
    for &i in &ix[1..] {
        b.put(i as u64, 4);
    }
    b.word
}

pub(super) fn flat_block(px: &Block) -> [u8; 16] {
    let (mut lo, mut hi) = (0, 0);
    for i in 1..16 {
        if px[i][3] < px[lo][3] {
            lo = i;
        }
        if px[i][3] > px[hi][3] {
            hi = i;
        }
    }
    let a = endpoint(&px[lo]);
    let b = endpoint(&px[hi]);
    let mut pal = [[0i64; 4]; 16];
    for i in 0..16 {
        for c in 0..4 {
            pal[i][c] = interp(a.v[c], b.v[c], W4[i]);
        }
    }
    let mut ix = [0i64; 16];
    for t in 0..16 {
        let mut best = -1i64;
        let mut pick = 0;
        for (i, p) in pal.iter().enumerate() {
            let mut e = 0i64;
            for c in 0..4 {
                let d = p[c] - px[t][c];
                e += d * d * if c == 3 { 4 } else { 1 };
            }
            if best < 0 || e < best {
                best = e;
                pick = i as i64;
            }
        }
        ix[t] = pick;
    }
    let (mut q0, mut q1, mut p0, mut p1) = (a.q, b.q, a.p, b.p);
    if ix[0] >= 8 {
        std::mem::swap(&mut q0, &mut q1);
        std::mem::swap(&mut p0, &mut p1);
        for i in &mut ix {
            *i = 15 - *i;
        }
    }
    pack6(&q0, &q1, p0, p1, &ix)
}

#[derive(Clone, Copy, Default)]
struct Quant {
    q: [i64; 4],
    v: [i64; 4],
    p: i64,
}

fn quantise(ep: &[f64; 4]) -> Quant {
    let mut c = [0i64; 4];
    for k in 0..4 {
        c[k] = rint_to(ep[k], 0, 255);
    }
    let mut best = Quant::default();
    let mut best_err = 0.0f64;
    for pb in 0..2i64 {
        let mut cur = Quant {
            p: pb,
            ..Quant::default()
        };
        let mut err = 0.0f64;
        for k in 0..4 {
            cur.q[k] = ((c[k] - pb + 1) >> 1).clamp(0, 127);
            cur.v[k] = (cur.q[k] << 1) | pb;
            let d = cur.v[k] as f64 - ep[k];
            err += d * d;
        }
        if pb == 0 || err < best_err {
            best = cur;
            best_err = err;
        }
    }
    best
}

fn assign(a: &Quant, b: &Quant, px: &Block, wt: &[[f64; 4]; 16], idx: &mut [usize; 16]) -> f64 {
    let mut pal = [[0i64; 4]; 16];
    for i in 0..16 {
        for c in 0..4 {
            pal[i][c] = ((64 - W4[i]) * a.v[c] + W4[i] * b.v[c] + 32) >> 6;
        }
    }
    let mut per = [0.0f64; 16];
    for t in 0..16 {
        let mut best = 0.0f64;
        let mut pick = 0;
        for (i, p) in pal.iter().enumerate() {
            let mut e = 0.0f64;
            for c in 0..4 {
                let d = (p[c] - px[t][c]) as f64;
                e += (d * d) * wt[t][c];
            }
            if i == 0 || e < best {
                best = e;
                pick = i;
            }
        }
        idx[t] = pick;
        per[t] = best;
    }
    pairwise16(&per)
}

fn fit(src: &[[f64; 4]; 16], wt: &[[f64; 4]; 16], idx: &[usize; 16]) -> ([f64; 4], [f64; 4]) {
    let mut f = [0.0f64; 16];
    let mut g = [0.0f64; 16];
    for t in 0..16 {
        f[t] = W4[idx[t]] as f64 / 64.0;
        g[t] = 1.0 - f[t];
    }
    let (mut e0, mut e1) = ([0.0f64; 4], [0.0f64; 4]);
    for c in 0..4 {
        let (mut a, mut b, mut cc, mut x0, mut x1) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for t in 0..16 {
            a += wt[t][c] * (g[t] * g[t]);
            b += wt[t][c] * (g[t] * f[t]);
            cc += wt[t][c] * (f[t] * f[t]);
            x0 += (wt[t][c] * g[t]) * src[t][c];
            x1 += (wt[t][c] * f[t]) * src[t][c];
        }
        let det = a * cc - b * b;
        let ok = det.abs() > 1e-9;
        let flat = (x0 + x1) / cmax(a + 2.0 * b + cc, 1e-9);
        let r0 = if ok { (cc * x0 - b * x1) / det } else { flat };
        let r1 = if ok { (a * x1 - b * x0) / det } else { flat };
        e0[c] = cmin(255.0, cmax(0.0, r0));
        e1[c] = cmin(255.0, cmax(0.0, r1));
    }
    (e0, e1)
}

struct Cand {
    a: Quant,
    b: Quant,
    idx: [usize; 16],
    err: f64,
}

fn candidate(px: &Block, src: &[[f64; 4]; 16], wt: &[[f64; 4]; 16], s0: &[f64; 4], s1: &[f64; 4]) -> Cand {
    let mut cur = Cand {
        a: quantise(s0),
        b: quantise(s1),
        idx: [0; 16],
        err: 0.0,
    };
    cur.err = assign(&cur.a, &cur.b, px, wt, &mut cur.idx);
    for _ in 0..4 {
        let (f0, f1) = fit(src, wt, &cur.idx);
        let (r0, r1) = (quantise(&f0), quantise(&f1));
        let mut ridx = [0usize; 16];
        let rerr = assign(&r0, &r1, px, wt, &mut ridx);
        if rerr < cur.err {
            cur.a = r0;
            cur.b = r1;
            cur.err = rerr;
            cur.idx = ridx;
        }
    }
    cur
}

pub(super) fn colour_block(px: &Block) -> [u8; 16] {
    let mut src = [[0.0f64; 4]; 16];
    let mut wt = [[0.0f64; 4]; 16];
    let mut sw = [[0.0f64; 4]; 16];
    for t in 0..16 {
        for c in 0..4 {
            src[t][c] = px[t][c] as f64;
        }
    }
    for t in 0..16 {
        for c in 0..4 {
            wt[t][c] = if c < 3 { src[t][3] / 255.0 } else { 4.0 };
            sw[t][c] = wt[t][c].sqrt();
        }
    }
    let (mut mean, mut swm) = ([0.0f64; 4], [0.0f64; 4]);
    for c in 0..4 {
        let (mut s, mut w, mut m) = (0.0f64, 0.0f64, 0.0f64);
        for t in 0..16 {
            s += src[t][c] * wt[t][c];
            w += wt[t][c];
            m += sw[t][c];
        }
        mean[c] = s / w;
        swm[c] = m / 16.0;
    }
    let mut cov = [[0.0f64; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let mut s = 0.0f64;
            for t in 0..16 {
                s += ((src[t][i] - mean[i]) * sw[t][i]) * ((src[t][j] - mean[j]) * sw[t][j]);
            }
            cov[i][j] = s;
        }
    }
    let mut axis = [0.5f64; 4];
    for _ in 0..16 {
        let mut nx = [0.0f64; 4];
        for i in 0..4 {
            let mut s = 0.0f64;
            for j in 0..4 {
                s += cov[i][j] * axis[j];
            }
            nx[i] = s;
        }
        normalise(&mut nx);
        axis = nx;
    }
    for c in 0..4 {
        axis[c] /= cmax(swm[c], 1e-3);
    }
    normalise(&mut axis);
    let (mut tmin, mut tmax) = (0.0f64, 0.0f64);
    for t in 0..16 {
        let mut s = 0.0f64;
        for c in 0..4 {
            s += (src[t][c] - mean[c]) * axis[c];
        }
        if t == 0 || s < tmin {
            tmin = s;
        }
        if t == 0 || s > tmax {
            tmax = s;
        }
    }
    let mut seeds = [[[0.0f64; 4]; 2]; 3];
    let (mut lo, mut hi) = (0, 0);
    for t in 1..16 {
        if src[t][3] < src[lo][3] {
            lo = t;
        }
        if src[t][3] > src[hi][3] {
            hi = t;
        }
    }
    for c in 0..4 {
        let (mut mn, mut mx) = (src[0][c], src[0][c]);
        for row in &src[1..] {
            mn = cmin(mn, row[c]);
            mx = cmax(mx, row[c]);
        }
        seeds[0][0][c] = mean[c] + tmin * axis[c];
        seeds[0][1][c] = mean[c] + tmax * axis[c];
        seeds[1][0][c] = mn;
        seeds[1][1][c] = mx;
        seeds[2][0][c] = src[lo][c];
        seeds[2][1][c] = src[hi][c];
    }
    let mut best: Option<Cand> = None;
    for s in &seeds {
        let c = candidate(px, &src, &wt, &s[0], &s[1]);
        if best.as_ref().is_none_or(|b| c.err < b.err) {
            best = Some(c);
        }
    }
    let best = best.expect("three seeds");
    let mut ix = [0i64; 16];
    for t in 0..16 {
        ix[t] = best.idx[t] as i64;
    }
    let (mut e0, mut e1) = (best.a.q, best.b.q);
    let (mut p0, mut p1) = (best.a.p, best.b.p);
    if ix[0] >= 8 {
        std::mem::swap(&mut e0, &mut e1);
        std::mem::swap(&mut p0, &mut p1);
        for i in &mut ix {
            *i = 15 - *i;
        }
    }
    pack6(&e0, &e1, p0, p1, &ix)
}

pub(super) fn decode6(b: &[u8; 16]) -> Block {
    let bit = |pos: usize| ((b[pos >> 3] >> (pos & 7)) & 1) as i64;
    let get = |pos: usize, n: usize| {
        let mut v = 0i64;
        for i in 0..n {
            v |= bit(pos + i) << i;
        }
        v
    };
    let mut ep = [[0i64; 4]; 2];
    let mut pos = 7;
    for c in 0..4 {
        for e in &mut ep {
            e[c] = get(pos, 7);
            pos += 7;
        }
    }
    for e in &mut ep {
        for c in 0..4 {
            e[c] = (e[c] << 1) | bit(pos);
        }
        pos += 1;
    }
    let mut out = [[0i64; 4]; 16];
    for (t, o) in out.iter_mut().enumerate() {
        let n = if t == 0 { 3 } else { 4 };
        let ix = get(pos, n) as usize;
        pos += n;
        for c in 0..4 {
            o[c] = interp(ep[0][c], ep[1][c], W4[ix]);
        }
    }
    out
}
