//! How a session's game ended, read from its exit code, who stopped it and what the mod loader saw.

use crate::diagnostics::launch::LoaderSeen;
use crate::session::{CORE_STOPPED_LOAD, Ending, SessionEnd, crash_code};

impl SessionEnd {
    /// `code` is the exit code the session read, or None with `unread`
    /// saying why it could not. The session launcher's line in the loader
    /// log gives the game's own code, which the session lacks when the EA
    /// app started the game and gets only the low byte of under Proton.
    /// `stopped` says who closed the game on purpose.
    pub fn resolve(
        code: Option<i32>,
        unread: &str,
        stopped: Option<&str>,
        loader: Option<&LoaderSeen>,
    ) -> Self {
        let loader = loader.copied().unwrap_or_default();
        let code = loader.stub_exit.map(|c| c as i32).or(code);
        let end = |end, crash_code, reason: String| Self {
            end,
            exit_code: code,
            crash_code,
            reason,
        };
        let named = |x: u32| {
            crate::diagnostics::crash::fatal_name(x)
                .map(|n| format!("{n}, "))
                .unwrap_or_default()
        };
        let Some(c) = code else {
            return match (stopped, loader.unhandled) {
                (Some(who), _) => end(Ending::Stopped, None, format!("{who}; {unread}")),
                (None, Some(x)) => end(
                    Ending::Crash,
                    Some(x),
                    format!(
                        "the mod loader saw an unhandled exception ({}{x:#010x}); {unread}",
                        named(x)
                    ),
                ),
                (None, None) if loader.low_disk => end(
                    Ending::Crash,
                    None,
                    format!("F1 25 closed after its Insufficient Storage dialog; {unread}"),
                ),
                (None, None) => end(Ending::Unknown, None, unread.to_owned()),
            };
        };
        if let Some(who) = stopped {
            return end(
                Ending::Stopped,
                None,
                format!("{who}; F1 25 exited with code {c}"),
            );
        }
        if c == 0 {
            return end(Ending::Quit, None, "F1 25 exited with code 0".into());
        }
        if c == CORE_STOPPED_LOAD && loader.attached && loader.unhandled.is_none() {
            return end(
                Ending::Stopped,
                None,
                format!(
                    "the core closed F1 25 at the session load (exit code {c}) instead of letting it crash: the save \
                     names a grid, a list or a track this build doesn't have, and loader.log's [era] line says which"
                ),
            );
        }
        if c == crate::saves::launcher::EXIT && loader.attached && loader.unhandled.is_none() {
            return end(
                Ending::Stopped,
                None,
                format!(
                    "the core closed F1 25 before the title screen because launcher saves could not be used (exit \
                     code {c}); no save was read or written. Play once with your Steam saves, or turn off Use \
                     launcher saves for the profile; loader.log's [saves] lines say why"
                ),
            );
        }
        if let Some(x) = loader.unhandled {
            return end(
                Ending::Crash,
                Some(x),
                format!(
                    "the mod loader saw an unhandled exception ({}{x:#010x}), then F1 25 exited with code {c}",
                    named(x)
                ),
            );
        }
        let u = c as u32;
        if loader.low_disk {
            return end(
                Ending::Crash,
                None,
                format!("F1 25 closed with code {c} after its Insufficient Storage dialog"),
            );
        }
        if let Some(sentence) = crate::diagnostics::fast_fail::sentence(u) {
            return end(Ending::Crash, Some(u), sentence);
        }
        if crash_code(u) {
            return end(
                Ending::Crash,
                Some(u),
                format!("F1 25 exited with a crash code ({}{u:#010x})", named(u)),
            );
        }
        let seen = if loader.attached {
            "the mod loader saw no unhandled exception"
        } else {
            "the mod loader was not attached to see why"
        };
        let reason = if c == -1 {
            format!(
                "F1 25 ended itself with code -1, which its crash handler uses and which it also leaves when \
                 it fails while closing; {seen}"
            )
        } else {
            format!("F1 25 exited with code {c} ({u:#x}); {seen}")
        };
        end(Ending::Unknown, None, reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{STOP_FILE, SessionRecord, request_stop};

    fn seen(log: &str) -> LoaderSeen {
        LoaderSeen::scan(log)
    }

    const ATTACHED: &str = "swofty loader (version.dll proxy) attached, pid 15228\n";
    const UNHANDLED: &str = "[22104] [crash] unhandled ACCESS_VIOLATION (0xc0000005) at F1_25.exe+0xb9ae23 \
                                 reached the top-level filter; passing it to the game's filter\n";

    #[test]
    fn a_crash_needs_a_crash_code_or_an_unhandled_exception() {
        let crashed = format!("{ATTACHED}{UNHANDLED}[stub] [offline] game exited with code 4294967295\n");
        let end = SessionEnd::resolve(Some(-1), "", None, Some(&seen(&crashed)));
        assert_eq!(
            (end.end, end.exit_code, end.crash_code),
            (Ending::Crash, Some(-1), Some(0xc000_0005))
        );
        assert!(
            end.reason.contains("access violation, 0xc0000005"),
            "{}",
            end.reason
        );

        let end = SessionEnd::resolve(Some(-1073740791), "", None, None);
        assert_eq!((end.end, end.crash_code), (Ending::Crash, Some(0xc000_0409)));
        assert_eq!(
            end.reason,
            "F1 25 stopped itself after detecting a corrupted stack or an invalid call (0xc0000409)"
        );
        for code in [0xc0e9_0002u32, 0xcfff_ffff, 0xc000_001d, 0x8000_0003] {
            assert_eq!(
                SessionEnd::resolve(Some(code as i32), "", None, None).end,
                Ending::Crash
            );
        }

        let quit_late =
            format!("{ATTACHED}[crash] TerminateProcess(4294967295) on thread 39612, 2916 s after load\n");
        let end = SessionEnd::resolve(Some(-1), "", None, Some(&seen(&quit_late)));
        assert_eq!(
            (end.end, end.exit_code, end.crash_code),
            (Ending::Unknown, Some(-1), None)
        );
        assert!(
            end.reason.contains("saw no unhandled exception"),
            "{}",
            end.reason
        );
        let end = SessionEnd::resolve(Some(-1), "", None, None);
        assert!(end.reason.contains("not attached"), "{}", end.reason);
        let full = format!(
            "{ATTACHED}[3820] dialog shown: Insufficient Storage: 652/2048 MiB available on 'C:\\', please free \
                 some space.\n[stub] [offline] game exited with code 4294967295\n"
        );
        let end = SessionEnd::resolve(Some(-1), "", None, Some(&seen(&full)));
        assert_eq!((end.end, end.crash_code), (Ending::Crash, None));
        assert!(end.reason.contains("Insufficient Storage"), "{}", end.reason);
        let end = SessionEnd::resolve(None, "no code", None, Some(&seen(&full)));
        assert_eq!(end.end, Ending::Crash);
        let stopped = SessionEnd::resolve(
            Some(-1),
            "",
            Some("Stop in the launcher closed F1 25"),
            Some(&seen(&full)),
        );
        assert_eq!(stopped.end, Ending::Stopped);
        for code in [1, 0x4001_0004, 255] {
            assert_eq!(
                SessionEnd::resolve(Some(code), "", None, None).end,
                Ending::Unknown
            );
        }
        assert_eq!(SessionEnd::resolve(Some(0), "", None, None).end, Ending::Quit);
    }

    #[test]
    fn a_load_the_core_stopped_is_named() {
        let end = SessionEnd::resolve(Some(CORE_STOPPED_LOAD), "", None, Some(&seen(ATTACHED)));
        assert_eq!(end.end, Ending::Stopped);
        assert!(
            end.reason.contains("the core closed F1 25 at the session load"),
            "{}",
            end.reason
        );
        let plain = SessionEnd::resolve(Some(CORE_STOPPED_LOAD), "", None, None);
        assert_eq!(plain.end, Ending::Unknown);
    }

    #[test]
    fn the_session_launchers_line_gives_the_code_the_session_could_not_read() {
        let ea = "[stub] [offline] game exited with code 1\n";
        let end = SessionEnd::resolve(None, "the EA app started F1 25", None, Some(&seen(ea)));
        assert_eq!((end.end, end.exit_code), (Ending::Unknown, Some(1)));
        let proton = "[stub] [offline] game exited with code 3221225477\n";
        let end = SessionEnd::resolve(Some(5), "", None, Some(&seen(proton)));
        assert_eq!((end.end, end.exit_code), (Ending::Crash, Some(-1073741819)));
        let end = SessionEnd::resolve(None, "the EA app started F1 25", None, Some(&seen(ATTACHED)));
        assert_eq!(
            end,
            SessionEnd {
                end: Ending::Unknown,
                exit_code: None,
                crash_code: None,
                reason: "the EA app started F1 25".into(),
            }
        );
    }

    #[test]
    fn an_unhandled_exception_is_a_crash_when_the_ea_app_started_the_game() {
        let log = format!("{ATTACHED}{UNHANDLED}");
        let end = SessionEnd::resolve(None, "the EA app started F1 25", None, Some(&seen(&log)));
        assert_eq!(
            (end.end, end.exit_code, end.crash_code),
            (Ending::Crash, None, Some(0xc000_0005))
        );
        assert!(end.reason.contains("access violation"), "{}", end.reason);
        let stopped = SessionEnd::resolve(
            None,
            "",
            Some("Stop in the launcher closed F1 25"),
            Some(&seen(&log)),
        );
        assert_eq!(stopped.end, Ending::Stopped);
    }

    #[test]
    fn a_stop_is_not_a_crash() {
        let crashed = format!("{ATTACHED}{UNHANDLED}");
        let end = SessionEnd::resolve(
            Some(-1),
            "",
            Some("Stop in the launcher closed F1 25"),
            Some(&seen(&crashed)),
        );
        assert_eq!(
            (end.end, end.exit_code, end.crash_code),
            (Ending::Stopped, Some(-1), None)
        );
        assert_eq!(
            end.reason,
            "Stop in the launcher closed F1 25; F1 25 exited with code -1"
        );
        let end = SessionEnd::resolve(None, "signal 9", Some("asked to stop"), None);
        assert_eq!((end.end, end.exit_code), (Ending::Stopped, None));
        assert_eq!(end.reason, "asked to stop; signal 9");
    }

    #[test]
    fn the_end_file_decides_what_shows_as_a_crash() {
        let root = std::env::temp_dir().join(format!("swofty-session-end-{}", std::process::id()));
        let dir = root.join("sessions/20261023-221530");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(root.join("sessions/latest.txt"), format!("{}\n", dir.display())).unwrap();
        std::fs::write(dir.join("game_start"), "100\n").unwrap();
        request_stop(&root);
        assert!(dir.join(STOP_FILE).is_file());
        std::fs::write(dir.join("exit_code"), "-1\n").unwrap();
        std::fs::write(dir.join("game_end"), "200\n").unwrap();
        assert!(
            !SessionRecord::read(&dir).crashed(),
            "no end file: -1 is not a crash code"
        );
        let mut end = SessionEnd::resolve(Some(-1), "", Some("Stop in the launcher closed F1 25"), None);
        end.write(&dir).unwrap();
        let record = SessionRecord::read(&dir);
        assert_eq!(record.end.as_ref(), Some(&end));
        assert!(!record.crashed());
        for (ending, code, shown) in [
            (Ending::Crash, Some(-1), true),
            (Ending::Unknown, Some(-1), false),
            (Ending::Unknown, None, false),
            (Ending::Quit, Some(0), false),
        ] {
            end.end = ending;
            end.exit_code = code;
            end.write(&dir).unwrap();
            assert_eq!(SessionRecord::read(&dir).crashed(), shown, "{ending:?} {code:?}");
        }
        std::fs::remove_file(dir.join(STOP_FILE)).unwrap();
        request_stop(&root);
        assert!(!dir.join(STOP_FILE).exists(), "the session has ended");
        std::fs::remove_dir_all(&root).ok();
    }
}
