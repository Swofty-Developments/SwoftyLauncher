use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::SystemTime;

use anyhow::Result;
use swofty_core::game_folder::recovery::SessionLock;
use swofty_core::platform::process::{descends_from, flag, games_in, stop_process};
use swofty_core::session::{SessionRecord, request_stop};
use swofty_core::store::Store;

/// What starts the game for a batch: `swofty run`, or a stand-in in tests.
pub(crate) trait Launcher {
    fn launch(
        &mut self,
        profile: &str,
        game_args: &[String],
        env: &[(String, String)],
        dir: &Path,
    ) -> Result<Box<dyn Running>>;
}

/// A started batch.
pub(crate) trait Running {
    /// The game has exited and its game files are back.
    fn finished(&mut self) -> bool;
    /// Ask the game to close, by its exact pid.
    fn stop(&mut self);
    /// Give up: end the launcher process itself.
    fn kill(&mut self);
    /// The session folder with the session's logs, once known.
    fn session(&mut self) -> Option<PathBuf>;
}

pub(crate) struct SwoftyRun {
    pub(crate) data: PathBuf,
    pub(crate) game: PathBuf,
}

impl Launcher for SwoftyRun {
    fn launch(
        &mut self,
        profile: &str,
        game_args: &[String],
        env: &[(String, String)],
        dir: &Path,
    ) -> Result<Box<dyn Running>> {
        let log = std::fs::File::create(dir.join("launcher.log"))?;
        let mut c = Command::new(std::env::current_exe()?);
        c.arg(flag("data-dir", &self.data))
            .arg(flag("profile", profile))
            .arg("run")
            .arg("--apply")
            .arg("--direct")
            .arg("--no-start-store")
            .arg(flag("game", &self.game));
        for a in game_args {
            c.arg(format!("--game-arg={a}"));
        }
        c.env("SWOFTY_KEEP_FOCUS", "1")
            .env(swofty_core::build_support::ALLOW_ENV, "1")
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut c, 0);
        let before = std::fs::read(self.data.join("state/launch.json")).ok();
        let child = c.spawn()?;
        Ok(Box::new(SwoftyRunning {
            child,
            data: self.data.clone(),
            game: self.game.clone(),
            launched: SystemTime::now(),
            before,
            exited: false,
            session: None,
        }))
    }
}

struct SwoftyRunning {
    pub(crate) child: Child,
    pub(crate) data: PathBuf,
    pub(crate) game: PathBuf,
    pub(crate) launched: SystemTime,
    pub(crate) before: Option<Vec<u8>>,
    pub(crate) exited: bool,
    pub(crate) session: Option<PathBuf>,
}

impl Running for SwoftyRunning {
    fn finished(&mut self) -> bool {
        if !self.exited {
            self.exited = !matches!(self.child.try_wait(), Ok(None));
        }
        self.exited && !SessionLock::held_elsewhere(&self.data)
    }

    fn stop(&mut self) {
        request_stop(&self.data);
        let ours = games_in(&self.game).into_iter().filter(|g| {
            descends_from(g.pid, self.child.id()) || g.started.is_some_and(|s| s >= self.launched)
        });
        for g in ours {
            stop_process(g.pid).ok();
        }
    }

    fn kill(&mut self) {
        stop_process(self.child.id()).ok();
    }

    fn session(&mut self) -> Option<PathBuf> {
        if self.session.is_some() {
            return self.session.clone();
        }
        let now = std::fs::read(self.data.join("state/launch.json")).ok()?;
        if Some(&now) == self.before.as_ref() {
            return None;
        }
        let id = serde_json::from_slice::<serde_json::Value>(&now).ok()?["id"]
            .as_str()?
            .to_owned();
        let store = Store::new(&self.data);
        let dir = store.latest_session()?;
        let record = SessionRecord::read(&dir);
        if record.request_id.as_deref() == Some(id.as_str()) {
            self.session = Some(dir);
        }
        self.session.clone()
    }
}
