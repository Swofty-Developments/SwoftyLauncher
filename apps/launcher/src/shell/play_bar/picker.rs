//! The bottom left of the play bar: the art of the game shown, which opens the game picker.

use std::sync::Arc;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{Icon, Side, h_flex, v_flex};
use gpui_kit::*;
use swofty_core::game::GameId;
use swofty_i18n::{tr, trn};

use crate::shell::Shell;
use crate::ui::theme::Palette;

/// The art for `game`: F1 25's own, Steam's library art for another game when Steam has it, else a tile.
pub(crate) fn art(game: GameId, image: Option<Arc<Image>>, size: f32, p: Palette) -> Div {
    match (game, image) {
        (GameId::F1_25, _) => crate::ui::game_art(size, size),
        (_, Some(image)) => div()
            .w(px(size))
            .h(px(size))
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(px(size * 0.21))
            .child(img(image).size_full().object_fit(ObjectFit::Cover)),
        (_, None) => div()
            .w(px(size))
            .h(px(size))
            .flex_shrink_0()
            .rounded(px(size * 0.21))
            .bg(p.elevated)
            .border_1()
            .border_color(p.border)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::new(IconName::Gamepad2)
                    .size(px(size * 0.45))
                    .text_color(p.text_muted),
            ),
    }
}

/// A game in the picker: its id, its art, whether it was found (`None` while looking) and the line saying so.
type Entry = (GameId, Option<Arc<Image>>, Option<bool>, String);

impl Shell {
    fn art_of(&self, game: GameId) -> Option<Arc<Image>> {
        self.games.art.get(&game).cloned().flatten()
    }

    /// Whether `game` was found on this computer, as the picker says it: `None` while still looking.
    fn found_line(&self, game: GameId) -> (Option<bool>, String) {
        let (detected, copies) = match self.games.side_of(game) {
            Some(side) => (side.detected, side.copies.len()),
            None => (self.game.detected, self.game.copies.len()),
        };
        match (detected, copies) {
            (false, _) => (None, tr("Looking for the game…").to_owned()),
            (true, 0) => (Some(false), tr("Not found").to_owned()),
            (true, n) => (Some(true), trn(n, "Found", "Found · {count} copies")),
        }
    }

    pub(super) fn game_picker(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.games.active;
        let shell = cx.entity().downgrade();
        let entries: Vec<Entry> = self
            .games
            .offered()
            .into_iter()
            .map(|g| {
                let (found, line) = self.found_line(g);
                (g, self.art_of(g), found, line)
            })
            .collect();
        Button::new("game-picker")
            .ghost()
            .w(px(64.))
            .h(px(64.))
            .m(px(4.))
            .p(px(4.))
            .rounded(px(14.))
            .child(
                div()
                    .relative()
                    .child(art(active, self.art_of(active), 56., p))
                    .child(
                        div()
                            .absolute()
                            .right(px(-3.))
                            .bottom(px(-3.))
                            .size(px(20.))
                            .rounded_full()
                            .bg(p.chrome)
                            .border_1()
                            .border_color(p.border)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::ChevronsUpDown)
                                    .size(px(12.))
                                    .text_color(p.text_muted),
                            ),
                    ),
            )
            .tooltip(tr("Pick the game"))
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                menu = menu.min_w(px(300.)).check_side(Side::Right).label(tr("Game"));
                for (game, image, found, line) in &entries {
                    let (game, image, found, line) = (*game, image.clone(), *found, line.clone());
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            let colour = match found {
                                Some(true) => p.success,
                                Some(false) => p.text_placeholder,
                                None => p.text_muted,
                            };
                            h_flex()
                                .w_full()
                                .gap(px(12.))
                                .py(px(4.))
                                .child(art(game, image.clone(), 40., p))
                                .child(
                                    v_flex()
                                        .min_w_0()
                                        .gap(px(2.))
                                        .child(
                                            div()
                                                .text_size(px(14.))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(game.info().name),
                                        )
                                        .child(
                                            h_flex()
                                                .gap(px(6.))
                                                .text_size(px(12.))
                                                .text_color(colour)
                                                .child(div().size(px(6.)).rounded_full().bg(colour))
                                                .child(line.clone()),
                                        ),
                                )
                        })
                        .checked(game == active)
                        .on_click(move |_, window, cx| {
                            shell.update(cx, |s, cx| s.pick_game(game, window, cx)).ok();
                        }),
                    );
                }
                menu
            })
            .into_any_element()
    }
}
