use std::process::ExitCode;

use anyhow::Result;
use clap::Args;
use swofty_core::games::{self, Found, GameDef, Probe};
use swofty_core::store::Store;

use crate::commands::{Execute, Invocation};

#[derive(Args)]
pub(crate) struct GamesArgs {
    #[arg(long, help = "Print the games as JSON")]
    json: bool,
}

impl Execute for GamesArgs {
    fn execute(self, invocation: &Invocation) -> Result<ExitCode> {
        let text = list(&invocation.store, self.json)?;
        print!("{text}");
        Ok(ExitCode::SUCCESS)
    }
}

pub(crate) fn list(store: &Store, json: bool) -> Result<String> {
    let config = store.config();
    let probe = Probe::here(&config);
    let found: Vec<(&GameDef, Vec<Found>)> = games::ALL.iter().map(|d| (*d, d.find(&probe))).collect();
    Ok(match json {
        true => format!("{}\n", serde_json::to_string_pretty(&to_json(&found))?),
        false => text(&found),
    })
}

fn to_json(found: &[(&GameDef, Vec<Found>)]) -> serde_json::Value {
    found
        .iter()
        .map(|(d, copies)| {
            serde_json::json!({
                "id": d.info.id,
                "name": d.info.name,
                "support": d.info.support.id(),
                "found": copies.iter().map(|f| serde_json::json!({
                    "dir": f.dir,
                    "store": f.store.map(|s| s.id()),
                    "build_id": f.build_id,
                })).collect::<Vec<_>>(),
            })
        })
        .collect()
}

fn text(found: &[(&GameDef, Vec<Found>)]) -> String {
    let mut out = String::new();
    for (d, copies) in found {
        let support = match d.info.support {
            swofty_core::game::Support::Full => "supported",
            swofty_core::game::Support::NotYet => "not supported yet",
        };
        out.push_str(&format!("{}: {support}\n", d.info.name));
        if copies.is_empty() {
            out.push_str("  not found\n");
        }
        for f in copies {
            let store = f.store.map_or("Other", |s| s.name());
            out.push_str(&format!("  {store:10} {}\n", f.dir.display()));
        }
    }
    out
}
