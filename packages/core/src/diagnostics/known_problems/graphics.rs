use crate::diagnostics::overlay_guard::{Removal, SessionOverlay};

use super::crash_record::{crash_holds, crash_line, stamp, thread};
use super::evidence::Evidence;
use super::memory::{KEEP_YEARS, needs_all_years};
use super::wording::{VERIFY, list, name};
use super::{Crash, Diagnosis};

pub(super) fn gpu_removed(e: &Evidence) -> bool {
    crash_holds(
        e,
        &[
            "887a0005",
            "887a0006",
            "887a0007",
            "dxgi_error_device_removed",
            "dxgi_error_device_hung",
            "dxgi_error_device_reset",
        ],
    )
}

pub(super) fn out_of_memory(e: &Evidence) -> bool {
    crash_holds(e, &["8007000e", "e_outofmemory"])
}

/// The names of the addons that drew with Swofty's overlay in the session.
pub(super) fn painters(e: &Evidence, overlay: &SessionOverlay) -> Vec<String> {
    overlay
        .painters
        .iter()
        .filter(|id| !e.builtin.contains(*id))
        .map(|id| e.names.get(id).cloned().unwrap_or_else(|| id.clone()))
        .collect()
}

/// Graphics drivers' user-mode DLLs, by the start of their name.
pub(super) const DRIVERS: &[(&str, &str)] = &[
    ("nvwgf2um", "NVIDIA"),
    ("nvd3dum", "NVIDIA"),
    ("nvoglv", "NVIDIA"),
    ("nvlddmkm", "NVIDIA"),
    ("amdxc64", "AMD"),
    ("amdxx64", "AMD"),
    ("atidxx64", "AMD"),
    ("igd12umd", "Intel"),
    ("igd10iumd", "Intel"),
    ("igdumdim", "Intel"),
    ("igc64", "Intel"),
];

/// The game's frame generation DLLs.
pub(super) const FRAME_GEN: &[(&str, &str)] = &[
    ("nvngx_dlssg", "DLSS Frame Generation"),
    ("sl.dlss_g", "DLSS Frame Generation"),
    ("libxess_fg", "XeSS frame generation"),
];

/// Modules that mean the crash happened in graphics code.
pub(super) fn graphics(module: &str) -> bool {
    module.starts_with("sl.")
        || module.starts_with("nvngx")
        || module.starts_with("dxgi")
        || module.starts_with("d3d1")
        || module.starts_with("dxcore")
        || module.starts_with("libxess")
        || module.starts_with("amd_fidelityfx")
        || DRIVERS.iter().any(|(p, _)| module.starts_with(p))
}

/// Names in `loaded` that only graphics mods use; Windows and the drivers
/// ship DLLs named like the others (dxgi.dll, nvapi64.dll).
pub(super) fn distinctive(name: &str) -> bool {
    let f = name.to_ascii_lowercase();
    f.starts_with("reshade")
        || f.starts_with("optiscaler")
        || f.starts_with("specialk")
        || f.starts_with("dlssg_to_fsr3")
        || f.ends_with(".asi")
        || f.ends_with(".addon64")
        || f.ends_with(".addon")
}

