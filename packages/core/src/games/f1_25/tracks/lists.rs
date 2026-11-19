use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Serialize};
use swofty_formats::gvdb::Gvdb;

use crate::Result;
use crate::content::track_package::{TrackPackage, Unusable};

use super::{BuildTrack, TRACK_DATA, TRACKS};

/// The most tracks the game's track lists hold. F1 25 gives each listed track one bit of a 64-bit set when it
/// loads its options at boot (`cmp eax, 0x40` then `bts` at 0x142ae87c7 on Steam) and throws
/// `Ne_Std_ThrowInvalidBitsetPosition` for the 65th, which leaves the screen black after the EA logo.
pub const TRACK_LIST_ROOM: usize = 64;
/// The track the game leaves out of those lists by name.
const UNLISTED_TRACK: &str = "madrid";
/// The fewest added tracks that can fill the lists: F1 25 ships 43 track rows.
pub const FILL_FROM: usize = TRACK_LIST_ROOM - 43 + 1;

/// The tracks a database puts in the game's track lists: its tracks.Track rows with a TrackData row in any
/// dataset, but madrid.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListedTracks {
    rows: BTreeSet<String>,
    with_data: BTreeSet<String>,
}

impl ListedTracks {
    pub fn of(db: &Gvdb) -> Self {
        let text = |path: &str| db.get(path).map(|r| r.text().trim().to_owned());
        let count = |table: &str| {
            let (container, row) = table.rsplit_once('.').expect("a table path");
            text(&swofty_formats::gvdb::instance_count_path(container, row))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(0)
        };
        let rows = (0..count(TRACKS))
            .filter_map(|i| text(&format!("{TRACKS}_{i}._metadata_")))
            .collect();
        let mut with_data = BTreeSet::new();
        for s in 0..count(TRACK_DATA) {
            let table = format!("{TRACK_DATA}_{s}.TrackData");
            with_data.extend((0..count(&table)).filter_map(|i| text(&format!("{table}_{i}.track"))));
        }
        Self { rows, with_data }
    }

    /// How many of the game's own tracks the lists hold.
    pub fn stock(&self) -> usize {
        self.rows.iter().filter(|c| self.lists(c)).count()
    }

    pub(super) fn lists(&self, code: &str) -> bool {
        code != UNLISTED_TRACK && self.rows.contains(code) && self.with_data.contains(code)
    }

    /// Whether activating `pkg` adds a track to the lists: activation copies its base track's TrackData rows to
    /// it, so a track whose base has none stays out of them, and a code the lists hold already adds nothing.
    fn adds(&self, pkg: &TrackPackage) -> bool {
        let code = pkg.track_code.as_str();
        code != UNLISTED_TRACK
            && !self.lists(code)
            && pkg
                .activation
                .base_track()
                .is_some_and(|base| self.with_data.contains(base))
    }
}

/// Whether `added` tracks can fill the game's track lists, so a build has to read the database to know.
pub fn may_fill_track_lists(added: usize) -> bool {
    added >= FILL_FROM
}

/// The build's tracks the game's track lists have room for, and the ones left out: the tracks of `recorded`
/// addons first, since the core adds their rows in game whatever the build does, then the others in load
/// order until the lists are full. A track that adds nothing to the lists is always kept.
pub fn fit_track_lists(
    listed: &ListedTracks,
    tracks: Vec<BuildTrack>,
    recorded: &BTreeSet<String>,
) -> (Vec<BuildTrack>, Vec<(BuildTrack, Unusable)>) {
    let room = TRACK_LIST_ROOM.saturating_sub(listed.stock());
    let mut used = tracks
        .iter()
        .filter(|(_, p, o)| recorded.contains(&o.addon) && listed.adds(p))
        .count();
    let mut kept = Vec::with_capacity(tracks.len());
    let mut left_out = Vec::new();
    for t in tracks {
        let (_, pkg, origin) = &t;
        if recorded.contains(&origin.addon) || !listed.adds(pkg) {
            kept.push(t);
            continue;
        }
        if used < room {
            used += 1;
            kept.push(t);
            continue;
        }
        let why = Unusable {
            code: Some(pkg.track_code.clone()),
            name: Some(pkg.display_name().to_owned()),
            why: track_lists_full(listed),
            fix: Some("Turn some track addons off to make room for it".to_owned()),
        };
        left_out.push((t, why));
    }
    (kept, left_out)
}

fn track_lists_full(listed: &ListedTracks) -> String {
    let stock = listed.stock();
    format!(
        "the game has room for {TRACK_LIST_ROOM} tracks, {stock} of its own and {} added ones, and the track \
         addons before it in the load order fill them",
        TRACK_LIST_ROOM.saturating_sub(stock)
    )
}

/// The build's notice for the tracks [`fit_track_lists`] leaves out, given by name.
pub fn track_lists_full_warning(listed: &ListedTracks, left_out: &[String]) -> Option<String> {
    let shown = match left_out.len() {
        0 => return None,
        n if n <= 8 => left_out.join(", "),
        n => format!("{} and {} more", left_out[..8].join(", "), n - 8),
    };
    let stock = listed.stock();
    Some(format!(
        "Too many tracks: {} track(s) are left out of this build because the game has room for {TRACK_LIST_ROOM} \
         tracks, {stock} of its own and {} added ones, and more stop it at a black screen after the EA logo: \
         {shown}. Turn some track addons off to play them",
        left_out.len(),
        TRACK_LIST_ROOM.saturating_sub(stock)
    ))
}

/// The track addons a build leaves out because the game's track lists are full, for the launcher to show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackLimit {
    pub room: usize,
    pub stock: usize,
    /// The display name of each track left out, in load order.
    pub names: Vec<String>,
    /// The addons that lose a track, each once.
    pub addons: Vec<String>,
}

impl TrackLimit {
    pub fn new(listed: &ListedTracks, names: Vec<String>, addons: Vec<String>) -> Option<Self> {
        (!names.is_empty()).then(|| Self {
            room: TRACK_LIST_ROOM,
            stock: listed.stock(),
            names,
            addons: addons.into_iter().collect::<BTreeSet<_>>().into_iter().collect(),
        })
    }
}

const LIMIT_FILE: &str = "state/track-limit.json";

#[derive(Debug, Default, Serialize, Deserialize)]
struct LimitRecord {
    profile: String,
    addons: Vec<String>,
}

/// Remembers which addons the last check of `profile` found left out for the full track lists.
pub fn record_track_limit(data: &Path, profile: &str, limit: Option<&TrackLimit>) -> Result<()> {
    let path = data.join(LIMIT_FILE);
    let addons = limit.map(|l| l.addons.clone()).unwrap_or_default();
    if addons.is_empty() && !path.is_file() {
        return Ok(());
    }
    crate::write_json(
        &path,
        &LimitRecord {
            profile: profile.to_owned(),
            addons,
        },
    )
}

/// The addons the last check of `profile` found left out for the full track lists.
pub fn recorded_track_limit(data: &Path, profile: &str) -> BTreeSet<String> {
    crate::read_json::<LimitRecord>(&data.join(LIMIT_FILE))
        .ok()
        .filter(|r| r.profile == profile)
        .map(|r| r.addons.into_iter().collect())
        .unwrap_or_default()
}
