use super::model::*;
use super::widgets::*;
use super::{Load, Target};
use crate::format::{iso_date, lap_time, sector_time};
use crate::scale::{Hit, px};
use crate::shell::Shell;
use crate::theme::{Palette, ScrollArea};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Disableable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::tr;

impl Shell {
    pub(crate) fn online_board(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let page = match &self.online.board {
            None | Some((_, Load::Idle | Load::Loading)) => return self.skeleton(p, 6, 56.),
            Some((_, Load::Failed(e))) => {
                let e = e.clone();
                return self.failed_view(&e, cx);
            }
            Some((_, Load::Ready(page))) => page.clone(),
        };
        let mut items: Vec<Target> = Vec::new();
        let sectors = self.content_width >= 900.;
        let best = best_sectors(&page.rows, 3);
        let leader = page.rows.first().map_or(0, |r| r.time_ms);
        let me = self.account_name();
        let mut body: Vec<AnyElement> = Vec::new();
        for r in &page.rows {
            let idx = self.target_item(&mut items, Target::Profile(r.driver.clone()));
            let mine = me.as_deref() == Some(r.driver.name.as_str());
            body.push(self.board_row(r, idx, leader, &best, sectors, mine, p, cx));
            if self.online.viewing.as_deref() == Some(r.lap_id.as_str()) {
                body.push(self.lap_actions(&r.lap_id, p, cx));
            }
            if self.online.setup_open.as_deref() == Some(r.lap_id.as_str())
                && let Some(setup) = &r.setup
            {
                let mine = page
                    .mine
                    .as_ref()
                    .filter(|m| m.lap_id != r.lap_id)
                    .and_then(|m| m.setup.as_ref());
                body.push(self.setup_panel(setup, mine, p, cx));
            }
        }
        self.online.items = items;
        let remaining = page.total.saturating_sub(page.rows.len() as u32);
        let more = (remaining > 0).then(|| {
            let key = page.track.key.clone();
            let at = page.rows.len() as u32;
            Button::new("board-more")
                .outline()
                .tap_sm()
                .disabled(self.online.more)
                .label(if self.online.more {
                    tr("Loading…")
                } else {
                    tr("Show more laps")
                })
                .on_click(cx.listener(move |this, _, _, cx| this.online_load_board(key.clone(), at, cx)))
        });
        let header = self.board_header(&page, p);
        let filters = self.filter_chips(p, cx);
        let pinned = page
            .mine
            .clone()
            .filter(|m| page.rows.iter().all(|r| r.lap_id != m.lap_id))
            .map(|m| self.board_row(&m, usize::MAX, leader, &best, sectors, true, p, cx));
        let history = self.history_strip(&page, p);
        let head = h_flex()
            .h(px(32.))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .bg(p.elevated)
            .child(head_cell(40., "RANK", p))
            .child(div().flex_1().min_w_0().child(head_cell(120., "DRIVER", p)))
            .child(head_cell(88., "TIME", p))
            .child(head_cell(74., "GAP", p))
            .when(sectors, |d| {
                d.child(head_cell(66., "S1", p))
                    .child(head_cell(66., "S2", p))
                    .child(head_cell(66., "S3", p))
            })
            .when(self.content_width >= 1100., |d| {
                d.child(head_cell(130., "CAR", p))
            })
            .child(head_cell(84., "DATE", p))
            .child(head_cell(112., "", p));
        v_flex()
            .flex_1()
            .min_h_0()
            .scroll_y("online-board")
            .px(px(28.))
            .pb(px(28.))
            .gap(px(16.))
            .child(header)
            .child(filters)
            .child(panel(p).child(head).children(pinned).children(body))
            .children(more)
            .children(history)
            .into_any_element()
    }

