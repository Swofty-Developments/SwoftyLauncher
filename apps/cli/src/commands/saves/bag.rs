use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};
use swofty_core::saves::bag::{self, Bag, List, Value};

use crate::console::output::say;

fn scalar(v: &Value) -> String {
    match serde_json::to_value(v) {
        Ok(mut j) => j
            .get_mut("value")
            .map_or_else(String::new, |x| x.take().to_string()),
        Err(_) => String::new(),
    }
}

fn type_name(v: &Value) -> String {
    String::from_utf8_lossy(&v.type_id().to_le_bytes()).into_owned()
}

fn render_value(out: &mut String, label: &str, v: &Value, depth: usize) {
    let pad = "  ".repeat(depth);
    match v {
        Value::Bag(b) => {
            let _ = writeln!(out, "{pad}{label}: pbag ({} entries)", b.entries.len());
            render_bag(out, b, depth + 1);
        }
        Value::List(l) => {
            let _ = writeln!(out, "{pad}{label}: plst ({} items)", l.items.len());
            render_list(out, l, depth + 1);
        }
        other => {
            let _ = writeln!(out, "{pad}{label}: {} = {}", type_name(other), scalar(other));
        }
    }
}

fn render_bag(out: &mut String, bag: &Bag, depth: usize) {
    for e in &bag.entries {
        let label = e
            .key
            .as_str()
            .map_or_else(|| format!("{:?}", e.key.0), str::to_owned);
        render_value(out, &label, &e.value, depth);
    }
}

fn render_list(out: &mut String, list: &List, depth: usize) {
    for (i, v) in list.items.iter().enumerate() {
        render_value(out, &format!("[{i}]"), v, depth);
    }
}

pub(super) fn decode_bag(file: &Path, json: bool) -> Result<()> {
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    let tree = bag::decode(&bytes).with_context(|| format!("decoding {}", file.display()))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&tree)?);
        return Ok(());
    }
    let mut out = String::new();
    let _ = writeln!(
        out,
        "pbag v{} ({} entries, {} bytes)",
        tree.version,
        tree.entries.len(),
        bytes.len()
    );
    render_bag(&mut out, &tree, 1);
    print!("{out}");
    say("ok", "the stream decodes");
    Ok(())
}
