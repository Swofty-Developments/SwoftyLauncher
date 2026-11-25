//! Data-only selection dialogs. Send these only when hello advertises
//! `view.selection_dialog`; older launchers cannot read this button kind.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SelectionDialog {
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    pub options: Vec<SelectionOption>,
    #[serde(default)]
    pub selected: Vec<String>,
    /// False draws radio buttons; true draws checkboxes and Select all / Clear.
    #[serde(default)]
    pub multiple: bool,
    /// Whether confirming an empty selection is meaningful to the addon.
    #[serde(default)]
    pub allow_empty: bool,
    /// The primary action's label. The launcher defaults to Apply.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apply: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectionOption {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl SelectionDialog {
    pub const MAX_OPTIONS: usize = 256;
    pub const MAX_FIELD_CHARS: usize = 256;
    pub const MAX_TEXT_CHARS: usize = 2000;

    pub fn validate(&self) -> Result<(), String> {
        if self.options.len() > Self::MAX_OPTIONS {
            return Err(format!(
                "a selection dialog supports at most {} options",
                Self::MAX_OPTIONS
            ));
        }
        let long = |s: &str| s.chars().count() > Self::MAX_FIELD_CHARS;
        if long(&self.title)
            || self.apply.as_deref().is_some_and(long)
            || self.options.iter().any(|o| {
                long(&o.value)
                    || long(&o.label)
                    || o.group.as_deref().is_some_and(long)
                    || o.help.as_deref().is_some_and(long)
            })
        {
            return Err(format!(
                "selection titles, labels, values, groups and help are at most {} characters",
                Self::MAX_FIELD_CHARS
            ));
        }
        if self.text.chars().count() > Self::MAX_TEXT_CHARS {
            return Err(format!(
                "selection text is at most {} characters",
                Self::MAX_TEXT_CHARS
            ));
        }
        let mut options = HashSet::new();
        for option in &self.options {
            if option.value.is_empty() || !options.insert(option.value.as_str()) {
                return Err("selection options need unique, non-empty values".into());
            }
        }
        let mut selected = HashSet::new();
        for value in &self.selected {
            if !options.contains(value.as_str()) || !selected.insert(value) {
                return Err("selected values must name distinct selection options".into());
            }
        }
        if !self.multiple && self.selected.len() > 1 {
            return Err("a single-choice dialog has at most one selected value".into());
        }
        Ok(())
    }

    /// Validates a confirmed selection, including an unchanged default. The
    /// empty initial selection is allowed even when confirmation needs a choice.
    pub fn accepts(&self, values: &[String]) -> bool {
        if (!self.allow_empty && values.is_empty()) || (!self.multiple && values.len() > 1) {
            return false;
        }
        let options: HashSet<&str> = self.options.iter().map(|o| o.value.as_str()).collect();
        let mut seen = HashSet::with_capacity(values.len());
        values
            .iter()
            .all(|value| options.contains(value.as_str()) && seen.insert(value.as_str()))
    }
}
