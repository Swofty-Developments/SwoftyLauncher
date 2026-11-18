/// `std::min(a, b)` of the C++ standard library: `b < a ? b : a`.
pub(super) fn cmin(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// `std::max(a, b)` of the C++ standard library: `a < b ? b : a`.
pub(super) fn cmax(a: f64, b: f64) -> f64 {
    if a < b { b } else { a }
}

pub(super) fn pairwise16(a: &[f64; 16]) -> f64 {
    let mut r = [0.0f64; 8];
    r.copy_from_slice(&a[..8]);
    for j in 0..8 {
        r[j] += a[8 + j];
    }
    ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]))
}

pub(super) fn rint_to(v: f64, lo: i64, hi: i64) -> i64 {
    let r = v.round_ties_even();
    if r.is_nan() || r <= lo as f64 {
        return lo;
    }
    if r > hi as f64 {
        return hi;
    }
    r as i64
}

#[derive(Default)]
pub(super) struct Bits {
    pub(super) word: [u8; 16],
    pos: usize,
}

impl Bits {
    pub(super) fn put(&mut self, v: u64, n: usize) {
        let v = if n >= 64 { v } else { v & ((1u64 << n) - 1) };
        for i in 0..n {
            if (v >> i) & 1 != 0 {
                self.word[self.pos >> 3] |= 1u8 << (self.pos & 7);
            }
            self.pos += 1;
        }
    }
}

pub(super) fn interp(a: i64, b: i64, w: i64) -> i64 {
    ((64 - w) * a + w * b + 32) >> 6
}

pub(super) fn normalise(v: &mut [f64]) {
    let mut s = 0.0f64;
    for x in v.iter() {
        s += x * x;
    }
    let norm = cmax(s.sqrt(), 1e-12);
    for x in v.iter_mut() {
        *x /= norm;
    }
}
