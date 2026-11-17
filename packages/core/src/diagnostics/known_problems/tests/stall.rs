use super::*;

fn stuck(rows: &[&str], uptime: u32, names: &[(&str, &str)]) -> Evidence {
    with_names(
        Evidence {
            perf: Some(perf(rows, uptime)),
            ..Default::default()
        },
        names,
    )
}

#[test]
fn reads_the_last_screen_from_the_perf_timeline() {
    assert_eq!(perf_start_screen(&perf(SPLASH, 408)), Some(319.0));
    let menu = [
        SPLASH[2],
        "00:01:45      27019     2404     15        271     189.46  flow #04fc8ffa Exiting; screen playercard  [x]",
        "00:02:12        653       31      0         50      11.96  screen fe_home (current)",
    ];
    assert_eq!(perf_start_screen(&perf(&menu, 408)), None);
    assert_eq!(
        timeline_row(
            "00:04:44    2585578   469557      7       1420          -  screen splash_transition (current)"
        ),
        Some((284.0, "screen splash_transition"))
    );
    assert_eq!(
        timeline_row("at          took ms   frames   slow   worst ms  swofty ms  what"),
        None
    );
}

#[test]
fn a_stall_right_after_the_profile_load_points_at_the_save() {
    let load = "2026-11-04 03:32:34.875 [ 5256] [saves] profile load 1: manager 5 slot 2: gm_my_team id=16 \
                    key=66f051ee751e04d1 career=my_team:66f051ee751e04d1 flags=0x20 name=\"My Team Save 3\"\n";
    let mut e = stuck(SPLASH, 408, &[]);
    e.loader = load.into();
    let d = stalled(&e).unwrap();
    assert_eq!(d.id, "stalled_save");
    assert_eq!(
        d.title,
        "F1 25 stayed on the loading screen after reading your save"
    );
    assert!(d.what.contains("for 5 minutes"), "{}", d.what);
    assert_eq!(d.steps.len(), 2);
    let mut quick = stuck(SPLASH, 120, &[]);
    quick.loader = load.into();
    assert_eq!(stalled(&quick), None, "30 s is a normal load");
    assert_eq!(
        stalled(&stuck(SPLASH, 408, &[])),
        None,
        "no profile load, no save to blame"
    );
    let mut addon = stuck(SPLASH, 408, &[LIVERY]);
    addon.loader = load.into();
    assert_eq!(
        stalled(&addon).unwrap().id,
        "boot_stall",
        "the known addon comes first"
    );
}

#[test]
fn a_boot_stuck_on_loading_names_the_addon_known_to_cause_it() {
    let d = stalled(&stuck(
        SPLASH,
        408,
        &[LIVERY, ("giggles.helmet-exchange", "Helmet Exchange")],
    ))
    .unwrap();
    assert_eq!(d.id, "boot_stall");
    assert_eq!(d.title, "BP Ultimate Livery kept F1 25 on the loading screen");
    assert!(d.what.contains("for 5 minutes"), "{}", d.what);
    assert_eq!(
        d.steps,
        ["Turn off BP Ultimate Livery in your profile, then press Play again."]
    );
    assert_eq!(
        stalled(&stuck(
            SPLASH,
            408,
            &[("giggles.helmet-exchange", "Helmet Exchange")]
        )),
        None
    );
    assert_eq!(
        stalled(&stuck(SPLASH, 120, &[LIVERY])),
        None,
        "30 s is a normal load"
    );
    let mut off = stuck(SPLASH, 408, &[LIVERY]);
    off.enabled = vec!["giggles.helmet-exchange".into()];
    assert_eq!(stalled(&off), None, "installed but off");
    let menu = [
        SPLASH[2],
        "00:05:21        653       31      0         50      11.96  screen fe_home (current)",
    ];
    assert_eq!(stalled(&stuck(&menu, 408, &[LIVERY])), None);
}
