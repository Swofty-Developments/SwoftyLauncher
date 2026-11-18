use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::shell::notices::Notification;
use gpui_kit::component::input::InputState;
use gpui_kit::*;
use swofty_core::dbedit::draft::{Details, DraftFile, Written};
use swofty_core::dbedit::package::Imported;
use swofty_core::dbedit::{Catalog, Edit, Edits, Hit, ImportReport, LinkSpec, Module, ModuleView, NodeId};

use super::draft::DraftBook;
use super::fields::Slots;
use crate::services::game::Install;
use crate::shell::Shell;
use crate::shell::host::{Host, Hosted, PromptHost, Prompted};
use swofty_core::store::Store;
use swofty_i18n::trf;

pub(crate) enum Load {
    Idle,
    Loading,
    Ready(Arc<Catalog>),
    Failed(String),
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Nav {
    Module(String),
    Raw(NodeId),
    Unnamed,
    Changes,
}

pub(crate) struct Reveal {
    pub(super) module: String,
    pub(super) scope: Option<String>,
    pub(super) label: String,
}

pub(crate) struct ViewSlot {
    pub module: String,
    pub scope: Option<String>,
    pub view: Arc<ModuleView>,
    pub choices: Arc<HashMap<usize, Vec<(String, String)>>>,
    pub shown: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Which {
    Search,
    Filter,
    Unnamed,
    Raw,
}

pub(crate) struct Debounced {
    pub input: Entity<InputState>,
    pub query: String,
    pub(super) generation: u64,
    pub(super) _sub: Subscription,
}

pub(super) type GameId = (PathBuf, Option<u64>, Option<String>);

pub(super) fn game_id(install: &crate::services::game::Install) -> GameId {
    (install.dir.clone(), install.build_id, install.exe_version.clone())
}

pub(crate) struct Db {
    pub load: Load,
    /// The draft and the DB Editor addons it can be saved into.
    pub book: DraftBook,
    pub game: Option<GameId>,
    pub modules: &'static [Module],
    pub(super) draft_read: bool,
    pub(super) file: Arc<Mutex<DraftFile>>,
    pub(super) others_gen: u64,
    pub(super) _quit: Subscription,
    pub(super) index: HashMap<u64, usize>,
    /// An addon to open once the player answers Save / Discard / Cancel.
    pub(super) pending_open: Option<String>,
    pub others: Arc<HashMap<u64, Vec<(String, String)>>>,
    pub toasts: Vec<Notification>,
    pub nav: Nav,
    pub scopes: HashMap<String, String>,
    pub view: Option<ViewSlot>,
    pub view_error: Option<String>,
    pub(super) view_want: Option<(String, Option<String>, u64)>,
    pub(super) stale_choices: bool,
    pub(super) rev: u64,
    pub row: Option<String>,
    pub max_cols: usize,
    pub notes_open: bool,
    pub reveal: Option<Reveal>,
    pub key: Option<NodeId>,
    pub errors: HashMap<u64, String>,
    pub search: Option<Debounced>,
    pub hits: Vec<Hit>,
    pub hits_open: bool,
    pub hit: usize,
    pub filter: Option<Debounced>,
    pub unnamed: Option<Debounced>,
    pub raw_filter: Option<Debounced>,
    pub raw_rows: Option<(NodeId, String, Arc<Vec<NodeId>>)>,
    pub(super) clear_raw: bool,
    pub unnamed_rows: Arc<Vec<NodeId>>,
    pub cell: Option<CellEdit>,
    pub slots: Slots,
    pub focus: Option<FocusHandle>,
    pub rows_scroll: UniformListScrollHandle,
    pub rows_motion: crate::ui::scroll::SmoothScroll,
    pub list_scroll: UniformListScrollHandle,
    pub list_motion: crate::ui::scroll::SmoothScroll,
    pub grid_scroll: ScrollHandle,
    pub(super) pending_tab: Option<(String, String, String)>,
    pub(super) pending_link: Option<LinkSpec>,
    pub importing: bool,
    pub(super) autosave: Autosave,
}

pub(crate) struct CellEdit {
    pub key: u64,
    pub input: Entity<InputState>,
    pub(super) _sub: Subscription,
}

impl Db {
    pub(crate) fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            load: Load::Idle,
            book: DraftBook::default(),
            game: None,
            modules: &[],
            draft_read: false,
            file: Arc::default(),
            others_gen: 0,
            _quit: cx.on_app_quit(|s: &mut Shell, _| {
                s.db_flush_now();
                async {}
            }),
            index: HashMap::new(),
            pending_open: None,
            others: Arc::new(HashMap::new()),
            toasts: Vec::new(),
            nav: Nav::Raw(0),
            scopes: HashMap::new(),
            view: None,
            view_error: None,
            view_want: None,
            stale_choices: false,
            rev: 0,
            row: None,
            max_cols: 8,
            notes_open: false,
            reveal: None,
            key: None,
            errors: HashMap::new(),
            search: None,
            hits: Vec::new(),
            hits_open: false,
            hit: 0,
            filter: None,
            unnamed: None,
            raw_filter: None,
            raw_rows: None,
            clear_raw: false,
            unnamed_rows: Arc::new(Vec::new()),
            cell: None,
            slots: Slots::default(),
            focus: None,
            rows_scroll: UniformListScrollHandle::new(),
            rows_motion: Default::default(),
            list_scroll: UniformListScrollHandle::new(),
            list_motion: Default::default(),
            grid_scroll: ScrollHandle::new(),
            pending_tab: None,
            pending_link: None,
            importing: false,
            autosave: Autosave::default(),
        }
    }

