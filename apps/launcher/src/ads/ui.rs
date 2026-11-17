//! Where the shell shows its ad slots: a rail ad under the news page's release
//! notes, and a floating video while the game builds and starts.
//! `SWOFTY_ADS_VIDEO=1` shows the video without pressing Play.

use super::Unit;
use crate::scale::{Hit, px};
use crate::shell::{PlayState, Shell};
use crate::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, WindowExt, h_flex, v_flex};
use gpui_kit::*;
use std::time::{Duration, Instant};
use swofty_i18n::tr;

/// How long the video plays before it can be closed.
const VIDEO_SKIP: Duration = Duration::from_secs(5);

/// The video for one press of Play.
pub(crate) struct AdVideo {
    since: Instant,
    closed: bool,
}

fn label(p: Palette) -> Div {
    div()
        .text_size(px(11.))
        .text_color(p.text_placeholder)
        .child(tr("Advertisement"))
}

impl Shell {
    /// Whether an overlay could be over a slot: slots draw over everything in
    /// the window, so they hide while one is open.
    pub(crate) fn ads_covered(&self, window: &mut Window, cx: &mut App) -> bool {
        self.popover.is_some()
            || self.tasks.is_some()
            || (self.crash_open && self.crash_view.is_some())
            || self.ban.is_some()
            || self.moving.is_some()
            || self.found_data.is_some()
            || self.dropping
            || window.has_active_dialog(cx)
            || window.has_active_sheet(cx)
    }

    /// The width, in layout units, the news page's side column needs for the rail ad.
    pub(crate) fn ad_rail_width(&self) -> Option<f32> {
        self.ads
            .on()
            .then(|| f32::from(Unit::Rail.size().width) / crate::scale::current() + 2.)
    }

    /// Steps aside while the video is up, which floats over the page.
    pub(crate) fn ad_rail(&self, p: Palette) -> Option<AnyElement> {
        if self.ad_video.as_ref().is_some_and(|v| !v.closed) {
            return None;
        }
        let slot = self.ads.slot(Unit::Rail, p.surface)?;
        Some(
            v_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.))
                .child(label(p))
                .child(slot)
                .into_any_element(),
        )
    }

    /// The video from pressing Play until the game is running.
    pub(crate) fn ad_video(
        &mut self,
        state: &PlayState,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let starting = matches!(
            state,
            PlayState::Preparing | PlayState::Building | PlayState::Launching
        ) || std::env::var_os("SWOFTY_ADS_VIDEO").is_some_and(|v| v == "1");
        if !self.ads.on() || !starting {
            self.ad_video = None;
            return None;
        }
        let video = self.ad_video.get_or_insert_with(|| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(VIDEO_SKIP).await;
                this.update(cx, |_, cx| cx.notify()).ok();
            })
            .detach();
            AdVideo {
                since: Instant::now(),
                closed: false,
            }
        });
        if video.closed {
            return None;
        }
        let closable = video.since.elapsed() >= VIDEO_SKIP;
        let slot = self.ads.slot(Unit::Video, p.surface)?;
        let close = Button::new("ad-video-close")
            .ghost()
            .tap_sm()
            .icon(IconName::X)
            .disabled(!closable)
            .on_click(cx.listener(|this, _, _, cx| {
                if let Some(video) = &mut this.ad_video {
                    video.closed = true;
                }
                cx.notify();
            }));
        Some(
            v_flex()
                .absolute()
                .right(px(16.))
                .bottom(px(crate::play_bar::HEIGHT + 16.))
                .p(px(8.))
                .gap(px(6.))
                .rounded(px(10.))
                .border_1()
                .border_color(p.border)
                .bg(p.surface)
                .shadow_lg()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(label(p))
                        .child(close),
                )
                .child(slot)
                .into_any_element(),
        )
    }
}
