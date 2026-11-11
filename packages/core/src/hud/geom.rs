//! HUD units to scene units, the game's text model and the sweep's arc fit.
//!
//! A HUD point `(u, v)` is 1 px of a 640x360 frame, origin at the dial centre, `v` down. The scene
//! is in OSD units (1 unit = screen width / 19.2), `y` up. Measured in game, not derived.

pub const K: f64 = 0.018;
/// The game font's cap height over its em, measured on OSD_24_bold and 17_bold.
pub const CAP_EM: f64 = 0.672;
/// The cap centre's height above the eText origin, in em; the origin is the baseline.
pub const CAP_OFF: f64 = 0.336;
/// A clip's length: a fraction `f` bound to the node plays the clip at `f * ANIM_T`.
pub const ANIM_T: f64 = 1.0;

pub fn scene_point(u: f64, v: f64) -> [f64; 3] {
    [K * u, -K * v, 0.0]
}

pub const STYLES: &[(&str, f64)] = &[
    ("17_bold", 17.0),
    ("24_bold", 24.0),
    ("OSD_24_bold", 24.0),
    ("OSD_17_bold", 17.0),
];

pub fn style_height(style: &str) -> Option<f64> {
    STYLES.iter().find(|(n, _)| *n == style).map(|(_, h)| *h)
}

/// A rotation of `theta` degrees (counter-clockwise on screen) about z, written with `w <= 0` like
/// the stock files.
pub fn quat_z(theta: f64) -> [f64; 4] {
    let h = theta.to_radians() / 2.0;
    let mut q = [0.0, 0.0, h.sin(), h.cos()];
    if q[3] > 0.0 {
        q = q.map(|x| -x);
    }
    q
}

pub const QUAT_IDENTITY: [f64; 4] = [-0.0, -0.0, 0.0, -1.0];
#[allow(clippy::approx_constant)]
pub const QUAT_QUAD: [f64; 4] = [-0.707107, -0.0, 0.0, -0.707107];

pub struct TextPlace {
    pub scale: f64,
    pub box_width: f64,
    pub origin_y: f64,
}

/// A text of cap height `cap` (HUD units) in `style` with a box of `box_hud`: the holder null's
/// scale, the eText's box width in its own units and the eText's offset below the holder.
pub fn text_place(cap: f64, style_h: f64, box_hud: f64) -> TextPlace {
    let scale = (K * cap / CAP_EM) / (style_h / 100.0);
    TextPlace {
        scale,
        box_width: K * box_hud / scale,
        origin_y: -CAP_OFF * style_h / 100.0,
    }
}

pub struct Sweep {
    pub frame_deg: f64,
    pub sigma: f64,
    pub reach: f64,
    pub keys: Vec<(f64, f64)>,
}

/// The sweep of a quad about `pivot` under a mask covering `bounds`: the frame the fill quad hangs
/// in is turned to `start` degrees, the clip keys are `(fraction, angle)` in radians relative to it.
pub fn sweep(pivot: [f64; 2], start: f64, arc: &[[f64; 2]], bounds: [f64; 4]) -> Sweep {
    let c = scene_point(pivot[0], pivot[1]);
    let keys: Vec<(f64, f64)> = arc.iter().map(|k| (k[0], k[1].to_radians())).collect();
    let last = keys.last().map_or(0.0, |k| k.1);
    let sigma = if last > 0.0 { 1.0 } else { -1.0 };
    let mut reach = 0.0f64;
    for x in [bounds[0], bounds[2]] {
        for y in [bounds[1], bounds[3]] {
            reach = reach.max((K * x - c[0]).hypot(-K * y - c[1]));
        }
    }
    Sweep {
        frame_deg: start,
        sigma,
        reach: reach * 1.15,
        keys,
    }
}

/// `{nn}` in a leaf pattern is the two-digit index.
pub fn leaf_name(pattern: &str, index: u32) -> String {
    pattern.replace("{nn}", &format!("{index:02}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_points_flip_v() {
        assert_eq!(scene_point(10.0, 5.0), [0.18, -0.09, 0.0]);
    }

    #[test]
    fn rotations_keep_w_non_positive() {
        for t in [-270.0, -90.0, 0.0, 33.0, 180.0, 225.0] {
            assert!(quat_z(t)[3] <= 0.0, "{t}");
        }
        let q = quat_z(90.0);
        assert!((q[2].abs() - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-5);
    }

    #[test]
    fn text_scale_matches_the_measured_model() {
        let p = text_place(9.5, 24.0, 40.0);
        assert!((p.scale - (0.018 * 9.5 / 0.672) / 0.24).abs() < 1e-12);
        assert!((p.origin_y + 0.336 * 0.24).abs() < 1e-12);
        assert!((p.box_width * p.scale - 0.018 * 40.0).abs() < 1e-12);
    }

    #[test]
    fn sweep_sign_follows_the_arc() {
        let s = sweep(
            [0.0, 0.0],
            225.0,
            &[[0.0, 0.0], [1.0, -270.0]],
            [-10.0, -10.0, 10.0, 10.0],
        );
        assert_eq!(s.sigma, -1.0);
        assert!(s.reach > 0.018 * 10.0);
        assert_eq!(
            leaf_name("view_model_data.hud_x_dp{nn}", 7),
            "view_model_data.hud_x_dp07"
        );
    }
}
