use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use swofty_addon::asset::Origin;
use swofty_addon::manifest::AddonManifest;
use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::fnv1a64;

use super::refs::{GAME_MODES, MissingFix, MissingTrack};
use super::schedule::{OptionTrack, SORTED_ORDER};
use super::*;
use crate::content::track_package::TrackPackage;
use crate::store::Store;

mod addons;
mod changes;
mod lists;
mod older;
mod profile;
mod records;
mod refs;
mod schedule;

const STOCK_WITH_DATA: [&str; 36] = [
    "melbourne",
    "suzuka",
    "shanghai",
    "bahrain",
    "jeddah",
    "miami",
    "imola",
    "monaco",
    "catalunya",
    "montreal",
    "austria",
    "silverstone",
    "spa_francorchamps",
    "hungaroring",
    "zandvoort",
    "monza",
    "baku",
    "singapore",
    "texas",
    "mexico",
    "brazil",
    "las_vegas",
    "losail",
    "abu_dhabi",
    "portimao",
    "texas_nat",
    "suzuka_east",
    "bahrain_pad",
    "silverstone_int",
    "sochi",
    "styria",
    "hanoi",
    "silverstone_reverse",
    "austria_reverse",
    "zandvoort_reverse",
    "madrid",
];
const STOCK_WITHOUT_DATA: [&str; 7] = [
    "paul_ricard",
    "imola_prototype",
    "melbourne24",
    "bahrain24",
    "suzuka24",
    "imola24",
    "miami24",
];

fn stock_lists() -> Gvdb {
    let codes: Vec<&str> = STOCK_WITH_DATA
        .iter()
        .chain(&STOCK_WITHOUT_DATA)
        .copied()
        .collect();
    let mut records = vec![
        rec("tracks._Track_instancecount_", &codes.len().to_string()),
        rec("track_data.game_setup._TrackDataSet_instancecount_", "2"),
        rec(&format!("{TRACK_DATA}_0._TrackData_instancecount_"), "30"),
        rec(&format!("{TRACK_DATA}_1._TrackData_instancecount_"), "6"),
    ];
    for (i, code) in codes.iter().enumerate() {
        records.push(rec(&format!("{TRACKS}_{i}._metadata_"), code));
    }
    for (i, code) in STOCK_WITH_DATA.iter().enumerate() {
        let (set, row) = match i < 30 {
            true => (0, i),
            false => (1, i - 30),
        };
        records.push(rec(&format!("{TRACK_DATA}_{set}.TrackData_{row}.track"), code));
    }
    Gvdb::new(records)
}

fn build_track(addon: &str, code: &str, base: &str) -> BuildTrack {
    let pkg = TrackPackage {
        track_code: code.into(),
        track_name: Some(format!("{code} circuit")),
        version: None,
        author_name: None,
        erp: format!("{code}.erp"),
        activation: crate::content::track_package::Activation {
            copy_from_track: Some(base.into()),
            ..Default::default()
        },
        ers: Default::default(),
        copy_directories: Vec::new(),
        ui: None,
        extra: Default::default(),
    };
    let origin = Origin {
        addon: addon.into(),
        item: None,
    };
    (PathBuf::from(format!("/addons/{addon}/track")), pkg, origin)
}

fn codes(tracks: &[BuildTrack]) -> Vec<&str> {
    tracks.iter().map(|(_, p, _)| p.track_code.as_str()).collect()
}

fn rec(path: &str, value: &str) -> Record {
    Record::new(path, 2, value.as_bytes().to_vec())
}

fn db() -> Gvdb {
    let mut records = vec![
        rec("tracks._Track_instancecount_", "3"),
        rec("schedule.all_track_events._fixed_event_instancecount_", "3"),
        rec("schedule.activities._activity_race_weekend_instancecount_", "1"),
        rec("schedule.activities.activity_race_weekend_0._metadata_", "suzuka"),
        rec("schedule.activities.activity_race_weekend_0.track", "suzuka"),
        rec("content_sets._content_set_instancecount_", "2"),
        rec("content_sets.content_set_0._metadata_", "2024"),
        rec("content_sets.content_set_0._available_tracks_instancecount_", "1"),
        rec(
            "content_sets.content_set_0.available_tracks_0.tracks",
            "bahrain24",
        ),
        rec("content_sets.content_set_1._metadata_", "2026"),
        rec("content_sets.content_set_1._available_tracks_instancecount_", "1"),
        rec(
            "content_sets.content_set_1.available_tracks_0.tracks",
            "suzuka,okayama",
        ),
    ];
    for (i, (code, id, res)) in [
        ("suzuka", "13", "2019"),
        ("bahrain24", "35", "swofty_bahrain24"),
        ("okayama", "43", "swofty_okayama"),
    ]
    .into_iter()
    .enumerate()
    {
        records.push(rec(&format!("tracks.Track_{i}._metadata_"), code));
        records.push(rec(&format!("tracks.Track_{i}.statId"), id));
        records.push(rec(&format!("tracks.Track_{i}.track_resource"), res));
    }
    for (i, (name, activity)) in [
        ("rw_suzuka", "suzuka"),
        ("rw_suzuka_26", "suzuka"),
        ("rw_gone", "gone"),
    ]
    .into_iter()
    .enumerate()
    {
        records.push(rec(
            &format!("schedule.all_track_events.fixed_event_{i}._metadata_"),
            name,
        ));
        records.push(rec(
            &format!("schedule.all_track_events.fixed_event_{i}.activity"),
            activity,
        ));
    }
    Gvdb::new(records)
}

fn weekends(db: &mut Gvdb, rows: &[(&str, &str)]) {
    let n = rows.len();
    db.set(
        "schedule.activities._activity_race_weekend_instancecount_",
        n.to_string(),
        Some(2),
    )
    .unwrap();
    for (i, (label, track)) in rows.iter().enumerate() {
        db.set(&format!("{ACTIVITIES}_{i}._metadata_"), *label, Some(2))
            .unwrap();
        db.set(&format!("{ACTIVITIES}_{i}.track"), *track, Some(2))
            .unwrap();
    }
}

fn schedule() -> Schedule {
    let mut records = vec![
        rec("tracks._Track_instancecount_", "4"),
        rec("schedule.activities._activity_race_weekend_instancecount_", "3"),
        rec("schedule.all_track_events._fixed_event_instancecount_", "3"),
    ];
    for (i, code) in ["melbourne", "spa_francorchamps", "portimao", "sochi"]
        .iter()
        .enumerate()
    {
        records.push(rec(&format!("{TRACKS}_{i}._metadata_"), code));
    }
    for (i, code) in ["spa_francorchamps", "portimao", "sochi"].iter().enumerate() {
        records.push(rec(&format!("{ACTIVITIES}_{i}._metadata_"), code));
        records.push(rec(&format!("{ACTIVITIES}_{i}.track"), code));
        records.push(rec(&format!("{EVENTS}_{i}._metadata_"), &format!("rw_{code}")));
        records.push(rec(&format!("{EVENTS}_{i}.activity"), code));
    }
    Schedule::of(&Gvdb::new(records))
}
