//! How a launch link reaches the EA app: the setting's command, the handler the EA app registered, or
//! EALauncher.exe, and what opens the EA app without a link.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::LAUNCHER_EXE;

#[cfg(windows)]
use super::installs::program_files;
#[cfg(windows)]
use super::read_reg;
#[cfg(windows)]
use super::{DESKTOP_KEYS, PROTOCOL_KEY};
use crate::platform::run_as_admin::parse_reg_query;
use crate::session::COMPAT_LAYER;

/// A command line split into words, honouring double quotes.
pub fn split_command(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut any) = (false, false);
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                any = false;
            }
            c => word.push(c),
        }
    }
    if any || !word.is_empty() {
        words.push(word);
    }
    words
}

/// A registered handler's command line: the EA app writes its program path
/// unquoted, spaces and all, so the program runs up to the first `.exe`.
pub fn split_handler(line: &str) -> Vec<String> {
    let line = line.trim();
    if line.starts_with('"') {
        return split_command(line);
    }
    let lower = line.to_ascii_lowercase();
    let end = lower.match_indices(".exe").map(|(i, _)| i + 4).find(|&i| {
        line[i..]
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == '"')
    });
    match end {
        Some(end) => std::iter::once(line[..end].to_owned())
            .chain(split_command(&line[end..]))
            .collect(),
        None => split_command(line),
    }
}

/// The longest run of words a path with spaces is looked for in.
const PATH_WORDS: usize = 8;

/// Whether `word` holds no shell syntax: no quotes, escapes, expansions,
/// separators, redirections, globs or braces, and no `~` to expand.
fn plain_word(word: &str) -> bool {
    !word.starts_with('~')
        && !word
            .chars()
            .any(|c| c.is_whitespace() || "'\"\\`$;|&<>(){}[]*?#!".contains(c))
}

fn shell_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A shell command line with each unquoted absolute path that runs on past
/// a space put in single quotes. The shell cuts
/// `/home/me/.steam/steam/steamapps/common/Proton - Experimental/proton` at
/// its first space, so where the path up to a space is not on disk and the
/// path with the words after it is, those words are quoted together. The
/// value of a leading `NAME=` assignment counts the same. A line whose paths
/// are on disk as written stays as it is, and so does everything from the
/// first word with shell syntax on.
pub fn quote_spaced_paths(line: &str, exists: &dyn Fn(&Path) -> bool) -> String {
    let words: Vec<&str> = line.split(' ').collect();
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let word = words[i];
        if !plain_word(word) && !word.is_empty() {
            out.extend(words[i..].iter().map(|w| (*w).to_owned()));
            break;
        }
        let (prefix, path) = match word.split_once('=') {
            Some((name, value)) if shell_name(name) => (&word[..=name.len()], value),
            _ => ("", word),
        };
        let mut quoted = None;
        if path.starts_with('/') && !exists(Path::new(path)) {
            let mut joined = path.to_owned();
            for (j, next) in words.iter().enumerate().skip(i + 1).take(PATH_WORDS) {
                if !plain_word(next) && !next.is_empty() {
                    break;
                }
                joined.push(' ');
                joined.push_str(next);
                if exists(Path::new(&joined)) {
                    quoted = Some((j, format!("{prefix}'{joined}'")));
                    break;
                }
            }
        }
        match quoted {
            Some((last, word)) => {
                out.push(word);
                i = last + 1;
            }
            None => {
                out.push(word.to_owned());
                i += 1;
            }
        }
    }
    out.join(" ")
}

/// How a launch link reaches the EA app.
#[derive(Debug, Clone, PartialEq)]
pub enum Handoff {
    /// A program and its arguments, with `%1` or `{url}` where the link goes.
    Program(Vec<String>),
    /// A shell command line with `{url}` where the link goes.
    Shell(String),
}

impl Handoff {
    /// The `origin2` handler Windows registered, from `reg query <key> /ve`.
    pub fn from_protocol(reg_out: &str) -> Option<Self> {
        let (_, command) = parse_reg_query(reg_out).into_iter().next()?;
        let words = split_handler(&command);
        (!words.is_empty()).then_some(Self::Program(words))
    }

    pub fn from_setting(command: &str) -> Option<Self> {
        let command = command.trim();
        if command.is_empty() {
            None
        } else if cfg!(windows) {
            Some(Self::Program(split_handler(command)))
        } else {
            Some(Self::Shell(quote_spaced_paths(command, &|p: &Path| p.exists())))
        }
    }

    /// The words to run for `url`; the link is appended when the command
    /// has no placeholder.
    pub fn argv(&self, url: &str) -> Vec<String> {
        match self {
            Self::Program(words) => {
                let mut placed = false;
                let mut out: Vec<String> = words
                    .iter()
                    .map(|w| {
                        if w.contains("%1") || w.contains("{url}") {
                            placed = true;
                            w.replace("%1", url).replace("{url}", url)
                        } else {
                            w.clone()
                        }
                    })
                    .collect();
                if !placed {
                    out.push(url.to_owned());
                }
                out
            }
            Self::Shell(line) => {
                let quoted = format!("'{}'", url.replace('\'', r"'\''"));
                let line = if line.contains("{url}") {
                    line.replace("{url}", &quoted)
                } else {
                    format!("{line} {quoted}")
                };
                vec!["sh".into(), "-c".into(), line]
            }
        }
    }

