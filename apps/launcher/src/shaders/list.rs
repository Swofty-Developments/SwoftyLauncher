use std::path::PathBuf;

use crate::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::{Disableable, Icon, Sizable, WindowExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use swofty_core::shaders::{self, ConfigEntry, ConfigRef};
use swofty_core::store::Store;

use super::{Event, ShadersView, list_width};
use crate::pages::{label, tag};
use crate::theme::{Palette, ScrollArea};

#[derive(Default)]
pub(super) struct ListState {
    pub rename: Option<Rename>,
    pub pending_rename: Option<ConfigRef>,
    pub rename_new: bool,
}

pub(super) struct Rename {
    pub target: ConfigRef,
    pub input: Entity<InputState>,
    pub _events: Subscription,
}

type Job = Box<dyn FnOnce(&Store) -> Result<Option<ConfigRef>, String> + Send>;

impl ShadersView {
    pub(super) fn list_pane(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let locked = self.forced.is_some();
        let yours: Vec<ConfigEntry> = self.entries.iter().filter(|e| e.editable).cloned().collect();
        let addons: Vec<ConfigEntry> = self.entries.iter().filter(|e| !e.editable).cloned().collect();
        let off_on = self.active.is_none() && self.forced.is_none();
        let off = self.row_shell("shaders-off", off_on, false, p).child(
            h_flex()
                .flex_1()
                .gap(px(10.))
                .items_center()
                .child(self.state_dot(off_on, p))
                .child(
                    v_flex()
                        .flex_1()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .child("Off"),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_placeholder)
                                .child("Play without shaders"),
                        ),
                ),
        );
        let off = if locked {
            off.opacity(0.5)
        } else {
            off.cursor_pointer()
                .hover(move |s| s.bg(p.hover))
                .on_click(cx.listener(|this, _, _, cx| this.use_config(None, cx)))
        };
        let mut body = v_flex().gap(px(2.)).p(px(6.)).child(off);
        body = body.child(self.group_label("YOUR CONFIGS", yours.len(), p));
        if yours.is_empty() {
            body = body.child(self.hint("Nothing here yet. Start from the template, or import one.", p));
        }
        for entry in &yours {
            body = body.child(self.config_row(entry, locked, p, cx));
        }
        body = body.child(self.group_label("FROM ADDONS", addons.len(), p));
        if addons.is_empty() {
            body = body.child(self.hint("Addons that bundle shader configs show up here.", p));
        }
        for entry in &addons {
            body = body.child(self.config_row(entry, locked, p, cx));
        }
        let shaders = cx.entity().downgrade();
        let import = Button::new("shaders-import")
            .outline()
            .small()
            .icon(IconName::FileUp)
            .label("Import")
            .dropdown_caret(true)
            .disabled(self.busy)
            .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, _, _| {
                let (a, b) = (shaders.clone(), shaders.clone());
                menu.item(PopupMenuItem::new("From a file…").on_click(move |_, window, cx| {
                    a.update(cx, |v, cx| v.import_file(window, cx)).ok();
                }))
                .item(
                    PopupMenuItem::new("From the clipboard").on_click(move |_, _, cx| {
                        b.update(cx, |v, cx| v.import_clipboard(cx)).ok();
                    }),
                )
            });
        v_flex()
            .w(px(list_width()))
            .flex_shrink_0()
            .h_full()
            .rounded(px(6.))
            .border_1()
            .border_color(p.border)
            .bg(p.surface)
            .overflow_hidden()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(v_flex().size_full().scroll_y("shaders-list").child(body)),
            )
            .child(
                h_flex()
                    .flex_shrink_0()
                    .p(px(10.))
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(p.border_variant)
                    .child(
                        Button::new("shaders-new")
                            .small()
                            .icon(IconName::Plus)
                            .label("New config")
                            .disabled(self.busy)
                            .on_click(cx.listener(|this, _, window, cx| this.new_config(window, cx))),
                    )
                    .child(import),
            )
            .into_any_element()
    }

    fn group_label(&self, title: &'static str, count: usize, p: Palette) -> Div {
        h_flex()
            .px(px(8.))
            .pt(px(14.))
            .pb(px(4.))
            .gap(px(6.))
            .child(label(title, p))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(p.text_placeholder)
                    .child(count.to_string()),
            )
    }

    fn hint(&self, text: &'static str, p: Palette) -> Div {
        div()
            .px(px(8.))
            .py(px(6.))
            .text_size(px(12.))
            .text_color(p.text_placeholder)
            .child(text)
    }

    fn row_shell(&self, id: impl Into<ElementId>, selected: bool, _dim: bool, p: Palette) -> Stateful<Div> {
        h_flex()
            .id(id)
            .min_h(px(44.))
            .px(px(8.))
            .py(px(6.))
            .gap(px(8.))
            .items_center()
            .rounded(px(5.))
            .border_1()
            .map(|d| {
                if selected {
                    d.bg(p.selected).border_color(p.border)
                } else {
                    d.border_color(gpui_kit::transparent_black())
                }
            })
    }

    fn state_dot(&self, on: bool, p: Palette) -> Div {
        div()
            .size(px(18.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(if on {
                Icon::new(IconName::CircleCheck)
                    .size(px(16.))
                    .text_color(p.success)
            } else {
                Icon::new(IconName::Circle).size(px(14.)).text_color(p.border)
            })
    }

    fn config_row(
        &self,
        entry: &ConfigEntry,
        locked: bool,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let r = entry.r#ref.clone();
        let is_active = self.running_ref() == Some(&r) && (self.forced.is_some() || self.active.is_some());
        let selected = self.selected.as_ref() == Some(&r);
        let renaming = self.list.rename.as_ref().filter(|n| n.target == r);
        let editable = entry.editable;
        let id = SharedString::from(format!("shaders-row-{r}"));
        let pick = r.clone();
        let menu_ref = r.clone();
        let shaders = cx.entity().downgrade();
        let name_el: AnyElement = match renaming {
            Some(n) => div()
                .flex_1()
                .min_w_0()
                .child(Input::new(&n.input).small())
                .into_any_element(),
            None => v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_size(px(13.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(entry.name.clone()),
                )
                .child(div().flex().child(tag(
                    if editable {
                        "Yours".to_owned()
                    } else {
                        entry.source.clone()
                    },
                    p,
                )))
                .into_any_element(),
        };
        let more = Button::new(SharedString::from(format!("shaders-more-{r}")))
            .ghost()
            .small()
            .icon(IconName::Ellipsis)
            .dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
                let r = menu_ref.clone();
                let act = |shaders: &WeakEntity<ShadersView>,
                           f: fn(&mut ShadersView, ConfigRef, &mut Window, &mut Context<ShadersView>),
                           r: &ConfigRef| {
                    let (shaders, r) = (shaders.clone(), r.clone());
                    move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                        let r = r.clone();
                        shaders.update(cx, |v, cx| f(v, r, window, cx)).ok();
                    }
                };
                menu = menu.item(PopupMenuItem::new("Use").disabled(locked).on_click(act(
                    &shaders,
                    |v, r, _, cx| v.use_config(Some(r), cx),
                    &r,
                )));
                if editable {
                    menu = menu
                        .item(PopupMenuItem::new("Rename").on_click(act(
                            &shaders,
                            ShadersView::begin_rename,
                            &r,
                        )))
                        .item(PopupMenuItem::new("Duplicate").on_click(act(
                            &shaders,
                            ShadersView::copy_to_personal,
                            &r,
                        )))
                        .item(PopupMenuItem::new("Export…").on_click(act(&shaders, ShadersView::export, &r)))
                        .item(PopupMenuItem::new("Delete…").on_click(act(
                            &shaders,
                            ShadersView::ask_delete,
                            &r,
                        )));
                } else {
                    menu = menu
                        .item(PopupMenuItem::new("Copy to personal").on_click(act(
                            &shaders,
                            ShadersView::copy_to_personal,
                            &r,
                        )))
                        .item(PopupMenuItem::new("Export…").on_click(act(&shaders, ShadersView::export, &r)));
                }
                menu
            });
        let dot_ref = r.clone();
        let dot = div()
            .id(SharedString::from(format!("shaders-use-{r}")))
            .child(self.state_dot(is_active, p))
            .when(!locked, |d| {
                d.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.use_config(Some(dot_ref.clone()), cx)
                }))
            });
        self.row_shell(id, selected, false, p)
            .cursor_pointer()
            .hover(move |s| if selected { s } else { s.bg(p.hover) })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected = Some(pick.clone());
                cx.notify();
            }))
            .child(dot)
            .child(name_el)
            .child(more)
            .into_any_element()
    }

    pub(super) fn run(&mut self, cx: &mut Context<Self>, job: Job) {
        self.busy = true;
        let store = self.store.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let out = cx.background_executor().spawn(async move { job(&store) }).await;
            this.update(cx, |v, cx| {
                v.busy = false;
                match out {
                    Ok(selected) => {
                        v.error = None;
                        if let Some(r) = selected {
                            if std::mem::take(&mut v.list.rename_new) {
                                v.list.pending_rename = Some(r.clone());
                            }
                            v.selected = Some(r);
                        }
                        v.refresh(cx);
                    }
                    Err(e) => {
                        v.list.rename_new = false;
                        v.error = Some(e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn use_config(&mut self, r: Option<ConfigRef>, cx: &mut Context<Self>) {
        if self.forced.is_some() || self.busy {
            return;
        }
        let name = self.info.profile.clone();
        let live = self.info.running.then(|| self.info.game_dir.clone()).flatten();
        self.busy = true;
        let store = self.store.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let out = cx
                .background_executor()
                .spawn(async move {
                    let mut profile = store.profile(&name).map_err(|e| e.to_string())?;
                    shaders::set_active(&store, &mut profile, r).map_err(|e| e.to_string())?;
                    if let Some(dir) = live {
                        shaders::push_live(&dir, &shaders::active_file(&store, &profile))
                            .map_err(|e| e.to_string())?;
                    }
                    Ok::<(), String>(())
                })
                .await;
            this.update(cx, |v, cx| {
                v.busy = false;
                v.error = out.err().map(|e| format!("Couldn't change the shaders: {e}"));
                v.refresh(cx);
                cx.emit(Event::ActiveChanged);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn new_config(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let names: Vec<String> = self.entries.iter().map(|e| e.name.clone()).collect();
        let name = unused_name("New config", &names);
        self.list.rename_new = true;
        self.run(
            cx,
            Box::new(move |store| {
                shaders::create(store, &name, None)
                    .map(Some)
                    .map_err(|e| e.to_string())
            }),
        );
    }

    pub(super) fn copy_to_personal(&mut self, r: ConfigRef, _window: &mut Window, cx: &mut Context<Self>) {
        self.run(
            cx,
            Box::new(move |store| {
                shaders::copy_to_personal(store, &r)
                    .map(Some)
                    .map_err(|e| e.to_string())
            }),
        );
    }

    pub(super) fn begin_rename(&mut self, r: ConfigRef, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.entry_of(&r).map(|e| e.name.clone()) else {
            self.list.pending_rename = Some(r);
            return;
        };
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let events = cx.subscribe_in(
            &input,
            window,
            |this, input, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    let value = input.read(cx).value().to_string();
                    this.commit_rename(value, window, cx);
                }
                _ => {}
            },
        );
        input.update(cx, |s, cx| s.focus(window, cx));
        self.list.rename = Some(Rename {
            target: r,
            input,
            _events: events,
        });
        cx.notify();
    }

    pub(super) fn start_pending_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.list.pending_rename.clone() else {
            return;
        };
        if self.entry_of(&r).is_some() {
            self.list.pending_rename = None;
            self.begin_rename(r, window, cx);
        }
    }

    fn commit_rename(&mut self, value: String, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.list.rename.take() else {
            return;
        };
        cx.notify();
        let value = value.trim().to_owned();
        let current = self.entry_of(&rename.target).map(|e| e.name.clone());
        if value.is_empty() || current.as_deref() == Some(value.as_str()) {
            return;
        }
        let old = rename.target;
        let was_active = self.active.as_ref() == Some(&old);
        let was_selected = self.selected.as_ref() == Some(&old);
        let profile = self.info.profile.clone();
        let live = self.info.running.then(|| self.info.game_dir.clone()).flatten();
        self.run(
            cx,
            Box::new(move |store| {
                let new = shaders::rename(store, &old, &value).map_err(|e| e.to_string())?;
                if was_active && new != old {
                    let mut p = store.profile(&profile).map_err(|e| e.to_string())?;
                    shaders::set_active(store, &mut p, Some(new.clone())).map_err(|e| e.to_string())?;
                    if let Some(dir) = live {
                        shaders::push_live(&dir, &shaders::active_file(store, &p))
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok(was_selected.then_some(new))
            }),
        );
    }

    pub(super) fn export(&mut self, r: ConfigRef, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .entry_of(&r)
            .map_or_else(|| "shaders".to_owned(), |e| e.name.clone());
        let file = format!("{}.swfx", name.replace(['/', '\\', ':'], "-"));
        let start = dirs_start();
        let rx = cx.prompt_for_new_path(&start, Some(&file));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                this.update(cx, |v, cx| {
                    v.run(
                        cx,
                        Box::new(move |store| {
                            shaders::export(store, &r, &path)
                                .map(|()| None)
                                .map_err(|e| e.to_string())
                        }),
                    );
                })
                .ok();
            }
        })
        .detach();
    }

    pub(super) fn ask_delete(&mut self, r: ConfigRef, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.entry_of(&r).cloned() else {
            return;
        };
        let shaders = cx.entity().downgrade();
        let was_active = self.active.as_ref() == Some(&r);
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (shaders, r) = (shaders.clone(), r.clone());
            alert
                .width(px(460.))
                .title(format!("Delete {}?", entry.name))
                .description(if was_active {
                    "This config is on right now. The game plays without shaders after it is deleted."
                } else {
                    "The config file is removed from your computer."
                })
                .ok_text("Delete")
                .ok_variant(ButtonVariant::Danger)
                .show_cancel(true)
                .on_ok(move |_, _, cx| {
                    let r = r.clone();
                    shaders.update(cx, |v, cx| v.delete(r, cx)).ok();
                    true
                })
        });
    }

    fn delete(&mut self, r: ConfigRef, cx: &mut Context<Self>) {
        let was_active = self.active.as_ref() == Some(&r);
        let was_selected = self.selected.as_ref() == Some(&r);
        let profile = self.info.profile.clone();
        let live = self.info.running.then(|| self.info.game_dir.clone()).flatten();
        if was_selected {
            self.selected = None;
        }
        self.run(
            cx,
            Box::new(move |store| {
                shaders::delete(store, &r).map_err(|e| e.to_string())?;
                if was_active {
                    let mut p = store.profile(&profile).map_err(|e| e.to_string())?;
                    shaders::set_active(store, &mut p, None).map_err(|e| e.to_string())?;
                    if let Some(dir) = live {
                        shaders::push_live(&dir, &shaders::active_file(store, &p))
                            .map_err(|e| e.to_string())?;
                    }
                }
                Ok(None)
            }),
        );
        if was_active {
            cx.emit(Event::ActiveChanged);
        }
    }

    pub(super) fn import_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(path) = paths.into_iter().next()
            {
                this.update(cx, |v, cx| {
                    v.run(
                        cx,
                        Box::new(move |store| {
                            shaders::import(store, &path).map(Some).map_err(|e| e.to_string())
                        }),
                    );
                })
                .ok();
            }
        })
        .detach();
    }

    pub(super) fn import_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else {
            self.error = Some("The clipboard has no text to import.".into());
            cx.notify();
            return;
        };
        let catalog = self.catalog.clone();
        let names: Vec<String> = self.entries.iter().map(|e| e.name.clone()).collect();
        self.run(
            cx,
            Box::new(move |store| {
                let lint = swofty_shaders::lint(&text, &catalog);
                let name = lint
                    .config
                    .as_ref()
                    .map(|c| c.name.trim().to_owned())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Imported config".to_owned());
                let name = unused_name(&name, &names);
                shaders::create(store, &name, Some(&text))
                    .map(Some)
                    .map_err(|e| e.to_string())
            }),
        );
    }
}

fn unused_name(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|n| n == base) {
        return base.to_owned();
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken.contains(n))
        .unwrap_or_else(|| base.to_owned())
}

fn dirs_start() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}
