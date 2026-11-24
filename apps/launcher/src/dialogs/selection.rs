//! The dialog owns its draft, independently of page refreshes and form slots.
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::radio::Radio;
use gpui_kit::component::{Disableable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::view::SelectionDialog;
use swofty_i18n::tr;

use crate::ui::scale::px;
use crate::ui::theme::{ScrollArea, palette};

pub(crate) type Confirm = Rc<dyn Fn(Vec<String>, &mut Window, &mut App) -> bool>;

struct Selection {
    spec: SelectionDialog,
    values: Vec<String>,
    danger: bool,
    confirm: Confirm,
}

impl Selection {
    fn set(&mut self, value: &str, on: bool) {
        if !self.spec.options.iter().any(|o| o.value == value) {
            return;
        }
        if !self.spec.multiple {
            self.values.clear();
        } else {
            self.values.retain(|v| v != value);
        }
        if on {
            self.values.push(value.to_owned());
        }
    }

    fn ordered(&self) -> Vec<String> {
        self.spec
            .options
            .iter()
            .filter(|o| self.values.contains(&o.value))
            .map(|o| o.value.clone())
            .collect()
    }
}

/// Uses the launcher's native alert surface and focus trap. A false confirm
/// result leaves the dialog open, for example when its origin is no longer valid.
pub(crate) fn open(
    spec: SelectionDialog,
    danger: bool,
    confirm: Confirm,
    window: &mut Window,
    cx: &mut App,
) -> Result<(), String> {
    spec.validate()?;
    let title = spec.title.clone();
    let text = spec.text.clone();
    let selection = cx.new(|_| Selection {
        values: spec.selected.clone(),
        spec,
        danger,
        confirm,
    });
    window.open_alert_dialog(cx, move |alert, window, _| {
        alert
            .width(px(
                (f32::from(window.viewport_size().width) - 48.).clamp(240., 540.)
            ))
            .title(title.clone())
            .when(!text.is_empty(), |alert| alert.description(text.clone()))
            .child(selection.clone())
            // The entity renders its footer so validity updates along with its choices.
            .footer(div())
            .on_ok(|_, _, _| false)
    });
    Ok(())
}

impl Render for Selection {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let mut rows = v_flex().gap(px(8.));
        let mut group = None;
        for (i, option) in self.spec.options.iter().enumerate() {
            if option.group != group {
                if let Some(label) = &option.group {
                    rows = rows.child(
                        div()
                            .pt(px(8.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text_muted)
                            .child(label.clone()),
                    );
                }
                group.clone_from(&option.group);
            }
            let checked = self.values.contains(&option.value);
            let value = option.value.clone();
            let control = if self.spec.multiple {
                Checkbox::new(("selection-choice", i))
                    .label(option.label.clone())
                    .checked(checked)
                    .on_change(cx.listener(move |this, on: &bool, _, cx| {
                        this.set(&value, *on);
                        cx.notify();
                    }))
                    .into_any_element()
            } else {
                Radio::new(("selection-choice", i))
                    .label(option.label.clone())
                    .checked(checked)
                    .on_change(cx.listener(move |this, on: &bool, _, cx| {
                        this.set(&value, *on);
                        cx.notify();
                    }))
                    .into_any_element()
            };
            rows = rows.child(v_flex().gap(px(3.)).child(control).when_some(
                option.help.clone(),
                |d, help| {
                    d.child(
                        div()
                            .pl(px(24.))
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(help),
                    )
                },
            ));
        }
        if self.spec.options.is_empty() {
            rows = rows.child(tr("No options available"));
        }
        let values = self.ordered();
        let valid = self.spec.accepts(&values);
        let label = self.spec.apply.clone().unwrap_or_else(|| tr("Apply").into());
        let confirm = Button::new("selection-apply")
            .label(label)
            .map(|b| if self.danger { b.danger() } else { b.primary() })
            .disabled(!valid)
            .on_click(cx.listener(|this, _, window, cx| {
                let values = this.ordered();
                if this.spec.accepts(&values) && (this.confirm)(values, window, cx) {
                    window.close_dialog(cx);
                }
            }));
        v_flex()
            .gap(px(14.))
            .when(self.spec.multiple, |d| {
                d.child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            Button::new("selection-all")
                                .label(tr("Select all"))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.values = this.spec.options.iter().map(|o| o.value.clone()).collect();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("selection-clear")
                                .label(tr("Clear"))
                                .ghost()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.values.clear();
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                rows.max_h(px(
                    (f32::from(window.viewport_size().height) - 340.).clamp(80., 420.)
                ))
                .scroll_y("selection-options"),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        Button::new("selection-cancel")
                            .label(tr("Cancel"))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(confirm),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use serde_json::json;

    fn selection(multiple: bool) -> Selection {
        let spec: SelectionDialog = serde_json::from_value(json!({
            "title":"Choose", "multiple":multiple,
            "options":[{"value":"a","label":"A"},{"value":"b","label":"B"}],
            "selected":["a"]
        }))
        .unwrap();
        Selection {
            values: spec.selected.clone(),
            spec,
            danger: false,
            confirm: Rc::new(|_, _, _| true),
        }
    }

    #[test]
    fn checkbox_edits_stay_local_and_return_in_option_order() {
        let mut s = selection(true);
        s.set("b", true);
        s.set("a", false);
        s.set("a", true);
        assert_eq!(s.ordered(), ["a", "b"]);
        assert_eq!(s.spec.selected, ["a"]);
        s.set("missing", true);
        assert_eq!(s.ordered(), ["a", "b"]);
    }

    #[test]
    fn radio_choices_replace_the_previous_selection() {
        let mut s = selection(false);
        s.set("b", true);
        assert_eq!(s.ordered(), ["b"]);
        assert_eq!(s.spec.selected, ["a"]);
    }

    #[test]
    fn an_unchanged_default_can_confirm_and_empty_deletion_cannot() {
        let mut s = selection(true);
        assert!(s.spec.accepts(&s.ordered()));
        s.values.clear();
        assert!(!s.spec.accepts(&s.ordered()));
        s.spec.allow_empty = true;
        assert!(s.spec.accepts(&s.ordered()));
    }
}
