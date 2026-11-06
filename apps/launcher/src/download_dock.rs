//! The floating download dock: when it shows, and how far it has slid in.

use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    Window, div, px, relative,
};

use crate::shell::{Page, Shell};
use crate::theme::Palette;

pub(crate) const LINGER: Duration = Duration::from_secs(4);
pub(crate) const SLIDE: Duration = Duration::from_millis(260);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Input {
    pub(crate) pending: usize,
    pub(crate) failed: usize,
    pub(crate) on_downloads: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    /// 0 is fully shown, 1 is fully tucked away.
    pub(crate) out: f32,
    pub(crate) animating: bool,
    pub(crate) wake: Option<Duration>,
}

#[derive(Debug, Default)]
pub(crate) struct Dock {
    shown: bool,
    moved: Option<Instant>,
    idle_since: Option<Instant>,
    had_work: bool,
    dismissed: usize,
}

impl Dock {
    pub(crate) fn failure_shown(&self, failed: usize) -> bool {
        failed > self.dismissed
    }

    pub(crate) fn dismiss(&mut self, failed: usize) {
        self.dismissed = failed;
    }

    fn wants_shown(&mut self, now: Instant, input: Input) -> bool {
        if input.on_downloads {
            self.dismissed = input.failed;
            self.idle_since = None;
            self.had_work = false;
            return false;
        }
        self.dismissed = self.dismissed.min(input.failed);
        if input.pending > 0 {
            self.idle_since = None;
            self.had_work = true;
            return true;
        }
        if self.had_work && self.idle_since.is_none() {
            self.idle_since = Some(now);
        }
        let lingering = self
            .idle_since
            .is_some_and(|since| now.saturating_duration_since(since) < LINGER);
        if !lingering {
            self.had_work = false;
            self.idle_since = None;
        }
        lingering || input.failed > self.dismissed
    }