    /// The program the hand-off runs, when it is an exe rather than a shell
    /// command line.
    pub fn program(&self) -> Option<PathBuf> {
        match self {
            Self::Program(words) if words[0].to_ascii_lowercase().ends_with(".exe") => {
                Some(PathBuf::from(&words[0]))
            }
            _ => None,
        }
    }

    /// The command that hands over `url`. The EA app it may start is the
    /// one that starts F1 25, so the launcher's [`COMPAT_LAYER`] stays out.
    pub fn command(&self, url: &str) -> Command {
        let argv = self.argv(url);
        let mut c = Command::new(&argv[0]);
        c.args(&argv[1..]).env_remove(COMPAT_LAYER);
        c
    }
}

impl std::fmt::Display for Handoff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Program(words) => f.write_str(&words.join(" ")),
            Self::Shell(line) => f.write_str(line),
        }
    }
}

/// EALauncher.exe from the EA Desktop key's values, else its default folder.
/// `InstallLocation` is the EA Desktop folder, with the app one level down.
pub fn launcher_exe(desktop_values: &str, defaults: &[PathBuf]) -> Option<PathBuf> {
    let values = parse_reg_query(desktop_values);
    let value = |name: &str| {
        values
            .iter()
            .filter(|(n, d)| n.eq_ignore_ascii_case(name) && !d.trim().is_empty())
            .map(|(_, d)| PathBuf::from(d.trim().trim_matches('"')))
            .collect::<Vec<_>>()
    };
    let beside = |exe: PathBuf| exe.parent().map(|d| d.join(LAUNCHER_EXE));
    value("LauncherAppPath")
        .into_iter()
        .chain(
            value("InstallLocation")
                .into_iter()
                .flat_map(|d| [d.join("EA Desktop").join(LAUNCHER_EXE), d.join(LAUNCHER_EXE)]),
        )
        .chain(value("DesktopAppPath").into_iter().filter_map(beside))
        .chain(value("ClientPath").into_iter().filter_map(beside))
        .chain(defaults.iter().cloned())
        .find(|p| p.is_file())
}

/// How this computer hands a launch link to the EA app: the setting when
/// set, else on Windows the `origin2` handler the EA app registered. The
/// handler is what Windows runs for EA's own links, so it follows the EA
/// app wherever it is installed and shows up front whether it is; running
/// it directly instead of through `start` keeps the link's `&` away from
/// cmd.exe. EALauncher.exe from the EA Desktop key is the fallback.
pub fn handoff(setting: Option<&str>) -> Option<Handoff> {
    if let Some(h) = setting.and_then(Handoff::from_setting) {
        return Some(h);
    }
    windows_handoff()
}

#[cfg(windows)]
pub(super) fn windows_handoff() -> Option<Handoff> {
    if let Some(h) = read_reg(&[PROTOCOL_KEY, "/ve"]).and_then(|o| Handoff::from_protocol(&o)) {
        return Some(h);
    }
    let desktop: String = DESKTOP_KEYS.iter().filter_map(|k| read_reg(&[k])).collect();
    let defaults: Vec<PathBuf> = program_files()
        .iter()
        .map(|p| {
            p.join(r"Electronic Arts\EA Desktop\EA Desktop")
                .join(LAUNCHER_EXE)
        })
        .collect();
    launcher_exe(&desktop, &defaults).map(|exe| Handoff::Program(vec![exe.display().to_string()]))
}

#[cfg(not(windows))]
pub(super) fn windows_handoff() -> Option<Handoff> {
    None
}

/// What opens the EA app without a launch link: EALauncher.exe on its own,
/// or the setting's command with the link left out.
pub fn opener(handoff: &Handoff) -> Option<Command> {
    let mut c = match handoff {
        Handoff::Program(words) if words[0].to_ascii_lowercase().ends_with(".exe") => Command::new(&words[0]),
        Handoff::Shell(line) => {
            let mut c = Command::new("sh");
            c.arg("-c").arg(line.replace("{url}", ""));
            c
        }
        Handoff::Program(_) => return None,
    };
    c.env_remove(COMPAT_LAYER);
    Some(c)
}

/// Starts a program as the signed-in user from an administrator's process.
pub const DESKTOP_SHELL: &str = "explorer.exe";

/// What opens the EA app for a process that may run as administrator, and
/// whether the command's exit code says if it worked. A program started by an
/// administrator's process runs as administrator too, so from one the EA app
/// goes through Explorer and runs with the signed-in user's own rights, as
/// from its shortcut. Explorer's exit code says nothing about the EA app.
pub fn desktop_opener(handoff: &Handoff, elevated: bool) -> Option<(Command, bool)> {
    let open = opener(handoff)?;
    if !elevated || !matches!(handoff, Handoff::Program(_)) {
        return Some((open, true));
    }
    let mut c = Command::new(DESKTOP_SHELL);
    c.arg(open.get_program()).env_remove(COMPAT_LAYER);
    Some((c, false))
}

pub const NO_HANDOFF_LINUX: &str = "this is the EA app's copy of F1 25, and on Linux Swofty starts it through the EA app with \
     the launch.ea_launch_command setting: a command that hands {url} to the EA app in its Wine prefix. Set it in \
     config.json (see Playing the EA app copy in the docs), or switch to another copy";

pub const NO_HANDOFF_WINDOWS: &str =
    "the EA app was not found on this computer; install or repair the EA app, then press Play again";

pub fn no_handoff() -> &'static str {
    if cfg!(windows) {
        NO_HANDOFF_WINDOWS
    } else {
        NO_HANDOFF_LINUX
    }
}
