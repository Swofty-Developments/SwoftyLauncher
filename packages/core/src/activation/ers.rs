use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_formats::bxml::{Document, Element};
use swofty_formats::erp::{Entry, ErpFile, Packer, Resource, ZSTD_LEVEL};

use crate::{Error, Result, io_err};

pub const TEAMS: &str = "2025_asset_groups/f1_2026_vehicle_package/teams";
const ERS: &[u8] = b"EnergyRecoverySystem";
const SETTINGS: &[u8] = b"TrackSettings";
const COUNT: &[u8] = b"numTrackSettingsTypes";
pub const FIELDS: &[&str] = &[
    "Track",
    "MediumDeployFactor",
    "MediumDerateFactor",
    "HotLapDeployFactor",
    "HotLapDerateFactor",
    "HarvestFactorMGUH",
    "OverrideDetectionLapProgress",
    "OverrideActivationLapProgress",
    "HarvestLimitMGUK",
    "HarvestLimitQualifyingMGUK",
    "HarvestTorqueFactor",
];

pub struct ErsMap<'a> {
    pub code: &'a str,
    pub base: &'a str,
    pub values: &'a BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ErsBuild {
    pub dir: Option<PathBuf>,
    pub replaced: BTreeMap<String, PathBuf>,
    pub handling_files: usize,
    pub notes: Vec<String>,
}

pub fn format_value(v: &serde_json::Value) -> Result<String> {
    match v {
        serde_json::Value::Number(n) if n.is_i64() || n.is_u64() => Ok(n.to_string()),
        serde_json::Value::Number(n) => {
            let f = n.as_f64().expect("number");
            Ok(if f.fract() == 0.0 && f.abs() >= 1e6 {
                format!("{}", f as i64)
            } else {
                format!("{f}")
            })
        }
        serde_json::Value::String(s) => Ok(s.clone()),
        other => Err(Error::Map(format!("ers value {other} is not a number"))),
    }
}

fn ers_node(root: &mut Element) -> Option<&mut Element> {
    root.children
        .iter_mut()
        .find(|c| c.name == ERS && c.child(SETTINGS).is_some())
}

fn track_of(e: &Element) -> Option<&[u8]> {
    e.child(b"Track")?.attr(b"value")
}

pub fn add_track(root: &mut Element, map: &ErsMap) -> Result<bool> {
    let Some(ers) = ers_node(root) else {
        return Ok(false);
    };
    let code = map.code.as_bytes();
    let count = ers.children_named(SETTINGS).count();
    let at = match ers
        .children
        .iter()
        .position(|c| c.name == SETTINGS && track_of(c) == Some(code))
    {
        Some(i) => i,
        None => {
            let Some(base) = ers
                .children
                .iter()
                .find(|c| c.name == SETTINGS && track_of(c) == Some(map.base.as_bytes()))
                .cloned()
            else {
                return Ok(false);
            };
            let mut new = base;
            new.set_attr(b"name", count.to_string().as_bytes());
            if let Some(t) = new.child_mut(b"Track") {
                t.set_attr(b"value", code);
            }
            let last = ers
                .children
                .iter()
                .rposition(|c| c.name == SETTINGS)
                .expect("has settings");
            ers.children.insert(last + 1, new);
            ers.set_attr(COUNT, (count + 1).to_string().as_bytes());
            last + 1
        }
    };
    let template = ers.children[at]
        .children
        .iter()
        .find(|c| c.name != b"Track")
        .or_else(|| ers.children[at].children.first())
        .cloned()
        .ok_or_else(|| Error::Map("an ERS TrackSettings entry has no fields".into()))?;
    let entry = &mut ers.children[at];
    for (key, value) in map.values {
        // A key this build does not know (a newer track's file, such as DeployLimitMGUK) is left out rather than
        // refusing the whole build.
        let Some(order) = FIELDS.iter().position(|f| f == key).filter(|&i| i > 0) else {
            continue;
        };
        let value = format_value(value)?;
        if let Some(f) = entry.child_mut(key.as_bytes()) {
            f.set_attr(b"value", value.as_bytes());
            continue;
        }
        let mut leaf = template.clone();
        leaf.name = key.as_bytes().to_vec();
        leaf.attrs = vec![(b"value".to_vec(), value.into_bytes())];
        leaf.children.clear();
        let pos = entry
            .children
            .iter()
            .position(|c| {
                FIELDS
                    .iter()
                    .position(|f| f.as_bytes() == c.name.as_slice())
                    .is_some_and(|i| i > order)
            })
            .unwrap_or(entry.children.len());
        entry.children.insert(pos, leaf);
    }
    Ok(true)
}