    pub(crate) fn frame(&mut self, now: Instant, input: Input) -> Option<Frame> {
        let target = self.wants_shown(now, input);
        if target != self.shown {
            self.shown = target;
            self.moved = Some(now);
        }
        let progress = self.moved.map_or(1., |at| {
            (now.saturating_duration_since(at).as_secs_f32() / SLIDE.as_secs_f32()).clamp(0., 1.)
        });
        let eased = 1. - (1. - progress).powi(3);
        let out = if self.shown { 1. - eased } else { eased };
        if !self.shown && progress >= 1. {
            return None;
        }
        let wake = self
            .idle_since
            .map(|since| LINGER.saturating_sub(now.saturating_duration_since(since)))
            .filter(|_| self.shown && input.pending == 0);
        Some(Frame {
            out,
            animating: progress < 1.,
            wake,
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Line {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) fraction: Option<f32>,
    pub(crate) more: usize,
    pub(crate) failed: bool,
}

const WIDTH: f32 = 300.;
const RISE: f32 = 24.;

impl Shell {
    pub(crate) fn download_dock(
        &mut self,
        p: Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let now = Instant::now();
        let input = self.dock_input();
        let frame = self.dock.frame(now, input)?;
        if frame.animating {
            window.request_animation_frame();
        }
        match frame.wake {
            Some(wake) if !self.dock_wake => {
                self.dock_wake = true;
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(wake + Duration::from_millis(20))
                        .await;
                    this.update(cx, |this, cx| {
                        this.dock_wake = false;
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            _ => {}
        }
        let line = self.dock_line()?;
        let base = 84. + if self.progress.is_some() { 80. } else { 0. };
        let tint = if line.failed { p.warning } else { p.accent };
        let icon = if line.failed {
            IconName::CircleAlert
        } else if line.fraction == Some(1.) && input.pending == 0 {
            IconName::Check
        } else {
            IconName::Download
        };
        let failed = input.failed;
        Some(
            h_flex()
                .id("download-dock")
                .absolute()
                .right(px(16.))
                .bottom(px(base - frame.out * RISE))
                .opacity(1. - frame.out)
                .w(px(WIDTH))
                .p(px(12.))
                .gap(px(10.))
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(if line.failed { p.warning } else { p.border })
                .bg(p.elevated)
                .cursor_pointer()
                .hover(|d| d.bg(p.surface))
                .on_click(cx.listener(|this, _, _, cx| this.open_page(Page::Downloads, cx)))
                .child(Icon::new(icon).size(px(16.)).text_color(tint))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(4.))
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(line.name),
                                )
                                .children((line.more > 0).then(|| {
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(12.))
                                        .text_color(p.text_muted)
                                        .child(format!("+{} more", line.more))
                                })),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(px(12.))
                                .text_color(if line.failed { p.warning } else { p.text_muted })
                                .child(line.status),
                        )
                        .children(line.fraction.filter(|_| !line.failed).map(|done| {
                            div()
                                .h(px(4.))
                                .w_full()
                                .rounded_full()
                                .bg(p.border_variant)
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(done.clamp(0., 1.)))
                                        .rounded_full()
                                        .bg(p.accent),
                                )
                        })),
                )
                .children(line.failed.then(|| {
                    div()
                        .id("download-dock-dismiss")
                        .flex_shrink_0()
                        .p(px(4.))
                        .rounded(px(4.))
                        .cursor_pointer()
                        .hover(|d| d.bg(p.border_variant))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.dock.dismiss(failed);
                            cx.notify();
                        }))
                        .child(Icon::new(IconName::X).size(px(14.)).text_color(p.text_muted))
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn busy() -> Input {
        Input {
            pending: 1,
            ..Input::default()
        }
    }

    fn idle() -> Input {
        Input::default()
    }

    #[test]
    fn hidden_until_a_download_starts() {
        let mut dock = Dock::default();
        assert_eq!(dock.frame(Instant::now(), idle()), None);
    }

    #[test]
    fn visible_while_queued_or_active() {
        let mut dock = Dock::default();
        let t = Instant::now();
        assert!(dock.frame(t, busy()).is_some());
        let settled = dock.frame(t + SLIDE + Duration::from_secs(60), busy()).unwrap();
        assert_eq!(settled.out, 0.);
        assert!(!settled.animating);
        assert_eq!(settled.wake, None);
    }

    #[test]
    fn slides_in_from_tucked_away() {
        let mut dock = Dock::default();
        let t = Instant::now();
        assert_eq!(dock.frame(t, busy()).unwrap().out, 1.);
        let mid = dock.frame(t + SLIDE / 2, busy()).unwrap();
        assert!(mid.out > 0. && mid.out < 1. && mid.animating);
    }

    #[test]
    fn lingers_then_slides_out() {
        let mut dock = Dock::default();
        let t = Instant::now();
        dock.frame(t, busy());
        let done = t + Duration::from_secs(10);
        let lingering = dock.frame(done, idle()).unwrap();
        assert_eq!(lingering.wake, Some(LINGER));
        assert!(
            dock.frame(done + LINGER - Duration::from_millis(1), idle())
                .is_some()
        );
        let leaving = dock.frame(done + LINGER, idle()).unwrap();
        assert!(leaving.animating);
        assert_eq!(dock.frame(done + LINGER + SLIDE, idle()), None);
    }

    #[test]
    fn new_work_cancels_the_linger() {
        let mut dock = Dock::default();
        let t = Instant::now();
        dock.frame(t, busy());
        dock.frame(t + Duration::from_secs(10), idle());
        let again = t + Duration::from_secs(12);
        dock.frame(again, busy());
        let frame = dock.frame(again + LINGER * 3, busy()).unwrap();
        assert_eq!(frame.out, 0.);
    }

    #[test]
    fn failure_stays_until_dismissed() {
        let mut dock = Dock::default();
        let t = Instant::now();
        let failed = Input {
            failed: 1,
            ..Input::default()
        };
        dock.frame(t, busy());
        dock.frame(t + Duration::from_secs(1), failed);
        let late = t + Duration::from_secs(600);
        assert_eq!(dock.frame(late, failed).unwrap().out, 0.);
        dock.dismiss(1);
        dock.frame(late + Duration::from_secs(1), failed);
        assert_eq!(dock.frame(late + Duration::from_secs(2), failed), None);
    }

    #[test]
    fn a_new_failure_shows_after_a_dismissal() {
        let mut dock = Dock::default();
        let t = Instant::now();
        dock.dismiss(1);
        let two = Input {
            failed: 2,
            ..Input::default()
        };
        assert!(dock.frame(t, two).is_some());
    }

    #[test]
    fn clearing_failures_resets_the_dismissal() {
        let mut dock = Dock::default();
        let t = Instant::now();
        dock.dismiss(2);
        let two = Input {
            failed: 2,
            ..Input::default()
        };
        assert!(dock.frame(t, two).is_none());
        let one = Input {
            failed: 1,
            ..Input::default()
        };
        assert!(dock.frame(t + Duration::from_secs(1), one).is_none());
        let two = Input {
            failed: 2,
            ..Input::default()
        };
        assert!(dock.frame(t + Duration::from_secs(2), two).is_some());
    }

    #[test]
    fn hidden_on_the_downloads_page_and_failures_count_as_seen() {
        let mut dock = Dock::default();
        let t = Instant::now();
        let failed_here = Input {
            failed: 1,
            on_downloads: true,
            ..Input::default()
        };
        let busy_here = Input {
            pending: 2,
            on_downloads: true,
            ..Input::default()
        };
        dock.frame(t, busy());
        assert!(dock.frame(t + SLIDE * 2, busy_here).unwrap().animating);
        assert_eq!(dock.frame(t + SLIDE * 5, busy_here), None);
        dock.frame(t + SLIDE * 6, failed_here);
        let away = Input {
            failed: 1,
            ..Input::default()
        };
        assert_eq!(dock.frame(t + SLIDE * 8, away), None);
    }
}
