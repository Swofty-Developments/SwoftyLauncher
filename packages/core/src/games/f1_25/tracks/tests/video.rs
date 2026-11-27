use super::*;
use crate::stdlib::deps::Std;

fn addon_with(root: &std::path::Path, track: &str, video: Option<(&str, &[u8])>) -> BTreeMap<String, Std> {
    std::fs::create_dir_all(root.join("mod.v/registry")).unwrap();
    let field = match video {
        Some((rel, bytes)) => {
            let at = root.join("mod.v").join(rel);
            std::fs::create_dir_all(at.parent().unwrap()).unwrap();
            std::fs::write(at, bytes).unwrap();
            format!(r#", "video": "{rel}""#)
        }
        None => String::new(),
    };
    std::fs::write(
        root.join("mod.v/registry/track.json"),
        format!(r#"{{ "track": {{ "{track}": {{ "code": "{track}", "base": "game/melbourne", "files": "t/"{field} }} }} }}"#),
    )
    .unwrap();
    let s = Std {
        id: "mod.v".into(),
        name: "mod.v".into(),
        version: semver::Version::new(1, 0, 0),
        registry: None,
        lock: None,
        data_files: vec!["registry/track.json".into()],
        records: Vec::new(),
        files: Default::default(),
        unreadable: Vec::new(),
        older_tracks: Vec::new(),
    };
    BTreeMap::from([("mod.v".to_owned(), s)])
}

fn run(
    root: &std::path::Path,
    std16: &BTreeMap<String, Std>,
    kept: &[&str],
) -> (Vec<TrackVideo>, Vec<String>) {
    let kept = kept.iter().map(|c| c.to_string()).collect();
    track_videos(root, &["mod.v".to_owned()], std16, &kept)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("swofty-track-video-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    root
}

#[test]
fn the_value_is_below_videos_and_unique_by_addon_and_code() {
    assert_eq!(
        video_value("mod.okayama", "okayama"),
        "track_map/swofty/mod.okayama/okayama.bk2"
    );
    assert_ne!(video_value("a.b", "c"), video_value("a.b", "d"));
    assert_ne!(video_value("a", "c"), video_value("b", "c"));
    assert_eq!(
        video_value("Me/../x", "Ring 1"),
        "track_map/swofty/me_.._x/ring_1.bk2"
    );
}

#[test]
fn a_bink_2_file_in_the_addon_is_placed_for_a_kept_track() {
    let root = scratch("ok");
    let mut bytes = b"KB2j".to_vec();
    bytes.extend([1u8; 16]);
    let std16 = addon_with(&root, "ring", Some(("videos/ring.bk2", &bytes)));
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(videos.len(), 1);
    let v = &videos[0];
    assert_eq!(
        (
            v.id.as_str(),
            v.code.as_str(),
            v.value.as_str(),
            v.game_path.as_str(),
            v.game_path_26.as_str()
        ),
        (
            "mod.v/ring",
            "ring",
            "track_map/swofty/mod.v/ring.bk2",
            "videos/track_map/swofty/mod.v/ring.bk2",
            "videos/track_map/26/swofty/mod.v/ring.bk2"
        )
    );
    assert_eq!(v.source, root.join("mod.v/videos/ring.bk2"));
    let (none, quiet) = run(&root, &std16, &[]);
    assert!(
        none.is_empty() && quiet.is_empty(),
        "a track the build leaves out gets nothing"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_record_without_a_video_gets_nothing() {
    let root = scratch("none");
    let std16 = addon_with(&root, "ring", None);
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert!(videos.is_empty() && warnings.is_empty());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn each_unusable_video_is_a_warning_and_the_track_keeps_its_base_s() {
    let root = scratch("bad");
    let bink = {
        let mut b = b"KB2j".to_vec();
        b.extend([0u8; 8]);
        b
    };
    let cases: [(&str, Option<&[u8]>, &str); 7] = [
        ("videos/ring.bk2", None, "is not in the addon"),
        ("videos/ring.mp4", Some(&bink), "is not a .bk2 file"),
        ("../ring.bk2", Some(&bink), "leaves the addon's folder"),
        (
            "videos/ring.bk2",
            Some(b"BIK1 old"),
            "is not a Bink 2 file (it does not start with KB2)",
        ),
        ("videos/ring.bk2", Some(b"KB"), "is not a Bink 2 file"),
        ("videos/ring.bk2", Some(b""), "is not a Bink 2 file"),
        ("videos/", None, "is not a .bk2 file"),
    ];
    for (rel, bytes, why) in cases {
        std::fs::remove_dir_all(&root).ok();
        let std16 = addon_with(
            &root,
            "ring",
            bytes.map(|b| (rel, b)).filter(|(r, _)| !r.starts_with("..")),
        );
        if bytes.is_none() || rel.starts_with("..") {
            let f = root.join("mod.v/registry/track.json");
            let text = std::fs::read_to_string(&f).unwrap();
            std::fs::write(
                &f,
                text.replace(r#""files": "t/""#, &format!(r#""files": "t/", "video": "{rel}""#)),
            )
            .unwrap();
        }
        let (videos, warnings) = run(&root, &std16, &["ring"]);
        assert!(videos.is_empty(), "{rel}");
        assert_eq!(warnings.len(), 1, "{rel}: {warnings:?}");
        assert!(
            warnings[0].starts_with("track mod.v/ring: video ")
                && warnings[0].contains(why)
                && warnings[0].ends_with("; it plays its base track's intro"),
            "{rel}: {}",
            warnings[0]
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_video_over_the_size_cap_is_refused() {
    let root = scratch("big");
    let mut bytes = b"KB2j".to_vec();
    bytes.push(0);
    let std16 = addon_with(&root, "ring", Some(("videos/ring.bk2", &bytes)));
    let f = std::fs::OpenOptions::new()
        .write(true)
        .open(root.join("mod.v/videos/ring.bk2"))
        .unwrap();
    f.set_len(MAX_VIDEO_BYTES + 1).unwrap();
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert!(videos.is_empty());
    assert!(
        warnings[0].contains("MB, over the 256 MB a circuit intro may be"),
        "{warnings:?}"
    );
    f.set_len(MAX_VIDEO_BYTES).unwrap();
    let (videos, _) = run(&root, &std16, &["ring"]);
    assert_eq!(videos.len(), 1);
    std::fs::remove_dir_all(&root).ok();
}
