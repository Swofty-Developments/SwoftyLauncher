//! Editor support: highlighting tokens, completions and hover docs.

mod completions;
mod context;
mod hover;
mod tokens;

use crate::text::number;
use crate::{Param, ParamKind, Value};
pub use completions::completions;
pub use hover::hover;
pub use tokens::tokens;

fn default_text(p: &Param) -> String {
    let float = !matches!(p.kind, ParamKind::Int { .. });
    match &p.default {
        Value::Number(n) => number(*n, float),
        Value::Bool(b) => b.to_string(),
        Value::Text(t) => format!("\"{t}\""),
        Value::Vector(v) => format!(
            "[{}]",
            v.iter().map(|n| number(*n, true)).collect::<Vec<_>>().join(", ")
        ),
    }
}

#[cfg(test)]
mod tests;
