use super::evidence::Evidence;
use super::wording::{list, name};
use super::{Crash, Diagnosis};

/// Imported Sepang, the heaviest imported track: the track code of its package.
pub(super) const SEPANG: &str = "bahrain24";

/// The fatal engine heap error the game ended with: its code and message.
pub(super) fn heap_error(e: &Evidence) -> Option<(u32, String)> {
    let parse = |l: &str| {
        let rest = &l[l.find("Ne heap error 0x")? + "Ne heap error 0x".len()..];
        let hex: String = rest.chars().take_while(char::is_ascii_hexdigit).collect();
        let fatal =
            l.contains("fatal Ne heap error") || rest[hex.len()..].trim_start().starts_with("(fatal)");
        fatal.then(|| Some((u32::from_str_radix(&hex, 16).ok()?, rest.to_owned())))?
    };
    e.loader
        .lines()
        .rev()
        .filter(|l| l.contains("[crash] "))
        .find_map(parse)
        .or_else(|| {
            e.report
                .as_deref()?
                .lines()
                .find(|l| l.starts_with("reason"))
                .and_then(parse)
        })
}

/// How full Windows' memory commit was in the newest `[perf]` minute line that reports it, as a percentage of the
/// commit limit.
pub(super) fn commit_percent(e: &Evidence) -> Option<u32> {
    let line = e
        .loader
        .lines()
        .rev()
        .find(|l| l.contains("[perf] ") && l.contains(" memory: ") && l.contains(" MiB used, commit "))?;
    let rest = line.split(" MiB used, commit ").nth(1)?;
    let (used, rest) = rest.split_once('/')?;
    let limit: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let (used, limit) = (used.trim().parse::<u64>().ok()?, limit.parse::<u64>().ok()?);
    (limit > 0).then(|| (used * 100 / limit).min(100) as u32)
}

pub(super) const PAGE_FILE_STEP: &str = "Let Windows manage the page file: search Windows for View advanced system settings, open Performance \
     Settings, Advanced, Virtual memory, Change, and tick Automatically manage paging file size for all \
     drives. Keep 20 GB or more free on drive C.";

pub(super) fn windows_out_of_memory(code: u32, message: &str) -> bool {
    matches!(code, 0x5 | 0x58)
        || message.contains("useAddressSpace")
        || message.contains("Out of physical memory")
}

/// The enabled Classic Seasons year packs.
pub(super) fn classic_years(e: &Evidence) -> usize {
    e.enabled
        .iter()
        .filter(|id| {
            id.strip_prefix("swofty.classic-seasons-")
                .is_some_and(|y| !y.is_empty() && y.bytes().all(|b| b.is_ascii_digit()))
        })
        .count()
}

pub(super) const CAREER_ADDONS: [&str; 2] = ["swofty.progressive", "career.classic-career"];

pub(super) const SWOFTY_PROGRESSIVE: &str = "swofty.swofty-seasons-progressive";

pub(super) fn needs_all_years(e: &Evidence) -> bool {
    e.enabled.iter().any(|id| CAREER_ADDONS.contains(&id.as_str()))
}

fn needs_swofty_packs(e: &Evidence) -> bool {
    e.enabled.iter().any(|id| id == SWOFTY_PROGRESSIVE)
}

fn keep_packs(e: &Evidence) -> Option<&'static str> {
    if needs_all_years(e) {
        Some(KEEP_YEARS)
    } else if needs_swofty_packs(e) {
        Some(KEEP_SWOFTY_PACKS)
    } else {
        None
    }
}

pub(super) const KEEP_SWOFTY_PACKS: &str = "Keep the SWOFTY season packs on (Progressive Career needs them for the next seasons); turn \
     off imported tracks that aren't in your calendar and other content addons you don't need.";

const HEAP_ENV_STEP: &str = "To give the game more of this memory, open config.json in the launcher's data folder \
     (%LOCALAPPDATA%\\Swofty on Windows) and add \"SWOFTY_GAME_HEAP_MB\": \"1024\" under \"env\" in \"launch\", then restart the launcher and press Play.";

/// The GAME_DEFAULT size the core picks on a PC under 12 GiB of RAM (`heap_sizes_logic.hpp` `small_mb`).
const SMALL_RAM_HEAP_MIB: &str = "640 MiB";

fn small_ram_heap(e: &Evidence) -> bool {
    e.loader.lines().any(|l| {
        l.contains("[heap] ")
            && l.contains("GAME_DEFAULT")
            && (l.contains("default for a machine under 12 GiB")
                || l.contains(&format!("GAME_DEFAULT heap armed: {SMALL_RAM_HEAP_MIB} ")))
    })
}

pub(super) const KEEP_YEARS: &str = "Keep every Classic Seasons year on (Progressive needs them for the next seasons); turn off \
     imported tracks that aren't in your calendar and other content addons you don't need.";

pub(super) fn years(n: usize) -> String {
    format!("{n} Classic Seasons year{}", if n == 1 { "" } else { "s" })
}

pub(super) fn saving(e: &Evidence) -> bool {
    e.report
        .as_deref()
        .is_some_and(|r| r.contains("Game_SaveData_WriteBag"))
}

