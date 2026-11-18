//! The addon rows of the era picker and the addon boxes of the car category screen.

use std::path::{Path, PathBuf};

use swofty_formats::erp::Packer;

use super::archives::{read_frag, write_archive};
use super::{Writer, ui_file};
use super::{car_category, check, era};
use crate::{Error, Result, io_err};

pub(super) fn era_rows(
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    packer: &Packer,
) -> Result<Vec<&'static str>> {
    let world = input(&ui_file(era::ARCHIVES[0]));
    let title = read_frag(&world, era::TITLE_SCENE, 0)?;
    let mut added = Vec::new();
    if era::ROW_FADE {
        let image = format!("{}.image", era::FADE_SOURCE);
        let fade = era::fade(
            &read_frag(&world, era::FADE_SOURCE, 0)?,
            &read_frag(&world, &image, 0)?,
            era::FADE,
        )?;
        added.push((era::FADE.to_owned(), era::SRV_KIND, vec![fade[0].clone()]));
        added.push((
            format!("{}.image", era::FADE),
            era::SURFACE_KIND,
            fade[1..].to_vec(),
        ));
    }
    let mut written = Vec::new();
    for rel in era::ARCHIVES {
        let (src, dst) = (input(&ui_file(rel)), out.join(ui_file(rel)));
        let ui = read_frag(&src, era::UI, era::UI_FRAGMENT)?;
        let scene = read_frag(&src, era::SCENE, 0)?;
        let edit = era::taken(&ui, &scene).and_then(|taken| match taken {
            true => Ok(None),
            false => Ok(Some((era::build_ui(&ui)?, era::build_scene(&scene, &title)?))),
        });
        let edit = match edit {
            Ok(edit) => edit,
            Err(e) => {
                let by = [
                    (rel, era::UI),
                    (rel, era::SCENE),
                    (era::ARCHIVES[0], era::TITLE_SCENE),
                ]
                .into_iter()
                .find_map(|(erp, res)| writer(&ui_file(erp), res));
                let Some(addon) = by else { return Err(e) };
                warnings.push(format!(
                    "ui: {addon} has its own copy of the era picker in {rel}, which Swofty can't edit; the era picker boxes there are off while it's on ({e})"
                ));
                continue;
            }
        };
        let Some((ui, scene)) = edit else {
            lines.push(format!(
                "ui: the era picker in {rel} already has an Addons row from another addon; left as it is"
            ));
            continue;
        };
        write_archive(
            &src,
            &dst,
            vec![
                (era::UI, era::UI_FRAGMENT, Box::new(move |_| Ok(ui))),
                (era::SCENE, 0, Box::new(move |_| Ok(scene))),
            ],
            added.clone(),
            packer,
        )?;
        written.push(rel);
    }
    if !written.is_empty() {
        let files: Vec<(String, PathBuf, PathBuf)> = written
            .iter()
            .map(|rel| (ui_file(rel), input(&ui_file(rel)), out.join(ui_file(rel))))
            .collect();
        let pairs: Vec<(&str, &Path, &Path)> = files
            .iter()
            .map(|(name, src, dst)| (name.as_str(), src.as_path(), dst.as_path()))
            .collect();
        let report = check::archive_files(&pairs)?;
        if !report.hard.is_empty() {
            let found: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
            return Err(Error::Other(format!(
                "the era picker rows break the game's UI loader rules: {}",
                found.join("; ")
            )));
        }
        lines.push(format!(
            "ui: the Grand Prix and Time Trial era picker gets Built In and Addons rows in {} archive(s)",
            written.len()
        ));
    }
    Ok(written)
}

/// Gives the LAN Car Category screen a row of addon boxes. An archive the era picker rows already
/// rewrote is read from that output. A failure leaves the screen stock and is a warning: the boxes
/// are an extra on a screen most players never open.
pub(super) fn car_category_boxes(
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    eras: &[&'static str],
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    packer: &Packer,
) -> Result<Vec<&'static str>> {
    let mut written = Vec::new();
    for rel in car_category::ARCHIVES {
        let dst = out.join(ui_file(rel));
        let src = if eras.contains(&rel) {
            dst.clone()
        } else {
            input(&ui_file(rel))
        };
        if let Some(addon) = [car_category::UI, car_category::SCENE]
            .into_iter()
            .find_map(|res| writer(&ui_file(rel), res))
        {
            lines.push(format!(
                "ui: {addon} has its own copy of the Car Category screen in {rel}; left as it is"
            ));
            continue;
        }
        let edit = (|| -> Result<Option<(Vec<u8>, Vec<u8>)>> {
            let ui = read_frag(&src, car_category::UI, car_category::UI_FRAGMENT)?;
            let scene = read_frag(&src, car_category::SCENE, 0)?;
            if car_category::taken(&ui, &scene)? {
                return Ok(None);
            }
            Ok(Some((
                car_category::build_ui(&ui)?,
                car_category::build_scene(&scene)?,
            )))
        })();
        let (ui, scene) = match edit {
            Ok(Some(edit)) => edit,
            Ok(None) => continue,
            Err(e) => {
                warnings.push(format!(
                    "ui: the Car Category screen in {rel} keeps its two boxes ({e})"
                ));
                continue;
            }
        };
        let tmp = dst.with_extension("erp.car-category");
        write_archive(
            &src,
            &tmp,
            vec![
                (
                    car_category::UI,
                    car_category::UI_FRAGMENT,
                    Box::new(move |_| Ok(ui)),
                ),
                (car_category::SCENE, 0, Box::new(move |_| Ok(scene))),
            ],
            Vec::new(),
            packer,
        )?;
        swofty_formats::erp::replace(&tmp, &dst).map_err(io_err(&dst))?;
        written.push(rel);
    }
    if written.is_empty() {
        return Ok(written);
    }
    let files: Vec<(String, PathBuf, PathBuf)> = written
        .iter()
        .map(|rel| (ui_file(rel), input(&ui_file(rel)), out.join(ui_file(rel))))
        .collect();
    let pairs: Vec<(&str, &Path, &Path)> = files
        .iter()
        .map(|(name, src, dst)| (name.as_str(), src.as_path(), dst.as_path()))
        .collect();
    let report = check::archive_files(&pairs)?;
    if !report.hard.is_empty() {
        let found: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
        return Err(Error::Other(format!(
            "the Car Category addon boxes break the game's UI loader rules: {}",
            found.join("; ")
        )));
    }
    lines.push(format!(
        "ui: the Car Category screen gets {} addon box slot(s) in {} archive(s)",
        car_category::SLOTS,
        written.len()
    ));
    Ok(written)
}
