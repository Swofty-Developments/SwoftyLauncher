use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::vault::careers::State as CareerState;
use swofty_core::vault::restore::Running;
use swofty_core::vault::{self};
use swofty_i18n::{tr, trf};

use super::labels::{careers_held, listed_later, short_block, steam_save_missing_text, version_label};
use super::state::{Entry, Imported, SavesView, Version};
use crate::format::{ago_since as ago, size};
use crate::pages::{GUTTER, label};
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::Palette;

pub(super) const MODE_W: f32 = 124.;
pub(super) const PLAYED_W: f32 = 104.;
pub(super) const CHIP_W: f32 = 112.;
pub(super) const WHEN_W: f32 = 104.;
pub(super) const SIZE_W: f32 = 64.;
pub(super) const ACTIONS_W: f32 = 164.;
pub(super) const PIN_W: f32 = 18.;

impl Shell {
    pub(super) fn careers_block(&self, view: &SavesView, p: Palette, _cx: &mut Context<Self>) -> Div {
        let rows: Vec<AnyElement> = match (&view.imported, view.entries.is_empty()) {
            (Some(i), true) => vec![self.imported_row(i, p)],
            (None, true) => vec![
                div()
                    .px(px(12.))
                    .py(px(14.))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(tr("No careers yet. They appear after you play this profile."))
                    .into_any_element(),
            ],
            _ => view.entries.iter().map(|e| self.career_row(e, p)).collect(),
        };
        v_flex()
            .gap(px(8.))
            .child(label(tr("CAREERS"), p))
            .child(self.rows_box(rows, p))
    }

    pub(super) fn rows_box(&self, rows: Vec<AnyElement>, p: Palette) -> Div {
        v_flex()
            .rounded(px(8.))
            .border_1()
            .border_color(p.border)
            .overflow_hidden()
            .children(rows.into_iter().enumerate().map(|(i, r)| {
                div()
                    .w_full()
                    .when(i > 0, |d| d.border_t_1().border_color(p.border_variant))
                    .child(r)
            }))
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

    pub(super) fn career_row(&self, e: &Entry, p: Palette) -> AnyElement {
        let chip = match e.l.state {
            CareerState::Present => None,
            CareerState::Missing => Some((tr("Missing"), p.warning)),
            CareerState::DeletedInGame => Some((tr("Deleted in game"), p.text_placeholder)),
        };
        let dim = chip.is_some();
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
                    .children(chip.map(|(t, c)| {
                        div()
                            .px(px(6.))
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .rounded(px(4.))
                            .border_1()
                            .border_color(c)
                            .text_size(px(11.))
                            .text_color(c)
                            .whitespace_nowrap()
                            .child(t)
                    })),
            )
            .into_any_element()
    }

    pub(super) fn versions_block(&self, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> Div {
        let rows: Vec<AnyElement> = match view.versions.is_empty() {
            true => vec![
                div()
                    .px(px(12.))
                    .py(px(14.))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(match view.steam_missing {
                        true => steam_save_missing_text(),
                        false => tr("No versions yet."),
                    })
                    .into_any_element(),
            ],
            false => view
                .versions
                .iter()
                .map(|v| self.version_row(v, view, p, cx))
                .collect(),
        };
        v_flex()
            .gap(px(8.))
            .child(label(tr("VERSIONS"), p))
            .child(self.rows_box(rows, p))
    }

    pub(super) fn version_row(
        &self,
        v: &Version,
        view: &SavesView,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let s = &v.snap;
        let id = s.id.clone();
        let pinned = !s.pinned.is_empty();
        let own_pin = s.pinned.iter().any(|x| x == "pinned");
        let confirming = self.saves.confirm.as_deref() == Some(s.id.as_str());
        let layout = view.layout();
        let running = Running {
            game: self.saves.game_running,
            steam: view.steam_running,
            ea_app: false,
        };
        let blocker = vault::restore::blocker(layout, running);
        let why = short_block(layout, running);
        let source = s.source.clone();
        let (restore_id, restore_source) = (id.clone(), source.clone());
        let (pin_id, pin_source) = (id.clone(), source);
        let ask_id = id.clone();
        let full = vault::human(s.at);
        let held = v.held.as_ref().map(|h| careers_held(h));
        v_flex()
            .w_full()
            .child(
                h_flex()
                    .h(px(44.))
                    .px(px(12.))
                    .gap(px(10.))
                    .items_center()
                    .child(div().w(px(PIN_W)).flex_shrink_0().when(pinned, |d| {
                        d.child(Icon::new(IconName::Star).size(px(13.)).text_color(p.accent))
                    }))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(version_label(s)),
                            )
                            .children(held.map(|h| {
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .text_color(p.text_placeholder)
                                    .child(h)
                            })),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("when-{id}")))
                            .w(px(WHEN_W))
                            .flex_shrink_0()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(p.text_muted)
                            .child(ago(s.at as f64))
                            .tooltip(move |window, cx| Tooltip::new(full.clone()).build(window, cx)),
                    )
                    .child(
                        div()
                            .w(px(SIZE_W))
                            .flex_shrink_0()
                            .text_right()
                            .text_size(px(13.))
                            .text_color(p.text_muted)
                            .child(size(s.total())),
                    )
                    .child(
                        h_flex()
                            .w(px(ACTIONS_W))
                            .flex_shrink_0()
                            .gap(px(4.))
                            .justify_end()
                            .child(
                                Button::new(SharedString::from(format!("pin-{id}")))
                                    .ghost()
                                    .small()
                                    .icon(IconName::Star)
                                    .label(if own_pin { tr("Unpin") } else { tr("Pin") })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.pin_version(&pin_source, &pin_id, own_pin, cx)
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("restore-{id}")))
                                    .outline()
                                    .small()
                                    .icon(IconName::Undo2)
                                    .label(tr("Restore"))
                                    .disabled(why.is_some())
                                    .when_some(why, |b, t| b.tooltip(t))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.saves.confirm =
                                            match this.saves.confirm.as_deref() == Some(ask_id.as_str()) {
                                                true => None,
                                                false => Some(ask_id.clone()),
                                            };
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .when(confirming, |d| {
                d.child(div().px(px(12.)).pb(px(12.)).child(self.restore_panel(
                    v,
                    layout,
                    blocker,
                    (restore_id, restore_source),
                    p,
                    cx,
                )))
            })
            .into_any_element()
    }

    pub(super) fn storage_line(&self, view: &SavesView, p: Palette, cx: &mut Context<Self>) -> Div {
        let dir = view.dir.clone();
        h_flex()
            .px(px(GUTTER))
            .py(px(8.))
            .gap(px(4.))
            .flex_shrink_0()
            .items_center()
            .border_t_1()
            .border_color(p.border_variant)
            .text_size(px(13.))
            .text_color(p.text_muted)
            .child(trf!(
                "Versions use {used} of {cap}",
                used = size(view.usage),
                cap = size(view.cap)
            ))
            .child("·")
            .child(
                Button::new("saves-storage-change")
                    .ghost()
                    .small()
                    .label(tr("Change"))
                    .on_click(cx.listener(|this, _, window, cx| this.cap_prompt(window, cx))),
            )
            .child("·")
            .child(
                Button::new("saves-storage-open")
                    .ghost()
                    .small()
                    .label(tr("Open folder"))
                    .on_click(cx.listener(move |_, _, _, cx| crate::reveal::show(&dir, cx))),
            )
    }
}
