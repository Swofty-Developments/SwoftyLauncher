use crate::shell::notices::{Notification, Notify};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::*;
use swofty_core::session::{Config, UI_SCALE_MAX, UI_SCALE_MIN, UiMode, clamp_ui_scale};

use crate::shell::Shell;
use crate::ui::theme::Palette;
use swofty_i18n::tr;

pub(crate) struct Scaling {
    pub(crate) mode: Option<UiMode>,
    pub(crate) percent: Option<u16>,
    pub(crate) slider: Entity<SliderState>,
    pub(crate) drag: Option<u16>,
    _slider_changes: Subscription,
}

impl Scaling {
    pub(crate) fn new(config: &Config, window: &mut Window, cx: &mut Context<Shell>) -> Self {
        let percent = config.ui_scale_percent();
        let slider = cx.new(|_| {
            SliderState::new()
                .min(f32::from(UI_SCALE_MIN))
                .max(f32::from(UI_SCALE_MAX))
                .step(1.)
                .default_value(f32::from(percent.unwrap_or(100)))
        });
        let slider_changes = cx.subscribe_in(
            &slider,
            window,
            |this, _, ev: &SliderEvent, window, cx| match ev {
                SliderEvent::Change(v) => {
                    this.scaling.drag = Some(clamp_ui_scale(v.start().round().max(0.) as u16));
                    cx.notify();
                }
                SliderEvent::Release(v) => {
                    this.scaling.drag = None;
                    this.set_ui_scale(Some(clamp_ui_scale(v.start().round().max(0.) as u16)), window, cx);
                }
            },
        );
        Self {
            mode: config.ui_mode,
            percent,
            slider,
            drag: None,
            _slider_changes: slider_changes,
        }
    }
}

impl Shell {
    pub(crate) fn start_ui_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = self.scaling.mode.unwrap_or_default();
        crate::ui::scale::apply(mode, self.scaling.percent, window, cx);
        if !self.store.config().needs_onboarding() {
            window.focus(&self.root_focus, cx);
            self.check_ad_consent(window, cx);
            return;
        }
        self.start_onboarding(mode, window, cx);
    }

    pub(crate) fn set_ui_mode(&mut self, mode: UiMode, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.ui_mode = Some(mode);
        self.save_config_toast(&config, window, cx);
        self.scaling.mode = config.ui_mode;
        crate::ui::scale::apply(mode, self.scaling.percent, window, cx);
        cx.notify();
    }

    pub(crate) fn set_ui_scale(&mut self, percent: Option<u16>, window: &mut Window, cx: &mut Context<Self>) {
        let mut config = self.store.config();
        config.ui_scale = percent.map(swofty_core::session::clamp_ui_scale);
        self.save_config_toast(&config, window, cx);
        self.scaling.percent = config.ui_scale_percent();
        self.scaling.drag = None;
        let at = f32::from(self.scaling.percent.unwrap_or(100));
        self.scaling
            .slider
            .update(cx, |s, cx| s.set_value(at, window, cx));
        crate::ui::scale::apply(config.ui_mode_or_default(), self.scaling.percent, window, cx);
        cx.notify();
    }

    pub(crate) fn save_config_toast(
        &mut self,
        config: &swofty_core::session::Config,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(e) = self.store.save_config(config) {
            crate::app::app_log::action("save settings").failed(&e);
            window.notify(Notification::error(e.to_string()), cx);
        }
    }
}

const RESCUE_BELOW: f32 = 0.6;
const RESCUE_GAP: f32 = 12.;
const RESCUE_HEIGHT: f32 = 40.;

pub(crate) fn needs_rescue(percent: Option<u16>, scale: f32, window_width: f32) -> bool {
    percent.is_some_and(|percent| {
        scale < RESCUE_BELOW || (percent > 100 && window_width / scale < crate::ui::scale::MIN_WINDOW_WIDTH)
    })
}

pub(crate) fn rescue_bottom(scale: f32, window_height: f32) -> f32 {
    (crate::shell::play_bar::HEIGHT * scale + RESCUE_GAP)
        .min(window_height - RESCUE_HEIGHT - RESCUE_GAP)
        .max(RESCUE_GAP)
}

pub(crate) fn resets_scale(keystroke: &Keystroke, secondary: Modifiers) -> bool {
    keystroke.key == "0" && keystroke.modifiers == secondary
}

