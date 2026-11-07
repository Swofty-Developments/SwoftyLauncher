use super::drag::{Dragged, drop_mark};
use super::model::AddonListItem;
use crate::library::AddonItem;
use crate::pages::{badge, label};
use crate::scale::px;
use crate::shell::Shell;
use crate::theme::{Palette, palette};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::HashMap;
use swofty_core::folders::{Entry, Key};
use swofty_core::profile::VANILLA;

impl Shell {
    pub(super) fn addon_list(
        &mut self,
        query: &str,
        highlight: Option<&(String, f32)>,
        split: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !cx.has_active_drag() {
            self.addons.drop = None;
        }
        let vanilla = self.vanilla();
        let filtering = !query.is_empty() || !self.addons.facets.is_empty();
        let (positions, _) = self.positions();
        let indices: HashMap<&str, usize> = self
            .library
            .addons
            .iter()
            .enumerate()
            .map(|(i, a)| (a.manifest.id.as_str(), i))
            .collect();
        let listed: HashMap<&str, &AddonItem> = self
            .library
            .listed()
            .map(|a| (a.manifest.id.as_str(), a))
            .collect();
        let total = listed.len();
        let passes = |id: &str| listed.get(id).is_some_and(|m| self.passes(m, query, None));
        let matched = listed.keys().filter(|id| passes(id)).count();
        let mut children = vec![AddonListItem::Heading];
        let mut scroll_to = None;
        let mut push =
            |children: &mut Vec<AddonListItem>, id: &str, at: usize, nested: Option<(Key, usize)>| {
                let Some(&index) = indices.get(id) else {
                    return;
                };
                if highlight.is_some_and(|(h, _)| h == id) && self.addons.scroll_pending {
                    scroll_to = Some(children.len());
                }
                children.push(AddonListItem::Addon {
                    index,
                    position: positions.get(id).copied(),
                    at,
                    nested,
                    flagged: self.problems_for(id).len(),
                });
                if !split && self.open_addon.as_deref() == Some(id) {
                    children.push(AddonListItem::Detail(id.to_owned()));
                }
            };
        for (at, entry) in self.library.entries.iter().enumerate() {
            match entry {
                Entry::Addon(id) if passes(id) => push(&mut children, id, at, None),
                Entry::Addon(_) => {}
                Entry::Folder(key, members) => {
                    let inside: Vec<&AddonItem> = members
                        .iter()
                        .filter_map(|id| listed.get(id.as_str()).copied())
                        .collect();
                    let matching: Vec<(usize, &String)> =
                        members.iter().enumerate().filter(|(_, id)| passes(id)).collect();
                    if matching.is_empty() && (filtering || !matches!(key, Key::Own(_))) {
                        continue;
                    }
                    let open = filtering || !self.library.folders.is_collapsed(key);
                    children.push(AddonListItem::Folder {
                        key: key.clone(),
                        at,
                        count: inside.len(),
                        on: inside.iter().filter(|m| self.addon_on(m)).count(),
                        open,
                    });
                    if open {
                        for (j, id) in matching {
                            push(&mut children, id, at, Some((key.clone(), j)));
                        }
                    }
                }
            }
        }
        if !vanilla {
            children.push(AddonListItem::End);
        }
        let needle = query.trim().to_lowercase();
        let broken: Vec<usize> = (0..self.library.broken.len())
            .filter(|&i| {
                let u = &self.library.broken[i];
                needle.is_empty() || u.name.to_lowercase().contains(&needle) || u.id.contains(&needle)
            })
            .collect();
        if !broken.is_empty() {
            children.push(AddonListItem::BrokenHeading);
            children.extend(broken.into_iter().map(AddonListItem::Broken));
        }
        if self.addons.list_items.len() == children.len() {
            // Same rows with new contents, such as a switch or a folder's count: keep the scroll.
            for (i, (old, new)) in self.addons.list_items.iter().zip(&children).enumerate() {
                if old != new || matches!(new, AddonListItem::Detail(_)) {
                    self.addons.scroll.remeasure_items(i..i + 1);
                }
            }
            if self.addons.list_items != children {
                self.addons.list_items = children;
                self.addons.drop = None;
            }
        } else {
            let old = &self.addons.list_items;
            let prefix = old.iter().zip(&children).take_while(|(a, b)| a == b).count();
            let suffix = old[prefix..]
                .iter()
                .rev()
                .zip(children[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            self.addons
                .scroll
                .splice(prefix..old.len() - suffix, children.len() - prefix - suffix);
            self.addons.list_items = children;
            self.addons.drop = None;
        }
        if let Some(ix) = scroll_to {
            self.addons.smooth.cancel();
            self.addons.scroll_pending = false;
            // Off-screen rows have no measured height yet; address the item directly.
            self.addons.scroll.scroll_to(ListOffset {
                item_ix: ix,
                offset_in_item: px(0.),
            });
        }
        let highlight = highlight.cloned();
        let header = h_flex()
            .h(px(28.))
            .gap(px(8.))
            .flex_shrink_0()
            .text_size(px(12.))
            .text_color(p.text_muted)
            .child(Icon::new(IconName::Layers).size(px(13.)))
            .child(div().flex_1().min_w_0().truncate().child(if vanilla {
                format!("Switch away from {VANILLA} to use addons")
            } else {
                "Lower in the list wins when two addons change the same thing".to_owned()
            }))
            .when(matched != total, |d| {
                d.child(
                    div()
                        .flex_shrink_0()
                        .text_color(p.text_placeholder)
                        .child(format!("{matched} of {total}")),
                )
            })
            .when(!vanilla && self.library.not_in_profile().next().is_some(), |d| {
                d.child(
                    Button::new("add-to-profile")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Plus)
                        .label("Add addons")
                        .on_click(cx.listener(|this, _, window, cx| this.add_to_profile(window, cx))),
                )
            })
            .when(!vanilla, |d| {
                d.child(
                    Button::new("new-folder")
                        .ghost()
                        .xsmall()
                        .icon(IconName::FolderPlus)
                        .label("New folder")
                        .on_click(cx.listener(|this, _, window, cx| this.new_folder(window, cx))),
                )
            });
        let body = if matched == 0 {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .py(px(40.))
                .rounded(px(8.))
                .border_1()
                .border_color(p.border_variant)
                .child(badge(IconName::Search, 36., p))
                .child(
                    div()
                        .pt(px(4.))
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("No addons match"),
                )
                .child(
                    Button::new("empty-clear")
                        .ghost()
                        .small()
                        .label("Clear filters")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.addons.facets.clear();
                            this.addons.clear_search(cx);
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .rounded(px(8.))
                .border_1()
                .border_color(p.border)
                .overflow_hidden()
                .on_drag_move(cx.listener(|this, e: &DragMoveEvent<Dragged>, _, cx| {
                    if !e.bounds.contains(&e.event.position) && this.addons.drop.take().is_some() {
                        cx.notify();
                    }
                }))
                .child(
                    list(
                        self.addons.scroll.clone(),
                        cx.processor(move |this, i, _, cx| {
                            let p = palette(cx);
                            let item = match this.addons.list_items.get(i).cloned() {
                                Some(AddonListItem::Heading) => this.list_heading(p),
                                Some(AddonListItem::Folder {
                                    key, count, on, open, ..
                                }) => this.folder_row(&key, count, on, open, p, cx),
                                Some(AddonListItem::Addon {
                                    index,
                                    position,
                                    nested,
                                    ..
                                }) => {
                                    let m = &this.library.addons[index];
                                    let fade = highlight
                                        .as_ref()
                                        .filter(|(id, _)| *id == m.manifest.id)
                                        .map(|(_, k)| *k);
                                    this.list_row(m, position, nested.is_some(), fade, p, cx)
                                }
                                Some(AddonListItem::Detail(id)) => this.addon_detail(&id, true, p, cx),
                                Some(AddonListItem::End) => div()
                                    .h(px(30.))
                                    .px(px(12.))
                                    .flex()
                                    .items_center()
                                    .text_size(px(12.))
                                    .text_color(p.text_placeholder)
                                    .when(cx.has_active_drag(), |d| d.child("Drop here to move to the end"))
                                    .into_any_element(),
                                Some(AddonListItem::BrokenHeading) => v_flex()
                                    .px(px(12.))
                                    .pt(px(14.))
                                    .pb(px(6.))
                                    .child(label("CAN'T BE USED", p))
                                    .into_any_element(),
                                Some(AddonListItem::Broken(i)) => this.broken_row(i, p, cx),
                                None => div().into_any_element(),
                            };
                            let mark = this
                                .addons
                                .drop
                                .as_ref()
                                .filter(|d| d.ix == i)
                                .map(|d| drop_mark(d.edge, d.nested, p));
                            div()
                                .w_full()
                                .relative()
                                .child(item)
                                .children(mark)
                                .when(!this.vanilla(), |d| {
                                    d.on_drag_move(cx.listener(
                                        move |this, e: &DragMoveEvent<Dragged>, _, cx| {
                                            if !e.bounds.contains(&e.event.position) {
                                                return;
                                            }
                                            let frac =
                                                (e.event.position.y - e.bounds.top()) / e.bounds.size.height;
                                            let dragging = e.drag(cx).item.clone();
                                            let drop = this.drop_for(i, frac, &dragging);
                                            if drop != this.addons.drop {
                                                this.addons.drop = drop;
                                                cx.notify();
                                            }
                                        },
                                    ))
                                    .on_drop(cx.listener(
                                        |this, d: &Dragged, window, cx| this.apply_drop(d, window, cx),
                                    ))
                                })
                                .into_any_element()
                        }),
                    )
                    .flex_1()
                    .min_h_0()
                    .pb(px(6.)),
                )
                .child(self.addons.smooth.overlay(&self.addons.scroll, cx))
                .into_any_element()
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap(px(6.))
            .child(header)
            .child(body)
            .into_any_element()
    }

    /// An installed addon this launcher can't read: its name, why, and Remove.
    pub(super) fn broken_row(&self, i: usize, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(u) = self.library.broken.get(i) else {
            return div().into_any_element();
        };
        let (id, name) = (u.id.clone(), u.name.clone());
        let reason = format!("{}.", crate::update::sentence(&format!("it {}", u.reason)));
        h_flex()
            .id(SharedString::from(format!("broken-{id}")))
            .min_h(px(54.))
            .px(px(14.))
            .py(px(8.))
            .gap(px(12.))
            .border_t_1()
            .border_color(p.border_variant)
            .child(div().w(px(14.)).flex_shrink_0())
            .child(div().w(px(18.)).flex_shrink_0())
            .child(badge(IconName::TriangleAlert, 28., p).text_color(p.warning))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_baseline()
                            .child(
                                div()
                                    .min_w_0()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.text_muted)
                                    .truncate()
                                    .child(name.clone()),
                            )
                            .when_some(u.version.clone(), |d, v| {
                                d.child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(12.))
                                        .text_color(p.text_placeholder)
                                        .child(v),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(17.))
                            .text_color(p.warning)
                            .child(reason),
                    ),
            )
            .child(
                Button::new(SharedString::from(format!("broken-remove-{id}")))
                    .ghost()
                    .small()
                    .icon(IconName::Trash)
                    .label("Remove")
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.remove_addon(&id, &name, window, cx)),
                    ),
            )
            .into_any_element()
    }
}
