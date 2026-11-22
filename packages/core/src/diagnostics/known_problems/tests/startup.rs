use super::graphics::{BOOT_ABORT, RESHADE_FOLDER};
use super::*;
use crate::addons::other_game::Packed;
use crate::asset::other_game::OtherGame;

const PASTOOT: (&str, &str) = ("imported-mod.pastoot", "PastootF1_2009-2013_v0.1.0_FULL");

fn with_pastoot(loader: &str, other: bool) -> Evidence {
    let mut e = with_names(ev(loader, RESHADE_FOLDER), &[PASTOOT]);
    e.packed = vec![(
        PASTOOT.0.to_owned(),
        Packed {
            files: vec!["game.dat"],
            other: other.then(|| OtherGame {
                game: "F1 2020".into(),
                sign: "2020_asset_groups".into(),
            }),
        },
    )];
    e
}

#[test]
fn an_addon_with_its_own_game_dat_is_named_for_an_abort_as_f1_25_starts() {
    let d = diagnose(&with_pastoot(BOOT_ABORT, true)).unwrap();
    assert_eq!(d.id, "packed_game_data");
    assert_eq!(
        d.title,
        "PastootF1_2009-2013_v0.1.0_FULL stopped F1 25 from starting"
    );
    assert_eq!(
        d.what,
        "F1 25 closed itself as it started, before it set up its graphics. PastootF1_2009-2013_v0.1.0_FULL is on \
         in your profile and carries its own game.dat, the file that holds F1 25's packed game data, and F1 25 \
         can't start with that file replaced. PastootF1_2009-2013_v0.1.0_FULL is made for F1 2020, not F1 25 (it \
         has 2020_asset_groups)."
    );
    assert_eq!(
        d.steps[..2],
        [
            "Turn off PastootF1_2009-2013_v0.1.0_FULL in your profile, then press Play.".to_owned(),
            "A mod made for another F1 game doesn't work in F1 25. Remove PastootF1_2009-2013_v0.1.0_FULL on the \
             Addons page."
                .to_owned()
        ]
    );
    assert_eq!(
        d.steps.last().map(String::as_str),
        Some(super::super::wording::VERIFY)
    );
    let own = diagnose(&with_pastoot(BOOT_ABORT, false)).unwrap();
    assert_eq!(own.id, "packed_game_data");
    assert!(!own.what.contains("made for"), "{}", own.what);
    assert_eq!(own.steps.len(), 2);
}

#[test]
fn the_cores_own_factory_beside_the_start_still_names_the_addon() {
    let beside = BOOT_ABORT.replacen(
        "2026-11-23 00:16:08.888",
        "2026-11-23 00:16:08.300 [27628] [d3d12] CreateDXGIFactory1 from swofty_core.dll (thread 27628) while the D3D12 arm runs: passed: the core's own call\n2026-11-23 00:16:08.888",
        1,
    );
    assert_eq!(
        diagnose(&with_pastoot(&beside, true)).map(|d| d.id),
        Some("packed_game_data")
    );
}

#[test]
fn an_addon_with_its_own_game_dat_is_not_blamed_once_f1_25_made_its_graphics() {
    let windowed = format!(
        "{BOOT_ABORT}2026-11-23 00:16:08.100 [29700] stage: window (USER32!RegisterClassExW from 00000001439f2670 (.tls$)), addons were loaded at the early stage\n"
    );
    assert!(diagnose(&with_pastoot(&windowed, true)).is_none_or(|d| d.id != "packed_game_data"));
    let device = BOOT_ABORT.replacen(
        "2026-11-23 00:16:08.888",
        "2026-11-23 00:16:08.500 [29700] [d3d12] the first D3D12CreateDevice from outside the D3D12 arm, from sl.interposer.dll (thread 29700): the arm had finished, 0.0 ms waited, 203.6 ms in the call (0x00000000)\n2026-11-23 00:16:08.888",
        1,
    );
    assert!(diagnose(&with_pastoot(&device, true)).is_none_or(|d| d.id != "packed_game_data"));
    assert!(diagnose(&with_names(ev(BOOT_ABORT, &["F1_25.exe"]), &[PASTOOT])).is_none());
}
