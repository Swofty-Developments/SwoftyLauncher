//! The All addons view: one row of cards per category, each scrolling sideways.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::catalog::Addon;
use swofty_i18n::{tr, trf, trn};

use super::tiles::landing_rows;
use super::*;
use crate::pages::tile;
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::Palette;
use crate::ui::ProgressBar;

const ROW_CAP: usize = 24;
const ROW_HEIGHT: f32 = 310.;
const POSTER_W: f32 = 216.;
const POSTER_ART: f32 = 128.;
const POSTER_ICON: f32 = 76.;
const END_W: f32 = 168.;
const FADE_W: f32 = 64.;
const GLIDE_SECONDS: f32 = 0.32;
const DRAG_SLOP: f32 = 6.;
const FLING_MIN: f32 = 240.;
const FLING_REACH: f32 = 0.22;
const FLING_SECONDS: f32 = 0.6;
const FLING_STALE: Duration = Duration::from_millis(80);

#[derive(Clone, Default)]
struct Row {
    scroll: ScrollHandle,
    glide: Rc<Glide>,
    drag: Rc<Drag>,
}

#[derive(Default)]
struct Glide {
    generation: Cell<u64>,
    target: Cell<Option<f32>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lane {
    Undecided,
    Across,
    Down,
}

#[derive(Clone, Copy)]
struct Press {
    from: Point<Pixels>,
    last: Point<Pixels>,
    time: Instant,
    offset: f32,
    velocity: f32,
    lane: Lane,
}

#[derive(Default)]
struct Drag {
    press: Cell<Option<Press>>,
}

#[derive(Clone, Copy)]
struct Motion {
    from: f32,
    to: f32,
    start: Instant,
    seconds: f32,
    generation: u64,
}

#[derive(Default)]
pub(super) struct Rows(RefCell<HashMap<Tab, Row>>);

impl Rows {
    fn row(&self, tab: Tab) -> Row {
        self.0.borrow_mut().entry(tab).or_default().clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    Back,
    Forward,
}

/// The categories that hold an addon in `shown`, each with its addons in `shown`'s order.
fn group_rows(addons: &[Addon], shown: &[usize]) -> Vec<(Tab, Vec<usize>)> {
    shelves()
        .filter_map(|tab| {
            let found: Vec<usize> = shown
                .iter()
                .copied()
                .filter(|&i| {
                    addons
                        .get(i)
                        .is_some_and(|a| tab.is_none_or(|t| a.category() == t))
                })
                .collect();
            (!found.is_empty()).then_some((tab, found))
        })
        .collect()
}

/// Where a row scrolled to `at` (zero or negative) ends up after paging `way` by
/// whole cards `unit` wide, `group` at a time, never past `-max`.
fn paged(at: f32, unit: f32, group: usize, max: f32, way: Way) -> f32 {
    let first = (-at / unit).round();
    let step = group.max(1) as f32;
    let next = match way {
        Way::Back => (first - step).max(0.),
        Way::Forward => first + step,
    };
    (-next * unit).clamp(-max.max(0.), 0.)
}

fn glide(row: &Row, to: f32, seconds: f32, view: EntityId, window: &mut Window) {
    let generation = row.glide.generation.get().wrapping_add(1);
    row.glide.generation.set(generation);
    row.glide.target.set(Some(to));
    let motion = Motion {
        from: f32::from(row.scroll.offset().x),
        to,
        start: Instant::now(),
        seconds,
        generation,
    };
    glide_frame(row.clone(), motion, view, window);
}

fn glide_frame(row: Row, motion: Motion, view: EntityId, window: &mut Window) {
    window.on_next_frame(move |window, cx| {
        if row.glide.generation.get() != motion.generation {
            return;
        }
        let t = if cx.reduce_motion() {
            1.
        } else {
            (motion.start.elapsed().as_secs_f32() / motion.seconds).min(1.)
        };
        let eased = 1. - (1. - t).powi(3);
        let y = row.scroll.offset().y;
        let x = motion.from + (motion.to - motion.from) * eased;
        row.scroll.set_offset(point(px(x), y));
        cx.notify(view);
        if t < 1. {
            glide_frame(row, motion, view, window);
        } else {
            row.glide.target.set(None);
        }
    });
}

fn lane_of(dx: f32, dy: f32) -> Lane {
    if dx.hypot(dy) < DRAG_SLOP {
        Lane::Undecided
    } else if dx.abs() >= dy.abs() {
        Lane::Across
    } else {
        Lane::Down
    }
}

fn dragged(origin: f32, dx: f32, max: f32) -> f32 {
    (origin + dx).clamp(-max.max(0.), 0.)
}

fn fling_target(at: f32, velocity: f32, max: f32) -> Option<f32> {
    (velocity.abs() >= FLING_MIN).then(|| (at + velocity * FLING_REACH).clamp(-max.max(0.), 0.))
}

impl Shell {
    pub(super) fn category_rows(
        &mut self,
        shown: Arc<[usize]>,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups: Arc<[(Tab, Vec<usize>)]> = group_rows(&self.browser.addons, &shown).into();
        let header = landing_rows(width);
        let layout = (shown, groups.len() + header, true, width.to_bits());
        if self.browser.grid_layout != layout {
            let offset = self.browser.grid_scroll.logical_scroll_top();
            self.browser
                .grid_scroll
                .reset_with_uniform_height(groups.len() + header, px(ROW_HEIGHT));
            self.browser.grid_scroll.scroll_to(offset);
            self.browser.grid_layout = layout;
        }
        let content = list(
            self.browser.grid_scroll.clone(),
            cx.processor(move |this, index: usize, _, cx| {
                if index < header {
                    return this.landing_row(index, width, p, cx);
                }
                match groups.get(index - header) {
                    Some((tab, found)) => this.category_row(*tab, found, width, p, cx),
                    None => div().into_any_element(),
                }
            }),
        )
        .size_full();
        self.browser
            .smooth
            .wrap(content, &self.browser.grid_scroll, cx)
            .into_any_element()
    }

