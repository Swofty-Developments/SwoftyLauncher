use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use swofty_addon::manifest::AddonManifest;
use swofty_formats::gvdb::{Gvdb, Record};
use swofty_formats::hash::fnv1a64;

use crate::store::Store;

use super::{ACTIVITIES, EVENTS, TRACKS, added_codes};

/// The track a weekend with no stock track to go back to races at when its own track is left out, as the
/// game's Grand Prix does when its remembered track has no record: melbourne, else the first track the
/// build holds.
const FALLBACK_TRACK: &str = "melbourne";

/// Puts each race weekend at a track a build left out back at its stock
/// track, and says so; a weekend the game doesn't ship, an addon's own row,
/// races at [`FALLBACK_TRACK`] instead. Names the weekends the build holds no
/// track to put anywhere.
pub(crate) fn restore_weekends(
    db: &mut Gvdb,
    shipped: &Gvdb,
    left_out: &[&str],
) -> std::result::Result<Vec<String>, Vec<String>> {
    let text = |db: &Gvdb, path: &str| db.get_text(path).map(|t| t.trim().to_owned());
    let count = |db: &Gvdb, table: &str| {
        let (container, row) = table.rsplit_once('.').expect("a table path");
        db.instance_count(container, row).unwrap_or(0)
    };
    let mut held: Option<Vec<String>> = None;
    let mut out = Vec::new();
    let mut stuck = Vec::new();
    for i in 0..count(db, ACTIVITIES) {
        let path = format!("{ACTIVITIES}_{i}.track");
        let Some(code) = text(db, &path).filter(|c| left_out.contains(&c.as_str())) else {
            continue;
        };
        let held = held.get_or_insert_with(|| {
            (0..count(db, TRACKS))
                .filter_map(|t| text(db, &format!("{TRACKS}_{t}._metadata_")))
                .collect()
        });
        if held.contains(&code) {
            continue;
        }
        let weekend = text(db, &format!("{ACTIVITIES}_{i}._metadata_")).unwrap_or_else(|| i.to_string());
        let stock = shipped
            .get(&path)
            .filter(|r| !left_out.contains(&r.text().trim()) && held.iter().any(|h| h == r.text().trim()))
            .cloned();
        match stock {
            Some(r) => {
                let to = r.text().trim().to_owned();
                if db.set(&path, r.value, Some(r.tag)).is_ok() {
                    out.push(format!(
                        "race weekend {weekend}: races at {to} again; track {code} is left out of this build"
                    ));
                }
            }
            None => {
                let usable = |h: &&String| !left_out.contains(&h.as_str());
                let to = held
                    .iter()
                    .find(|h| *h == FALLBACK_TRACK)
                    .or_else(|| {
                        held.iter()
                            .filter(usable)
                            .find(|h| !DORMANT.contains(&h.as_str()))
                    })
                    .or_else(|| held.iter().find(usable))
                    .cloned();
                match to {
                    Some(to) if db.set(&path, to.clone().into_bytes(), None).is_ok() => out.push(format!(
                        "race weekend {weekend}: races at {to} instead; track {code} is left out of this build \
                         and the game won't start with a race weekend at it"
                    )),
                    _ => stuck.push(format!(
                        "race weekend {weekend} races at {code}, which is left out of this build, and the build \
                         holds no other track to move it to; the game won't start like this"
                    )),
                }
            }
        }
    }
    match stuck.is_empty() {
        true => Ok(out),
        false => Err(stuck),
    }
}

pub(super) const SORTED_ORDER: &str = "track_data.sorted_track_order.sorted_track_order";

