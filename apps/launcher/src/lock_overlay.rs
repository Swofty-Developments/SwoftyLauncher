//! A full-window overlay that blocks the page under it while something long
//! runs: a title, a line of detail, a progress bar and an optional action.

use crate::scale::px;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Sizable, TITLE_BAR_HEIGHT, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::theme::Palette;

pub(crate) struct LockOverlay {
    title: SharedString,
    detail: Option<SharedString>,
    status: Option<SharedString>,
    fraction: Option<f32>,
    action: Option<AnyElement>,
}

impl LockOverlay {
    pub(crate) fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            detail: None,
            status: None,
            fraction: None,
            action: None,
        }
    }

    pub(crate) fn detail(mut self, text: impl Into<SharedString>) -> Self {
        self.detail = Some(text.into());
        self
    }

    pub(crate) fn status(mut self, text: impl Into<SharedString>) -> Self {
        self.status = Some(text.into());
        self
    }

    pub(crate) fn progress(mut self, fraction: f32) -> Self {
        self.fraction = Some(fraction.clamp(0., 1.));
        self
    }

    pub(crate) fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    pub(crate) fn render(self, id: impl Into<ElementId>, p: Palette) -> impl IntoElement {
        let bar = self.fraction.map(|done| {
            v_flex()
                .w_full()
                .gap(px(6.))
                .child(
                    div()
                        .h(px(6.))
                        .w_full()
                        .rounded_full()
                        .bg(p.border_variant)
                        .child(div().h_full().w(relative(done)).rounded_full().bg(p.accent)),
                )
                .child(
                    h_flex()
                        .w_full()
                        .gap(px(10.))
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(div().flex_1().min_w_0().truncate().children(self.status.clone()))
                        .child(format!("{}%", (done * 100.).floor())),
                )
        });
        div()
            .id(id)
            .absolute()
            .top(TITLE_BAR_HEIGHT)
            .left_0()
            .right_0()
            .bottom_0()
            .occlude()
            .bg(p.background)
            .child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .px(px(24.))
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(560.))
                            .gap(px(16.))
                            .items_center()
                            .child(Spinner::new().with_size(px(28.)))
                            .child(
                                div()
                                    .text_size(px(22.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(p.text)
                                    .child(self.title),
                            )
                            .when_some(self.detail, |c, text| {
                                c.child(
                                    div()
                                        .w_full()
                                        .text_center()
                                        .text_size(px(13.))
                                        .line_height(px(19.))
                                        .text_color(p.text_muted)
                                        .child(text),
                                )
                            })
                            .children(bar)
                            .children(self.action),
                    ),
            )
    }
}
