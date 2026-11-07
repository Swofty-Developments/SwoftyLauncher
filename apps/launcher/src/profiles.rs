use crate::library::ProfileItem;
use crate::scale::px;
use crate::shell::{Page, Prompt, Shell};
use crate::theme::{Palette, ScrollArea, palette};
use crate::ui::tokens::space::GUTTER;
use crate::ui::{avatar, label, names};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Disableable, Icon, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::rc::Rc;
use swofty_core::store::ProfileDeletion;
use swofty_i18n::{tr, trf, trn};

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
    pub(crate) fn profiles_page(&mut self, p: Palette, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let subtitle =
            tr("Each profile is its own set of addons. Play starts F1 25 with the active one.").into();
        let actions = Button::new("new-profile")
            .outline()
            .icon(IconName::Plus)
            .label(tr("New profile"))
            .on_click(cx.listener(|this, _, window, cx| this.profile_prompt(None, false, window, cx)));
        let active = self.library.profile.name.clone();
        let deletable = self.library.profiles.iter().filter(|i| !i.vanilla()).count() > 1;
        let mut cards: Vec<AnyElement> = self
            .library
            .profiles
            .iter()
            .map(|item| self.profile_card(item, item.name == active, deletable, p, cx))
            .collect();
        cards.push(
            v_flex()
                .id("new-profile-card")
                .min_h(px(180.))
                .items_center()
                .justify_center()
                .gap(px(4.))
                .rounded(px(6.))
                .border_1()
                .border_dashed()
                .border_color(p.border)
                .cursor_pointer()
                .text_color(p.text_muted)
                .hover(move |s| s.bg(p.surface).text_color(p.text))
                .child(Icon::new(IconName::Plus).size(px(18.)))
                .child(
                    div()
                        .pt(px(6.))
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(tr("New profile")),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.text_placeholder)
                        .child(tr("Starts with SwoftyMod and the Simplified Importer")),
                )
                .on_click(cx.listener(|this, _, window, cx| this.profile_prompt(None, false, window, cx)))
                .into_any_element(),
        );
        v_flex()
            .size_full()
            .child(self.header(Page::Profiles.label(), subtitle, actions, p))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("profiles")
                    .px(px(GUTTER))
                    .pt(px(4.))
                    .pb(px(24.))
                    .child(
                        div()
                            .grid()
                            .grid_cols(if wide { 3 } else { 2 })
                            .gap(px(12.))
                            .children(cards),
                    ),
            )
            .into_any_element()
    }

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
                            window.push_notification(Notification::success(text), cx);
                        }
                        Err(e) => {
                            crate::app_log::action("delete profile").failed(&e);
                            window.push_notification(Notification::error(e), cx)
                        }
                    }
                    shell.update(cx, |s, cx| s.reload(cx)).ok();
                    true
                })
        });
    }

    pub(crate) fn profile_card(
        &self,
        item: &ProfileItem,
        active: bool,
        deletable: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = item.name.clone();
        let vanilla = item.vanilla();
        let contents = profile_contents(item, p);
        let footer = self.profile_footer(item, active, deletable, p, cx);
        v_flex()
            .size_full()
            .p(px(16.))
            .gap(px(14.))
            .rounded(px(6.))
            .border_1()
            .border_color(if active { p.accent } else { p.border })
            .child(
                h_flex()
                    .gap(px(10.))
                    .child(avatar(&name, 32., p))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.text_muted)
                                    .child(if vanilla {
                                        item.summary()
                                    } else {
                                        trf!(
                                            "{addons} on",
                                            addons =
                                                trn(item.addons.len(), "{count} addon", "{count} addons")
                                        )
                                    }),
                            ),
                    )
                    .when(active, |d| {
                        d.child(
                            h_flex()
                                .gap(px(6.))
                                .text_size(px(12.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(p.accent)
                                .child(div().size(px(6.)).rounded_full().bg(p.accent))
                                .child(tr("Active")),
                        )
                    }),
            )
            .child(div().flex_1().child(contents))
            .child(footer)
            .into_any_element()
    }
}

fn profile_contents(item: &ProfileItem, p: Palette) -> Div {
    let vanilla = item.vanilla();
    if vanilla {
        v_flex()
            .gap(px(3.))
            .child(label(tr("CONTENTS"), p))
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(px(19.))
                    .child(tr("No addons")),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(tr("Plays F1 25 exactly as shipped.")),
            )
    } else {
        v_flex()
            .gap(px(12.))
            .child(names(tr("ADDONS"), &item.addons, p))
            .children(
                item.summaries
                    .iter()
                    .map(|s| names(s.label.to_uppercase(), &s.items, p)),
            )
    }
}

impl Shell {
    fn profile_footer(
        &self,
        item: &ProfileItem,
        active: bool,
        deletable: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let name = item.name.clone();
        let vanilla = item.vanilla();
        let button = |id: &str| Button::new(SharedString::from(format!("{id}-{name}")));
        let (rename, duplicate, switch, delete) = (name.clone(), name.clone(), name.clone(), name.clone());
        h_flex()
            .gap(px(2.))
            .pt(px(12.))
            .border_t_1()
            .border_color(p.border_variant)
            .child(if active {
                h_flex()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(
                        Icon::new(IconName::CircleCheck)
                            .size(px(14.))
                            .text_color(p.accent),
                    )
                    .child(tr("Used when you press Play"))
                    .into_any_element()
            } else {
                button("use")
                    .outline()
                    .small()
                    .icon(IconName::ArrowRightLeft)
                    .label(tr("Use this profile"))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.switch_profile(&switch, window, cx)),
                    )
                    .into_any_element()
            })
            .child(div().flex_1())
            .when(!vanilla, |d| {
                d.child(
                    button("rename")
                        .ghost()
                        .small()
                        .icon(IconName::Pencil)
                        .tooltip(tr("Rename"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.profile_prompt(Some(rename.clone()), true, window, cx)
                        })),
                )
                .child(
                    button("duplicate")
                        .ghost()
                        .small()
                        .icon(IconName::Copy)
                        .tooltip(tr("Duplicate"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.profile_prompt(Some(duplicate.clone()), false, window, cx)
                        })),
                )
                .child(
                    button("delete")
                        .ghost()
                        .small()
                        .icon(IconName::Trash)
                        .tooltip(if deletable {
                            tr("Delete")
                        } else {
                            tr("Your only profile can't be deleted")
                        })
                        .disabled(!deletable)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_delete_profile(&delete, window, cx)
                        })),
                )
            })
    }
}
