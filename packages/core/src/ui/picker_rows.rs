//! The addon rows of the era picker and the addon boxes of the car category screen.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::archives::{Added, ArchiveWriter, read_frag, write_archive};
use super::check::ArchiveUi;
use super::{Patch, Writer, ui_file};
use super::{car_category, check, era};
use crate::{Error, Result};

/// A fragment an edit rewrites: its resource, its index and the bytes it gets.
type Edit = (&'static str, usize, Vec<u8>);

/// The fragments an edit rewrites, by archive.
type Edits = Vec<(&'static str, Vec<Edit>)>;

/// The era picker's Built In and Addons rows, while `split` (SwoftyMod's Split the season picker) is on, and the
/// Car Category screen's addon boxes, with the archives they wrote. The two edit different resources, so each archive
/// is written once, from `input`, with the edits of both.
pub(super) fn picker_rows(
    split: bool,
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    archives: &ArchiveWriter,
) -> Result<Vec<&'static str>> {
    if !split {
        lines.push(
            "ui: the season picker keeps the game's single row (SwoftyMod's Split the season picker is off)"
                .into(),
        );
    }
    let which = if split { Rows::Both } else { Rows::Cars };
    rows(which, input, writer, out, lines, warnings, archives)
}

/// The era picker rows alone.
#[cfg(test)]
pub(super) fn era_rows(
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    archives: &ArchiveWriter,
) -> Result<Vec<&'static str>> {
    rows(Rows::Era, input, writer, out, lines, warnings, archives)
}

/// The Car Category boxes alone.
#[cfg(test)]
pub(super) fn car_category_boxes(
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    archives: &ArchiveWriter,
) -> Result<Vec<&'static str>> {
    rows(Rows::Cars, input, writer, out, lines, warnings, archives)
}

/// Which of the two edits a [`rows`] call makes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rows {
    Both,
    #[cfg_attr(not(test), allow(dead_code))]
    Era,
    Cars,
}

fn rows(
    which: Rows,
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    out: &Path,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
    archives: &ArchiveWriter,
) -> Result<Vec<&'static str>> {
    let (eras, added) = match which {
        Rows::Cars => (Vec::new(), Vec::new()),
        Rows::Both | Rows::Era => era_edits(input, writer, lines, warnings)?,
    };
    let mut car_lines = Vec::new();
    let cars = match which {
        Rows::Era => Vec::new(),
        Rows::Both | Rows::Cars => car_category_edits(input, writer, &mut car_lines, warnings),
    };
    let era_rels: Vec<&'static str> = eras.iter().map(|(rel, _)| *rel).collect();
    let car_rels: Vec<&'static str> = cars.iter().map(|(rel, _)| *rel).collect();
    let mut per_archive: Vec<(&'static str, Vec<Edit>, Vec<Added>)> = eras
        .into_iter()
        .map(|(rel, edits)| (rel, edits, added.clone()))
        .collect();
    for (rel, edits) in cars {
        match per_archive.iter_mut().find(|(r, ..)| *r == rel) {
            Some((_, have, _)) => have.extend(edits),
            None => per_archive.push((rel, edits, Vec::new())),
        }
    }
    let written: Vec<&'static str> = per_archive.iter().map(|(rel, ..)| *rel).collect();
    for (rel, edits, added) in per_archive {
        let patches: Vec<(&str, usize, Patch)> = edits
            .into_iter()
            .map(|(res, index, data)| (res, index, Box::new(move |_: &[u8]| Ok(data)) as Patch))
            .collect();
        write_archive(
            &input(&ui_file(rel)),
            &out.join(ui_file(rel)),
            patches,
            added,
            archives,
        )?;
    }
    check_rows(input, out, &era_rels, &car_rels)?;
    if !era_rels.is_empty() {
        lines.push(format!(
            "ui: the Grand Prix and Time Trial era picker gets Built In and Addons rows in {} archive(s)",
            era_rels.len()
        ));
    }
    lines.extend(car_lines);
    if !car_rels.is_empty() {
        lines.push(format!(
            "ui: the Car Category screen gets {} addon box slot(s) in {} archive(s)",
            car_category::SLOTS,
            car_rels.len()
        ));
    }
    Ok(written)
}

