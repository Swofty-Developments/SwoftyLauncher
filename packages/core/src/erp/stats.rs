use swofty_formats::erp::{ErpFile, Packer, Resource};

use super::ErpBuild;
use super::pack;
use crate::Result;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StatsRepair {
    pub repaired: Vec<(String, String)>,
    pub broken: Vec<(String, String)>,
}

pub fn stale_stats_in(file: &ErpFile) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for r in file.erp.resources.iter().filter(|r| r.has_stats()) {
        if file.load(r)?.stale_stats()? {
            out.push(r.name.clone());
        }
    }
    Ok(out)
}

// The resources of `file` with stale TSMD stats: rewritten, by index, and the names
// of those whose stats cannot be rewritten.
pub type ArchiveRepair = (Vec<(usize, Resource)>, Vec<String>);

pub fn repair_stats_in(file: &ErpFile, packer: &Packer) -> Result<ArchiveRepair> {
    let (mut fixed, mut broken) = (Vec::new(), Vec::new());
    for (i, r) in file
        .erp
        .resources
        .iter()
        .enumerate()
        .filter(|(_, r)| r.has_stats())
    {
        let mut res = file.load(r)?;
        match pack::repair_stats(packer, &mut res) {
            Ok(true) => fixed.push((i, res)),
            Ok(false) => {}
            Err(_) => broken.push(r.name.clone()),
        }
    }
    Ok((fixed, broken))
}

fn file_name(uri: &str) -> &str {
    let path = uri.split('?').next().unwrap_or(uri);
    path.rsplit('/').next().unwrap_or(path)
}

pub fn repaired_note(who: &str, resources: &[String]) -> String {
    let what = match resources.iter().all(|r| r.ends_with(".aispline")) {
        true => "AI racing line(s)",
        false => "file(s)",
    };
    format!(
        "fixed the size headers of {} {what} in {who} so F1 25 reads them safely",
        resources.len()
    )
}

pub fn unrepairable_message(who: &str, resources: &[String], fix: &str) -> String {
    let first = resources.first().map_or("", |r| file_name(r));
    let what = match resources.len() {
        1 => format!(
            "its {first} doesn't match the size header the game reads with it, and the launcher can't read the file to fix the header"
        ),
        n => format!(
            "its {first} and {} more file(s) don't match the size headers the game reads with them, and the launcher can't read the files to fix the headers",
            n - 1
        ),
    };
    format!("{who} would crash F1 25: {what}. {fix}")
}

impl ErpBuild<'_> {
    // Rewrites the TSMD stats of every resource the ops left with a bxml fragment its
    // stats no longer describe, with the origin that last wrote each.
    pub fn repair_stats(&mut self) -> StatsRepair {
        let packer = self.packer.clone();
        let mut out = StatsRepair::default();
        for (rel, archive) in &mut self.erps {
            for r in archive.written_mut() {
                let origin = self.writers.get(&(rel.clone(), r.name.clone()));
                let at = (origin.cloned().unwrap_or_default(), r.name.clone());
                match pack::repair_stats(&packer, r) {
                    Ok(true) => out.repaired.push(at),
                    Ok(false) => {}
                    Err(_) => out.broken.push(at),
                }
            }
        }
        out
    }
}
