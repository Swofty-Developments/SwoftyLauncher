use anyhow::{Result, bail};
use swofty_core::saves::plain::{Plain, mode_of};
use swofty_core::saves::vault;
use swofty_core::store::Store;

use super::open;

fn plain_of(store: &Store, source: Option<&str>) -> Result<Plain> {
    let (v, _) = open(store, source)?;
    Ok(Plain::of_source(&store.root, &v.source))
}

pub(super) fn careers_in_plain(store: &Store, source: Option<&str>) -> Result<()> {
    let plain = plain_of(store, source)?;
    let Some(newest) = plain.newest() else {
        println!("no plain copy of the saves in {} yet", plain.root.display());
        return Ok(());
    };
    println!(
        "generation {} ({} at {}), {} container(s)",
        newest.generation,
        match newest.event {
            swofty_core::saves::plain::Event::Load => "load",
            swofty_core::saves::plain::Event::Save => "save",
            swofty_core::saves::plain::Event::Other => "other",
        },
        vault::human(newest.written_at),
        newest.containers.len()
    );
    for row in plain.careers() {
        println!(
            "{}\t{}\t{}\t{}",
            row.mode,
            row.slot.guid,
            row.slot.name,
            vault::human(row.slot.last_played)
        );
    }
    if let Some(loss) = plain.loss(&[]) {
        println!(
            "missing from generation {} but in generation {}: {}",
            loss.newest,
            loss.good,
            loss.names().join("; ")
        );
    }
    for r in plain.pending() {
        println!(
            "waiting for the next Play: {} from generation {}",
            r.container, r.generation
        );
    }
    for r in plain.applying() {
        println!(
            "taken by a profile load that has not answered yet: {} from generation {}",
            r.container, r.generation
        );
    }
    for a in plain.answers() {
        println!(
            "the core answered {}: {} {}",
            a.id,
            if a.ok { "ok" } else { "failed" },
            a.detail
        );
    }
    Ok(())
}

pub(super) fn restore_career(
    store: &Store,
    mode: &str,
    generation: Option<u64>,
    source: Option<&str>,
) -> Result<()> {
    let plain = plain_of(store, source)?;
    let mode = mode_of(mode);
    let generation = match generation {
        Some(g) => g,
        None => match plain
            .generations()
            .iter()
            .find(|m| m.is_session_save() && m.halves(&mode).next().is_some())
        {
            Some(m) => m.generation,
            None => bail!(
                "no saved generation in {} holds mode {mode}",
                plain.root.display()
            ),
        },
    };
    let asked = plain.restore_mode(generation, &mode, vault::now())?;
    println!(
        "asked the core to restore {} from generation {generation} at the next Play; `swofty saves plain` shows its answer afterwards",
        asked
            .iter()
            .map(|r| r.container.as_str())
            .collect::<Vec<_>>()
            .join(" and ")
    );
    Ok(())
}

pub(super) fn remap_ids(
    plain: &std::path::Path,
    aliases: &std::path::Path,
    dry_run: bool,
    save: Option<&std::path::Path>,
) -> Result<()> {
    let aliases: swofty_core::row_ids::IdAliases = serde_json::from_slice(&std::fs::read(aliases)?)?;
    let lines = match dry_run {
        true => swofty_core::saves::plain::rewrite::dry_run(plain, &aliases)?,
        false => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let maps = swofty_core::saves::remap::IdMaps::from_aliases(&aliases);
            match Plain::at(plain).rewrite_for_aliases(&maps, None, now)? {
                swofty_core::saves::plain::rewrite::Outcome::Wrote(r) => {
                    r.careers.iter().map(|c| c.line()).collect()
                }
                other => vec![format!("{other:?}")],
            }
        }
    };
    for l in lines {
        println!("{l}");
    }
    if let (Some(path), true) = (save, dry_run) {
        let maps = swofty_core::saves::remap::IdMaps::from_aliases(&aliases);
        let file = swofty_core::saves::plain::rewrite::SaveFile { path, snapshot: None };
        for l in swofty_core::saves::plain::rewrite::map_save_file(file, &maps, false).lines {
            println!("{l}");
        }
    }
    Ok(())
}
