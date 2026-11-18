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

    fn data_section(&mut self, ui: &mut egui::Ui) {
        let install = resolve(&self.folder).ok();
        let current = self.data.current();
        match (self.adopting(), self.offer) {
            (Some(found), _) => {
                ui.label(
                    RichText::new(format!(
                        "Your addons and saves will come from {}.",
                        found.display()
                    ))
                    .color(MUTED),
                );
            }
            _ if self.data_target() == current.display().to_string() => {
                ui.label(
                    RichText::new(format!(
                        "Your addons and saves stay at {} ({}).",
                        current.display(),
                        addons_text(self.addons)
                    ))
                    .color(MUTED),
                );
            }
            _ => {}
        }
        if let Some(install) = install.as_ref().filter(|i| data::asks(i, &self.data)) {
            ui.add_space(8.0);
            ui.label(
                RichText::new("Your addons and saves are on another drive than the launcher.").color(TEXT),
            );
            ui.radio_value(&mut self.choice, data::Choice::Leave, "Leave them where they are");
            ui.radio_value(
                &mut self.choice,
                data::Choice::Move,
                format!("Move them to {} too", data::drive_name(install)),
            );
        }
        ui.add_space(8.0);
        egui::CollapsingHeader::new(RichText::new("Advanced").color(MUTED))
            .id_salt("advanced")
            .show(ui, |ui| {
                self.existing.advanced(ui, &self.folder);
                ui.add_space(4.0);
                ui.label(RichText::new("Folder for your addons and saves").color(MUTED));
                ui.add_enabled_ui(self.data.movable(), |ui| {
                    ui.horizontal(|ui| {
                        let browse = 84.0;
                        let width = ui.available_width() - browse - ui.spacing().item_spacing.x;
                        let mut shown = self.data_target();
                        let edited = ui
                            .add_sized(
                                [width, 30.0],
                                egui::TextEdit::singleline(&mut shown).align(egui::Align2::LEFT_CENTER),
                            )
                            .changed();
                        if edited {
                            self.choice = data::Choice::Leave;
                            self.data_folder = shown;
                        }
                        if ui
                            .add_sized([browse, 30.0], egui::Button::new("Browse…"))
                            .clicked()
                        {
                            self.choice = data::Choice::Leave;
                            self.browse_data();
                        }
                    });
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
            });
    }
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
        let (working, progress, error, open) = match &*self.stage.lock().unwrap() {
            Stage::Ready => (false, None, None, None),
            Stage::Open(root, why) => (true, None, None, Some((root.clone(), why.clone()))),
            Stage::Working { done, text } => (true, Some((*done, text.clone())), None, None),
            Stage::Failed(e) => (false, None, Some(e.clone()), None),
            Stage::Launched => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                (true, None, None, None)
            }
        };
        let bottom = egui::CentralPanel::default_margins()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(Margin::same(24)))
            .show(ui, |ui| {
                ui.label(RichText::new("Swofty Launcher").size(22.0).strong().color(TEXT));
                ui.label(RichText::new("Installs the latest release for F1 25 and opens it.").color(MUTED));
                ui.add_space(18.0);
                ui.add_enabled_ui(!working, |ui| {
                    self.found_offer(ui);
                    self.existing.list(ui, &mut self.folder);
                    ui.label(RichText::new("Install to").color(MUTED));
                    ui.horizontal(|ui| {
                        let browse = 84.0;
                        let width = ui.available_width() - browse - ui.spacing().item_spacing.x;
                        ui.add_sized(
                            [width, 30.0],
                            egui::TextEdit::singleline(&mut self.folder).align(egui::Align2::LEFT_CENTER),
                        );
                        if ui
                            .add_sized([browse, 30.0], egui::Button::new("Browse…"))
                            .clicked()
                        {
                            self.browse();
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new(running::placement(&self.folder)).color(MUTED));
                    ui.add_space(6.0);
                    ui.checkbox(&mut self.desktop, "Add a desktop shortcut");
                    self.existing.status(ui, &self.folder);
                    ui.add_space(14.0);
                    self.data_section(ui);
                });
                ui.add_space(18.0);
                if let Some((root, why)) = open {
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
                            self.run(root, true, &ctx);
                        }
                        if ui
                            .add_sized([cancel, 34.0], egui::Button::new("Cancel"))
                            .clicked()
                        {
                            *self.stage.lock().unwrap() = Stage::Ready;
                        }
                    });
                } else if let Some((done, text)) = progress {
                    ui.add(
                        egui::ProgressBar::new(done)
                            .desired_height(30.0)
                            .corner_radius(6)
                            .fill(ACCENT)
                            .text(RichText::new(text).color(ON_ACCENT)),
                    );
                } else if !working {
                    let label = if error.is_some() { "Try again" } else { "Install" };
                    let button =
                        egui::Button::new(RichText::new(label).size(15.0).color(ON_ACCENT)).fill(ACCENT);
                    if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
                        self.start(&ctx);
                    }
                }
                if let Some(error) = error {
                    ui.add_space(10.0);
                    ui.label(RichText::new(error).color(ERROR));
                }
                ui.min_rect().bottom()
            })
            .inner;
        let height = (bottom + 24.0).max(HEIGHT).round();
        if height != self.height {
            self.height = height;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(540.0, height)));
        }
    }
}
