use std::path::{Path, PathBuf};

use super::super::fixture::*;
use super::super::*;
use super::*;

const STEAM: &str = "46009bd31f49d93a92d73cbd0b66131d2b37964a92da7293168d3c3a872d9389";
const EA: &str = "8361a266d7aa1f43bdb35da89ccfe66010901e43dddd44b6d5cb8c9f090a99f7";
const EA_KB: &str = "CC506879C1C834AA54D3478B2F2133D2EC243F2F2D313397631145EA618FF7F4";

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plain-split-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn shared(d: &Path) -> Plain {
    Plain::of_source(d, "launcher-default")
}

fn ea_store(d: &Path) -> Plain {
    Plain::of_source(d, "launcher-ea.default")
}

fn steam_gen(n: u64, event: &str, session: &str) -> Gen<'static> {
    let s: &'static str = Box::leak(session.to_owned().into_boxed_str());
    let e: &'static str = Box::leak(event.to_owned().into_boxed_str());
    career_gen(n, e, Some(s), &[("0x0a", "Steam Jacob")])
}

fn ea_gen(n: u64, event: &str, session: &str) -> Gen<'static> {
    let s: &'static str = Box::leak(session.to_owned().into_boxed_str());
    let e: &'static str = Box::leak(event.to_owned().into_boxed_str());
    let mut containers = pair("driver_career", "game", vec![("0x01", "Stock")]);
    containers.extend(pair("f1_world", "kept", vec![("0x0b", "EA Ada")]));
    Gen {
        n,
        event: e,
        session: Some(s),
        containers,
    }
}

fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(rd) = std::fs::read_dir(at) else { return };
        for e in rd.flatten() {
            let p = e.path();
            match p.is_dir() {
                true => walk(root, &p, out),
                false => out.push((
                    p.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(&p).unwrap(),
                )),
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn bags(p: &Plain, n: u64) -> Vec<Vec<u8>> {
    let m = p.manifest(n).unwrap();
    m.containers
        .iter()
        .map(|c| std::fs::read(p.bag_path(n, c)).unwrap())
        .collect()
}

#[test]
fn a_generation_is_known_by_the_exe_that_wrote_it() {
    let d = dir("writer");
    let p = shared(&d);
    write_on(&p, &steam_gen(1, "save", "s1"), STEAM);
    write_on(&p, &ea_gen(2, "save", "s2"), EA);
    write_on(&p, &ea_gen(3, "save", "s3"), EA_KB);
    write_on(&p, &ea_gen(4, "save", "s4"), "");
    write_on(&p, &ea_gen(5, "save", "s5"), "f".repeat(64).as_str());
    current(&p, 5);
    let writers: Vec<Writer> = (1..=5).map(|n| writer(&p.manifest(n).unwrap())).collect();
    assert_eq!(
        writers,
        [
            Writer::Steam,
            Writer::Ea,
            Writer::Ea,
            Writer::Unknown,
            Writer::Unknown
        ]
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn without_the_split_a_steam_load_after_an_ea_save_raises_a_loss() {
    let d = dir("unsplit");
    let p = shared(&d);
    write_on(&p, &steam_gen(39, "save", "s39"), STEAM);
    write_on(&p, &ea_gen(40, "save", "s40"), EA);
    write_on(&p, &steam_gen(41, "load", "s41"), STEAM);
    current(&p, 41);
    let loss = p
        .loss(&[])
        .expect("the EA save's career is missing from the Steam load");
    assert_eq!((loss.good, loss.newest), (40, 41));
    assert_eq!(loss.modes(), ["f1_world"]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_shared_store_of_steam_39_and_ea_40_splits_without_losing_either_side_or_raising_a_loss() {
    let d = dir("39-40");
    let s = shared(&d);
    write_on(&s, &steam_gen(39, "save", "s39"), STEAM);
    write_on(&s, &ea_gen(40, "save", "s40"), EA);
    current(&s, 40);
    let (m39, m40) = (s.manifest(39).unwrap(), s.manifest(40).unwrap());
    let (b39, b40) = (bags(&s, 39), bags(&s, 40));

    let split = split_profile(&d, "default").unwrap().unwrap();
    assert_eq!(
        split,
        Split {
            schema: 1,
            ea: vec![40],
            copied: true,
            unknown: Vec::new(),
        }
    );

    let e = ea_store(&d);
    assert_eq!(e.root, d.join("saves/default/ea-plain"));
    assert_eq!(s.newest().unwrap(), m39);
    assert_eq!(s.newest_session_save().unwrap(), m39);
    assert_eq!(bags(&s, 39), b39);
    assert_eq!(e.newest().unwrap(), m40);
    assert_eq!(e.newest_session_save().unwrap(), m40);
    assert_eq!(bags(&e, 40), b40);
    assert_eq!(e.current(), Some(40));
    assert_eq!(
        s.current(),
        Some(40),
        "the shared store's CURRENT stays as the core wrote it"
    );
    let kept = Plain::at(s.root.join(SPLIT_ASIDE));
    assert_eq!(
        kept.manifest(40).unwrap(),
        m40,
        "the shared store keeps the EA generation it held"
    );
    assert_eq!(bags(&kept, 40), b40);
    assert!(s.loss(&[]).is_none());
    assert!(e.loss(&[]).is_none());

    write_on(&s, &steam_gen(41, "load", "s41"), STEAM);
    current(&s, 41);
    write_on(&e, &ea_gen(41, "load", "e41"), EA);
    current(&e, 41);
    assert!(
        s.loss(&[]).is_none(),
        "a Steam session's first load after the split"
    );
    assert!(
        e.loss(&[]).is_none(),
        "an EA session's first load after the split"
    );
    assert_eq!(s.careers()[1].slot.name, "Steam Jacob");
    assert_eq!(e.careers()[1].slot.name, "EA Ada");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_ea_only_store_moves_to_the_ea_root_whole() {
    let d = dir("ea-only");
    let s = shared(&d);
    write_on(&s, &ea_gen(1, "load", "e1"), EA);
    write_on(&s, &ea_gen(2, "save", "e1"), EA);
    current(&s, 2);
    let split = split_profile(&d, "default").unwrap().unwrap();
    assert_eq!(split.ea, [1, 2]);
    assert!(split.copied);
    let e = ea_store(&d);
    assert_eq!(e.generations().len(), 2);
    assert_eq!(e.newest().unwrap().generation, 2);
    assert!(s.generations().is_empty());
    assert!(s.loss(&[]).is_none() && e.loss(&[]).is_none());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_steam_only_store_stays_as_it_is_and_the_ea_root_is_not_made() {
    let d = dir("steam-only");
    let s = shared(&d);
    write_on(&s, &steam_gen(1, "save", "s1"), STEAM);
    write_on(&s, &steam_gen(2, "load", "s2"), STEAM);
    current(&s, 2);
    let before = tree(&s.root);
    let split = split_profile(&d, "default").unwrap().unwrap();
    assert!(split.ea.is_empty() && !split.copied);
    let mut after = tree(&s.root);
    after.retain(|(p, _)| p != SPLIT);
    assert_eq!(after, before);
    assert!(!ea_store(&d).root.exists());
    assert!(split_all_held(&d).is_empty());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_profile_without_a_plain_store_has_nothing_to_split() {
    let d = dir("none");
    std::fs::create_dir_all(d.join("saves/default/remote")).unwrap();
    assert_eq!(split_profile(&d, "default").unwrap(), None);
    assert!(!shared(&d).root.join(SPLIT).exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_second_split_changes_nothing() {
    let d = dir("twice");
    let s = shared(&d);
    write_on(&s, &steam_gen(39, "save", "s39"), STEAM);
    write_on(&s, &ea_gen(40, "save", "s40"), EA);
    current(&s, 40);
    let lines = split_all_held(&d);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("profile default") && lines[0].contains("(40)"),
        "{}",
        lines[0]
    );
    let (shared_after, ea_after) = (tree(&s.root), tree(&ea_store(&d).root));
    assert_eq!(split_profile(&d, "default").unwrap(), None);
    assert!(split_all_held(&d).is_empty());
    assert_eq!(tree(&s.root), shared_after);
    assert_eq!(tree(&ea_store(&d).root), ea_after);
    std::fs::remove_dir_all(&d).ok();
}

fn three_way(d: &Path) -> (Plain, Manifest, Manifest, Manifest) {
    let s = shared(d);
    write_on(&s, &ea_gen(38, "save", "e38"), EA);
    write_on(&s, &steam_gen(39, "save", "s39"), STEAM);
    write_on(&s, &ea_gen(40, "save", "e40"), EA);
    current(&s, 40);
    let ms = (
        s.manifest(38).unwrap(),
        s.manifest(39).unwrap(),
        s.manifest(40).unwrap(),
    );
    (s, ms.0, ms.1, ms.2)
}

fn assert_split(d: &Path, s: &Plain, m38: &Manifest, m39: &Manifest, m40: &Manifest) {
    let e = ea_store(d);
    assert_eq!(s.generations(), std::slice::from_ref(m39));
    assert_eq!(e.generations(), [m40.clone(), m38.clone()]);
    assert_eq!(e.current(), Some(40));
    let kept = Plain::at(s.root.join(SPLIT_ASIDE));
    assert_eq!(kept.manifest(38).as_ref(), Some(m38));
    assert_eq!(kept.manifest(40).as_ref(), Some(m40));
    assert!(!e.root.join(COPYING).exists());
    assert!(s.root.join(SPLIT).is_file());
    assert!(s.loss(&[]).is_none() && e.loss(&[]).is_none());
}

#[test]
fn a_split_that_stopped_while_it_copied_runs_again_from_the_start_of_that_copy() {
    let d = dir("stop-copy");
    let (s, m38, m39, m40) = three_way(&d);
    let e = ea_store(&d);
    let torn = e.root.join(COPYING).join(format!("{:012}", 40));
    std::fs::create_dir_all(torn.join("containers")).unwrap();
    std::fs::write(torn.join("manifest.json"), b"{ half").unwrap();
    assert!(e.generations().is_empty(), "a copy in progress is no generation");
    let split = split_profile(&d, "default").unwrap().unwrap();
    assert_eq!(split.ea, [38, 40]);
    assert!(split.copied);
    assert_split(&d, &s, &m38, &m39, &m40);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_split_that_stopped_after_one_copy_finishes_the_rest() {
    let d = dir("stop-one");
    let (s, m38, m39, m40) = three_way(&d);
    let e = ea_store(&d);
    copy_generation(&s.generation_dir(38), &e, 38).unwrap();
    assert!(e.newest().is_none(), "no CURRENT yet, no store");
    split_profile(&d, "default").unwrap().unwrap();
    assert_split(&d, &s, &m38, &m39, &m40);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_split_that_stopped_while_it_set_generations_aside_finishes_them() {
    let d = dir("stop-aside");
    let (s, m38, m39, m40) = three_way(&d);
    let e = ea_store(&d);
    copy_generation(&s.generation_dir(38), &e, 38).unwrap();
    copy_generation(&s.generation_dir(40), &e, 40).unwrap();
    crate::write_json(&e.root.join(CURRENT), &40u64).unwrap();
    let kept = Plain::at(s.root.join(SPLIT_ASIDE));
    std::fs::create_dir_all(kept.root.join(GENERATIONS)).unwrap();
    std::fs::rename(s.generation_dir(38), kept.generation_dir(38)).unwrap();
    let split = split_profile(&d, "default").unwrap().unwrap();
    assert_eq!(
        split.ea,
        [38, 40],
        "a generation set aside before the stop still counts"
    );
    assert!(split.copied);
    assert_split(&d, &s, &m38, &m39, &m40);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_split_that_stopped_before_its_marker_writes_the_same_marker() {
    let d = dir("stop-marker");
    let (s, m38, m39, m40) = three_way(&d);
    let first = split_profile(&d, "default").unwrap().unwrap();
    std::fs::remove_file(s.root.join(SPLIT)).unwrap();
    let ea_before = tree(&ea_store(&d).root);
    let again = split_profile(&d, "default").unwrap().unwrap();
    assert_eq!(again, first);
    assert_eq!(tree(&ea_store(&d).root), ea_before);
    assert_split(&d, &s, &m38, &m39, &m40);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn an_ea_root_a_core_wrote_first_gets_no_copy_and_keeps_its_own_generations() {
    let d = dir("own");
    let (s, _, m39, m40) = three_way(&d);
    let e = ea_store(&d);
    write_on(&e, &ea_gen(1, "load", "new"), EA);
    current(&e, 1);
    let own = e.manifest(1).unwrap();
    let split = split_profile(&d, "default").unwrap().unwrap();
    assert_eq!(split.ea, [38, 40]);
    assert!(!split.copied);
    assert_eq!(e.generations(), [own]);
    assert_eq!(e.current(), Some(1));
    assert_eq!(s.generations(), [m39]);
    assert_eq!(Plain::at(s.root.join(SPLIT_ASIDE)).manifest(40), Some(m40));
    let lines = split_all_held(&d);
    assert!(lines.is_empty(), "the split ran once");
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_generation_no_known_build_wrote_stays_in_the_shared_store() {
    let d = dir("unknown");
    let s = shared(&d);
    write_on(&s, &steam_gen(1, "save", "s1"), STEAM);
    write_on(&s, &ea_gen(2, "save", "x2"), "");
    write_on(&s, &ea_gen(3, "save", "e3"), EA);
    current(&s, 3);
    let lines = split_all_held(&d);
    assert_eq!(lines.len(), 1);
    assert!(
        lines[0].contains("1 generation(s) from a build the launcher does not know"),
        "{}",
        lines[0]
    );
    let split: Split = crate::read_json(&s.root.join(SPLIT)).unwrap();
    assert_eq!((split.ea, split.unknown), (vec![3], vec![2]));
    assert_eq!(
        s.generations().iter().map(|m| m.generation).collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(ea_store(&d).newest().unwrap().generation, 3);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn a_torn_ea_generation_stays_where_it_is() {
    let d = dir("torn");
    let s = shared(&d);
    write_on(&s, &steam_gen(1, "save", "s1"), STEAM);
    write_on(&s, &ea_gen(2, "save", "e2"), EA);
    current(&s, 2);
    let m = s.manifest(2).unwrap();
    std::fs::remove_file(s.bag_path(2, &m.containers[0])).unwrap();
    let split = split_profile(&d, "default").unwrap().unwrap();
    assert!(split.ea.is_empty());
    assert!(s.generation_dir(2).is_dir());
    assert!(!ea_store(&d).root.exists());
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn every_profile_and_its_ea_root_are_plain_roots_for_pruning_and_withdrawing() {
    let d = dir("roots");
    let (s, ..) = three_way(&d);
    split_profile(&d, "default").unwrap().unwrap();
    let mut roots = super::super::prune::roots(&d);
    roots.sort();
    assert_eq!(roots, [ea_store(&d).root, s.root.clone()]);
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn the_lost_careers_banner_of_neither_copy_comes_up_after_the_split() {
    use crate::saves::vault::{Location, Vault};
    let d = dir("banner");
    let s = shared(&d);
    write_on(&s, &steam_gen(39, "save", "s39"), STEAM);
    write_on(&s, &ea_gen(40, "save", "e40"), EA);
    write_on(&s, &steam_gen(41, "load", "s41"), STEAM);
    current(&s, 41);
    let steam = Vault::of(&d, &Location::launcher(&d, "default")).unwrap();
    let ea = Vault::of(&d, &Location::launcher_ea(&d, "default")).unwrap();
    assert_eq!(
        steam.probable_loss().unwrap().generation(),
        40,
        "the shared store's false loss"
    );

    split_profile(&d, "default").unwrap().unwrap();
    assert!(steam.probable_loss().is_none());
    assert!(ea.probable_loss().is_none());

    let e = ea_store(&d);
    write_on(&e, &ea_gen(41, "load", "e41"), EA);
    current(&e, 41);
    write_on(&s, &steam_gen(42, "load", "s42"), STEAM);
    current(&s, 42);
    assert!(
        steam.probable_loss().is_none(),
        "the first Steam session after the split"
    );
    assert!(
        ea.probable_loss().is_none(),
        "the first EA app session after the split"
    );
    std::fs::remove_dir_all(&d).ok();
}
