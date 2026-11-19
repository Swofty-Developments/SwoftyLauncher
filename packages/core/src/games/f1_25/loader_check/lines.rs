use std::path::Path;

use crate::GAME_EXE;
use crate::diagnostics::quarantine::PreferSource;
use crate::install::PROXY;

use super::{Findings, MAX_EVENTS, Proxy, Seen, file_name, in_folder, tidy};

impl Findings {
    /// The findings as log lines.
    pub fn lines(&self, game: &Path) -> Vec<String> {
        let mut out = Vec::new();
        out.push(match &self.proxy {
            Proxy::Missing => format!("{PROXY}: gone from the game folder"),
            Proxy::Present { size, same, zone } => format!(
                "{PROXY}: in the game folder, {size} bytes, {}{}",
                match same {
                    Some(true) => "the bytes the launcher placed",
                    Some(false) => "NOT the bytes the launcher placed",
                    None => "no record of what the launcher placed",
                },
                zone.map(|z| format!(", marked as downloaded (Zone.Identifier ZoneId={z})"))
                    .unwrap_or_default()
            ),
            Proxy::Unreadable(e) => format!("{PROXY}: in the game folder but unreadable ({e})"),
        });
        if self.rotated {
            out.push(
                "loader.prev.log was written after the game started: the loader ran and moved its log aside"
                    .into(),
            );
        }
        if let Some(a) = &self.attached {
            out.push(a.line());
        }
        if self.games.is_empty() {
            out.push(format!("game: no {GAME_EXE} is running"));
        }
        if let Some(pid) = self.created.filter(|p| !self.games.iter().any(|g| g.pid == *p)) {
            out.push(format!(
                "game: the offline play stub created pid {pid}, which is not running"
            ));
        }
        for g in &self.games {
            out.extend(self.game_lines(g, game));
        }
        match &self.search {
            Some(Ok(s)) => out.push(format!(
                "DLL search: {PROXY} is {}a KnownDLL, SafeDllSearchMode {}",
                if s.known_dll { "" } else { "not " },
                s.safe_search.map_or("default (on)".into(), |v| v.to_string())
            )),
            Some(Err(e)) => out.push(format!("DLL search: not readable ({e})")),
            None => {}
        }
        match &self.exec_options {
            Some(Ok(names)) if names.is_empty() => {
                out.push(format!("Image File Execution Options for {GAME_EXE}: none"))
            }
            Some(Ok(names)) => out.push(format!(
                "Image File Execution Options for {GAME_EXE}: {}",
                names.join(", ")
            )),
            Some(Err(e)) => out.push(format!(
                "Image File Execution Options for {GAME_EXE}: not readable ({e})"
            )),
            None => {}
        }
        match &self.prefer_from {
            Some(Ok(found)) if found.is_empty() => out.push(format!(
                "prefer System32 DLLs: not turned on in the Exploit protection program or system settings or by an \
                 Exploit protection policy for {GAME_EXE}"
            )),
            Some(Ok(found)) => out.push(format!(
                "prefer System32 DLLs: turned on by {}",
                found.iter().map(PreferSource::name).collect::<Vec<_>>().join(" and ")
            )),
            Some(Err(e)) => out.push(format!("prefer System32 DLLs: settings not readable ({e})")),
            None => {}
        }
        match &self.layers {
            Some(Ok(layers)) if layers.is_empty() => {
                out.push(format!("compatibility settings on {GAME_EXE}: none"))
            }
            Some(Ok(layers)) => out.push(format!(
                "compatibility settings on {GAME_EXE}: {}",
                layers.join(", ")
            )),
            Some(Err(e)) => out.push(format!(
                "compatibility settings on {GAME_EXE}: not readable ({e})"
            )),
            None => {}
        }
        match &self.starter_layers {
            Some(found) if found.is_empty() => out.push(format!(
                "compatibility settings on {} and the offline play stub: none",
                crate::platform::run_as_admin::STARTERS.join(", ")
            )),
            Some(found) => out.extend(found.iter().map(|l| {
                format!(
                    "compatibility settings on {}: {} ({}) on {}",
                    l.name,
                    l.layers.trim(),
                    l.scope(),
                    l.exe
                )
            })),
            None => {}
        }
        for a in &self.ea_app {
            let pid = a.pid;
            let name = crate::platform::ea_app::PROCESS;
            out.push(match &a.shims {
                Ok(s) if s.is_empty() => format!("{name} pid {pid}: no Windows compatibility shims"),
                Ok(s) => format!(
                    "{name} pid {pid}: Windows compatibility shims loaded: {}",
                    s.join(", ")
                ),
                Err(e) => format!("{name} pid {pid}: modules not readable ({e})"),
            });
        }
        match &self.smart_app_control {
            Some(Ok(state)) => out.push(format!(
                "Smart App Control: {}",
                match state {
                    None => "not set".into(),
                    Some(0) => "off".into(),
                    Some(1) => "on".into(),
                    Some(2) => "evaluation".into(),
                    Some(n) => format!("state {n}"),
                }
            )),
            Some(Err(e)) => out.push(format!("Smart App Control: not readable ({e})")),
            None => {}
        }
        for log in &self.logs {
            match &log.read {
                Ok(events) if events.is_empty() => out.push(format!(
                    "{} log: nothing about {PROXY} or {GAME_EXE} since the game started",
                    log.name
                )),
                Ok(events) => {
                    out.push(format!(
                        "{} log: {} event(s) about {PROXY} or {GAME_EXE} since the game started",
                        log.name,
                        events.len()
                    ));
                    for e in events.iter().take(MAX_EVENTS) {
                        out.push(format!(
                            "  {} {}{}: {}{}: {}{}{}",
                            log.name,
                            e.id,
                            e.at.as_ref().map(|t| format!(" at {t}")).unwrap_or_default(),
                            e.file.as_deref().map(tidy).unwrap_or_else(|| "?".into()),
                            e.process
                                .as_deref()
                                .map(|p| format!(" in {}", file_name(p)))
                                .unwrap_or_default(),
                            e.what(),
                            e.policy.as_ref().map(|p| format!(" ({p})")).unwrap_or_default(),
                            e.threat.as_ref().map(|t| format!(" ({t})")).unwrap_or_default(),
                        ));
                    }
                }
                Err(e) => out.push(format!("{} log: not readable ({e})", log.name)),
            }
        }
        match &self.antivirus {
            Some(Ok(names)) if names.is_empty() => out.push("antivirus registered with Windows: none".into()),
            Some(Ok(names)) => out.push(format!("antivirus registered with Windows: {}", names.join(", "))),
            Some(Err(e)) => out.push(format!("antivirus registered with Windows: not readable ({e})")),
            None => {}
        }
        match self.wine_override {
            Some(true) => out.push(format!("Proton prefix: user.reg has the {PROXY} override")),
            Some(false) => out.push(format!(
                "Proton prefix: user.reg lacks the {PROXY} override, so Wine uses its builtin {PROXY}"
            )),
            None => {}
        }
        out
    }

