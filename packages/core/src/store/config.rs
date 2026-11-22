//! The launcher's config and folders files, the session settings a profile gives a session, and the requests
//! the game makes.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::Store;
use super::profiles::profile_write;
use super::scan::set_aside_unreadable;
use crate::addons::builtin::CORE_ID;
use crate::addons::settings::{self, Merge, Requests, SessionSettings};
use crate::profile::Profile;
use crate::session::Config;
use crate::{Result, io_err};

/// Prints a warning to stderr once per process.
pub(super) static WARNING_SINK: std::sync::OnceLock<fn(&str)> = std::sync::OnceLock::new();

/// Sends the store's warnings to `sink` as well as stderr; the launcher uses its app log.
pub fn set_warning_sink(sink: fn(&str)) {
    WARNING_SINK.set(sink).ok();
}

pub(super) fn warn(text: String) {
    eprintln!("swofty: warning: {text}");
    if let Some(sink) = WARNING_SINK.get() {
        sink(&text);
    }
}

pub(super) fn warn_once(text: String) {
    static SEEN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if seen.insert(text.clone()) {
        eprintln!("swofty: warning: {text}");
    }
}

/// A settings file, or the defaults when there is none or this build can't read it. One that is
/// JSON in a shape this build can't read (a hand edit with a wrong type, a value a newer launcher
/// wrote) stays where it is, so fixing it or going back to that launcher brings it back, until a
/// save moves it aside with [`set_aside_unreadable`].
pub(super) fn load_settings<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    match crate::load_json(path) {
        Ok(crate::Loaded::Found(v)) => v,
        Ok(crate::Loaded::Missing | crate::Loaded::Damaged(_)) => T::default(),
        Err(e) => {
            warn_once(format!("{e}; using the defaults until it can be read"));
            T::default()
        }
    }
}

impl Store {
    pub fn config(&self) -> Config {
        let mut config: Config = load_settings(&self.config_path());
        config.migrate_games();
        config
    }

    pub fn save_config(&self, config: &Config) -> Result<()> {
        let path = self.config_path();
        set_aside_unreadable::<Config>(&path)?;
        crate::write_json(&path, config)
    }

    pub fn folders_path(&self) -> PathBuf {
        self.root.join(crate::profile::folders::FILE)
    }

    pub fn folders(&self) -> crate::profile::folders::Folders {
        load_settings(&self.folders_path())
    }

    pub fn save_folders(&self, folders: &crate::profile::folders::Folders) -> Result<()> {
        let path = self.folders_path();
        set_aside_unreadable::<crate::profile::folders::Folders>(&path)?;
        crate::write_json(&path, folders)
    }

    pub fn latest_session(&self) -> Option<PathBuf> {
        let dir = std::fs::read_to_string(self.sessions_dir().join("latest.txt")).ok()?;
        Some(PathBuf::from(dir.trim()))
    }

    pub fn requests_path(&self, profile: &str) -> PathBuf {
        self.requests_dir().join(format!("{profile}.json"))
    }

    pub fn session_settings(&self, profile: &str) -> Result<SessionSettings> {
        self.session_settings_of(&self.profile(profile)?)
    }

    /// The settings of a session of `p`, a profile as the caller has it, such as one a room lends the host's
    /// load order and options, on the copy Play uses.
    pub fn session_settings_of(&self, p: &Profile) -> Result<SessionSettings> {
        let copy = crate::game_folder::copies::find_active(&self.config())
            .map_or(crate::install::Source::Unknown, |c| c.source);
        self.session_settings_on(p, copy)
    }

    /// [`Self::session_settings_of`] on `copy`, the store that installed the game folder the session runs.
    pub fn session_settings_on(&self, p: &Profile, copy: crate::install::Source) -> Result<SessionSettings> {
        let requests = crate::data::paths::game_folder_view(&self.requests_path(&p.name));
        let installed = self.addons()?;
        let mut s = SessionSettings::build(p, &installed, Some(requests))?;
        // Each addon's folder and track flag, as the launcher's Addons page has them.
        let folders = self.folders();
        for a in &mut s.addons {
            let Some(m) = installed.get(&a.id) else { continue };
            let builtin = self.is_builtin(&a.id) || a.id == crate::updates::managed::ID;
            a.track |=
                !self.is_builtin(&a.id) && crate::tracks::is_track_addon(&self.addons_dir().join(&a.id), m);
            let category = m
                .category
                .or(a.track.then_some(swofty_addon::manifest::Category::Tracks));
            a.folder = folders.place(&a.id, builtin, category).map(|k| folders.name(&k));
        }
        (s.launcher_saves, s.plain_saves) = crate::saves::launcher::session_saves(
            &self.root,
            &p.name,
            p.launcher_saves,
            p.is_vanilla(),
            copy,
        );
        let on: Vec<String> = p.enabled().map(str::to_owned).collect();
        s.resume_needs = crate::stdlib::careers::session_needs(&self.root, &p.name, &on, &installed);
        s.addons
            .retain(|a| a.id == CORE_ID || !self.is_builtin(&a.id) || !a.options.is_empty());
        Ok(s)
    }

    pub fn merge_requests(&self, profile: &str, consume: bool) -> Result<Option<Merge>> {
        let path = self.requests_path(profile);
        if !path.is_file() {
            return Ok(None);
        }
        let req: Requests = crate::read_json(&path)?;
        let _write = profile_write();
        let merged = if req.profile.is_empty() || req.profile == profile {
            let mut p = self.profile(profile)?;
            let out = settings::merge(&mut p, &self.addons()?, &req);
            if !out.changed.is_empty() {
                self.save_profile(&p)?;
            }
            Some(out)
        } else {
            None
        };
        if consume {
            std::fs::remove_file(&path).map_err(io_err(&path))?;
        }
        Ok(merged)
    }
}
