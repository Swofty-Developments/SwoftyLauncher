use super::crash_record::{Fatal, module_bases, record_registers};
use super::{Crash, Diagnosis};

/// How far past a module's load address a register still counts as pointing into it.
pub(super) const MODULE_REACH: u64 = 64 << 20;

/// The hook a jump to an address outside every module came from: the game ran
/// an address that is not code, and a register points into the hook's module.
/// A hook that patches the game's graphics calls leaves the game on a dead
/// trampoline this way, so no stack frame names it.
pub(super) fn stray_jump_hook(f: &Fatal, loader: &str) -> Option<(&'static Hook, String)> {
    if f.module.is_some() || !f.record.iter().any(|l| l.contains("executing (DEP)")) {
        return None;
    }
    let bases = module_bases(loader);
    record_registers(&f.record).into_iter().find_map(|value| {
        let (name, base) = bases
            .iter()
            .filter(|(_, base)| *base <= value)
            .max_by_key(|(_, base)| *base)?;
        if value - base >= MODULE_REACH {
            return None;
        }
        let hook = HOOKS.iter().find(|h| !h.own_only && name.starts_with(h.prefix))?;
        Some((hook, name.clone()))
    })
}

/// Programs that hook into games as they run, by the start of their DLL's name.
pub(super) struct Hook {
    pub(super) prefix: &'static str,
    pub(super) product: &'static str,
    pub(super) step: &'static str,
    /// Only when the exception happened in it: the DLL also sits harmlessly
    /// on stacks of crashes it had no part in.
    pub(super) own_only: bool,
}

pub(super) const HOOKS: &[Hook] = &[
    Hook {
        prefix: "rtsshooks",
        product: "RivaTuner Statistics Server (RTSS), which comes with MSI Afterburner",
        step: "Close RivaTuner Statistics Server and MSI Afterburner, including from the system tray, before you \
               press Play. Or, in RTSS, add F1_25.exe and set its Application detection level to None.",
        own_only: false,
    },
    Hook {
        prefix: "windhawk",
        product: "Windhawk",
        step: "Quit Windhawk from its tray icon before you press Play, or add F1_25.exe to the processes \
               Windhawk doesn't inject into in its settings.",
        own_only: false,
    },
    Hook {
        prefix: "medal-hook",
        product: "Medal",
        step: "Close Medal, including from the system tray, before you press Play.",
        own_only: false,
    },
    Hook {
        prefix: "discordhook",
        product: "Discord's in-game overlay",
        step: "Turn off Discord's in-game overlay in Discord's settings (Game Overlay).",
        own_only: false,
    },
    Hook {
        prefix: "graphics-hook",
        product: "OBS Studio's game capture",
        step: "In OBS Studio, capture F1 25 with Window Capture or Display Capture instead of Game Capture, or \
               close OBS while you play.",
        own_only: false,
    },
    Hook {
        prefix: "nvpresent",
        product: "the NVIDIA app's overlay and frame tools",
        step: "Turn off the in-game overlay and Smooth Motion for F1 25 in the NVIDIA app.",
        own_only: true,
    },
    Hook {
        prefix: "nvspcap",
        product: "the NVIDIA app's overlay",
        step: "Turn off the in-game overlay in the NVIDIA app's settings.",
        own_only: true,
    },
    Hook {
        prefix: "gameoverlayrenderer",
        product: "the Steam overlay",
        step: "Turn off the Steam overlay for F1 25: right-click F1 25 in Steam, Properties, General, and untick \
               Enable the Steam Overlay while in-game.",
        own_only: true,
    },
    Hook {
        prefix: "specialk",
        product: "Special K",
        step: "Turn Special K off for F1 25, or remove it from the game folder.",
        own_only: false,
    },
    Hook {
        prefix: "pid.dll",
        product: "Windows' force feedback driver for your wheel",
        step: "Update your wheel's driver and software (for example Logitech G HUB, Fanatec, Thrustmaster or \
               Moza), plug the wheel straight into the PC, and make sure only one wheel program is running.",
        own_only: true,
    },
];

impl Hook {
    /// The diagnosis for a crash this hook caused, after `happened` says where.
    fn crashed(&self, happened: String) -> Diagnosis {
        Diagnosis {
            id: "third_party_hook",
            title: format!("F1 25 crashed inside {}", self.product),
            what: format!(
                "{happened}, which belongs to {}. It hooks into games while they run, and here it took F1 25 \
                 down with it.",
                self.product
            ),
            steps: vec![self.step.to_owned(), "Press Play again.".to_owned()],
        }
    }
}

pub(super) fn crash_in_hook(c: &Crash) -> Option<Diagnosis> {
    let f = &c.fatal;
    let hook = HOOKS.iter().find(|hook| {
        let own = f.module.as_deref().is_some_and(|m| m.starts_with(hook.prefix));
        let near = !hook.own_only && f.near().any(|m| m.starts_with(hook.prefix));
        own || near
    })?;
    let module = f
        .near()
        .find(|m| m.starts_with(hook.prefix))
        .unwrap_or(hook.prefix)
        .to_owned();
    Some(hook.crashed(format!("The crash happened in {module}")))
}

pub(super) fn stray_jump(c: &Crash) -> Option<Diagnosis> {
    let (hook, module) = stray_jump_hook(&c.fatal, &c.evidence.loader)?;
    Some(hook.crashed(format!(
        "F1 25 jumped to an address that is not code while {module} was at work"
    )))
}
