//! The play bar of a game the launcher plays through its own services: the copy, what Play will do, and Play.

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Disableable, Icon, Side as MenuSide, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::game::StoreId;
use swofty_i18n::{tr, trf, trn};

use super::{HEIGHT, badge, launch_failed, link, status_row};
use crate::shell::Shell;
use crate::shell::games::{Side, SideState, store_label};
use crate::ui::theme::Palette;

fn store_colour(store: Option<StoreId>, p: Palette) -> Hsla {
    match store {
        Some(StoreId::Steam { .. }) => p.accent,
        Some(StoreId::Epic { .. }) => p.text,
        Some(StoreId::EaApp { .. }) => p.warning,
        None => p.text_muted,
    }
}

impl Shell {
    pub(super) fn side_play_bar(
        &self,
        side: &Side,
        p: Palette,
        wide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .h(px(HEIGHT))
            .flex_shrink_0()
            .bg(p.chrome)
            .border_t_1()
            .border_color(p.border_variant)
            .child(self.game_picker(p, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .pl(px(16.))
                    .pr(px(16.))
                    .gap(px(20.))
                    .child(self.side_info(side, p, wide, cx))
                    .child(self.side_status(side, p))
                    .child(self.footer_links(p, cx))
                    .child(self.side_button(side, p, cx)),
            )
            .into_any_element()
    }

    fn side_info(&self, side: &Side, p: Palette, wide: bool, cx: &mut Context<Self>) -> AnyElement {
        let width = if wide { 320. } else { 240. };
        let name = side.def.info.name;
        let muted = |s: String| {
            div()
                .min_w_0()
                .text_size(px(12.))
                .text_color(p.text_placeholder)
                .truncate()
                .child(s)
                .into_any_element()
        };
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
        let Some(copy) = side.copy() else {
            let (title, line) = match side.detected {
                true => (
                    trf!("{game} not found", game = name),
                    tr("Not in Steam or the Epic Games Launcher").to_owned(),
                ),
                false => (name.to_owned(), tr("Looking for the game…").to_owned()),
            };
            return v_flex()
                .w(px(width))
                .flex_shrink_1()
                .min_w(px(170.))
                .child(text(title, muted(line)))
                .into_any_element();
        };
        let title = match copy.build_id {
            Some(build) => trf!("{game} · build {build}", game = name, build = build),
            None => name.to_owned(),
        };
        let line = h_flex()
            .w_full()
            .min_w_0()
            .gap(px(6.))
            .child(badge(store_label(copy.store), store_colour(copy.store, p)))
            .child(muted(copy.dir.display().to_string()))
            .into_any_element();
        let shell = cx.entity().downgrade();
        let copies = side.copies.clone();
        let active = copy.dir.clone();
        Button::new("side-copy-switcher")
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
            .tooltip(trf!("Switch the copy of {game} Play uses", game = name))
            .disabled(side.run.is_some() || side.starting)
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |mut menu, _, _| {
                menu = menu
                    .min_w(px(360.))
                    .max_w(px(560.))
                    .check_side(MenuSide::Right)
                    .label(tr("Play this copy"));
                for c in &copies {
                    let (shell, dir) = (shell.clone(), c.dir.clone());
                    let store = c.store;
                    let build = c.build_id.map_or_else(
                        || tr("build unknown").to_owned(),
                        |b| trf!("build {build}", build = b),
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
                                        .child(badge(store_label(store), store_colour(store, p)))
                                        .child(div().text_size(px(13.)).child(build.clone())),
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
                        .on_click(move |_, _, cx| {
                            shell.update(cx, |s, cx| s.pick_side_copy(dir.clone(), cx)).ok();
                        }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    fn side_status(&self, side: &Side, p: Palette) -> AnyElement {
        let dot = |c: Hsla| div().size(px(7.)).rounded_full().bg(c).flex_shrink_0();
        let row = status_row();
        let addons = trn(side.on(), "{count} addon on", "{count} addons on");
        let line = |row: Div, colour: Hsla, text: String| {
            row.text_color(p.text_muted)
                .child(dot(colour))
                .child(div().min_w_0().truncate().child(text))
                .into_any_element()
        };
        match side.state() {
            SideState::Running => {
                let pid = side.run.as_ref().map_or(0, |r| r.started.game_pid());
                line(
                    row,
                    p.success,
                    trf!("Running · {addons} · pid {pid}", addons = addons, pid = pid),
                )
            }
            SideState::Stopping => line(row, p.warning, tr("Closing the game…").to_owned()),
            SideState::Starting => line(row, p.accent, tr("Starting with the Swofty Core…").to_owned()),
            _ if side.notice.is_some() => {
                launch_failed(row, side.notice.as_deref().unwrap_or_default(), p).into_any_element()
            }
            SideState::NoCore => row
                .text_color(p.warning)
                .child(Icon::new(IconName::TriangleAlert).size(px(16.)))
                .child(div().min_w_0().truncate().child(tr(
                    "The Swofty Core for this game is missing; reinstall the launcher",
                )))
                .into_any_element(),
            SideState::Ready => {
                let core = side
                    .core
                    .as_deref()
                    .map(|d| swofty_core::games::overcooked_2::core_dir::CoreDir(d.to_path_buf()).versions())
                    .and_then(|v| v.core);
                let text = match (&side.ended, core) {
                    (Some(e), _) if !e.failed.is_empty() => trf!(
                        "{addons} · {count} failed to load last time",
                        addons = addons,
                        count = e.failed.len()
                    ),
                    (_, Some(v)) => trf!("{addons} · Swofty Core {version}", addons = addons, version = v),
                    (_, None) => addons,
                };
                let log = side
                    .ended
                    .as_ref()
                    .map(|e| {
                        e.session
                            .join(swofty_core::games::overcooked_2::session::LOG_FILE)
                    })
                    .filter(|l| l.is_file());
                let row = row
                    .text_color(p.text_muted)
                    .child(dot(p.success))
                    .child(div().min_w_0().truncate().child(text));
                match log {
                    Some(log) => row
                        .child(
                            link("side-core-log", tr("Core log"), p)
                                .on_click(move |_, _, cx| cx.open_with_system(&log)),
                        )
                        .into_any_element(),
                    None => row.into_any_element(),
                }
            }
            SideState::Detecting | SideState::NotFound => row.into_any_element(),
        }
    }

    fn side_button(&self, side: &Side, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let state = side.state();
        let (icon, label, bg, fg, enabled) = match state {
            SideState::Ready => (IconName::Play, tr("Play"), p.accent, p.on_accent, true),
            SideState::Running => (IconName::Square, tr("Stop"), p.elevated, p.error, true),
            SideState::Starting => (
                IconName::LoaderCircle,
                tr("Launching…"),
                p.elevated,
                p.accent,
                false,
            ),
            SideState::Stopping => (
                IconName::LoaderCircle,
                tr("Stopping…"),
                p.elevated,
                p.accent,
                false,
            ),
            SideState::Detecting => (
                IconName::LoaderCircle,
                tr("Play"),
                p.elevated,
                p.text_muted,
                false,
            ),
            SideState::NotFound | SideState::NoCore => {
                (IconName::Play, tr("Play"), p.elevated, p.text_muted, false)
            }
        };
        let ready = state == SideState::Ready;
        let running = state == SideState::Running;
        div()
            .id("play")
            .h(px(52.))
            .w(px(170.))
            .flex_shrink_0()
            .rounded(px(9.))
            .bg(bg)
            .text_color(fg)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .when(!ready, |d| d.border_1().border_color(p.border))
            .when(ready, |d| d.hover(move |s| s.bg(p.accent_hover)))
            .when(running, |d| d.hover(move |s| s.bg(p.selected)))
            .when(enabled, |d| d.cursor_pointer())
            .map(|d| match icon {
                IconName::LoaderCircle => d.child(Spinner::new().icon(icon).with_size(px(18.))),
                _ => d.child(Icon::new(icon).size(px(18.))),
            })
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label),
            )
            .when(enabled, |d| {
                d.on_click(cx.listener(move |this, _, window, cx| {
                    if running {
                        this.stop_side(cx);
                    } else {
                        this.play_side(window, cx);
                    }
                }))
            })
            .into_any_element()
    }
}
