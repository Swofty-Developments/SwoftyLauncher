use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{ElementId, Interactivity, RenderOnce, ScrollHandle, StyleRefinement};
use gpui_kit::component::scroll::{Scrollable, ScrollableElement, Scrollbar, ScrollbarHandle};
use gpui_kit::*;

const WHEEL_LINE_HEIGHT: f32 = 24.;
const DECAY_SECONDS: f32 = 0.08;
const SETTLE_DISTANCE: f32 = 0.25;

#[derive(Default)]
struct Motion {
    pending: f32,
    last: Option<Instant>,
    scheduled: bool,
    generation: u64,
}

impl Motion {
    fn cancel(&mut self) {
        self.pending = 0.;
        self.last = None;
        self.scheduled = false;
        self.generation = self.generation.wrapping_add(1);
    }

    fn step(&mut self, now: Instant, reduced: bool) -> f32 {
        let elapsed = self.last.replace(now).map_or(0., |last| {
            now.duration_since(last).as_secs_f32().clamp(0.008, 0.05)
        });
        let step = if reduced || self.pending.abs() < SETTLE_DISTANCE {
            self.pending
        } else {
            self.pending * (1. - (-elapsed / DECAY_SECONDS).exp())
        };
        self.pending -= step;
        step
    }
}

/// Clones share one motion, so a handle kept in a map can be used where the page is drawn.
#[derive(Default, Clone)]
pub(crate) struct SmoothScroll(Rc<RefCell<Motion>>);

impl SmoothScroll {
    pub(crate) fn cancel(&self) {
        self.0.borrow_mut().cancel();
    }

    pub(crate) fn wrap<H: ScrollbarHandle + Clone, T: 'static>(
        &self,
        content: impl IntoElement,
        handle: &H,
        cx: &Context<T>,
    ) -> Div {
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(content)
            .child(self.overlay(handle, cx))
    }

    pub(crate) fn overlay_view<H: ScrollbarHandle + Clone>(
        &self,
        handle: &H,
        view: EntityId,
        id: impl Into<ElementId>,
    ) -> Div {
        let handle = handle.clone();
        let wheel_handle = handle.clone();
        let motion = self.0.clone();
        let mouse_motion = self.0.clone();
        let id = id.into();
        div()
            .absolute()
            .inset_0()
            .on_scroll(move |event, window, cx| {
                gpui_kit::base::apply_system_reduce_motion(cx);
                let mut state = motion.borrow_mut();
                let ScrollDelta::Lines(delta) = event.delta else {
                    // Preserve the trackpad's pixel precision and native momentum.
                    state.cancel();
                    return;
                };
                if delta.y == 0. || !delta.y.is_finite() {
                    return;
                }
                let distance = -delta.y * WHEEL_LINE_HEIGHT;
                let at = wheel_handle.offset().y;
                let max = max_offset(&wheel_handle);
                if (distance < 0. && at >= px(0.)) || (distance > 0. && at <= -max) {
                    state.cancel();
                    return;
                }
                cx.stop_propagation();
                if cx.reduce_motion() {
                    let remaining = state.pending;
                    state.cancel();
                    scroll_by(&wheel_handle, remaining + distance);
                    cx.notify(view);
                    return;
                }
                if state.pending * distance < 0. {
                    state.pending = 0.;
                }
                if state.pending == 0. {
                    state.last = Some(Instant::now());
                }
                // Bound the target so repeated wheel events cannot queue motion past an edge.
                state.pending = (state.pending + distance).clamp(f32::from(at), f32::from(at + max));
                if !state.scheduled {
                    state.scheduled = true;
                    schedule(
                        wheel_handle.clone(),
                        motion.clone(),
                        view,
                        state.generation,
                        window,
                    );
                }
            })
            .capture_any_mouse_down(move |_, _, _| mouse_motion.borrow_mut().cancel())
            .child(Scrollbar::vertical(&handle).id(id).viewport_from_layout())
    }

    pub(crate) fn overlay<H: ScrollbarHandle + Clone, T: 'static>(&self, handle: &H, cx: &Context<T>) -> Div {
        let view = cx.entity_id();
        let id = ElementId::NamedInteger("smooth-scroll".into(), Rc::as_ptr(&self.0) as u64);
        self.overlay_view(handle, view, id)
    }
}

#[derive(IntoElement)]
pub struct SmoothScrollable<E: InteractiveElement + Styled + ParentElement + Element> {
    id: ElementId,
    scrollable: Scrollable<E>,
}

impl<E> SmoothScrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element + ScrollableElement,
{
    pub fn new(element: E, id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            scrollable: element.overflow_y_scrollbar().id(id),
        }
    }

    #[allow(dead_code)]
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        let id = id.into();
        self.id = id.clone();
        self.scrollable = self.scrollable.id(id);
        self
    }
}

impl<E> Styled for SmoothScrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    fn style(&mut self) -> &mut StyleRefinement {
        self.scrollable.style()
    }
}

impl<E> ParentElement for SmoothScrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.scrollable.extend(elements)
    }
}

