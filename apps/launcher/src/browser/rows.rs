//! The All addons view: one row of cards per category, each scrolling sideways.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use swofty_i18n::{tr, trf};

use super::*;

const ROW_CAP: usize = 24;
const ROW_HEIGHT: f32 = 340.;
const END_W: f32 = 168.;
const FADE_W: f32 = 64.;
const GLIDE_SECONDS: f32 = 0.32;

#[derive(Clone, Default)]
struct Row {
    scroll: ScrollHandle,
    glide: Rc<Glide>,
}

#[derive(Default)]
struct Glide {
    generation: Cell<u64>,
    target: Cell<Option<f32>>,
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

fn glide(row: &Row, to: f32, view: EntityId, window: &mut Window) {
    let generation = row.glide.generation.get().wrapping_add(1);
    row.glide.generation.set(generation);
    row.glide.target.set(Some(to));
    let from = f32::from(row.scroll.offset().x);
    glide_frame(row.clone(), from, to, Instant::now(), generation, view, window);
}

fn glide_frame(
    row: Row,
    from: f32,
    to: f32,
    start: Instant,
    generation: u64,
    view: EntityId,
    window: &mut Window,
) {
    window.on_next_frame(move |window, cx| {
        if row.glide.generation.get() != generation {
            return;
        }
        let t = if cx.reduce_motion() {
            1.
        } else {
            (start.elapsed().as_secs_f32() / GLIDE_SECONDS).min(1.)
        };
        let eased = 1. - (1. - t).powi(3);
        let y = row.scroll.offset().y;
        row.scroll.set_offset(point(px(from + (to - from) * eased), y));
        cx.notify(view);
        if t < 1. {
            glide_frame(row, from, to, start, generation, view, window);
        } else {
            row.glide.target.set(None);
        }
    });
}

impl Shell {
    pub(super) fn category_rows(
        &mut self,
        shown: Arc<[usize]>,
        query: Arc<Query>,
        width: f32,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let groups: Arc<[(Tab, Vec<usize>)]> = group_rows(&self.browser.addons, &shown).into();
        let layout = (shown, groups.len(), true, width.to_bits());
        if self.browser.grid_layout != layout {
            let offset = self.browser.grid_scroll.logical_scroll_top();
            self.browser
                .grid_scroll
                .reset_with_uniform_height(groups.len(), px(ROW_HEIGHT));
            self.browser.grid_scroll.scroll_to(offset);
            self.browser.grid_layout = layout;
        }
        let content = list(
            self.browser.grid_scroll.clone(),
            cx.processor(move |this, index: usize, _, cx| match groups.get(index) {
                Some((tab, found)) => this.category_row(*tab, found, &query, width, p, cx),
                None => div().into_any_element(),
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
        query: &Query,
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
            .map(|a| self.browser_card(a, query, CARD_W, p, cx));
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
            .children(cards)
            .when(found.len() > ROW_CAP, |d| {
                d.child(self.row_end(tab, found.len(), p, cx))
            });
        v_flex()
            .pb(px(26.))
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
                    .child(plural(count, "addon", "addons")),
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
        let unit = f32::from(px(CARD_W + GAP));
        let viewport = f32::from(row.scroll.bounds().size.width);
        let max = f32::from(row.scroll.max_offset().x).abs();
        let from = row
            .glide
            .target
            .get()
            .unwrap_or_else(|| f32::from(row.scroll.offset().x));
        let group = (viewport / unit).floor() as usize;
        glide(row, paged(from, unit, group, max, way), cx.entity_id(), window);
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
    use super::{Addon, Category, Tab, Way, group_rows, paged};

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
}
