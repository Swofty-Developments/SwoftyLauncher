use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::scale::px;
use crate::theme::Palette;

/// One row of a list card: a top divider unless it is `first`, tinted while `open`.
pub(crate) fn row(first: bool, open: bool, p: Palette) -> Div {
    v_flex()
        .flex_shrink_0()
        .when(!first, |d| d.border_t_1().border_color(p.border_variant))
        .when(open, |d| d.bg(p.surface))
}

/// The clickable head of a [`row`].
pub(crate) fn row_head(id: impl Into<SharedString>, open: bool, p: Palette) -> Stateful<Div> {
    h_flex()
        .id(id.into())
        .h(px(54.))
        .px(px(14.))
        .gap(px(12.))
        .cursor_pointer()
        .when(!open, |d| d.hover(move |s| s.bg(p.hover)))
}

type Change = Box<dyn Fn(&bool, &mut Window, &mut App) + 'static>;

/// A switch that doesn't pass its clicks on to a clickable row around it.
#[derive(IntoElement)]
pub(crate) struct Toggle {
    id: SharedString,
    on: bool,
    disabled: bool,
    on_change: Option<Change>,
}

impl Toggle {
    pub(crate) fn new(id: impl Into<SharedString>, on: bool) -> Self {
        Self {
            id: id.into(),
            on,
            disabled: false,
            on_change: None,
        }
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on_change(mut self, f: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }
}

impl RenderOnce for Toggle {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let hold = SharedString::from(format!("{}-hold", self.id));
        let switch = Switch::new(self.id)
            .checked(self.on)
            .small()
            .disabled(self.disabled)
            .when_some(self.on_change, |s, f| s.on_change(f));
        div()
            .id(hold)
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(switch)
    }
}
