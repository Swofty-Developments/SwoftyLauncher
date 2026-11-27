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

fn bink() -> Vec<u8> {
    let mut b = b"KB2j".to_vec();
    b.extend([1u8; 16]);
    b
}

fn addon_with_fields(root: &std::path::Path, fields: &str, files: &[&str]) -> BTreeMap<String, Std> {
    let std16 = addon_with(root, "ring", None);
    for rel in files {
        let at = root.join("mod.v").join(rel);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bink()).unwrap();
    }
    let f = root.join("mod.v/registry/track.json");
    let text = std::fs::read_to_string(&f).unwrap();
    std::fs::write(
        &f,
        text.replace(r#""files": "t/""#, &format!(r#""files": "t/", {fields}"#)),
    )
    .unwrap();
    std16
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
            v.value.as_deref().unwrap(),
            v.placed[0].game_path.as_str(),
            v.placed[1].game_path.as_str()
        ),
        (
            "mod.v/ring",
            "ring",
            "track_map/swofty/mod.v/ring.bk2",
            "videos/track_map/swofty/mod.v/ring.bk2",
            "videos/track_map/26/swofty/mod.v/ring.bk2"
        )
    );
    assert_eq!(v.placed.len(), 2);
    assert_eq!(
        v.placed
            .iter()
            .map(|p| (p.slot, p.named_by.as_str()))
            .collect::<Vec<_>>(),
        [(Slot::Intro, "video"), (Slot::Intro26, "video")]
    );
    assert!(
        v.placed
            .iter()
            .all(|p| p.source == root.join("mod.v/videos/ring.bk2"))
    );
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

fn placed(videos: &[TrackVideo]) -> Vec<(Slot, String, String)> {
    videos
        .iter()
        .flat_map(|v| &v.placed)
        .map(|p| {
            (
                p.slot,
                p.named_by.clone(),
                p.source.file_name().unwrap().to_string_lossy().into_owned(),
            )
        })
        .collect()
}

