use super::super::slow_init::*;
use super::*;

const OFFLINE_QUIET: &str = "\
2026-11-14 20:30:54.598 [38576] addon offline_quiet.dll: loading D:\\SteamLibrary\\steamapps\\common\\F1 25\\swofty\\addons\\twilight.offline-quiet\\native\\offline_quiet.dll
2026-11-14 20:30:54.600 [38576] addon offline_quiet.dll: loaded at 00007ffb032e0000, calling SwoftyAddonInit (API v9)
2026-11-14 20:30:54.600 [38576] [events] on_frame by twilight.offline-quiet -> ok token 30
2026-11-14 20:30:54.600 [38576] [offline-quiet] settings: EA login hidden, online error hidden, chat restricted hidden
2026-11-14 20:30:59.249 [38576] [offline-quiet] gate at base+0x42f7509 (int3 padding), trampoline at 0000000067B00000
2026-11-14 20:30:59.249 [38576] [offline-quiet] popup hook at base+0x42d9bf0 installed after another hook
2026-11-14 20:30:59.249 [38576] [core] addon offline_quiet.dll: registered 0 mixin(s)
2026-11-14 20:30:59.249 [38576] [perf] step 4649.0 ms: addon init offline_quiet.dll
2026-11-14 20:30:59.249 [38576] [perf] slow addon init 4649 ms: twilight.offline-quiet
2026-11-14 20:30:59.249 [38576] addon offline_quiet.dll: SwoftyAddonInit returned 1 (ok)
";

/// A 1.9.x session's lines around its addon inits, from before the core logged their times.
const OLD_CORE: &str = r#"2026-11-16 17:27:52.790 [10404] addon parts_detach.dll: loading C:\Program Files (x86)\Steam\steamapps\common\F1 25\swofty\addons\vmknolle.parts-detach\native\parts_detach.dll
2026-11-16 17:27:52.792 [10404] addon parts_detach.dll: loaded at 00007ff8ea440000, calling SwoftyAddonInit (API v9)
2026-11-16 17:27:52.792 [10404] [core] addon parts_detach.dll: registered 3 mixin(s)
2026-11-16 17:27:52.792 [10404] addon parts_detach.dll: SwoftyAddonInit returned 1 (ok)
2026-11-16 17:27:52.792 [10404] addon offline_quiet.dll: loading C:\Program Files (x86)\Steam\steamapps\common\F1 25\swofty\addons\twilight.offline-quiet\native\offline_quiet.dll
2026-11-16 17:27:52.798 [10404] addon offline_quiet.dll: loaded at 00007ff92e9b0000, calling SwoftyAddonInit (API v9)
2026-11-16 17:27:52.798 [10404] scan .xtls [48 B9 62 65 74 61 5F 6D 6F 74 48 89 4C 24 58 66 C7 44 24 60 64 00 48 8D 0D ?? ?? ?? ?? 48 89 4C 24 40 C5 F9 57 C0 C5 F9 29 44 24 30 C5 F9 11 44 24 20 48 8D 9C 24 E0 00 00 00 4C 8D 44 24 40 4C 8D 4C 24 30 48 89 C1 48 89 DA E8 ?? ?? ?? ??] -> 1 hit at 0x1436d23ca (base+0x36d23ca), the address cached for this exe
2026-11-16 17:27:52.798 [10404] [events] on_frame by twilight.offline-quiet -> ok token 50
2026-11-16 17:27:52.798 [10404] [offline-quiet] settings: EA login hidden, online error hidden, chat restricted hidden
[offline] module: team_orders.dll @ 0x7ff915bb0000
[offline] module: kizi_pitlane.dll @ 0x7ff9164e0000
[offline] module: cooldown_lap.dll @ 0x7ff8fcd30000
[offline] module: race_start.dll @ 0x7ff916480000
[offline] module: grid_start_2026.dll @ 0x7ff90cdc0000
[offline] module: halo_hud.dll @ 0x7ff8fcca0000
[offline] module: race_broadcast.dll @ 0x7ff8f7190000
[offline] module: ffb_overhaul.dll @ 0x7ff8fc9c0000
[offline] module: auto_save_race_replay.dll @ 0x7ff96d750000
[offline] module: kizi_no_auto_reset.dll @ 0x7ff9150c0000
[offline] module: realistic_rain.dll @ 0x7ff8ea4f0000
[offline] module: cinecam.dll @ 0x7ff8edd10000
[offline] module: rnd_departments.dll @ 0x7ff8f2ba0000
[offline] module: dynamic_events.dll @ 0x7ff8ea490000
[offline] module: parts_detach.dll @ 0x7ff8ea440000
[offline] module: offline_quiet.dll @ 0x7ff92e9b0000
[offline] module: wheel_failure.dll @ 0x7ff926000000
2026-11-16 17:28:07.783 [10404] [offline-quiet] gate at base+0x42f7509 (int3 padding), trampoline at 000000006BC90000
2026-11-16 17:28:07.783 [10404] [offline-quiet] popup hook at base+0x42d9bf0 installed after another hook
2026-11-16 17:28:07.783 [10404] [core] addon offline_quiet.dll: registered 0 mixin(s)
2026-11-16 17:28:07.783 [10404] addon offline_quiet.dll: SwoftyAddonInit returned 1 (ok)
"#;

