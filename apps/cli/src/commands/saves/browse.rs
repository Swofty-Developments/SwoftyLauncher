use anyhow::Result;
use swofty_core::saves::vault::{self, Snapshot, Vault, migrate};
use swofty_core::store::Store;

use crate::commands::saves::open;
use crate::commands::saves::session::report;
use crate::console::output::say;

pub(super) fn mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

fn row(s: &Snapshot) -> String {
    let mut out = format!("{}  {}", s.id, s.reason.text());
    if let Some(p) = &s.profile {
        out.push_str(&format!(", profile {p}"));
    }
    if !s.absent_addons.is_empty() {
        out.push_str(&format!(", without {}", s.absent_addons.join(", ")));
    }
    out.push_str(&format!(", {} file(s), {} bytes", s.files.len(), s.total()));
    if !s.pinned.is_empty() {
        out.push_str(&format!(", pinned ({})", s.pinned.join(", ")));
    }
    out
}

pub(super) fn list(store: &Store, source: Option<&str>) -> Result<()> {
    if let Err(e) = migrate::migrate(&store.root).map(report) {
        say("warning", &format!("saves: {e}"));
    }
    let (v, loc) = match open(store, source) {
        Ok(x) => x,
        Err(e) if source.is_none() && Vault::sources(&store.root).is_empty() => {
            println!("no snapshots: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let snaps = v.list()?;
    println!(
        "vault {}: {} snapshot(s), {} of {}",
        v.dir.display(),
        snaps.len(),
        mb(v.usage()),
        mb(v.policy.cap_bytes)
    );
    if let Some(l) = &loc {
        match vault::save_status(l) {
            Ok(st) if st.files > 0 => println!(
                "save {}: {} file(s), {} bytes, Steam Cloud: {}",
                l.dir.display(),
                st.files,
                st.total,
                st.cloud.text()
            ),
            Ok(_) => println!("save {}: no profile save yet", l.dir.display()),
            Err(e) => println!("save {}: {e}", l.dir.display()),
        }
    }
    for s in snaps {
        println!("{}", row(&s));
    }
    Ok(())
}

pub(super) fn show(store: &Store, id: &str, source: Option<&str>) -> Result<()> {
    let (v, _) = open(store, source)?;
    let s = v.find(id)?;
    println!("{}", row(&s));
    println!("taken {} by launcher {}", vault::human(s.at), s.launcher);
    println!("save folder {}", s.save_dir.display());
    if let Some(n) = &s.note {
        println!("note: {n}");
    }
    if let Some(sid) = &s.session_id {
        println!("session {sid}");
    }
    if let Some(exe) = &s.exe_sha256 {
        println!("F1_25.exe sha256 {exe}");
    }
    println!("Steam Cloud when taken: {}", s.cloud().text());
    for f in &s.files {
        println!(
            "  {} ({}): {} bytes, sha256 {}, sha1 {}",
            f.name, f.file, f.size, f.sha256, f.sha1
        );
        if let Some(r) = &f.remotecache {
            println!(
                "    remotecache: size {}, sha {}, syncstate {}",
                r.size, r.sha, r.syncstate
            );
        }
    }
    for e in v
        .events()
        .iter()
        .filter(|e| e.snapshot.as_deref() == Some(id) || e.from.as_deref() == Some(id))
    {
        println!(
            "  event {} {}{}",
            vault::human(e.at),
            serde_json::to_value(e.kind)
                .ok()
                .and_then(|k| k.as_str().map(str::to_owned))
                .unwrap_or_default(),
            e.detail.as_deref().map(|d| format!(": {d}")).unwrap_or_default()
        );
    }
    Ok(())
}

pub(super) fn names(list: &[vault::careers::Brief]) -> String {
    list.iter()
        .map(|b| match &b.place {
            Some(p) => format!("{} ({p})", b.name),
            None => b.name.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn versions(store: &Store, source: Option<&str>) -> Result<()> {
    if let Err(e) = migrate::migrate(&store.root).map(report) {
        say("warning", &format!("saves: {e}"));
    }
    let (v, loc) = match open(store, source) {
        Ok(x) => x,
        Err(e) if source.is_none() && Vault::sources(&store.root).is_empty() => {
            println!("no versions: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let snaps = v.list()?;
    println!(
        "{} version(s) in {}, {} of {}",
        snaps.len(),
        v.dir.display(),
        mb(v.usage()),
        mb(v.policy.cap_bytes)
    );
    if let Some(st) = loc.as_ref().and_then(|l| vault::save_status(l).ok()) {
        println!("Steam Cloud now: {}", st.cloud.text());
    }
    for s in snaps {
        let held = v.index_of(&s).map(|i| {
            let held: Vec<String> = vault::careers::in_save(&i)
                .map(|c| match c.label.is_empty() {
                    true => c.career.clone(),
                    false => c.label.clone(),
                })
                .collect();
            match held.len() {
                0 => "no careers".to_owned(),
                n => format!("{n} career(s): {}", held.join(", ")),
            }
        });
        println!(
            "{}  {}, {}, {}, Steam Cloud: {}, {}{}",
            s.id,
            s.reason.text(),
            match &s.profile {
                Some(p) => format!("profile {p}"),
                None => "no profile".to_owned(),
            },
            mb(s.total()),
            s.cloud().text(),
            held.as_deref().unwrap_or("careers unknown"),
            match s.pinned.is_empty() {
                true => String::new(),
                false => format!(", pinned ({})", s.pinned.join(", ")),
            }
        );
        if let Some(d) = v.preview(&s).filter(|d| !d.is_empty()) {
            if !d.get_back.is_empty() {
                println!("    restoring it gets back: {}", names(&d.get_back));
            }
            if !d.lose.is_empty() {
                println!("    restoring it loses: {}", names(&d.lose));
            }
            for (now, then) in &d.progress {
                println!(
                    "    restoring it loses progress in {}: {} back to {}",
                    now.name,
                    now.place.as_deref().unwrap_or("now"),
                    then.place.as_deref().unwrap_or("then")
                );
            }
        }
    }
    Ok(())
}

pub(super) fn careers(store: &Store, source: Option<&str>) -> Result<()> {
    let (v, _) = match open(store, source) {
        Ok(x) => x,
        Err(e) if source.is_none() && Vault::sources(&store.root).is_empty() => {
            println!("no careers: {e}");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let table = v.careers();
    if table.careers.is_empty() {
        println!(
            "no careers yet: the vault learns them from the career index F1 25 writes in a launcher session"
        );
        return Ok(());
    }
    for r in &table.careers {
        let mut line = format!("{}  {}", r.career, r.name());
        if let Some(m) = &r.mode {
            line.push_str(&format!(", {m}"));
        }
        if let Some(d) = &r.driver {
            line.push_str(&format!(", {d}"));
        }
        if let Some(t) = &r.team {
            line.push_str(&format!(", {t}"));
        }
        if let Some(p) = r.place() {
            line.push_str(&format!(", {p}"));
        }
        if let Some(at) = r.last_played {
            line.push_str(&format!(", last played {}", vault::human(at)));
        }
        let needs: Vec<&str> = r.needs().collect();
        if !needs.is_empty() {
            line.push_str(&format!(", needs {}", needs.join(" ")));
        }
        line.push_str(&format!(", {}", r.status.slug()));
        println!("{line}");
        if r.status.is_missing() {
            println!("    last in the save at snapshot {}", r.last_seen);
        }
    }
    Ok(())
}
