//! Play, its menu, and the button for a room's lobby.

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, Side, Sizable, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trf};

use crate::features::multiplayer::PlayWord;
use crate::shell::{Page, PlayState, Shell};
use crate::ui::theme::Palette;
use swofty_core::install::Source;
use swofty_core::session::LaunchMode;
use swofty_core::updates::updater::Gate;

impl Shell {
    pub(super) fn play_caret(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let shell = cx.entity().downgrade();
        let vr = self.launch_mode().is_vr();
        let start_store = self.prefs.start_store;
        let start_label = match self.game.install.as_ref().map(|i| i.source) {
            Some(Source::EaApp) => tr("Start the EA app automatically"),
            _ => tr("Start Steam automatically"),
        };
        Button::new("play-menu")
            .h(px(52.))
            .w(px(34.))
            .rounded_l(px(0.))
            .rounded_r(px(9.))
            .primary()
            .child(Icon::new(IconName::ChevronDown).size(px(16.)))
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, cx| {
                let dev_mode = shell
                    .upgrade()
                    .is_some_and(|s| s.read(cx).store.config().developer);
                let mode = |label: &'static str, icon: IconName, mode: LaunchMode| {
                    let shell = shell.clone();
                    PopupMenuItem::new(label)
                        .icon(icon)
                        .checked(vr == mode.is_vr())
                        .on_click(move |_, window, cx| {
                            shell
                                .update(cx, |s, cx| {
                                    s.set_launch_mode(mode, window, cx);
                                    s.play(window, cx);
                                })
                                .ok();
                        })
                };
                let (console, steam, developer) = (shell.clone(), shell.clone(), shell.clone());
                let (saves, once) = (shell.clone(), shell.clone());
                let profile = shell.upgrade().map(|s| {
                    let p = &s.read(cx).library.profile;
                    (p.is_vanilla(), p.launcher_saves)
                });
                let (vanilla, saves_on) = profile.unwrap_or((true, false));
                menu.min_w(px(220.))
                    .check_side(Side::Right)
                    .item(mode(tr("Play"), IconName::Play, LaunchMode::Normal))
                    .item(mode(tr("Play in VR"), IconName::RectangleGoggles, LaunchMode::Vr))
                    .separator()
                    .item(
                        PopupMenuItem::new(tr("Play with console"))
                            .icon(IconName::SquareTerminal)
                            .on_click(move |_, window, cx| {
                                console
                                    .update(cx, |s, cx| {
                                        s.set_console(true, window, cx);
                                        s.play(window, cx);
                                    })
                                    .ok();
                            }),
                    )
                    .separator()
                    .when(!vanilla, |menu| {
                        menu.item(
                            PopupMenuItem::new(tr("Use launcher saves"))
                                .icon(IconName::Save)
                                .checked(saves_on)
                                .on_click(move |_, window, cx| {
                                    saves
                                        .update(cx, |s, cx| s.set_launcher_saves(!saves_on, window, cx))
                                        .ok();
                                }),
                        )
                        .when(saves_on, |menu| {
                            menu.item(
                                PopupMenuItem::new(tr("Play once with your Steam saves"))
                                    .icon(IconName::Play)
                                    .on_click(move |_, window, cx| {
                                        once.update(cx, |s, cx| {
                                            crate::shell::Launching::play_with_steam_saves(s, window, cx)
                                        })
                                        .ok();
                                    }),
                            )
                        })
                    })
                    .separator()
                    .item(
                        PopupMenuItem::new(start_label)
                            .icon(IconName::CirclePower)
                            .checked(start_store)
                            .on_click(move |_, window, cx| {
                                steam
                                    .update(cx, |s, cx| s.set_start_store(!start_store, window, cx))
                                    .ok();
                            }),
                    )
                    .item(
                        PopupMenuItem::new(tr("Developer mode"))
                            .icon(IconName::SquareTerminal)
                            .checked(dev_mode)
                            .on_click(move |_, window, cx| {
                                developer
                                    .update(cx, |s, cx| {
                                        s.set_developer(!dev_mode, window, cx);
                                        if !dev_mode {
                                            s.go(Page::Tools, cx);
                                        }
                                    })
                                    .ok();
                            }),
                    )
            })
    }

    pub(super) fn big_button(&self, state: &PlayState, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let gate = self.update_gate(state);
        if gate != Gate::Play {
            return self.update_button(gate, p, cx);
        }
        if self.addon_update_takes_play(state) {
            return self.addon_update_button(p, cx);
        }
        let vr = self.launch_mode().is_vr();
        let starting = self.room_play_word();
        let (icon, label, bg, fg, enabled) = match state {
            PlayState::Ready | PlayState::Crashed(_) if starting == Some(PlayWord::Needed) => (
                IconName::CircleAlert,
                PlayWord::Needed.label(),
                p.elevated,
                p.warning,
                true,
            ),
            PlayState::Ready | PlayState::Crashed(_) if starting.is_some() => (
                IconName::LoaderCircle,
                starting.map(PlayWord::label).unwrap_or_default(),
                p.elevated,
                p.accent,
                false,
            ),
            PlayState::Ready | PlayState::Crashed(_) if vr => (
                IconName::RectangleGoggles,
                tr("Play in VR"),
                p.accent,
                p.on_accent,
                true,
            ),
            PlayState::Ready | PlayState::Crashed(_) => {
                (IconName::Play, tr("Play"), p.accent, p.on_accent, true)
            }
            PlayState::Preparing | PlayState::Launching => (
                IconName::LoaderCircle,
                tr("Launching…"),
                p.elevated,
                p.accent,
                false,
            ),
            PlayState::Restoring => (
                IconName::LoaderCircle,
                tr("Restoring…"),
                p.elevated,
                p.accent,
                false,
            ),
            PlayState::Building if self.launching.cancelling => (
                IconName::LoaderCircle,
                tr("Cancelling…"),
                p.elevated,
                p.accent,
                false,
            ),
            PlayState::Building => (
                IconName::LoaderCircle,
                tr("Building…"),
                p.elevated,
                p.accent,
                true,
            ),
            PlayState::Running => (IconName::Square, tr("Stop"), p.elevated, p.error, true),
            PlayState::NotInstalled => (
                IconName::FolderSearch,
                tr("Locate game"),
                p.elevated,
                p.text,
                true,
            ),
            PlayState::Detecting | PlayState::Checking => (
                IconName::LoaderCircle,
                tr("Play"),
                p.elevated,
                p.text_muted,
                false,
            ),
            PlayState::Elsewhere(_) => (IconName::Play, tr("Play"), p.elevated, p.text_muted, false),
        };
        let ready = matches!(state, PlayState::Ready | PlayState::Crashed(_)) && starting.is_none();
        let needed =
            matches!(state, PlayState::Ready | PlayState::Crashed(_)) && starting == Some(PlayWord::Needed);
        let missing = matches!(state, PlayState::NotInstalled);
        let running = matches!(state, PlayState::Running);
        let building = matches!(state, PlayState::Building);
        let button = div()
            .id("play")
            .h(px(52.))
            .w(px(170.))
            .flex_shrink_0()
            .when(ready, |d| d.rounded_l(px(9.)))
            .when(!ready, |d| d.rounded(px(9.)))
            .bg(bg)
            .text_color(fg)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .when(!ready, |d| d.border_1().border_color(p.border))
            .when(ready, |d| d.hover(move |s| s.bg(p.accent_hover)))
            .when(missing, |d| d.hover(move |s| s.bg(p.selected)))
            .when(enabled, |d| d.cursor_pointer())
            .when(building && enabled, |d| d.hover(move |s| s.bg(p.selected)))
            .when(needed, |d| d.hover(move |s| s.bg(p.selected)))
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
                        this.confirm_stop(window, cx);
                    } else if needed {
                        this.room_step_in(cx);
                    } else if building {
                        this.cancel_build(window, cx);
                    } else if missing {
                        this.locate_game(window, cx);
                    } else {
                        this.play(window, cx);
                    }
                }))
            });
        h_flex()
            .flex_shrink_0()
            .child(button)
            .when(ready, |d| d.child(self.play_caret(cx)))
            .into_any_element()
    }

    /// "Play <host>'s Lobby" while a room is open on the Online page. `primary` gives it the Play button's
    /// filled look, for when it stands in for Play.
    pub(super) fn room_button(
        &self,
        state: &PlayState,
        primary: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let ready = matches!(
            state,
            PlayState::Ready | PlayState::Running | PlayState::Crashed(_)
        );
        if !ready || !self.room_can_play() {
            return None;
        }
        let host = self.room_viewed()?.host.name.clone();
        Some(
            div()
                .id("play-room")
                .h(px(52.))
                .px(px(18.))
                .flex_shrink_0()
                .rounded(px(9.))
                .when(primary, |d| {
                    d.min_w(px(170.))
                        .bg(p.accent)
                        .text_color(p.on_accent)
                        .hover(move |s| s.bg(p.accent_hover))
                })
                .when(!primary, |d| {
                    d.border_1()
                        .border_color(p.accent)
                        .text_color(p.accent)
                        .hover(move |s| s.bg(p.selected))
                })
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .cursor_pointer()
                .child(Icon::new(IconName::Users).size(px(18.)))
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(trf!("Play {host}'s Lobby", host = host)),
                )
                .on_click(cx.listener(|this, _, window, cx| this.room_view_play(window, cx)))
                .into_any_element(),
        )
    }
}
