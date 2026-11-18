use std::collections::HashSet;

pub(super) fn in_comment(prefix: &str) -> bool {
    let (mut dq, mut sq, mut escaped) = (false, false, false);
    for c in prefix.chars() {
        if dq && escaped {
            escaped = false;
        } else if dq && c == '\\' {
            escaped = true;
        } else if c == '"' && !sq {
            dq = !dq;
        } else if c == '\'' && !dq {
            sq = !sq;
        } else if c == '#' && !dq && !sq {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Section {
    Top,
    Effect(String),
    Rule,
    Set(String),
}

fn header_section(line: &str) -> Option<Section> {
    let t = line.trim_start_matches([' ', '\t']);
    let rest = t.strip_prefix('[')?;
    if let Some(inner) = rest.strip_prefix('[') {
        let word: String = inner
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        return Some(if word == "when" {
            Section::Rule
        } else {
            Section::Effect(String::new())
        });
    }
    let inner = rest.split(']').next().unwrap_or("");
    let mut parts = inner.split('.').map(str::trim);
    let id: String = parts
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    if id == "when" {
        if parts.next() == Some("set") {
            return Some(Section::Set(parts.next().unwrap_or("").to_owned()));
        }
        return Some(Section::Effect(id));
    }
    Some(Section::Effect(id))
}

fn key_of(line: &str) -> Option<String> {
    let t = line.trim_start_matches([' ', '\t']);
    if t.starts_with(['#', '[']) {
        return None;
    }
    let key: String = t
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let rest = t[key.len()..].trim_start_matches([' ', '\t']);
    (!key.is_empty() && rest.starts_with('=')).then_some(key)
}

/// The table the line at `line_start` belongs to and the keys that table already sets on other lines.
pub(super) fn table_context(text: &str, line_start: usize) -> (Section, HashSet<String>) {
    let mut tables: Vec<(Section, HashSet<String>)> = vec![(Section::Top, HashSet::new())];
    let mut found = None;
    let mut offset = 0;
    for raw in text.split_inclusive('\n') {
        let start = offset;
        offset += raw.len();
        let line = raw.trim_end_matches(['\n', '\r']);
        if let Some(section) = header_section(line) {
            tables.push((section, HashSet::new()));
            if start == line_start {
                found = Some(tables.len() - 1);
            }
        } else if start == line_start {
            found = Some(tables.len() - 1);
        } else if let Some(k) = key_of(line) {
            tables.last_mut().expect("never empty").1.insert(k);
        }
    }
    tables.swap_remove(found.unwrap_or(tables.len() - 1))
}

pub(super) fn word_end(text: &str, from: usize) -> usize {
    let rest = &text[from..];
    from + rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
        .unwrap_or(rest.len())
}
