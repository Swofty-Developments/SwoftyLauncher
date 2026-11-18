use std::path::PathBuf;

use eframe::egui::{self, CornerRadius, Margin, RichText, Stroke};

use crate::app::{Installer, Offer, Stage};
use crate::theme::{ACCENT, BACKGROUND, ERROR, MUTED, ON_ACCENT, SURFACE, TEXT};
use crate::{HEIGHT, data, resolve, running};

impl Installer {
    fn found_offer(&mut self, ui: &mut egui::Ui) {
        if self.offer != Offer::Ask {
            return;
        }
        let Some(found) = self.found.first() else {
            return;
        };
        let text = format!(
            "We found your addons and saves in {} ({}, {}). Use them?",
            found.path.display(),
            addons_text(found.addons),
            data::size(found.bytes),
        );
        egui::Frame::new()
            .fill(SURFACE)
            .stroke(Stroke::new(1.0, ACCENT))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.label(RichText::new(text).color(TEXT));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .add(egui::Button::new(RichText::new("Use them").color(ON_ACCENT)).fill(ACCENT))
                        .clicked()
                    {
                        self.offer = Offer::Use;
                    }
                    if ui.button("Start fresh").clicked() {
                        self.offer = Offer::Fresh;
                    }
                });
            });
        ui.add_space(14.0);
    }

    fn install_form(&mut self, ui: &mut egui::Ui) {
        self.found_offer(ui);
        self.existing.list(ui, &mut self.folder);
        ui.label(RichText::new("Install to").color(MUTED));
        let (_, browse) = folder_row(ui, &mut self.folder);
        if browse.clicked() {
            self.browse();
        }
        ui.add_space(4.0);
        ui.label(RichText::new(running::placement(&self.folder)).color(MUTED));
        ui.add_space(6.0);
        ui.checkbox(&mut self.desktop, "Add a desktop shortcut");
        self.existing.status(ui, &self.folder);
        ui.add_space(14.0);
        self.data_section(ui);
    }

    fn data_section(&mut self, ui: &mut egui::Ui) {
        self.data_source(ui);
        self.drive_choice(ui);
        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new("Advanced").color(MUTED))
            .id_salt("advanced")
            .show(ui, |ui| self.advanced(ui));
    }

    fn data_source(&self, ui: &mut egui::Ui) {
        let current = self.data.current();
        if let Some(found) = self.adopting() {
            ui.label(
                RichText::new(format!(
                    "Your addons and saves will come from {}.",
                    found.display()
                ))
                .color(MUTED),
            );
        } else if self.data_target() == current.display().to_string() {
            ui.label(
                RichText::new(format!(
                    "Your addons and saves stay at {} ({}).",
                    current.display(),
                    addons_text(self.addons)
                ))
                .color(MUTED),
            );
        }
    }

    fn drive_choice(&mut self, ui: &mut egui::Ui) {
        let Some(install) = resolve(&self.folder).ok().filter(|i| data::asks(i, &self.data)) else {
            return;
        };
        ui.add_space(8.0);
        ui.label(RichText::new("Your addons and saves are on another drive than the launcher.").color(TEXT));
        ui.radio_value(&mut self.choice, data::Choice::Leave, "Leave them where they are");
        ui.radio_value(
            &mut self.choice,
            data::Choice::Move,
            format!("Move them to {} too", data::drive_name(&install)),
        );
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        self.existing.advanced(ui, &self.folder);
        ui.add_space(4.0);
        ui.label(RichText::new("Folder for your addons and saves").color(MUTED));
        ui.add_enabled_ui(self.data.movable(), |ui| {
            let mut shown = self.data_target();
            let (edit, browse) = folder_row(ui, &mut shown);
            if edit.changed() {
                self.choice = data::Choice::Leave;
                self.data_folder = shown;
            }
            if browse.clicked() {
                self.choice = data::Choice::Leave;
                self.browse_data();
            }
        });
        ui.add_space(4.0);
        match self.describe_data() {
            Ok(text) => ui.label(RichText::new(text).color(MUTED)),
            Err(e) => ui.label(RichText::new(e).color(ERROR)),
        };
        if let Some(env) = &self.data.env {
            ui.label(
                RichText::new(format!(
                    "{} is set to {}. It decides where your addons and saves are.",
                    data::ENV,
                    env.display()
                ))
                .color(MUTED),
            );
        }
    }

    fn stage_row(&mut self, ui: &mut egui::Ui, stage: Stage, ctx: &egui::Context) {
        match stage {
            Stage::Ready => self.install_button(ui, "Install", ctx),
            Stage::Open(root, why) => self.close_prompt(ui, root, why, ctx),
            Stage::Working { done, text } => progress_bar(ui, done, text),
            Stage::Failed(error) => {
                self.install_button(ui, "Try again", ctx);
                ui.add_space(10.0);
                ui.label(RichText::new(error).color(ERROR));
            }
            Stage::Launched => {}
        }
    }

    fn close_prompt(&mut self, ui: &mut egui::Ui, root: PathBuf, why: String, ctx: &egui::Context) {
        ui.label(RichText::new(why).color(TEXT));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let cancel = 84.0;
            let width = ui.available_width() - cancel - ui.spacing().item_spacing.x;
            let close = egui::Button::new(
                RichText::new("Close Swofty Launcher and continue")
                    .size(15.0)
                    .color(ON_ACCENT),
            )
            .fill(ACCENT);
            if ui.add_sized([width, 34.0], close).clicked() {
                self.run(root, true, ctx);
            }
            if ui
                .add_sized([cancel, 34.0], egui::Button::new("Cancel"))
                .clicked()
            {
                *self.stage.lock().unwrap() = Stage::Ready;
            }
        });
    }

    fn install_button(&mut self, ui: &mut egui::Ui, label: &str, ctx: &egui::Context) {
        let button = egui::Button::new(RichText::new(label).size(15.0).color(ON_ACCENT)).fill(ACCENT);
        if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
            self.start(ctx);
        }
    }

    fn fit_height(&mut self, ctx: &egui::Context, bottom: f32) {
        let height = (bottom + 24.0).max(HEIGHT).round();
        if height != self.height {
            self.height = height;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(540.0, height)));
        }
    }
}