    fn category_row(
        &self,
        tab: Tab,
        found: &[usize],
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = self.browser.rows.row(tab);
        let name = tab_label(tab);
        let cards = found
            .iter()
            .take(ROW_CAP)
            .filter_map(|&i| self.browser.addons.get(i))
            .map(|a| self.poster_card(a, tab, p, cx))
            .collect::<Vec<_>>();
        let at = f32::from(row.scroll.offset().x);
        let max = f32::from(row.scroll.max_offset().x).abs();
        let measured = row.scroll.bounds().size.width > px(0.);
        let fade_left = measured && at < -0.5;
        let fade_right = measured && at > -max + 0.5;
        let scroller = h_flex()
            .id(SharedString::from(format!("browser-row-{name}")))
            .w(px(width - 1.))
            .items_stretch()
            .gap(px(GAP))
            .overflow_x_scroll()
            .track_scroll(&row.scroll)
            .capture_any_mouse_down(self.press_listener(&row, cx))
            .on_mouse_move(self.drag_listener(&row, cx))
            .capture_any_mouse_up(self.release_listener(&row, cx))
            .on_mouse_up_out(MouseButton::Left, self.release_listener(&row, cx))
            .children(cards)
            .when(found.len() > ROW_CAP, |d| {
                d.child(self.row_end(tab, found.len(), p, cx))
            });
        let keyed = row.clone();
        v_flex()
            .id(SharedString::from(format!("browser-row-box-{name}")))
            .pb(px(26.))
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| {
                let way = match e.keystroke.key.as_str() {
                    "left" => Way::Back,
                    "right" => Way::Forward,
                    _ => return,
                };
                if e.keystroke.modifiers.modified() {
                    return;
                }
                cx.stop_propagation();
                this.page_row(&keyed, way, window, cx);
            }))
            .child(self.row_heading(tab, found.len(), &row, (at, max, measured), p, cx))
            .child(
                div()
                    .relative()
                    .w(px(width - 1.))
                    .child(scroller)
                    .when(fade_left, |d| d.child(edge_fade(true, p)))
                    .when(fade_right, |d| d.child(edge_fade(false, p))),
            )
            .into_any_element()
    }