/// The era picker's edits, by archive, and the resources it adds to each.
fn era_edits(
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<(Edits, Vec<Added>)> {
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
    let mut edits = Vec::new();
    for rel in era::ARCHIVES {
        let src = input(&ui_file(rel));
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
        edits.push((rel, vec![(era::UI, era::UI_FRAGMENT, ui), (era::SCENE, 0, scene)]));
    }
    Ok((edits, added))
}

/// The LAN Car Category screen's row of addon boxes, by archive. The era picker leaves these resources as they are,
/// so they are read from `input` even where it edits the same archive. A failure leaves the screen stock and is a
/// warning: the boxes are an extra on a screen most players never open.
fn car_category_edits(
    input: &dyn Fn(&str) -> PathBuf,
    writer: &Writer,
    lines: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Edits {
    let mut edits = Vec::new();
    for rel in car_category::ARCHIVES {
        let src = input(&ui_file(rel));
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
        match edit {
            Ok(Some((ui, scene))) => edits.push((
                rel,
                vec![
                    (car_category::UI, car_category::UI_FRAGMENT, ui),
                    (car_category::SCENE, 0, scene),
                ],
            )),
            Ok(None) => {}
            Err(e) => warnings.push(format!(
                "ui: the Car Category screen in {rel} keeps its two boxes ({e})"
            )),
        }
    }
    edits
}

/// Checks the era picker's archives and the Car Category screen's against the game's UI loader rules, each as if it
/// were the only edit to them: the era picker's view of an archive both edit keeps the Car Category resources stock.
/// The UI of each archive is read once from `input` and once from `out`.
fn check_rows(
    input: &dyn Fn(&str) -> PathBuf,
    out: &Path,
    eras: &[&'static str],
    cars: &[&'static str],
) -> Result<()> {
    let mut stock = BTreeMap::new();
    let mut edited = BTreeMap::new();
    for rel in eras.iter().chain(cars) {
        if !stock.contains_key(rel) {
            stock.insert(*rel, ArchiveUi::read(&ui_file(rel), &input(&ui_file(rel)))?);
            edited.insert(*rel, ArchiveUi::read(&ui_file(rel), &out.join(ui_file(rel)))?);
        }
    }
    let era_view = |rel: &&'static str| {
        let mut ui = edited[rel].clone();
        if cars.contains(rel) {
            for res in [car_category::UI, car_category::SCENE] {
                match stock[rel].resources.get(res) {
                    Some(r) => ui.resources.insert(res.to_owned(), r.clone()),
                    None => ui.resources.remove(res),
                };
            }
        }
        ui
    };
    let before =
        |rels: &[&'static str]| -> Vec<ArchiveUi> { rels.iter().map(|rel| stock[rel].clone()).collect() };
    if !eras.is_empty() {
        let after: Vec<ArchiveUi> = eras.iter().map(era_view).collect();
        loader_rules(&after, &before(eras), "the era picker rows")?;
    }
    if !cars.is_empty() {
        let after: Vec<ArchiveUi> = cars.iter().map(|rel| edited[rel].clone()).collect();
        loader_rules(&after, &before(cars), "the Car Category addon boxes")?;
    }
    Ok(())
}

/// Fails with the rules `after` breaks, which `what` made of `before`.
fn loader_rules(after: &[ArchiveUi], before: &[ArchiveUi], what: &str) -> Result<()> {
    let report = check::archives(
        after,
        before,
        crate::ui::catalog::stock(),
        crate::ui::catalog::engine(),
    )?;
    if report.hard.is_empty() {
        return Ok(());
    }
    let found: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    Err(Error::Other(format!(
        "{what} break the game's UI loader rules: {}",
        found.join("; ")
    )))
}
