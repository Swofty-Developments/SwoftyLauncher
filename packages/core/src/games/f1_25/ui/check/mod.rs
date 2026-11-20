use swofty_formats::pkg::Pkg;

use super::catalog::{Catalog, Engine, Scenes};
use super::idf;

mod archives;
mod findings;
mod flows;
mod integrity;
mod loader;
mod props;
mod stock;

pub use archives::{ArchiveUi, archive_files, archives, scene_bindings};
pub use findings::{Finding, Level, Report};
pub use flows::{flow_loading, route_files, routes, screens, stock_kept};
use integrity::integrity_findings;
pub use integrity::{STALE_HASH, integrity};
pub use loader::loader_findings;
pub use props::{dropped_props, runtime_bindings};
pub use stock::{against_stock, scene_against_stock};

const PARSED_KEPT: usize = 8;

thread_local! {
    static PARSED: std::cell::RefCell<Vec<(Vec<u8>, std::rc::Rc<Pkg>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `data` parsed. A file the checks read again, as each of them parses what it checks, and one several
/// archives carry, is parsed once while it is among the last [`PARSED_KEPT`] read.
pub(super) fn parsed(data: &[u8]) -> swofty_formats::Result<std::rc::Rc<Pkg>> {
    PARSED.with(|kept| {
        let mut kept = kept.borrow_mut();
        if let Some(at) = kept.iter().position(|(d, _)| d == data) {
            let hit = kept.remove(at);
            let pkg = hit.1.clone();
            kept.push(hit);
            return Ok(pkg);
        }
        let pkg = std::rc::Rc::new(Pkg::parse(data)?);
        if kept.len() >= PARSED_KEPT {
            kept.remove(0);
        }
        kept.push((data.to_vec(), pkg.clone()));
        Ok(pkg)
    })
}

#[derive(Clone, Copy)]
pub struct UiInput<'a> {
    pub resource: &'a str,
    pub frag: &'a str,
    pub data: &'a [u8],
    pub stock: Option<&'a [u8]>,
}

pub fn ui_edit(input: &UiInput, scenes: &Scenes, cat: &Catalog, engine: &Engine) -> crate::Result<Report> {
    let mut report = Report::default();
    let frag = input.frag;
    let mut found = integrity_findings(input.data, frag)?;
    found.extend(loader_findings(
        input.resource,
        frag,
        input.data,
        input.stock,
        cat,
        engine,
    )?);
    found.extend(against_stock(input.data, cat, scenes)?.into_iter().map(|mut f| {
        f.at = format!("{frag}: {}", f.at);
        f
    }));
    if let Some(s) = input.stock {
        found.extend(runtime_bindings(s, input.data, frag)?);
        found.extend(dropped_props(s, input.data, frag, engine)?);
    }
    for f in found {
        report.add(f);
    }
    Ok(report)
}

pub fn scene_edit(resource: &str, data: &[u8], cat: &Catalog) -> crate::Result<Report> {
    let mut report = Report::default();
    let p = parsed(data)?;
    let format = "format: the game finds a scene by its componentId, md5-64 of its eaid path, and an element by its leaf name through nodeidlookup";
    for m in idf::xref_mismatches(&p)? {
        let (at, msg) = m.split_once(": ").unwrap_or((&m, ""));
        report.add(Finding::hard(
            "xref-mismatch",
            at,
            msg.to_owned(),
            "format: the game loads the scene an eXRef's xref id names",
        ));
    }
    if resource.ends_with(".idf") {
        let want = idf::scene_id(resource);
        for entry in idf::scenes(&p)? {
            if idf::scene_name(entry) != Some(want) {
                report.add(Finding::hard(
                    "scene-id",
                    resource,
                    format!("the scene's name is not {want:#018x}, the componentId of its path"),
                    format,
                ));
            }
        }
    }
    for problem in idf::id_problems(&p)? {
        if problem == "nodeidlookup" {
            report.add(Finding::hard(
                "nodeidlookup",
                resource,
                "nodeidlookup is not the sorted djb2 of every element's leaf name, so layout attaches cannot find the missing elements".into(),
                format,
            ));
        } else {
            report.add(Finding::warn(
                "element-id",
                "",
                &problem,
                resource,
                format!("{problem} is not the djb2 of the element's dotted path"),
            ));
        }
    }
    for f in scene_against_stock(data, cat)? {
        report.add(f);
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
