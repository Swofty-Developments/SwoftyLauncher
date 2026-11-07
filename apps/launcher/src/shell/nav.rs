use crate::library::{AddonIcon, SectionItem};
use crate::pages::{addon_icon, label};
use crate::scale::px;
use crate::theme::{Palette, SCROLLBAR_GAP, ScrollArea};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::sync::Arc;
use swofty_i18n::tr;

use super::{Page, Shell};

pub(super) enum NavArt {
    Icon(IconName),
    Glyph(Arc<[u8]>),
    Image(Arc<Image>),
}

fn nav_art(art: NavArt, size: f32, active: bool, p: Palette) -> Div {
    let color = if active { p.on_accent } else { p.text_muted };
    let frame = || {
        div()
            .size(px(size))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(size * 0.25))
            .bg(if active { p.accent } else { p.surface })
    };
    match art {
        NavArt::Image(image) => addon_icon(&AddonIcon::Image(image), size, p),
        NavArt::Glyph(data) => frame().child(svg().data(&data).size(px(size * 0.55)).text_color(color)),
        NavArt::Icon(icon) => frame().child(Icon::new(icon).size(px(size * 0.55)).text_color(color)),
    }
}

fn panel_art(art: NavArt, active: bool, p: Palette) -> Div {
    let color = if active {
        p.accent_hover
    } else {
        p.text_placeholder
    };
    let frame = div()
        .size(px(18.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center();
    match art {
        NavArt::Image(image) => frame.child(addon_icon(&AddonIcon::Image(image), 18., p)),
        NavArt::Glyph(data) => frame.child(svg().data(&data).size(px(16.)).text_color(color)),
        NavArt::Icon(icon) => frame.child(Icon::new(icon).size(px(16.)).text_color(color)),
    }
}

impl Shell {
    pub(crate) fn cancel_scroll(&self) {
        self.news_scroll.cancel();
        self.addons.cancel_scroll();
        self.browser.smooth.cancel();
        self.db.rows_motion.cancel();
        self.db.list_motion.cancel();
    }

    pub(crate) fn open_page(&mut self, page: Page, cx: &mut Context<Self>) {
        if self.page != page {
            self.cancel_scroll();
        }
        self.db_page_change(page, cx);
        let was = self.page;
        self.page = page;
        if page == Page::Shaders {
            self.sync_shaders(true, cx);
        }
        if page == Page::Careers && was != Page::Careers {
            self.refresh_saves_in_background(cx);
        }
        if let Some(i) = self.open_section().filter(|i| !self.section_loaded(*i)) {
            self.fetch_views(Some(i), cx);
        }
        cx.notify();
    }

    pub(super) fn section_loaded(&self, i: usize) -> bool {
        self.library
            .sections
            .get(i)
            .is_some_and(|s| self.views.contains_key(&format!("{}/{}", s.addon, s.spec.id)))
    }

    pub(super) fn big_nav(
        &self,
        page: Page,
        count: Option<usize>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let art = NavArt::Icon(page.icon());
        self.big_row(page, page.nav_label().into(), art, count, p, cx)
    }

    pub(super) fn profile_pointer(&self, p: Palette) -> Div {
        v_flex().flex_shrink_0().items_center().gap(px(2.)).child(
            h_flex()
                .w_full()
                .gap(px(8.))
                .items_center()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(p.accent)
                .bg(p.selected)
                .text_size(px(12.))
                .text_color(p.text)
                .child(
                    Icon::new(IconName::ArrowDown)
                        .size(px(14.))
                        .flex_shrink_0()
                        .text_color(p.accent),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(tr("Switch profiles here to see their saves")),
                ),
        )
    }

    pub(super) fn sub_nav(&self, page: Page, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let active = self.page == page;
        h_flex()
            .id(SharedString::from(format!("nav-{}", page.nav_label())))
            .h(px(32.))
            .flex_shrink_0()
            .ml(px(18.))
            .pl(px(6.))
            .pr(px(10.))
            .gap(px(8.))
            .rounded(px(8.))
            .border_1()
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .map(|d| {
                if active {
                    d.bg(p.selected).border_color(p.accent).text_color(p.text)
                } else {
                    d.border_color(transparent_black())
                        .text_color(p.text_muted)
                        .hover(move |s| s.bg(p.hover).text_color(p.text))
                }
            })
            .child(nav_art(NavArt::Icon(page.icon()), 22., active, p))
            .child(div().flex_1().truncate().child(page.nav_label()))
            .on_click(cx.listener(move |this, _, _, cx| this.open_page(page, cx)))
            .into_any_element()
    }

    pub(super) fn big_row(
        &self,
        page: Page,
        title: SharedString,
        art: NavArt,
        count: Option<usize>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.page == page;
        h_flex()
            .id(SharedString::from(format!("nav-{title}")))
            .h(px(40.))
            .flex_shrink_0()
            .pl(px(6.))
            .pr(px(10.))
            .gap(px(10.))
            .rounded(px(8.))
            .border_1()
            .cursor_pointer()
            .text_size(px(14.))
            .font_weight(FontWeight::MEDIUM)
            .map(|d| {
                if active {
                    d.bg(p.selected).border_color(p.accent).text_color(p.text)
                } else {
                    d.border_color(transparent_black())
                        .text_color(p.text_muted)
                        .hover(move |s| s.bg(p.hover).text_color(p.text))
                }
            })
            .child(nav_art(art, 28., active, p))
            .child(div().flex_1().truncate().child(title))
            .children(count.map(|n| {
                div()
                    .text_size(px(12.))
                    .text_color(if active { p.text_muted } else { p.text_placeholder })
                    .child(n.to_string())
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.open_page(page, cx)))
            .into_any_element()
    }

    /// A section's sidebar art: its `sections/<id>.svg`, else its addon's icon, else
    /// the icon its manifest names.
    pub(super) fn section_art(&self, s: &SectionItem) -> NavArt {
        let addon = self.library.addons.iter().find(|a| a.manifest.id == s.addon);
        match s.icon.clone().or_else(|| addon.and_then(|a| a.icon.clone())) {
            Some(AddonIcon::Glyph(data)) => NavArt::Glyph(data),
            Some(AddonIcon::Image(image)) => NavArt::Image(image),
            None => NavArt::Icon(crate::sections::icon(s.spec.icon)),
        }
    }

    /// Simplified for Swofty's pages, in the order its manifest lists them: the
    /// importer, as Importer with the import icon, and every other page with its
    /// own title and art. Until the addon is on, just the importer, which offers
    /// to install or turn it on.
    pub(super) fn simplified(&self, p: Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let mut importer = false;
        for (i, s) in self.library.sections.iter().enumerate() {
            if !s.is_simplified() || s.is_importer_tracks() {
                continue;
            }
            if s.is_importer() {
                importer = true;
                rows.push(self.big_nav(Page::Importer, None, p, cx));
                continue;
            }
            let art = self.section_art(s);
            let count = self.section_count(i);
            rows.push(self.big_row(Page::Section(i), s.spec.title.clone().into(), art, count, p, cx));
        }
        if !importer {
            rows.push(self.big_nav(Page::Importer, None, p, cx));
        }
        rows
    }

    pub(super) fn panel_nav(
        &self,
        page: Page,
        title: SharedString,
        art: NavArt,
        count: Option<usize>,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.page == page;
        h_flex()
            .id(SharedString::from(format!("panel-{title}")))
            .h(px(32.))
            .flex_shrink_0()
            .px(px(8.))
            .gap(px(10.))
            .rounded(px(6.))
            .border_1()
            .cursor_pointer()
            .text_size(px(13.))
            .map(|d| {
                if active {
                    d.bg(p.selected).border_color(p.accent).text_color(p.text)
                } else {
                    d.border_color(transparent_black())
                        .text_color(p.text_muted)
                        .hover(move |s| s.bg(p.hover).text_color(p.text))
                }
            })
            .child(panel_art(art, active, p))
            .child(div().flex_1().truncate().child(title))
            .children(count.map(|n| {
                div()
                    .text_size(px(11.))
                    .text_color(p.text_placeholder)
                    .child(n.to_string())
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.open_page(page, cx)))
            .into_any_element()
    }

    /// The addon panels the sidebar lists: the DB Editor, then every addon
    /// page except Simplified for Swofty's, which have their own heading.
    pub(super) fn panels(&self, p: Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let db = Page::Database;
        let mut rows = vec![self.panel_nav(
            db,
            db.label().into(),
            NavArt::Icon(db.icon()),
            Some(self.db_count()).filter(|n| *n > 0),
            p,
            cx,
        )];
        for (i, s) in self.library.sections.iter().enumerate() {
            if s.is_simplified() {
                continue;
            }
            let art = self.section_art(s);
            rows.push(self.panel_nav(
                Page::Section(i),
                s.spec.title.clone().into(),
                art,
                self.section_count(i),
                p,
                cx,
            ));
        }
        rows
    }

    pub(super) fn sidebar(&self, p: Palette, short: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let mut big = vec![
            (Page::News, None),
            (Page::Addons, Some(self.library.listed().count())),
            (Page::Browser, None),
            (Page::Shaders, None),
            (Page::Profiles, Some(self.library.profiles.len())),
            (Page::Careers, None),
        ];
        if self.developer_mode {
            big.insert(3, (Page::Tools, None));
        }
        if self.mp_allowed() {
            big.push((Page::Multiplayer, None));
        }
        big.push((Page::Settings, None));
        v_flex()
            .w(px(crate::scale::SIDEBAR_WIDTH))
            .h_full()
            .flex_shrink_0()
            .px(px(10.))
            .pt(px(4.))
            .pb(px(10.))
            .gap(px(8.))
            .bg(p.chrome)
            .child(
                h_flex()
                    .flex_shrink_0()
                    .justify_center()
                    .pt(px(2.))
                    .pb(px(if short { 6. } else { 14. }))
                    .child(crate::pages::wordmark(if short { 112. } else { 148. })),
            )
            .child(
                v_flex().flex_1().min_h_0().mr(px(-SCROLLBAR_GAP)).child(
                    v_flex()
                        .flex_1()
                        .min_h_0()
                        .scroll_y("sidebar")
                        .pr(px(SCROLLBAR_GAP))
                        .gap(px(8.))
                        .child(v_flex().flex_shrink_0().gap(px(2.)).children(big.into_iter().map(
                            |(page, count)| match page {
                                Page::Careers => self.sub_nav(page, p, cx),
                                _ => self.big_nav(page, count, p, cx),
                            },
                        )))
                        .child(
                            v_flex()
                                .flex_shrink_0()
                                .pt(px(if short { 4. } else { 12. }))
                                .gap(px(6.))
                                .child(label(tr("SIMPLIFIED FOR SWOFTY"), p).px(px(8.)))
                                .children(self.simplified(p, cx)),
                        )
                        .child(
                            v_flex()
                                .flex_shrink_0()
                                .pt(px(if short { 4. } else { 12. }))
                                .gap(px(6.))
                                .child(label(tr("ADDON PANELS"), p).px(px(8.)))
                                .child(v_flex().gap(px(1.)).children(self.panels(p, cx))),
                        ),
                ),
            )
            .child(crate::donate::button())
            .child(self.send_logs_button(p, cx))
            .child(
                v_flex()
                    .flex_shrink_0()
                    .gap(px(6.))
                    .pt(px(4.))
                    .pb(px(2.))
                    .children((self.page == Page::Careers).then(|| self.profile_pointer(p)))
                    .child(label(tr("ACTIVE PROFILE"), p).px(px(4.)))
                    .child(self.profile_picker(p, cx)),
            )
    }
}
