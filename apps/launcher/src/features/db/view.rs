use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::ui::scale::px;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::*;
use swofty_core::dbedit::{Catalog, Kind, Module, ModuleView, NodeId, NodeKind, Target, driver_manager};
use swofty_i18n::{tr, trf};

use super::state::{Which, game_id};
use super::{Db, Debounced, LinkState, Load, Nav, Reveal, Then, ViewSlot, guarded};
use crate::shell::Shell;
use crate::shell::host::Host;

type Loaded = Result<(Arc<Catalog>, &'static [Module]), String>;

const DEBOUNCE: Duration = Duration::from_millis(120);

impl Shell {
    pub(crate) fn sync_db(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.game.install.as_ref().map(game_id);
        if self.db.game.is_some() && self.db.game != id {
            self.db_forget_game();
        }
        if matches!(self.db.load, Load::Idle) && id.is_some() {
            Db::open(self, cx);
        }
        if self.db.focus.is_none() {
            let focus = cx.focus_handle();
            focus.focus(window, cx);
            self.db.focus = Some(focus);
        }
        if self.db.search.is_none() {
            let search = self.debounced(Which::Search, window, cx);
            self.db.search = Some(search);
        }
        if self.db.filter.is_none() {
            let filter = self.debounced(Which::Filter, window, cx);
            self.db.filter = Some(filter);
        }
        if self.db.unnamed.is_none() {
            let unnamed = self.debounced(Which::Unnamed, window, cx);
            self.db.unnamed = Some(unnamed);
        }
        if self.db.raw_filter.is_none() {
            let raw = self.debounced(Which::Raw, window, cx);
            self.db.raw_filter = Some(raw);
        }
        if std::mem::take(&mut self.db.clear_raw)
            && let Some(d) = self.db.raw_filter.as_mut()
        {
            d.query.clear();
            d.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        if let Some(id) = self.db.pending_open.take() {
            self.db_leave(Then::Open(id), window, cx);
        }
        self.db_apply_reveal(cx);
        self.sync_fields(window, cx);
    }

    // A search jump selects its row once the view shows the hit's module and scope.
    fn db_apply_reveal(&mut self, cx: &mut Context<Self>) {
        let (Some(r), Some(slot)) = (self.db.reveal.as_ref(), self.db.view.as_ref()) else {
            return;
        };
        let here = Nav::Module(slot.module.clone()) == self.db.nav
            && r.module == slot.module
            && r.scope.as_ref().is_none_or(|s| slot.scope.as_ref() == Some(s));
        if !here {
            return;
        }
        let Some(r) = self.db.reveal.take() else {
            return;
        };
        if let Some(i) = slot
            .shown
            .iter()
            .position(|&i| slot.view.rows[i].label == r.label)
        {
            self.db.rows_motion.cancel();
            self.db.rows_scroll.scroll_to_item(i, ScrollStrategy::Center);
        }
        if slot.view.rows.iter().any(|row| row.label == r.label) {
            self.db.row = Some(r.label);
        }
        cx.notify();
    }

    fn debounced(&mut self, which: Which, window: &mut Window, cx: &mut Context<Self>) -> Debounced {
        let placeholder = match which {
            Which::Search => tr("Search drivers, teams, tracks, keys and values"),
            Which::Filter => tr("Filter rows"),
            Which::Unnamed => tr("Filter by key or value"),
            Which::Raw => tr("Filter this level"),
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let sub = cx.subscribe_in(&input, window, move |this, state, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {}
                InputEvent::PressEnter { .. } if which == Which::Search => {
                    this.db_open_hit(this.db.hit, window, cx);
                    return;
                }
                InputEvent::Focus if which == Which::Search => {
                    this.db.hits_open = true;
                    cx.notify();
                    return;
                }
                _ => return,
            }
            let query = state.read(cx).value().to_string();
            let Some(slot) = this.db.slot(which) else {
                return;
            };
            slot.generation += 1;
            let generation = slot.generation;
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(DEBOUNCE).await;
                this.update(cx, |s, cx| {
                    if let Some(slot) = s.db.slot(which).filter(|d| d.generation == generation) {
                        slot.query = query;
                        match which {
                            Which::Search => Db::find_hits(s, cx),
                            Which::Filter => {
                                s.db.refilter();
                                cx.notify();
                            }
                            Which::Unnamed => Db::list_unnamed(s, cx),
                            Which::Raw => cx.notify(),
                        }
                    }
                })
                .ok();
            })
            .detach();
        });
        Debounced {
            input,
            query: String::new(),
            generation: 0,
            _sub: sub,
        }
    }

    pub(crate) fn db_focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = &self.db.search {
            s.input.update(cx, |s, cx| {
                s.focus(window, cx);
                s.select_all(window, cx);
            });
            self.db.hits_open = true;
            cx.notify();
        }
    }

    pub(crate) fn db_open_hit(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(hit) = self.db.hits.get(index).cloned() else {
            return;
        };
        self.db.hits_open = false;
        if let Some(f) = &self.db.focus {
            f.focus(window, cx);
        }
        match hit.target {
            Target::ModuleRow { module, scope, label } => {
                if let Some(scope) = scope.clone() {
                    self.db.scopes.insert(module.clone(), scope);
                }
                Db::go(self, Nav::Module(module.clone()), cx);
                self.db.row = None;
                self.db.cell = None;
                self.db.reveal = Some(Reveal { module, scope, label });
                Db::refresh_view(self, cx);
            }
            Target::Node(node) => self.db_show_node(node, cx),
            Target::Link(link) => match self.db_link_state(&link) {
                LinkState::Open => self.db_open_section(&link, cx),
                _ => {
                    if let Some(m) = self.db.modules.iter().find(|m| m.links.contains(&link)) {
                        Db::go(self, Nav::Module(m.id.clone()), cx);
                    }
                }
            },
        }
        cx.notify();
    }

    pub(crate) fn db_show_node(&mut self, node: NodeId, cx: &mut Context<Self>) {
        let Some(catalog) = self.db.catalog().cloned() else {
            return;
        };
        let n = catalog.node(node);
        let parent = n.parent.unwrap_or(catalog.root());
        match n.kind {
            NodeKind::Key if catalog.node(parent).kind == NodeKind::Unnamed => {
                Db::go(self, Nav::Unnamed, cx);
                self.db.key = Some(node);
            }
            NodeKind::Key => {
                Db::go(self, Nav::Raw(parent), cx);
                self.db.key = Some(node);
            }
            NodeKind::Unnamed => Db::go(self, Nav::Unnamed, cx),
            _ => Db::go(self, Nav::Raw(node), cx),
        }
    }

    pub(crate) fn db_loading(&self, text: &str, p: crate::ui::theme::Palette) -> AnyElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .pb(px(48.))
            .child(crate::ui::badge(IconName::Database, 44., p))
            .child(
                div()
                    .pt(px(8.))
                    .text_size(px(13.))
                    .text_color(p.text_muted)
                    .child(text.to_owned()),
            )
            .into_any_element()
    }
}

