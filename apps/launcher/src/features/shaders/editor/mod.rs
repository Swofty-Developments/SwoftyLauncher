use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::input::TextDecorationCollection;
use gpui_kit::component::input::{EditorState, InputEvent, InputHighlighter};
use gpui_kit::*;
use swofty_i18n::tr;
use swofty_shaders::{Catalog, Severity};

use super::ShadersView;
use crate::ui::theme::Palette;

mod language;
mod lint;
mod view;

use language::{Completer, Fixer, Hoverer, SwfxHighlighter};

const LINT_DEBOUNCE: Duration = Duration::from_millis(120);
const SAVE_DEBOUNCE: Duration = Duration::from_millis(500);

pub(super) type SharedCatalog = Rc<RefCell<Arc<Catalog>>>;
pub(super) type SharedProblems = Rc<RefCell<Vec<Problem>>>;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Problem {
    pub severity: Severity,
    pub range: Range<usize>,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub fix: Option<swofty_shaders::Fix>,
}

#[derive(Default)]
pub(super) struct LintView {
    pub rev: u64,
    pub problems: Vec<Problem>,
    pub done: bool,
}

impl LintView {
    pub fn errors(&self) -> usize {
        self.problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .count()
    }

    pub fn warnings(&self) -> usize {
        self.problems
            .iter()
            .filter(|p| p.severity == Severity::Warning)
            .count()
    }
}

pub(super) struct EditorPane {
    pub state: Entity<EditorState>,
    pub loaded: Option<swofty_core::content::shaders::ConfigRef>,
    pub loading: bool,
    pub saved: String,
    pub last_text: String,
    pub lint: LintView,
    pub catalog: SharedCatalog,
    pub problems: SharedProblems,
    pub squiggles: TextDecorationCollection,
    pub lint_task: Option<Task<()>>,
    pub save_task: Option<Task<()>>,
    pub saving: bool,
    pub _events: Subscription,
}

impl EditorPane {
    pub fn new(p: Palette, window: &mut Window, cx: &mut Context<ShadersView>) -> Self {
        let catalog: SharedCatalog = Rc::new(RefCell::new(Arc::new(Catalog::default())));
        let problems: SharedProblems = Rc::new(RefCell::new(Vec::new()));
        let state = cx.new(|cx| EditorState::new(window, cx).language("swfx"));
        state.update(cx, |s, cx| {
            s.set_highlighter_factory(
                Rc::new(move |lang| {
                    (lang == "swfx").then(|| Box::new(SwfxHighlighter::new(p)) as Box<dyn InputHighlighter>)
                }),
                cx,
            );
            let lsp = s.lsp_mut();
            lsp.completion_provider = Some(Rc::new(Completer {
                catalog: catalog.clone(),
            }));
            lsp.hover_provider = Some(Rc::new(Hoverer {
                catalog: catalog.clone(),
                problems: problems.clone(),
            }));
            lsp.code_action_providers.push(Rc::new(Fixer {
                problems: problems.clone(),
            }));
        });
        let squiggles = state.update(cx, |s, cx| s.create_decorations_collection(Vec::new(), cx));
        let events = cx.subscribe_in(&state, window, |this, _, ev: &InputEvent, _window, cx| {
            if matches!(ev, InputEvent::Change) {
                this.edited(cx);
            }
        });
        Self {
            state,
            loaded: None,
            loading: false,
            saved: String::new(),
            last_text: String::new(),
            lint: LintView::default(),
            catalog,
            problems,
            squiggles,
            lint_task: None,
            save_task: None,
            saving: false,
            _events: events,
        }
    }
}

impl ShadersView {}

fn squiggle_range(text: &str, range: &Range<usize>) -> Option<Range<usize>> {
    let start = range.start.min(text.len());
    let end = range.end.min(text.len());
    if start < end {
        return Some(start..end);
    }
    if let Some(c) = text.get(start..).and_then(|t| t.chars().next()) {
        return Some(start..start + c.len_utf8());
    }
    let c = text.get(..start)?.chars().next_back()?;
    Some(start - c.len_utf8()..start)
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Error => tr("Error"),
        Severity::Warning => tr("Warning"),
        Severity::Info => tr("Info"),
        Severity::Hint => tr("Hint"),
    }
}

fn icon_of(s: Severity) -> IconName {
    match s {
        Severity::Error => IconName::CircleAlert,
        Severity::Warning => IconName::TriangleAlert,
        Severity::Info | Severity::Hint => IconName::Info,
    }
}
