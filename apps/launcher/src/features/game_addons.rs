//! The Library of a game that keeps its addons in its own store: each addon with its switch, its folder and
//! Uninstall, installs from a file, a folder or a drop, and a clean page when there are none.

use crate::shell::Shell;
use crate::ui::scale::px;
use crate::ui::theme::Palette;
use crate::ui::tokens::space::GUTTER;
use crate::ui::{EmptyState, PageHeader, Toggle, badge};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::games::Installed;
use swofty_i18n::{tr, trf, trn};

impl Shell {
    fn side_install_button(&self, id: &'static str, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let shell = cx.entity().downgrade();
        Button::new(id)
            .outline()
            .icon(IconName::Plus)
            .label(tr("Install addon…"))
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                let (file, folder) = (shell.clone(), shell.clone());
                menu.item(
                    PopupMenuItem::new(tr("Choose a file…"))
                        .icon(IconName::File)
                        .on_click(move |_, window, cx| {
                            file.update(cx, |s, cx| s.pick_side_addons(false, window, cx))
                                .ok();
                        }),
                )
                .item(
                    PopupMenuItem::new(tr("Choose a folder…"))
                        .icon(IconName::FolderOpen)
                        .on_click(move |_, window, cx| {
                            folder
                                .update(cx, |s, cx| s.pick_side_addons(true, window, cx))
                                .ok();
                        }),
                )
            })
    }

    pub(crate) fn game_addons_page(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let root = self.store.root.clone();
        let Some(side) = self.games.side() else {
            return div().into_any_element();
        };
        let name = side.def.info.name;
        let folder = side.shelf.addons_dir(&root);
        let listing = side.listing.clone();
        let on = side.on();
        let subtitle = match listing.addons.len() {
            0 => trf!("Addons for {game} you install show here", game = name),
            n => trf!(
                "{addons} for {game} · {on} on · loaded in this order at Play",
                addons = trn(n, "{count} addon", "{count} addons"),
                game = name,
                on = on
            ),
        };
        let open_folder = {
            let folder = folder.clone();
            Button::new("side-open-folder")
                .ghost()
                .icon(IconName::FolderOpen)
                .label(tr("Open folder"))
                .on_click(move |_, _, cx| {
                    std::fs::create_dir_all(&folder).ok();
                    crate::services::reveal::show(&folder, cx);
                })
        };
        let actions = h_flex()
            .gap(px(8.))
            .child(open_folder)
            .child(self.side_install_button("side-install", cx));
        let header = PageHeader::new(trf!("{game} addons", game = name))
            .subtitle(subtitle)
            .actions(actions);
        if listing.addons.is_empty() && listing.unreadable.is_empty() {
            let browse = Button::new("side-empty-browse")
                .primary()
                .icon(IconName::PackageSearch)
                .label(tr("Open the Addon Browser"))
                .on_click(cx.listener(|this, _, _, cx| this.open_page(crate::shell::Page::Browser, cx)));
            return v_flex()
                .size_full()
                .child(header)
                .child(
                    EmptyState::new(IconName::Puzzle, trf!("No {game} addons yet", game = name))
                        .body(trf!(
                            "Drop an addon's zip or folder on the launcher, choose one with Install addon, or find one in the Addon Browser. Addons you install for {game} come on and load with the Swofty Core when you press Play.",
                            game = name
                        ))
                        .action(
                            h_flex()
                                .gap(px(8.))
                                .child(browse)
                                .child(self.side_install_button("side-empty-install", cx)),
                        ),
                )
                .into_any_element();
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        for (i, a) in listing.addons.iter().enumerate() {
            rows.push(self.side_addon_row(i == 0, a, p, cx));
        }
        for (i, u) in listing.unreadable.iter().enumerate() {
            rows.push(
                self.row(rows.is_empty() && i == 0, false, p)
                    .child(
                        h_flex()
                            .h(px(54.))
                            .px(px(14.))
                            .gap(px(12.))
                            .child(badge(IconName::TriangleAlert, 34., p).text_color(p.warning))
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .child(div().text_size(px(14.)).child(u.id.clone()))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(p.warning)
                                            .truncate()
                                            .child(trf!("Can't be used: {why}", why = u.reason)),
                                    ),
                            ),
                    )
                    .into_any_element(),
            );
        }
        v_flex()
            .size_full()
            .child(header)
            .child(self.list("side-addons", rows, p))
            .child(
                div()
                    .px(px(GUTTER))
                    .pb(px(14.))
                    .text_size(px(12.))
                    .text_color(p.text_placeholder)
                    .child(trf!("Installed in {folder}", folder = folder.display())),
            )
            .into_any_element()
    }

    fn side_addon_row(&self, first: bool, a: &Installed, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let m = &a.manifest;
        let id = m.id.clone();
        let icon = a
            .icon
            .as_ref()
            .and_then(|path| std::fs::read(path).ok())
            .map(|bytes| std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, bytes)));
        let icon = match icon {
            Some(image) => div()
                .size(px(34.))
                .flex_shrink_0()
                .rounded(px(8.))
                .overflow_hidden()
                .child(img(image).size_full())
                .into_any_element(),
            None => badge(IconName::Puzzle, 34., p).into_any_element(),
        };
        let author = (!m.author.is_empty()).then(|| trf!("by {author}", author = m.author));
        let toggle = {
            let id = id.clone();
            Toggle::new(format!("side-toggle-{id}"), a.enabled).on_change(
                cx.listener(move |this, on: &bool, window, cx| this.set_side_addon(&id, *on, window, cx)),
            )
        };
        let menu = {
            let (shell, dir, id) = (cx.entity().downgrade(), a.dir.clone(), id.clone());
            Button::new(format!("side-menu-{id}"))
                .ghost()
                .icon(IconName::Ellipsis)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
                    let (dir, shell, id) = (dir.clone(), shell.clone(), id.clone());
                    menu.item(
                        PopupMenuItem::new(tr("Open folder"))
                            .icon(IconName::FolderOpen)
                            .on_click(move |_, _, cx| crate::services::reveal::show(&dir, cx)),
                    )
                    .separator()
                    .item(
                        PopupMenuItem::new(tr("Uninstall"))
                            .icon(IconName::Trash)
                            .on_click(move |_, window, cx| {
                                shell
                                    .update(cx, |s, cx| s.uninstall_side_addon(&id, window, cx))
                                    .ok();
                            }),
                    )
                })
        };
        self.row(first, false, p)
            .child(
                h_flex()
                    .id(SharedString::from(format!("side-row-{id}")))
                    .min_h(px(62.))
                    .px(px(14.))
                    .py(px(8.))
                    .gap(px(12.))
                    .child(icon)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(2.))
                            .child(
                                h_flex()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_size(px(14.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .when(!a.enabled, |d| d.text_color(p.text_muted))
                                            .child(m.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(p.text_placeholder)
                                            .child(m.version.to_string()),
                                    )
                                    .children(author.map(|a| {
                                        div().text_size(px(12.)).text_color(p.text_placeholder).child(a)
                                    })),
                            )
                            .children(m.description.clone().map(|d| {
                                div()
                                    .text_size(px(12.))
                                    .text_color(p.text_muted)
                                    .truncate()
                                    .child(d)
                            })),
                    )
                    .child(menu)
                    .child(toggle),
            )
            .into_any_element()
    }
}
