use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::record_tracks;
use crate::stdlib::deps::Std;

pub const MAX_VIDEO_BYTES: u64 = 256 * 1024 * 1024;

const MAGIC: &[u8; 3] = b"KB2";
const GAME_DIR: &str = "videos/";
const VALUE_DIR: &str = "track_map/swofty";
const SET_26_DIR: &str = "track_map/26/swofty";
const WALL_DIR: &str = "track_select";
const WALL_26_DIR: &str = "track_select/26";

/// The per-box keys of a video field: the F1 boxes before 2026 and the F1 2026 box.
pub const KEYS: [&str; 2] = ["f1", "f1_2026"];

/// What a track record's `video` or `time_trial_video` names: one file for every box, or one per box.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoSet {
    /// The string form: the file every box without a key of its own plays.
    pub every: Option<String>,
    pub f1: Option<String>,
    pub f1_2026: Option<String>,
}

/// A place the game looks for a track's video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Slot {
    /// The circuit intro of the boxes that take their intros from `track_map/`.
    Intro,
    /// The circuit intro of the F1 2026 box, from `track_map/26/`.
    Intro26,
    /// The Time Trial track select's background of those boxes, `track_select/`.
    Wall,
    /// The Time Trial track select's background of the F1 2026 box, `track_select/26/`.
    Wall26,
}

impl Slot {
    pub fn label(self) -> &'static str {
        match self {
            Slot::Intro => "circuit intro",
            Slot::Intro26 => "circuit intro of the F1 2026 box",
            Slot::Wall => "Time Trial background",
            Slot::Wall26 => "Time Trial background of the F1 2026 box",
        }
    }

    fn is_26(self) -> bool {
        matches!(self, Slot::Intro26 | Slot::Wall26)
    }

    fn is_intro(self) -> bool {
        matches!(self, Slot::Intro | Slot::Intro26)
    }

    fn field(self) -> &'static str {
        match self.is_intro() {
            true => "video",
            false => "time_trial_video",
        }
    }
}

impl VideoSet {
    /// Reads the JSON a record holds in `field`: a string, or an object with the keys of [`KEYS`].
    pub fn parse(field: &str, value: &Value) -> Result<Self, String> {
        let text = |key: &str, v: &Value| match v.as_str() {
            Some(s) => Ok(s.to_owned()),
            None => Err(format!("{key} is not a string")),
        };
        match value {
            Value::String(s) => Ok(Self {
                every: Some(s.clone()),
                ..Self::default()
            }),
            Value::Object(map) => {
                let mut set = Self::default();
                for (key, v) in map {
                    let at = format!("{field}.{key}");
                    match key.as_str() {
                        "f1" => set.f1 = Some(text(&at, v)?),
                        "f1_2026" => set.f1_2026 = Some(text(&at, v)?),
                        "f2" if field == "video" => {
                            return Err(format!(
                                "{at} is not supported: the game names an F2 intro by the track, not by a row, so an added track's F2 intro can't be pointed at a file"
                            ));
                        }
                        _ => {
                            return Err(format!(
                                "{at} is not a box key; the keys are {}",
                                KEYS.join(" and ")
                            ));
                        }
                    }
                }
                match set.f1.is_none() && set.f1_2026.is_none() {
                    true => Err(format!("{field} is an object with no {} key", KEYS.join(" or "))),
                    false => Ok(set),
                }
            }
            _ => Err(format!("{field} is not a string or an object")),
        }
    }

    /// The files `slot` may play, best first, each with the key that names it. The boxes before 2026 take
    /// their intro from the row alone, so with only an `f1_2026` file an intro goes to them as well: the
    /// track has one row, and a row pointing at a file that is not there would leave them without one.
    pub(super) fn candidates(&self, slot: Slot) -> Vec<(&'static str, &str)> {
        let f1 = self.f1.as_deref().map(|v| ("f1", v));
        let f1_2026 = self.f1_2026.as_deref().map(|v| ("f1_2026", v));
        let every = self.every.as_deref().map(|v| ("", v));
        let order = match (slot.is_26(), slot.is_intro()) {
            (true, _) => [f1_2026, f1, every],
            (false, true) => [f1, every, f1_2026],
            (false, false) => [f1, every, None],
        };
        order.into_iter().flatten().collect()
    }
}

/// One file the build places for a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placed {
    pub slot: Slot,
    /// The record field and key that name the file, such as `video` or `time_trial_video.f1_2026`.
    pub named_by: String,
    pub source: PathBuf,
    /// The game-folder path the build places it at.
    pub game_path: String,
}

/// The videos a track record ships: the files and where the build puts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackVideo {
    pub id: String,
    pub code: String,
    /// What the track's `TrackVideo` row holds, the path of the plain circuit intro below `videos/`. None
    /// when the record ships no circuit intro, so the row keeps its base's.
    pub value: Option<String>,
    pub placed: Vec<Placed>,
}

fn safe(s: &str) -> String {
    s.chars()
        .map(
            |c| match c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                true => c.to_ascii_lowercase(),
                false => '_',
            },
        )
        .collect()
}