const QUIET: (&str, &str) = ("twilight.offline-quiet", "Offline Quiet");

fn slow(loader: &str, names: &[(&str, &str)]) -> Evidence {
    with_names(ev(loader, &[]), names)
}

fn timed(ms: u64, addon: &str) -> String {
    OFFLINE_QUIET
        .replace("4649 ms: twilight.offline-quiet", &format!("{ms} ms: {addon}"))
        .replace('\n', "\r\n")
}

#[test]
fn reads_each_addons_init_time_from_the_perf_lines() {
    assert_eq!(
        slow_inits(OFFLINE_QUIET),
        [("twilight.offline-quiet".to_owned(), 4649)]
    );
    let again = format!(
        "{OFFLINE_QUIET}2026-11-14 20:31:02.100 [38576] [perf] slow addon init 1203 ms: twilight.offline-quiet\n"
    );
    assert_eq!(slow_inits(&again), [("twilight.offline-quiet".to_owned(), 5852)]);
    assert_eq!(
        slow_inits(&timed(4649, "offline_quiet.dll")),
        [("offline_quiet.dll".to_owned(), 4649)]
    );
}

#[test]
fn an_init_under_five_seconds_is_not_named() {
    assert_eq!(slow_addon_init(&slow(OFFLINE_QUIET, &[QUIET])), None);
}

#[test]
fn an_addon_that_held_up_the_start_is_named_with_its_time() {
    let again = format!(
        "{OFFLINE_QUIET}2026-11-14 20:31:02.100 [38576] [perf] slow addon init 1203 ms: twilight.offline-quiet\n"
    );
    let d = slow_addon_init(&slow(&again, &[QUIET])).unwrap();
    assert_eq!(d.id, "slow_addon_init");
    assert_eq!(d.title, "Offline Quiet held up F1 25's start by 6 seconds");
    assert_eq!(
        d.what,
        "Offline Quiet took 6 seconds to start up while F1 25 loaded, and F1 25 waited for it before it went on. \
         The other addons in your profile started in moments."
    );
    assert_eq!(
        d.steps,
        [
            "If F1 25 takes too long to start, turn off Offline Quiet in your profile and press Play again to compare.",
            "Check for an update to Offline Quiet on the Addons page.",
        ]
    );
    let long = slow_addon_init(&slow(&timed(150_000, "twilight.offline-quiet"), &[QUIET])).unwrap();
    assert_eq!(long.title, "Offline Quiet held up F1 25's start by 3 minutes");
}

#[test]
fn a_dll_name_resolves_to_the_addon_that_ships_it() {
    let mut e = slow(&timed(10_012, "Offline_Quiet.DLL"), &[QUIET]);
    e.plugins
        .insert("offline_quiet.dll".into(), "twilight.offline-quiet".into());
    assert!(
        slow_addon_init(&e)
            .unwrap()
            .title
            .starts_with("Offline Quiet held up")
    );
    e.plugins.clear();
    let d = slow_addon_init(&e).unwrap();
    assert!(
        d.title.starts_with("Offline Quiet held up"),
        "from the loading line: {}",
        d.title
    );
    let unknown = slow(&timed(10_012, "mes_upgrade_pool.dll"), &[]);
    assert_eq!(
        slow_addon_init(&unknown).unwrap().title,
        "the plugin mes_upgrade_pool.dll held up F1 25's start by 10 seconds"
    );
}