    fn game_lines(&self, g: &Seen, game: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let pid = g.pid;
        out.push(format!(
            "game pid {pid}{}: {}{}, {}",
            if self.created == Some(pid) {
                " (the one the offline play stub created)"
            } else {
                ""
            },
            g.image
                .as_deref()
                .map(tidy)
                .unwrap_or_else(|| "exe path unknown".into()),
            g.image_size.map(|s| format!(" ({s} bytes)")).unwrap_or_default(),
            match (g.ours, self.exe_size) {
                (true, _) => format!("the game folder's {GAME_EXE}"),
                (false, Some(s)) => format!("NOT the game folder's {GAME_EXE} ({s} bytes)"),
                (false, None) => format!("NOT the game folder's {GAME_EXE}"),
            }
        ));
        match &g.modules {
            Ok(m) => {
                let (ours, others): (Vec<&String>, Vec<&String>) =
                    m.version.iter().partition(|p| in_folder(p, game));
                let mut held: Vec<String> = Vec::new();
                if !ours.is_empty() {
                    held.push("the game folder's (the loader)".into());
                }
                held.extend(others.iter().map(|p| tidy(p)));
                out.push(format!(
                    "pid {pid}: {} modules, {PROXY} from {}",
                    m.count,
                    if held.is_empty() {
                        "nowhere (none loaded)".into()
                    } else {
                        held.join(" and ")
                    }
                ));
                if !m.others.is_empty() {
                    out.push(format!(
                        "pid {pid}: modules from outside the game and Windows folders: {}",
                        m.others.join(", ")
                    ));
                }
                if !m.shims.is_empty() {
                    out.push(format!(
                        "pid {pid}: Windows compatibility shims loaded: {}",
                        m.shims.join(", ")
                    ));
                }
            }
            Err(e) => out.push(format!("pid {pid}: modules not readable ({e})")),
        }
        match &g.mitigation {
            Some(Ok(m)) => {
                let names = m.names();
                out.push(format!(
                    "pid {pid}: DLL load policies: {}",
                    if names.is_empty() {
                        "none".into()
                    } else {
                        names.join(", ")
                    }
                ));
            }
            Some(Err(e)) => out.push(format!("pid {pid}: DLL load policies not readable ({e})")),
            None => {}
        }
        out
    }
}
