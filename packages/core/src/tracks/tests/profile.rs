use super::*;

#[test]
fn events_resolve_through_activities_and_packages_name_modded_tracks() {
    let added = [PlanTrack {
        code: "okayama".into(),
        name: Some("Okayama".into()),
        addon: Some("teamsimplified.mods".into()),
    }];
    let tracks = list(&db(), &added);
    let by = |c: &str| tracks.iter().find(|t| t.code == c).unwrap();
    assert_eq!(tracks.len(), 3);
    assert_eq!(by("suzuka").stat_id, 13);
    assert_eq!(by("suzuka").events, ["rw_suzuka", "rw_suzuka_26"]);
    assert!(!by("suzuka").modded);
    assert!(by("suzuka").playable && by("okayama").playable && !by("bahrain24").playable);
    assert!(by("bahrain24").modded && by("bahrain24").name.is_none() && by("bahrain24").events.is_empty());
    let o = by("okayama");
    assert_eq!(
        (o.stat_id, o.modded, o.name.as_deref()),
        (43, true, Some("Okayama"))
    );
    assert_eq!(o.addon.as_deref(), Some("teamsimplified.mods"));
}
