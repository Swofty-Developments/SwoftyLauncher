use crate::ui::scale::{Hit, fixed};
use crate::ui::theme::Palette;
use crate::ui::{logo, wordmark};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_i18n::tr;

use super::fit::{self, Account};
use super::overflow::Link;
use super::{BAR_HEIGHT, Popover, SUB_HEIGHT, filled_icon};
use crate::shell::tabs::{Row, Tab};
use crate::shell::{Page, Shell};

impl Shell {
    pub(in crate::shell) fn top_bar(
        &self,
        p: Palette,
        layout_width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let labels: Vec<&str> = Tab::ALL.iter().map(|t| t.label()).collect();
        let step = fit::choose(
            layout_width,
            &labels,
            self.account.display_name().map(|n| n.chars().count()),
            BAR_HEIGHT,
        );
        let collapsed = step.collapsed();
        let account = step.account();
        let current = self.navigation.page.tab();
        let focus = self
            .navigation
            .cursor
            .filter(|c| c.row == Row::Tabs)
            .map(|c| c.col);
        let left = h_flex()
            .id("notch-left")
            .flex_shrink_0()
            .items_center()
            .gap(fixed(10.))
            .child(logo(42.))
            .when(step.wordmark(), |d| d.child(wordmark(84.)));
        let tabs = if collapsed {
            self.tab_menu(current, focus, p, cx).into_any_element()
        } else {
            h_flex()
                .gap(fixed(2.))
                .children(Tab::ALL.iter().enumerate().map(|(i, tab)| {
                    self.tab_button(*tab, Some(*tab) == current, focus == Some(i), step.tight(), p, cx)
                }))
                .into_any_element()
        };
        let action_focus = |a: usize| focus == Some(Tab::ALL.len() + a);
        let notices = self.notices().len() + cx.global::<crate::shell::notices::Notices>().unread();
        crate::ui::title_bar(p)
            .h(fixed(BAR_HEIGHT))
            .pl(fixed(0.))
            .border_0()
            .child(
                h_flex()
                    .id("top-bar")
                    .relative()
                    .w_full()
                    .h_full()
                    .items_stretch()
                    .pl(fixed(14.))
                    .gap(fixed(10.))
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
                            .gap(fixed(4.))
                            .mr(fixed(6.))
                            .child(self.bell(notices, action_focus(0), p, cx))
                            .child(
                                self.icon_button(
                                    "open-settings",
                                    IconName::Settings,
                                    tr("Settings"),
                                    action_focus(1),
                                    p,
                                )
                                .when(
                                    self.navigation.page == Page::Settings
                                        || self.navigation.page == Page::Tools,
                                    |d| d.bg(p.selected),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.navigation.cursor = None;
                                    this.navigation.popover = None;
                                    this.open_page(Page::Settings, cx);
                                })),
                            ),
                    )
                    .child(self.account_block(account, action_focus(2), p, cx)),
            )
    }

    pub(super) fn icon_button(
        &self,
        id: &'static str,
        icon: IconName,
        tip: &'static str,
        focused: bool,
        p: Palette,
    ) -> Stateful<Div> {
        h_flex()
            .id(id)
            .size(fixed(44.))
            .items_center()
            .justify_center()
            .rounded(fixed(8.))
            .border_1()
            .cursor_pointer()
            .occlude()
            .text_color(p.text_muted)
            .border_color(if focused { p.accent } else { transparent_black() })
            .hover(move |s| s.bg(p.hover).text_color(p.text))
            .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
            .child(Icon::new(icon).size(fixed(24.)))
    }

    pub(super) fn bell(
        &self,
        count: usize,
        focused: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let open = self.navigation.popover == Some(Popover::Notices);
        div()
            .relative()
            .child(
                self.icon_button("open-notices", IconName::Bell, tr("Notifications"), focused, p)
                    .when(open, |d| d.bg(p.selected))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.navigation.cursor = None;
                        this.open_popover(Popover::Notices, cx);
                    })),
            )
            .when(count > 0, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(fixed(1.))
                        .right(fixed(0.))
                        .min_w(fixed(15.))
                        .h(fixed(15.))
                        .px(fixed(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(p.accent)
                        .text_color(p.on_accent)
                        .text_size(fixed(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(count.to_string()),
                )
            })
    }

    pub(super) fn account_block(
        &self,
        account: Account,
        focused: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ring = move |d: Stateful<Div>| d.when(focused, |d| d.border_2().border_color(p.accent));
        match self.account.display_name() {
            Some(name) => {
                let face = match &self.account.avatar {
                    Some(image) => div()
                        .size(fixed(BAR_HEIGHT))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .child(img(image.clone()).size_full())
                        .into_any_element(),
                    None => div()
                        .size(fixed(BAR_HEIGHT))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(p.elevated)
                        .text_size(fixed(26.))
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
                        .gap(fixed(16.))
                        .pr(fixed(if account == Account::IconOnly { 8. } else { 24. }))
                        .bg(p.surface)
                        .cursor_pointer()
                        .hover(move |s| s.bg(p.hover)),
                )
                .child(face)
                .children((account != Account::IconOnly).then(|| {
                    div()
                        .max_w(fixed(if account == Account::Small { 110. } else { 190. }))
                        .truncate()
                        .text_size(fixed(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(name)
                }))
                .children((account != Account::IconOnly).then(|| {
                    Icon::new(IconName::ChevronDown)
                        .size(fixed(18.))
                        .text_color(p.text_muted)
                }))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.navigation.cursor = None;
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
                    .gap(fixed(10.))
                    .px(fixed(20.))
                    .cursor_pointer()
                    .text_size(fixed(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff))
                    .bg(rgb(crate::ui::discord::BLURPLE))
                    .hover(|s| s.bg(rgb(crate::ui::discord::BLURPLE_HOVER)))
                    .when(self.account.busy || self.game_busy(), |d| d.opacity(0.5))
                    .when(self.game_busy(), |d| {
                        d.tooltip(|window, cx| {
                            Tooltip::new(tr("Close the game to sign in or out")).build(window, cx)
                        })
                    }),
            )
            .child(
                svg()
                    .data(crate::ui::discord::LOGO)
                    .size(fixed(18.))
                    .text_color(rgb(0xffffff)),
            )
            .children((account != Account::IconOnly).then(|| {
                if self.account.busy {
                    tr("Signing in…")
                } else if account == Account::Small {
                    tr("Log in")
                } else {
                    tr("Log in with Discord")
                }
            }))
            .on_click(cx.listener(|this, _, _, cx| crate::shell::account::Account::sign_in(this, cx)))
            .into_any_element(),
        }
    }

    pub(super) fn tab_button(
        &self,
        tab: Tab,
        active: bool,
        focused: bool,
        tight: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .id(SharedString::from(format!("tab-{}", tab.label())))
            .h(fixed(BAR_HEIGHT - 8.))
            .px(fixed(if tight { 10. } else { 22. }))
            .gap(fixed(4.))
            .items_center()
            .justify_center()
            .rounded(fixed(8.))
            .border_1()
            .cursor_pointer()
            .occlude()
            .text_size(fixed(13.))
            .font_weight(FontWeight::MEDIUM)
            .map(|d| {
                let d = if active {
                    d.text_color(rgb(0xffffff))
                } else {
                    let hint = tab.tint(&p).opacity(0.85);
                    d.text_color(p.text_muted).hover(move |s| s.text_color(hint))
                };
                d.border_color(if focused { p.accent } else { transparent_black() })
            })
            .child(match filled_icon(tab.icon(), cx).filter(|_| active) {
                Some(data) => svg()
                    .data(&data)
                    .size(fixed(26.))
                    .text_color(tab.tint(&p))
                    .into_any_element(),
                None => Icon::new(tab.icon()).size(fixed(26.)).into_any_element(),
            })
            .child(tab.label())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.navigation.cursor = None;
                this.navigation.popover = None;
                let page = if this.navigation.page.tab() == Some(tab) {
                    this.navigation.page
                } else {
                    this.tab_home(tab)
                };
                this.open_page(page, cx);
            }))
            .into_any_element()
    }

    pub(super) fn tab_menu(
        &self,
        current: Option<Tab>,
        focus: Option<usize>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let shown = focus
            .filter(|i| *i < Tab::ALL.len())
            .map(|i| Tab::ALL[i])
            .or(current)
            .map_or(tr("Menu"), |t| t.label());
        let shell = cx.entity().downgrade();
        Button::new("tab-menu")
            .outline()
            .h(fixed(34.))
            .label(shown)
            .dropdown_caret(true)
            .when(focus.is_some(), |b| b.border_color(p.accent))
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |mut menu, _, _| {
                for tab in Tab::ALL {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(tab.label())
                            .checked(Some(tab) == current)
                            .on_click(move |_, _, cx| {
                                shell
                                    .update(cx, |s, cx| {
                                        let page = if s.navigation.page.tab() == Some(tab) {
                                            s.navigation.page
                                        } else {
                                            s.tab_home(tab)
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

    pub(in crate::shell) fn sub_row(
        &self,
        p: Palette,
        layout_width: f32,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let nav = self.sub_nav(layout_width)?;
        let focus = self
            .navigation
            .cursor
            .filter(|c| c.row == Row::Links)
            .map(|c| c.col);
        let inline_len = nav.inline.len();
        let more_active =
            nav.more.iter().any(|l| l.page == self.navigation.page) || focus.is_some_and(|f| f >= inline_len);
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
                    .w(fixed(1.))
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
            let current = self.navigation.page;
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
                        .tap_sm()
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
                .top(fixed(BAR_HEIGHT))
                .left_0()
                .right_0()
                .justify_center()
                .child(
                    h_flex()
                        .id("sub-notch")
                        .h(fixed(SUB_HEIGHT))
                        .items_stretch()
                        .gap(fixed(26.))
                        .px(fixed(30.))
                        .rounded_b(fixed(16.))
                        .bg(p.chrome)
                        .children(links)
                        .children(more),
                ),
        )
    }

    pub(super) fn link(&self, link: &Link, focused: bool, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.navigation.page == link.page;
        let underline = self.navigation.page.tab().map_or(p.accent, |t| t.tint(&p));
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
            .gap(fixed(7.))
            .px(fixed(2.))
            .cursor_pointer()
            .border_b_2()
            .text_size(fixed(13.))
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
                    underline
                } else {
                    transparent_black()
                })
            })
            .when(focused && !active, |d| d.bg(p.hover))
            .child(div().whitespace_nowrap().child(link.label.clone()))
            .children(link.count.map(|n| {
                div()
                    .px(fixed(6.))
                    .rounded_full()
                    .bg(if active { p.selected } else { p.elevated })
                    .text_size(fixed(11.))
                    .text_color(if active { p.accent_hover } else { p.text_muted })
                    .child(n.to_string())
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.navigation.cursor = None;
                this.open_page(page, cx);
            }))
            .into_any_element()
    }
}
