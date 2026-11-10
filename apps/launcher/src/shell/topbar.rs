use crate::pages::{avatar, logo};
use crate::scale::px;
use crate::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::tr;

use super::tabs::{Cursor, Key, Row, Step, Tab};
use super::{Page, Shell};

pub(super) const COLLAPSE_BELOW: f32 = 900.;
const COMPACT_BELOW: f32 = 1180.;

#[derive(Clone)]
pub(crate) struct Link {
    pub page: Page,
    pub label: SharedString,
    pub count: Option<usize>,
}

impl Link {
    fn new(page: Page, count: Option<usize>) -> Self {
        Self {
            page,
            label: page.sub_label().into(),
            count,
        }
    }
}

impl Shell {
    pub(crate) fn nav_groups(&self) -> Vec<Vec<Link>> {
        match self.page.tab() {
            Tab::News | Tab::Shaders => Vec::new(),
            Tab::Online => {
                let mut links = vec![Link::new(Page::Online, None)];
                if self.mp_allowed() {
                    links.push(Link::new(Page::Multiplayer, None));
                }
                vec![links]
            }
            Tab::Settings => {
                let mut links = vec![Link::new(Page::Settings, None)];
                if self.developer_mode {
                    links.push(Link::new(Page::Tools, None));
                }
                vec![links]
            }
            Tab::Addons => {
                let main = vec![
                    Link::new(Page::Addons, Some(self.library.listed().count())),
                    Link::new(Page::Browser, None),
                    Link::new(Page::Profiles, Some(self.library.profiles.len())),
                    Link::new(Page::Careers, None),
                ];
                vec![main, self.addon_page_links()]
            }
        }
    }

    fn addon_page_links(&self) -> Vec<Link> {
        let mut links = Vec::new();
        let mut importer = false;
        for (i, s) in self.library.sections.iter().enumerate() {
            if !s.is_simplified() || s.is_importer_tracks() {
                continue;
            }
            if s.is_importer() {
                importer = true;
                links.push(Link::new(Page::Importer, None));
                continue;
            }
            links.push(Link {
                page: Page::Section(i),
                label: s.spec.title.clone().into(),
                count: self.section_count(i),
            });
        }
        if !importer {
            links.push(Link::new(Page::Importer, None));
        }
        links.push(Link::new(
            Page::Database,
            Some(self.db_count()).filter(|n| *n > 0),
        ));
        for (i, s) in self.library.sections.iter().enumerate() {
            if s.is_simplified() {
                continue;
            }
            links.push(Link {
                page: Page::Section(i),
                label: s.spec.title.clone().into(),
                count: self.section_count(i),
            });
        }
        links
    }

    pub(crate) fn nav_flat(&self) -> Vec<Link> {
        self.nav_groups().into_iter().flatten().collect()
    }

