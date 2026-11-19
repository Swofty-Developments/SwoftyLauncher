//! How often the shell of swofty-core names an F1 25 module (`crate::stdlib`, `use crate::{build, ...}`), per
//! file, against `boundary.json`. The F1 25 modules are the ones `tools/games-move/modules.txt` lists. The
//! check starts once `tools/games-move/move.py` has moved them under `src/games/f1_25`; after that a file
//! that names them more often fails, and `SWOFTY_BOUNDARY_UPDATE=1` writes the lower counts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CRATE: &str = env!("CARGO_MANIFEST_DIR");
const MODULES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/games-move/modules.txt");
const BASELINE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/boundary.json");

fn modules() -> Vec<String> {
    std::fs::read_to_string(MODULES)
        .unwrap_or_else(|e| panic!("{MODULES}: {e}"))
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.trim_end_matches(".rs").to_owned())
        .collect()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|x| x == "rs") {
            out.push(path);
        }
    }
}

fn is_shell(rel: &str, modules: &[String]) -> bool {
    let f1 = rel == "games/f1_25.rs"
        || rel.starts_with("games/f1_25/")
        || modules
            .iter()
            .any(|m| rel == format!("{m}.rs") || rel.starts_with(&format!("{m}/")));
    let test = rel.split('/').any(|part| part == "tests" || part == "tests.rs");
    !f1 && !test && rel != "lib.rs"
}

fn ident(text: &str) -> &str {
    let end = text
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(text.len());
    &text[..end]
}

fn group_items(group: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut depth = 0;
    let mut start = true;
    for (i, c) in group.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 0 => break,
            '}' => depth -= 1,
            ',' if depth == 0 => start = true,
            c if c.is_whitespace() => {}
            c if depth == 0 && start && (c.is_alphanumeric() || c == '_') => {
                items.push(ident(&group[i..]));
                start = false;
            }
            _ => {
                if depth == 0 {
                    start = false;
                }
            }
        }
    }
    items
}

/// How many paths in `source` start at one of `modules` through `crate::`. Inline test modules don't count.
fn edges(source: &str, modules: &[String]) -> usize {
    let source = source
        .match_indices("#[cfg(test)]")
        .find(|(at, m)| source[at + m.len()..].trim_start().starts_with("mod "))
        .map_or(source, |(at, _)| &source[..at]);
    let named = |name: &str| modules.iter().any(|m| m == name);
    let mut count = 0;
    let mut rest = source;
    while let Some(at) = rest.find("crate::") {
        let before = rest[..at].chars().next_back();
        let after = &rest[at + "crate::".len()..];
        if !before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            count += match after.strip_prefix('{') {
                Some(group) => group_items(group).into_iter().filter(|i| named(i)).count(),
                None => usize::from(named(ident(after))),
            };
        }
        rest = after;
    }
    count
}

fn counts() -> BTreeMap<String, usize> {
    let modules = modules();
    let src = Path::new(CRATE).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    files
        .into_iter()
        .filter_map(|path| {
            let rel = path
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if !is_shell(&rel, &modules) {
                return None;
            }
            let n = edges(&std::fs::read_to_string(&path).unwrap(), &modules);
            (n > 0).then_some((rel, n))
        })
        .collect()
}

#[test]
fn the_shell_names_the_f1_25_modules_no_more_often_than_boundary_json_allows() {
    let moved = Path::new(CRATE).join("src/games/f1_25");
    if !moved.is_dir() {
        eprintln!("inactive until tools/games-move/move.py moves the F1 25 modules under src/games/f1_25");
        return;
    }
    for m in modules() {
        let dir = moved.join(&m);
        assert!(
            dir.is_dir() || dir.with_extension("rs").is_file(),
            "{m} is in modules.txt but not under src/games/f1_25"
        );
    }
    let now = counts();
    if std::env::var_os("SWOFTY_BOUNDARY_UPDATE").is_some() {
        let text = serde_json::to_string_pretty(&now).unwrap();
        std::fs::write(BASELINE, text + "\n").unwrap();
        return;
    }
    let text = std::fs::read_to_string(BASELINE)
        .unwrap_or_else(|e| panic!("{BASELINE}: {e}; write it with SWOFTY_BOUNDARY_UPDATE=1"));
    let allowed: BTreeMap<String, usize> = serde_json::from_str(&text).unwrap();
    let grown: Vec<String> = now
        .iter()
        .filter(|(file, n)| **n > allowed.get(*file).copied().unwrap_or(0))
        .map(|(file, n)| {
            format!(
                "{file}: names the F1 25 modules {n} times, {} allowed",
                allowed.get(file).copied().unwrap_or(0)
            )
        })
        .collect();
    assert!(
        grown.is_empty(),
        "reach F1 25 through a service on its GameDef instead of a crate:: path:\n{}",
        grown.join("\n")
    );
    let shrunk = allowed
        .iter()
        .filter(|(file, n)| now.get(*file).copied().unwrap_or(0) < **n)
        .count();
    if shrunk > 0 {
        eprintln!(
            "{shrunk} files name the F1 25 modules less often than boundary.json allows; lower it with SWOFTY_BOUNDARY_UPDATE=1"
        );
    }
}

#[test]
fn it_counts_paths_and_grouped_uses_into_the_f1_25_modules() {
    let modules: Vec<String> = ["stdlib", "build", "saves"].map(String::from).into();
    let source = r#"
        use crate::stdlib::deps;
        use crate::{build::Plan, game_folder::{saves, copies}, saves::vault};
        use crate::store::Store;
        fn f() { crate::build::build(); swofty_crate::stdlib::x(); }
        #[cfg(test)]
        mod tests { use crate::stdlib::paths; }
    "#;
    assert_eq!(edges(source, &modules), 4);
    assert!(is_shell("install/apply.rs", &modules));
    assert!(!is_shell("stdlib/deps.rs", &modules));
    assert!(!is_shell("games/f1_25/stdlib/deps.rs", &modules));
    assert!(!is_shell("install/tests.rs", &modules));
    assert!(!is_shell("build/tests/digests.rs", &modules));
    assert!(!is_shell("lib.rs", &modules));
}

#[test]
fn modules_txt_names_every_module_that_moves() {
    let modules = modules();
    assert_eq!(modules.len(), 17);
    for m in &modules {
        let src = Path::new(CRATE).join("src");
        let found = [
            src.join(m),
            src.join(format!("{m}.rs")),
            src.join("games/f1_25").join(m),
        ]
        .iter()
        .any(|p| p.exists() || p.with_extension("rs").is_file());
        assert!(found, "{m} is not a module of swofty-core");
    }
}