impl Db {
    fn slot(&mut self, which: Which) -> Option<&mut Debounced> {
        match which {
            Which::Search => self.search.as_mut(),
            Which::Filter => self.filter.as_mut(),
            Which::Unnamed => self.unnamed.as_mut(),
            Which::Raw => self.raw_filter.as_mut(),
        }
    }

    pub(super) fn open<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (db, needs) = host.split();
        let Some(install) = needs.install.cloned() else {
            return;
        };
        let id = game_id(&install);
        db.load = Load::Loading;
        db.game = Some(id.clone());
        let store = needs.store.clone();
        cx.spawn(async move |this, cx| {
            let loaded: Loaded = cx
                .background_executor()
                .spawn(async move {
                    let catalog = guarded(|| {
                        let game = crate::features::sections::game(&store, Some(&install))
                            .ok_or(tr("The game folder could not be read."))?;
                        Catalog::open(&store, &game)
                            .map_err(|e| trf!("The game database could not be read: {e}", e = e))
                    })?;
                    guarded(|| Ok(catalog.search("x", 1))).ok();
                    let modules = guarded(|| Ok(swofty_core::dbedit::modules())).unwrap_or(&[]);
                    Ok((catalog, modules))
                })
                .await;
            this.update(cx, |host, cx| {
                let current = host.split().0.game.as_ref() == Some(&id);
                if current {
                    Self::loaded(host, loaded, cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn loaded<H: Host<Self>>(host: &mut H, loaded: Loaded, cx: &mut Context<H>) {
        let db = host.split().0;
        match loaded {
            Ok((catalog, modules)) => {
                db.modules = modules;
                if let Nav::Raw(0) = db.nav {
                    db.nav = modules
                        .first()
                        .map_or(Nav::Raw(catalog.root()), |m| Nav::Module(m.id.clone()));
                }
                db.load = Load::Ready(catalog);
                Self::changed(host, None, false, cx);
                Self::list_unnamed(host, cx);
            }
            Err(e) => db.load = Load::Failed(e),
        }
        cx.notify();
    }

    pub(crate) fn go<H: Host<Self>>(host: &mut H, nav: Nav, cx: &mut Context<H>) {
        host.split().0.move_to(nav);
        Self::refresh_view(host, cx);
        cx.notify();
    }

    fn move_to(&mut self, nav: Nav) {
        if self.nav != nav {
            self.rows_motion.cancel();
            self.list_motion.cancel();
            self.nav = nav;
            self.row = None;
            self.reveal = None;
            self.key = None;
            self.cell = None;
            self.rows_scroll = UniformListScrollHandle::new();
            self.list_scroll = UniformListScrollHandle::new();
            self.clear_raw = self.raw_filter.as_ref().is_some_and(|d| !d.query.is_empty());
        }
        self.hits_open = false;
    }

    // After any edit: persist, patch the cached module view, then rebuild it off the UI thread.
    pub(crate) fn changed<H: Host<Self>>(host: &mut H, key: Option<u64>, save: bool, cx: &mut Context<H>) {
        host.split().0.patch(key);
        if save {
            Self::persist(host, cx);
        }
        Self::refresh_view(host, cx);
        cx.notify();
    }

    fn patch(&mut self, key: Option<u64>) {
        self.rev += 1;
        self.reindex();
        let edit = key.and_then(|k| self.edit(k).cloned());
        if let (Some(key), Some(catalog), Some(slot)) = (key, self.catalog().cloned(), self.view.as_mut()) {
            let view = Arc::make_mut(&mut slot.view);
            for row in &mut view.rows {
                for cell in row.cells.iter_mut().filter(|c| c.key == Some(key)) {
                    cell.edited = edit.is_some();
                    cell.value = match &edit {
                        Some(e) => e.value.clone().unwrap_or_default(),
                        None => catalog
                            .record_by_key(key)
                            .map_or(cell.stock.clone(), |r| r.value.to_owned()),
                    };
                }
            }
        }
    }

    pub(crate) fn refresh_view<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (db, needs) = host.split();
        let Nav::Module(id) = db.nav.clone() else {
            return;
        };
        let (Some(catalog), Some(module)) = (db.catalog().cloned(), db.module(&id)) else {
            return;
        };
        let scope = db.scopes.get(&id).cloned();
        let want = (id.clone(), scope.clone(), db.rev);
        if db.view_want.as_ref() == Some(&want) {
            return;
        }
        db.view_want = Some(want.clone());
        let stale = std::mem::take(&mut db.stale_choices);
        let choices = db
            .view
            .as_ref()
            .filter(|v| !stale && v.module == id && v.scope == scope)
            .map(|v| v.choices.clone());
        let edits = db.book.draft.edits.clone();
        let store = needs.store.clone();
        cx.spawn(async move |this, cx| {
            let (view, choices) = cx
                .background_executor()
                .spawn(async move {
                    let view = guarded(|| {
                        module
                            .view(&catalog, &edits, scope.as_deref())
                            .map_err(|e| e.to_string())
                    });
                    let choices = choices.unwrap_or_else(|| {
                        let tracks = swofty_core::tracks::installed(&store);
                        let drivers = driver_manager::drivers(&store, &catalog);
                        let heads = swofty_core::game_db::heads::installed(&store);
                        let refs = module
                            .fields
                            .iter()
                            .enumerate()
                            .filter(|(_, f)| matches!(f.kind, Some(Kind::Ref | Kind::RefList)))
                            .map(|(i, _)| {
                                let c = module.choices_with(
                                    &catalog,
                                    i,
                                    scope.as_deref(),
                                    &tracks,
                                    &drivers,
                                    &heads,
                                );
                                (i, c)
                            })
                            .collect();
                        Arc::new(refs)
                    });
                    (view, choices)
                })
                .await;
            this.update(cx, |host, cx| {
                host.split().0.take_view(want, view, choices);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn take_view(
        &mut self,
        want: (String, Option<String>, u64),
        view: Result<ModuleView, String>,
        choices: Arc<HashMap<usize, Vec<(String, String)>>>,
    ) {
        if self.view_want.as_ref() != Some(&want) {
            return;
        }
        match view {
            Ok(view) => {
                if let Some(scope) = view.scope.clone() {
                    self.scopes.entry(want.0.clone()).or_insert(scope);
                }
                if self
                    .view
                    .as_ref()
                    .is_some_and(|v| v.module == want.0 && v.scope != view.scope)
                {
                    self.row = None;
                    self.cell = None;
                }
                self.view_error = None;
                self.view = Some(ViewSlot {
                    module: want.0,
                    scope: view.scope.clone(),
                    view: Arc::new(view),
                    choices,
                    shown: Vec::new(),
                });
                self.refilter();
            }
            Err(e) => {
                self.view = None;
                self.view_error = Some(e);
            }
        }
    }

    pub(crate) fn set_scope<H: Host<Self>>(host: &mut H, scope: String, cx: &mut Context<H>) {
        let db = host.split().0;
        let Nav::Module(id) = db.nav.clone() else {
            return;
        };
        db.scopes.insert(id, scope);
        db.row = None;
        db.cell = None;
        db.reveal = None;
        Self::refresh_view(host, cx);
    }

    fn refilter(&mut self) {
        let query = self
            .filter
            .as_ref()
            .map(|f| f.query.trim().to_lowercase())
            .unwrap_or_default();
        if let Some(slot) = self.view.as_mut() {
            let view = &slot.view;
            let columns: Vec<usize> = view.columns.iter().map(|c| c.field).collect();
            slot.shown = (0..view.rows.len())
                .filter(|&i| {
                    let r = &view.rows[i];
                    query.is_empty()
                        || r.name.to_lowercase().contains(&query)
                        || r.label.to_lowercase().contains(&query)
                        || columns.iter().any(|&c| {
                            r.cells.get(c).is_some_and(|cell| {
                                cell.value.to_lowercase().contains(&query)
                                    || cell
                                        .text
                                        .as_ref()
                                        .is_some_and(|t| t.to_lowercase().contains(&query))
                            })
                        })
                })
                .collect();
        }
    }

    fn find_hits<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let (db, needs) = host.split();
        let query = db
            .search
            .as_ref()
            .map(|s| s.query.trim().to_owned())
            .unwrap_or_default();
        let generation = db.search.as_ref().map_or(0, |s| s.generation);
        let Some(catalog) = db.catalog().cloned() else {
            return;
        };
        if query.is_empty() {
            db.hits.clear();
            cx.notify();
            return;
        }
        let from = catalog.clone();
        let store = needs.store.clone();
        cx.spawn(async move |this, cx| {
            let hits = cx
                .background_executor()
                .spawn(async move {
                    guarded(|| {
                        let drivers = driver_manager::drivers(&store, &catalog);
                        Ok(catalog.search_with(&query, 60, &drivers))
                    })
                    .unwrap_or_default()
                })
                .await;
            this.update(cx, |host, cx| {
                let db = host.split().0;
                if db.search.as_ref().is_some_and(|d| d.generation == generation)
                    && db.catalog().is_some_and(|c| Arc::ptr_eq(c, &from))
                {
                    let mut hits = hits;
                    hits.sort_by_key(|h| h.kind);
                    db.hits = hits;
                    db.hit = 0;
                    db.hits_open = true;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn list_unnamed<H: Host<Self>>(host: &mut H, cx: &mut Context<H>) {
        let db = host.split().0;
        let Some(catalog) = db.catalog().cloned() else {
            return;
        };
        let query = db
            .unnamed
            .as_ref()
            .map(|d| d.query.trim().to_lowercase())
            .unwrap_or_default();
        let generation = db.unnamed.as_ref().map_or(0, |d| d.generation);
        let from = catalog.clone();
        cx.spawn(async move |this, cx| {
            let rows = cx
                .background_executor()
                .spawn(async move {
                    guarded(|| {
                        let Some(root) = catalog
                            .children(catalog.root())
                            .iter()
                            .copied()
                            .find(|&c| catalog.node(c).kind == NodeKind::Unnamed)
                        else {
                            return Ok(Vec::new());
                        };
                        Ok(catalog
                            .children(root)
                            .iter()
                            .copied()
                            .filter(|&n| {
                                if query.is_empty() {
                                    return true;
                                }
                                let node = catalog.node(n);
                                node.name.to_lowercase().contains(&query)
                                    || node.record.is_some_and(|r| {
                                        catalog.record(r).value.to_lowercase().contains(&query)
                                    })
                            })
                            .collect())
                    })
                    .unwrap_or_default()
                })
                .await;
            this.update(cx, |host, cx| {
                let db = host.split().0;
                if db.unnamed.as_ref().is_none_or(|d| d.generation == generation)
                    && db.catalog().is_some_and(|c| Arc::ptr_eq(c, &from))
                {
                    db.unnamed_rows = Arc::new(rows);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}
