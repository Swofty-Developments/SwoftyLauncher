//! The category tiles above the Addon Browser: All and one tile per category.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::tr;

use super::*;
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::Palette;
use crate::ui::tile;

const TILE_W: f32 = 122.;
const TILE_H: f32 = 70.;
const TILE_GAP: f32 = 8.;
const GLYPH_BOX: f32 = 24.;
const GLYPH: f32 = 15.;

impl Shell {
    pub(super) fn category_tiles(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let counts = self
            .browser
            .grid_cache
            .as_ref()
            .map(|cache| cache.categories.clone())
            .unwrap_or_default();
        let tiles: Vec<AnyElement> = [None]
            .into_iter()
            .chain(shelves())
            .enumerate()
            .map(|(i, tab)| {
                let count = counts.get(i).copied().unwrap_or(0);
                self.category_tile(tab, count, p, cx)
            })
            .collect();
        h_flex().gap(px(TILE_GAP)).flex_wrap().children(tiles)
    }

    fn category_tile(&self, tab: Tab, count: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let on = self.browser.tab == tab;
        let tint = shelf_tint(tab, p);
        let empty = count == 0 && tab.is_some();
        let glyph_color = if empty { p.text_placeholder } else { tint };
        v_flex()
            .id(SharedString::from(format!("browser-tab-{tab:?}")))
            .focusable()
            .w(px(TILE_W))
            .h(px(TILE_H))
            .flex_none()
            .p(px(8.))
            .justify_between()
            .rounded(px(10.))
            .border_1()
            .cursor_pointer()
            .map(|d| {
                if on {
                    d.bg(p.surface.blend(tint.opacity(0.18)))
                        .border_color(tint.opacity(0.8))
                } else {
                    d.bg(p.surface)
                        .border_color(p.border)
                        .hover(move |s| s.bg(p.hover).border_color(tint.opacity(0.5)))
                }
            })
            .focus_visible(move |s| s.border_color(p.accent))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        tile(GLYPH_BOX, p)
                            .rounded(px(7.))
                            .bg(tint.opacity(if empty { 0.05 } else { 0.14 }))
                            .border_color(tint.opacity(if empty { 0.12 } else { 0.3 }))
                            .child(shelf_glyph(tab, GLYPH, glyph_color)),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(if on { p.text_muted } else { p.text_placeholder })
                            .child(count.to_string()),
                    ),
            )
            .child(
                div()
                    .truncate()
                    .text_size(px(13.))
                    .font_weight(if on {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .text_color(if on || !empty { p.text } else { p.text_muted })
                    .child(tr(tab_label(tab))),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.shelve(tab, cx)))
            .into_any_element()
    }
}
