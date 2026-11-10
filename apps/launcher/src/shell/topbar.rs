use crate::pages::{avatar, logo, wordmark};
use crate::scale::px;
use crate::theme::Palette;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::{tr, trn};

use super::overflow::{Link, split_overflow};
use super::tabs::{ACTIONS, Action, Cursor, Key, Row, Step, Tab};
use super::{Page, Shell};

pub(super) const COLLAPSE_BELOW: f32 = 1040.;
const COMPACT_BELOW: f32 = 1180.;
pub(crate) const BAR_HEIGHT: f32 = 68.;
pub(crate) const SUB_HEIGHT: f32 = 42.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Popover {
    Notices,
    Account,
}

pub(crate) struct Notice {
    pub icon: IconName,
    pub text: String,
    pub page: Option<Page>,
}

pub(crate) struct SubNav {
    pub split: Option<usize>,
    pub inline: Vec<Link>,
    pub more: Vec<Link>,
}

impl SubNav {
    fn flat(&self) -> Vec<Link> {
        self.inline.iter().chain(&self.more).cloned().collect()
    }
}

impl Shell {
    pub(crate) fn nav_mp(&self) -> bool {
        self.mp_allowed() || (cfg!(debug_assertions) && std::env::var_os("SWOFTY_FAKE_MP").is_some())
    }

    pub(crate) fn account_name(&self) -> Option<String> {
        self.mp.as_ref().map(|s| s.name.clone()).or_else(|| {
            cfg!(debug_assertions)
                .then(|| std::env::var("SWOFTY_FAKE_SIGNED_IN").ok())
                .flatten()
        })
    }

    pub(crate) fn sub_nav(&self, layout_width: f32) -> Option<SubNav> {
        let tab = self.page.tab();
        let (links, fixed) = match tab {
            Some(Tab::Library) => {
                let mut links = vec![
                    Link::new(Page::Addons, Some(self.library.listed().count())),
                    Link::new(Page::Shaders, None),
                    Link::new(Page::Careers, None),
                ];
                links.extend(self.addon_page_links());
                (links, 3)
            }
            Some(Tab::Multiplayer) if self.nav_mp() => (
                vec![Link::new(Page::Online, None), Link::new(Page::Multiplayer, None)],
                2,
            ),
            None if self.developer_mode => (
                vec![Link::new(Page::Settings, None), Link::new(Page::Tools, None)],
                2,
            ),
            _ => return None,
        };
        let (inline, more) = split_overflow(links, fixed, (layout_width - 260.).max(300.));
        let split = (tab == Some(Tab::Library) && inline.len() + more.len() > fixed).then_some(fixed);
        Some(SubNav { split, inline, more })
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

    pub(crate) fn notices(&self) -> Vec<Notice> {
        let mut out: Vec<Notice> = self
            .launcher_notices()
            .into_iter()
            .map(|text| Notice {
                icon: IconName::CircleArrowUp,
                text,
                page: None,
            })
            .collect();
        let n = self.addon_updates().len();
        if n > 0 {
            out.push(Notice {
                icon: IconName::Download,
                text: trn(
                    n,
                    "{count} addon update available",
                    "{count} addon updates available",
                ),
                page: Some(Page::Browser),
            });
        }
        out
    }

    fn open_popover(&mut self, which: Popover, cx: &mut Context<Self>) {
        self.popover = (self.popover != Some(which)).then_some(which);
        if self.popover == Some(Popover::Notices) {
            cx.update_global(|n: &mut crate::notices::Notices, _| {
                n.mark_read();
                n.dismiss_popups();
            });
        }
        cx.notify();
    }

    pub(crate) fn nav_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        let plain = !ev.keystroke.modifiers.control && !ev.keystroke.modifiers.platform;
        if key == "escape" && self.popover.is_some() {
            self.popover = None;
            cx.notify();
            return;
        }
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
        let links = self
            .sub_nav(self.layout_width)
            .map(|s| s.flat())
            .unwrap_or_default();
        let moved = match key {
            "left" => Some(Key::Left),
            "right" => Some(Key::Right),
            "up" => Some(Key::Up),
            "down" => Some(Key::Down),
            _ => None,
        };
        cx.stop_propagation();
        if let Some(k) = moved {
            if cursor.apply(k, Tab::ALL.len() + ACTIONS, links.len()) == Step::IntoPage {
                self.nav = None;
                self.popover = None;
                window.focus(&self.root_focus, cx);
            } else {
                self.nav = Some(cursor);
            }
        } else if key == "enter" || key == "space" {
            match cursor.row {
                Row::Tabs if cursor.col < Tab::ALL.len() => {
                    let tab = Tab::ALL[cursor.col];
                    let page = if self.page.tab() == Some(tab) {
                        self.page
                    } else {
                        tab.home()
                    };
                    self.open_page(page, cx);
                }
                Row::Tabs => match Action::ALL[cursor.col - Tab::ALL.len()] {
                    Action::Notifications => self.open_popover(Popover::Notices, cx),
                    Action::Settings => self.open_page(Page::Settings, cx),
                    Action::Account => {
                        if self.account_name().is_some() {
                            self.open_popover(Popover::Account, cx);
                        } else {
                            self.mp_sign_in(window, cx);
                        }
                    }
                },
                Row::Links => {
                    if let Some(l) = links.get(cursor.col) {
                        self.open_page(l.page, cx);
                    }
                }
            }
        } else if key == "escape" {
            self.nav = None;
            window.focus(&self.root_focus, cx);
        }
        cx.notify();
    }

