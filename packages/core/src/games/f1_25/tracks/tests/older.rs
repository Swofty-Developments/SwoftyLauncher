use super::*;

#[test]
fn venues_match_a_track_name_never_its_code() {
    let v = Venues::shipped();
    assert!(v.venues.len() >= 33, "{}", v.venues.len());
    assert_eq!(
        normalise("  Autódromo José-Carlos  Pace "),
        "aut dromo jos carlos pace"
    );
    assert_eq!(normalise("Spa-Francorchamps"), "spa francorchamps");
    assert_eq!(v.by_name("Sepang").as_deref(), Some("venue/sepang"));
    assert_eq!(
        v.by_name("SEPANG international circuit").as_deref(),
        Some("venue/sepang")
    );
    assert_eq!(v.by_name("Okayama").as_deref(), Some("venue/okayama"));
    assert_eq!(v.by_name("Delhi").as_deref(), Some("venue/buddh"));
    assert_eq!(v.by_name("Igora Drive").as_deref(), Some("venue/igora_drive"));
    assert_eq!(v.by_name("bahrain24"), None);
    assert_eq!(v.by_name(""), None);
    assert_eq!(v.by_name("Test Ring"), None);
    let twice = Venues::parse(
        r#"{ "version": 1, "venues": {
                "a": { "name": "Ring", "country": "x", "aliases": [], "stock": null },
                "b": { "name": "Other", "country": "x", "aliases": ["ring"], "stock": "b" } } }"#,
    )
    .unwrap();
    assert_eq!(twice.by_name("Ring"), None, "two venues match: none");
    assert_eq!(twice.by_name("other").as_deref(), Some("venue/b"));
    assert!(twice.stock_provides("venue/b") && !twice.stock_provides("venue/a"));
    assert!(!twice.stock_provides("venue/zzz") && !twice.stock_provides("b"));
    assert!(v.stock_provides("venue/albert_park") && !v.stock_provides("venue/sochi"));
}

#[test]
fn older_tracks_name_their_base_the_row_they_fill_their_stat_id_and_venue() {
    let pkg = |code: &str, name: Option<&str>, from: &str, slot: Option<&str>| TrackPackage {
        track_code: code.into(),
        track_name: name.map(str::to_owned),
        version: None,
        author_name: None,
        erp: format!("{code}.erp"),
        activation: crate::content::track_package::Activation {
            copy_from_track: Some(from.into()),
            copy_slot: slot.map(str::to_owned),
            ..Default::default()
        },
        ers: Default::default(),
        copy_directories: Vec::new(),
        ui: None,
        extra: Default::default(),
    };
    let shipped = {
        let mut d = db();
        d.set("tracks._Track_instancecount_", "4", None).unwrap();
        d.records.push(rec("tracks.Track_3._metadata_", "sochi"));
        d.records.push(rec("tracks.Track_3.statId", "30"));
        d
    };
    let mut built = shipped.clone();
    built.set("tracks._Track_instancecount_", "5", None).unwrap();
    built.records.push(rec("tracks.Track_4._metadata_", "okayama2"));
    built.records.push(rec("tracks.Track_4.statId", "44"));
    let okayama = pkg("okayama2", Some("Okayama"), "melbourne", Some("melbourne"));
    let sochi = pkg("sochi", Some("Sochi Autodrom"), "baku", None);
    let unnamed = pkg("ring", None, "zandvoort", Some("zandvoort"));
    let out = older_tracks(
        "Classic Seasons",
        &[
            ("mod.okayama", &okayama),
            ("mod.sochi", &sochi),
            ("mod.ring", &unnamed),
        ],
        &shipped,
        &built,
        Venues::shipped(),
    );
    assert_eq!((out.version, out.profile.as_str()), (1, "Classic Seasons"));
    let entry = |addon: &str,
                 code: &str,
                 name: &str,
                 base: &str,
                 slot: Option<&str>,
                 id: Option<u32>,
                 venue: Option<&str>| OlderTrack {
        addon: addon.into(),
        code: code.into(),
        name: name.into(),
        base: base.into(),
        slot: slot.map(str::to_owned),
        stat_id: id,
        venue: venue.map(str::to_owned),
    };
    assert_eq!(
        out.tracks,
        [
            entry(
                "mod.okayama",
                "okayama2",
                "Okayama",
                "game/melbourne",
                None,
                Some(44),
                Some("venue/okayama")
            ),
            entry(
                "mod.sochi",
                "sochi",
                "Sochi Autodrom",
                "game/baku",
                Some("game/sochi"),
                Some(30),
                Some("venue/sochi")
            ),
            entry("mod.ring", "ring", "ring", "game/zandvoort", None, None, None),
        ]
    );
    let json = serde_json::to_value(&out).unwrap();
    assert_eq!(json["tracks"][2]["slot"], serde_json::Value::Null);
    assert_eq!(json["tracks"][2]["venue"], serde_json::Value::Null);
}