/// Takes each `left_out` track out of the game's sorted track orders, the lists its menus read tracks from,
/// and names the orders it was in.
pub(crate) fn drop_left_out_orders(db: &mut Gvdb, left_out: &[&str]) -> Vec<String> {
    let count = |db: &Gvdb, container: &str, row: &str| db.instance_count(container, row).unwrap_or(0);
    let mut out = Vec::new();
    for o in 0..count(db, "track_data.sorted_track_order", "sorted_track_order") {
        let order = format!("{SORTED_ORDER}_{o}");
        let rows: Vec<(Option<Record>, Option<Record>)> = (0..count(db, &order, "TrackRef"))
            .map(|r| {
                (
                    db.get(&format!("{order}.TrackRef_{r}._metadata_")).cloned(),
                    db.get(&format!("{order}.TrackRef_{r}.track")).cloned(),
                )
            })
            .collect();
        let held = |(_, t): &&(Option<Record>, Option<Record>)| {
            !t.as_ref().is_some_and(|t| left_out.contains(&t.text().trim()))
        };
        let kept: Vec<_> = rows.iter().filter(held).collect();
        if kept.len() == rows.len() {
            continue;
        }
        let gone: Vec<String> = rows
            .iter()
            .filter(|r| !held(r))
            .filter_map(|(_, t)| Some(t.as_ref()?.text().trim().to_owned()))
            .collect();
        for (r, (meta, track)) in kept.iter().enumerate() {
            for (field, rec) in [("_metadata_", meta), ("track", track)] {
                if let Some(rec) = rec {
                    let _ = db.set(
                        &format!("{order}.TrackRef_{r}.{field}"),
                        rec.value.clone(),
                        Some(rec.tag),
                    );
                }
            }
        }
        for r in kept.len()..rows.len() {
            db.remove(&format!("{order}.TrackRef_{r}._metadata_"));
            db.remove(&format!("{order}.TrackRef_{r}.track"));
        }
        let _ = db.set(
            &swofty_formats::gvdb::instance_count_path(&order, "TrackRef"),
            kept.len().to_string().into_bytes(),
            None,
        );
        let label = db
            .get_text(&format!("{order}._metadata_"))
            .map_or_else(|| o.to_string(), |l| l.trim().to_owned());
        out.push(format!(
            "sorted track order {label} listed {}, which is left out of this build; the build takes it out",
            gone.join(", ")
        ));
    }
    out
}

/// The stock rows of the tracks table with no track archive: a round there has no track until an addon fills
/// the row.
pub(crate) const DORMANT: [&str; 9] = [
    "paul_ricard",
    "portimao",
    "sochi",
    "hanoi",
    "texas_nat",
    "suzuka_east",
    "bahrain_pad",
    "silverstone_int",
    "bahrain24",
];

const ACTIVITY_TABLE: &str = "schedule.activities";
const EVENT_TABLE: &str = "schedule.all_track_events";

/// The shipped schedule an addon's added rounds are checked against: each race weekend's track and each
/// event's race weekend, by label, and the stock tracks with a track archive.
#[derive(Debug, Clone, Default)]
pub(crate) struct Schedule {
    weekends: HashMap<String, String>,
    events: HashMap<String, String>,
    pub(super) tracks: HashSet<String>,
}

impl Schedule {
    pub(crate) fn of(db: &Gvdb) -> Self {
        let count_key = |table: &str| {
            let (container, row) = table.rsplit_once('.').expect("a table path");
            fnv1a64(swofty_formats::gvdb::instance_count_path(container, row))
        };
        let tables = [
            (ACTIVITIES, "track"),
            (EVENTS, "activity"),
            (TRACKS, "_metadata_"),
        ];
        let counts = db.get_keys(&tables.iter().map(|(t, _)| count_key(t)).collect());
        let count = |table: &str| {
            counts
                .get(&count_key(table))
                .and_then(|r| r.text().trim().parse().ok())
                .unwrap_or(0)
        };
        let wanted = tables
            .iter()
            .flat_map(|&(table, field)| {
                (0..count(table)).flat_map(move |i| {
                    [
                        fnv1a64(format!("{table}_{i}._metadata_")),
                        fnv1a64(format!("{table}_{i}.{field}")),
                    ]
                })
            })
            .collect();
        let rows = db.get_keys(&wanted);
        let text = |path: &str| rows.get(&fnv1a64(path)).map(|r| r.text().trim().to_owned());
        let pairs = |table: &str, field: &str| {
            (0..count(table))
                .filter_map(|i| {
                    Some((
                        text(&format!("{table}_{i}._metadata_"))?,
                        text(&format!("{table}_{i}.{field}"))?,
                    ))
                })
                .collect()
        };
        Self {
            weekends: pairs(ACTIVITIES, "track"),
            events: pairs(EVENTS, "activity"),
            tracks: (0..count(TRACKS))
                .filter_map(|i| text(&format!("{TRACKS}_{i}._metadata_")))
                .filter(|c| !DORMANT.contains(&c.as_str()))
                .collect(),
        }
    }

