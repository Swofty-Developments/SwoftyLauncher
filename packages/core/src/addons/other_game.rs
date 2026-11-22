//! Addons and mods that put their own copy of F1 25's packed game data (`game.dat`), which stops F1 25
//! as it starts. An install refuses one whose files are made for another F1 game, the importer refuses
//! such a mod before it converts it, the build leaves the packed game data out of every addon, and the
//! crash screen names an addon that carries it.

use std::path::Path;

use crate::asset::other_game::{OtherGame, made_for, packed, packed_in};
use crate::manifest::AddonManifest;

/// What an addon's `file.put` ops put over F1 25's packed game data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packed {
    /// The packed game data files the addon puts, such as `game.dat`.
    pub files: Vec<&'static str>,
    /// The F1 game the addon's files are made for, when it isn't F1 25.
    pub other: Option<OtherGame>,
}

impl Packed {
    /// The files, as `game.dat` or `game.dat and game.daf`.
    pub fn names(&self) -> String {
        self.files.join(" and ")
    }

    fn of<'a>(targets: impl IntoIterator<Item = &'a str> + Clone) -> Option<Self> {
        let files = packed_in(targets.clone());
        (!files.is_empty()).then(|| Self {
            files,
            other: made_for(targets),
        })
    }
}

/// What the `file.put` ops of the addon `m` in `dir` put over F1 25's packed game data, from its manifest's
/// `operations.files` and the `file.put` targets its asset op files write out, before any option fills them.
pub fn scan(dir: &Path, m: &AddonManifest) -> Option<Packed> {
    let declared = m.operations.files.iter().map(|f| f.target.as_str());
    let files: Vec<Vec<u8>> = m
        .operations
        .assets
        .iter()
        .filter_map(|rel| std::fs::read(dir.join(rel)).ok())
        .collect();
    let mentions = |bytes: &[u8]| bytes.windows(7).any(|w| w.eq_ignore_ascii_case(b"game.da"));
    if !declared.clone().any(|t| packed(t).is_some()) && !files.iter().any(|f| mentions(f)) {
        return None;
    }
    let mut targets: Vec<String> = declared.map(str::to_owned).collect();
    for bytes in &files {
        let Ok(doc) = serde_json::from_slice::<serde_json::Value>(bytes) else {
            continue;
        };
        let ops = doc.get("ops").and_then(serde_json::Value::as_array);
        targets.extend(ops.into_iter().flatten().filter_map(|op| {
            (op.get("op")?.as_str()? == "file.put").then_some(())?;
            Some(op.get("target")?.as_str()?.to_owned())
        }));
    }
    Packed::of(targets.iter().map(String::as_str))
}

/// Why the addon `m`, unpacked in `dir`, is not installed: it puts F1 25's packed game data and its files
/// are made for another F1 game. One that only puts the packed game data installs, and its builds leave
/// that file out.
pub fn install_refusal(dir: &Path, m: &AddonManifest) -> Option<String> {
    let found = scan(dir, m)?;
    let other = found.other.as_ref()?;
    Some(format!(
        "{} is made for {}, not F1 25 (it has {}), and it replaces {}, F1 25's packed game data. F1 25 can't \
         start with that replaced, so {} was not installed",
        m.name,
        other.game,
        other.sign,
        found.names(),
        m.name
    ))
}

/// Why the mod named `file`, whose archive or folder holds the paths `names`, is not imported: it holds F1 25's
/// packed game data and its files are made for another F1 game.
pub fn import_refusal(file: &str, names: &[String]) -> Option<String> {
    let files = packed_in(names.iter().map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n)));
    if files.is_empty() {
        return None;
    }
    let other = made_for(names.iter().map(String::as_str))?;
    Some(format!(
        "{file} is made for {}, not F1 25 (it has {}), and it replaces {}, F1 25's packed game data. F1 25 can't \
         start with that replaced, so it was not imported",
        other.game,
        other.sign,
        files.join(" and ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addon(dir: &Path, ops: &str, files: &str) -> AddonManifest {
        std::fs::create_dir_all(dir.join("ops")).unwrap();
        std::fs::write(
            dir.join("ops/files.json"),
            format!(r#"{{ "schema_version": 1, "ops": [{ops}] }}"#),
        )
        .unwrap();
        AddonManifest::parse(&format!(
            r#"{{ "schema_version": 1, "id": "imported-mod.pastoot", "name": "PastootF1", "version": "1.0.0",
                 "author": "t", "compat": {{ "game": "f1_25" }},
                 "operations": {{ "assets": ["ops/files.json"], "files": [{files}] }} }}"#
        ))
        .unwrap()
    }

    fn put(target: &str) -> String {
        format!(r#"{{ "op": "file.put", "source": "files/{target}", "target": "{target}" }}"#)
    }

    #[test]
    fn an_import_of_another_games_mod_that_replaces_game_dat_is_refused() {
        let dir = std::env::temp_dir().join(format!("swofty-other-game-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let ops = [
            put("2020_asset_groups/ui_package/flow_frontend.erp"),
            put("2020_asset_groups/ui_package/textures/track_maps/sepang.erp"),
            put("game.dat"),
            put("videos/attract.bk2"),
        ];
        let m = addon(&dir, &ops.join(","), "");
        assert_eq!(
            scan(&dir, &m),
            Some(Packed {
                files: vec!["game.dat"],
                other: Some(OtherGame {
                    game: "F1 2020".into(),
                    sign: "2020_asset_groups".into()
                }),
            })
        );
        assert_eq!(
            install_refusal(&dir, &m).as_deref(),
            Some(
                "PastootF1 is made for F1 2020, not F1 25 (it has 2020_asset_groups), and it replaces game.dat, \
                 F1 25's packed game data. F1 25 can't start with that replaced, so PastootF1 was not installed"
            )
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_f1_25_addon_installs_and_one_that_only_puts_game_dat_is_left_to_the_build() {
        let dir = std::env::temp_dir().join(format!("swofty-own-game-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let m = addon(
            &dir,
            &[put("2025_asset_groups/a.erp"), put("videos/attract.bk2")].join(","),
            "",
        );
        assert_eq!(scan(&dir, &m), None);
        assert_eq!(install_refusal(&dir, &m), None);
        let m = addon(
            &dir,
            &put("2025_asset_groups/a.erp"),
            r#"{ "source": "files/game.dat", "target": "Game.dat" }"#,
        );
        assert_eq!(
            scan(&dir, &m),
            Some(Packed {
                files: vec!["game.dat"],
                other: None
            })
        );
        assert_eq!(install_refusal(&dir, &m), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_dropped_mod_for_another_game_is_refused_before_it_is_imported() {
        let names: Vec<String> = [
            "PastootF1/F1_2020.exe",
            "PastootF1/F1_2020_dx12.exe",
            "PastootF1/game.dat",
            "PastootF1/2020_asset_groups/ui_package/flow_frontend.erp",
            "PastootF1/videos/attract.bk2",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            import_refusal("PastootF1.zip", &names).as_deref(),
            Some(
                "PastootF1.zip is made for F1 2020, not F1 25 (it has F1_2020.exe), and it replaces game.dat, \
                 F1 25's packed game data. F1 25 can't start with that replaced, so it was not imported"
            )
        );
        let livery: Vec<String> = ["2020_asset_groups/livery.erp"].map(str::to_owned).to_vec();
        assert_eq!(import_refusal("livery.zip", &livery), None);
        let own: Vec<String> = [
            "F1 25/game.dat",
            "F1 25/2025_asset_groups/a.erp",
            "F1 25/F1_25.exe",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(import_refusal("F1 25", &own), None);
    }
}
