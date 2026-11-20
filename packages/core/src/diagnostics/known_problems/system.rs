use crate::diagnostics::launch::LOW_DISK;

use super::Diagnosis;
use super::crash_record::Fatal;
use super::evidence::Evidence;

/// The exit code of a game Windows stopped from loading a file it doesn't
/// trust (STATUS_SYSTEM_INTEGRITY_POLICY_VIOLATION).
pub(super) const APP_CONTROL: u32 = 0xc0e9_0002;

pub(super) fn app_control() -> Diagnosis {
    Diagnosis {
        id: "app_control",
        title: "Windows blocked Swofty's files from loading".to_owned(),
        what: "Windows stopped F1 25 from loading Swofty's version.dll (error 0xc0e90002, Bad Image), so the game \
               closed before it started. Smart App Control or an app control policy does this to files it doesn't \
               know."
            .to_owned(),
        steps: vec![
            "Turn off Smart App Control: open Windows Security, App & browser control, Smart App Control settings, \
             and choose Off."
                .to_owned(),
            "On a work or school PC, an app control policy from your organisation does this instead. Ask whoever \
             manages the PC to allow F1 25's folder."
                .to_owned(),
            "Press Play again.".to_owned(),
        ],
    }
}

/// F1 25's Insufficient Storage dialog: the drive it names, and the MiB free
/// and needed there.
pub(super) fn low_disk(loader: &str) -> Option<(String, u64, u64)> {
    loader.lines().rev().find_map(|l| {
        let rest = &l[l.find(LOW_DISK)? + LOW_DISK.len()..];
        let (free, rest) = rest.split_once('/')?;
        let (need, rest) = rest.split_once(" MiB available on ")?;
        let drive = rest
            .trim_start_matches(['\'', '"'])
            .get(..2)
            .filter(|d| d.ends_with(':') && d.starts_with(|c: char| c.is_ascii_alphabetic()))
            .map_or_else(
                || "the drive F1 25 saves to".to_owned(),
                |d| format!("drive {}", d[..1].to_ascii_uppercase()),
            );
        Some((drive, free.trim().parse().ok()?, need.trim().parse().ok()?))
    })
}

pub(super) fn gb(mib: u64, round: fn(f64) -> f64) -> String {
    let tenths = round(mib as f64 / 102.4);
    if tenths % 10.0 == 0.0 {
        format!("{:.0} GB", tenths / 10.0)
    } else {
        format!("{:.1} GB", tenths / 10.0)
    }
}

pub(super) fn low_disk_space(drive: &str, free: u64, need: u64) -> Diagnosis {
    let mut what = format!(
        "F1 25 needs {} free on {drive} and found {}, so it showed Insufficient Storage and closed.",
        gb(need, f64::round),
        gb(free, f64::floor)
    );
    if drive == "drive C" {
        what.push_str(" It checks drive C even when the game is installed on another drive.");
    }
    Diagnosis {
        id: "low_disk",
        title: format!("F1 25 needs more free space on {drive}"),
        what,
        steps: vec![
            format!(
                "Free up at least {} on {drive}: empty the Recycle Bin, and delete temporary files in Windows \
                 Settings, System, Storage, Temporary files.",
                gb(need.saturating_sub(free).max(1), f64::ceil)
            ),
            "Press Play again.".to_owned(),
        ],
    }
}

/// The core's `[hwsettings]` line about the player's hardware_settings_config.xml, from the loader.log.
pub(super) fn hardware_settings_line(loader: &str) -> Option<&str> {
    loader
        .lines()
        .find_map(|l| l.find("[hwsettings] ").map(|at| &l[at..]))
}

/// Whether the game aborted in its startup sequence, right after reading its hardware settings file, before
/// any window, with the core's line saying that file is a OneDrive file or cloud-only.
pub(super) fn hardware_settings_crash(e: &Evidence, f: &Fatal) -> bool {
    let at_abort = f
        .frames
        .iter()
        .any(|l| l.contains("Game_Startup_InitSequence+0x291"));
    let line = hardware_settings_line(&e.loader);
    at_abort
        && e.loader.contains("before the game's window")
        && line.is_some_and(|l| l.contains("cloud-only=yes") || l.contains("onedrive=yes"))
}

pub(super) fn hardware_settings(e: &Evidence) -> Diagnosis {
    let cloud = hardware_settings_line(&e.loader).is_some_and(|l| l.contains("cloud-only=yes"));
    let what = if cloud {
        "F1 25 closed while it started up because it could not read its graphics settings file, \
         hardware_settings_config.xml in Documents\\My Games\\F1 25\\hardwaresettings. OneDrive has that file in \
         the cloud only, not on your PC, and the game can't download it."
    } else {
        "F1 25 closed while it started up because it could not read its graphics settings file, \
         hardware_settings_config.xml in Documents\\My Games\\F1 25\\hardwaresettings. Your Documents folder \
         is in OneDrive, which can leave that file unreadable while it syncs or after it is moved to the cloud."
    };
    Diagnosis {
        id: "hardware_settings_unreadable",
        title: "F1 25 couldn't read its graphics settings file".to_owned(),
        what: what.to_owned(),
        steps: vec![
            "In File Explorer, right-click Documents\\My Games\\F1 25\\hardwaresettings, choose \"Always keep on this device\", and wait for OneDrive to finish syncing, then press Play again."
                .to_owned(),
            "If it still closes, quit OneDrive, rename hardware_settings_config.xml in that folder to hardware_settings_config.xml.bak (F1 25 makes a new one, with its graphics settings back at their defaults), and press Play again."
                .to_owned(),
            "If it still closes, move your Documents folder out of OneDrive: OneDrive settings, Sync and backup, Manage backup, turn Documents off."
                .to_owned(),
        ],
    }
}

pub(super) const QUIT_SECS: f64 = 10.0;

/// The flow graph's root state, StateApp, as the black box prints it: by its name hash, or by name. It exits only
/// when the game itself closes; every other state (a session, a screen's flow) exits while the game keeps running.
pub(super) const APP_FLOW: [&str; 2] = ["#e11edb87", "StateApp"];

/// Whether the crash report's black box shows the game itself was closing: its root flow state had just exited,
/// with a first-chance access violation in the game's own code just before the process ended.
pub(super) fn quit_fault(e: &Evidence) -> bool {
    let Some(report) = &e.report else {
        return false;
    };
    let secs_before = |line: &str| -> Option<f64> {
        let rest = &line[line.find('(')? + 1..];
        rest.split(" s before").next()?.parse().ok()
    };
    let state = |key: &str| report.lines().find(|l| l.trim_start().starts_with(key));
    let exiting = state("last flow").is_some_and(|l| {
        let mut words = l.split_whitespace().skip(2);
        words.next().is_some_and(|flow| APP_FLOW.contains(&flow))
            && words.next() == Some("Exiting")
            && secs_before(l).is_some_and(|s| s <= QUIT_SECS)
    });
    let violation = state("last exception")
        .is_some_and(|l| l.contains("ACCESS_VIOLATION") && l.to_ascii_lowercase().contains(" at f1_25.exe+"));
    exiting && violation
}

pub(super) fn quit_fault_diagnosis() -> Diagnosis {
    Diagnosis {
        id: "quit_fault",
        title: "F1 25 hit an error as it closed".to_owned(),
        what: "F1 25 was already closing when it hit an error and ended itself. This is a fault in the game's own \
               shutdown, not a crash during play, and nothing you were doing is lost."
            .to_owned(),
        steps: vec!["Nothing to fix. Press Play when you want to play again.".to_owned()],
    }
}
