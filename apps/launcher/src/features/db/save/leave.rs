//! Discarding a draft, or leaving it with unsaved changes.

use crate::ui::scale::px;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::*;
use swofty_i18n::{tr, trf, trn};

use crate::features::db::Db;
use crate::features::db::Then;
use crate::shell::Shell;

impl Shell {
    pub(crate) fn db_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.db.book.draft.unsaved();
        if n == 0 {
            return;
        }
        let shell = cx.entity().downgrade();
        let body = match self.db.book.bound() {
            Some(info) => trf!(
                "This puts {name} back to how it was last saved.",
                name = info.name
            ),
            None => tr("The new addon goes back to no changes.").to_owned(),
        };
        window.open_alert_dialog(cx, move |alert, _, _| {
            let shell = shell.clone();
            alert
                .title(trf!(
                    "Discard {changes}?",
                    changes = trn(n, "{count} unsaved change", "{count} unsaved changes")
                ))
                .description(trf!("{body} This can't be undone.", body = body))
                .ok_text(tr("Discard changes"))
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    shell
                        .update(cx, |s, cx| {
                            s.db.book.draft.discard();
                            s.db.errors.clear();
                            s.db.slots = crate::features::db::Slots::default();
                            Db::changed(s, None, true, cx);
                        })
                        .ok();
                    true
                })
        });
    }

    /// Runs `then`, first asking Save / Discard / Cancel when the draft has unsaved changes.
    pub(crate) fn db_leave(&mut self, then: Then, window: &mut Window, cx: &mut Context<Self>) {
        let n = self.db.book.draft.unsaved();
        if n == 0 {
            self.db_then(then, cx);
            return;
        }
        let name = self.db.book.draft_name();
        let bound = self.db.book.draft.addon.is_some();
        let shell = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (s1, s2, t1, t2) = (shell.clone(), shell.clone(), then.clone(), then.clone());
            let footer = DialogFooter::new()
                .child(
                    Button::new("db-leave-cancel")
                        .outline()
                        .label(tr("Cancel"))
                        .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                    Button::new("db-leave-discard")
                        .outline()
                        .label(tr("Discard"))
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            s1.update(cx, |s, cx| {
                                s.db.book.draft.discard();
                                s.db_then(t1.clone(), cx);
                            })
                            .ok();
                        }),
                )
                .child(
                    Button::new("db-leave-save")
                        .primary()
                        .label(if bound { tr("Save") } else { tr("Save as Addon…") })
                        .on_click(move |_, window, cx| {
                            window.close_dialog(cx);
                            s2.update(cx, |s, cx| s.db_save_checked(Some(t2.clone()), window, cx))
                                .ok();
                        }),
                );
            alert
                .width(px(480.))
                .title(trf!("Save changes to {name}?", name = name))
                .description(trf!(
                    "You have {changes}.",
                    changes = trn(n, "{count} unsaved change", "{count} unsaved changes")
                ))
                .footer(footer)
        });
    }

    pub(crate) fn db_new_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.db_leave(Then::New, window, cx);
    }
}