#[test]
fn the_launchers_own_inits_are_left_out() {
    assert_eq!(
        slow_addon_init(&slow(&timed(12_000, "swofty_core.dll"), &[])),
        None
    );
    let mut e = slow(&timed(12_000, "twilight.offline-quiet"), &[QUIET]);
    e.builtin.insert("twilight.offline-quiet".into());
    assert_eq!(slow_addon_init(&e), None);
}

#[test]
fn several_slow_addons_name_the_slowest_and_list_the_rest() {
    let loader = format!(
        "{}2026-11-14 20:31:10.000 [38576] [perf] slow addon init 11021 ms: mes.upgrade-pool\n\
         2026-11-14 20:31:21.000 [38576] [perf] slow addon init 6402 ms: mtp.owner-names\n\
         2026-11-14 20:31:22.000 [38576] [perf] slow addon init 1450 ms: podium.anthems\n",
        timed(10_012, "twilight.offline-quiet")
    );
    let names = [
        QUIET,
        ("mes.upgrade-pool", "Upgrade Pool"),
        ("mtp.owner-names", "Owner Names"),
    ];
    let d = slow_addon_init(&slow(&loader, &names)).unwrap();
    assert_eq!(d.title, "Upgrade Pool held up F1 25's start by 11 seconds");
    assert!(
        d.what
            .ends_with("before it went on. Offline Quiet and Owner Names were also slow to start."),
        "{}",
        d.what
    );
    let two = format!(
        "{}2026-11-14 20:31:10.000 [38576] [perf] slow addon init 11021 ms: mes.upgrade-pool\n",
        timed(10_012, "twilight.offline-quiet")
    );
    assert!(
        slow_addon_init(&slow(&two, &names))
            .unwrap()
            .what
            .ends_with("Offline Quiet was also slow to start.")
    );
}

#[test]
fn a_stall_no_rule_explains_names_the_slow_addon() {
    let mut e = slow(&timed(10_012, "twilight.offline-quiet"), &[QUIET]);
    e.perf = Some(perf(SPLASH, 408));
    assert_eq!(stalled(&e).unwrap().id, "slow_addon_init");
    e.loader.push_str(
        "2026-11-04 03:32:34.875 [ 5256] [saves] profile load 1: manager 5 slot 2: gm_my_team id=16 \
         key=66f051ee751e04d1 career=my_team:66f051ee751e04d1 flags=0x20 name=\"My Team Save 3\"\n",
    );
    assert_eq!(
        stalled(&e).unwrap().id,
        "stalled_save",
        "the save rule comes first"
    );
}

#[test]
fn an_older_core_s_init_time_is_read_from_the_loader_lines_around_it() {
    assert_eq!(
        timed_inits(OLD_CORE),
        [
            ("parts_detach.dll".to_owned(), 0),
            ("offline_quiet.dll".to_owned(), 14985)
        ]
    );
    let d = slow_addon_init(&slow(OLD_CORE, &[QUIET])).unwrap();
    assert_eq!(d.title, "Offline Quiet held up F1 25's start by 15 seconds");
    assert!(
        d.what
            .ends_with("The other addons in your profile started in moments."),
        "{}",
        d.what
    );
}

#[test]
fn the_perf_lines_win_over_the_loader_lines_when_a_core_logs_both() {
    let both = format!("{OLD_CORE}{OFFLINE_QUIET}");
    assert_eq!(addon_inits(&both), [("twilight.offline-quiet".to_owned(), 4649)]);
    assert_eq!(slow_addon_init(&slow(&both, &[QUIET])), None);
}

#[test]
fn an_init_across_midnight_and_on_another_thread_is_timed_on_its_own_thread() {
    let lines = "\
2026-11-16 23:59:58.000 [ 7] addon late.dll: loaded at 00007ff92e9b0000, calling SwoftyAddonInit (API v9)
2026-11-16 23:59:59.000 [ 8] addon late.dll: SwoftyAddonInit returned 1 (ok)
2026-11-17 00:00:04.500 [ 7] addon late.dll: SwoftyAddonInit returned 1 (ok)
";
    assert_eq!(timed_inits(lines), [("late.dll".to_owned(), 6500)]);
}
