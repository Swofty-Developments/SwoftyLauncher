use gpui_kit::*;

use crate::ui::scale::px;
use crate::ui::theme::palette;

/// A thin bar filled to `fraction` (0 to 1).
#[derive(IntoElement)]
pub(crate) struct ProgressBar {
    fraction: f32,
    color: Option<Hsla>,
    height: f32,
}

impl ProgressBar {
    pub(crate) fn new(fraction: f32) -> Self {
        Self {
            fraction,
            color: None,
            height: 4.,
        }
    }

    /// Fill with an exact colour.
    pub(crate) fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
}

impl RenderOnce for ProgressBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = palette(cx);
        let fill = self.color.unwrap_or(p.accent);
        div()
            .h(px(self.height))
            .w_full()
            .rounded_full()
            .bg(p.border_variant)
            .overflow_hidden()
            .child(
                div()
                    .h_full()
                    .w(relative(self.fraction.clamp(0., 1.)))
                    .rounded_full()
                    .bg(fill),
            )
    }
}
