//! What kept version.dll out of the game, and what the player can do about each cause.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::GAME_EXE;
use crate::install::PROXY;

/// What kept version.dll from running in the game, from what the session
/// found once the loader missed it (see `loader_check`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cause {
    /// Smart App Control blocked it (`seen`: a Code Integrity event says
    /// so), or is on and nothing else explains the miss.
    SmartAppControl { seen: bool },
    /// An App Control (WDAC) policy other than Smart App Control blocked it.
    Policy { name: Option<String> },
    /// F1_25.exe runs with Code integrity guard: Microsoft-signed DLLs only.
    SignedOnly,
    /// Microsoft Defender blocked it: a detection or an attack surface
    /// reduction rule.
    Defender,
    /// The registry lists version.dll as a KnownDLL.
    KnownDll,
    /// The game holds a version.dll from another folder and not the game
    /// folder's (`prefer_system32`: it runs with that DLL load policy, which
    /// `from` turns on; None when no setting the session reads does).
    OtherCopy {
        path: String,
        prefer_system32: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<PreferSource>,
    },
    /// The game holds another version.dll, and F1_25.exe has Exploit
    /// protection program settings.
    ExploitProtection { path: String },
    /// The game holds another version.dll, and compatibility settings are on
    /// for F1_25.exe or Windows loaded its compatibility shims into it.
    /// `from`: the program that started the game with them.
    CompatLayer {
        path: String,
        layers: Vec<String>,
        shims: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Carrier>,
    },
    /// The game holds another version.dll, and an antivirus other than
    /// Microsoft Defender is on.
    Antivirus { path: String, names: Vec<String> },
    /// The game holds another version.dll and nothing the session can see
    /// explains it; `hooks` are the programs hooked into the game.
    Unexplained { path: String, hooks: Vec<String> },
    /// Windows denied the session a look inside the game process, and no
    /// block event or other antivirus explains the miss.
    Unreadable,
    /// The game had already exited when the session looked, so what it
    /// loaded is unknown; Windows logged no block. `antivirus`: the products
    /// other than Microsoft Defender that are on.
    GameEnded {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        antivirus: Vec<String>,
    },
    /// The game held the game folder's version.dll, yet the loader never
    /// logged an attach: it was loaded and failed or stalled on its own.
    LoadedSilently {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        exec_options: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        layers: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        security: Vec<String>,
    },
}

pub const EXEC_OPTIONS_KEY: &str =
    r"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\F1_25.exe";

/// Where Windows turns on "prefer System32 DLLs" for F1_25.exe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreferSource {
    /// F1_25.exe's Exploit protection program settings, by name.
    Program,
    /// F1_25.exe's Exploit protection program settings for one path.
    Path { path: String },
    /// Exploit protection's system settings.
    System,
    /// An Exploit protection policy; `from` is the file it names.
    Policy {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<String>,
    },
}

impl PreferSource {
    pub fn name(&self) -> String {
        match self {
            Self::Program => format!("the Exploit protection program settings for {GAME_EXE}"),
            Self::Path { path } => format!("the Exploit protection program settings for {path}"),
            Self::System => "Exploit protection's system settings".to_owned(),
            Self::Policy { from } => format!(
                "an Exploit protection policy set by an organisation or a tool{}",
                from.as_ref().map(|f| format!(" ({f})")).unwrap_or_default()
            ),
        }
    }
}

/// A program that hands compatibility layers on to the game it starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carrier {
    /// Its file name, such as EADesktop.exe.
    pub name: String,
    /// Its path, as the setting stores it or as it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// The settings on it, as `<layers> (for this user|for all users)`;
    /// empty when it runs with shims and has no setting of its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<String>,
}

impl Carrier {
    fn ea_app(&self) -> bool {
        [
            crate::platform::ea_app::PROCESS,
            crate::platform::ea_app::LAUNCHER_EXE,
        ]
        .iter()
        .any(|n| n.eq_ignore_ascii_case(&self.name))
    }

    fn launcher(&self) -> bool {
        self.name.eq_ignore_ascii_case("swofty-launcher.exe")
    }
}

