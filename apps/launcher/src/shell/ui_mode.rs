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

impl Shell {
    pub(crate) fn scale_rescue(&self, p: Palette, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        (self.scaling.percent.is_some() && crate::ui::scale::current() < 0.6).then(|| {
            let f = crate::ui::scale::fixed;
            div()
                .id("ui-scale-rescue")
                .absolute()
                .right(f(12.))
                .bottom(f(12.))
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
}