fn folder_row(ui: &mut egui::Ui, folder: &mut String) -> (egui::Response, egui::Response) {
    ui.horizontal(|ui| {
        let browse = 84.0;
        let width = ui.available_width() - browse - ui.spacing().item_spacing.x;
        let edit = ui.add_sized(
            [width, 30.0],
            egui::TextEdit::singleline(folder).align(egui::Align2::LEFT_CENTER),
        );
        let browse = ui.add_sized([browse, 30.0], egui::Button::new("Browse…"));
        (edit, browse)
    })
    .inner
}

fn progress_bar(ui: &mut egui::Ui, done: f32, text: String) {
    ui.add(
        egui::ProgressBar::new(done)
            .desired_height(30.0)
            .corner_radius(6)
            .fill(ACCENT)
            .text(RichText::new(text).color(ON_ACCENT)),
    );
}

pub(super) fn addons_text(n: usize) -> String {
    if n == 1 {
        "1 addon".to_owned()
    } else {
        format!("{n} addons")
    }
}

impl eframe::App for Installer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let stage = self.stage.lock().unwrap().clone();
        if matches!(stage, Stage::Launched) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let working = matches!(stage, Stage::Open(..) | Stage::Working { .. } | Stage::Launched);
        let bottom = egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(Margin::same(24)))
            .show(ui, |ui| {
                ui.label(RichText::new("Swofty Launcher").size(22.0).strong().color(TEXT));
                ui.label(RichText::new("Installs the latest release for F1 25 and opens it.").color(MUTED));
                ui.add_space(18.0);
                ui.add_enabled_ui(!working, |ui| self.install_form(ui));
                ui.add_space(18.0);
                self.stage_row(ui, stage, &ctx);
                ui.min_rect().bottom()
            })
            .inner;
        self.fit_height(&ctx, bottom);
    }
}
