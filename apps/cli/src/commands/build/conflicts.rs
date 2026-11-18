use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::Result;
use swofty_core::store::Store;

pub fn conflicts(store: &Store, profile: &str, check: bool, game: Option<PathBuf>) -> Result<ExitCode> {
    let report = match check {
        true => swofty_core::profile::conflicts::check(store, profile, &super::game(store, game)?)?,
        false => match swofty_core::profile::conflicts::last(store, profile) {
            Some(report) => report,
            None => {
                println!("{profile} has not been built yet; run swofty conflicts --check");
                return Ok(ExitCode::SUCCESS);
            }
        },
    };
    let names: std::collections::BTreeMap<String, String> = store
        .registry()?
        .manifests()
        .into_iter()
        .map(|(id, m)| (id, m.name))
        .collect();
    print!("{}", conflicts_text(&report, &names));
    Ok(ExitCode::SUCCESS)
}

pub(super) fn conflicts_text(
    report: &swofty_core::profile::conflicts::Report,
    names: &std::collections::BTreeMap<String, String>,
) -> String {
    if report.pairs.is_empty() {
        return format!("{}: no addon overrides another\n", report.profile);
    }
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let mut out = String::new();
    for pair in &report.pairs {
        out.push_str(&pair.line(&name));
        out.push('\n');
        for item in &pair.items {
            out.push_str(&format!("  {item}\n"));
        }
        let more =
            (pair.database + pair.resources + pair.files + pair.strings).saturating_sub(pair.items.len());
        if more > 0 && pair.items.len() >= swofty_core::profile::conflicts::MAX_ITEMS {
            out.push_str(&format!("  and {more} more\n"));
        }
    }
    out
}
