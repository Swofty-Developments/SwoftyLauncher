mod art;
mod steps;
mod welcome;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Icon, Sizable, h_flex};
use gpui_kit::*;
use swofty_core::session::{ONBOARDING_VERSION, UiMode};
use swofty_i18n::tr;

use crate::scale::px;
use crate::shell::Shell;
use crate::theme::Palette;

const BUTTON_HEIGHT: f32 = 48.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    Welcome,
    Language,
    Display,
    Done,
}

impl Step {
    const ALL: [Step; 4] = [Step::Welcome, Step::Language, Step::Display, Step::Done];

    fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    fn next(self) -> Option<Step> {
        Self::ALL.get(self.index() + 1).copied()
    }

    fn previous(self) -> Option<Step> {
        self.index().checked_sub(1).map(|i| Self::ALL[i])
    }

    fn forward_label(self) -> &'static str {
        match self {
            Step::Welcome => tr("Get started"),
            Step::Done => tr("Start"),
            _ => tr("Next"),
        }
    }
}

pub(crate) struct Onboarding {
    step: Step,
    mode: UiMode,
    pub(crate) focus: FocusHandle,
}

impl Onboarding {
    pub(crate) fn new(mode: UiMode, cx: &mut App) -> Self {
        Self {
            step: Step::Welcome,
            mode,
            focus: cx.focus_handle(),
        }
    }
}

impl Shell {
    fn onboarding_go(&mut self, step: Step, cx: &mut Context<Self>) {
        if let Some(wizard) = &mut self.onboarding {
            wizard.step = step;
        }
        cx.notify();
    }

    fn onboarding_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.onboarding.as_ref().map(|w| w.step) else {
            return;
        };
        match step.next() {
            Some(next) => self.onboarding_go(next, cx),
            None => self.onboarding_finish(window, cx),
        }
    }

    fn onboarding_back(&mut self, cx: &mut Context<Self>) {
        let previous = self.onboarding.as_ref().and_then(|w| w.step.previous());
        if let Some(previous) = previous {
            self.onboarding_go(previous, cx);
        }
    }

    fn onboarding_pick_mode(&mut self, mode: UiMode, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(wizard) = &mut self.onboarding {
            wizard.mode = mode;
        }
        self.set_ui_mode(mode, window, cx);
    }

    fn onboarding_pick_language(&mut self, offset: isize, window: &mut Window, cx: &mut Context<Self>) {
        let languages = swofty_i18n::languages();
        let current = languages
            .iter()
            .position(|l| l.code == swofty_i18n::language())
            .unwrap_or(0);
        let target = current.saturating_add_signed(offset).min(languages.len() - 1);
        self.set_language(languages[target].code, window, cx);
    }

    pub(crate) fn onboarding_finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(wizard) = self.onboarding.take() else {
            return;
        };
        let mut config = self.store.config();
        config.onboarding = Some(ONBOARDING_VERSION);
        config.ui_mode = Some(wizard.mode);
        self.save_config_toast(&config, window, cx);
        self.ui_mode = config.ui_mode;
        cx.notify();
    }

    fn onboarding_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some((step, root_focused)) = self
            .onboarding
            .as_ref()
            .map(|w| (w.step, w.focus.is_focused(window)))
        else {
            return;
        };
        match (key, step) {
            ("escape", _) => self.onboarding_finish(window, cx),
            ("enter", _) if root_focused => self.onboarding_forward(window, cx),
            ("down", Step::Language) => self.onboarding_pick_language(1, window, cx),
            ("up", Step::Language) => self.onboarding_pick_language(-1, window, cx),
            ("left", Step::Display) => self.onboarding_pick_mode(UiMode::Pc, window, cx),
            ("right", Step::Display) => self.onboarding_pick_mode(UiMode::Tv, window, cx),
            ("left", _) => self.onboarding_back(cx),
            ("right", _) => self.onboarding_forward(window, cx),
            _ => {}
        }
    }

    pub(crate) fn onboarding_view(&self, p: Palette, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let wizard = self.onboarding.as_ref()?;
        let step = wizard.step;
        let body = match step {
            Step::Welcome => welcome::view(p),
            Step::Language => self.language_step(p, cx),
            Step::Display => self.display_step(wizard.mode, p, cx),
            Step::Done => steps::done(p),
        };
        let stage = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .child(steps::enter(step, body));
        Some(
            crate::overlay::takeover("onboarding", p)
                .track_focus(&wizard.focus)
                .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    this.onboarding_key(ev.keystroke.key.as_str(), window, cx);
                }))
                .child(welcome::backdrop(step, p))
                .child(crate::overlay::title_bar(p))
                .child(stage)
                .child(self.footer(step, p, cx)),
        )
    }

    fn footer(&self, step: Step, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let back = match step.previous() {
            Some(_) => Button::new("onboarding-back")
                .outline()
                .large()
                .h(px(BUTTON_HEIGHT))
                .px(px(24.))
                .child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(Icon::new(IconName::ArrowLeft).size(px(18.)))
                        .child(div().text_size(px(16.)).child(tr("Back"))),
                )
                .on_click(cx.listener(|this, _, _, cx| this.onboarding_back(cx)))
                .into_any_element(),
            None => Button::new("onboarding-skip")
                .ghost()
                .large()
                .h(px(BUTTON_HEIGHT))
                .px(px(20.))
                .child(
                    div()
                        .text_size(px(15.))
                        .text_color(p.text_muted)
                        .child(tr("Skip")),
                )
                .on_click(cx.listener(|this, _, window, cx| this.onboarding_finish(window, cx)))
                .into_any_element(),
        };
        let forward = Button::new("onboarding-next")
            .primary()
            .large()
            .h(px(BUTTON_HEIGHT))
            .px(px(28.))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(step.forward_label()),
                    )
                    .child(Icon::new(IconName::ArrowRight).size(px(18.))),
            )
            .on_click(cx.listener(|this, _, window, cx| this.onboarding_forward(window, cx)));
        h_flex()
            .w_full()
            .px(px(40.))
            .pb(px(32.))
            .items_center()
            .child(div().flex_1().flex().justify_start().child(back))
            .child(steps::dots(step, p))
            .child(div().flex_1().flex().justify_end().child(forward))
    }
}
