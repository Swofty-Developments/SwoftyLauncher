use anyhow::{Result, bail};
use swofty_core::saves::launcher as launcher_saves;
use swofty_core::saves::vault::restore::{self, Running};
use swofty_core::saves::vault::{self, Location, Meta, Reason, Taken, Vault, migrate};
use swofty_core::store::Store;

use crate::commands::saves::session::report;
use crate::commands::saves::{PINNED, open};
use crate::console::output::say;

pub(super) fn pin_snapshot(store: &Store, id: &str, source: Option<&str>, pin: bool) -> Result<()> {
    let (v, _) = open(store, source)?;
    let s = match pin {
        true => v.pin(id, PINNED, vault::now())?,
        false => v.unpin_why(id, PINNED, vault::now())?,
    };
    match (pin, s.pinned.is_empty()) {
        (true, _) => println!("pinned snapshot {}", s.id),
        (false, true) => println!("unpinned snapshot {}", s.id),
        (false, false) => println!(
            "unpinned snapshot {}; it stays kept for {}",
            s.id,
            s.pinned.join(", ")
        ),
    }
    Ok(())
}

pub(super) fn backup(store: &Store) -> Result<()> {
    migrate::migrate(&store.root).map(report).ok();
    let Some(loc) = Location::active(&store.root) else {
        bail!("no F1 25 profile save found to back up");
    };
    let v = Vault::of(&store.root, &loc)?;
    let profile = store.active_profile();
    let meta = Meta::new(Reason::Manual).profile(profile).pin("backup");
    match v.check(&loc, meta, vault::now())?.taken {
        Taken::New(s) => println!("saved and pinned snapshot {} ({} bytes)", s.id, s.total()),
        Taken::Unchanged(s) => println!("the profile save is unchanged since snapshot {}; pinned it", s.id),
        Taken::NoSave => bail!("no profile save in {} yet", loc.dir.display()),
    }
    Ok(())
}

pub(super) fn restore_snapshot(store: &Store, id: &str, source: Option<&str>) -> Result<()> {
    let (v, loc) = open(store, source)?;
    let snap = v.find(id)?;
    let loc = match loc {
        Some(l) => l,
        None => Location {
            source: snap.source.clone(),
            layout: snap.layout,
            dir: snap.save_dir.clone(),
        },
    };
    let r = restore::restore(&v, id, &loc, Running::now(), vault::now())?;
    say("info", &r.log_line());
    if let Some(b) = &r.before {
        println!("the save it replaced is snapshot {b}");
    }
    println!("restored {id} into {}", r.dir.display());
    if !r.kept.is_empty() {
        println!("left as they are (not in the snapshot): {}", r.kept.join(", "));
    }
    if loc.layout.is_launcher() {
        println!("the launcher saves are restored; Steam's saves are not touched");
    }
    if loc.layout == vault::Layout::Steam {
        println!(
            "start Steam to upload the restored save; if it asks which copy to keep, keep the local files (this computer)"
        );
    }
    Ok(())
}

fn profile_arg(store: &Store, profile: Option<String>) -> Result<String> {
    let p = profile.unwrap_or_else(|| store.active_profile());
    let loaded = store.profile(&p)?;
    if loaded.is_vanilla() {
        bail!("{p} plays without the Swofty core and always uses Steam's saves");
    }
    Ok(p)
}

pub(super) fn import_saves(
    store: &Store,
    from: Option<String>,
    from_profile: Option<String>,
    profile: Option<String>,
) -> Result<()> {
    let profile = profile_arg(store, profile)?;
    let (src, reason) = match (from.as_deref(), from_profile) {
        (Some(other), _) if other != "steam" => {
            bail!("--from takes steam; use --from-profile for another profile")
        }
        (_, Some(p)) => {
            if p == profile {
                bail!("{p} is the profile to import into");
            }
            store.profile(&p)?;
            (launcher_saves::location(&store.root, &p), Reason::ImportedProfile)
        }
        _ => match Location::active(&store.root) {
            Some(l) if l.layout == vault::Layout::Steam => (l, Reason::Imported),
            Some(_) => bail!("{}", launcher_saves::EA_NOTE),
            None => bail!("no Steam save of F1 25 found to import"),
        },
    };
    let (files, bytes, snapshot) =
        launcher_saves::import(&store.root, &profile, &src, reason, Running::now(), vault::now())?;
    say(
        "info",
        &format!(
            "saves: imported {files} file(s) from {} into the launcher saves of profile {profile}",
            src.dir.display()
        ),
    );
    println!(
        "imported {files} file(s), {bytes} bytes, from {} into {}",
        src.dir.display(),
        launcher_saves::remote_dir(&store.root, &profile).display()
    );
    if let Some(s) = snapshot {
        println!("the vault holds it as snapshot {s}");
    }
    Ok(())
}

pub(super) fn export_saves(store: &Store, to: &str, profile: Option<String>) -> Result<()> {
    if to != "steam" {
        bail!("--to takes steam");
    }
    let profile = profile_arg(store, profile)?;
    let steam = match Location::active(&store.root) {
        Some(l) if l.layout == vault::Layout::Steam => l,
        Some(_) => bail!("{}", launcher_saves::EA_NOTE),
        None => bail!("no Steam save folder of F1 25 found to export to"),
    };
    let r = launcher_saves::export(&store.root, &profile, &steam, Running::now(), vault::now())?;
    say("info", &r.log_line());
    if let Some(b) = &r.before {
        println!("the Steam save it replaced is snapshot {b}");
    }
    println!(
        "exported the launcher saves of profile {profile} into {}",
        r.dir.display()
    );
    println!(
        "start Steam to upload the exported save; if it asks which copy to keep, keep the local files (this computer)"
    );
    Ok(())
}