/// The `TrackVideo` value of the video of track `code` of addon `addon`: collision-free by addon and code.
pub fn video_value(addon: &str, code: &str) -> String {
    format!("{VALUE_DIR}/{}/{}.bk2", safe(addon), safe(code))
}

/// The game-folder path of the copy in the `track_map/26/` set of the video whose `TrackVideo` value is `value`.
pub fn video_path_26(value: &str) -> String {
    let rest = value.strip_prefix(VALUE_DIR).unwrap_or(value);
    format!("{GAME_DIR}{SET_26_DIR}{rest}")
}

/// The game-folder path of the Time Trial background of track `code`: the game fills
/// `videos/track_select/[BRANDING][TRACK].bk2` with the box's video folder and the track's file name, which
/// for an added track is its code.
pub fn wall_path(code: &str, f1_2026: bool) -> String {
    let dir = match f1_2026 {
        true => WALL_26_DIR,
        false => WALL_DIR,
    };
    format!("{GAME_DIR}{dir}/{}.bk2", safe(code))
}

fn game_path(slot: Slot, value: &str, code: &str) -> String {
    match slot {
        Slot::Intro => format!("{GAME_DIR}{value}"),
        Slot::Intro26 => video_path_26(value),
        Slot::Wall => wall_path(code, false),
        Slot::Wall26 => wall_path(code, true),
    }
}

fn check(dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let Some(inside) = crate::stdlib::paths::inside(rel) else {
        return Err(format!("{rel} leaves the addon's folder"));
    };
    if inside.ends_with('/') || !inside.to_ascii_lowercase().ends_with(".bk2") {
        return Err(format!("{rel} is not a .bk2 file"));
    }
    let path = dir.join(&inside);
    let meta = match std::fs::metadata(&path) {
        Ok(m) if m.is_file() => m,
        _ => return Err(format!("{rel} is not in the addon")),
    };
    if meta.len() > MAX_VIDEO_BYTES {
        return Err(format!(
            "{rel} is {} MB, over the {} MB a circuit intro may be",
            meta.len() / (1024 * 1024),
            MAX_VIDEO_BYTES / (1024 * 1024)
        ));
    }
    let mut head = [0u8; 3];
    let read = std::fs::File::open(&path).and_then(|mut f| f.read_exact(&mut head));
    if read.is_err() || &head != MAGIC {
        return Err(format!("{rel} is not a Bink 2 file (it does not start with KB2)"));
    }
    Ok(path)
}

/// The videos the track records of `order` ship for the tracks in `kept`, in load order, and a warning for
/// each one that can't be used. A track without a usable circuit intro keeps its base's.
pub fn track_videos(
    addons_dir: &Path,
    order: &[String],
    std16: &BTreeMap<String, Std>,
    kept: &BTreeSet<String>,
) -> (Vec<TrackVideo>, Vec<String>) {
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    for id in order {
        let Some(s) = std16.get(id).filter(|s| !s.data_files.is_empty()) else {
            continue;
        };
        let dir = addons_dir.join(id);
        for t in record_tracks(&dir, id, &s.data_files) {
            if !kept.contains(&t.code) {
                continue;
            }
            let value = video_value(id, &t.code);
            let mut checked: BTreeMap<String, Result<PathBuf, String>> = BTreeMap::new();
            let mut placed = Vec::new();
            for (field, spec) in [("video", &t.video), ("time_trial_video", &t.time_trial_video)] {
                let set = match spec {
                    None => continue,
                    Some(Ok(set)) => set,
                    Some(Err(why)) => {
                        warnings.push(format!("track {}: {why}; {}", t.id, ignored(field)));
                        continue;
                    }
                };
                let slots: &[Slot] = match field {
                    "video" => &[Slot::Intro, Slot::Intro26],
                    _ => &[Slot::Wall, Slot::Wall26],
                };
                for &slot in slots {
                    let mut chosen = None;
                    for (key, rel) in set.candidates(slot) {
                        let named_by = match key {
                            "" => slot.field().to_owned(),
                            k => format!("{}.{k}", slot.field()),
                        };
                        let result = checked.entry(rel.to_owned()).or_insert_with(|| {
                            let r = check(&dir, rel);
                            if let Err(why) = &r {
                                warnings.push(format!(
                                    "track {}: {} {why}; {}",
                                    t.id,
                                    named_by,
                                    ignored(field)
                                ));
                            }
                            r
                        });
                        if let Ok(source) = result {
                            chosen = Some((named_by, source.clone()));
                            break;
                        }
                    }
                    if let Some((named_by, source)) = chosen {
                        placed.push(Placed {
                            slot,
                            named_by,
                            source,
                            game_path: game_path(slot, &value, &t.code),
                        });
                    }
                }
            }
            if placed.is_empty() {
                continue;
            }
            let intro = placed.iter().any(|p| p.slot == Slot::Intro);
            out.push(TrackVideo {
                id: t.id,
                code: t.code,
                value: intro.then_some(value),
                placed,
            });
        }
    }
    (out, warnings)
}

fn ignored(field: &str) -> &'static str {
    match field {
        "video" => "it plays its base track's intro",
        _ => "it gets no Time Trial background of its own",
    }
}
