use std::collections::HashSet;

use swofty_formats::gvdb::Gvdb;
use swofty_formats::hash::fnv1a64;

use super::{ACTIVITIES, CONTENT_SETS, TRACKS};

pub(super) const TRACK_DATA: &str = "track_data.game_setup.TrackDataSet";
pub(super) const GAME_MODES: &str = "game_setup.game_setup_modes.game_setup_mode";

/// A track a build's database names that its tracks table doesn't hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MissingTrack {
    /// The record that names it.
    pub key: u64,
    /// The row that names it, such as `race weekend bahrain`.
    pub at: String,
    pub track: String,
    pub fix: MissingFix,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MissingFix {
    /// The record has its shipped track again.
    Stock(String),
    /// The track is out of the record's list.
    Dropped,
    /// The record is as it was: the game ships no track there the build holds.
    Kept,
}

impl MissingTrack {
    /// The build's warning, with the addon that set the record (`by`) and the addon
    /// that adds the track and is off (`adds`) when they are known.
    pub(crate) fn line(&self, by: Option<&str>, adds: Option<&str>) -> String {
        let (at, track) = (&self.at, &self.track);
        let head = match (by, &self.fix) {
            (Some(by), MissingFix::Dropped) => format!("{by} adds {track} to {at}"),
            (Some(by), _) => format!("{by} sets {at} to {track}"),
            (None, MissingFix::Dropped) => format!("{at} lists {track}"),
            (None, _) => format!("{at} is set to {track}"),
        };
        let fix = match adds {
            Some(addon) => format!("which is off: turn {addon} on"),
            None => "which isn't in this build".to_owned(),
        };
        match &self.fix {
            MissingFix::Stock(stock) => format!("{head}, {fix}; until then this build puts back {stock}"),
            MissingFix::Dropped => format!("{head}, {fix}; until then this build leaves it out"),
            MissingFix::Kept if adds.is_some() => {
                format!("{head}, {fix} or change it in DB Editor; the game won't start like this")
            }
            MissingFix::Kept => {
                format!("{head}, {fix}: change it in DB Editor; the game won't start like this")
            }
        }
    }
}

/// Points each record that names a track the build doesn't hold at its shipped track
/// again, or takes the track out of a record's list; a record the game ships with no
/// track the build holds is only named. The build holds its tracks table's tracks and
/// `in_game`, the tracks whose rows the game adds. A track the game ships the record
/// with is left as it is, and race weekends at a `left_out` track are
/// [`restore_weekends`]'s.
pub(crate) fn keep_track_refs(
    db: &mut Gvdb,
    shipped: &Gvdb,
    in_game: &[&str],
    left_out: &[&str],
) -> Vec<MissingTrack> {
    let text = |db: &Gvdb, path: &str| db.get(path).map(|r| r.text().trim().to_owned());
    let count = |db: &Gvdb, table: &str| {
        let (container, row) = table.rsplit_once('.').expect("a table path");
        text(db, &swofty_formats::gvdb::instance_count_path(container, row))
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(0)
    };
    let label = |db: &Gvdb, table: &str, i: usize| {
        text(db, &format!("{table}_{i}._metadata_")).unwrap_or_else(|| i.to_string())
    };
    let mut have: HashSet<String> = (0..count(db, TRACKS))
        .filter_map(|i| text(db, &format!("{TRACKS}_{i}._metadata_")))
        .collect();
    if have.is_empty() {
        return Vec::new();
    }
    have.extend(in_game.iter().map(|c| (*c).to_owned()));
    let mut refs: Vec<(String, String, bool)> = Vec::new();
    for i in 0..count(db, ACTIVITIES) {
        let path = format!("{ACTIVITIES}_{i}.track");
        if text(db, &path).is_some_and(|c| left_out.contains(&c.as_str())) {
            continue;
        }
        refs.push((path, format!("race weekend {}", label(db, ACTIVITIES, i)), false));
    }
    for s in 0..count(db, TRACK_DATA) {
        let rows = format!("{TRACK_DATA}_{s}.TrackData");
        let set = label(db, TRACK_DATA, s);
        for i in 0..count(db, &rows) {
            let at = format!("track settings {set}/{}", label(db, &rows, i));
            refs.push((format!("{rows}_{i}.track"), at, false));
        }
    }
    for s in 0..count(db, CONTENT_SETS) {
        let rows = format!("{CONTENT_SETS}_{s}.available_tracks");
        let at = format!("content set {}", label(db, CONTENT_SETS, s));
        for i in 0..count(db, &rows) {
            refs.push((format!("{rows}_{i}.tracks"), at.clone(), true));
        }
    }
    for i in 0..count(db, GAME_MODES) {
        let at = format!("game mode {}", label(db, GAME_MODES, i));
        refs.push((format!("{GAME_MODES}_{i}.restricted_tracks"), at, true));
    }
    let mut out = Vec::new();
    for (path, row, list) in refs {
        let key = fnv1a64(&path);
        let Some(now) = db.get_key(key).map(|r| r.text().trim().to_owned()) else {
            continue;
        };
        let shipped_text = shipped
            .get_key(key)
            .map(|r| r.text().trim().to_owned())
            .unwrap_or_default();
        if !list {
            if now.is_empty() || have.contains(&now) || now == shipped_text {
                continue;
            }
            let fix = match shipped.get_key(key).filter(|r| have.contains(r.text().trim())) {
                Some(r) => {
                    if let Some((tag, value)) = db.records.value_mut(key) {
                        (*tag, *value) = (r.tag, r.value.clone());
                    }
                    MissingFix::Stock(r.text().trim().to_owned())
                }
                None => MissingFix::Kept,
            };
            out.push(MissingTrack {
                key,
                at: row,
                track: now,
                fix,
            });
            continue;
        }
        let codes: Vec<&str> = now.split(',').map(str::trim).filter(|c| !c.is_empty()).collect();
        let (kept, gone): (Vec<&str>, Vec<&str>) = codes
            .into_iter()
            .partition(|c| have.contains(*c) || shipped_text.split(',').any(|s| s.trim() == *c));
        if gone.is_empty() {
            continue;
        }
        if let Some((_, value)) = db.records.value_mut(key) {
            *value = kept.join(",").into_bytes();
        }
        out.extend(gone.into_iter().map(|track| MissingTrack {
            key,
            at: row.clone(),
            track: track.to_owned(),
            fix: MissingFix::Dropped,
        }));
    }
    out
}
