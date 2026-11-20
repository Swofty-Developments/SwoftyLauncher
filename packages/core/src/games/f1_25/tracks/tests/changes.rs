use super::*;

#[test]
fn track_changes_survive_ops_that_carry_shipped_values() {
    let list = |path: &str, v: &str| Record::new(path, 13, v);
    let int = |path: &str, v: &str| Record::new(path, 0, v);
    let shipped = Gvdb::new(vec![
        list("sets.a.tracks", "melbourne,suzuka"),
        list("sets.b.tracks", "melbourne"),
        list("sets.c.tracks", "melbourne"),
        int("tracks._Track_instancecount_", "2"),
        int("tracks.Track_0.statId", "5"),
    ]);
    let mut db = Gvdb::new(vec![
        list("sets.a.tracks", "melbourne,suzuka,testring"),
        list("sets.b.tracks", "melbourne,testring"),
        list("sets.c.tracks", "melbourne,testring"),
        int("tracks._Track_instancecount_", "3"),
        int("tracks.Track_0.statId", "6"),
        int("tracks.Track_2.statId", "50"),
    ]);
    let changed = changed_records(&db, &shipped);
    assert_eq!(changed.len(), 5);
    db.set("sets.a.tracks", "melbourne,suzuka", None).unwrap();
    db.set("sets.b.tracks", "suzuka,imola", None).unwrap();
    db.set("tracks._Track_instancecount_", "2", None).unwrap();
    db.set("tracks.Track_0.statId", "9", None).unwrap();
    let kept = keep_changes(&mut db, &shipped, &changed);
    assert_eq!(kept.len(), 3);
    assert_eq!(
        db.get_text("sets.a.tracks").as_deref(),
        Some("melbourne,suzuka,testring")
    );
    assert_eq!(
        db.get_text("sets.b.tracks").as_deref(),
        Some("suzuka,imola,testring")
    );
    assert_eq!(
        db.get_text("sets.c.tracks").as_deref(),
        Some("melbourne,testring")
    );
    assert_eq!(db.get_text("tracks._Track_instancecount_").as_deref(), Some("3"));
    assert_eq!(db.get_text("tracks.Track_0.statId").as_deref(), Some("9"));
}

#[test]
fn a_track_change_held_twice_is_put_back_in_the_record_the_database_reads() {
    let list = |path: &str, v: &str| Record::new(path, 13, v);
    let path = "sets.a.tracks";
    let shipped = Gvdb::new(vec![list(path, "melbourne")]);
    let ours = list(path, "melbourne,testring");
    let mut db = Gvdb::new(vec![ours.clone(), ours.clone()]);
    db.set(path, "melbourne", None).unwrap();
    assert_eq!(keep_changes(&mut db, &shipped, &[ours]), [fnv1a64(path)]);
    assert_eq!(db.get_text(path).as_deref(), Some("melbourne,testring"));
}
