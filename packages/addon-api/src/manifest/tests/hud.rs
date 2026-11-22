use super::*;

#[test]
fn hud_mfd_rows_name_toggle_or_choice_options() {
    let with = |hud: &str| GRID24.replace(r#""operations":"#, &format!(r#""hud": {hud}, "operations":"#));
    let m = AddonManifest::parse(&with(
        r#"{ "mfd": { "title": "LIVERY", "rows": ["team_livery"] } }"#,
    ))
    .unwrap();
    let mfd = m.hud.unwrap().mfd.unwrap();
    assert_eq!(mfd.title.as_deref(), Some("LIVERY"));
    assert_eq!(mfd.rows, ["team_livery"]);
    for (bad, why) in [
        (r#"{ "mfd": { "rows": [] } }"#, "rows must list"),
        (r#"{ "mfd": { "rows": ["missing"] } }"#, "names no option"),
        (
            r#"{ "mfd": { "rows": ["team_livery", "team_livery"] } }"#,
            "listed twice",
        ),
        (
            r#"{ "mfd": { "title": "", "rows": ["team_livery"] } }"#,
            "title must be",
        ),
        (
            r#"{ "mfd": { "rows": ["team_livery"], "page": 2 } }"#,
            "unknown field",
        ),
    ] {
        let e = AddonManifest::parse(&with(bad)).unwrap_err().to_string();
        assert!(e.contains(why), "{bad}: {e}");
    }
    let slider = GRID24
        .replace(
            r#""type": "choice", "default": "titlesponsor","#,
            r#""type": "slider", "default": 1, "min": 0, "max": 2, "step": 1,"#,
        )
        .replace(
            r#""values": ["titlesponsor", { "value": "tier01", "label": "Tier 1" }, "replica"], "#,
            "",
        );
    let e = AddonManifest::parse(&slider.replace(
        r#""operations":"#,
        r#""hud": { "mfd": { "rows": ["team_livery"] } }, "operations":"#,
    ))
    .unwrap_err()
    .to_string();
    assert!(e.contains("only toggle and choice"), "{e}");
}

#[test]
fn hud_pit_rows_take_one_or_two_rows_with_unique_ids_and_short_labels() {
    let with = |hud: &str| GRID24.replace(r#""operations":"#, &format!(r#""hud": {hud}, "operations":"#));
    let m = AddonManifest::parse(&with(
        r#"{ "pit_rows": [{ "id": "serve_penalty", "label": "SERVE PENALTY" }, { "id": "x2", "label": "X" }] }"#,
    ))
    .unwrap();
    let rows = m.hud.unwrap().pit_rows.unwrap();
    assert_eq!(
        rows,
        [
            HudPitRow {
                id: "serve_penalty".into(),
                label: "SERVE PENALTY".into()
            },
            HudPitRow {
                id: "x2".into(),
                label: "X".into()
            }
        ]
    );
    let long_id = "a".repeat(PIT_ROW_ID_MAX + 1);
    let long_label = "A".repeat(PIT_ROW_LABEL_MAX + 1);
    for (bad, why) in [
        (r#"{ "pit_rows": [] }"#.to_owned(), "hud.pit_rows: list 1 to 2 rows"),
        (
            r#"{ "pit_rows": [{ "id": "a", "label": "A" }, { "id": "b", "label": "B" }, { "id": "c", "label": "C" }] }"#
                .to_owned(),
            "hud.pit_rows: list 1 to 2 rows",
        ),
        (
            r#"{ "pit_rows": [{ "id": "Serve", "label": "A" }] }"#.to_owned(),
            "hud.pit_rows: [0].id \"Serve\" must be",
        ),
        (
            r#"{ "pit_rows": [{ "id": "", "label": "A" }] }"#.to_owned(),
            "hud.pit_rows: [0].id \"\" must be",
        ),
        (
            format!(r#"{{ "pit_rows": [{{ "id": "{long_id}", "label": "A" }}] }}"#),
            "must be 1 to 32",
        ),
        (
            r#"{ "pit_rows": [{ "id": "a", "label": "A" }, { "id": "a", "label": "B" }] }"#.to_owned(),
            "hud.pit_rows: [1].id a is listed twice",
        ),
        (
            r#"{ "pit_rows": [{ "id": "a", "label": " " }] }"#.to_owned(),
            "hud.pit_rows: [0].label must be 1 to 24 characters",
        ),
        (
            format!(r#"{{ "pit_rows": [{{ "id": "a", "label": "{long_label}" }}] }}"#),
            "hud.pit_rows: [0].label must be 1 to 24 characters",
        ),
        (
            r#"{ "pit_rows": [{ "id": "a", "label": "A", "default": true }] }"#.to_owned(),
            "unknown field",
        ),
    ] {
        let e = AddonManifest::parse(&with(&bad)).unwrap_err().to_string();
        assert!(e.contains(why), "{bad}: {e}");
    }
    let label = "Ä".repeat(PIT_ROW_LABEL_MAX);
    assert!(
        AddonManifest::parse(&with(&format!(
            r#"{{ "pit_rows": [{{ "id": "a", "label": "{label}" }}] }}"#
        )))
        .is_ok()
    );
}
