use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use swofty_formats::gvdb::Gvdb;

use crate::content::track_package::TrackPackage;
use crate::{Result, io_err};

use super::{Rows, TRACKS};

/// `$DATA/registry/older-tracks.json`: the tracks `track.add` adds for addons with no track record.
pub const OLDER_FILE: &str = "older-tracks.json";
pub const OLDER_VERSION: u32 = 1;
const VENUES_JSON: &str = include_str!("../../../../../sdk/std/data/venues.json");

/// Writes `$DATA/registry/older-tracks.json` for the tracks `track.add` added for addons with no
/// track record; `shipped` and `built` are the game's database and the build's.
pub fn write_older(
    data: &Path,
    profile: &str,
    added: &[(&str, &TrackPackage)],
    shipped: &Path,
    built: &Path,
) -> Result<()> {
    let path = crate::stdlib::reports::path(data, OLDER_FILE);
    let file = match added.is_empty() {
        true => OlderTracks {
            version: OLDER_VERSION,
            profile: profile.to_owned(),
            tracks: Vec::new(),
        },
        false => {
            let read = |p: &Path| -> Result<Gvdb> { Ok(Gvdb::parse(&std::fs::read(p).map_err(io_err(p))?)?) };
            older_tracks(profile, added, &read(shipped)?, &read(built)?, Venues::shipped())
        }
    };
    if crate::read_json::<OlderTracks>(&path).is_ok_and(|f| f == file) {
        return Ok(());
    }
    crate::write_json(&path, &file)
}

/// The tracks the last build recorded in `older-tracks.json`, for `profile`: their stat ids seed the ids the store keeps.
pub fn read_older(data: &Path, profile: &str) -> Vec<OlderTrack> {
    let path = crate::stdlib::reports::path(data, OLDER_FILE);
    crate::read_json::<OlderTracks>(&path)
        .ok()
        .filter(|f| f.profile == profile)
        .map(|f| f.tracks)
        .unwrap_or_default()
}

/// One venue of `packages/sdk/std/data/venues.json`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Venue {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub stock: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Venues {
    #[serde(default)]
    pub venues: BTreeMap<String, Venue>,
}

/// Lower case, every run of characters other than `a-z0-9` one space, trimmed.
pub fn normalise(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.to_lowercase().chars() {
        match c.is_ascii_lowercase() || c.is_ascii_digit() {
            true => out.push(c),
            false if !out.ends_with(' ') => out.push(' '),
            false => {}
        }
    }
    out.trim().to_owned()
}

impl Venues {
    pub fn parse(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }

    /// The venues the launcher ships (`packages/sdk/std/data/venues.json`).
    pub fn shipped() -> &'static Self {
        static PARSED: std::sync::OnceLock<Venues> = std::sync::OnceLock::new();
        PARSED.get_or_init(|| Self::parse(VENUES_JSON).unwrap_or_default())
    }

    /// `venue/<key>` of the one venue whose name or an alias equals `name` once normalised;
    /// `None` when none does or more than one does.
    pub fn by_name(&self, name: &str) -> Option<String> {
        let want = normalise(name);
        if want.is_empty() {
            return None;
        }
        let mut found = self.venues.iter().filter(|(_, v)| {
            std::iter::once(&v.name)
                .chain(&v.aliases)
                .any(|n| normalise(n) == want)
        });
        let (key, _) = found.next()?;
        found.next().is_none().then(|| format!("venue/{key}"))
    }

    /// Whether a stock track with assets provides `id` (`venue/<key>`).
    pub fn stock_provides(&self, id: &str) -> bool {
        id.strip_prefix("venue/")
            .and_then(|k| self.venues.get(k))
            .is_some_and(|v| v.stock.as_deref().is_some_and(|s| !s.is_empty()))
    }
}

/// One entry of `older-tracks.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OlderTrack {
    pub addon: String,
    pub code: String,
    pub name: String,
    pub base: String,
    pub slot: Option<String>,
    pub stat_id: Option<u32>,
    pub venue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OlderTracks {
    pub version: u32,
    pub profile: String,
    pub tracks: Vec<OlderTrack>,
}

fn label_rows(db: &Gvdb) -> Vec<(String, Option<u32>)> {
    Rows::new(db)
        .table(TRACKS, &["_metadata_", "statId"])
        .into_iter()
        .filter_map(|r| Some((r[0].clone()?, r[1].as_deref().and_then(|s| s.parse().ok()))))
        .collect()
}

/// `older-tracks.json` for the tracks `track.add` added in a build, by addon id in the build's
/// order: `shipped` is the game's own database, `built` the build's. `slot` is the stock row a
/// track fills (`game/<code>`: a row the game ships with the track's code), else null; `base` is
/// the track its rows were cloned from.
pub fn older_tracks(
    profile: &str,
    added: &[(&str, &TrackPackage)],
    shipped: &Gvdb,
    built: &Gvdb,
    venues: &Venues,
) -> OlderTracks {
    let stock: Vec<String> = label_rows(shipped).into_iter().map(|(c, _)| c).collect();
    let ids = label_rows(built);
    let tracks = added
        .iter()
        .map(|(addon, pkg)| {
            let code = pkg.track_code.clone();
            let name = pkg.display_name().trim().to_owned();
            OlderTrack {
                addon: (*addon).to_owned(),
                venue: pkg.track_name.as_deref().and_then(|n| venues.by_name(n)),
                base: format!("game/{}", pkg.activation.base_track().unwrap_or(&code)),
                slot: stock.contains(&code).then(|| format!("game/{code}")),
                stat_id: ids.iter().find(|(c, _)| *c == code).and_then(|(_, id)| *id),
                name: match name.is_empty() {
                    true => code.clone(),
                    false => name,
                },
                code,
            }
        })
        .collect();
    OlderTracks {
        version: OLDER_VERSION,
        profile: profile.to_owned(),
        tracks,
    }
}
