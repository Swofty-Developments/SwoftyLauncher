use crate::ui::scale::{Hit, px};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::online::multiplayer::types::RoomSummary;
use swofty_i18n::{tr, trf, trn};

use super::model::*;
use super::state::*;
use crate::shell::Shell;
use crate::ui::badge;
use crate::ui::theme::Palette;

impl Shell {
    /// The Multiplayer page's main column: the open rooms with the create form and joining by code.
    pub(crate) fn room_column(&mut self, width: f32, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let lobby = &self.rooms.lobby;
        let title = if lobby.rooms_loaded {
            trf!("OPEN ROOMS \u{00b7} {count}", count = lobby.rooms.len())
        } else {
            tr("OPEN ROOMS").to_owned()
        };
        let head = self
            .section_head(title, p)
            .flex_wrap()
            .child(div().flex_1())
            .when(width >= 900., |d| d.child(self.game_status(p)))
            .child(self.code_join("mp-join-code", false, p, cx))
            .child(
                Button::new("mp-create-open")
                    .outline()
                    .tap_sm()
                    .icon(IconName::Plus)
                    .label(tr("Create room"))
                    .disabled(self.rooms.form || self.rooms.busy())
                    .on_click(cx.listener(|this, _, window, cx| this.room_open_form(window, cx))),
            );
        v_flex()
            .gap(px(8.))
            .children(self.room_strip(p, cx))
            .child(head)
            .when_some(self.rooms.notice.clone(), |d, n| {
                d.child(div().text_size(px(13.)).text_color(p.text_muted).child(n))
            })
            .when_some(self.rooms.error.clone(), |d, e| {
                d.child(div().text_size(px(13.)).text_color(p.error).child(e))
            })
            .when(self.rooms.form, |d| d.child(self.create_form(p, cx)))
            .child(self.room_list(p, cx))
            .into_any_element()
    }

