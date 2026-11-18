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
