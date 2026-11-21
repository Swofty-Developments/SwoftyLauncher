use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::saves::vault::careers::{State as CareerState, mode_label};
use swofty_i18n::{tr, trf};

use super::labels::{close_game_first, listed_later};
use super::plain::{PlainCareer, PlainMode};
use super::state::{Entry, Imported, SavesView};
use crate::shell::Shell;
use crate::ui::format::ago_since as ago;
use crate::ui::label;
use crate::ui::scale::{Hit, px};
use crate::ui::theme::Palette;

use super::list::{list_box, placeholder};
use crate::ui::Pill;
use crate::ui::tokens::Tone;
pub(super) const MODE_W: f32 = 124.;
pub(super) const PLAYED_W: f32 = 104.;
pub(super) const CHIP_W: f32 = 112.;
pub(super) const FORGET_W: f32 = 96.;

impl Shell {
    pub(super) fn careers_block(&self, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> Div {
        let plain = view.plain.as_ref().filter(|v| !v.careers.is_empty());
        let rows: Vec<AnyElement> = match (plain, &view.imported, view.entries.is_empty()) {
            (Some(v), _, _) => v
                .modes
                .iter()
                .filter(|m| v.careers.iter().any(|c| c.mode == m.mode))
                .flat_map(|m| {
                    let header = self.mode_header(&v.source, m, p, cx);
                    let rows = v
                        .careers
                        .iter()
                        .filter(|c| c.mode == m.mode)
                        .map(|c| self.plain_row(c, p));
                    std::iter::once(header).chain(rows).collect::<Vec<_>>()
                })
                .collect(),
            (None, Some(i), true) => vec![self.imported_row(i, p)],
            (None, None, true) => vec![placeholder(
                tr("No careers yet. They appear after you play this profile."),
                p,
            )],
            _ => {
                let forget = view.entries.iter().any(|e| e.l.state != CareerState::Present);
                view.entries
                    .iter()
                    .map(|e| self.career_row(e, forget, p, cx))
                    .collect()
            }
        };
        let notes: Vec<Div> = plain
            .into_iter()
            .flat_map(|v| {
                v.answer_line()
                    .into_iter()
                    .chain(v.waiting_line().map(str::to_owned))
            })
            .map(|t| div().text_size(px(12.)).text_color(p.text_muted).child(t))
            .collect();
        v_flex()
            .gap(px(8.))
            .child(label(tr("CAREERS"), p))
            .child(list_box(rows, p))
            .children(notes)
    }

    fn mode_header(&self, source: &str, m: &PlainMode, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let chip = m
            .waiting
            .then(|| Pill::new(tr("Restore waits for Play")).tone(Tone::Warning));
        let restore = (!m.older.is_empty()).then(|| {
            let shell = cx.entity().downgrade();
            let older = m.older.clone();
            let (source, mode) = (source.to_owned(), m.mode.clone());
            Button::new(SharedString::from(format!("plain-restore-{mode}")))
                .ghost()
                .tap_sm()
                .label(tr("Restore"))
                .dropdown_caret(true)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                    for o in &older {
                        let (shell, source, mode, generation) =
                            (shell.clone(), source.clone(), mode.clone(), o.generation);
                        menu = menu.item(
                            PopupMenuItem::new(trf!("From {when}", when = ago(o.at as f64))).on_click(
                                move |_, window, cx| {
                                    shell
                                        .update(cx, |s, cx| {
                                            s.restore_mode(&source, &mode, generation, window, cx)
                                        })
                                        .ok();
                                },
                            ),
                        );
                    }
                    menu
                })
        });
        h_flex()
            .min_h(px(40.))
            .px(px(12.))
            .py(px(6.))
            .gap(px(10.))
            .items_center()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.text_muted)
                    .child(mode_label(Some(&m.mode))),
            )
            .children(chip)
            .children(restore)
            .into_any_element()
    }

    pub(super) fn plain_row(&self, c: &PlainCareer, p: Palette) -> AnyElement {
        h_flex()
            .min_h(px(52.))
            .px(px(12.))
            .py(px(8.))
            .gap(px(10.))
            .items_center()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(c.name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(match &c.place {
                                Some(pl) => format!("{pl} · {}", c.lives()),
                                None => c.lives(),
                            }),
                    ),
            )
            .child(
                div()
                    .w(px(PLAYED_W))
                    .flex_shrink_0()
                    .truncate()
                    .text_right()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(ago(c.last_played as f64)),
            )
            .into_any_element()
    }

    pub(super) fn imported_row(&self, i: &Imported, p: Palette) -> AnyElement {
        h_flex()
            .min_h(px(52.))
            .px(px(12.))
            .py(px(8.))
            .gap(px(10.))
            .items_center()
            .child(
                div().w(px(MODE_W)).flex_shrink_0().child(
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .child(Icon::new(IconName::Save).size(px(15.)))
                        .child(tr("Saved game")),
                ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(i.title.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(listed_later()),
                    ),
            )
            .child(
                div()
                    .w(px(PLAYED_W + CHIP_W))
                    .flex_shrink_0()
                    .truncate()
                    .text_right()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .children(i.at.map(|at| trf!("imported {when}", when = ago(at as f64)))),
            )
            .into_any_element()
    }

    pub(super) fn career_row(
        &self,
        e: &Entry,
        forget: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chip = match e.l.state {
            CareerState::Present => None,
            CareerState::Missing => Some(Pill::new(tr("Missing")).tone(Tone::Warning)),
            CareerState::DeletedInGame => Some(Pill::new(tr("Deleted in game"))),
        };
        let dim = chip.is_some();
        let blocked = self.saves.game_running;
        let career = e.l.career.clone();
        let button = dim.then(|| {
            Button::new(SharedString::from(format!("forget-{career}")))
                .ghost()
                .tap_sm()
                .label(tr("Forget"))
                .disabled(blocked)
                .tooltip(match blocked {
                    true => close_game_first(),
                    false => tr("Take this career off the list. No save is deleted, and it shows again if the game finds it."),
                })
                .on_click(cx.listener(move |this, _, window, cx| this.forget_career(&career, window, cx)))
        });
        h_flex()
            .min_h(px(52.))
            .px(px(12.))
            .py(px(8.))
            .gap(px(10.))
            .items_center()
            .child(
                div()
                    .w(px(MODE_W))
                    .flex_shrink_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(e.mode()),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .when(dim, |d| d.text_color(p.text_muted))
                            .child(e.l.title()),
                    )
                    .children(e.place.clone().map(|pl| {
                        div()
                            .truncate()
                            .text_size(px(12.))
                            .text_color(p.text_placeholder)
                            .child(pl)
                    })),
            )
            .child(
                div()
                    .w(px(PLAYED_W))
                    .flex_shrink_0()
                    .truncate()
                    .text_right()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .children(e.l.last_played.map(|at| ago(at as f64))),
            )
            .child(
                h_flex()
                    .w(px(CHIP_W))
                    .flex_shrink_0()
                    .justify_end()
                    .children(chip),
            )
            .when(forget, |row| {
                row.child(
                    h_flex()
                        .w(px(FORGET_W))
                        .flex_shrink_0()
                        .justify_end()
                        .children(button),
                )
            })
            .into_any_element()
    }
}
