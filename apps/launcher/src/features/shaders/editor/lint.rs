//! Linting and saving a config as it is typed, and jumping to or fixing a problem.

use std::time::Duration;

use crate::ui::scale::px;
use gpui_kit::base::input::TextDecoration;
use gpui_kit::component::input::Position;
use gpui_kit::*;
use swofty_i18n::trf;
use swofty_shaders::{ActiveInstance, Config, Severity};

use crate::features::shaders::ShadersView;
use crate::ui::theme::palette;

use super::{LINT_DEBOUNCE, LintView, Problem, SAVE_DEBOUNCE, squiggle_range};

use super::language::{apply_edit, problem_of};

impl ShadersView {
    pub(in crate::features::shaders) fn edited(&mut self, cx: &mut Context<Self>) {
        let text = self.editor.state.read(cx).value().to_string();
        if text == self.editor.last_text {
            return;
        }
        self.editor.last_text = text.clone();
        self.schedule_lint(text, LINT_DEBOUNCE, cx);
    }

    pub(in crate::features::shaders) fn schedule_lint(
        &mut self,
        text: String,
        wait: Duration,
        cx: &mut Context<Self>,
    ) {
        self.editor.lint.rev += 1;
        let rev = self.editor.lint.rev;
        let catalog = self.catalog.clone();
        self.editor.lint_task = Some(cx.spawn(async move |this, cx| {
            if !wait.is_zero() {
                cx.background_executor().timer(wait).await;
            }
            let source = text.clone();
            let (problems, config, resolved) = cx
                .background_executor()
                .spawn(async move {
                    let lint = swofty_shaders::lint(&source, &catalog);
                    let instances = lint.config.as_ref().map(|c| swofty_shaders::resolve(c, &catalog));
                    let rules = lint
                        .config
                        .as_ref()
                        .map(|c| swofty_shaders::resolve_rules(c, &catalog));
                    let problems: Vec<Problem> = lint.diagnostics.into_iter().map(problem_of).collect();
                    (problems, lint.config, instances.zip(rules))
                })
                .await;
            this.update(cx, |v, cx| v.lint_done(rev, text, problems, config, resolved, cx))
                .ok();
        }));
    }

    pub(super) fn lint_done(
        &mut self,
        rev: u64,
        text: String,
        problems: Vec<Problem>,
        config: Option<Config>,
        resolved: Option<(Vec<ActiveInstance>, Vec<swofty_shaders::Rule>)>,
        cx: &mut Context<Self>,
    ) {
        if rev != self.editor.lint.rev {
            return;
        }
        let p = palette(cx);
        let squiggles: Vec<TextDecoration> = problems
            .iter()
            .filter_map(|problem| {
                let range = squiggle_range(&text, &problem.range)?;
                let color = match problem.severity {
                    Severity::Error => p.error,
                    Severity::Warning => p.warning,
                    Severity::Info => p.text_muted,
                    Severity::Hint => p.text_placeholder,
                };
                Some(TextDecoration::new(
                    range,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            color: Some(color),
                            thickness: px(1.),
                            wavy: true,
                        }),
                        ..Default::default()
                    },
                ))
            })
            .fold(Vec::<TextDecoration>::new(), |mut out, d| {
                if !out
                    .iter()
                    .any(|o| o.range.start < d.range.end && d.range.start < o.range.end)
                {
                    out.push(d);
                }
                out
            });
        self.editor.squiggles.set(squiggles, cx);
        *self.editor.problems.borrow_mut() = problems.clone();
        let valid = config.is_some();
        self.editor.lint = LintView {
            rev,
            problems,
            done: true,
        };
        if valid {
            if let Some((instances, rules)) = resolved {
                self.preview_instances(instances, rules, cx);
            }
            self.schedule_save(text, cx);
        }
        cx.notify();
    }

    pub(super) fn schedule_save(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(entry) = self.selected_entry().filter(|e| e.editable) else {
            return;
        };
        if text == self.editor.saved {
            return;
        }
        let rev = self.editor.lint.rev;
        let r = entry.r#ref.clone();
        let store = self.store.clone();
        let live = self.live_target(&r);
        self.editor.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            let stale = this.update(cx, |v, _| v.editor.lint.rev != rev).unwrap_or(true);
            if stale {
                return;
            }
            this.update(cx, |v, cx| {
                v.editor.saving = true;
                cx.notify();
            })
            .ok();
            let saved = text.clone();
            let done = cx
                .background_executor()
                .spawn(async move {
                    swofty_core::content::shaders::save(&store, &r, &text).map_err(|e| e.to_string())?;
                    if let Some((dir, name)) = live {
                        let profile = store.profile(&name).map_err(|e| e.to_string())?;
                        let active = swofty_core::content::shaders::active_file(&store, &profile);
                        swofty_core::content::shaders::push_live(&dir, &active).map_err(|e| e.to_string())?;
                    }
                    Ok::<(), String>(())
                })
                .await;
            this.update(cx, |v, cx| {
                v.editor.saving = false;
                match done {
                    Ok(()) => {
                        v.editor.saved = saved;
                        v.error = None;
                        v.refresh(cx);
                    }
                    Err(e) => v.error = Some(trf!("Couldn't save the config: {e}", e = e)),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    pub(in crate::features::shaders) fn jump_to(
        &mut self,
        line: u32,
        column: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.editor.state.clone();
        state.update(cx, |s, cx| {
            s.set_cursor_position(Position::new(line, column), window, cx);
            s.focus(window, cx);
        });
    }

    pub(in crate::features::shaders) fn apply_fix(
        &mut self,
        fix: swofty_shaders::Fix,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.editor.state.clone();
        state.update(cx, |s, cx| {
            apply_edit(s, fix.range.clone(), &fix.replacement, window, cx)
        });
        self.edited(cx);
    }
}
