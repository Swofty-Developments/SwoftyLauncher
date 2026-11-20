//! The swfx language in the editor: highlighting, completion, hover and quick fixes.

use std::ops::Range;

use gpui_kit::component::input::{
    CodeActionProvider, CompletionProvider, EditorState, FoldRange, HighlightStyleResolver, HoverProvider,
    InputEdit, InputHighlighter, Rope, RopeExt,
};
use gpui_kit::*;
use lsp_types::{
    CodeAction, CodeActionKind, CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse,
    CompletionTextEdit, Documentation, HoverContents, MarkupContent, MarkupKind, TextEdit,
};
use swofty_shaders::{CompletionKind, TokenKind};

use crate::ui::theme::Palette;

use super::{Problem, SharedCatalog, SharedProblems, severity_label, squiggle_range};

pub(super) struct SwfxHighlighter {
    pub(super) palette: Palette,
    pub(super) tokens: Vec<swofty_shaders::Token>,
}

impl SwfxHighlighter {
    pub(super) fn new(palette: Palette) -> Self {
        Self {
            palette,
            tokens: Vec::new(),
        }
    }

    fn style(&self, kind: TokenKind) -> HighlightStyle {
        let p = self.palette;
        let (color, weight) = match kind {
            TokenKind::Comment => (p.text_placeholder, None),
            TokenKind::Header => (p.accent_hover, Some(FontWeight::SEMIBOLD)),
            TokenKind::Key => (p.text, None),
            TokenKind::Number => (p.warning, None),
            TokenKind::String => (p.success, None),
            TokenKind::Bool => (p.accent, None),
            TokenKind::Punct => (p.text_muted, None),
        };
        HighlightStyle {
            color: Some(color),
            font_weight: weight,
            ..Default::default()
        }
    }
}

impl InputHighlighter for SwfxHighlighter {
    fn language(&self) -> SharedString {
        "swfx".into()
    }

    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        self.tokens = swofty_shaders::tokens(&text.to_string());
        self.tokens.sort_by_key(|t| t.range.start);
    }

    fn styles(
        &self,
        range: &Range<usize>,
        _resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut out = Vec::new();
        let mut at = range.start;
        for token in &self.tokens {
            if token.range.end <= at {
                continue;
            }
            if token.range.start >= range.end {
                break;
            }
            let start = token.range.start.max(at);
            let end = token.range.end.min(range.end);
            if start > at {
                out.push((at..start, HighlightStyle::default()));
            }
            out.push((start..end, self.style(token.kind)));
            at = end;
        }
        if at < range.end {
            out.push((at..range.end, HighlightStyle::default()));
        }
        out
    }

    fn fold_ranges(&self, _text: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

pub(super) fn lsp_position(text: &Rope, offset: usize) -> lsp_types::Position {
    text.offset_to_position(offset.min(text.len()))
}

pub(super) struct Completer {
    pub(super) catalog: SharedCatalog,
}

impl CompletionProvider for Completer {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _trigger: CompletionContext,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<anyhow::Result<CompletionResponse>> {
        let catalog = self.catalog.borrow().clone();
        let source = text.to_string();
        let items = swofty_shaders::completions(&source, offset.min(source.len()), &catalog)
            .into_iter()
            .map(|c| CompletionItem {
                kind: Some(match c.kind {
                    CompletionKind::Effect => CompletionItemKind::CLASS,
                    CompletionKind::Param => CompletionItemKind::PROPERTY,
                    CompletionKind::Value => CompletionItemKind::VALUE,
                    CompletionKind::Keyword => CompletionItemKind::KEYWORD,
                }),
                detail: (!c.detail.is_empty()).then_some(c.detail),
                documentation: (!c.doc.is_empty()).then_some(Documentation::String(c.doc)),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range: lsp_types::Range {
                        start: lsp_position(text, c.range.start),
                        end: lsp_position(text, c.range.end),
                    },
                    new_text: c.insert,
                })),
                label: c.label,
                ..Default::default()
            })
            .collect();
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _offset: usize, new_text: &str, _cx: &mut App) -> bool {
        new_text
            .chars()
            .last()
            .is_some_and(|c| c.is_alphanumeric() || matches!(c, '[' | '.' | '=' | ' ' | '"' | '_'))
    }
}

pub(super) struct Hoverer {
    pub(super) catalog: SharedCatalog,
    pub(super) problems: SharedProblems,
}

impl HoverProvider for Hoverer {
    fn hover(
        &self,
        text: &Rope,
        offset: usize,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<anyhow::Result<Option<lsp_types::Hover>>> {
        let catalog = self.catalog.borrow().clone();
        let source = text.to_string();
        let docs = swofty_shaders::hover(&source, offset.min(source.len()), &catalog);
        let notes: Vec<String> = self
            .problems
            .borrow()
            .iter()
            .filter(|p| squiggle_range(&source, &p.range).is_some_and(|r| r.contains(&offset)))
            .map(|p| format!("**{}** {}", severity_label(p.severity), p.message))
            .collect();
        let range = docs.as_ref().map(|h| h.range.clone());
        let value = notes
            .into_iter()
            .chain(docs.map(|h| h.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        let hover = (!value.is_empty()).then(|| lsp_types::Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: range.map(|r| lsp_types::Range {
                start: lsp_position(text, r.start),
                end: lsp_position(text, r.end),
            }),
        });
        Task::ready(Ok(hover))
    }
}

pub(super) struct Fixer {
    pub(super) problems: SharedProblems,
}

impl CodeActionProvider for Fixer {
    fn id(&self) -> SharedString {
        "swfx-fixes".into()
    }

    fn code_actions(
        &self,
        _state: Entity<EditorState>,
        range: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<anyhow::Result<Vec<CodeAction>>> {
        let actions = self
            .problems
            .borrow()
            .iter()
            .filter(|p| p.range.start <= range.end && range.start <= p.range.end)
            .filter_map(|p| {
                let fix = p.fix.as_ref()?;
                Some(CodeAction {
                    title: fix.title.clone(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    data: serde_json::to_value((fix.range.start, fix.range.end, &fix.replacement)).ok(),
                    ..Default::default()
                })
            })
            .collect();
        Task::ready(Ok(actions))
    }

    fn perform_code_action(
        &self,
        state: Entity<EditorState>,
        action: CodeAction,
        _push_to_history: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Task<anyhow::Result<()>> {
        if let Some((start, end, text)) = action
            .data
            .and_then(|d| serde_json::from_value::<(usize, usize, String)>(d).ok())
        {
            state.update(cx, |s, cx| apply_edit(s, start..end, &text, window, cx));
        }
        Task::ready(Ok(()))
    }
}

pub(in crate::features::shaders) fn apply_edit(
    state: &mut EditorState,
    range: Range<usize>,
    new_text: &str,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    let rope = state.text().clone();
    let len = rope.len();
    let edit = TextEdit {
        range: lsp_types::Range {
            start: lsp_position(&rope, range.start.min(len)),
            end: lsp_position(&rope, range.end.min(len)),
        },
        new_text: new_text.to_owned(),
    };
    state.apply_lsp_edits(&vec![edit], window, cx);
}

pub(super) fn problem_of(d: swofty_shaders::Diagnostic) -> Problem {
    Problem {
        severity: d.severity,
        range: d.range,
        line: d.start.line,
        column: d.start.character,
        message: d.message,
        fix: d.fix,
    }
}
