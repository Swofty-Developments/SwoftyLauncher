//! The play bar's left side: the copy Play uses, its store, build and last session.

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{Disableable, Icon, Side, h_flex, v_flex};
use gpui_kit::*;
use swofty_i18n::tr;

use super::store_badge;
use crate::shell::{PlayState, Shell};
use crate::ui::theme::Palette;
use swofty_core::install::Source;

impl Shell {
    pub(super) fn game_info(&self, p: Palette, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let width = if wide { 320. } else { 240. };
        let text = |title: String, line: AnyElement| {
            v_flex()
                .flex_1()
                .min_w_0()
                .items_start()
                .gap(px(1.))
                .child(
                    div()
                        .w_full()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .truncate()
                        .child(title),
                )
                .child(line)
        };
        let muted = |s: String| {
            div()
                .min_w_0()
                .text_size(px(12.))
                .text_color(p.text_placeholder)
                .truncate()
                .child(s)
                .into_any_element()
        };
        let Some(i) = &self.game.install else {
            let (title, line) = if self.game.detected {
                (tr("F1 25 not found"), tr("Not in Steam or the EA app"))
            } else {
                ("F1 25", tr("Looking for the game…"))
            };
            return v_flex()
                .w(px(width))
                .flex_shrink_1()
                .min_w(px(170.))
                .child(text(title.into(), muted(line.into())))
                .into_any_element();
        };
        let title = match i.version() {
            Some(v) => format!("F1 25 · {v}"),
            None => "F1 25".into(),
        };
        let mode = match i.source {
            Source::EaApp if !cfg!(windows) => "Proton".to_owned(),
            _ => i.mode.clone(),
        };
        let line = h_flex()
            .w_full()
            .min_w_0()
            .gap(px(6.))
            .child(store_badge(i.source, p))
            .child(muted(mode))
            .into_any_element();
        let shell = cx.entity().downgrade();
        let copies = self.game.copies.clone();
        let missing = self.game.missing_copies.clone();
        let active = i.dir.clone();
        Button::new("game-switcher")
            .ghost()
            .h(px(48.))
            .w(px(width))
            .flex_shrink_1()
            .min_w(px(170.))
            .px(px(8.))
            .ml(px(-8.))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap(px(8.))
                    .child(text(title, line))
                    .child(
                        Icon::new(IconName::ChevronsUpDown)
                            .size(px(14.))
                            .text_color(p.text_muted)
                            .flex_shrink_0(),
                    ),
            )
            .tooltip(tr("Switch the copy of F1 25 Play uses"))
            .disabled(matches!(
                self.play_state(),
                PlayState::Running
                    | PlayState::Preparing
                    | PlayState::Launching
                    | PlayState::Restoring
                    | PlayState::Building
            ))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                menu = menu
                    .min_w(px(360.))
                    .max_w(px(560.))
                    .check_side(Side::Right)
                    .label(tr("Play this copy"));
                for c in &copies {
                    let (shell, dir) = (shell.clone(), c.dir.clone());
                    let (source, version) = (
                        c.source,
                        c.version().unwrap_or_else(|| tr("version unknown").into()),
                    );
                    let folder = c.dir.display().to_string();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            v_flex()
                                .w_full()
                                .min_w_0()
                                .gap(px(3.))
                                .py(px(3.))
                                .child(
                                    h_flex()
                                        .gap(px(8.))
                                        .child(store_badge(source, p))
                                        .child(div().text_size(px(13.)).child(version.clone())),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(p.text_muted)
                                        .truncate()
                                        .child(folder.clone()),
                                )
                        })
                        .checked(c.dir == active)
                        .on_click(move |_, window, cx| {
                            shell.update(cx, |s, cx| s.use_copy(dir.clone(), window, cx)).ok();
                        }),
                    );
                }
                for dir in &missing {
                    let folder = dir.display().to_string();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            v_flex()
                                .w_full()
                                .min_w_0()
                                .gap(px(3.))
                                .py(px(3.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(p.text_muted)
                                        .child(tr("Folder not found")),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(p.text_placeholder)
                                        .truncate()
                                        .child(folder.clone()),
                                )
                        })
                        .disabled(true),
                    );
                }
                let shell = shell.clone();
                menu.separator().item(
                    PopupMenuItem::new(tr("Add another folder…"))
                        .icon(IconName::FolderPlus)
                        .on_click(move |_, window, cx| {
                            shell.update(cx, |s, cx| s.locate_game(window, cx)).ok();
                        }),
                )
            })
            .into_any_element()
    }
}