pub(super) fn absurd_request(message: &str) -> bool {
    message
        .split("Requested size:")
        .nth(1)
        .map(|rest| rest.chars().take_while(char::is_ascii_digit).collect::<String>())
        .and_then(|n| n.parse::<u64>().ok())
        .is_some_and(|n| n > 1_000_000_000_000_000)
}

pub(super) fn replay_state_dropped(e: &Evidence) -> bool {
    e.loader
        .lines()
        .any(|l| l.contains("[replay] ") && l.contains("is not recorded"))
}

pub(super) fn replay_state() -> Diagnosis {
    Diagnosis {
        id: "replay_state",
        title: "F1 25 read back a replay it could not record on a grid this big".to_owned(),
        what: "A season with 23 or more cars outgrew part of the game's replay recorder, so that part was left out \
               of the recording. Going back through the replay (a flashback, an instant replay) then read it as \
               garbage and the game closed. It is not a memory problem, and fewer mods or more memory in your PC \
               won't change it."
            .to_owned(),
        steps: vec![
            "Turn Flashbacks off in the assists for seasons with 23 or more cars.".to_owned(),
            "Avoid instant replays in those sessions until the next Swofty update.".to_owned(),
            "Press Play again.".to_owned(),
        ],
    }
}

/// Whether `id` is an import of Sepang, under any id the launcher installs one as.
fn is_sepang(id: &str) -> bool {
    id.split_once('.').is_some_and(|(_, name)| name == SEPANG)
}

pub(super) fn game_heap(e: &Evidence) -> Diagnosis {
    let mut tracks: Vec<&String> = e.tracks.iter().collect();
    tracks.sort_by_key(|id| !is_sepang(id));
    let tracks: Vec<String> = tracks.iter().map(|id| name(e, id)).collect();
    let mut heavy = tracks.clone();
    let years_on = classic_years(e);
    let keep = keep_packs(e);
    let all_years = keep.is_some();
    let small = small_ram_heap(e);
    let more_ram = if small {
        "The launcher gives the game less of this memory on a PC with under 12 GB of RAM."
    } else {
        "More memory in your PC doesn't change that."
    };
    if years_on > 0 {
        heavy.push(years(years_on));
    }
    let mut what = if saving(e) {
        "F1 25 keeps a fixed amount of memory for game data, and it filled up while the game was saving your \
         career, so the game closed."
    } else {
        "F1 25 keeps a fixed amount of memory for game data, and it filled up, so the game closed."
    }
    .to_owned();
    what.push(' ');
    what.push_str(more_ram);
    if !heavy.is_empty() {
        what.push_str(&format!(
            " Imported tracks and season packs use a lot of it, and your profile has {} on.",
            list(&heavy)
        ));
    }
    let mut steps = Vec::new();
    if let Some(first) = tracks.first() {
        let one = tracks.len() == 1 || e.tracks.iter().any(|t| is_sepang(t));
        let (which, at) = match one {
            true => (first.clone(), first.clone()),
            false => (list(&tracks), "one of them".to_owned()),
        };
        steps.push(if all_years {
            format!(
                "For Grand Prix and Time Trial, turn off {which} or make a lighter profile without it. Keep it on \
                 for a career whose calendar races there."
            )
        } else {
            format!(
                "Turn off {which} in your profile, or make a lighter profile without it for the sessions it crashes in."
            )
        });
        if !all_years {
            steps[0].push_str(&format!(
                " In a My Team career whose next round is at {at}, change that round with the Career Mode Editor \
                 first: the career won't load while that track is off."
            ));
        }
    }
    steps.push(if let Some(keep) = keep {
        keep.to_owned()
    } else if years_on > 0 {
        "Turn on fewer imported tracks and only the Classic Seasons years you race.".to_owned()
    } else {
        "Play with fewer imported tracks and season addons on at once.".to_owned()
    });
    if small {
        steps.push(HEAP_ENV_STEP.to_owned());
    }
    Diagnosis {
        id: "game_heap",
        title: "F1 25's game memory filled up".to_owned(),
        what,
        steps,
    }
}

pub(super) fn windows_memory(e: &Evidence) -> Diagnosis {
    let years_on = classic_years(e);
    let fewer = if let Some(keep) = keep_packs(e) {
        keep.to_owned()
    } else if years_on > 0 {
        format!(
            "Turn on only the Classic Seasons years you race; your profile has {} on.",
            years(years_on)
        )
    } else {
        "Play with fewer car, livery and track addons on at once.".to_owned()
    };
    Diagnosis {
        id: "windows_out_of_memory",
        title: "Windows ran out of memory".to_owned(),
        what: "Windows had no memory left to give F1 25, so the game closed. This is your PC's memory (RAM and the \
               page file), not the graphics card's."
            .to_owned(),
        steps: vec![
            PAGE_FILE_STEP.to_owned(),
            "Close other programs, especially web browsers, while you play.".to_owned(),
            fewer,
        ],
    }
}

pub(super) fn heap_exhausted(c: &Crash) -> Option<Diagnosis> {
    let e = c.evidence;
    let (code, message) = heap_error(e)?;
    if code == 0xc && message.contains("'GAME_DEFAULT'") {
        if absurd_request(&message) && replay_state_dropped(e) {
            return Some(replay_state());
        }
        return Some(game_heap(e));
    }
    (c.on_windows && windows_out_of_memory(code, &message)).then(|| windows_memory(e))
}
