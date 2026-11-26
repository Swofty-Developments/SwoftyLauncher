use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

use super::data;

pub const REGISTRY: &str = "ui.tile";

pub const PLACES: [&str; 2] = ["home", "career"];

fn named_image(record: &Value) -> bool {
    record
        .get("image")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
}

fn places_of(record: &Value) -> Vec<&'static str> {
    match record.get("place").and_then(Value::as_str) {
        Some(p) => PLACES.into_iter().filter(|n| *n == p).collect(),
        None if record.get("extends").is_some() => PLACES.to_vec(),
        None => Vec::new(),
    }
}

/// The places (home, career) with a ui.tile record in the data text `text` that names an image, in the order of
/// [`PLACES`]. A record that extends another and sets no place counts for both, since its place is its base's.
pub fn image_places(text: &str) -> BTreeSet<&'static str> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return BTreeSet::new();
    };
    map.get(REGISTRY)
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(key, rec)| !key.starts_with('$') && named_image(rec))
        .flat_map(|(_, rec)| places_of(rec))
        .collect()
}

/// The places with a ui.tile record naming an image in the data files of the addon folders `dirs`, home before career.
pub fn image_places_in<'a>(dirs: impl IntoIterator<Item = &'a Path>) -> Vec<String> {
    let mut found = BTreeSet::new();
    for dir in dirs {
        for rel in data::files(dir) {
            if let Ok(text) = std::fs::read_to_string(dir.join(&rel)) {
                found.extend(image_places(&text));
            }
        }
    }
    PLACES
        .into_iter()
        .filter(|p| found.contains(p))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_with_an_image_counts_for_its_place() {
        let text = r#"{ "ui.tile": {
            "a": { "place": "home", "title": "x", "image": "mod/slice" },
            "b": { "place": "career", "title": "y" },
            "c": { "place": "f1_world", "title": "z", "image": "mod/slice" } } }"#;
        assert_eq!(image_places(text), BTreeSet::from(["home"]));
    }

    #[test]
    fn an_empty_or_missing_image_counts_for_nothing() {
        let text = r#"{ "ui.tile": { "a": { "place": "home", "title": "x", "image": "" },
                                      "b": { "place": "career", "title": "y", "image": null } } }"#;
        assert!(image_places(text).is_empty());
        assert!(image_places("not json").is_empty());
        assert!(image_places(r#"{ "ui.screen": { "a": { "image": "s" } } }"#).is_empty());
    }

    #[test]
    fn a_variant_without_a_place_counts_for_both() {
        let text = r#"{ "ui.tile": { "game/x": { "image": "mod/slice" },
                                      "v": { "extends": "mod/a", "image": "mod/slice" } } }"#;
        assert_eq!(image_places(text), BTreeSet::from(["career", "home"]));
    }
}