#[test]
fn a_video_is_a_string_for_every_box_or_an_object_with_a_key_per_box() {
    let parse = |json: &str| VideoSet::parse("video", &serde_json::from_str(json).unwrap());
    assert_eq!(
        parse(r#""videos/a.bk2""#),
        Ok(VideoSet {
            every: Some("videos/a.bk2".into()),
            ..Default::default()
        })
    );
    assert_eq!(
        parse(r#"{ "f1": "a.bk2", "f1_2026": "b.bk2" }"#),
        Ok(VideoSet {
            every: None,
            f1: Some("a.bk2".into()),
            f1_2026: Some("b.bk2".into())
        })
    );
    assert_eq!(
        parse(r#"{ "f1_2026": "b.bk2" }"#).unwrap().f1_2026.as_deref(),
        Some("b.bk2")
    );
    for (json, why) in [
        ("{}", "video is an object with no f1 or f1_2026 key"),
        (r#"{ "f1": 3 }"#, "video.f1 is not a string"),
        (r#"{ "f1_2026": null }"#, "video.f1_2026 is not a string"),
        (
            r#"{ "f3": "a.bk2" }"#,
            "video.f3 is not a box key; the keys are f1 and f1_2026",
        ),
        ("3", "video is not a string or an object"),
        ("[\"a.bk2\"]", "video is not a string or an object"),
    ] {
        assert_eq!(parse(json), Err(why.to_owned()), "{json}");
    }
    let f2 = parse(r#"{ "f1": "a.bk2", "f2": "c.bk2" }"#).unwrap_err();
    assert!(
        f2.starts_with("video.f2 is not supported: the game names an F2 intro by the track"),
        "{f2}"
    );
    let tt = VideoSet::parse("time_trial_video", &serde_json::json!({ "f2": "c.bk2" })).unwrap_err();
    assert!(tt.starts_with("time_trial_video.f2 is not a box key"), "{tt}");
}

fn pick(s: &VideoSet, slot: Slot) -> Option<(&'static str, &str)> {
    s.candidates(slot).first().copied()
}

#[test]
fn a_box_without_its_own_key_falls_back_to_f1_then_the_string() {
    let set = |every: Option<&str>, f1: Option<&str>, f1_2026: Option<&str>| VideoSet {
        every: every.map(str::to_owned),
        f1: f1.map(str::to_owned),
        f1_2026: f1_2026.map(str::to_owned),
    };
    let all = set(Some("e"), Some("a"), Some("b"));
    assert_eq!(pick(&all, Slot::Intro), Some(("f1", "a")));
    assert_eq!(pick(&all, Slot::Intro26), Some(("f1_2026", "b")));
    let no_26 = set(Some("e"), Some("a"), None);
    assert_eq!(pick(&no_26, Slot::Intro26), Some(("f1", "a")));
    let string_only = set(Some("e"), None, None);
    assert_eq!(pick(&string_only, Slot::Intro), Some(("", "e")));
    assert_eq!(pick(&string_only, Slot::Intro26), Some(("", "e")));
    let no_f1 = set(Some("e"), None, Some("b"));
    assert_eq!(pick(&no_f1, Slot::Intro26), Some(("f1_2026", "b")));
    assert_eq!(pick(&no_f1, Slot::Intro), Some(("", "e")));
    let only_26 = set(None, None, Some("b"));
    assert_eq!(
        pick(&only_26, Slot::Intro),
        Some(("f1_2026", "b")),
        "the row is shared, so the other boxes play the one intro there is"
    );
    assert_eq!(
        pick(&only_26, Slot::Wall),
        None,
        "a Time Trial background of the F1 2026 box alone is not given to the other boxes"
    );
    assert_eq!(pick(&only_26, Slot::Wall26), Some(("f1_2026", "b")));
    assert_eq!(pick(&no_26, Slot::Wall26), Some(("f1", "a")));
}

#[test]
fn each_slot_is_placed_from_the_file_its_fallback_names() {
    let root = scratch("slots");
    let std16 = addon_with_fields(
        &root,
        r#""video": { "f1": "videos/a.bk2", "f1_2026": "videos/b.bk2" }, "time_trial_video": "videos/c.bk2""#,
        &["videos/a.bk2", "videos/b.bk2", "videos/c.bk2"],
    );
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        placed(&videos),
        [
            (Slot::Intro, "video.f1".to_owned(), "a.bk2".to_owned()),
            (Slot::Intro26, "video.f1_2026".to_owned(), "b.bk2".to_owned()),
            (Slot::Wall, "time_trial_video".to_owned(), "c.bk2".to_owned()),
            (Slot::Wall26, "time_trial_video".to_owned(), "c.bk2".to_owned()),
        ]
    );
    let paths: Vec<&str> = videos[0].placed.iter().map(|p| p.game_path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "videos/track_map/swofty/mod.v/ring.bk2",
            "videos/track_map/26/swofty/mod.v/ring.bk2",
            "videos/track_select/ring.bk2",
            "videos/track_select/26/ring.bk2",
        ]
    );
    assert_eq!(
        videos[0].value.as_deref(),
        Some("track_map/swofty/mod.v/ring.bk2")
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_time_trial_background_alone_leaves_the_circuit_intro_row_alone() {
    let root = scratch("wall-only");
    let std16 = addon_with_fields(
        &root,
        r#""time_trial_video": { "f1_2026": "videos/w.bk2" }"#,
        &["videos/w.bk2"],
    );
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(videos.len(), 1);
    assert_eq!(videos[0].value, None, "no TrackVideo row is written for it");
    assert_eq!(
        placed(&videos),
        [(
            Slot::Wall26,
            "time_trial_video.f1_2026".to_owned(),
            "w.bk2".to_owned()
        )]
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_key_with_an_unusable_file_is_dropped_and_the_box_falls_back() {
    let root = scratch("fallback");
    let std16 = addon_with_fields(
        &root,
        r#""video": { "f1": "videos/a.bk2", "f1_2026": "videos/gone.bk2" }"#,
        &["videos/a.bk2"],
    );
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert_eq!(
        placed(&videos),
        [
            (Slot::Intro, "video.f1".to_owned(), "a.bk2".to_owned()),
            (Slot::Intro26, "video.f1".to_owned(), "a.bk2".to_owned()),
        ]
    );
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(
        warnings[0],
        "track mod.v/ring: video.f1_2026 videos/gone.bk2 is not in the addon; it plays its base track's intro"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_video_field_that_can_not_be_read_is_one_warning_and_places_nothing() {
    let root = scratch("unreadable");
    let std16 = addon_with_fields(
        &root,
        r#""video": { "f1": "videos/a.bk2", "f2": "videos/f2.bk2" }, "time_trial_video": 4"#,
        &["videos/a.bk2", "videos/f2.bk2"],
    );
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert!(videos.is_empty(), "{videos:?}");
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings[0].starts_with("track mod.v/ring: video.f2 is not supported")
            && warnings[0].ends_with("; it plays its base track's intro"),
        "{}",
        warnings[0]
    );
    assert_eq!(
        warnings[1],
        "track mod.v/ring: time_trial_video is not a string or an object; it gets no Time Trial background of its own"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_file_two_keys_name_is_checked_once() {
    let root = scratch("once");
    let std16 = addon_with_fields(
        &root,
        r#""video": "videos/bad.mp4", "time_trial_video": { "f1": "videos/bad.mp4", "f1_2026": "videos/bad.mp4" }"#,
        &[],
    );
    let (videos, warnings) = run(&root, &std16, &["ring"]);
    assert!(videos.is_empty());
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("videos/bad.mp4 is not a .bk2 file"),
        "{warnings:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_time_trial_background_is_named_for_the_code_in_the_box_s_video_folder() {
    assert_eq!(wall_path("okayama", false), "videos/track_select/okayama.bk2");
    assert_eq!(wall_path("okayama", true), "videos/track_select/26/okayama.bk2");
    assert_eq!(wall_path("Okayama 2", false), "videos/track_select/okayama_2.bk2");
}