    pub(crate) fn catalog(&self) -> Option<&Arc<Catalog>> {
        match &self.load {
            Load::Ready(c) => Some(c),
            _ => None,
        }
    }

    pub(crate) fn edit(&self, key: u64) -> Option<&Edit> {
        self.index
            .get(&key)
            .and_then(|&i| self.book.draft.edits.edits.get(i))
    }

    pub(crate) fn current(&self, key: u64) -> String {
        match self.edit(key) {
            Some(e) => e.value.clone().unwrap_or_default(),
            None => self
                .catalog()
                .and_then(|c| self.book.draft.edits.value(c, key))
                .map(str::to_owned)
                .unwrap_or_default(),
        }
    }

    pub(super) fn reindex(&mut self) {
        self.index = self
            .book
            .draft
            .edits
            .edits
            .iter()
            .enumerate()
            .map(|(i, e)| (e.key, i))
            .collect();
    }

    pub(crate) fn module(&self, id: &str) -> Option<&'static Module> {
        self.modules.iter().find(|m| m.id == id)
    }
}

/// What runs once the player answers Save / Discard / Cancel about unsaved changes.
#[derive(Clone)]
pub(crate) enum Then {
    Open(String),
    New,
    Load {
        file: String,
        name: String,
        edits: Edits,
        report: ImportReport,
    },
}

/// Writing the draft to its file in the background, a moment after the last change.
#[derive(Default)]
pub(crate) struct Autosave {
    pub(super) ticket: u64,
    /// Changes not yet written to the draft file.
    pub(super) unwritten: bool,
    pub(super) writing: bool,
    /// Another launcher window wrote a newer draft; nothing is written until the player picks one.
    pub(super) newer: bool,
    pub(super) error: Option<String>,
}

impl Autosave {
    pub(super) fn mark(&mut self) -> u64 {
        self.unwritten = true;
        self.ticket += 1;
        self.ticket
    }

    pub(super) fn due(&self, ticket: u64) -> bool {
        self.ticket == ticket
    }

    pub(super) fn begin(&mut self) -> bool {
        if !self.unwritten || self.writing || self.newer {
            return false;
        }
        self.unwritten = false;
        self.writing = true;
        true
    }

    /// Takes a finished write; true when changes made meanwhile need writing too.
    pub(super) fn written(&mut self, result: Result<Written, String>) -> bool {
        self.writing = false;
        match result {
            Ok(Written::Saved) => self.error = None,
            Ok(Written::Newer) => {
                self.unwritten = true;
                self.newer = true;
            }
            Err(e) => {
                self.unwritten = true;
                self.error = Some(trf!("Your changes could not be saved to the draft: {e}", e = e));
                return false;
            }
        }
        self.unwritten && !self.newer
    }

    pub(super) fn idle(&self) -> bool {
        !self.unwritten && !self.writing && !self.newer
    }

    /// Drops this window's unwritten changes for the draft another window wrote.
    pub(super) fn give_way(&mut self) {
        self.newer = false;
        self.unwritten = false;
        self.ticket += 1;
    }
}

pub(crate) struct DbNeeds<'a> {
    pub(crate) store: &'a Store,
    pub(crate) profile: &'a str,
    pub(crate) install: Option<&'a Install>,
}

/// What the DB Editor's work reports to the shell.
pub(crate) enum DbChange {
    /// The draft was saved into `id`: the library is read again, then `then` runs or Addons shows the addon.
    Saved { id: String, then: Option<Then> },
    /// The draft was saved as the new addon `id`: turn it on, say so, then what follows a save.
    SavedNew {
        id: String,
        details: Details,
        then: Option<Then>,
    },
    /// An import installed an addon: turn it on, say so and show it on Addons.
    Imported(Imported),
}

impl Hosted for Db {
    type Needs<'a> = DbNeeds<'a>;
    type Change = DbChange;
}

/// What the DB Editor's work hands the shell to open in the window.
pub(crate) enum DbPrompt {
    /// Leave the draft for `Then`, asking first when it has unsaved changes.
    Leave(Then),
}

impl Prompted for Db {
    type Prompt = DbPrompt;
}

impl PromptHost<Db> for Shell {
    fn prompt(&mut self, prompt: DbPrompt, window: &mut Window, cx: &mut Context<Self>) {
        match prompt {
            DbPrompt::Leave(then) => self.db_leave(then, window, cx),
        }
    }
}

impl Host<Db> for Shell {
    fn split(&mut self) -> (&mut Db, DbNeeds<'_>) {
        (
            &mut self.db,
            DbNeeds {
                store: &self.store,
                profile: &self.library.profile.name,
                install: self.game.install.as_ref(),
            },
        )
    }

    fn changed(&mut self, change: DbChange, cx: &mut Context<Self>) {
        match change {
            DbChange::Saved { id, then } => self.db_after_save(id, then, cx),
            DbChange::SavedNew { id, details, then } => self.db_saved_new(id, &details, then, cx),
            DbChange::Imported(done) => self.db_addon_imported(done, cx),
        }
    }
}
