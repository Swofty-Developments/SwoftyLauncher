use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use super::record_tracks;
use crate::stdlib::deps::Std;

pub const MAX_VIDEO_BYTES: u64 = 256 * 1024 * 1024;

const MAGIC: &[u8; 3] = b"KB2";
const GAME_DIR: &str = "videos/";
const VALUE_DIR: &str = "track_map/swofty";

/// A circuit intro a track record ships: the file in the addon, and where the build puts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackVideo {
    pub id: String,
    pub code: String,
    pub source: PathBuf,
    /// The game-folder path the build places it at.
    pub game_path: String,
    /// What the track's `TrackVideo` row holds: the path below `videos/`.
    pub value: String,
}

/// The `TrackVideo` value of the video of track `code` of addon `addon`: collision-free by addon and code.
pub fn video_value(addon: &str, code: &str) -> String {
    let safe = |s: &str| {
        s.chars()
            .map(
                |c| match c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                    true => c.to_ascii_lowercase(),
                    false => '_',
                },
            )
            .collect::<String>()
    };
    format!("{VALUE_DIR}/{}/{}.bk2", safe(addon), safe(code))
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

/// The circuit intros the track records of `order` ship for the tracks in `kept`, in load order, and a
/// warning for each one that can't be used. A track without a usable video keeps its base's.
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
            let Some(rel) = t.video.as_deref().filter(|_| kept.contains(&t.code)) else {
                continue;
            };
            match check(&dir, rel) {
                Ok(source) => {
                    let value = video_value(id, &t.code);
                    out.push(TrackVideo {
                        id: t.id,
                        code: t.code,
                        source,
                        game_path: format!("{GAME_DIR}{value}"),
                        value,
                    });
                }
                Err(why) => warnings.push(format!(
                    "track {}: video {why}; it plays its base track's intro",
                    t.id
                )),
            }
        }
    }
    (out, warnings)
}
