//! Asking before a save when Driver Manager's Game drivers sets values the draft changes.

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::{Icon, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::dbedit::driver_manager::{self, Override, Part};
use swofty_i18n::{tr, trf};

use crate::features::db::{Db, Then};
use crate::shell::Shell;
use crate::ui::theme::palette;

/// Drivers the popup names; the rest are counted.
const SHOWN: usize = 6;

/// `Head`, `Helmet` or `Head and helmet`.
pub(crate) fn parts_text(parts: &[Part]) -> &'static str {
    match (parts.contains(&Part::Head), parts.contains(&Part::Helmet)) {
        (true, true) => tr("Head and helmet"),
        (false, true) => tr("Helmet"),
        _ => tr("Head"),
    }
}

/// What the popup says above the drivers, naming the active profile.
pub(crate) fn overrides_text(profile: &str) -> String {
    trf!(
        "Driver Manager is on in {profile}. When you play, its Game drivers changes replace these values from this addon:",
        profile = profile
    )
}

impl Shell {
    /// The draft's values Driver Manager's Game drivers overrides in the active profile.
    fn db_overrides(&self) -> Vec<Override> {
        let Some(catalog) = self.db.catalog() else {
            return Vec::new();
        };
        driver_manager::overrides(
            &self.store,
            &self.library.profile,
            catalog,
            &self.db.book.draft.edits,
        )
    }

    /// Save or Save as Addon, first asking when Driver Manager overrides some of the draft's values.
    pub(crate) fn db_save_checked(
        &mut self,
        then: Option<Then>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let overrides = self.db_overrides();
        if overrides.is_empty() {
            self.db_save_now(then, window, cx);
            return;
        }
        let text = overrides_text(&self.library.profile.name);
        let shell = cx.entity().downgrade();
        let p = palette(cx);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (s1, s2, then) = (shell.clone(), shell.clone(), then.clone());
            let shown = overrides.len().min(SHOWN);
            let drivers = v_flex()
                .gap(px(4.))
                .p(px(10.))
                .rounded(px(6.))
                .bg(p.elevated)
                .border_1()
                .border_color(p.border)
                .text_size(px(13.))
                .children(overrides[..shown].iter().map(|o| {
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(
                            Icon::new(IconName::UserRound)
                                .size(px(14.))
                                .flex_shrink_0()
                                .text_color(p.warning),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(p.text)
                                .child(o.name.clone()),
                        )
                        .child(div().text_color(p.text_muted).child(parts_text(&o.parts)))
                }))
                .when(overrides.len() > shown, |d| {
                    d.child(
                        div()
                            .pl(px(22.))
                            .text_color(p.text_muted)
                            .child(trf!("and {count} more", count = overrides.len() - shown)),
                    )
                });
            let footer = DialogFooter::new()
                .child(
                    Button::new("db-dm-cancel")
                        .outline()
                        .label(tr("Cancel"))
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    Button::new("db-dm-open")
                        .outline()
                        .label(tr("Open Driver Manager"))
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            s1.update(cx, |s, cx| s.db_follow(&driver_manager::link(), window, cx))
                                .ok();
                        }),
                )
                .child(
                    Button::new("db-dm-save")
                        .primary()
                        .label(tr("Save anyway"))
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            s2.update(cx, |s, cx| s.db_save_now(then.clone(), window, cx))
                                .ok();
                        }),
                );
            alert
                .width(px(500.))
                .title(tr("Driver Manager overrides these changes"))
                .child(
                    v_flex()
                        .gap(px(12.))
                        .child(div().text_size(px(13.)).text_color(p.text).child(text.clone()))
                        .child(drivers)
                        .child(div().text_size(px(13.)).text_color(p.text_muted).child(tr(
                            "To use this addon's values, open Driver Manager, go to Game drivers and press \
                             Use the stock head and helmet on the driver.",
                        ))),
                )
                .footer(footer)
        });
    }

    fn db_save_now(&mut self, then: Option<Then>, window: &mut Window, cx: &mut Context<Self>) {
        if self.db.book.draft.addon.is_some() {
            Db::save_bound(self, then, window, cx);
        } else {
            self.db_save_as(then, window, cx);
        }
    }
}
