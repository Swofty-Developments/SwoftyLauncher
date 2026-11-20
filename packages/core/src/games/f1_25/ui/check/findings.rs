//! What a check finds, at what level, and the report the findings of one edit add up to.

use crate::ui::catalog::EngineRule;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Hard,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub level: Level,
    pub code: String,
    pub node_type: String,
    pub name: String,
    pub at: String,
    pub message: String,
    pub source: String,
}

impl Finding {
    pub fn warn(code: &str, node_type: &str, name: &str, at: &str, message: String) -> Self {
        Self {
            level: Level::Warn,
            code: code.to_owned(),
            node_type: node_type.to_owned(),
            name: name.to_owned(),
            at: at.to_owned(),
            message,
            source: "catalogue".into(),
        }
    }

    pub(super) fn stock_diff(code: &str, node_type: &str, name: &str, at: &str, message: String) -> Self {
        Self {
            source: "stock diff".into(),
            ..Self::warn(code, node_type, name, at, message)
        }
    }

    pub(super) fn engine(rule: &EngineRule, at: &str, message: String) -> Self {
        Self {
            level: Level::Hard,
            source: rule.cite(),
            ..Self::warn(&rule.id, "", "", at, message)
        }
    }

    pub(super) fn hard(code: &str, at: &str, message: String, source: &str) -> Self {
        Self {
            level: Level::Hard,
            source: source.to_owned(),
            ..Self::warn(code, "", "", at, message)
        }
    }
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} [{}]", self.at, self.message, self.source)
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub hard: Vec<Finding>,
    pub warnings: Vec<Finding>,
}

impl Report {
    pub fn add(&mut self, f: Finding) {
        match f.level {
            Level::Hard => self.hard.push(f),
            Level::Warn => self.warnings.push(f),
        }
    }

    pub fn extend(&mut self, other: Report) {
        self.hard.extend(other.hard);
        self.warnings.extend(other.warnings);
    }
}
