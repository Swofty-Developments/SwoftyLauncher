use crate::ops::IdFrom;

use super::{FILE, Store, addon_of, row_name};

/// The table name of the track rows `track.add` adds; the game reads their `statId` in a byte, 0xff = none.
const TRACK_TABLE: &str = "tracks.Track";
pub const TRACK_STAT_MAX: i64 = 254;

/// The `<table>.<row>/<label>` name of the track row `track.add` adds for the track `code`, as a build's added-row
/// ids and [`Overrides::rows`] name it.
pub fn track_row(code: &str) -> String {
    format!("{TRACK_TABLE}/{code}")
}

impl Store {
    /// The stat id of the track row `addon` adds for the track `code` through `track.add`, kept in the store under
    /// `<addon>/tracks.Track/<code>`: the id kept for it, else `seed` (the id the last build gave it), else the first
    /// free id from `top`, the table's highest plus one, to [`TRACK_STAT_MAX`] and then from 0. `held` says whether a
    /// track row of this build holds an id. An id kept for any other track stays that track's whether or not its addon
    /// is on, so no id is ever given to two tracks; only when every id is kept does a track whose addon the build
    /// leaves out give its id up.
    pub fn track_stat_id(
        &mut self,
        addon: &str,
        code: &str,
        seed: Option<i64>,
        top: i64,
        held: &dyn Fn(i64) -> bool,
    ) -> std::result::Result<(i64, String), String> {
        let base = format!("{addon}/{}", track_row(code));
        let mut nth = 0;
        let key = loop {
            let key = match nth {
                0 => base.clone(),
                n => format!("{base}#{}", n + 1),
            };
            if self.claimed.insert(key.clone()) {
                break key;
            }
            nth += 1;
        };
        let blocked = |s: &Self, id: i64| {
            !(0..=TRACK_STAT_MAX).contains(&id)
                || held(id)
                || s.by_id
                    .get(TRACK_TABLE)
                    .and_then(|ids| ids.get(&id))
                    .is_some_and(|k| *k != key)
        };
        let kept = self.get(&key);
        if let Some(id) = kept.filter(|id| !blocked(self, *id)) {
            return Ok((id, format!("stat id {id} (kept from {FILE})")));
        }
        if kept.is_none()
            && let Some(id) = seed.filter(|id| !blocked(self, *id))
        {
            self.put(&key, id, IdFrom::Seeded);
            return Ok((id, format!("stat id {id} (as the last build gave it)")));
        }
        let from = match kept {
            Some(_) => IdFrom::Moved,
            None => IdFrom::Next,
        };
        let free = (top.max(0)..=TRACK_STAT_MAX)
            .chain(0..top.clamp(0, TRACK_STAT_MAX + 1))
            .find(|id| !blocked(self, *id));
        // Every id is kept for some track ever installed: the id of a track whose addon this build leaves out goes to
        // this one. A track of an addon in the build never lends its id, whether or not the build reached it yet.
        // Saves name a track by its id and no record says which ids careers use, so a calendar that named the left-out
        // track shows this one from then on. The left-out track takes a free or spare id again when it is turned on;
        // the ids UDP telemetry reports never change with it (udp_ids.rs, swofty_track_id).
        let spare = || {
            self.by_id
                .get(TRACK_TABLE)
                .into_iter()
                .flatten()
                .filter(|(id, k)| {
                    **k != key
                        && (0..=TRACK_STAT_MAX).contains(*id)
                        && !held(**id)
                        && !self.claimed.contains(*k)
                        && !self.building.contains(addon_of(k))
                })
                .map(|(id, k)| (*id, k.clone()))
                .min()
        };
        let (id, from, lent) = match free {
            Some(id) => (id, from, None),
            None => match spare() {
                Some((id, k)) => (id, IdFrom::Reused, Some(k)),
                None => {
                    return Err(format!(
                        "{code}: tracks have no free statId up to {TRACK_STAT_MAX}, the highest the game reads, and every track that holds \
                         one is in this build"
                    ));
                }
            },
        };
        self.put(&key, id, from);
        if let Some(k) = lent {
            self.reused += 1;
            return Ok((
                id,
                format!(
                    "stat id {id} (every id is kept; this one was {}, whose addon is off)",
                    row_name(&k)
                ),
            ));
        }
        let why = match kept {
            Some(old) => format!("stat id {id} (the {old} kept for it is held by another track)"),
            None => format!("stat id {id} (new, kept in {FILE})"),
        };
        Ok((id, why))
    }
}
