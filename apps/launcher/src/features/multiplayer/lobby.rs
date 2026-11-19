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
use crate::shell::{Page, Shell};
use crate::ui::badge;
use crate::ui::theme::{Palette, ScrollArea};
use crate::ui::tokens::space::GUTTER;

impl Shell {
    pub(super) fn room_lobby(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let subtitle =
            "Race with players who use the same addons. The host picks tracks in F1 25's LAN lobby."
                .to_owned();
        let actions = h_flex()
            .gap(px(14.))
            .items_center()
            .child(self.game_status(p))
            .child(
                Button::new("mp-create-open")
                    .outline()
                    .icon(IconName::Plus)
                    .label(tr("Create room"))
                    .disabled(self.rooms.form)
                    .on_click(cx.listener(|this, _, window, cx| this.room_open_form(window, cx))),
            );
        let rooms: AnyElement = if let Some(why) = &self.rooms.refused {
            div()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(refusal_text(why))
                .into_any_element()
        } else if !self.rooms.lobby.rooms_loaded && self.rooms.lobby.rooms_error.is_none() {
            div()
                .text_size(px(13.))
                .text_color(p.text_muted)
                .child(tr("Loading rooms\u{2026}"))
                .into_any_element()
        } else if self.rooms.lobby.rooms.is_empty() {
            let body = match &self.rooms.lobby.rooms_error {
                Some(_) => tr("The room list didn't load. It tries again in a few seconds."),
                None => tr("Create one, or join a private room with its invite code."),
            };
            v_flex()
                .py(px(40.))
                .items_center()
                .gap(px(8.))
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .child(badge(IconName::Users, 40., p))
                .child(
                    div()
                        .pt(px(6.))
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(tr("No open rooms")),
                )
                .child(div().text_size(px(13.)).text_color(p.text_muted).child(body))
                .into_any_element()
        } else {
            let list = self.rooms.lobby.rooms.clone();
            v_flex()
                .rounded(px(6.))
                .border_1()
                .border_color(p.border)
                .overflow_hidden()
                .children(
                    list.iter()
                        .enumerate()
                        .map(|(i, r)| self.room_row(r, i == 0, p, cx)),
                )
                .into_any_element()
        };
        v_flex()
            .size_full()
            .child(self.header(Page::Multiplayer.label(), subtitle, actions, p))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .scroll_y("mp-lobby")
                    .px(px(GUTTER))
                    .pt(px(4.))
                    .pb(px(24.))
                    .gap(px(24.))
                    .when(self.rooms.form, |d| d.child(self.create_form(p, cx)))
                    .child(
                        v_flex()
                            .gap(px(8.))
                            .child(
                                self.section_head(
                                    if self.rooms.lobby.rooms_loaded {
                                        trf!(
                                            "OPEN ROOMS \u{00b7} {count}",
                                            count = self.rooms.lobby.rooms.len()
                                        )
                                    } else {
                                        tr("OPEN ROOMS").to_owned()
                                    },
                                    p,
                                )
                                .child(div().flex_1())
                                .child(self.code_join(p, cx)),
                            )
                            .when_some(self.rooms.notice.clone(), |d, n| {
                                d.child(div().text_size(px(13.)).text_color(p.text_muted).child(n))
                            })
                            .when_some(self.rooms.error.clone(), |d, e| {
                                d.child(div().text_size(px(13.)).text_color(p.error).child(e))
                            })
                            .child(rooms),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn code_join(&self, _p: Palette, cx: &mut Context<Self>) -> Div {
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
                Button::new("mp-join-code")
                    .outline()
                    .tap_sm()
                    .label(tr("Join with code"))
                    .loading(joining && typed)
                    .disabled(!typed || self.rooms.busy())
                    .on_click(cx.listener(|this, _, window, cx| this.room_join_code(window, cx))),
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
            .py(px(14.))
            .gap(px(14.))
            .items_center()
            .when(!first, |d| d.border_t_1().border_color(p.border_variant))
            .child(self.face(&room.host.name, room.host.avatar.as_deref(), 36., p))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(room.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text_muted)
                            .truncate()
                            .child(room_meta(room)),
                    )
                    .when(!names.is_empty(), |d| {
                        d.child(
                            h_flex()
                                .gap(px(6.))
                                .items_start()
                                .text_size(px(12.))
                                .text_color(p.text_placeholder)
                                .child(
                                    Icon::new(IconName::Package)
                                        .size(px(13.))
                                        .mt(px(1.))
                                        .text_color(p.text_placeholder),
                                )
                                .child(div().flex_1().min_w_0().child(names.join(", "))),
                        )
                    }),
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
        let players = Button::new("mp-max")
            .outline()
            .tap_sm()
            .w(px(160.))
            .label(trn(usize::from(m.max), "{count} player", "{count} players"))
            .dropdown_caret(true)
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for n in MIN_PLAYERS..=MAX_PLAYERS {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(trn(usize::from(n), "{count} player", "{count} players"))
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