    pub(super) fn top_bar(&self, p: Palette, layout_width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = layout_width < COLLAPSE_BELOW;
        let compact = layout_width < COMPACT_BELOW;
        let current = self.page.tab();
        let focus = self.nav.filter(|c| c.row == Row::Tabs).map(|c| c.col);
        let left = h_flex()
            .id("notch-left")
            .flex_shrink_0()
            .items_center()
            .gap(px(10.))
            .child(logo(42.))
            .when(!collapsed, |d| d.child(wordmark(84.)));
        let tabs =
            if collapsed {
                self.tab_menu(current, focus, p, cx).into_any_element()
            } else {
                h_flex()
                    .gap(px(2.))
                    .children(Tab::ALL.iter().enumerate().map(|(i, tab)| {
                        self.tab_button(*tab, Some(*tab) == current, focus == Some(i), p, cx)
                    }))
                    .into_any_element()
            };
        let action_focus = |a: usize| focus == Some(Tab::ALL.len() + a);
        let notices = self.notices().len() + cx.global::<crate::notices::Notices>().unread();
        crate::ui::title_bar(p)
            .h(px(BAR_HEIGHT))
            .pl(px(0.))
            .border_0()
            .child(
                h_flex()
                    .id("top-bar")
                    .relative()
                    .w_full()
                    .h_full()
                    .items_stretch()
                    .pl(px(14.))
                    .gap(px(10.))
                    .child(
                        h_flex()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h_full()
                            .items_center()
                            .justify_center()
                            .child(tabs),
                    )
                    .child(div().flex().items_center().child(left))
                    .child(div().flex_1())
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(4.))
                            .mr(px(6.))
                            .child(self.bell(notices, action_focus(0), p, cx))
                            .child(
                                self.icon_button(
                                    "open-settings",
                                    IconName::Settings,
                                    tr("Settings"),
                                    action_focus(1),
                                    p,
                                )
                                .when(self.page == Page::Settings || self.page == Page::Tools, |d| {
                                    d.bg(p.selected)
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.nav = None;
                                    this.popover = None;
                                    this.open_page(Page::Settings, cx);
                                })),
                            ),
                    )
                    .child(self.account_block(compact, action_focus(2), p, cx)),
            )
    }

    fn icon_button(
        &self,
        id: &'static str,
        icon: IconName,
        tip: &'static str,
        focused: bool,
        p: Palette,
    ) -> Stateful<Div> {
        h_flex()
            .id(id)
            .size(px(44.))
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .border_1()
            .cursor_pointer()
            .occlude()
            .text_color(p.text_muted)
            .border_color(if focused { p.accent } else { transparent_black() })
            .hover(move |s| s.bg(p.hover).text_color(p.text))
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            .child(Icon::new(icon).size(px(24.)))
    }

    fn bell(&self, count: usize, focused: bool, p: Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.popover == Some(Popover::Notices);
        div()
            .relative()
            .child(
                self.icon_button("open-notices", IconName::Bell, tr("Notifications"), focused, p)
                    .when(open, |d| d.bg(p.selected))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = None;
                        this.open_popover(Popover::Notices, cx);
                    })),
            )
            .when(count > 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(1.))
                        .right(px(0.))
                        .min_w(px(15.))
                        .h(px(15.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(p.accent)
                        .text_color(p.on_accent)
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(count.to_string()),
                )
            })
    }

    fn account_block(&self, compact: bool, focused: bool, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let ring = move |d: Stateful<Div>| d.when(focused, |d| d.border_2().border_color(p.accent));
        match self.account_name() {
            Some(name) => {
                let face = match &self.avatar {
                    Some(image) => div()
                        .size(px(BAR_HEIGHT))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .child(img(image.clone()).size_full())
                        .into_any_element(),
                    None => div()
                        .size(px(BAR_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(p.elevated)
                        .text_size(px(26.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(
                            name.chars()
                                .next()
                                .map(|c| c.to_uppercase().to_string())
                                .unwrap_or_default(),
                        )
                        .into_any_element(),
                };
                ring(
                    h_flex()
                        .id("account-block")
                        .occlude()
                        .h_full()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(16.))
                        .pr(px(24.))
                        .bg(p.surface)
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.hover)),
                )
                .child(face)
                .child(
                    div()
                        .max_w(px(if compact { 110. } else { 190. }))
                        .truncate()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(18.))
                        .text_color(p.text_muted),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.nav = None;
                    this.open_popover(Popover::Account, cx);
                }))
                .into_any_element()
            }
            None => ring(
                h_flex()
                    .id("mp-login")
                    .occlude()
                    .h_full()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(10.))
                    .px(px(20.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .bg(rgb(crate::discord::BLURPLE))
                    .hover(|s| s.bg(rgb(crate::discord::BLURPLE_HOVER)))
                    .when(self.mp_busy, |d| d.opacity(0.6)),
            )
            .child(
                svg()
                    .data(crate::discord::LOGO)
                    .size(px(18.))
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

    fn tab_button(
        &self,
        tab: Tab,
        active: bool,
        focused: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mp = self.nav_mp();
        v_flex()
            .id(SharedString::from(format!("tab-{}", tab.label(mp))))
            .h(px(BAR_HEIGHT - 8.))
            .px(px(22.))
            .gap(px(4.))
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .border_1()
            .cursor_pointer()
            .occlude()
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .map(|d| {
                let d = if active {
                    d.bg(p.selected).text_color(p.text)
                } else {
                    d.text_color(p.text_muted)
                        .hover(move |s| s.bg(p.hover).text_color(p.text))
                };
                d.border_color(if focused { p.accent } else { transparent_black() })
            })
            .child(Icon::new(tab.icon(mp)).size(px(26.)))
            .child(tab.label(mp))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.nav = None;
                this.popover = None;
                let page = if this.page.tab() == Some(tab) {
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
        current: Option<Tab>,
        focus: Option<usize>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mp = self.nav_mp();
        let shown = focus
            .filter(|i| *i < Tab::ALL.len())
            .map(|i| Tab::ALL[i])
            .or(current)
            .map_or(tr("Menu"), |t| t.label(mp));
        let shell = cx.entity().downgrade();
        Button::new("tab-menu")
            .outline()
            .h(px(34.))
            .label(shown)
            .dropdown_caret(true)
            .when(focus.is_some(), |b| b.border_color(p.accent))
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for tab in Tab::ALL {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tab.label(mp))
                            .checked(Some(tab) == current)
                            .on_click(move |_, _, cx| {
                                shell
                                    .update(cx, |s, cx| {
                                        let page = if s.page.tab() == Some(tab) {
                                            s.page
                                        } else {
                                            tab.home()
                                        };
                                        s.open_page(page, cx);
                                    })
                                    .ok();
                            }),
                    );
                }
                menu
            })
    }

    pub(super) fn sub_row(
        &self,
        p: Palette,
        layout_width: f32,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let nav = self.sub_nav(layout_width)?;
        let focus = self.nav.filter(|c| c.row == Row::Links).map(|c| c.col);
        let inline_len = nav.inline.len();
        let more_active =
            nav.more.iter().any(|l| l.page == self.page) || focus.is_some_and(|f| f >= inline_len);
        let mut links: Vec<AnyElement> = nav
            .inline
            .iter()
            .enumerate()
            .map(|(i, l)| self.link(l, focus == Some(i), p, cx))
            .collect();
        if let Some(at) = nav.split.filter(|at| *at <= links.len()) {
            links.insert(
                at,
                div()
                    .id("sub-divider")
                    .w(px(1.))
                    .h_full()
                    .flex_shrink_0()
                    .bg(p.border)
                    .into_any_element(),
            );
        }
        let more = (!nav.more.is_empty()).then(|| {
            let shell = cx.entity().downgrade();
            let items: Vec<(SharedString, Page)> =
                nav.more.iter().map(|l| (l.label.clone(), l.page)).collect();
            let current = self.page;
            h_flex()
                .h_full()
                .items_center()
                .border_b_2()
                .border_color(if more_active {
                    p.accent
                } else {
                    transparent_black()
                })
                .child(
                    Button::new("sub-more")
                        .ghost()
                        .small()
                        .label(tr("More"))
                        .dropdown_caret(true)
                        .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                            for (label, page) in &items {
                                let (shell, page) = (shell.clone(), *page);
                                menu = menu.item(
                                    PopupMenuItem::new(label.clone())
                                        .checked(page == current)
                                        .on_click(move |_, _, cx| {
                                            shell.update(cx, |s, cx| s.open_page(page, cx)).ok();
                                        }),
                                );
                            }
                            menu
                        }),
                )
        });
        Some(
            h_flex()
                .id("sub-row")
                .absolute()
                .top(px(BAR_HEIGHT))
                .left_0()
                .right_0()
                .justify_center()
                .child(
                    h_flex()
                        .id("sub-notch")
                        .h(px(SUB_HEIGHT))
                        .items_stretch()
                        .gap(px(22.))
                        .px(px(26.))
                        .rounded_b(px(14.))
                        .border_1()
                        .border_t_0()
                        .border_color(p.border)
                        .bg(p.surface)
                        .children(links)
                        .children(more),
                ),
        )
    }

    fn link(&self, link: &Link, focused: bool, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.page == link.page;
        let page = link.page;
        let key = match page {
            Page::Section(i) => i + 100,
            _ => 0,
        };
        h_flex()
            .id(SharedString::from(format!("link-{}-{key}", link.label)))
            .h_full()
            .flex_shrink_0()
            .items_center()
            .gap(px(7.))
            .px(px(2.))
            .cursor_pointer()
            .border_b_2()
            .text_size(px(13.))
            .font_weight(if active {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::MEDIUM
            })
            .map(|d| {
                let d = if active {
                    d.text_color(p.text)
                } else {
                    d.text_color(p.text_muted).hover(move |s| s.text_color(p.text))
                };
                d.border_color(if active || focused {
                    p.accent
                } else {
                    transparent_black()
                })
            })
            .when(focused && !active, |d| d.bg(p.hover))
            .child(div().whitespace_nowrap().child(link.label.clone()))
            .children(link.count.map(|n| {
                div()
                    .px(px(6.))
                    .rounded_full()
                    .bg(if active { p.selected } else { p.elevated })
                    .text_size(px(11.))
                    .text_color(if active { p.accent_hover } else { p.text_muted })
                    .child(n.to_string())
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.nav = None;
                this.open_page(page, cx);
            }))
            .into_any_element()
    }

    pub(super) fn popover_layer(&self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let which = self.popover?;
        let panel = match which {
            Popover::Notices => self.notices_panel(p, cx),
            Popover::Account => self.account_panel(p, cx),
        };
        Some(
            div()
                .id("popover-layer")
                .absolute()
                .inset_0()
                .occlude()
                .on_click(cx.listener(|this, _, _, cx| {
                    this.popover = None;
                    cx.notify();
                }))
                .child(div().absolute().top(px(BAR_HEIGHT)).right(px(8.)).child(panel))
                .into_any_element(),
        )
    }

    fn panel(&self, p: Palette) -> Stateful<Div> {
        v_flex()
            .id("popover-panel")
            .w(px(340.))
            .rounded(px(8.))
            .border_1()
            .border_color(p.border)
            .bg(p.elevated)
            .overflow_hidden()
            .on_click(|_, _, cx| cx.stop_propagation())
    }

    fn account_panel(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let name = self.account_name().unwrap_or_default();
        self.panel(p)
            .w(px(220.))
            .child(
                h_flex()
                    .gap(px(10.))
                    .p(px(12.))
                    .items_center()
                    .child(avatar(&name, 28., p).rounded_full())
                    .child(div().truncate().font_weight(FontWeight::MEDIUM).child(name)),
            )
            .child(
                Button::new("mp-logout")
                    .ghost()
                    .icon(IconName::LogOut)
                    .label(tr("Log out"))
                    .disabled(self.mp_busy)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.popover = None;
                        this.mp_sign_out(window, cx);
                    })),
            )
            .into_any_element()
    }

    fn notices_panel(&self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let status = self.notices();
        let history: Vec<crate::notices::Entry> = cx.global::<crate::notices::Notices>().entries().to_vec();
        let empty = status.is_empty() && history.is_empty();
        let mut rows: Vec<AnyElement> = Vec::new();
        for (i, n) in status.into_iter().enumerate() {
            let page = n.page;
            rows.push(
                h_flex()
                    .id(SharedString::from(format!("notice-{i}")))
                    .gap(px(10.))
                    .p(px(12.))
                    .items_start()
                    .border_b_1()
                    .border_color(p.border_variant)
                    .when(page.is_some(), |d| {
                        d.cursor_pointer().hover(move |s| s.bg(p.hover))
                    })
                    .child(
                        Icon::new(n.icon)
                            .size(px(16.))
                            .text_color(p.accent)
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .child(n.text),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.popover = None;
                        if let Some(page) = page {
                            this.open_page(page, cx);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        for e in &history {
            rows.push(self.history_row(e, p));
        }
        let body: Vec<AnyElement> = if empty {
            vec![
                v_flex()
                    .items_center()
                    .gap(px(8.))
                    .py(px(26.))
                    .child(
                        Icon::new(IconName::Bell)
                            .size(px(22.))
                            .text_color(p.text_placeholder),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(p.text_muted)
                            .child(tr("No notifications")),
                    )
                    .into_any_element(),
            ]
        } else {
            rows
        };
        self.panel(p)
            .child(
                h_flex()
                    .px(px(12.))
                    .py(px(6.))
                    .items_center()
                    .border_b_1()
                    .border_color(p.border_variant)
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(p.text_muted)
                            .child(tr("NOTIFICATIONS")),
                    )
                    .when(!history.is_empty(), |d| {
                        d.child(
                            Button::new("clear-notices")
                                .ghost()
                                .small()
                                .label(tr("Clear all"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    cx.update_global(|n: &mut crate::notices::Notices, _| n.clear());
                                    this.popover = None;
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(v_flex().max_h(px(480.)).overflow_y_hidden().children(body))
            .into_any_element()
    }

    fn history_row(&self, e: &crate::notices::Entry, p: Palette) -> AnyElement {
        use crate::notices::Kind;
        let (icon, tone) = match e.note.kind {
            Kind::Info => (IconName::Info, p.accent),
            Kind::Success => (IconName::CircleCheck, p.success),
            Kind::Warning => (IconName::TriangleAlert, p.warning),
            Kind::Error => (IconName::CircleX, p.error),
        };
        let action = e.note.action.clone();
        h_flex()
            .id(SharedString::from(format!("history-{}", e.id)))
            .gap(px(10.))
            .p(px(12.))
            .items_start()
            .border_b_1()
            .border_color(p.border_variant)
            .child(Icon::new(icon).size(px(16.)).text_color(tone).flex_shrink_0())
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .children(e.note.title.clone().map(|t| {
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t)
                    }))
                    .child(
                        div()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .text_color(if e.note.title.is_some() {
                                p.text_muted
                            } else {
                                p.text
                            })
                            .child(e.note.message.clone()),
                    )
                    .children(action.map(|(label, run)| {
                        Button::new(SharedString::from(format!("history-action-{}", e.id)))
                            .outline()
                            .small()
                            .label(label)
                            .on_click(move |_, window, cx| run(window, cx))
                    })),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.text_placeholder)
                    .flex_shrink_0()
                    .child(crate::format::ago(e.at.elapsed().as_secs_f64())),
            )
            .into_any_element()
    }

    pub(super) fn popup_layer(&mut self, p: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.popover == Some(Popover::Notices) {
            return None;
        }
        let now = std::time::Instant::now();
        let popups: Vec<crate::notices::Entry> = cx
            .global::<crate::notices::Notices>()
            .popups(now)
            .into_iter()
            .cloned()
            .collect();
        if popups.is_empty() {
            return None;
        }
        if !self.popup_timer {
            self.popup_timer = true;
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1200))
                    .await;
                this.update(cx, |s, cx| {
                    s.popup_timer = false;
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        Some(
            v_flex()
                .id("popups")
                .absolute()
                .top(px(BAR_HEIGHT
                    + 6.
                    + if self.notch_beside {
                        crate::download_dock::NOTCH_HEIGHT + 6.
                    } else {
                        0.
                    }))
                .right(px(8.))
                .w(px(340.))
                .gap(px(6.))
                .children(popups.iter().map(|e| {
                    let id = e.id;
                    div()
                        .id(SharedString::from(format!("popup-{id}")))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.elevated)
                        .overflow_hidden()
                        .cursor_pointer()
                        .occlude()
                        .child(self.history_row(e, p))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.update_global(|n: &mut crate::notices::Notices, _| n.dismiss_popup(id));
                            cx.notify();
                        }))
                }))
                .into_any_element(),
        )
    }
}