    /// The room this launcher is in, while its player is on the list of rooms or on Time Trial.
    pub(crate) fn room_strip(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.rooms.state.as_ref().filter(|_| self.rooms.in_room())?;
        let name = state
            .room
            .as_ref()
            .map_or_else(|| tr("Room").to_owned(), |r| r.name.clone());
        let status = room_status(state);
        Some(
            h_flex()
                .px(px(16.))
                .py(px(12.))
                .gap(px(14.))
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(p.accent)
                .bg(p.selected)
                .child(Icon::new(IconName::Users).size(px(18.)).text_color(p.accent))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .truncate()
                                .child(trf!("You're in {room}", room = name)),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(p.text_muted)
                                .truncate()
                                .child(status.progress.label.clone()),
                        ),
                )
                .child(
                    Button::new("mp-back-to-room")
                        .primary()
                        .tap_sm()
                        .label(tr("Open room"))
                        .on_click(cx.listener(|this, _, _, cx| this.room_step_in(cx))),
                )
                .into_any_element(),
        )
    }

    fn room_list(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let lobby = &self.rooms.lobby;
        if let Some(why) = &self.rooms.refused {
            return div()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(refusal_text(why))
                .into_any_element();
        }
        if !lobby.rooms_loaded && lobby.rooms_error.is_none() {
            return v_flex()
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .children((0..2).map(|i| {
                    h_flex()
                        .h(px(64.))
                        .px(px(16.))
                        .gap(px(14.))
                        .items_center()
                        .when(i > 0, |d| d.border_t_1().border_color(p.border_variant))
                        .child(div().size(px(32.)).rounded_full().bg(p.elevated))
                        .child(div().h(px(14.)).w(px(220.)).rounded(px(4.)).bg(p.elevated))
                        .when(i == 0, |d| {
                            d.child(div().flex_1()).child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(p.text_muted)
                                    .child(tr("Loading rooms\u{2026}")),
                            )
                        })
                }))
                .into_any_element();
        }
        if lobby.rooms.is_empty() {
            let body = match &lobby.rooms_error {
                Some(_) => tr("The room list didn't load. It tries again in a few seconds."),
                None => tr("Create one, or join a private room with its invite code."),
            };
            return h_flex()
                .px(px(16.))
                .py(px(16.))
                .gap(px(14.))
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .child(badge(IconName::Users, 36., p))
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tr("No open rooms")),
                        )
                        .child(div().text_size(px(13.)).text_color(p.text_muted).child(body)),
                )
                .into_any_element();
        }
        v_flex()
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
            .overflow_hidden()
            .children(
                lobby
                    .rooms
                    .iter()
                    .enumerate()
                    .map(|(i, r)| self.room_row(r, i == 0, p, cx)),
            )
            .into_any_element()
    }

    /// The invite code box and its join button. From the rooms card, `card` joins on the Multiplayer page.
    pub(super) fn code_join(&self, id: &'static str, card: bool, _p: Palette, cx: &mut Context<Self>) -> Div {
        let typed = !self.rooms.code.read(cx).value().trim().is_empty();
        let joining = self
            .rooms
            .starting
            .as_ref()
            .is_some_and(|s| s.as_str() != "create");
        h_flex()
            .gap(px(8.))
            .child(
                div()
                    .w(px(170.))
                    .child(Input::new(&self.rooms.code).tap_sm().disabled(self.rooms.busy())),
            )
            .child(
                Button::new(id)
                    .outline()
                    .tap_sm()
                    .label(tr("Join with code"))
                    .loading(joining && typed)
                    .disabled(!typed || self.rooms.busy())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if card {
                            this.room_join_code_from_card(window, cx);
                        } else {
                            this.room_join_code(window, cx);
                        }
                    })),
            )
    }

    pub(super) fn room_row(
        &self,
        room: &RoomSummary,
        first: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status = RoomStatus::of(room, &self.room_enabled_addons(), self.rooms.lobby.print.as_ref());
        let code = room.code.clone();
        let names = card_addons(room);
        h_flex()
            .px(px(16.))
            .py(px(10.))
            .gap(px(14.))
            .items_center()
            .when(!first, |d| d.border_t_1().border_color(p.border_variant))
            .child(self.face(&room.host.name, room.host.avatar.as_deref(), 32., p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(room.name.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .child(div().flex_shrink_0().child(room_meta(room)))
                            .when(!names.is_empty(), |d| {
                                d.child(
                                    Icon::new(IconName::Package)
                                        .size(px(13.))
                                        .ml(px(6.))
                                        .text_color(p.text_placeholder),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(p.text_placeholder)
                                        .child(names.join(", ")),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(13.))
                    .text_color(if status == RoomStatus::Differ {
                        p.warning
                    } else {
                        p.text_muted
                    })
                    .child(status.text()),
            )
            .child(
                Button::new(SharedString::from(format!("mp-view-{}", room.code)))
                    .tap_sm()
                    .outline()
                    .label("View")
                    .disabled(self.rooms.busy())
                    .on_click(cx.listener(move |this, _, _, cx| this.room_open(code.clone(), cx))),
            )
            .into_any_element()
    }

    pub(super) fn create_form(&self, p: Palette, cx: &mut Context<Self>) -> Div {
        let m = &self.rooms;
        let creating = m.starting.as_deref() == Some("create");
        let shell = cx.entity().downgrade();
        let chosen = m.max;
        let players = Button::new("mp-max")
            .outline()
            .tap_sm()
            .w(px(160.))
            .label(trn(usize::from(m.max), "{count} player", "{count} players"))
            .dropdown_caret(true)
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                menu = menu.max_h(px(320.)).scrollable(true).min_w(px(160.));
                for n in room_sizes() {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(trn(usize::from(n), "{count} player", "{count} players"))
                            .checked(n == chosen)
                            .on_click(move |_, _, cx| {
                                shell
                                    .update(cx, |s, cx| {
                                        s.rooms.max = n;
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    );
                }
                menu
            });
        let public = h_flex()
            .gap(px(10.))
            .child(
                Switch::new("mp-public")
                    .checked(m.public)
                    .tap_sm()
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.rooms.public = *on;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(if m.public {
                        tr("Listed: anyone can see it and join")
                    } else {
                        tr("Private: only people with the code can join")
                    }),
            );
        let row = |title: &'static str, control: AnyElement, first: bool| {
            h_flex()
                .px(px(16.))
                .py(px(10.))
                .gap(px(12.))
                .when(!first, |d| d.border_t_1().border_color(p.border_variant))
                .child(
                    div()
                        .w(px(120.))
                        .flex_shrink_0()
                        .text_size(px(13.))
                        .text_color(p.text_muted)
                        .child(title),
                )
                .child(h_flex().flex_1().min_w_0().child(control))
        };
        v_flex()
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
            .child(
                v_flex()
                    .px(px(16.))
                    .pt(px(14.))
                    .pb(px(12.))
                    .gap(px(3.))
                    .border_b_1()
                    .border_color(p.border_variant)
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr("New room")),
                    )
                    .child(div().text_size(px(13.)).text_color(p.text_muted).child(tr(
                        "F1 25 opens its LAN lobby, where you pick the tracks and settings.",
                    ))),
            )
            .child(row(
                "Name",
                div()
                    .w(px(320.))
                    .child(Input::new(&m.name).tap_sm())
                    .into_any_element(),
                true,
            ))
            .child(row("Visibility", public.into_any_element(), false))
            .child(row("Players", players.into_any_element(), false))
            .child(
                h_flex()
                    .px(px(16.))
                    .py(px(12.))
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(p.border_variant)
                    .bg(p.surface)
                    .child(div().flex_1())
                    .child(
                        Button::new("mp-create-cancel")
                            .ghost()
                            .tap_sm()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rooms.form = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mp-create")
                            .primary()
                            .tap_sm()
                            .label(tr("Create room"))
                            .loading(creating)
                            .disabled(m.busy())
                            .on_click(cx.listener(|this, _, window, cx| this.room_create(window, cx))),
                    ),
            )
    }
}