pub(crate) fn set_data(packer: &Packer, res: &mut Resource, index: usize, data: &[u8]) -> Result<()> {
    crate::erp::pack::set_fragment_at(packer, res, index, data, ZSTD_LEVEL)
}

#[cfg(test)]
thread_local! {
    static PARSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether the ERS edit for `maps` can change the handling file `data`. A BXML file keeps its element names and
/// attribute values as they are, so one without TrackSettings under EnergyRecoverySystem, or without a Track value
/// naming a map's track or its base, has nothing [`add_track`] can change. A file that is not BXML is left to the
/// parser.
fn may_change(data: &[u8], maps: &[ErsMap]) -> bool {
    let has = |needle: &[u8]| data.windows(needle.len()).any(|w| w == needle);
    let value = |track: &str| [&b"value\0"[..], track.as_bytes(), b"\0"].concat();
    !data.starts_with(swofty_formats::bxml::MAGIC)
        || has(&[ERS, b"\0"].concat())
            && has(&[SETTINGS, b"\0"].concat())
            && maps.iter().any(|m| has(&value(m.code)) || has(&value(m.base)))
}

pub fn edit_vtf(res: &Resource, maps: &[ErsMap], packer: &Packer) -> Result<Option<Resource>> {
    let data = res.fragments[0].data()?;
    if !may_change(&data, maps) {
        return Ok(None);
    }
    #[cfg(test)]
    PARSED.with(|n| n.set(n.get() + 1));
    let mut doc = Document::parse(&data)?;
    let mut changed = false;
    for m in maps {
        changed |= add_track(&mut doc.root, m)?;
    }
    if !changed {
        return Ok(None);
    }
    let bytes = doc.to_bytes();
    if bytes == data {
        return Ok(None);
    }
    let mut out = res.clone();
    set_data(packer, &mut out, 0, &bytes)?;
    if let Some(stats) = out.fragments.get(1) {
        let old = stats.data()?;
        let version = old
            .get(4..8)
            .map_or(1, |b| u32::from_le_bytes(b.try_into().expect("4 bytes")));
        set_data(packer, &mut out, 1, &doc.string_stats(version))?;
    }
    Ok(Some(out))
}

/// Whether `rel` is a team archive of a vehicle package, whose handling files take the tracks' entries.
pub fn is_car_archive(rel: &str) -> bool {
    let rel = rel.to_ascii_lowercase();
    rel.starts_with("2025_asset_groups/") && rel.contains("_vehicle_package/teams/") && rel.ends_with(".erp")
}

pub fn in_teams(rel: &str) -> bool {
    rel.len() >= TEAMS.len()
        && rel.is_char_boundary(TEAMS.len())
        && rel[..TEAMS.len()].eq_ignore_ascii_case(TEAMS)
}

/// Writes to `out` a copy of the archive at `src` with the tracks' entries in its handling files, and returns how
/// many handling files changed; with none it writes nothing.
pub fn edit_archive(src: &Path, maps: &[ErsMap], out: &Path, packer: &Packer) -> Result<usize> {
    let file = ErpFile::open(src)?;
    let mut replace = BTreeMap::new();
    for (i, r) in file.erp.resources.iter().enumerate() {
        if r.kind() == "VTF"
            && let Some(new) = edit_vtf(&file.load(r)?, maps, packer)?
        {
            replace.insert(i, new);
        }
    }
    let changed = replace.len();
    if changed == 0 {
        return Ok(0);
    }
    let entries = file
        .erp
        .resources
        .iter()
        .enumerate()
        .map(|(i, r)| match replace.remove(&i) {
            Some(new) => Entry::Owned(new),
            None => Entry::Copy(r),
        })
        .collect();
    let dir = out.parent().expect("has parent");
    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    file.write(entries, out)?;
    Ok(changed)
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<()> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for e in rd {
        let p = e.map_err(io_err(dir))?.path();
        if p.is_dir() {
            walk(&p, base, out)?;
        } else if p.extension().is_some_and(|x| x == "erp") {
            out.push(
                p.strip_prefix(base)
                    .expect("under base")
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

pub fn car_archives(game: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    walk(&game.join(TEAMS), game, &mut out)?;
    out.sort();
    Ok(out)
}

pub fn stamp(maps: &[ErsMap], archives: &[String], original: &dyn Fn(&str) -> PathBuf) -> Result<String> {
    let mut h = <sha2::Sha256 as sha2::Digest>::new();
    for m in maps {
        sha2::Digest::update(
            &mut h,
            format!(
                "{}|{}|{}",
                m.code,
                m.base,
                serde_json::to_string(m.values).expect("json")
            ),
        );
    }
    for rel in archives {
        let p = original(rel);
        let meta = std::fs::metadata(&p).map_err(io_err(&p))?;
        sha2::Digest::update(&mut h, format!("{rel}|{}|{:?}", meta.len(), meta.modified().ok()));
    }
    Ok(crate::game_folder::baseline::hex(&sha2::Digest::finalize(h))[..16].to_owned())
}

pub fn build_ers(
    maps: &[ErsMap],
    game: &Path,
    original: &dyn Fn(&str) -> PathBuf,
    cache: &Path,
    packer: &Packer,
) -> Result<ErsBuild> {
    let mut build = ErsBuild::default();
    if maps.is_empty() {
        return Ok(build);
    }
    let archives = car_archives(game)?;
    let out = cache.join(stamp(maps, &archives, original)?);
    build.dir = Some(out.clone());
    let done = out.join(".done");
    if !done.is_file() {
        let mut count = 0;
        for rel in &archives {
            count += edit_archive(&original(rel), maps, &out.join(rel), packer)?;
        }
        std::fs::create_dir_all(&out).map_err(io_err(&out))?;
        std::fs::write(&done, count.to_string()).map_err(io_err(&done))?;
    }
    for rel in &archives {
        let dst = out.join(rel);
        if dst.is_file() {
            build.replaced.insert(rel.clone(), dst);
        }
    }
    build.handling_files = std::fs::read_to_string(&done)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    for m in maps {
        if !m.values.is_empty() && build.handling_files == 0 {
            build.notes.push(format!(
                "{}: no car has ERS settings for {} to copy",
                m.code, m.base
            ));
        }
    }
    Ok(build)
}

#[cfg(test)]
mod tests {
    use super::*;
    use swofty_formats::bxml::Kind;

    #[test]
    fn a_second_ers_edit_takes_its_frames_from_the_cache() {
        use crate::erp::pack::frame_cache;
        let padding = (0..900)
            .map(|i| leaf(&format!("Pad{i}"), "0.123456789"))
            .collect();
        let root = el(
            "VTF",
            &[],
            vec![
                el(
                    "EnergyRecoverySystem",
                    &[("numTrackSettingsTypes", "1")],
                    vec![el(
                        "TrackSettings",
                        &[("name", "0")],
                        vec![leaf("Track", "bahrain"), leaf("HarvestLimitMGUK", "7400000")],
                    )],
                ),
                el("Padding", &[], padding),
            ],
        );
        let data = Document {
            root,
            tail: Vec::new(),
        }
        .to_bytes();
        assert!(
            data.len() > 16 << 10,
            "the handling file is {} bytes, under the cached size",
            data.len()
        );
        let res = frame_cache::zstd_resource("eaid://vehicle_package/teams/a/a.vtf", "VTF", &data);
        let values = BTreeMap::from([("HarvestLimitMGUK".to_owned(), serde_json::json!(9000000))]);
        let maps = [ErsMap {
            code: "bahrain24",
            base: "bahrain",
            values: &values,
        }];
        let dir = std::env::temp_dir().join(format!("swofty-ers-frames-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let packer = Packer::cached(&dir);
        let first = edit_vtf(&res, &maps, &packer).unwrap().unwrap();
        assert!(!frame_cache::marked(first.fragments[0].raw().unwrap()));
        assert_eq!(frame_cache::mark(&dir), 1);
        let second = edit_vtf(&res, &maps, &packer).unwrap().unwrap();
        assert!(frame_cache::marked(second.fragments[0].raw().unwrap()));
        assert_eq!(
            second.fragments[0].data().unwrap(),
            first.fragments[0].data().unwrap()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    fn leaf(name: &str, v: &str) -> Element {
        Element {
            kind: Kind::Element,
            pad: 0,
            name: name.as_bytes().to_vec(),
            attrs: vec![(b"value".to_vec(), v.as_bytes().to_vec())],
            children: Vec::new(),
            end_pad: 0,
        }
    }

    fn el(name: &str, attrs: &[(&str, &str)], children: Vec<Element>) -> Element {
        Element {
            kind: Kind::Element,
            pad: 0,
            name: name.as_bytes().to_vec(),
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec()))
                .collect(),
            children,
            end_pad: 0,
        }
    }

    #[test]
    fn adds_a_track_cloned_from_its_base() {
        let mut root = el(
            "VTF",
            &[],
            vec![el(
                "EnergyRecoverySystem",
                &[("numTrackSettingsTypes", "1")],
                vec![el(
                    "TrackSettings",
                    &[("name", "0")],
                    vec![
                        leaf("Track", "bahrain"),
                        leaf("MediumDeployFactor", "0.8"),
                        leaf("HarvestLimitMGUK", "7400000"),
                    ],
                )],
            )],
        );
        let values = BTreeMap::from([
            ("HarvestLimitMGUK".to_owned(), serde_json::json!(9000000)),
            ("HotLapDerateFactor".to_owned(), serde_json::json!(0.1)),
        ]);
        let m = ErsMap {
            code: "bahrain24",
            base: "bahrain",
            values: &values,
        };
        assert!(add_track(&mut root, &m).unwrap());
        let ers = &root.children[0];
        assert_eq!(ers.attr(b"numTrackSettingsTypes"), Some(&b"2"[..]));
        let new = &ers.children[1];
        assert_eq!(new.attr(b"name"), Some(&b"1"[..]));
        let names: Vec<&[u8]> = new.children.iter().map(|c| c.name.as_slice()).collect();
        assert_eq!(
            names,
            [
                &b"Track"[..],
                b"MediumDeployFactor",
                b"HotLapDerateFactor",
                b"HarvestLimitMGUK"
            ]
        );
        assert_eq!(
            new.child(b"Track").unwrap().attr(b"value"),
            Some(&b"bahrain24"[..])
        );
        assert_eq!(
            new.child(b"HarvestLimitMGUK").unwrap().attr(b"value"),
            Some(&b"9000000"[..])
        );
        assert_eq!(
            new.child(b"HotLapDerateFactor").unwrap().attr(b"value"),
            Some(&b"0.1"[..])
        );
        assert!(add_track(&mut root, &m).unwrap());
        assert_eq!(root.children[0].children.len(), 2);
    }

    #[test]
    fn a_key_this_build_does_not_know_is_left_out() {
        let mut root = el(
            "VTF",
            &[],
            vec![el(
                "EnergyRecoverySystem",
                &[("numTrackSettingsTypes", "1")],
                vec![el(
                    "TrackSettings",
                    &[("name", "0")],
                    vec![leaf("Track", "bahrain"), leaf("MediumDeployFactor", "0.8")],
                )],
            )],
        );
        let values = BTreeMap::from([
            ("DeployLimitMGUK".to_owned(), serde_json::json!(4000000)),
            ("MediumDeployFactor".to_owned(), serde_json::json!(0.9)),
        ]);
        let m = ErsMap {
            code: "qiddiya",
            base: "bahrain",
            values: &values,
        };
        assert!(add_track(&mut root, &m).unwrap());
        let new = &root.children[0].children[1];
        assert!(new.child(b"DeployLimitMGUK").is_none());
        assert_eq!(
            new.child(b"MediumDeployFactor").unwrap().attr(b"value"),
            Some(&b"0.9"[..])
        );
    }

    fn vtf_archive(path: &Path, vtfs: &[(&str, Element)]) {
        use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, STORED};
        let mut header = [0u8; HEADER_LEN];
        header[..4].copy_from_slice(MAGIC);
        header[4..8].copy_from_slice(&3u32.to_le_bytes());
        let resources = vtfs
            .iter()
            .map(|(name, root)| {
                let data = Document {
                    root: root.clone(),
                    tail: Vec::new(),
                }
                .to_bytes();
                Resource {
                    name: (*name).into(),
                    kind: swofty_formats::erp::kind("VTF").unwrap(),
                    unk: vec![0; 4],
                    fragments: vec![Fragment::new(*b"temp", data.len() as u64, 0x10, STORED, data)],
                    hash: [0; 16],
                }
            })
            .collect();
        let bytes = Erp {
            header,
            version: 3,
            resources,
        }
        .to_bytes()
        .unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn settings(track: &str) -> Element {
        el(
            "VTF",
            &[],
            vec![el(
                "EnergyRecoverySystem",
                &[("numTrackSettingsTypes", "1")],
                vec![el(
                    "TrackSettings",
                    &[("name", "0")],
                    vec![leaf("Track", track), leaf("HarvestLimitMGUK", "7400000")],
                )],
            )],
        )
    }

    #[test]
    fn a_handling_file_no_track_setting_can_change_is_left_unread() {
        let dir = std::env::temp_dir().join(format!("swofty-ers-unread-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let values = BTreeMap::from([("HarvestLimitMGUK".to_owned(), serde_json::json!(9000000))]);
        let maps = [ErsMap {
            code: "bahrain24",
            base: "bahrain",
            values: &values,
        }];
        let packer = Packer::default();
        let chassis = el("VTF", &[], vec![el("Chassis", &[], vec![leaf("Mass", "798")])]);
        let src = dir.join("none.erp");
        vtf_archive(
            &src,
            &[
                ("eaid://p/teams/a/handling/a.vtf", chassis.clone()),
                ("eaid://p/teams/b/handling/b.vtf", settings("monza")),
            ],
        );
        PARSED.with(|n| n.set(0));
        let out = dir.join("none-out.erp");
        assert_eq!(edit_archive(&src, &maps, &out, &packer).unwrap(), 0);
        assert!(!out.exists());
        assert_eq!(
            PARSED.with(|n| n.get()),
            0,
            "a handling file without the track's or its base's settings is parsed"
        );

        let src = dir.join("some.erp");
        vtf_archive(
            &src,
            &[
                ("eaid://p/teams/a/handling/a.vtf", chassis),
                ("eaid://p/teams/b/handling/b.vtf", settings("bahrain")),
            ],
        );
        PARSED.with(|n| n.set(0));
        let out = dir.join("some-out.erp");
        assert_eq!(edit_archive(&src, &maps, &out, &packer).unwrap(), 1);
        assert_eq!(PARSED.with(|n| n.get()), 1);
        let file = ErpFile::open(&out).unwrap();
        let doc = Document::parse(
            &file.load(&file.erp.resources[1]).unwrap().fragments[0]
                .data()
                .unwrap(),
        )
        .unwrap();
        let mut root = settings("bahrain");
        assert!(add_track(&mut root, &maps[0]).unwrap());
        assert_eq!(doc.root, root);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn settings_equal_to_the_handling_files_own_change_nothing() {
        let values = BTreeMap::from([("HarvestLimitMGUK".to_owned(), serde_json::json!(7400000))]);
        let maps = [ErsMap {
            code: "bahrain",
            base: "bahrain",
            values: &values,
        }];
        let data = Document {
            root: settings("bahrain"),
            tail: Vec::new(),
        }
        .to_bytes();
        let res = Resource {
            name: "eaid://p/teams/b/handling/b.vtf".into(),
            kind: swofty_formats::erp::kind("VTF").unwrap(),
            unk: vec![0; 4],
            fragments: vec![swofty_formats::erp::Fragment::new(
                *b"temp",
                data.len() as u64,
                0x10,
                swofty_formats::erp::STORED,
                data,
            )],
            hash: [0; 16],
        };
        assert!(edit_vtf(&res, &maps, &Packer::default()).unwrap().is_none());
    }

    #[test]
    fn number_formatting_matches_the_handling_files() {
        assert_eq!(format_value(&serde_json::json!(9000000)).unwrap(), "9000000");
        assert_eq!(format_value(&serde_json::json!(0.953)).unwrap(), "0.953");
        assert_eq!(format_value(&serde_json::json!(9000000.0)).unwrap(), "9000000");
    }
}
