use std::path::Path;

use crate::diagnostics::quarantine::{Carrier, Cause, PreferSource};
use crate::platform::run_as_admin::AdminLayer;

use super::{
    EXPLOIT_PROTECTION, Findings, Mitigation, POLICY_BLOCK, Proof, SECURITY_MODULES, SIGNING_LEVEL, Search,
    Seen, hooks, in_folder, tidy,
};

impl Findings {
    /// What the player is told when the loader did attach in this launch,
    /// in place of a "Mods did not load" report.
    pub fn attached_text(&self) -> Option<&'static str> {
        let a = self.attached.as_ref()?;
        Some(match (a.window, a.proof) {
            (true, _) => "The mod loader started in F1 25 and reached the game window, where the mods load.",
            (false, Proof::Running) => {
                "The mod loader is running in F1 25 and is waiting for the game window. The mods load once the \
                 window opens; if F1 25 never shows one, close it and launch again."
            }
            (false, Proof::Rotated) => {
                "The mod loader started in F1 25 and was waiting for the game window, where the mods load."
            }
        })
    }

    /// What kept version.dll out of the game, from the strongest evidence:
    /// a blocking event about it, the game's DLL policy, KnownDLLs, another
    /// copy of it in the game, then Smart App Control being on.
    pub fn cause(&self, game: &Path) -> Option<Cause> {
        let sac = matches!(self.smart_app_control, Some(Ok(Some(1))));
        let blocked = self
            .logs
            .iter()
            .filter_map(|l| l.read.as_ref().ok())
            .flatten()
            .find(|e| e.proxy && e.blocks());
        if let Some(e) = blocked {
            return Some(match e.id {
                SIGNING_LEVEL => Cause::SignedOnly,
                POLICY_BLOCK => match &e.policy {
                    Some(p) if p.to_ascii_lowercase().contains("verifiedandreputable") => {
                        Cause::SmartAppControl { seen: true }
                    }
                    None if sac => Cause::SmartAppControl { seen: true },
                    name => Cause::Policy { name: name.clone() },
                },
                _ => Cause::Defender,
            });
        }
        let policy = |g: &Seen| match &g.mitigation {
            Some(Ok(m)) => *m,
            _ => Mitigation::default(),
        };
        if self
            .games
            .iter()
            .any(|g| policy(g).microsoft_signed || policy(g).store_signed)
        {
            return Some(Cause::SignedOnly);
        }
        if matches!(self.search, Some(Ok(Search { known_dll: true, .. }))) {
            return Some(Cause::KnownDll);
        }
        for g in &self.games {
            let Ok(m) = &g.modules else { continue };
            if m.version.iter().any(|p| in_folder(p, game)) {
                continue;
            }
            let Some(path) = m.version.first().map(|p| tidy(p)) else {
                continue;
            };
            if policy(g).prefer_system32 {
                return Some(Cause::OtherCopy {
                    path,
                    prefer_system32: true,
                    from: self.prefer_source(),
                });
            }
            if self.exploit_protection() {
                return Some(Cause::ExploitProtection { path });
            }
            let layers = match &self.layers {
                Some(Ok(l)) => l.clone(),
                _ => Vec::new(),
            };
            if !layers.is_empty() || !m.shims.is_empty() {
                return Some(Cause::CompatLayer {
                    path,
                    from: if layers.is_empty() { self.carrier() } else { None },
                    layers,
                    shims: m.shims.clone(),
                });
            }
            let names = self.third_party_antivirus();
            if !names.is_empty() {
                return Some(Cause::Antivirus { path, names });
            }
            return Some(Cause::Unexplained {
                path,
                hooks: hooks(&m.others),
            });
        }
        if sac {
            return Some(Cause::SmartAppControl { seen: false });
        }
        if self
            .games
            .iter()
            .any(|g| matches!(&g.modules, Ok(m) if m.version.iter().any(|p| in_folder(p, game))))
        {
            return Some(self.loaded_silently());
        }
        if self.games.is_empty() && self.created.is_some() {
            return Some(Cause::GameEnded {
                antivirus: self.third_party_antivirus(),
            });
        }
        let unreadable = !self.games.is_empty() && self.games.iter().all(|g| g.modules.is_err());
        (unreadable && self.third_party_antivirus().is_empty()).then_some(Cause::Unreadable)
    }

    /// The program that most likely started the game with compatibility
    /// layers: the first one with a setting of its own, else an EA app that
    /// runs with shims.
    fn carrier(&self) -> Option<Carrier> {
        if let Some(first) = self.starter_layers.iter().flatten().next() {
            let on = |l: &&AdminLayer| l.name == first.name;
            return Some(Carrier {
                name: first.name.clone(),
                exe: Some(first.exe.clone()),
                layers: self
                    .starter_layers
                    .iter()
                    .flatten()
                    .filter(on)
                    .map(|l| format!("{} ({})", l.layers.trim(), l.scope()))
                    .collect(),
            });
        }
        self.ea_app
            .iter()
            .find(|a| matches!(&a.shims, Ok(s) if !s.is_empty()))
            .map(|a| Carrier {
                name: crate::platform::ea_app::PROCESS.to_owned(),
                exe: a.image.as_deref().map(tidy),
                layers: Vec::new(),
            })
    }

    /// What turns on "prefer System32 DLLs", or with no reading of that,
    /// F1_25.exe's Exploit protection program settings when it has any.
    fn prefer_source(&self) -> Option<PreferSource> {
        match &self.prefer_from {
            Some(Ok(found)) => found.first().cloned(),
            _ => self.exploit_protection().then_some(PreferSource::Program),
        }
    }

    fn loaded_silently(&self) -> Cause {
        let exec_options = match &self.exec_options {
            Some(Ok(names)) => names.clone(),
            _ => Vec::new(),
        };
        let layers = match &self.layers {
            Some(Ok(l)) => l.clone(),
            _ => Vec::new(),
        };
        let mut security: Vec<String> = Vec::new();
        for g in &self.games {
            let Ok(m) = &g.modules else { continue };
            for name in &m.others {
                if SECURITY_MODULES
                    .iter()
                    .any(|p| name.to_lowercase().starts_with(p))
                    && !security.iter().any(|s| s.eq_ignore_ascii_case(name))
                {
                    security.push(name.clone());
                }
            }
        }
        Cause::LoadedSilently {
            exec_options,
            layers,
            security,
        }
    }

    fn exploit_protection(&self) -> bool {
        matches!(&self.exec_options, Some(Ok(names)) if names.iter().any(|n| {
            let value = n.rsplit('\\').next().unwrap_or(n);
            EXPLOIT_PROTECTION.iter().any(|k| value.eq_ignore_ascii_case(k))
        }))
    }

    /// The antivirus products registered with Windows and on, other than
    /// Microsoft Defender.
    fn third_party_antivirus(&self) -> Vec<String> {
        let Some(Ok(listed)) = &self.antivirus else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for entry in listed {
            let Some(name) = entry.strip_suffix(" (on)") else {
                continue;
            };
            if !name.contains("Microsoft Defender")
                && !name.contains("Windows Defender")
                && !out.iter().any(|n| n == name)
            {
                out.push(name.to_owned());
            }
        }
        out
    }
}
