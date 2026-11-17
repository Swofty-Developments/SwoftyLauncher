//! The editor pane: its header, the lint status, the problems strip and the mode switch.

use crate::ui::scale::{Hit, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::Editor;
use gpui_kit::component::{Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::content::shaders::ConfigEntry;
use swofty_i18n::{tr, trf, trn};
use swofty_shaders::Severity;

use crate::features::shaders::{Mode, ShadersView};
use crate::ui::EmptyState;
use crate::ui::theme::{Palette, ScrollArea};
use crate::ui::tokens::radius;

use super::icon_of;

impl ShadersView {
    pub(in crate::features::shaders) fn editor_pane(
        &mut self,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = self.selected_entry().cloned();
        let Some(entry) = entry else {
            return self.empty_editor(p, cx);
        };
        let editable = entry.editable;
        let header = self.editor_header(&entry, p, cx);
        v_flex()
            .size_full()
            .min_h_0()
            .gap(px(10.))
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(p.border)
                    .bg(p.surface)
                    .overflow_hidden()
                    .child(
                        Editor::new(&self.editor.state)
                            .bordered(false)
                            .h(relative(1.))
                            .readonly(!editable),
                    ),
            )
            .child(self.problems_strip(p, cx))
            .into_any_element()
    }

    pub(super) fn editor_header(&self, entry: &ConfigEntry, p: Palette, cx: &mut Context<Self>) -> Div {
        let editable = entry.editable;
        let live = self.is_live(&entry.r#ref);
        let (pill_text, pill_color, pill_icon) = self.lint_status(live, p);
        let source = self.source_line(entry);
        let copy_id = entry.r#ref.clone();
        h_flex()
            .gap(px(10.))
            .items_center()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(entry.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(source),
                    ),
            )
            .when(!editable, |d| {
                d.child(
                    Button::new("shaders-copy-edit")
                        .outline()
                        .tap_sm()
                        .icon(IconName::Copy)
                        .label(tr("Copy to personal to edit"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.copy_to_personal(copy_id.clone(), window, cx)
                        })),
                )
            })
            .child(
                h_flex()
                    .h(px(24.))
                    .px(px(9.))
                    .gap(px(6.))
                    .items_center()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(pill_color.opacity(0.5))
                    .bg(pill_color.opacity(0.12))
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(pill_color)
                    .child(Icon::new(pill_icon).size(px(13.)))
                    .child(pill_text),
            )
    }

    pub(super) fn lint_status(&self, live: bool, p: Palette) -> (String, Hsla, IconName) {
        let errors = self.editor.lint.errors();
        let warnings = self.editor.lint.warnings();
        if !self.editor.lint.done {
            (tr("Checking").to_owned(), p.text_muted, IconName::LoaderCircle)
        } else if errors > 0 {
            (
                trf!(
                    "{problems} · not applied",
                    problems = trn(errors, "{count} problem", "{count} problems")
                ),
                p.error,
                IconName::CircleAlert,
            )
        } else if live {
            (
                tr("Valid · live in game").to_owned(),
                p.success,
                IconName::CircleCheck,
            )
        } else if warnings > 0 {
            (
                trf!(
                    "Valid · {warnings}",
                    warnings = trn(warnings, "{count} warning", "{count} warnings")
                ),
                p.warning,
                IconName::TriangleAlert,
            )
        } else {
            (tr("Valid").to_owned(), p.success, IconName::CircleCheck)
        }
    }

    pub(super) fn source_line(&self, entry: &ConfigEntry) -> String {
        if entry.editable {
            if self.editor.saving {
                tr("Saving")
            } else {
                tr("Saved automatically")
            }
            .to_owned()
        } else {
            trf!("From {source} · read-only", source = entry.source)
        }
    }

    pub(super) fn empty_editor(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .rounded(radius::md())
            .border_1()
            .border_dashed()
            .border_color(p.border)
            .child(
                EmptyState::new(IconName::Palette, tr("No config selected"))
                    .body(tr("Pick one on the right, or start a new one."))
                    .action(
                        Button::new("shaders-empty-new")
                            .outline()
                            .icon(IconName::Plus)
                            .label(tr("New config"))
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| this.new_config(window, cx))),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn problems_strip(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let problems = self.editor.lint.problems.clone();
        if problems.is_empty() {
            return div().into_any_element();
        }
        let counts = [
            (Severity::Error, p.error),
            (Severity::Warning, p.warning),
            (Severity::Info, p.accent),
        ]
        .into_iter()
        .filter_map(|(sev, color)| {
            let n = problems.iter().filter(|x| x.severity == sev).count();
            (n > 0).then(|| {
                h_flex()
                    .gap(px(4.))
                    .items_center()
                    .text_color(color)
                    .child(Icon::new(icon_of(sev)).size(px(13.)))
                    .child(div().text_size(px(12.)).child(n.to_string()))
            })
        })
        .collect::<Vec<_>>();
        let editable = self.selected_entry().is_some_and(|e| e.editable);
        let rows = problems.into_iter().enumerate().map(|(i, problem)| {
            let color = match problem.severity {
                Severity::Error => p.error,
                Severity::Warning => p.warning,
                _ => p.accent,
            };
            let (line, column) = (problem.line, problem.column);
            let fix = problem.fix.clone();
            h_flex()
                .id(SharedString::from(format!("problem-{i}")))
                .gap(px(8.))
                .px(px(10.))
                .py(px(5.))
                .items_center()
                .cursor_pointer()
                .hover(move |s| s.bg(p.hover))
                .on_click(cx.listener(move |this, _, window, cx| this.jump_to(line, column, window, cx)))
                .child(
                    Icon::new(icon_of(problem.severity))
                        .size(px(13.))
                        .text_color(color),
                )
                .child(
                    div()
                        .w(px(46.))
                        .flex_shrink_0()
                        .text_size(px(12.))
                        .text_color(p.text_placeholder)
                        .child(format!("{}:{}", problem.line + 1, problem.column + 1)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.))
                        .text_color(p.text)
                        .child(problem.message),
                )
                .when_some(fix.filter(|_| editable), |d, fix| {
                    d.child(
                        Button::new(SharedString::from(format!("problem-fix-{i}")))
                            .outline()
                            .tap_sm()
                            .label(fix.title.clone())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.apply_fix(fix.clone(), window, cx)
                            })),
                    )
                })
        });
        v_flex()
            .flex_shrink_0()
            .max_h(px(168.))
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .child(
                h_flex()
                    .px(px(10.))
                    .py(px(5.))
                    .gap(px(12.))
                    .items_center()
                    .border_b_1()
                    .border_color(p.border_variant)
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(p.text_placeholder)
                            .child(tr("PROBLEMS")),
                    )
                    .children(counts),
            )
            .child(
                v_flex()
                    .id("shaders-problems")
                    .min_h_0()
                    .scroll_y("shaders-problems-scroll")
                    .children(rows),
            )
            .into_any_element()
    }

    pub(in crate::features::shaders) fn mode_switch(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let seg = |mode: Mode, label: &'static str, icon: IconName| {
            let on = self.mode == mode;
            h_flex()
                .id(SharedString::from(format!("shaders-mode-{label}")))
                .h(px(28.))
                .px(px(14.))
                .gap(px(7.))
                .items_center()
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .map(|d| {
                    if on {
                        d.bg(p.selected).text_color(p.text)
                    } else {
                        d.text_color(p.text_muted).hover(move |s| s.text_color(p.text))
                    }
                })
                .child(Icon::new(icon).size(px(14.)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.set_mode(mode, cx)))
        };
        h_flex()
            .p(px(2.))
            .gap(px(2.))
            .rounded(px(7.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .child(seg(Mode::Editor, tr("Editor"), IconName::SquarePen))
            .child(seg(Mode::Preview, tr("Preview"), IconName::Play))
            .child(seg(Mode::Documentation, tr("Documentation"), IconName::BookOpen))
            .into_any_element()
    }
}
