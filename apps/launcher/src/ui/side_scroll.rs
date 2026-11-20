//! A row that scrolls sideways. An end with more of the row past it fades out and holds a
//! button that scrolls on by most of the row's width.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::ui::scale::px;
use crate::ui::theme::Palette;

const FADE_W: f32 = 72.;
const BUTTON: f32 = 28.;
const STEP: f32 = 0.75;

/// Which ends of a row scrolled to `offset`, of at most `max`, have more past them: the start, then the end.
fn clipped(offset: f32, max: f32) -> (bool, bool) {
    (offset < -0.5, -offset < max - 0.5)
}

/// Where a press of an end's button scrolls the row from `offset`: `STEP` of the `visible` width on, within `max`.
fn stepped(offset: f32, max: f32, visible: f32, forward: bool) -> f32 {
    let step = visible * STEP;
    let to = if forward { offset - step } else { offset + step };
    to.clamp(-max.max(0.), 0.)
}

fn ends(handle: &ScrollHandle) -> (bool, bool) {
    clipped(handle.offset().x.as_f32(), handle.max_offset().x.as_f32())
}

/// `row`, which scrolls sideways, with its clipped ends faded into `p.background`. The ends drawn
/// come from the last frame's layout; when this frame's differ, the view draws again.
pub(crate) fn side_scroll<T: 'static>(
    id: &'static str,
    row: Stateful<Div>,
    handle: &ScrollHandle,
    p: Palette,
    cx: &Context<T>,
) -> Div {
    let drawn = ends(handle);
    let watch = {
        let handle = handle.clone();
        let view = cx.weak_entity();
        canvas(
            |_, _, _| {},
            move |_, _, _, cx| {
                if ends(&handle) != drawn {
                    cx.defer(move |cx| {
                        view.update(cx, |_, cx| cx.notify()).ok();
                    });
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    };
    div()
        .relative()
        .w_full()
        .child(row.track_scroll(handle))
        .when(drawn.0, |d| d.child(edge(id, false, handle, p, cx)))
        .when(drawn.1, |d| d.child(edge(id, true, handle, p, cx)))
        .child(watch)
}

fn edge<T: 'static>(
    id: &'static str,
    forward: bool,
    handle: &ScrollHandle,
    p: Palette,
    cx: &Context<T>,
) -> Div {
    let handle = handle.clone();
    let solid = p.background;
    let clear = solid.opacity(0.);
    let fade = if forward {
        linear_gradient(90., linear_color_stop(clear, 0.), linear_color_stop(solid, 0.55))
    } else {
        linear_gradient(90., linear_color_stop(solid, 0.45), linear_color_stop(clear, 1.))
    };
    let button = div()
        .id((id, usize::from(forward)))
        .size(px(BUTTON))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(p.border)
        .bg(p.elevated)
        .text_color(p.text_muted)
        .cursor_pointer()
        .occlude()
        .hover(move |st| st.bg(p.hover).text_color(p.text))
        .child(
            Icon::new(if forward {
                IconName::ChevronRight
            } else {
                IconName::ChevronLeft
            })
            .size(px(16.)),
        )
        .on_click(cx.listener(move |_, _, _, cx| {
            let to = stepped(
                handle.offset().x.as_f32(),
                handle.max_offset().x.as_f32(),
                handle.bounds().size.width.as_f32(),
                forward,
            );
            handle.set_offset(point(Pixels::from(to), handle.offset().y));
            cx.notify();
        }));
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(FADE_W))
        .flex()
        .items_center()
        .map(|d| {
            if forward {
                d.right_0().justify_end()
            } else {
                d.left_0().justify_start()
            }
        })
        .bg(fade)
        .child(button)
}

#[cfg(test)]
mod tests {
    use super::{clipped, stepped};

    #[test]
    fn a_sideways_row_marks_the_ends_with_more_past_them() {
        assert_eq!(clipped(0., 0.), (false, false));
        assert_eq!(clipped(0., 300.), (false, true));
        assert_eq!(clipped(-120., 300.), (true, true));
        assert_eq!(clipped(-300., 300.), (true, false));
    }

    #[test]
    fn an_end_button_scrolls_most_of_the_row_on_without_passing_the_end() {
        assert_eq!(stepped(0., 900., 400., true), -300.);
        assert_eq!(stepped(-800., 900., 400., true), -900.);
        assert_eq!(stepped(-200., 900., 400., false), 0.);
        assert_eq!(stepped(-700., 900., 400., false), -400.);
    }
}
