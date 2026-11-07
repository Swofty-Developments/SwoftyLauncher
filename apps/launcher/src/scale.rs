use std::sync::atomic::{AtomicU32, Ordering};

use gpui_kit::component::Theme;
use gpui_kit::{App, Pixels, Window};
use swofty_core::session::UiMode;

pub const TV_SCALE: f32 = 1.3;
const BASE_REM: f32 = 16.;
const BASE_FONT: f32 = 14.;

static SCALE: AtomicU32 = AtomicU32::new(1.0_f32.to_bits());

pub fn scale_for(mode: UiMode) -> f32 {
    match mode {
        UiMode::Pc => 1.,
        UiMode::Tv => TV_SCALE,
    }
}

fn scaled(value: f32, scale: f32) -> f32 {
    value * scale
}

pub fn px(value: f32) -> Pixels {
    gpui_kit::px(scaled(value, f32::from_bits(SCALE.load(Ordering::Relaxed))))
}

pub fn apply(mode: UiMode, window: &mut Window, cx: &mut App) {
    let scale = scale_for(mode);
    SCALE.store(scale.to_bits(), Ordering::Relaxed);
    window.set_rem_size(gpui_kit::px(BASE_REM * scale));
    Theme::update(cx, |t| {
        t.font_size = gpui_kit::px(BASE_FONT * scale);
        t.notification.margins.bottom = gpui_kit::px((crate::play_bar::HEIGHT + 16.) * scale);
    });
    Theme::sync_base(cx);
    if mode.is_tv() {
        widen_scrollbars(cx);
    }
    window.refresh();
}

fn widen_scrollbars(cx: &mut App) {
    let mut base = cx.global::<gpui_kit::base::Theme>().clone();
    let styles = base
        .scrollbar
        .styles()
        .clone()
        .track(|s| s.width(gpui_kit::px(32.)))
        .track_hover(|s| s.width(gpui_kit::px(32.)))
        .track_active(|s| s.width(gpui_kit::px(32.)))
        .thumb(|s| {
            s.width(gpui_kit::px(14.))
                .inset(gpui_kit::px(9.))
                .min_length(gpui_kit::px(96.))
        })
        .thumb_hover(|s| {
            s.width(gpui_kit::px(16.))
                .inset(gpui_kit::px(8.))
                .min_length(gpui_kit::px(96.))
        })
        .thumb_active(|s| {
            s.width(gpui_kit::px(18.))
                .inset(gpui_kit::px(7.))
                .min_length(gpui_kit::px(96.))
        });
    base.scrollbar = base.scrollbar.with_styles(styles);
    cx.set_global(base);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc_keeps_the_layout_and_tv_scales_it_up() {
        assert_eq!(scale_for(UiMode::Pc), 1.);
        assert!(scale_for(UiMode::Tv) > 1.);
    }

    #[test]
    fn scaling_multiplies_lengths() {
        assert_eq!(scaled(10., 1.), 10.);
        assert_eq!(scaled(10., 1.5), 15.);
        assert_eq!(scaled(0., TV_SCALE), 0.);
    }
}
