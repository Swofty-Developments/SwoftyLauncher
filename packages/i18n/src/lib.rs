//! The launcher's translations. The English text in the code is the key:
//! `tr("Settings")` looks it up in the chosen language, and text with no
//! translation stays English.
//!
//! To add a language, add `locales/<code>.json` with its name under `_language`
//! and what it translates. It then shows up under Language in Settings.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

pub const ENGLISH: &str = "en";
/// Where a locale file gives its language's own name.
const NAME_KEY: &str = "_language";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    pub code: &'static str,
    pub name: &'static str,
}

struct Loaded {
    language: Language,
    table: HashMap<&'static str, &'static str>,
}

/// English first, so index 0 is what an unknown language falls back to. The text
/// lives as long as the launcher, which lets `tr` hand back a `&'static str`.
static LOADED: LazyLock<Vec<Loaded>> = LazyLock::new(|| {
    let mut all: Vec<Loaded> = FILES.iter().filter_map(|(code, text)| load(code, text)).collect();
    all.sort_by_key(|l| (l.language.code != ENGLISH, l.language.code));
    all
});
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// A file that can't be read leaves its language out rather than stopping the
/// launcher; the tests here fail on one.
fn load(code: &'static str, text: &str) -> Option<Loaded> {
    let mut table: BTreeMap<String, String> = serde_json::from_str(text).ok()?;
    let name: &'static str = table.remove(NAME_KEY)?.leak();
    let table = table.into_iter().map(|(k, v)| (&*k.leak(), &*v.leak())).collect();
    Some(Loaded {
        language: Language { code, name },
        table,
    })
}

pub fn languages() -> Vec<Language> {
    LOADED.iter().map(|l| l.language).collect()
}

/// `code` from now on; an unknown code, or `None`, is English.
pub fn set_language(code: Option<&str>) {
    let found = code.and_then(|code| LOADED.iter().position(|l| l.language.code == code));
    CURRENT.store(found.unwrap_or(0), Ordering::Relaxed);
}

pub fn language() -> &'static str {
    LOADED
        .get(CURRENT.load(Ordering::Relaxed))
        .map_or(ENGLISH, |l| l.language.code)
}

/// `text` in the chosen language.
pub fn tr(text: &'static str) -> &'static str {
    match LOADED.get(CURRENT.load(Ordering::Relaxed)) {
        Some(l) => l.table.get(text).copied().unwrap_or(text),
        None => text,
    }
}

#[cfg(test)]
fn translate(code: &str, text: &'static str) -> &'static str {
    LOADED
        .iter()
        .find(|l| l.language.code == code)
        .and_then(|l| l.table.get(text).copied())
        .unwrap_or(text)
}

/// `text` with each `{name}` replaced by its value, once: a value holding `{x}`
/// stays as it is. Any other `{` stays too.
pub fn fill(text: &str, args: &[(&str, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let found = after.find('}').and_then(|close| {
            let value = args.iter().find(|(name, _)| *name == &after[..close])?;
            Some((close, &value.1))
        });
        match found {
            Some((close, value)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// [`tr`] for text with values in it: `trf!("{count} addons", count = n)`.
#[macro_export]
macro_rules! trf {
    ($text:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::fill($crate::tr($text), &[$((stringify!($name), ($value).to_string())),*])
    };
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use super::*;

    const SCANNED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/launcher/src");

    fn files() -> Vec<(&'static str, BTreeMap<String, String>)> {
        FILES
            .iter()
            .map(|(code, text)| {
                let table = serde_json::from_str(text).unwrap_or_else(|e| panic!("{code}.json: {e}"));
                (*code, table)
            })
            .collect()
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|rest| rest.split_once('}'))
            .map(|(name, _)| name)
            .collect()
    }

    /// The English text of every `tr("…")` and `trf!("…"` in `source`.
    fn wrapped(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        for call in ["tr(", "trf!("] {
            for (at, _) in source.match_indices(call) {
                let is_call = !source[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_');
                let rest = source[at + call.len()..].trim_start();
                let Some(rest) = rest.strip_prefix('"').filter(|_| is_call) else {
                    continue;
                };
                let mut text = String::new();
                let mut chars = rest.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => text.push('\n'),
                            Some(other) => text.push(other),
                            None => break,
                        },
                        _ => text.push(c),
                    }
                }
                found.push(text);
            }
        }
        found
    }

    fn sources(dir: &Path, out: &mut String) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push_str(&std::fs::read_to_string(&path).unwrap());
            }
        }
    }

    #[test]
    fn a_language_shows_its_own_name_with_english_first() {
        let all = languages();
        assert_eq!(
            all[0],
            Language {
                code: ENGLISH,
                name: "English"
            }
        );
        assert!(
            all.contains(&Language {
                code: "ru",
                name: "Русский"
            }),
            "{all:?}"
        );
        assert!(
            all.contains(&Language {
                code: "zh-CN",
                name: "简体中文"
            }),
            "{all:?}"
        );
    }

    #[test]
    fn text_is_translated_and_anything_unknown_stays_english() {
        assert_eq!(translate("ru", "Settings"), "Настройки");
        assert_eq!(translate("zh-CN", "Settings"), "设置");
        assert_eq!(translate(ENGLISH, "Settings"), "Settings");
        assert_eq!(
            translate("ru", "Text nobody translated"),
            "Text nobody translated"
        );
        assert_eq!(translate("xx", "Settings"), "Settings");
    }

    #[test]
    fn fill_replaces_each_name_once_and_leaves_the_rest() {
        let args = [("a", "{b}".to_owned()), ("b", "2".to_owned())];
        assert_eq!(fill("{a} and {b} and {c} {", &args), "{b} and 2 and {c} {");
    }

    #[test]
    fn choosing_a_language_changes_what_tr_returns() {
        set_language(Some("ru"));
        assert_eq!((language(), tr("Settings")), ("ru", "Настройки"));
        assert_eq!(trf!("Settings {n}", n = 2), "Settings 2");
        set_language(Some("no-such-language"));
        assert_eq!((language(), tr("Settings")), (ENGLISH, "Settings"));
        set_language(Some("zh-CN"));
        assert_eq!(tr("Settings"), "设置");
        set_language(None);
        assert_eq!(language(), ENGLISH);
    }

    #[test]
    fn every_locale_file_names_its_language_and_keeps_the_placeholders() {
        for (code, table) in files() {
            let name = table.get(NAME_KEY).map_or("", String::as_str);
            assert!(!name.is_empty(), "{code}.json has no {NAME_KEY}");
            for (text, translated) in table.iter().filter(|(k, _)| *k != NAME_KEY) {
                assert_eq!(
                    placeholders(text),
                    placeholders(translated),
                    "{code}.json: {text:?} -> {translated:?}"
                );
            }
        }
    }

    #[test]
    fn every_translation_is_of_text_the_launcher_still_has() {
        let mut source = String::new();
        sources(Path::new(SCANNED), &mut source);
        let used: BTreeSet<String> = wrapped(&source).into_iter().collect();
        for (code, table) in files() {
            for text in table.keys().filter(|k| *k != NAME_KEY) {
                assert!(
                    used.contains(text),
                    "{code}.json translates {text:?}, which no tr() uses"
                );
            }
        }
    }
}
