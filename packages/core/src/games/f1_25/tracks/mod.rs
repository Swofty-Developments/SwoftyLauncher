use serde::{Deserialize, Serialize};

mod addons;
mod changes;
mod lists;
mod older;
mod profile;
mod records;
mod refs;
mod schedule;

#[cfg(test)]
mod tests;

use self::addons::added_codes;
pub(crate) use self::addons::adders;
pub use self::addons::{
    added_count, installed, is_track_addon, left_out_line, left_out_lines, older_ids, package_author,
    unusable,
};
pub(crate) use self::changes::{changed_records, keep_changes};
pub use self::lists::{
    FILL_FROM, ListedTracks, TRACK_LIST_ROOM, TrackLimit, fit_track_lists, may_fill_track_lists,
    record_track_limit, recorded_track_limit, track_lists_full_warning,
};
pub use self::older::{
    OLDER_FILE, OLDER_VERSION, OlderTrack, OlderTracks, Venue, Venues, normalise, older_tracks, read_older,
    write_older,
};
use self::profile::Rows;
pub use self::profile::{for_profile, list};
pub use self::records::{
    BuildTrack, RecordTrack, TRACK_REGISTRY, keep_track_folders, record_package, record_packages,
    record_tracks, with_records,
};
use self::refs::TRACK_DATA;
pub(crate) use self::refs::keep_track_refs;
pub(crate) use self::schedule::{Schedule, drop_left_out_orders, option_tracks, restore_weekends};

const TRACKS: &str = "tracks.Track";
const EVENTS: &str = "schedule.all_track_events.fixed_event";
const ACTIVITIES: &str = "schedule.activities.activity_race_weekend";
const CONTENT_SETS: &str = "content_sets.content_set";
const SEASON_SETS: [&str; 2] = ["2025", "2026"];
const RESOURCE_PREFIX: &str = "swofty_";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlanTrack {
    pub code: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub addon: Option<String>,
}