/// Files in the game folder that F1 25 doesn't ship and that hook its
/// graphics, plus the distinctive ones among the game's loaded modules,
/// grouped by the tool they come from.
pub(super) fn graphics_mods(game_files: &[String], loaded: &[String]) -> Vec<(&'static str, Vec<String>)> {
    let mut files = game_files.to_vec();
    for m in loaded.iter().filter(|m| distinctive(m)) {
        if !files.iter().any(|f| f.eq_ignore_ascii_case(m)) {
            files.push(m.clone());
        }
    }
    let files = &files;
    let mut groups: Vec<(&'static str, Vec<String>)> = Vec::new();
    let mut add = |tool: &'static str, name: &str| match groups.iter_mut().find(|(t, _)| *t == tool) {
        Some((_, names)) => names.push(name.to_owned()),
        None => groups.push((tool, vec![name.to_owned()])),
    };
    let reshade = files.iter().any(|f| {
        let f = f.to_ascii_lowercase();
        f == "reshade.ini" || f.starts_with("reshade64") || f.ends_with(".addon64") || f.ends_with(".addon")
    });
    let optiscaler = files
        .iter()
        .any(|f| f.to_ascii_lowercase().starts_with("optiscaler"));
    for name in files {
        let f = name.to_ascii_lowercase();
        let tool = if f.starts_with("optiscaler")
            || f == "nvngx.dll"
            || f == "fakenvapi.ini"
            || f == "nvapi64.dll"
            || f.starts_with("dlssg_to_fsr3")
            || f == "amdxc64.dll"
            || f == "libxess_dx11.dll"
        {
            "OptiScaler"
        } else if f == "reshade.ini"
            || f.starts_with("reshade64")
            || f.ends_with(".addon64")
            || f.ends_with(".addon")
            || f == "reshade-shaders"
        {
            "ReShade"
        } else if f == "dxgi.dll" || f == "d3d12.dll" || f == "d3d11.dll" {
            if optiscaler {
                "OptiScaler"
            } else if reshade {
                "ReShade"
            } else {
                "a graphics mod"
            }
        } else if f.starts_with("specialk") {
            "Special K"
        } else if f.ends_with(".asi") || f == "winmm.dll" || f == "dinput8.dll" {
            "an ASI loader"
        } else {
            continue;
        };
        add(tool, name);
    }
    groups
}