    fn press_listener(
        &self,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let row = row.clone();
        cx.listener(move |_, e: &MouseDownEvent, _, _| {
            if e.button != MouseButton::Left {
                return;
            }
            row.drag.press.set(Some(Press {
                from: e.position,
                last: e.position,
                time: Instant::now(),
                offset: f32::from(row.scroll.offset().x),
                velocity: 0.,
                lane: Lane::Undecided,
            }));
        })
    }

    fn drag_listener(
        &self,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseMoveEvent, &mut Window, &mut App) + 'static {
        let row = row.clone();
        cx.listener(move |this, e: &MouseMoveEvent, _, cx| this.drag_row(&row, e, cx))
    }

    fn release_listener(
        &self,
        row: &Row,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static {
        let row = row.clone();
        cx.listener(move |this, e: &MouseUpEvent, window, cx| {
            if e.button == MouseButton::Left && this.end_drag(&row, window, cx) {
                cx.stop_propagation();
            }
        })
    }

    fn drag_row(&mut self, row: &Row, e: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(mut press) = row.drag.press.get() else {
            return;
        };
        if e.pressed_button != Some(MouseButton::Left) {
            row.drag.press.set(None);
            return;
        }
        let dx = f32::from(e.position.x - press.from.x);
        let dy = f32::from(e.position.y - press.from.y);
        if press.lane == Lane::Undecided {
            press.lane = lane_of(dx, dy);
            if press.lane != Lane::Undecided {
                row.glide
                    .generation
                    .set(row.glide.generation.get().wrapping_add(1));
                row.glide.target.set(None);
            }
        }
        match press.lane {
            Lane::Undecided => {}
            Lane::Across => {
                let max = f32::from(row.scroll.max_offset().x).abs();
                let y = row.scroll.offset().y;
                row.scroll
                    .set_offset(point(px(dragged(press.offset, dx, max)), y));
                let dt = press.time.elapsed().as_secs_f32();
                if dt > 0.001 {
                    let step = f32::from(e.position.x - press.last.x) / dt;
                    press.velocity = press.velocity * 0.4 + step * 0.6;
                    press.time = Instant::now();
                }
            }
            Lane::Down => {
                self.browser.smooth.cancel();
                self.browser.grid_scroll.scroll_by(press.last.y - e.position.y);
            }
        }
        press.last = e.position;
        row.drag.press.set(Some(press));
        cx.notify();
    }

    fn end_drag(&mut self, row: &Row, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(press) = row.drag.press.take() else {
            return false;
        };
        if press.lane == Lane::Across && press.time.elapsed() < FLING_STALE {
            let at = f32::from(row.scroll.offset().x);
            let max = f32::from(row.scroll.max_offset().x).abs();
            if let Some(to) = fling_target(at, press.velocity, max) {
                glide(row, to, FLING_SECONDS, cx.entity_id(), window);
            }
        }
        press.lane != Lane::Undecided
    }

    fn row_heading(
        &self,
        tab: Tab,
        count: usize,
        row: &Row,
        (at, max, measured): (f32, f32, bool),
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let tint = shelf_tint(tab, p);
        let name = tab_label(tab);
        let page = |way: Way, icon: IconName, off: bool| {
            let row = row.clone();
            Button::new(SharedString::from(format!("browser-row-{name}-{way:?}")))
                .ghost()
                .small()
                .icon(icon)
                .disabled(off)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.page_row(&row, way, window, cx);
                }))
        };
        h_flex()
            .pb(px(10.))
            .gap(px(10.))
            .items_center()
            .child(
                tile(28., p)
                    .bg(tint.opacity(0.14))
                    .border_color(tint.opacity(0.3))
                    .child(shelf_glyph(tab, 15., tint)),
            )
            .child(
                div()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(trn(count, "{count} addon", "{count} addons")),
            )
            .child(
                Button::new(SharedString::from(format!("browser-row-{name}-all")))
                    .ghost()
                    .small()
                    .label(tr("See all"))
                    .on_click(cx.listener(move |this, _, _, cx| this.shelve(tab, cx))),
            )
            .child(page(Way::Back, IconName::ChevronLeft, at > -0.5))
            .child(page(
                Way::Forward,
                IconName::ChevronRight,
                measured && at <= -max + 0.5,
            ))
    }

    fn page_row(&self, row: &Row, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        let unit = f32::from(px(POSTER_W + GAP));
        let viewport = f32::from(row.scroll.bounds().size.width);
        let max = f32::from(row.scroll.max_offset().x).abs();
        let from = row
            .glide
            .target
            .get()
            .unwrap_or_else(|| f32::from(row.scroll.offset().x));
        let group = (viewport / unit).floor() as usize;
        glide(
            row,
            paged(from, unit, group, max, way),
            GLIDE_SECONDS,
            cx.entity_id(),
            window,
        );
    }

    fn poster_card(&self, a: &Addon, tab: Tab, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let id = a.id.clone();
        let tint = shelf_tint(tab, p);
        let offer = self.offer(a);
        let failed = self.browser.failed.contains_key(&a.id);
        let updating = self.catalog_update(&a.id).is_some();
        let progress = self.browser.working.get(&a.id).map(Job::progress);
        let state = match &offer {
            Offer::Installed(_) => Some((IconName::Check, tr("Installed"), p.success)),
            Offer::Update(..) => Some((IconName::CircleArrowUp, tr("Update"), p.warning)),
            _ => None,
        };
        let art = div()
            .relative()
            .h(px(POSTER_ART))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(p.surface.blend(tint.opacity(0.16)))
            .child(self.catalog_icon(a, POSTER_ICON, p))
            .child(div().absolute().top(px(8.)).right(px(8.)).child(self.star_button(
                a,
                format!("browser-poster-star-{}", a.id),
                false,
                p,
                cx,
            )))
            .when_some(state, |d, (icon, text, color)| {
                d.child(
                    h_flex()
                        .absolute()
                        .top(px(8.))
                        .left(px(8.))
                        .h(px(22.))
                        .px(px(7.))
                        .gap(px(4.))
                        .items_center()
                        .rounded(px(11.))
                        .bg(p.background.opacity(0.8))
                        .text_size(px(11.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color)
                        .child(Icon::new(icon).size(px(12.)).text_color(color))
                        .child(text),
                )
            });
        let action = match offer {
            Offer::Installed(v) => div()
                .h(px(28.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .text_color(p.text_placeholder)
                .child(format!("v{v}")),
            _ => div().w_full().overflow_hidden().child(
                self.offer_button(a, format!("browser-poster-offer-{}", a.id), cx)
                    .w_full(),
            ),
        };
        v_flex()
            .id(SharedString::from(format!("browser-poster-{}", a.id)))
            .focusable()
            .w(px(POSTER_W))
            .flex_none()
            .rounded(px(10.))
            .border_1()
            .border_color(card_border(failed, updating, p))
            .bg(p.surface)
            .overflow_hidden()
            .cursor_pointer()
            .hover(move |s| s.border_color(p.accent).bg(p.hover))
            .focus_visible(move |s| s.border_color(p.accent))
            .on_click(cx.listener(move |this, _, _, cx| this.open_details(&id, cx)))
            .child(art)
            .child(
                v_flex()
                    .flex_1()
                    .p(px(12.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(a.name.clone()),
                    )
                    .child(self.byline(a, format!("browser-poster-by-{}", a.id), 12., p, cx))
                    .child(div().flex_1().min_h(px(6.)))
                    .child(action)
                    .when_some(progress, |d, pr| {
                        d.child(ProgressBar::new(pr.fraction()).height(3.))
                    }),
            )
            .into_any_element()
    }

    fn row_end(&self, tab: Tab, count: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let tint = shelf_tint(tab, p);
        v_flex()
            .id(SharedString::from(format!("browser-row-end-{}", tab_label(tab))))
            .w(px(END_W))
            .flex_none()
            .gap(px(8.))
            .items_center()
            .justify_center()
            .rounded(px(10.))
            .border_1()
            .border_dashed()
            .border_color(p.border)
            .cursor_pointer()
            .text_color(p.text_muted)
            .hover(move |s| s.bg(p.hover).border_color(tint).text_color(p.text))
            .child(Icon::new(IconName::ArrowRight).size(px(20.)))
            .child(
                div()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(trf!("See all {count}", count = count)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.shelve(tab, cx)))
            .into_any_element()
    }
}

fn edge_fade(left: bool, p: Palette) -> Div {
    let (from, to) = (p.background, p.background.opacity(0.));
    let (start, end) = if left { (from, to) } else { (to, from) };
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(FADE_W))
        .map(|d| if left { d.left_0() } else { d.right_0() })
        .bg(linear_gradient(
            90.,
            linear_color_stop(start, 0.),
            linear_color_stop(end, 1.),
        ))
}

#[cfg(test)]
mod tests {
    use super::{Addon, Lane, Tab, Way, dragged, fling_target, group_rows, lane_of, paged};
    use swofty_core::manifest::Category;

    fn addon(id: &str, category: Option<&str>) -> Addon {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "category": category
        }))
        .unwrap()
    }

    #[test]
    fn rows_follow_the_category_order_and_skip_empty_ones() {
        let addons = [
            addon("a", Some("shaders")),
            addon("b", Some("seasons")),
            addon("c", None),
            addon("d", Some("shaders")),
        ];
        let rows = group_rows(&addons, &[3, 0, 1, 2]);
        let found: Vec<(Tab, &[usize])> = rows.iter().map(|(t, v)| (*t, v.as_slice())).collect();
        assert_eq!(
            found,
            [
                (Some(Some(Category::Seasons)), &[1][..]),
                (Some(Some(Category::Shaders)), &[3, 0][..]),
                (Some(None), &[2][..]),
            ]
        );
    }

    #[test]
    fn rows_leave_out_what_is_not_shown() {
        let addons = [addon("a", Some("cars")), addon("b", Some("cars"))];
        assert_eq!(group_rows(&addons, &[1]).len(), 1);
        assert!(group_rows(&addons, &[]).is_empty());
    }

    #[test]
    fn paging_moves_whole_cards_and_stops_at_the_ends() {
        assert_eq!(paged(0., 100., 3, 1000., Way::Forward), -300.);
        assert_eq!(paged(-300., 100., 3, 1000., Way::Forward), -600.);
        assert_eq!(paged(-900., 100., 3, 1000., Way::Forward), -1000.);
        assert_eq!(paged(-300., 100., 3, 1000., Way::Back), 0.);
        assert_eq!(paged(-250., 100., 2, 1000., Way::Back), -100.);
        assert_eq!(paged(0., 100., 0, 1000., Way::Forward), -100.);
        assert_eq!(paged(0., 100., 3, 0., Way::Forward), 0.);
    }

    #[test]
    fn a_drag_picks_its_lane_after_a_few_pixels() {
        assert_eq!(lane_of(2., 1.), Lane::Undecided);
        assert_eq!(lane_of(-9., 3.), Lane::Across);
        assert_eq!(lane_of(2., -9.), Lane::Down);
    }

    #[test]
    fn dragging_follows_the_pointer_and_clamps_at_the_ends() {
        assert_eq!(dragged(-100., -50., 1000.), -150.);
        assert_eq!(dragged(-100., 50., 1000.), -50.);
        assert_eq!(dragged(-100., 500., 1000.), 0.);
        assert_eq!(dragged(-900., -500., 1000.), -1000.);
        assert_eq!(dragged(0., -50., 0.), 0.);
    }

    #[test]
    fn a_fast_release_glides_on_and_a_slow_one_stays() {
        assert_eq!(fling_target(-100., -1000., 2000.), Some(-320.));
        assert_eq!(fling_target(-100., 100., 2000.), None);
        assert_eq!(fling_target(-100., -5000., 300.), Some(-300.));
        assert_eq!(fling_target(-100., 5000., 300.), Some(0.));
    }
}
