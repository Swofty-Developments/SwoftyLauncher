//! The Donate to Developers button above Send Logs. It glows, a shine sweeps
//! across it and sparkles twinkle around the label, all from one animation.

use std::f32::consts::TAU;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::*;

pub const URL: &str = "https://donate.swofty.net/";
pub const LABEL: &str = "Donate to Developers";

const SPARKLE: &[u8] = include_bytes!("../assets/sparkle.svg");
const FROM: u32 = 0xf76707;
const TO: u32 = 0xd6336c;
const FROM_HOVER: u32 = 0xff8226;
const TO_HOVER: u32 = 0xe64980;
const GLOW: u32 = 0xff6b9d;
const GOLD: u32 = 0xffe08a;
const STAR: u32 = 0xffe7a8;
const CYCLE: Duration = Duration::from_millis(3600);
const FPS: f32 = 30.;
const SHINE_W: f32 = 22.;

struct Sparkle {
    right: f32,
    top: f32,
    size: f32,
    phase: f32,
}

const SPARKLES: [Sparkle; 4] = [
    Sparkle {
        right: 7.,
        top: 4.,
        size: 9.,
        phase: 0.,
    },
    Sparkle {
        right: 20.,
        top: 24.,
        size: 6.,
        phase: 0.4,
    },
    Sparkle {
        right: 52.,
        top: 1.,
        size: 5.,
        phase: 0.7,
    },
    Sparkle {
        right: 150.,
        top: 28.,
        size: 5.,
        phase: 0.25,
    },
];

pub(crate) fn button() -> impl IntoElement {
    div().flex_shrink_0().with_animation(
        "donate",
        Animation::new(CYCLE).repeat_synced().with_max_fps(FPS),
        |el, t| el.child(face(t)),
    )
}

fn face(t: f32) -> Stateful<Div> {
    let white = |alpha: f32| hsla(0., 0., 1., alpha);
    let breath = 0.5 - 0.5 * (TAU * t).cos();
    h_flex()
        .id("donate-button")
        .relative()
        .min_h(px(38.))
        .px(px(10.))
        .py(px(6.))
        .gap(px(9.))
        .items_center()
        .rounded(px(8.))
        .overflow_hidden()
        .border_1()
        .border_color(Hsla::from(rgb(GOLD)).opacity(0.3 + 0.25 * breath))
        .bg(gradient(FROM, TO))
        .hover(|s| s.bg(gradient(FROM_HOVER, TO_HOVER)))
        .shadow(vec![BoxShadow {
            color: Hsla::from(rgb(GLOW)).opacity(0.18 + 0.22 * breath),
            offset: point(px(0.), px(0.)),
            blur_radius: px(6. + 6. * breath),
            spread_radius: px(0.),
            inset: false,
        }])
        .cursor_pointer()
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .text_color(white(1.))
        .on_click(|_, _, cx| cx.open_url(URL))
        .child(Icon::new(IconName::Heart).size(px(15.)).flex_shrink_0())
        .child(div().flex_1().min_w_0().truncate().child(LABEL))
        .child(shine(t))
        .children(SPARKLES.iter().map(|s| sparkle(s, t)))
}

fn gradient(from: u32, to: u32) -> Background {
    linear_gradient(
        90.,
        linear_color_stop(rgb(from), 0.),
        linear_color_stop(rgb(to), 1.),
    )
}

fn shine(t: f32) -> Div {
    let white = |alpha: f32| hsla(0., 0., 1., alpha);
    let sweep = ((t - 0.15) / 0.4).clamp(0., 1.);
    let eased = sweep * sweep * (3. - 2. * sweep);
    h_flex()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(-2. * SHINE_W + eased * 260.))
        .child(div().h_full().w(px(SHINE_W)).bg(linear_gradient(
            90.,
            linear_color_stop(white(0.), 0.),
            linear_color_stop(white(0.2), 1.),
        )))
        .child(div().h_full().w(px(SHINE_W)).bg(linear_gradient(
            90.,
            linear_color_stop(white(0.2), 0.),
            linear_color_stop(white(0.), 1.),
        )))
}

fn sparkle(s: &Sparkle, t: f32) -> Svg {
    let glint = (TAU * (2. * t + s.phase)).sin().max(0.).powi(2);
    let size = s.size * (0.4 + 0.6 * glint);
    let inset = (s.size - size) / 2.;
    svg()
        .data(SPARKLE)
        .absolute()
        .right(px(s.right + inset))
        .top(px(s.top + inset))
        .size(px(size))
        .text_color(rgb(STAR))
        .opacity(0.1 + 0.7 * glint)
}
