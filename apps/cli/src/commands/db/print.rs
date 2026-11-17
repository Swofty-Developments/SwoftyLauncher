use std::collections::BTreeMap;

use swofty_core::dbedit::{Catalog, Edits, NodeId, NodeKind, RowCopy, deps};

pub(super) fn row_line(r: &RowCopy) -> String {
    let from = r.from.as_deref().unwrap_or("the game default row");
    format!("{}: a copy of {from}", r.select())
}

pub(super) fn print_deps(cat: &Catalog, row: NodeId) {
    let d = deps::deps(cat, row);
    let mut hows: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for n in &d.names {
        hows.entry(n.value.as_str()).or_default().push(n.how());
    }
    println!("{}", cat.path(row));
    println!("names the game can derive from the row:");
    for (value, how) in &hows {
        println!("  {value}: {}", how.join(", "));
    }
    let tag = |area: Option<&str>| area.map(|a| format!(" [{a}]")).unwrap_or_default();
    if !d.rules.is_empty() {
        println!("rows the game is known to look up for the row:");
    }
    for (rule, name, found) in &d.rules {
        let by = match rule.field {
            None => format!("labelled {name} ({})", rule.how()),
            Some(field) => format!("whose {field} names {name}"),
        };
        let found = match found {
            Some(row) => cat.path(*row),
            None => "none".into(),
        };
        println!("  {} row {by}: {found}; {}", rule.table, rule.reader);
    }
    let mut keyed: BTreeMap<(bool, String, &str), (usize, Option<&str>)> = BTreeMap::new();
    for k in d.keyed.iter().filter(|k| k.rule.is_none()) {
        let tables = match k.tables.len() {
            1 => cat.path(k.tables[0]),
            _ => k.family.clone(),
        };
        let by_label = hows[k.name.value.as_str()]
            .iter()
            .any(|h| h.starts_with("the label"));
        let n = keyed
            .entry((!by_label, tables, k.name.value.as_str()))
            .or_insert((0, k.area));
        n.0 = n.0.max(k.tables.len());
    }
    let mut section = None;
    for ((by_field, tables, value), (n, area)) in &keyed {
        if section != Some(*by_field) {
            section = Some(*by_field);
            match by_field {
                false => println!("rows of other tables labelled with the label or a name built from it:"),
                true => println!(
                    "rows of other tables labelled with a value of the row (a copy that changes it needs its own):"
                ),
            }
        }
        let tables = match n {
            1 => tables.clone(),
            n => format!("{tables} ({n} tables)"),
        };
        println!("  {tables}: {value} ({}){}", hows[value].join(", "), tag(*area));
    }
    if d.keyed.is_empty() {
        println!("rows of other tables labelled with these names: none");
    }
    println!("values that pick the row:");
    if d.references.is_empty() {
        println!("  none");
    }
    for r in &d.references {
        let by = match r.by {
            deps::By::Label => "label",
            deps::By::StatId => "stat_id",
        };
        println!("  by {by}: {} ({}){}", r.field, r.count, tag(r.area));
    }
    if d.unnamed > 0 {
        println!(
            "  {} value(s) under keys the catalogue does not name hold the label",
            d.unnamed
        );
    }
    if d.keyed.iter().any(|k| k.area.is_some()) || d.references.iter().any(|r| r.area.is_some()) {
        println!(
            "[area]: a table of a game area an add-row copy does not reach by default; the build lists its findings only with swofty build --side-data"
        );
    }
}

pub(super) fn show(cat: &Catalog, edits: &Edits, node: NodeId) {
    let n = cat.node(node);
    if let Some(r) = n.record.map(|r| cat.record(r)) {
        let value = edits.value(cat, r.key).unwrap_or("(removed)");
        println!("{} (0x{:016x}, type {})", cat.path(node), r.key, r.tag);
        println!("  value: {value}");
        if edits.get(r.key).is_some() {
            println!("  stock: {}", r.value);
        }
        if let Some(text) = cat.text(value) {
            println!("  English: {text}");
        }
        if let Some(m) = cat.meta(r.key) {
            for (k, v) in [("meaning", &m.meaning), ("unit", &m.unit), ("range", &m.range)] {
                if !v.is_empty() {
                    println!("  {k}: {v}");
                }
            }
        }
        return;
    }
    println!("{}", cat.path(node));
    if let Some(m) = cat.table_meaning(node) {
        println!("  {m}");
    }
    for &c in cat.children(node) {
        let child = cat.node(c);
        let about = match &child.kind {
            NodeKind::Table { rows, .. } => format!("table, {rows} rows"),
            NodeKind::Row { .. } => [child.label.as_deref(), child.display.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
            NodeKind::Key => child
                .record
                .map(|r| {
                    edits
                        .value(cat, cat.record(r).key)
                        .unwrap_or("(removed)")
                        .to_owned()
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        println!("  {}  {about}", child.name);
    }
}