impl Cause {
    pub fn what(&self) -> String {
        match self {
            Self::SmartAppControl { seen: true } => format!("Smart App Control blocked {PROXY}"),
            Self::SmartAppControl { seen: false } => {
                format!("Smart App Control is on and most likely blocked {PROXY}")
            }
            Self::Policy { name } => format!(
                "a Windows App Control policy{} blocked {PROXY}",
                name.as_ref().map(|n| format!(" ({n})")).unwrap_or_default()
            ),
            Self::SignedOnly => format!(
                "Exploit protection lets F1_25.exe load only Microsoft-signed DLLs, so Windows refused {PROXY}"
            ),
            Self::Defender => format!("Microsoft Defender blocked {PROXY}"),
            Self::KnownDll => format!(
                "{PROXY} is listed under KnownDLLs, so Windows loads its own copy instead of the game folder's"
            ),
            Self::OtherCopy {
                path,
                prefer_system32: true,
                from,
            } => format!(
                "the game loaded {path} instead of the game folder's {PROXY}: {}",
                match from {
                    Some(PreferSource::Program) => {
                        "F1_25.exe's Exploit protection program settings make it prefer System32 DLLs"
                            .to_owned()
                    }
                    Some(PreferSource::System) => {
                        "Exploit protection's system settings make every program prefer System32 DLLs"
                            .to_owned()
                    }
                    Some(source @ PreferSource::Policy { .. }) => {
                        format!("{} makes F1_25.exe prefer System32 DLLs", source.name())
                    }
                    Some(source) => format!("{} make F1_25.exe prefer System32 DLLs", source.name()),
                    None =>
                        "Windows makes F1_25.exe prefer System32 DLLs, and Swofty Launcher found no Exploit \
                             protection setting that asks for that"
                            .to_owned(),
                }
            ),
            Self::OtherCopy { path, .. } => format!(
                "the game loaded {path} instead of the game folder's {PROXY}. Security software on this PC most \
                 likely redirected it"
            ),
            Self::ExploitProtection { path } => format!(
                "the game loaded {path} instead of the game folder's {PROXY}, and F1_25.exe has its own Exploit \
                 protection settings in Windows Security, the most likely reason"
            ),
            Self::CompatLayer {
                path,
                layers,
                shims,
                from,
            } => {
                let why = match from {
                    _ if !layers.is_empty() => format!(
                        "compatibility settings are on for F1_25.exe ({})",
                        layers.join(", ")
                    ),
                    Some(c) if c.layers.is_empty() => format!(
                        "Windows applied compatibility shims to it ({}), passed on from {}, which runs with them too",
                        shims.join(", "),
                        c.name
                    ),
                    Some(c) => format!(
                        "Windows applied compatibility shims to it ({}), passed on from the compatibility settings \
                         on {} ({})",
                        shims.join(", "),
                        c.name,
                        c.layers.join(", ")
                    ),
                    None => format!("Windows applied compatibility shims to it ({})", shims.join(", ")),
                };
                format!(
                    "the game loaded {path} instead of the game folder's {PROXY}, and {why}, the most likely reason"
                )
            }
            Self::Antivirus { path, names } => format!(
                "the game loaded {path} instead of the game folder's {PROXY}. {}, the antivirus on this PC, most \
                 likely redirected it",
                names.join(" and ")
            ),
            Self::Unexplained { path, .. } => format!(
                "the game loaded {path} instead of the game folder's {PROXY}, and Swofty Launcher couldn't tell why"
            ),
            Self::Unreadable => format!(
                "Windows didn't let Swofty Launcher look inside the game (access denied), so it can't tell what kept \
                 {PROXY} out. Windows Security and Smart App Control show no block"
            ),
            Self::GameEnded { .. } => format!(
                "F1 25 had already closed when Swofty Launcher looked, so it can't tell whether {PROXY} was loaded. \
                 Windows Security and Smart App Control logged no block, so an antivirus is not shown to be the cause"
            ),
            Self::LoadedSilently {
                exec_options,
                layers,
                security,
            } => {
                let mut text = format!(
                    "the game loaded the game folder's {PROXY}, so nothing blocked it, but the loader never reported \
                     that it started"
                );
                if !exec_options.is_empty() {
                    text.push_str(&format!(
                        ". F1_25.exe has Image File Execution Options values set ({})",
                        exec_options.join(", ")
                    ));
                }
                if !layers.is_empty() {
                    text.push_str(&format!(
                        ". F1_25.exe has compatibility settings on ({})",
                        layers.join("; ")
                    ));
                }
                if !security.is_empty() {
                    text.push_str(&format!(". {} is loaded into the game", security.join(", ")));
                }
                text
            }
        }
    }