pub(super) fn tools(groups: &[(&'static str, Vec<String>)]) -> String {
    let names: Vec<String> = groups.iter().map(|(t, _)| (*t).to_owned()).collect();
    list(&names)
}

pub(super) fn move_out(groups: &[(&'static str, Vec<String>)]) -> String {
    let files: Vec<String> = groups.iter().flat_map(|(_, f)| f.iter().cloned()).collect();
    format!(
        "Close Swofty Launcher, then move {} out of the F1 25 folder into a folder of their own, so you can put \
         them back later.",
        list(&files)
    )
}

pub(super) fn driver_steps() -> Vec<String> {
    vec![
        "Update your graphics driver, or reinstall it cleanly. If you installed a new driver in the last few days, \
         try the one before it."
            .to_owned(),
        "Set any graphics card overclock or undervolt (MSI Afterburner, the NVIDIA app's tuning, AMD Adrenalin) \
         back to stock."
            .to_owned(),
        "Turn off frame generation (DLSS, FSR or XeSS frame generation) in F1 25's graphics settings.".to_owned(),
        "On a graphics card with 8 GB of memory or less, lower Texture Quality, or play with fewer car, livery \
         and track addons."
            .to_owned(),
    ]
}

/// The archive resources two car packs must both replace to count as full ones.
pub(super) const CAR_PACK_RESOURCES: usize = 1000;

/// Whether the game's graphics settings had frame generation on, as Swofty's
/// overlay last read them.
pub(super) fn frame_gen_on(loader: &str) -> bool {
    loader
        .lines()
        .rev()
        .find(|l| l.contains("[overlay] the game's hardware settings"))
        .is_some_and(|l| l.contains("have frame generation on"))
}

/// Two or more full car packs that replace each other's cars in the build, by name.
pub(super) fn car_packs(e: &Evidence) -> Vec<String> {
    let mut packs: Vec<&String> = Vec::new();
    for p in e.conflicts.iter().filter(|p| {
        p.resources >= CAR_PACK_RESOURCES && p.items.iter().any(|i| i.contains("vehicle_package/teams/"))
    }) {
        for id in [&p.winner, &p.loser] {
            if !packs.contains(&id) {
                packs.push(id);
            }
        }
    }
    if packs.len() < 2 {
        return Vec::new();
    }
    packs.sort_by_key(|id| e.enabled.iter().position(|x| x == *id));
    packs.iter().map(|id| name(e, id)).collect()
}

/// What the addons in the profile and the game's settings add to a graphics
/// card crash: sentences for what happened, and their steps.
pub(super) fn graphics_causes(e: &Evidence) -> (String, Vec<String>) {
    let mut what = String::new();
    let mut steps = Vec::new();
    let packs = car_packs(e);
    if !packs.is_empty() {
        what.push_str(&format!(
            " {} are all full car packs, and they replace each other's cars. Several on at once can load more \
             than the graphics card can take.",
            list(&packs)
        ));
        steps.push(format!("Keep only one of {} on in your profile.", list(&packs)));
    }
    if frame_gen_on(&e.loader) {
        what.push_str(" Frame generation was on in F1 25's graphics settings.");
        steps.push(FRAME_GEN_OFF.to_owned());
    }
    (what, steps)
}

pub(super) const FRAME_GEN_OFF: &str =
    "Turn off frame generation (DLSS, FSR or XeSS frame generation) in F1 25's graphics settings.";

pub(super) fn device_removed(c: &Crash) -> Option<Diagnosis> {
    let (e, mods) = (c.evidence, &c.mods);
    if !c.on_windows || !gpu_removed(e) {
        return None;
    }
    let overlay = SessionOverlay::scan(&e.loader).unwrap_or_default();
    let addons = match overlay.removal {
        Some(Removal::AfterDraw { .. }) if overlay.drew => painters(e, &overlay),
        _ => Vec::new(),
    };
    if overlay.removal == Some(Removal::AfterDraw { at_present: true }) && !addons.is_empty() {
        let mut steps = vec![format!(
            "Turn off {} in your profile, then play again.",
            list(&addons)
        )];
        steps.extend(driver_steps().into_iter().take(2));
        return Some(Diagnosis {
            id: "gpu_removed_overlay",
            title: "Your graphics card stopped responding while Swofty's overlay was drawing".to_owned(),
            what: format!(
                "Windows reset the graphics driver (DXGI_ERROR_DEVICE_REMOVED) shortly after Swofty's in-game \
                 overlay drew for {}, and F1 25 can't recover from that, so it closed. The overlay is \
                 experimental.",
                list(&addons)
            ),
            steps,
        });
    }
    let (causes, mut steps) = graphics_causes(e);
    if !mods.is_empty() {
        steps.insert(0, move_out(mods));
    }
    let frame_gen = steps.iter().any(|s| s == FRAME_GEN_OFF);
    steps.extend(
        driver_steps()
            .into_iter()
            .filter(|s| !frame_gen || !s.starts_with("Turn off frame generation")),
    );
    if !addons.is_empty() {
        steps.push(format!(
            "If it keeps happening, turn off {} in your profile for one session.",
            list(&addons)
        ));
    }
    let also = if mods.is_empty() {
        String::new()
    } else {
        format!(
            " Your game folder also has {} installed, which hooks F1 25's graphics.",
            tools(mods)
        )
    };
    let cause = if causes.is_empty() {
        " This comes from the graphics card or its driver, not from Swofty's code."
    } else {
        causes.as_str()
    };
    Some(Diagnosis {
        id: "gpu_removed",
        title: "Your graphics driver stopped responding".to_owned(),
        what: format!(
            "Windows reset the graphics driver while F1 25 was running (DXGI_ERROR_DEVICE_REMOVED). F1 25 can't \
             recover from that, so it closed.{cause}{also}"
        ),
        steps,
    })
}

pub(super) fn out_of_graphics_memory(c: &Crash) -> Option<Diagnosis> {
    let e = c.evidence;
    if !c.on_windows || !out_of_memory(e) {
        return None;
    }
    let (causes, mut steps) = graphics_causes(e);
    steps.extend([
        "Lower Texture Quality, and the resolution if it's above your screen's, in F1 25's graphics settings."
            .to_owned(),
        if needs_all_years(e) {
            KEEP_YEARS.to_owned()
        } else {
            "Play with fewer car, livery and track addons on at once, for example only the Classic Seasons years \
             you race."
                .to_owned()
        },
        "Close other programs, especially web browsers, while you play.".to_owned(),
        "On a laptop with integrated graphics, give the graphics more memory if its settings allow it (often \
         called UMA frame buffer size)."
            .to_owned(),
    ]);
    let what = if causes.is_empty() {
        " Car, livery and track addons use a lot of graphics memory, and several at once can use more than the \
         graphics card has."
    } else {
        causes.as_str()
    };
    Some(Diagnosis {
        id: "out_of_memory",
        title: "F1 25 ran out of graphics memory".to_owned(),
        what: format!("The game couldn't get the memory it asked for (E_OUTOFMEMORY), so it closed.{what}"),
        steps,
    })
}

/// The loader's lines once the game made a DXGI factory or a D3D12 device, or a swapchain: what a graphics mod in
/// the game folder hooks.
const GRAPHICS_STARTED: &[&str] = &[
    "[d3d12] the first D3D12CreateDevice ",
    "[d3d12] the first CreateDXGIFactory",
    "[d3d12] D3D12CreateDevice ",
    "[d3d12] CreateDXGIFactory",
    "[overlay] drawing on swapchain ",
];

/// Whether a DXGI factory, a D3D12 device or a swapchain was made before the crash: by the game on any thread, or
/// by the core's own D3D12 arm on the thread that crashed, as its arm runs beside the game's start.
pub(super) fn graphics_started(c: &Crash) -> bool {
    let e = c.evidence;
    let crash = crash_line(e, &c.fatal);
    let (crashed, on) = (crash.and_then(stamp), crash.and_then(thread));
    e.loader
        .lines()
        .filter(|l| made_graphics(l, on))
        .any(|l| match (crashed, stamp(l)) {
            (Some(crashed), Some(at)) => at <= crashed,
            _ => true,
        })
}

fn made_graphics(line: &str, crashed_on: Option<u32>) -> bool {
    let probe = line.contains("probe device made");
    let made = probe
        || GRAPHICS_STARTED.iter().any(|m| line.contains(m))
        || (line.contains("[d3d12] swapchain ") && line.contains(" created through "));
    let core = probe || line.contains("the core's own");
    made && (!core || (crashed_on.is_some() && thread(line) == crashed_on))
}

pub(super) fn graphics_mod_crash(c: &Crash) -> Option<Diagnosis> {
    let mods = &c.mods;
    let started = crate::diagnostics::launch::LoaderSeen::scan(&c.evidence.loader).window;
    let in_graphics = c.fatal.near().any(graphics);
    if mods.is_empty() || (!in_graphics && (started || !graphics_started(c))) {
        return None;
    }
    let when = if started {
        "in its graphics code"
    } else {
        "while it was setting up its graphics, before its window opened"
    };
    Some(Diagnosis {
        id: "graphics_mods",
        title: format!("{} in your game folder crashed F1 25", tools(mods)),
        what: format!(
            "Your F1 25 folder has {} installed. It changes how F1 25 draws, and F1 25 crashed {when}.",
            tools(mods)
        ),
        steps: vec![
            move_out(mods),
            VERIFY.to_owned(),
            "Press Play. If F1 25 starts, you can put the files back one at a time to find the one that breaks \
             it."
                .to_owned(),
        ],
    })
}

pub(super) fn driver_crash(c: &Crash) -> Option<Diagnosis> {
    let module = c.fatal.module.as_deref().filter(|_| c.on_windows)?;
    let (_, vendor) = DRIVERS.iter().find(|(p, _)| module.starts_with(p))?;
    Some(Diagnosis {
        id: "graphics_driver",
        title: format!("F1 25 crashed inside your {vendor} graphics driver"),
        what: format!("The crash happened in {module}, part of the {vendor} graphics driver."),
        steps: driver_steps(),
    })
}

pub(super) fn frame_generation_crash(c: &Crash) -> Option<Diagnosis> {
    let e = c.evidence;
    let module = c.fatal.module.as_deref()?;
    let (_, what) = FRAME_GEN.iter().find(|(p, _)| module.starts_with(p))?;
    let overlay = SessionOverlay::scan(&e.loader).unwrap_or_default();
    let mut steps = vec![format!(
        "Turn off {what} in F1 25's graphics settings, then play again."
    )];
    let mut also = String::new();
    if overlay.drew && !painters(e, &overlay).is_empty() {
        let addons = list(&painters(e, &overlay));
        also = format!(
            " Swofty's in-game overlay was drawing for {addons} at the time, and it is experimental."
        );
        steps.insert(0, format!("Turn off {addons} in your profile."));
    }
    steps.push("Update your graphics driver.".to_owned());
    Some(Diagnosis {
        id: "frame_generation",
        title: format!("F1 25 crashed in {what}"),
        what: format!("The crash happened in {module}, the game's {what} code.{also}"),
        steps,
    })
}