    pub(crate) fn nav_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        let plain = !ev.keystroke.modifiers.control && !ev.keystroke.modifiers.platform;
        let Some(mut cursor) = self.nav else {
            let wanted = key == "up"
                && plain
                && (self.ui_mode.is_some_and(|m| m.is_tv()) || ev.keystroke.modifiers.alt);
            if wanted && self.root_focus.is_focused(window) {
                self.nav = Some(Cursor::on_tab(self.page.tab()));
                cx.stop_propagation();
                cx.notify();
            }
            return;
        };
        let links = self.nav_flat();
        let moved = match key {
            "left" => Some(Key::Left),
            "right" => Some(Key::Right),
            "up" => Some(Key::Up),
            "down" => Some(Key::Down),
            _ => None,
        };
        cx.stop_propagation();
        if let Some(k) = moved {
            if cursor.apply(k, Tab::ALL.len(), links.len()) == Step::IntoPage {
                self.nav = None;
                window.focus(&self.root_focus, cx);
            } else {
                self.nav = Some(cursor);
            }
        } else if key == "enter" || key == "space" {
            let page = match cursor.row {
                Row::Tabs => {
                    let tab = Tab::ALL[cursor.col.min(Tab::ALL.len() - 1)];
                    if tab == self.page.tab() {
                        self.page
                    } else {
                        tab.home()
                    }
                }
                Row::Links => links.get(cursor.col).map_or(self.page, |l| l.page),
            };
            self.open_page(page, cx);
            if cursor.row == Row::Tabs {
                self.nav = Some(Cursor::on_tab(page.tab()));
            }
        } else if key == "escape" {
            self.nav = None;
            window.focus(&self.root_focus, cx);
        }
        cx.notify();
    }

    fn notch(&self, p: Palette) -> Div {
        h_flex()
            .h(px(34.))
            .flex_shrink_0()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border_variant)
            .bg(p.surface)
    }

    pub(super) fn top_bar(&self, p: Palette, layout_width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = layout_width < COLLAPSE_BELOW;
        let compact = layout_width < COMPACT_BELOW;
        let current = self.page.tab();
        let focus = self.nav.filter(|c| c.row == Row::Tabs).map(|c| c.col);
        let left = self
            .notch(p)
            .id("notch-left")
            .child(logo(22.))
            .when(!collapsed, |d| {
                d.child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text)
                        .child(tr("Swofty Launcher")),
                )
            });
        let tabs = if collapsed {
            self.tab_menu(current, focus, p, cx).into_any_element()
        } else {
            h_flex()
                .gap(px(2.))
                .children(
                    Tab::ALL
                        .iter()
                        .enumerate()
                        .map(|(i, tab)| self.tab_button(*tab, *tab == current, focus == Some(i), p, cx)),
                )
                .into_any_element()
        };
        h_flex()
            .id("top-bar")
            .h(px(46.))
            .flex_shrink_0()
            .items_center()
            .px(px(10.))
            .gap(px(10.))
            .bg(p.chrome)
            .child(left)
            .child(h_flex().flex_1().min_w_0().justify_center().child(tabs))
            .child(self.account_notch(compact, p, cx))
    }

    fn tab_button(
        &self,
        tab: Tab,
        active: bool,
        focused: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .id(SharedString::from(format!("tab-{}", tab.label())))
            .h(px(34.))
            .px(px(14.))
            .gap(px(8.))
            .items_center()
            .rounded(px(10.))
            .border_1()
            .cursor_pointer()
            .text_size(px(14.))
            .font_weight(FontWeight::MEDIUM)
            .map(|d| {
                let d = if active {
                    d.bg(p.selected).text_color(p.text)
                } else {
                    d.text_color(p.text_muted)
                        .hover(move |s| s.bg(p.hover).text_color(p.text))
                };
                d.border_color(if focused {
                    p.accent
                } else if active {
                    p.border
                } else {
                    transparent_black()
                })
            })
            .child(Icon::new(tab.icon()).size(px(15.)).text_color(if active {
                p.accent_hover
            } else {
                p.text_placeholder
            }))
            .child(tab.label())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.nav = None;
                let page = if this.page.tab() == tab {
                    this.page
                } else {
                    tab.home()
                };
                this.open_page(page, cx);
            }))
            .into_any_element()
    }

    fn tab_menu(
        &self,
        current: Tab,
        focus: Option<usize>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shown = focus.map_or(current, |i| Tab::ALL[i.min(Tab::ALL.len() - 1)]);
        let shell = cx.entity().downgrade();
        Button::new("tab-menu")
            .outline()
            .h(px(34.))
            .icon(shown.icon())
            .label(shown.label())
            .dropdown_caret(true)
            .when(focus.is_some(), |b| b.border_color(p.accent))
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for tab in Tab::ALL {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tab.label())
                            .icon(tab.icon())
                            .checked(tab == current)
                            .on_click(move |_, _, cx| {
                                shell
                                    .update(cx, |s, cx| {
                                        let page = if s.page.tab() == tab { s.page } else { tab.home() };
                                        s.open_page(page, cx);
                                    })
                                    .ok();
                            }),
                    );
                }
                menu
            })
    }

    fn account_notch(&self, compact: bool, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        match &self.mp {
            Some(sess) => self
                .notch(p)
                .id("notch-account")
                .child(avatar(&sess.name, 22., p).rounded_full())
                .child(
                    div()
                        .max_w(px(if compact { 90. } else { 150. }))
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(sess.name.clone()),
                )
                .child(
                    Button::new("mp-logout")
                        .ghost()
                        .small()
                        .icon(IconName::LogOut)
                        .tooltip(tr("Log out"))
                        .disabled(self.mp_busy)
                        .on_click(cx.listener(|this, _, window, cx| this.mp_sign_out(window, cx))),
                )
                .into_any_element(),
            None => h_flex()
                .id("mp-login")
                .h(px(34.))
                .flex_shrink_0()
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .rounded(px(10.))
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(0xffffff))
                .bg(rgb(crate::discord::BLURPLE))
                .hover(|s| s.bg(rgb(crate::discord::BLURPLE_HOVER)))
                .when(self.mp_busy, |d| d.opacity(0.6))
                .child(
                    svg()
                        .data(crate::discord::LOGO)
                        .size(px(16.))
                        .text_color(rgb(0xffffff)),
                )
                .child(if self.mp_busy {
                    tr("Signing in…")
                } else if compact {
                    tr("Log in")
                } else {
                    tr("Log in with Discord")
                })
                .on_click(cx.listener(|this, _, window, cx| this.mp_sign_in(window, cx)))
                .into_any_element(),
        }
    }

    pub(super) fn sub_row(&self, p: Palette, layout_width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let compact = layout_width < COMPACT_BELOW;
        let groups = self.nav_groups();
        let focus = self.nav.filter(|c| c.row == Row::Links).map(|c| c.col);
        let mut index = 0;
        let mut items: Vec<AnyElement> = Vec::new();
        for (g, group) in groups.iter().enumerate() {
            for link in group {
                if !items.is_empty() {
                    items.push(delimiter(g > 0 && index == group_start(&groups, g), p));
                }
                items.push(self.link(link, focus == Some(index), p, cx));
                index += 1;
            }
        }
        let overflow = self.nav_scroll.max_offset().x > px(0.);
        let scroll = self.nav_scroll.clone();
        let step = |dir: f32, id: &'static str, icon: IconName| {
            let scroll = scroll.clone();
            Button::new(id)
                .ghost()
                .small()
                .icon(icon)
                .on_click(move |_, _, cx| {
                    let at = scroll.offset();
                    scroll.set_offset(point(
                        (at.x + px(dir * 260.)).min(px(0.)).max(-scroll.max_offset().x),
                        at.y,
                    ));
                    cx.refresh_windows();
                })
        };
        let show_links = groups.iter().map(Vec::len).sum::<usize>() > 1 || self.page.tab() == Tab::Online;
        h_flex()
            .id("sub-row")
            .h(px(40.))
            .flex_shrink_0()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .bg(p.chrome)
            .when(show_links, |d| {
                d.child(
                    h_flex()
                        .id("sub-links")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .items_center()
                        .overflow_x_scroll()
                        .track_scroll(&self.nav_scroll)
                        .on_scroll(cx.listener(|this, ev: &ScrollEvent, window, cx| {
                            let delta = ev.delta.pixel_delta(window.line_height());
                            let at = this.nav_scroll.offset();
                            let max = this.nav_scroll.max_offset().x;
                            let x = (at.x + delta.x + delta.y).min(px(0.)).max(-max);
                            this.nav_scroll.set_offset(point(x, at.y));
                            cx.notify();
                        }))
                        .children(items),
                )
                .when(overflow, |d| {
                    d.child(step(1., "sub-left", IconName::ChevronLeft)).child(step(
                        -1.,
                        "sub-right",
                        IconName::ChevronRight,
                    ))
                })
            })
            .when(!show_links, |d| d.child(div().flex_1()))
            .child(self.send_logs_chip(compact, p, cx))
            .child(crate::donate::button())
    }

    fn send_logs_chip(&self, compact: bool, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("send-logs")
            .h(px(34.))
            .flex_shrink_0()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .rounded(px(8.))
            .border_1()
            .border_color(p.border_variant)
            .bg(p.surface)
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .text_color(p.text_muted)
            .hover(move |s| s.bg(p.hover).border_color(p.border).text_color(p.text))
            .tooltip(|window, cx| Tooltip::new(crate::send_logs::label()).build(window, cx))
            .on_click(cx.listener(|this, _, window, cx| this.send_logs(window, cx)))
            .child(Icon::new(crate::send_logs::icon()).size(px(15.)).flex_shrink_0())
            .when(!compact, |d| d.child(crate::send_logs::label()))
    }

    fn link(&self, link: &Link, focused: bool, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.page == link.page;
        let page = link.page;
        h_flex()
            .id(SharedString::from(format!(
                "link-{}-{}",
                link.label,
                self.link_key(page)
            )))
            .h(px(30.))
            .flex_shrink_0()
            .items_center()
            .gap(px(6.))
            .px(px(10.))
            .rounded(px(8.))
            .border_1()
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .map(|d| {
                let d = if active {
                    d.text_color(p.text).bg(p.selected)
                } else {
                    d.text_color(p.text_muted)
                        .hover(move |s| s.text_color(p.text).bg(p.hover))
                };
                d.border_color(if focused { p.accent } else { transparent_black() })
            })
            .child(div().whitespace_nowrap().child(link.label.clone()))
            .children(link.count.map(|n| {
                div()
                    .text_size(px(11.))
                    .text_color(p.text_placeholder)
                    .child(n.to_string())
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.nav = None;
                this.open_page(page, cx);
            }))
            .into_any_element()
    }

    fn link_key(&self, page: Page) -> usize {
        match page {
            Page::Section(i) => i + 100,
            _ => 0,
        }
    }
}

fn group_start(groups: &[Vec<Link>], g: usize) -> usize {
    groups[..g].iter().map(Vec::len).sum()
}

fn delimiter(between_groups: bool, p: Palette) -> AnyElement {
    div()
        .flex_shrink_0()
        .mx(px(if between_groups { 10. } else { 2. }))
        .text_size(px(15.))
        .text_color(if between_groups { p.text_muted } else { p.border })
        .child("|")
        .into_any_element()
}