    /// The tracks a build holds: the stock tracks with a track archive and the tracks the addons in `order`
    /// add, dormant rows they fill included.
    pub(crate) fn held(
        &self,
        store: &Store,
        order: &[String],
        addons: &BTreeMap<String, AddonManifest>,
    ) -> HashSet<String> {
        let mut out = self.tracks.clone();
        for id in order {
            if let Some(m) = addons.get(id) {
                out.extend(
                    added_codes(store, id, m)
                        .into_iter()
                        .filter(|(_, ok)| *ok)
                        .map(|(c, _)| c),
                );
            }
        }
        out
    }
}

/// A toggle of an addon whose rows add a round at a track the build doesn't hold.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct OptionTrack {
    pub option: String,
    pub track: String,
}

impl OptionTrack {
    /// The build's warning, with the addon's name, the option's label and the addon that adds the track when
    /// one is installed.
    pub(crate) fn line(&self, addon: &str, option: &str, adds: Option<&str>) -> String {
        let fix = match adds {
            Some(adds) => format!("turn {adds} on"),
            None => format!("install and turn on the addon that adds {}", self.track),
        };
        format!(
            "{addon}: option {option} adds rounds at {}, which isn't in this build, so the build leaves that \
             option's rows out and builds as if it were off; {fix}, or turn the option off",
            self.track
        )
    }
}

/// The toggles of `m` whose `when`-gated `add-row`s, in its database op files `docs`, add a race weekend or a
/// calendar event at a track `held` doesn't name. A weekend's track is the row's `track`, else its source
/// row's; an event's is its weekend's, from the row's `activity`, else its source row's.
pub(crate) fn option_tracks(
    m: &AddonManifest,
    docs: &[serde_json::Value],
    stock: &Schedule,
    held: &HashSet<String>,
) -> Vec<OptionTrack> {
    use serde_json::Value;
    use swofty_addon::manifest::OptionType;
    let toggles = |when: Option<&Value>| -> Vec<String> {
        when.and_then(Value::as_object)
            .into_iter()
            .flatten()
            .filter(|(id, want)| {
                want.as_bool() == Some(true)
                    && m.options
                        .iter()
                        .any(|o| &o.id == *id && o.kind == OptionType::Toggle)
            })
            .map(|(id, _)| id.clone())
            .collect()
    };
    let label = |from: &str| {
        let (_, rest) = from.split_once("[_metadata_=")?;
        rest.split_once(']').map(|(l, _)| l.trim().to_owned())
    };
    let mut rows = Vec::new();
    for doc in docs {
        let file = toggles(doc.get("when"));
        for op in doc.get("ops").and_then(Value::as_array).into_iter().flatten() {
            if op.get("op").and_then(Value::as_str) != Some("add-row") {
                continue;
            }
            let field = |f: &str| {
                op.get("set")
                    .and_then(|s| s.get(f))
                    .and_then(Value::as_str)
                    .map(|v| v.trim().to_owned())
            };
            let from = op.get("from").and_then(Value::as_str).unwrap_or_default();
            let mut gates = file.clone();
            gates.extend(toggles(op.get("when")));
            match op.get("table").and_then(Value::as_str) {
                Some(ACTIVITY_TABLE) if from.starts_with(ACTIVITIES) => {
                    let track = field("track").or_else(|| stock.weekends.get(&label(from)?).cloned());
                    rows.push((true, field("_metadata_"), track, gates));
                }
                Some(EVENT_TABLE) if from.starts_with(EVENTS) => {
                    let weekend = field("activity").or_else(|| stock.events.get(&label(from)?).cloned());
                    rows.push((false, weekend, None, gates));
                }
                _ => {}
            }
        }
    }
    let mut weekends = stock.weekends.clone();
    for (weekend, label, track) in rows.iter().map(|(w, l, t, _)| (w, l, t)) {
        if let (true, Some(label), Some(track)) = (weekend, label, track) {
            weekends.insert(label.clone(), track.clone());
        }
    }
    let mut out = BTreeSet::new();
    for (weekend, label, track, gates) in rows {
        let track = match weekend {
            true => track,
            false => label.and_then(|w| weekends.get(&w).cloned()),
        };
        let Some(track) = track.filter(|t| !t.is_empty() && !held.contains(t)) else {
            continue;
        };
        out.extend(gates.into_iter().map(|option| OptionTrack {
            option,
            track: track.clone(),
        }));
    }
    out.into_iter().collect()
}
