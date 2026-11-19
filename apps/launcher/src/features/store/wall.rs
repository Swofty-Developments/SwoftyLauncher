use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::online::server::support::{BIO_MAX, Supporter};
use swofty_i18n::tr;

use super::StorePage;
use super::parts::*;
use crate::features::time_trial::Load;
use crate::shell::Shell;
use crate::ui::scale::px;
use crate::ui::theme::Palette;

const SUPPORTER_MIN: f32 = 280.;
const GAP: f32 = 14.;

fn empty_note(text: String, p: Palette) -> AnyElement {
    div()
        .w_full()
        .py(px(32.))
        .flex()
        .justify_center()
        .rounded(px(14.))
        .border_1()
        .border_color(p.border_variant)
        .bg(p.surface)
        .text_size(px(14.))
        .text_color(p.text_muted)
        .child(text)
        .into_any_element()
}

impl Shell {
    pub(super) fn store_supporters(
        &self,
        bio: Entity<InputState>,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let count = match &self.store_page.supporters {
            Load::Ready(s) if !s.is_empty() => Some(s.len()),
            _ => None,
        };
        let wall = match &self.store_page.supporters {
            Load::Idle | Load::Loading => empty_note(tr("Loading supporters…").to_owned(), p),
            Load::Failed(e) => empty_note(e.clone(), p),
            Load::Ready(s) if s.is_empty() => {
                empty_note(tr("No supporters yet. Be the first.").to_owned(), p)
            }
            Load::Ready(s) => {
                let cols = ((width + GAP) / (SUPPORTER_MIN + GAP)).floor().max(1.) as u16;
                div()
                    .w_full()
                    .grid()
                    .grid_cols(cols)
                    .gap(px(GAP))
                    .children(s.iter().enumerate().map(|(i, x)| self.supporter_card(i, x, p)))
                    .into_any_element()
            }
        };
        v_flex()
            .gap(px(18.))
            .child(section_head(
                IconName::Users,
                tr("Supporters"),
                tr("Thank you to everyone who keeps the launcher free"),
                count,
                p,
            ))
            .children(self.store_listing(bio, p, cx))
            .child(wall)
            .into_any_element()
    }

    fn supporter_card(&self, i: usize, s: &Supporter, p: Palette) -> AnyElement {
        let named = s.display.name.as_deref().filter(|_| !s.display.anonymous);
        let face = match named {
            Some(n) => self.face(n, s.display.avatar_url.as_deref(), 52., p),
            None => div()
                .size(px(52.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(p.elevated)
                .child(Icon::new(IconName::User).size(px(24.)).text_color(p.text_muted)),
        };
        let plus = s.plus();
        let face = face.when(plus, |d| d.border_2().border_color(rgb(PURPLE)));
        let bio = s.bio.clone().filter(|b| !b.trim().is_empty());
        h_flex()
            .id(SharedString::from(format!("supporter-{i}")))
            .min_w_0()
            .gap(px(14.))
            .p(px(16.))
            .rounded(px(14.))
            .border_1()
            .map(|d| match plus {
                true => d.border_color(hue(PURPLE).opacity(0.45)).bg(linear_gradient(
                    135.,
                    linear_color_stop(tinted(p, PURPLE, 0.16), 0.),
                    linear_color_stop(p.surface, 1.),
                )),
                false => d.border_color(p.border_variant).bg(p.surface),
            })
            .child(face)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(5.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(if plus { PLUS_NAME } else { PINK }))
                                    .child(named.map_or_else(|| tr("Anonymous").to_owned(), str::to_owned)),
                            )
                            .child(badge(SharedString::from(format!("supporter-glint-{i}")), plus)),
                    )
                    .children(bio.map(|b| {
                        div()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .text_color(p.text_muted)
                            .line_clamp(2)
                            .child(b)
                    })),
            )
            .into_any_element()
    }

    /// The signed-in supporter's own line on the wall and how they are shown there.
    fn store_listing(
        &self,
        bio: Entity<InputState>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Load::Ready(me) = &self.store_page.me else {
            return None;
        };
        if !me.active {
            return None;
        }
        let typed = bio.read(cx).value().to_string();
        let count = typed.chars().count();
        let mode = self.store_page.mode.clone();
        let busy = self.store_page.busy;
        let choice = |id: &'static str, key: &'static str, label: &'static str, cx: &mut Context<Self>| {
            Button::new(id)
                .small()
                .when_else(mode == key, |b| b.primary(), |b| b.outline())
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store_page.mode = key.to_owned();
                    cx.notify();
                }))
        };
        Some(
            v_flex()
                .gap(px(12.))
                .p(px(20.))
                .rounded(px(14.))
                .border_1()
                .border_color(hue(PINK).opacity(0.3))
                .bg(tinted(p, PINK, 0.04))
                .child(
                    v_flex()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(16.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(tr("Your listing")),
                        )
                        .child(muted(tr("Your line next to your name in the supporter list"), p)),
                )
                .child(Input::new(&bio))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(muted(tr("Show me as"), p))
                        .child(choice("mode-discord", "discord", tr("Discord name & avatar"), cx))
                        .child(choice("mode-anon", "anonymous", tr("Anonymous"), cx))
                        .child(div().flex_1())
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(if count > BIO_MAX { p.error } else { p.text_muted })
                                .child(format!("{count} / {BIO_MAX}")),
                        )
                        .child(
                            Button::new("store-save")
                                .small()
                                .primary()
                                .label(tr("Save"))
                                .disabled(busy || count > BIO_MAX)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    StorePage::save(this, typed.clone(), cx)
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
