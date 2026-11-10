use crate::notices::{Notification, Notify};
use crate::scale::px;
use crate::shell::{Prompt, Shell};
use crate::theme::{Palette, palette};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::{Icon, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::rc::Rc;
use swofty_core::store::ProfileDeletion;
use swofty_i18n::{tr, trf};

fn deletion_summary(plan: &ProfileDeletion, p: Palette) -> impl IntoElement {
    let name = &plan.name;
    let removed = if plan.saves {
        tr("Its choice of addons, their settings, its build cache and its saves are deleted.")
    } else {
        tr("Its choice of addons, their settings and its build cache are deleted.")
    };
    v_flex()
        .gap(px(12.))
        .child(div().text_size(px(13.)).text_color(p.text).child(trf!(
            "{removed} Installed addons stay on the Addons page, and other profiles don't change.",
            removed = removed
        )))
        .when_some(plan.switch_to.clone(), |d, to| {
            d.child(
                h_flex()
                    .gap(px(10.))
                    .items_start()
                    .p(px(12.))
                    .rounded(px(6.))
                    .bg(p.elevated)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        Icon::new(IconName::ArrowRightLeft)
                            .size(px(15.))
                            .text_color(p.accent),
                    )
                    .child(
                        v_flex()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(p.text)
                                    .child(trf!("{name} is the active profile", name = name)),
                            )
                            .child(div().text_size(px(13.)).text_color(p.text_muted).child(trf!(
                                "{to} becomes active first, and Play uses it from now on.",
                                to = to
                            ))),
                    ),
            )
        })
        .child(
            div()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(tr("This can't be undone.")),
        )
}

impl Shell {
    pub(crate) fn profile_prompt(
        &mut self,
        from: Option<String>,
        rename: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if rename && self.game_busy() {
            let name = from.clone().unwrap_or_default();
            window.open_alert_dialog(cx, move |alert, _, _| {
                alert
                    .title(trf!("Can't rename {name}", name = name))
                    .description(tr("Close F1 25 before renaming a profile.").to_owned())
            });
            return;
        }
        let (title, ok, initial) = match (&from, rename) {
            (Some(f), true) => (tr("Rename profile"), tr("Rename"), f.clone()),
            (Some(f), false) => (tr("Duplicate profile"), tr("Duplicate"), format!("{f}-copy")),
            (None, _) => (tr("New profile"), tr("Create"), String::new()),
        };
        self.prompt(
            Prompt {
                title,
                description: tr("Letters, digits, - and _. Each profile keeps its own addons and saves.")
                    .into(),
                initial,
                placeholder: "my-profile",
                ok,
                browse: None,
                action: Rc::new(move |store, name| {
                    let result = match (&from, rename) {
                        (Some(f), true) => store.rename_profile(f, name),
                        (f, _) => store.create_profile(name, f.as_deref()),
                    };
                    result.map_err(|e| e.to_string())?;
                    Ok(trf!("Saved profile {name}", name = name))
                }),
            },
            window,
            cx,
        );
    }

    pub(crate) fn game_busy(&self) -> bool {
        !self.processes.is_empty() || self.launch.is_some()
    }

    pub(crate) fn confirm_delete_profile(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let name = name.to_owned();
        let plan = if self.game_busy() {
            Err(tr("Close F1 25 before deleting a profile.").to_owned())
        } else {
            self.store.plan_profile_deletion(&name).map_err(|e| e.to_string())
        };
        let plan = match plan {
            Ok(plan) => plan,
            Err(reason) => {
                window.open_alert_dialog(cx, move |alert, _, _| {
                    alert
                        .title(trf!("Can't delete {name}", name = name))
                        .description(reason.clone())
                });
                return;
            }
        };
        let p = palette(cx);
        let shell = cx.entity().downgrade();
        let store = self.store.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shell, store, name) = (shell.clone(), store.clone(), name.clone());
            alert
                .width(px(480.))
                .title(trf!("Delete profile {name}?", name = name))
                .child(deletion_summary(&plan, p))
                .ok_text(tr("Delete profile"))
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let busy = shell.upgrade().is_some_and(|s| s.read(cx).game_busy());
                    let result = if busy {
                        Err(tr("Close F1 25 before deleting a profile.").to_owned())
                    } else {
                        store.delete_profile(&name).map_err(|e| e.to_string())
                    };
                    match result {
                        Ok(done) => {
                            let text = match &done.switch_to {
                                Some(to) => trf!(
                                    "Deleted profile {name}. {to} is now active.",
                                    name = name,
                                    to = to
                                ),
                                None => trf!("Deleted profile {name}", name = name),
                            };
                            window.notify(Notification::success(text), cx);
                        }
                        Err(e) => {
                            crate::app_log::action("delete profile").failed(&e);
                            window.notify(Notification::error(e), cx)
                        }
                    }
                    shell.update(cx, |s, cx| s.reload(cx)).ok();
                    true
                })
        });
    }
}