    /// Whether the session found nothing that explains the miss, so the
    /// player is pointed at Send Logs and the support Discord.
    pub fn unexplained(&self) -> bool {
        matches!(
            self,
            Self::Unexplained { .. }
                | Self::Unreadable
                | Self::GameEnded { .. }
                | Self::LoadedSilently { .. }
        )
    }

    /// The fix, when it is not the antivirus one.
    pub(super) fn fix(&self, game: &Path) -> Option<String> {
        Some(match self {
            Self::SmartAppControl { .. } => "To fix it, close F1 25, turn off Smart App Control in Windows \
                 Security > App & browser control > Smart App Control settings, then launch again."
                .to_owned(),
            Self::Policy { .. } => format!(
                "To fix it, close F1 25, ask whoever manages this PC to allow {PROXY} in the game folder ({}), \
                 then launch again.",
                game.display()
            ),
            Self::SignedOnly
            | Self::OtherCopy {
                prefer_system32: true,
                from: Some(PreferSource::Program),
                ..
            } => "To fix it, close F1 25, remove the F1_25.exe settings in Windows Security > App & browser \
                 control > Exploit protection > Program settings, then launch again."
                .to_owned(),
            Self::OtherCopy {
                prefer_system32: true,
                from: Some(PreferSource::Path { path }),
                ..
            } => format!(
                "To fix it, close F1 25, remove the F1_25.exe entry for {path} in Windows Security > App & browser \
                 control > Exploit protection > Program settings, then launch again."
            ),
            Self::OtherCopy {
                prefer_system32: true,
                from: Some(PreferSource::System),
                ..
            } => "To fix it, close F1 25, run Set-ProcessMitigation -System -Disable PreferSystem32 in PowerShell \
                 as administrator, restart the PC, then launch again. If that command reports an error, or mods \
                 still don't load, run Get-ProcessMitigation -System in PowerShell and post what it prints in the \
                 support Discord."
                .to_owned(),
            Self::OtherCopy {
                prefer_system32: true,
                from: Some(PreferSource::Policy { .. }),
                ..
            } => "To fix it, close F1 25, ask whoever manages this PC to remove PreferSystem32 for F1_25.exe from \
                 the policy, or undo the tool that set it, then restart the PC and launch again."
                .to_owned(),
            Self::OtherCopy {
                prefer_system32: true,
                from: None,
                ..
            } => "To fix it, close F1 25, restart the PC, then launch again. If mods still don't load, run \
                 Get-ProcessMitigation -Name F1_25.exe and Get-ProcessMitigation -System in PowerShell and post \
                 what they print in the support Discord."
                .to_owned(),
            Self::KnownDll => format!(
                "To fix it, close F1 25, remove {PROXY} from HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\\
                 Session Manager\\KnownDLLs in the registry, restart Windows, then launch again."
            ),
            Self::OtherCopy { .. } => "To fix it, close F1 25, turn off your antivirus's DLL or behaviour \
                 protection for F1_25.exe (a folder exclusion is often not enough), then launch again."
                .to_owned(),
            Self::ExploitProtection { .. } => "To fix it, close F1 25, open Windows Security > App & browser \
                 control > Exploit protection settings > Program settings, remove every F1_25.exe entry, restart \
                 the PC, then launch again."
                .to_owned(),
            Self::CompatLayer { layers, from, .. } => {
                const UNTICK: &str = "open Properties > Compatibility and untick everything, then select Change \
                     settings for all users and untick everything there too";
                const QUIT_EA: &str = "fully quit the EA app (right-click its icon in the system tray > Exit)";
                let at = |c: &Carrier| c.exe.as_ref().map(|e| format!(" ({e})")).unwrap_or_default();
                match from {
                    _ if !layers.is_empty() => format!(
                        "To fix it, close F1 25, right-click F1_25.exe in the game folder ({}), {UNTICK}, then \
                         launch again.",
                        game.display()
                    ),
                    Some(c) if c.layers.is_empty() => format!(
                        "To fix it, close F1 25, {QUIT_EA}, then press Play in Swofty Launcher without starting \
                         the EA app first. If mods still don't load, right-click {}{}, {UNTICK}.",
                        c.name,
                        at(c)
                    ),
                    Some(c) if c.launcher() => format!(
                        "To fix it, close F1 25 and Swofty Launcher, {QUIT_EA}, right-click {}{}, {UNTICK}, then \
                         start Swofty Launcher and launch again.",
                        c.name,
                        at(c)
                    ),
                    Some(c) if c.ea_app() => format!(
                        "To fix it, close F1 25, {QUIT_EA}, right-click {}{}, {UNTICK}, then launch again.",
                        c.name,
                        at(c)
                    ),
                    Some(c) => format!(
                        "To fix it, close F1 25, right-click {}{}, {UNTICK}, then launch again.",
                        c.name,
                        at(c)
                    ),
                    None => "To fix it, close F1 25, fully quit the EA app or Steam (right-click its icon in the \
                         system tray > Exit), then press Play in Swofty Launcher without starting them first. If \
                         mods still don't load, press Send Logs to Developers and ask in the support Discord."
                        .to_owned(),
                }
            }
            Self::Antivirus { names, .. } => format!(
                "To fix it, close F1 25, open {}, restore or allow {PROXY} and F1_25.exe if it blocked them, add \
                 exclusions for the game folder ({}) and the launcher folder, and turn off its DLL or behaviour \
                 protection for F1_25.exe (a folder exclusion is often not enough), then launch again.",
                names[0],
                game.display()
            ),
            Self::Unexplained { hooks, .. } => {
                let close = if hooks.is_empty() {
                    String::new()
                } else {
                    format!("Close {} before you press Play and try again. ", hooks.join(" and "))
                };
                format!(
                    "{close}If mods still don't load, press Send Logs to Developers and ask in the support Discord, \
                     so the developers can look at your PC's setup."
                )
            }
            Self::Unreadable => "To fix it, close F1 25, fully quit the EA app or Steam from the system tray, then \
                 launch again. If mods still don't load, press Send Logs to Developers and ask in the support \
                 Discord."
                .to_owned(),
            Self::GameEnded { antivirus } => {
                let av = match antivirus.as_slice() {
                    [] => String::new(),
                    names => format!(
                        " If {} is on for the game folder, check its protection history for {PROXY}.",
                        names.join(" and ")
                    ),
                };
                format!(
                    "Press Send Logs to Developers and say how far F1 25 got before it closed.{av}"
                )
            }
            Self::LoadedSilently { exec_options, layers, security } => {
                let mut steps: Vec<String> = Vec::new();
                if !exec_options.is_empty() {
                    steps.push(format!(
                        "delete the value{} {} under {EXEC_OPTIONS_KEY} in the registry editor (regedit)",
                        if exec_options.len() == 1 { "" } else { "s" },
                        exec_options.join(", ")
                    ));
                }
                if !layers.is_empty() {
                    steps.push(
                        "clear F1_25.exe's compatibility settings (right-click F1_25.exe > Properties > \
                         Compatibility, untick everything and press Apply)"
                            .to_owned(),
                    );
                }
                if !security.is_empty() {
                    steps.push(format!(
                        "add the game folder ({}) and F1_25.exe as exclusions in {} and turn off its game or \
                         process protection",
                        game.display(),
                        security.join(", ")
                    ));
                }
                if steps.is_empty() {
                    "Press Send Logs to Developers and ask in the support Discord; the loader's own \
                     files in the launcher folder are what the developers need to look at."
                        .to_owned()
                } else {
                    format!(
                        "To fix it, close F1 25, {}, then press Play in Swofty Launcher. If mods still don't load, \
                         press Send Logs to Developers and ask in the support Discord.",
                        steps.join(", then ")
                    )
                }
            }
            Self::Defender => return None,
        })
    }
}
