mod install;
mod view;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use eframe::egui;
use swofty_core::data::relocate::find;
use swofty_core::launcher::Progress;

use self::install::{install, install_folder, launch, pick_folder};
use crate::theme::visuals;
use crate::{FINDING, HEIGHT, data, existing, resolve, running};

#[derive(Clone)]
enum Stage {
    Ready,
    Open(PathBuf, String),
    Working { done: f32, text: String },
    Failed(String),
    Launched,
}

struct Reporter {
    stage: Arc<Mutex<Stage>>,
    ctx: egui::Context,
}

impl Reporter {
    fn set(&self, stage: Stage) {
        *self.stage.lock().unwrap() = stage;
        self.ctx.request_repaint();
    }
}

impl Progress for Reporter {
    fn report(&self, done: f32, text: &str) {
        self.set(Stage::Working {
            done,
            text: text.to_owned(),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Offer {
    Ask,
    Use,
    Fresh,
}

pub(crate) struct Installer {
    folder: String,
    desktop: bool,
    existing: existing::Existing,
    data: data::Data,
    data_folder: String,
    choice: data::Choice,
    addons: usize,
    found: Vec<find::Found>,
    offer: Offer,
    height: f32,
    stage: Arc<Mutex<Stage>>,
}

impl Installer {
    pub(crate) fn new(cc: &eframe::CreationContext) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Dark);
        cc.egui_ctx.set_visuals_of(egui::Theme::Dark, visuals());
        let existing = existing::Existing::find();
        let folder = existing
            .root()
            .map_or_else(existing::default_root, Path::to_path_buf);
        let data = data::Data::find();
        let found = data::finder(&data, existing.roots());
        Self {
            folder: folder.display().to_string(),
            desktop: true,
            existing,
            data_folder: data.current().display().to_string(),
            choice: data::Choice::Leave,
            addons: data::addon_count(&data.current()),
            offer: if found.is_empty() {
                Offer::Fresh
            } else {
                Offer::Ask
            },
            found,
            data,
            height: HEIGHT,
            stage: Arc::new(Mutex::new(Stage::Ready)),
        }
    }

    fn browse(&mut self) {
        if let Some(picked) = pick_folder("Install the Swofty Launcher in", &self.folder) {
            self.folder = install_folder(picked).display().to_string();
        }
    }

    fn browse_data(&mut self) {
        if let Some(picked) = pick_folder("Keep your addons and saves in", &self.data_folder) {
            self.data_folder = data::pick(picked, &self.data).display().to_string();
        }
    }

    fn data_for(&self) -> data::Data {
        data::Data {
            install: resolve(&self.folder).ok(),
            ..self.data.clone()
        }
    }

    fn data_target(&self) -> String {
        match resolve(&self.folder) {
            Ok(install) => data::target(self.choice, &install, &self.data, &self.data_folder),
            Err(_) => self.data_folder.clone(),
        }
    }

    fn adopting(&self) -> Option<PathBuf> {
        (self.offer == Offer::Use)
            .then(|| self.found.first().map(|f| f.path.clone()))
            .flatten()
    }

    fn data_move(&self) -> Result<Option<data::DataMove>, String> {
        let data = self.data_for();
        if self.adopting().is_some() {
            return Ok(None);
        }
        if !data.movable() {
            data.apart(&data.current())?;
            return Ok(None);
        }
        resolve(&self.data_target()).and_then(|to| data::plan(&data, &to))
    }

    fn describe_data(&self) -> Result<String, String> {
        let data = self.data_for();
        let to = resolve(&self.data_target())?;
        if !data.movable() {
            data.apart(&data.current())?;
        }
        data.describe(&to)
    }

    fn start(&mut self, ctx: &egui::Context) {
        let root = match resolve(&self.folder) {
            Ok(root) => root,
            Err(e) => {
                *self.stage.lock().unwrap() = Stage::Failed(e);
                return;
            }
        };
        if let Err(e) = self.existing.planned(&root) {
            *self.stage.lock().unwrap() = Stage::Failed(e);
            return;
        }
        self.folder = root.display().to_string();
        if let Some(why) = running::game_block() {
            *self.stage.lock().unwrap() = Stage::Failed(why.into());
            return;
        }
        let open = running::from(&root);
        match self.data_move() {
            Err(e) => {
                *self.stage.lock().unwrap() = Stage::Failed(e);
                return;
            }
            Ok(Some(_)) => {
                let elsewhere: Vec<running::Running> = running::anywhere()
                    .into_iter()
                    .filter(|r| !open.contains(r))
                    .collect();
                if let Some(why) = running::holding(&elsewhere, false) {
                    *self.stage.lock().unwrap() = Stage::Failed(why);
                    return;
                }
            }
            Ok(None) => {}
        }
        if !open.is_empty() {
            *self.stage.lock().unwrap() = Stage::Open(root, running::describe(&open));
            return;
        }
        self.run(root, false, ctx);
    }

    fn run(&mut self, root: PathBuf, close: bool, ctx: &egui::Context) {
        let moving = match self.existing.planned(&root) {
            Ok(moving) => moving,
            Err(e) => {
                *self.stage.lock().unwrap() = Stage::Failed(e);
                return;
            }
        };
        let data_move = match self.data_move() {
            Ok(data_move) => data_move,
            Err(e) => {
                *self.stage.lock().unwrap() = Stage::Failed(e);
                return;
            }
        };
        let reporter = Reporter {
            stage: self.stage.clone(),
            ctx: ctx.clone(),
        };
        reporter.report(0.0, if close { running::CLOSING } else { FINDING });
        let desktop = self.desktop;
        let adopt = self
            .adopting()
            .map(|found| (self.data_for(), found, self.existing.roots().to_vec()));
        std::thread::spawn(move || {
            let data_env = data_move.as_ref().and_then(|m| m.env()).map(Path::to_path_buf);
            let stage = match running::clear(&root, close, &reporter)
                .and_then(|()| match &adopt {
                    Some((data, found, installs)) => data::adopt(data, found, installs),
                    None => Ok(()),
                })
                .and_then(|()| match &data_move {
                    Some(m) => m.run(&data::Ops::system(), &reporter),
                    None => Ok(()),
                })
                .and_then(|()| install(&root, desktop, moving.as_ref(), &reporter))
                .and_then(|exe| launch(&exe, data_env.as_deref()))
            {
                Ok(()) => Stage::Launched,
                Err(e) => Stage::Failed(e),
            };
            reporter.set(stage);
        });
    }
}

#[cfg(test)]
mod tests;
