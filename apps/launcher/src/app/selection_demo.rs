//! An isolated native preview: confirmation only updates text, never files.
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::{TitleBar, v_flex};
use gpui_kit::*;
use swofty_core::view::{SelectionDialog, SelectionOption};
use swofty_i18n::{tr, trf};

use crate::dialogs::selection;
use crate::ui::scale::px;
use crate::ui::theme::{self, palette};

pub(crate) fn run() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            theme::init(cx);
            super::dialog_keys::init(cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1000.), px(760.)),
                    cx,
                ))),
                window_min_size: Some(size(px(600.), px(500.))),
                app_id: Some("net.swofty.SelectionDialogsDemo".into()),
                titlebar: Some(TitlebarOptions {
                    title: Some(tr("Swofty — Selection dialogs").into()),
                    ..TitleBar::title_bar_options()
                }),
                ..TitleBar::window_options()
            };
            let (main, _) = gpui_kit::open_window(options, cx, |_, cx| {
                cx.new(|_| Demo {
                    result: tr("No selection submitted").into(),
                })
            })
            .expect("open selection preview");
            cx.on_window_closed(move |cx, id| {
                if id == main.window_id() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}

struct Demo {
    result: String,
}

fn options(values: &[(&str, &'static str)]) -> Vec<SelectionOption> {
    values
        .iter()
        .map(|(value, label)| SelectionOption {
            value: (*value).into(),
            label: tr(label).into(),
            group: None,
            help: None,
        })
        .collect()
}

fn spec(kind: usize) -> SelectionDialog {
    match kind {
        0 => SelectionDialog {
            title: tr("Delete recordings?").into(),
            text: tr("Choose the qualifying recordings to delete. In an addon, this action permanently deletes their recorded data. This preview only displays your selection.").into(),
            options: options(&[("q1", "Qualifying 1"), ("q2", "Qualifying 2"), ("q3", "Qualifying 3")]),
            multiple: true,
            apply: Some(tr("Delete selected").into()),
            ..Default::default()
        },
        1 => SelectionDialog {
            title: tr("Drivers").into(),
            text: tr("Choose the drivers to show in the charts.").into(),
            options: (0..24).map(|i| SelectionOption { value: i.to_string(), label: trf!("Driver {number}", number = i + 1), group: Some(trf!("Team {number}", number = i / 2 + 1)), help: None }).collect(),
            selected: vec!["0".into(), "1".into()],
            multiple: true,
            allow_empty: true,
            ..Default::default()
        },
        _ => SelectionDialog {
            title: tr("Export recording").into(),
            text: tr("Choose what to export.").into(),
            options: options(&[("raw", "Raw CSV data"), ("visuals", "Visuals"), ("both", "Data and visuals")]),
            selected: vec!["raw".into()],
            apply: Some(tr("Export").into()),
            ..Default::default()
        },
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        v_flex()
            .size_full()
            .bg(p.background)
            .text_color(p.text)
            .child(TitleBar::new().bg(p.chrome))
            .child(
                v_flex()
                    .p(px(32.))
                    .gap(px(18.))
                    .child(div().text_size(px(24.)).child(tr("Addon selection dialogs")))
                    .child(tr(
                        "This isolated preview opens no game, profile or application store.",
                    ))
                    .children(
                        [tr("Delete recordings…"), tr("Drivers…"), tr("Export…")]
                            .into_iter()
                            .enumerate()
                            .map(|(kind, label)| {
                                Button::new(("selection-demo", kind))
                                    .label(label)
                                    .on_click(cx.listener(move |_, _, window, cx| {
                                        let demo = cx.entity().downgrade();
                                        selection::open(
                                            spec(kind),
                                            kind == 0,
                                            Rc::new(move |values, _, cx| {
                                                demo.update(cx, |this, cx| {
                                                    this.result = trf!(
                                                        "{action}: {selection}",
                                                        action = [
                                                            tr("Delete selected"),
                                                            tr("Drivers"),
                                                            tr("Export")
                                                        ][kind],
                                                        selection = values.join(", ")
                                                    );
                                                    cx.notify();
                                                })
                                                .is_ok()
                                            }),
                                            window,
                                            cx,
                                        )
                                        .expect("valid sample selection");
                                    }))
                            }),
                    )
                    .child(self.result.clone()),
            )
    }
}
