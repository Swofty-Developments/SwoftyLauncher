use anyhow::{Result, bail};
use swofty_core::saves::plain::Plain;
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
            row.container,
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
    container: &str,
    generation: Option<u64>,
    source: Option<&str>,
) -> Result<()> {
    let plain = plain_of(store, source)?;
    let generation = match generation {
        Some(g) => g,
        None => match plain
            .generations()
            .iter()
            .find(|m| m.is_session_save() && m.container(container).is_some())
        {
            Some(m) => m.generation,
            None => bail!(
                "no saved generation in {} holds {container}",
                plain.root.display()
            ),
        },
    };
    let r = plain.restore_container(generation, container, vault::now())?;
    println!(
        "asked the core to restore {container} from generation {generation} at the next Play ({}); `swofty saves plain` shows its answer afterwards",
        r.id
    );
    Ok(())
}