impl<E> InteractiveElement for SmoothScrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element,
{
    fn interactivity(&mut self) -> &mut Interactivity {
        self.scrollable.interactivity()
    }
}

impl<E> RenderOnce for SmoothScrollable<E>
where
    E: InteractiveElement + Styled + ParentElement + Element + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let view = window.current_view();
        let scroll_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::default())
            .read(cx)
            .clone();

        let motion = window
            .use_keyed_state((self.id.clone(), "smooth-motion"), cx, |_, _| {
                SmoothScroll::default()
            })
            .read(cx)
            .clone();

        let wheel_handle = scroll_handle.clone();
        let motion_clone = motion.0.clone();
        let mouse_motion = motion.0.clone();

        div().relative().size_full().child(self.scrollable).child(
            div()
                .absolute()
                .inset_0()
                .on_scroll(move |event, window, cx| {
                    gpui_kit::base::apply_system_reduce_motion(cx);
                    let mut state = motion_clone.borrow_mut();
                    let ScrollDelta::Lines(delta) = event.delta else {
                        // Preserve the trackpad's pixel precision and native momentum.
                        state.cancel();
                        return;
                    };
                    if delta.y == 0. || !delta.y.is_finite() {
                        return;
                    }
                    let distance = -delta.y * WHEEL_LINE_HEIGHT;
                    let at = wheel_handle.offset().y;
                    let max = max_offset(&wheel_handle);
                    if (distance < 0. && at >= px(0.)) || (distance > 0. && at <= -max) {
                        state.cancel();
                        return;
                    }
                    cx.stop_propagation();
                    if cx.reduce_motion() {
                        let remaining = state.pending;
                        state.cancel();
                        scroll_by(&wheel_handle, remaining + distance);
                        cx.notify(view);
                        return;
                    }
                    if state.pending * distance < 0. {
                        state.pending = 0.;
                    }
                    if state.pending == 0. {
                        state.last = Some(Instant::now());
                    }
                    // Bound the target so repeated wheel events cannot queue motion past an edge.
                    state.pending = (state.pending + distance).clamp(f32::from(at), f32::from(at + max));
                    if !state.scheduled {
                        state.scheduled = true;
                        schedule(
                            wheel_handle.clone(),
                            motion_clone.clone(),
                            view,
                            state.generation,
                            window,
                        );
                    }
                })
                .capture_any_mouse_down(move |_, _, _| mouse_motion.borrow_mut().cancel()),
        )
    }
}

fn max_offset(handle: &impl ScrollbarHandle) -> Pixels {
    (handle.content_size().height - handle.viewport_bounds().size.height).max(px(0.))
}

fn scroll_by(handle: &impl ScrollbarHandle, distance: f32) -> bool {
    let at = handle.offset();
    let max = max_offset(handle);
    let next = (at.y - px(distance)).clamp(-max, px(0.));
    handle.set_offset(point(at.x, next));
    next != at.y
}

fn schedule<H: ScrollbarHandle + Clone>(
    handle: H,
    motion: Rc<RefCell<Motion>>,
    view: EntityId,
    generation: u64,
    window: &mut Window,
) {
    window.on_next_frame(move |window, cx| {
        let mut state = motion.borrow_mut();
        if state.generation != generation {
            return;
        }
        state.scheduled = false;
        if state.pending == 0. {
            return;
        }
        let step = state.step(Instant::now(), cx.reduce_motion());
        if step != 0. && !scroll_by(&handle, step) {
            state.pending = 0.;
        }
        cx.notify(view);
        if state.pending != 0. {
            state.scheduled = true;
            drop(state);
            schedule(handle, motion, view, generation, window);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::Motion;
    use std::time::{Duration, Instant};

    #[test]
    fn motion_decay_steps_smoothly_to_zero() {
        let start = Instant::now();
        let mut motion = Motion {
            pending: 100.0,
            last: Some(start),
            ..Default::default()
        };

        let mut total_stepped = 0.0;
        let mut t = start;
        for _ in 0..60 {
            t += Duration::from_millis(16);
            let s = motion.step(t, false);
            total_stepped += s;
            if motion.pending == 0.0 {
                break;
            }
        }
        assert_eq!(motion.pending, 0.0);
        assert!((total_stepped - 100.0).abs() < 1e-4);
    }

    #[test]
    fn motion_cancels_cleanly() {
        let mut motion = Motion {
            pending: 50.0,
            scheduled: true,
            ..Default::default()
        };
        let generation = motion.generation;
        motion.cancel();
        assert_eq!(motion.pending, 0.0);
        assert!(!motion.scheduled);
        assert_eq!(motion.generation, generation.wrapping_add(1));
    }

    #[test]
    fn motion_reduced_motion_steps_all_at_once() {
        let start = Instant::now();
        let mut motion = Motion {
            pending: 75.0,
            last: Some(start),
            ..Default::default()
        };
        let s = motion.step(start + Duration::from_millis(16), true);
        assert_eq!(s, 75.0);
        assert_eq!(motion.pending, 0.0);
    }
}
