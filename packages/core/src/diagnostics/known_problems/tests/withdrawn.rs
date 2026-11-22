use super::*;

const BMW: (&str, &str) = ("tom.bmw-2026-testing", "BMW 2026 Testing");

fn on(mut e: Evidence, withdrawn: &[(&str, &str)]) -> Evidence {
    e.withdrawn = withdrawn
        .iter()
        .map(|(id, v)| ((*id).to_owned(), (*v).to_owned()))
        .collect();
    e
}

fn stuck(uptime: u32, names: &[(&str, &str)]) -> Evidence {
    with_names(
        Evidence {
            perf: Some(perf(SPLASH, uptime)),
            ..Default::default()
        },
        names,
    )
}

#[test]
fn a_stall_with_a_withdrawn_addon_on_names_it() {
    let e = on(stuck(408, &[BMW]), &[(BMW.0, "1.0.7")]);
    let d = stalled(&e).unwrap();
    assert_eq!(d.id, "withdrawn_addon_stall");
    assert_eq!(d.title, "BMW 2026 Testing 1.0.7 was withdrawn: turn it off");
    assert_eq!(
        d.what,
        "F1 25 stayed on Loading... after Press any button for 5 minutes and never reached the main menu. \
         BMW 2026 Testing 1.0.7 is on, and was withdrawn from the Addon Browser."
    );
    assert_eq!(
        d.steps,
        ["Turn off BMW 2026 Testing in your profile, then press Play again."]
    );
    assert_eq!(
        stalled(&on(stuck(120, &[BMW]), &[(BMW.0, "1.0.7")])),
        None,
        "30 s is a normal load"
    );
    assert_eq!(stalled(&stuck(408, &[BMW])), None, "not withdrawn");
}

#[test]
fn a_withdrawn_addon_comes_before_the_save() {
    let mut e = on(stuck(408, &[BMW]), &[(BMW.0, "1.0.7")]);
    e.loader = "2026-11-22 18:01:54.000 [ 5256] [saves] profile load 1: manager 5 slot 2\n".into();
    assert_eq!(stalled(&e).unwrap().id, "withdrawn_addon_stall");
}

#[test]
fn a_known_stall_also_names_the_withdrawn_addons() {
    let e = on(
        stuck(408, &[LIVERY, BMW, ("a.old", "Old Cars")]),
        &[(BMW.0, "1.0.7"), ("a.old", "2.1.0")],
    );
    let d = stalled(&e).unwrap();
    assert_eq!(d.id, "boot_stall");
    assert_eq!(
        d.steps.last().unwrap(),
        "BMW 2026 Testing 1.0.7 and Old Cars 2.1.0 were withdrawn from the Addon Browser: turn them off."
    );
}

#[test]
fn a_crash_no_rule_recognises_names_the_withdrawn_addons() {
    let mut e = on(
        ev(&crash_in("F1_25.exe"), &["F1_25.exe"]),
        &[(BMW.0, "1.0.7"), ("a.old", "2.1.0")],
    );
    e.names = [BMW, ("a.old", "Old Cars")]
        .iter()
        .map(|(id, n)| ((*id).to_owned(), (*n).to_owned()))
        .collect();
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, "withdrawn_addon");
    assert_eq!(d.title, "2 addons that are on were withdrawn: turn them off");
    assert_eq!(
        d.what,
        "F1 25 crashed while BMW 2026 Testing 1.0.7 and Old Cars 2.1.0 were on. Those versions were withdrawn \
         from the Addon Browser."
    );
    assert_eq!(
        d.steps,
        ["Turn off BMW 2026 Testing and Old Cars in your profile, then press Play again."]
    );
    e.withdrawn.truncate(1);
    let d = diagnose(&e).unwrap();
    assert_eq!(d.title, "BMW 2026 Testing 1.0.7 was withdrawn: turn it off");
    assert_eq!(
        d.what,
        "F1 25 crashed while BMW 2026 Testing 1.0.7 was on. That version was withdrawn from the Addon Browser."
    );
}

#[test]
fn a_crash_a_rule_recognises_ends_with_the_withdrawn_addon() {
    let plain = Evidence {
        code: Some(APP_CONTROL),
        ..Default::default()
    };
    let before = diagnose(&plain).unwrap();
    let mut e = on(plain, &[(BMW.0, "1.0.7")]);
    e.names = [BMW]
        .iter()
        .map(|(id, n)| ((*id).to_owned(), (*n).to_owned()))
        .collect();
    let d = diagnose(&e).unwrap();
    assert_eq!(d.id, before.id);
    assert_eq!(d.steps.len(), before.steps.len() + 1);
    assert_eq!(
        d.steps.last().unwrap(),
        "BMW 2026 Testing 1.0.7 was withdrawn from the Addon Browser: turn it off."
    );
}

#[test]
fn the_evidence_reads_the_saved_withdrawn_versions_of_the_addons_that_are_on() {
    use crate::catalog::withdrawn::{Check, Checks};
    use crate::profile::{Profile, ProfileAddon};
    let root = std::env::temp_dir().join(format!("swofty-kp-withdrawn-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let store = crate::store::Store::new(&root);
    let installed: BTreeMap<String, AddonManifest> =
        [(BMW.0, "1.0.7"), ("a.off", "1.0.0"), ("a.pinned", "3.0.0")]
            .iter()
            .map(|(id, v)| {
                let m: AddonManifest = serde_json::from_value(serde_json::json!({
                    "schema_version": 1, "id": id, "name": id, "version": v, "author": "a",
                    "compat": { "game": "f1_25" }
                }))
                .unwrap();
                ((*id).to_owned(), m)
            })
            .collect();
    let withdrawn = |v: &str| Check {
        version: semver::Version::parse(v).unwrap(),
        withdrawn: true,
        checked: 1,
    };
    let builtin: BTreeSet<String> = ["a.pinned".to_owned()].into();
    let built = vec![BMW.0.to_owned(), "a.pinned".to_owned()];
    assert!(withdrawn_on(&store, "p", &built, &builtin, &installed).is_empty());
    Checks(
        [
            (BMW.0.to_owned(), withdrawn("1.0.7")),
            ("a.off".to_owned(), withdrawn("1.0.0")),
            ("a.pinned".to_owned(), withdrawn("3.0.0")),
        ]
        .into(),
    )
    .save(&root)
    .unwrap();
    assert_eq!(
        withdrawn_on(&store, "p", &built, &builtin, &installed),
        [(BMW.0.to_owned(), "1.0.7".to_owned())]
    );
    let mut p = Profile::new("p");
    for (id, enabled) in [("a.off", false), (BMW.0, true)] {
        p.addons.push(ProfileAddon {
            id: id.into(),
            enabled,
            unknown: Default::default(),
        });
    }
    store.save_profile(&p).unwrap();
    assert_eq!(
        withdrawn_on(&store, "p", &[], &builtin, &installed),
        [(BMW.0.to_owned(), "1.0.7".to_owned())],
        "no build yet: the profile's own list"
    );
    std::fs::remove_dir_all(&root).ok();
}