    fn board_header(&self, page: &BoardPage, p: Palette) -> AnyElement {
        let t = &page.track;
        panel(p)
            .child(
                h_flex()
                    .p(px(14.))
                    .gap(px(16.))
                    .items_center()
                    .child(self.layout_art(t.layout.as_ref(), 84., p.accent_hover, p))
                    .child(
                        v_flex()
                            .gap(px(4.))
                            .child(
                                h_flex()
                                    .gap(px(10.))
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(26.))
                                            .font_weight(FontWeight::BOLD)
                                            .child(t.name.clone()),
                                    )
                                    .children(self.flag_chip(&t.flag, p)),
                            )
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(p.text_muted)
                                    .child(format!("{} - {}", page.season, t.key.weather)),
                            )
                            .children(t.record.as_ref().map(|r| {
                                h_flex()
                                    .gap(px(10.))
                                    .items_center()
                                    .pt(px(8.))
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(p.text_placeholder)
                                            .child(tr("RECORD")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(28.))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(p.accent_hover)
                                            .child(lap_time(r.time_ms)),
                                    )
                                    .child(self.tt_face(&r.driver, 26., p))
                                    .child(div().text_size(px(14.)).child(r.driver.name.clone()))
                            })),
                    ),
            )
            .into_any_element()
    }

    fn filter_chips(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let f = self.online.filter.clone();
        let chip = |id: &'static str, label: &'static str, on: bool, f: Filter, cx: &mut Context<Self>| {
            h_flex()
                .id(id)
                .h(px(30.))
                .px(px(14.))
                .items_center()
                .rounded_full()
                .border_1()
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .map(|d| {
                    if on {
                        d.bg(p.selected).border_color(p.accent).text_color(p.text)
                    } else {
                        d.border_color(p.border)
                            .text_color(p.text_muted)
                            .hover(move |s| s.bg(p.hover))
                    }
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| this.online_set_filter(f.clone(), cx)))
        };
        let mut assists = f.clone();
        assists.assists_off = !f.assists_off;
        let mut wheel = f.clone();
        wheel.wheel = !f.wheel;
        h_flex()
            .gap(px(8.))
            .items_center()
            .child(chip("f-assists", tr("No assists"), f.assists_off, assists, cx))
            .child(chip("f-wheel", tr("Wheel"), f.wheel, wheel, cx))
            .child(
                chip("f-clear", tr("Clear filters"), false, Filter::default(), cx)
                    .when(!f.any(), |d| d.opacity(0.4)),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn board_row(
        &self,
        r: &Row,
        idx: usize,
        leader: u64,
        best: &[Option<u64>],
        sectors: bool,
        mine: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let driver = r.driver.clone();
        let lap = r.lap_id.clone();
        let open = self.online.viewing.as_deref() == Some(r.lap_id.as_str());
        let personal: Vec<Option<u64>> = (0..3).map(|i| r.sectors_ms.get(i).copied()).collect();
        let colour = medal(r.rank, &p);
        h_flex()
            .id(SharedString::from(format!("row-{idx}-{}", r.lap_id)))
            .h(px(38.))
            .px(px(14.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border_variant)
            .when(r.rank <= 3, |d| d.bg(colour.opacity(0.06)))
            .when(mine, |d| d.bg(p.selected))
            .when(self.online.ringed(idx), |d| d.border_1().border_color(p.accent))
            .child(
                div()
                    .w(px(40.))
                    .text_size(px(15.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(colour)
                    .child(r.rank.to_string()),
            )
            .child(
                h_flex()
                    .id(SharedString::from(format!("drv-{idx}")))
                    .flex_1()
                    .min_w_0()
                    .gap(px(10.))
                    .items_center()
                    .cursor_pointer()
                    .hover(move |s| s.text_color(p.accent_hover))
                    .child(self.tt_face(&r.driver, 28., p))
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(r.driver.name.clone()),
                    )
                    .when(mine, |d| d.child(chip(tr("YOU"), p)))
                    .on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.online_go(Target::Profile(driver.clone()), cx)
                        }),
                    ),
            )
            .child(
                div()
                    .w(px(88.))
                    .flex_shrink_0()
                    .font_weight(FontWeight::BOLD)
                    .child(lap_time(r.time_ms)),
            )
            .child(
                div()
                    .w(px(74.))
                    .flex_shrink_0()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(gap_text(r.time_ms, leader)),
            )
            .when(sectors, |d| {
                d.children((0..3).map(|i| {
                    let t = personal[i];
                    let colour = match t.map(|t| sector_mark(t, best[i], None)) {
                        Some(SectorMark::Overall) => hsla(0.78, 0.7, 0.72, 1.),
                        _ => p.text_muted,
                    };
                    div()
                        .w(px(66.))
                        .flex_shrink_0()
                        .text_size(px(13.))
                        .font_weight(if colour == p.text_muted {
                            FontWeight::NORMAL
                        } else {
                            FontWeight::BOLD
                        })
                        .text_color(colour)
                        .child(t.map(sector_time).unwrap_or_else(|| "-".into()))
                }))
            })
            .when(self.content_width >= 1100., |d| {
                d.child(
                    div()
                        .w(px(130.))
                        .flex_shrink_0()
                        .truncate()
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .child(r.car.clone().unwrap_or_default()),
                )
            })
            .child(
                div()
                    .w(px(84.))
                    .flex_shrink_0()
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(iso_date(&r.date).to_owned()),
            )
            .child(
                h_flex()
                    .w(px(112.))
                    .flex_shrink_0()
                    .gap(px(4.))
                    .child(
                        Button::new(SharedString::from(format!("view-{idx}-{}", r.lap_id)))
                            .outline()
                            .tap_xs()
                            .label(tr("View"))
                            .when(open, |b| b.primary())
                            .on_click(cx.listener({
                                let lap = lap.clone();
                                move |this, _, _, cx| {
                                    let same = this.online.viewing.as_deref() == Some(lap.as_str());
                                    this.online.viewing = (!same).then(|| lap.clone());
                                    cx.notify();
                                }
                            })),
                    )
                    .child(self.setup_button(
                        &format!("{idx}-{}", r.lap_id),
                        &r.lap_id,
                        r.setup.is_some(),
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn lap_actions(&self, lap_id: &str, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let url = lap_url(lap_id);
        let running = self.online_game_running();
        let ready = self.online.replays.available();
        let id = lap_id.to_owned();
        h_flex()
            .px(px(14.))
            .py(px(8.))
            .gap(px(8.))
            .items_center()
            .bg(p.background)
            .border_t_1()
            .border_color(p.border_variant)
            .child(
                Button::new(SharedString::from(format!("web-{lap_id}")))
                    .outline()
                    .tap_sm()
                    .icon(IconName::ExternalLink)
                    .label(tr("Open on the web"))
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            )
            .when(running, |d| {
                d.child(
                    Button::new(SharedString::from(format!("watch-{lap_id}")))
                        .outline()
                        .tap_sm()
                        .icon(IconName::Play)
                        .label(tr("Watch in game"))
                        .disabled(!ready)
                        .tooltip(if ready {
                            tr("Play this lap's replay in the running game")
                        } else {
                            tr("In-game replays are not available yet")
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Err(e) = this.online.replays.watch(&id) {
                                crate::app_log::action("watch a lap in game").failed(&e);
                            }
                            cx.notify();
                        })),
                )
            })
            .into_any_element()
    }

    fn history_strip(&self, page: &BoardPage, p: Palette) -> Option<AnyElement> {
        if page.history.is_empty() {
            return None;
        }
        let mut last: Option<u64> = None;
        let points: Vec<AnyElement> = page
            .history
            .iter()
            .map(|h| {
                let step = last.map(|l| l.saturating_sub(h.time_ms));
                last = Some(h.time_ms);
                v_flex()
                    .w(px(150.))
                    .flex_shrink_0()
                    .gap(px(4.))
                    .p(px(12.))
                    .rounded(px(10.))
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border_variant)
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.text_placeholder)
                            .child(iso_date(&h.date).to_owned()),
                    )
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::BOLD)
                            .child(lap_time(h.time_ms)),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .child(self.tt_face(&h.driver, 18., p))
                            .child(div().truncate().text_size(px(12.)).child(h.driver.name.clone())),
                    )
                    .children(step.map(|s| {
                        div()
                            .text_size(px(11.))
                            .text_color(p.success)
                            .child(format!("-{}", sector_time(s)))
                    }))
                    .into_any_element()
            })
            .collect();
        Some(
            v_flex()
                .gap(px(10.))
                .child(section_title(tr("RECORD HISTORY"), p))
                .child(h_flex().gap(px(10.)).overflow_x_hidden().children(points))
                .into_any_element(),
        )
    }

    pub(super) fn setup_button(&self, id: &str, lap: &str, has: bool, cx: &mut Context<Self>) -> AnyElement {
        let lap = lap.to_owned();
        Button::new(SharedString::from(format!("setup-{id}")))
            .ghost()
            .tap_xs()
            .icon(IconName::Wrench)
            .disabled(!has)
            .tooltip(if has {
                tr("Show the car setup")
            } else {
                tr("No setup was recorded for this lap")
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                let same = this.online.setup_open.as_deref() == Some(lap.as_str());
                this.online.setup_open = (!same).then(|| lap.clone());
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn setup_panel(
        &self,
        setup: &Setup,
        mine: Option<&Setup>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let compare = self.online.setup_compare && mine.is_some();
        let text = setup.text();
        let groups: Vec<AnyElement> = SETUP_GROUPS
            .iter()
            .filter_map(|(title, rows)| {
                let lines: Vec<AnyElement> = rows
                    .iter()
                    .filter_map(|(key, label, unit)| {
                        let v = setup.get(key)?;
                        let diff = mine.filter(|_| compare).and_then(|m| setup.difference(m, key));
                        Some(
                            h_flex()
                                .gap(px(8.))
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(p.text_muted)
                                        .child(*label),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(setup_value_text(v, *unit)),
                                )
                                .children(diff.map(|d| {
                                    div()
                                        .w(px(52.))
                                        .text_size(px(11.))
                                        .text_color(if d > 0. { p.warning } else { p.accent_hover })
                                        .child(format!(
                                            "{}{}",
                                            if d > 0. { "+" } else { "" },
                                            setup_value_text(d, *unit)
                                        ))
                                }))
                                .into_any_element(),
                        )
                    })
                    .collect();
                let compound = (*title == "Tyres").then(|| setup.compound.clone()).flatten();
                if lines.is_empty() && compound.is_none() {
                    return None;
                }
                Some(
                    v_flex()
                        .w(px(260.))
                        .gap(px(6.))
                        .p(px(12.))
                        .rounded(px(10.))
                        .bg(p.surface)
                        .border_1()
                        .border_color(p.border_variant)
                        .child(section_title(tr(title), p))
                        .children(compound.map(|c| {
                            h_flex()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(p.text_muted)
                                        .child(tr("Compound")),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(crate::format::title_case(&c)),
                                )
                        }))
                        .children(lines)
                        .into_any_element(),
                )
            })
            .collect();
        v_flex()
            .gap(px(10.))
            .px(px(14.))
            .py(px(12.))
            .bg(p.background)
            .border_t_1()
            .border_color(p.border_variant)
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().flex_1().text_size(px(12.)).text_color(p.text_muted).child(
                        if setup.changed {
                            tr("The setup changed during this lap; these are the values it started with.")
                        } else {
                            tr("Car setup")
                        },
                    ))
                    .children(mine.map(|_| {
                        Button::new("setup-compare")
                            .outline()
                            .tap_sm()
                            .label(tr("Compare with mine"))
                            .when(compare, |b| b.primary())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.online.setup_compare = !this.online.setup_compare;
                                cx.notify();
                            }))
                    }))
                    .child(
                        Button::new("setup-copy")
                            .outline()
                            .tap_sm()
                            .icon(IconName::Copy)
                            .label(tr("Copy"))
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))
                            }),
                    ),
            )
            .child(h_flex().flex_wrap().items_start().gap(px(10.)).children(groups))
            .into_any_element()
    }
}

fn head_cell(width: f32, text: &'static str, p: Palette) -> Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text_placeholder)
        .child(text)
}
