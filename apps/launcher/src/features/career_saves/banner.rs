use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::saves::vault::{Notice, Vault};
use swofty_i18n::tr;

use super::banner_data::Kind;
use crate::shell::Shell;
use crate::ui::Callout;
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;
use crate::ui::tokens::space::GUTTER;

impl Shell {
    pub(crate) fn saves_banner(&self, on_page: bool, _: Palette, cx: &mut Context<Self>) -> Option<Div> {
        let shown = self.saves.banner.as_ref()?.shown();
        let kind = shown.kind;
        let review = shown.snapshot.clone();
        let restorable = shown.restorable;
        let restore = (
            shown.source.clone(),
            shown.snapshot,
            shown.generation,
            shown.modes,
        );
        let dismiss_source = shown.source;
        let dismiss_sha = shown.sha256;
        let actions = h_flex()
            .gap(px(8.))
            .flex_wrap()
            .when(restorable, |row| {
                row.child(
                    Button::new("saves-banner-restore")
                        .tap_sm()
                        .label(tr("Restore"))
                        .on_click(cx.listener(move |this, _, window, cx| match kind {
                            Kind::Lost => {
                                for mode in &restore.3 {
                                    this.restore_mode(&restore.0, mode, restore.2, window, cx);
                                }
                            }
                            Kind::Dropped | Kind::Stalled => {
                                this.restore_version(&restore.0, &restore.1, window, cx);
                            }
                        })),
                )
            })
            .child(
                Button::new("saves-banner-review")
                    .outline()
                    .tap_sm()
                    .label(match on_page {
                        true => tr("Review the restore"),
                        false => tr("Open Career saves"),
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_saves(Some(review.clone()), cx);
                    })),
            )
            .child(
                Button::new("saves-banner-dismiss")
                    .ghost()
                    .tap_sm()
                    .label(tr("Dismiss"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let root = this.store.root.clone();
                        if kind == Kind::Stalled {
                            swofty_core::saves::stall::dismiss(&root, &dismiss_source, &dismiss_sha).ok();
                        }
                        if let Ok(v) = Vault::open(&root, &dismiss_source) {
                            v.dismiss_drop().ok();
                            if kind == Kind::Lost {
                                v.dismiss_loss().ok();
                                Notice::clear(&root);
                            }
                        }
                        super::SavesPage::refresh(this, cx);
                        cx.notify();
                    })),
            );
        Some(
            div()
                .px(px(GUTTER))
                .pb(px(12.))
                .flex_shrink_0()
                .child(Callout::warning(shown.text).actions(actions)),
        )
    }
}
