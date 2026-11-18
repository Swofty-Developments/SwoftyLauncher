use super::*;

#[test]
fn an_op_on_a_row_a_later_addon_adds_applies_after_that_addon() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let grid = Origin {
        addon: "simplified".into(),
        item: Some("grid-editor".into()),
    };
    let mut p = Planner::new(&features, &opt_in);
    list_fields(&mut p, "cs");
    for json in [
        set("cs_f1_2016", "name", "edited"),
        set("cs_f1_2016", "colour", "grid"),
        set("f1", "name", "grid"),
    ] {
        p.push(Op {
            op: serde_json::from_str(&json).unwrap(),
            origin: grid.clone(),
        });
    }
    p.push(op("cs", &add("cs_f1_2016")));
    p.push(op("cs", &set("f1", "name", "cs")));
    p.push(op("cs", &set("cs_f1_2016", "name", "cs")));
    p.push(op("late", &set("cs_f1_2016", "colour", "late")));
    let mut db = lists();
    p.apply_db(&mut db).unwrap();
    assert_eq!(db.get_text("t.list_1._metadata_").as_deref(), Some("cs_f1_2016"));
    assert_eq!(db.get_text("t.list_1.name").as_deref(), Some("cs"));
    assert_eq!(db.get_text("t.list_1.colour").as_deref(), Some("late"));
    assert_eq!(db.get_text("t.list_0.name").as_deref(), Some("cs"));
    assert!(p.take_skipped().is_empty());
    assert_eq!(
        p.take_waits(),
        BTreeSet::from([("simplified".to_owned(), "cs".to_owned())])
    );
    let key = |path: &str| Target::Record(swofty_formats::hash::fnv1a64(path));
    let winner = |p: &Planned, path: &str| {
        let c = p.conflicts.iter().find(|c| c.target == key(path)).unwrap();
        (
            c.winner.to_string(),
            c.overridden.iter().map(Origin::to_string).collect::<Vec<_>>(),
        )
    };
    let planned = p.finish();
    assert_eq!(
        winner(&planned, "t.list_0.name"),
        ("cs".to_owned(), vec!["simplified (grid-editor)".to_owned()])
    );
    assert_eq!(
        winner(&planned, "t.list_1.name"),
        ("cs".to_owned(), vec!["simplified (grid-editor)".to_owned()])
    );
    assert_eq!(
        winner(&planned, "t.list_1.colour"),
        ("late".to_owned(), vec!["simplified (grid-editor)".to_owned()])
    );
}

#[test]
fn an_op_that_waits_never_overwrites_what_a_later_addon_wrote() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in);
    list_fields(&mut p, "cs");
    p.push(op("early", &set("cs_f1_2016", "name", "early")));
    p.push(op("early", &set("cs_f1_2016", "colour", "early")));
    p.push(op("cs", &add("cs_f1_2016")));
    p.push(op("cs", &set("cs_f1_2016", "name", "cs")));
    let mut db = lists();
    p.apply_db(&mut db).unwrap();
    assert_eq!(db.get_text("t.list_1.name").as_deref(), Some("cs"));
    assert_eq!(db.get_text("t.list_1.colour").as_deref(), Some("early"));
    let key = Target::Record(swofty_formats::hash::fnv1a64("t.list_1.name"));
    let planned = p.finish();
    let c = planned.conflicts.iter().find(|c| c.target == key).unwrap();
    assert_eq!(c.winner.to_string(), "cs");
    assert!(c.overridden.iter().any(|o| o.addon == "early"));
}

#[test]
fn a_waiting_op_loses_every_record_a_later_addon_wrote_and_keeps_the_rest() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let mut p = Planner::new(&features, &opt_in);
    list_fields(&mut p, "cs");
    p.push(op("early", &set("cs_f1_2016", "name", "early")));
    p.push(op("early", &set("f1", "name", "early")));
    p.push(op("cs", &add("cs_f1_2016")));
    p.push(op("cs", &set("f1", "name", "cs")));
    let mut db = lists();
    p.apply_db(&mut db).unwrap();
    assert_eq!(db.get_text("t.list_0.name").as_deref(), Some("cs"));
    assert_eq!(db.get_text("t.list_1.name").as_deref(), Some("early"));
}

#[test]
fn an_op_on_a_row_the_core_adds_at_the_start_is_skipped_with_a_warning() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let grid = Origin {
        addon: "simplified".into(),
        item: Some("grid-editor".into()),
    };
    let mut p = Planner::new(&features, &opt_in);
    list_fields(&mut p, "cs");
    p.add_core_rows("cs", ["t.list".to_owned()]);
    for json in [set("cs_f1_2020", "name", "edited"), set("f1", "name", "grid")] {
        p.push(Op {
            op: serde_json::from_str(&json).unwrap(),
            origin: grid.clone(),
        });
    }
    let mut db = lists();
    p.apply_db(&mut db).unwrap();
    assert_eq!(db.get_text("t.list_0.name").as_deref(), Some("grid"));
    assert_eq!(
        p.take_skipped(),
        [
            "simplified (grid-editor): change skipped: t.list[_metadata_=cs_f1_2020]: no row matches; the core adds that row from cs's records when the game starts, after the build"
        ]
    );

    let mut p = Planner::new(&features, &opt_in);
    list_fields(&mut p, "cs");
    p.add_core_rows("cs", ["t.other".to_owned(), "t.lis".to_owned()]);
    p.push(op("simplified", &set("cs_f1_2020", "name", "edited")));
    let e = p.apply_db(&mut lists()).unwrap_err();
    assert!(e.to_string().contains("no row matches, even after"), "{e}");
}

#[test]
fn an_op_on_a_row_no_addon_adds_is_skipped_or_fails_as_before() {
    let features = Features::default();
    let opt_in = BTreeSet::new();
    let lenient = BTreeMap::from([("edits".to_owned(), "My DB Edits".to_owned())]);
    let mut p = Planner::new(&features, &opt_in).lenient(lenient);
    list_fields(&mut p, "cs");
    p.push(op("edits", &set("gone", "name", "x")));
    p.push(op("edits", &set("f1", "name", "edits")));
    p.push(op("cs", &add("cs_f1_2016")));
    let mut db = lists();
    p.apply_db(&mut db).unwrap();
    assert_eq!(db.get_text("t.list_0.name").as_deref(), Some("edits"));
    assert_eq!(
        p.take_skipped(),
        [
            "My DB Edits: change skipped: t.list[_metadata_=gone]: no row matches, even after every addon's changes"
        ]
    );

    let mut p = Planner::new(&features, &opt_in);
    list_fields(&mut p, "cs");
    p.push(op("a", &set("gone", "name", "x")));
    p.push(op("cs", &add("cs_f1_2016")));
    let e = p.apply_db(&mut lists()).unwrap_err();
    assert_eq!(
        e.to_string(),
        "applying a: t.list[_metadata_=gone]: no row matches, even after every addon's changes"
    );

    let mut p = Planner::new(&features, &opt_in);
    list_fields(&mut p, "cs");
    p.push(op("a", &set("cs_f1_2016", "missing", "x")));
    p.push(op("cs", &add("cs_f1_2016")));
    let e = p.apply_db(&mut lists()).unwrap_err();
    assert_eq!(
        e.to_string(),
        "applying a: t.list[_metadata_=cs_f1_2016].missing: no such key"
    );
}