impl Shell {
    pub(crate) fn scale_rescue(
        &self,
        p: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let scale = crate::ui::scale::current();
        let viewport = window.viewport_size();
        needs_rescue(self.scaling.percent, scale, f32::from(viewport.width)).then(|| {
            let f = crate::ui::scale::fixed;
            div()
                .id("ui-scale-rescue")
                .absolute()
                .right(f(RESCUE_GAP))
                .bottom(f(rescue_bottom(scale, f32::from(viewport.height))))
                .px(f(14.))
                .py(f(8.))
                .rounded(f(6.))
                .bg(p.accent)
                .text_color(p.on_accent)
                .text_size(f(14.))
                .cursor_pointer()
                .child(tr("Reset interface scale"))
                .on_click(cx.listener(|this, _, window, cx| this.set_ui_scale(None, window, cx)))
        })
    }

    pub(crate) fn reset_scale_key(
        &mut self,
        ev: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !resets_scale(&ev.keystroke, Modifiers::secondary_key(cx)) {
            return false;
        }
        if self.scaling.percent.is_some() {
            self.set_ui_scale(None, window, cx);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{RESCUE_GAP, RESCUE_HEIGHT, needs_rescue, rescue_bottom, resets_scale};
    use crate::ui::scale::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, TV_SCALE};
    use gpui_kit::{Keystroke, Modifiers};
    use swofty_core::session::UiMode;

    #[test]
    fn auto_never_shows_the_rescue() {
        for scale in [0.01, 0.5, 1., TV_SCALE, 4.] {
            assert!(!needs_rescue(None, scale, 800.));
        }
    }

    #[test]
    fn zoomed_out_below_sixty_percent_shows_the_rescue() {
        assert!(needs_rescue(Some(1), 0.01, 1920.));
        assert!(needs_rescue(Some(59), 0.59, 1920.));
        assert!(!needs_rescue(Some(60), 0.6, 1920.));
        assert!(!needs_rescue(Some(90), 0.9, MIN_WINDOW_WIDTH));
    }

    #[test]
    fn zoomed_in_shows_the_rescue_once_the_window_is_narrower_than_the_smallest_window() {
        assert!(needs_rescue(Some(400), 4., 1920.));
        assert!(needs_rescue(Some(400), 4., 3840.));
        assert!(needs_rescue(Some(188), 1.88, 1920.));
        assert!(!needs_rescue(Some(187), 1.87, 1920.));
        assert!(!needs_rescue(Some(150), 1.5, 1920.));
        assert!(!needs_rescue(Some(200), 2., 2560.));
        assert!(needs_rescue(Some(101), 1.01, MIN_WINDOW_WIDTH));
    }

    #[test]
    fn tv_mode_counts_its_own_scale() {
        let tv = |percent: u16| crate::ui::scale::scale_with(UiMode::Tv, Some(percent));
        assert!(!needs_rescue(Some(100), tv(100), 1280.));
        assert!(!needs_rescue(Some(90), tv(90), 1280.));
        assert!(needs_rescue(Some(110), tv(110), 1280.));
        assert!(!needs_rescue(Some(140), tv(140), 2560.));
        assert!(needs_rescue(Some(400), tv(400), 3840.));
    }

    #[test]
    fn the_rescue_sits_above_the_play_bar_and_stays_in_the_window() {
        let play_bar = crate::shell::play_bar::HEIGHT;
        for scale in [0.01, 0.5, 1., TV_SCALE, 2., 4., TV_SCALE * 4.] {
            let bottom = rescue_bottom(scale, MIN_WINDOW_HEIGHT);
            assert!(bottom >= play_bar * scale, "{scale}");
            assert!(bottom + RESCUE_HEIGHT <= MIN_WINDOW_HEIGHT, "{scale}");
        }
        assert_eq!(rescue_bottom(1., 900.), play_bar + RESCUE_GAP);
        assert_eq!(rescue_bottom(4., 200.), 200. - RESCUE_HEIGHT - RESCUE_GAP);
        assert_eq!(rescue_bottom(4., 10.), RESCUE_GAP);
    }

    #[test]
    fn ctrl_zero_resets_the_scale() {
        let key = |text: &'static str| Keystroke::parse(text).unwrap();
        let ctrl = Modifiers::control();
        assert!(resets_scale(&key("ctrl-0"), ctrl));
        assert!(!resets_scale(&key("0"), ctrl));
        assert!(!resets_scale(&key("ctrl-shift-0"), ctrl));
        assert!(!resets_scale(&key("ctrl-alt-0"), ctrl));
        assert!(!resets_scale(&key("ctrl-1"), ctrl));
        assert!(resets_scale(&key("cmd-0"), Modifiers::command()));
        assert!(!resets_scale(&key("cmd-0"), ctrl));
    }
}
