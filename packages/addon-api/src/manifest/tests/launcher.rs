use super::*;

#[test]
fn parses_the_saves_flag() {
    let with = |saves: &str| {
        GRID24.replace(
            r#""author": "swofty","#,
            &format!(r#""author": "swofty", {saves}"#),
        )
    };
    let m = AddonManifest::parse(GRID24).unwrap();
    assert_eq!(m.saves, Saves::default());
    assert!(!m.saves.affects);
    assert!(serde_json::to_value(&m).unwrap().get("saves").is_none());
    let m = AddonManifest::parse(&with(r#""saves": { "affects": true },"#)).unwrap();
    assert!(m.saves.affects);
    assert_eq!(m.saves.message(), None);
    let m = AddonManifest::parse(&with(
        r#""saves": { "affects": true, "message": "Retire drivers instead of removing the addon." },"#,
    ))
    .unwrap();
    assert!(m.saves.affects);
    assert_eq!(
        m.saves.message(),
        Some("Retire drivers instead of removing the addon.")
    );
    let json = serde_json::to_value(&m).unwrap();
    assert_eq!(json["saves"]["affects"], true);
    assert_eq!(AddonManifest::parse(&json.to_string()).unwrap(), m);
    let m = AddonManifest::parse(&with(r#""saves": {},"#)).unwrap();
    assert!(!m.saves.affects);
    assert_eq!(
        AddonManifest::parse(&with(r#""saves": { "affects": true, "message": " " },"#))
            .unwrap()
            .saves
            .message(),
        None
    );
    for bad in [
        r#""saves": { "affects": true, "extra": 1 },"#,
        r#""saves": { "affects": "yes" },"#,
        r#""saves": true,"#,
    ] {
        assert!(AddonManifest::parse(&with(bad)).is_err(), "{bad}");
    }
}

#[test]
fn a_career_setup_names_a_route_and_options_of_its_addon() {
    let with = |setup: &str| {
        GRID24.replace(
            r#""operations": { "database": ["db/grid24.json"] }"#,
            &format!(r#""operations": {{ "database": ["db/grid24.json"] }}, "career_setup": {setup}"#),
        )
    };
    let m = AddonManifest::parse(&with(
            r#"{ "route": "career_wizard_driver_career_grid", "title": "Grid Career", "options": ["team_livery"] }"#,
        ))
        .unwrap();
    let c = m.career_setup.as_ref().unwrap();
    assert_eq!(c.next, CAREER_SETUP_NEXT);
    assert_eq!(c.screen(), "swofty_setup_career_wizard_driver_career_grid");
    assert_eq!(c.back_route(), "back_career_wizard_driver_career_grid");
    let json = serde_json::to_value(&m).unwrap();
    assert!(json["career_setup"].get("next").is_none());
    assert_eq!(json["career_setup"]["options"][0], "team_livery");
    let m = AddonManifest::parse(&with(
        r#"{ "route": "grid", "options": ["team_livery"], "next": "career_wizard_settings_select" }"#,
    ))
    .unwrap();
    let c = m.career_setup.unwrap();
    assert_eq!(
        (c.title, c.next.as_str()),
        (None, "career_wizard_settings_select")
    );

    let at = |m: &str| format!("invalid manifest: swofty.f1-2026-grid24: career_setup: {m}");
    for (setup, why) in [
        (
            r#"{ "route": "Grid", "options": ["team_livery"] }"#,
            r#"route "Grid" must be [a-z0-9_], at most 48 characters"#.to_owned(),
        ),
        (
            &format!(
                r#"{{ "route": "{}", "options": ["team_livery"] }}"#,
                "r".repeat(49)
            ),
            format!(
                r#"route "{}" must be [a-z0-9_], at most 48 characters"#,
                "r".repeat(49)
            ),
        ),
        (
            r#"{ "route": "grid", "options": [] }"#,
            "options names no option".to_owned(),
        ),
        (
            r#"{ "route": "grid", "options": ["nope"] }"#,
            "option nope is not one of the addon's options".to_owned(),
        ),
        (
            r#"{ "route": "grid", "options": ["team_livery", "team_livery"] }"#,
            "option team_livery is listed twice".to_owned(),
        ),
        (
            r#"{ "route": "grid", "title": " ", "options": ["team_livery"] }"#,
            "title is empty".to_owned(),
        ),
        (
            r#"{ "route": "grid", "options": ["team_livery"], "next": "a-b" }"#,
            r#"next "a-b" must be [a-z0-9_]"#.to_owned(),
        ),
    ] {
        assert_eq!(
            AddonManifest::parse(&with(setup)).unwrap_err().to_string(),
            at(&why),
            "{setup}"
        );
    }
    assert!(
        AddonManifest::parse(&with(
            r#"{ "route": "grid", "options": ["team_livery"], "back": "fe_career" }"#
        ))
        .is_err()
    );
    let hidden = with(r#"{ "route": "grid", "options": ["team_livery"] }"#)
        .replace(r#""binds": "db" }]"#, r#""binds": "db", "hidden": true }]"#);
    let m = AddonManifest::parse(&hidden).unwrap();
    assert!(m.options[0].hidden);
    assert_eq!(m.career_setup.unwrap().options, ["team_livery"]);
}

#[test]
fn a_hub_tab_names_its_template_view_model_and_shown_options() {
    let with = |tab: &str| {
        GRID24.replace(
            r#""operations": { "database": ["db/grid24.json"] }"#,
            &format!(r#""operations": {{ "database": ["db/grid24.json"] }}, "hub_tab": {tab}"#),
        )
    };
    let m = AddonManifest::parse(&with(
        r#"{ "name": "progressive", "title": "Progressive Settings", "options": ["team_livery"],
                 "data_group": "f1_career_progressive" }"#,
    ))
    .unwrap();
    let h = m.hub_tab.as_ref().unwrap();
    assert_eq!(h.template(), "swofty_hub_progressive");
    assert_eq!(h.view_model(), "view_model_data.hud_swofty_hub_progressive");
    assert_eq!(
        serde_json::to_value(&m).unwrap()["hub_tab"]["data_group"],
        "f1_career_progressive"
    );
    let at = |m: &str| format!("invalid manifest: swofty.f1-2026-grid24: hub_tab: {m}");
    let tab = |name: &str, title: &str, options: &str, group: &str| {
        format!(
            r#"{{ "name": "{name}", "title": "{title}", "options": {options}, "data_group": "{group}" }}"#
        )
    };
    for (t, why) in [
        (
            tab("Pro", "T", r#"["team_livery"]"#, "g"),
            r#"name "Pro" must be [a-z0-9_], at most 40 characters"#,
        ),
        (tab("p", " ", r#"["team_livery"]"#, "g"), "title is empty"),
        (
            tab("p", "T", r#"["team_livery"]"#, "a.b"),
            r#"data_group "a.b" must be [a-z0-9_]"#,
        ),
        (tab("p", "T", "[]", "g"), "options must name 1 to 6 options"),
        (
            tab("p", "T", r#"["nope"]"#, "g"),
            "option nope is not one of the addon's options",
        ),
        (
            tab("p", "T", r#"["team_livery", "team_livery"]"#, "g"),
            "option team_livery is listed twice",
        ),
    ] {
        assert_eq!(
            AddonManifest::parse(&with(&t)).unwrap_err().to_string(),
            at(why),
            "{t}"
        );
    }
    let hidden = with(&tab("p", "T", r#"["team_livery"]"#, "g"))
        .replace(r#""binds": "db" }]"#, r#""binds": "db", "hidden": true }]"#);
    let m = AddonManifest::parse(&hidden).unwrap();
    assert!(m.options[0].hidden);
    assert_eq!(m.hub_tab.unwrap().options, ["team_livery"]);
}
