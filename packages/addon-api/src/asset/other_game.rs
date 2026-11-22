//! F1 25's packed game data, which no addon replaces, and the paths that show files are made for
//! another F1 game: its asset folder (`2020_asset_groups/`) or its exe (`F1_2020.exe`).

use std::collections::BTreeMap;

/// The files that hold F1 25's packed game data. Their index is in `F1_25.exe`, so F1 25 can't
/// start with another copy of either.
pub const PACKED: &[&str] = &["game.dat", "game.daf"];

/// F1 25's year in its asset folder, `2025_asset_groups`.
const OWN_YEAR: u16 = 2025;

/// The packed game data file the game folder path `target` names.
pub fn packed(target: &str) -> Option<&'static str> {
    let target = target.replace('\\', "/");
    let target = target.trim_start_matches("./").trim_start_matches('/');
    PACKED.iter().copied().find(|p| target.eq_ignore_ascii_case(p))
}

/// The packed game data files the game folder paths `targets` name, sorted, each once.
pub fn packed_in<'a>(targets: impl IntoIterator<Item = &'a str>) -> Vec<&'static str> {
    let mut files: Vec<&'static str> = targets.into_iter().filter_map(packed).collect();
    files.sort_unstable();
    files.dedup();
    files
}

/// Why a `file.put` to `target` is refused.
pub fn refusal(target: &str) -> Option<String> {
    packed(target).map(|name| format!("file.put {target}: F1 25 can't start with its {name} replaced"))
}

/// The F1 game files are made for, and the path that shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherGame {
    /// The game's name, such as `F1 2020` or `F1 23`.
    pub game: String,
    /// The folder or file that names the game, such as `2020_asset_groups` or `F1_2020.exe`.
    pub sign: String,
}

/// The F1 game the files at `paths` are made for, when more of them are another F1 game's than F1 25's.
pub fn made_for<'a>(paths: impl IntoIterator<Item = &'a str>) -> Option<OtherGame> {
    let mut counts: BTreeMap<u16, (usize, String)> = BTreeMap::new();
    for (year, sign) in paths.into_iter().filter_map(year_of) {
        let (n, kept) = counts.entry(year).or_insert((0, String::new()));
        *n += 1;
        if kept.is_empty() || (is_exe(&sign) && !is_exe(kept)) {
            *kept = sign;
        }
    }
    let own = counts.remove(&OWN_YEAR).map_or(0, |(n, _)| n);
    let (year, (n, sign)) = counts.into_iter().max_by_key(|(year, (n, _))| (*n, *year))?;
    (n > own).then(|| OtherGame {
        game: game_name(year),
        sign,
    })
}

/// The F1 game a path belongs to, by year, and the part of it that says so: an `<year>_asset_groups`
/// folder, or an `F1_<year>.exe` or `F1_<year>_dx12.exe` file.
fn year_of(path: &str) -> Option<(u16, String)> {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let file = parts.last()?;
    let folder = parts.iter().find_map(|d| {
        let lower = d.to_ascii_lowercase();
        let year = lower.strip_suffix("_asset_groups")?;
        Some((four_digits(year)?, (*d).to_owned()))
    });
    folder.or_else(|| {
        let lower = file.to_ascii_lowercase();
        let stem = lower.strip_suffix(".exe")?;
        let stem = stem.strip_suffix("_dx12").unwrap_or(stem);
        let year = stem.strip_prefix("f1_")?;
        let year = match year.len() {
            2 => 2000 + year.parse::<u16>().ok()?,
            _ => four_digits(year)?,
        };
        Some((year, (*file).to_owned()))
    })
}

fn is_exe(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".exe")
}

fn four_digits(text: &str) -> Option<u16> {
    (text.len() == 4 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// An F1 game's name from its year: `F1 2020` and `F1 2021`, then `F1 22` on.
fn game_name(year: u16) -> String {
    match year {
        ..=2021 => format!("F1 {year}"),
        _ => format!("F1 {}", year % 100),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_packed_game_data_is_packed() {
        assert_eq!(packed("game.dat"), Some("game.dat"));
        assert_eq!(packed("Game.DAT"), Some("game.dat"));
        assert_eq!(packed("./game.daf"), Some("game.daf"));
        assert_eq!(packed("videos/game.dat"), None);
        assert_eq!(packed("game_package/gamedatabases/gamedatabases.2025.bin"), None);
        assert_eq!(
            refusal("game.dat").as_deref(),
            Some("file.put game.dat: F1 25 can't start with its game.dat replaced")
        );
        assert_eq!(refusal("videos/attract.bk2"), None);
    }

    #[test]
    fn files_in_another_games_asset_folder_are_made_for_it() {
        let paths = [
            "2020_asset_groups/ui_package/textures/track_maps/sepang.erp",
            "2020_asset_groups/ui_package/flow_frontend.erp",
            "f1_2020_vehicle_package/teams/ferrari/ferrari.erp",
            "videos/attract.bk2",
            "game.dat",
        ];
        assert_eq!(
            made_for(paths),
            Some(OtherGame {
                game: "F1 2020".into(),
                sign: "2020_asset_groups".into()
            })
        );
        let exe = made_for([
            "PastootF1/F1_2020.exe",
            "PastootF1/F1_2020_dx12.exe",
            "PastootF1/game.dat",
        ]);
        assert_eq!(
            exe.map(|o| (o.game, o.sign)),
            Some(("F1 2020".into(), "F1_2020.exe".into()))
        );
        assert_eq!(made_for(["F1_23.exe"]).map(|o| o.game), Some("F1 23".into()));
    }

    #[test]
    fn f1_25_files_are_not_made_for_another_game() {
        assert_eq!(made_for(["game.dat", "videos/attract.bk2"]), None);
        assert_eq!(made_for(["F1_25.exe", "2025_asset_groups/a.erp"]), None);
        assert_eq!(
            made_for([
                "2025_asset_groups/a.erp",
                "2025_asset_groups/b.erp",
                "2024_asset_groups/stray.erp",
            ]),
            None
        );
        assert_eq!(
            made_for(["audio/2020/soundbanks/music.bnk", "x_asset_groups/a.erp"]),
            None
        );
    }
}
