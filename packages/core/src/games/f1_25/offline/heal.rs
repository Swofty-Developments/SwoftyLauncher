use std::path::{Path, PathBuf};

use crate::game_folder::baseline::{DepotHashes, sha256_file, sha256_opt};
use crate::{Error, Result, io_err, with_retry};

use super::{EAC_BAK, EAC_EXE, journal_path, pin, restore};

/// Repair a half-staged game folder left by an interrupted session, so a new
/// session can start without anyone digging files out by hand. The backup is
/// put back in place; whatever sat in the launcher's slot is kept under
/// swofty/offline-recovery/ rather than destroyed. Returns what it did.
pub fn heal_stale_backup(game: &Path, journal: &Path) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    if journal.exists() {
        match restore(journal, false) {
            Ok(done) => notes.extend(done),
            Err(e) => notes.push(format!(
                "a leftover journal could not be replayed ({e}); recovered from the backup instead"
            )),
        }
        std::fs::remove_file(journal).map_err(io_err(journal))?;
    }
    notes.extend(heal_disk(game)?);
    Ok(notes)
}

/// Put the game folder back from what is in it, with no journal to go by:
/// the anti-cheat launcher from its backup, and the hook out of the loader's
/// slot.
pub(super) fn heal_disk(game: &Path) -> Result<Vec<String>> {
    let mut notes = heal_launcher(game, None)?;
    // A hook left in the loader's slot is ours: the journal is what normally
    // owns it, and staging cannot drop the new one while it is there.
    let hook = game.join("swofty/mods/core/f1op_hook.dll");
    if hook.exists() {
        with_retry(&hook, || std::fs::remove_file(&hook))?;
        notes.push("removed a stale hook from a previous session".into());
    }
    Ok(notes)
}

/// Clear a stale anti-cheat backup before a build's stock check, so a leftover
/// `.bak` of the wrong size can't block every launch. It does nothing while a
/// session is pending ([`restore_pending`] owns the launcher then). With a
/// depot list it keeps a Steam-verified live exe and sets the stale backup
/// aside; without one it keeps today's restore-from-backup behaviour. Returns
/// what it did.
pub fn heal_before_check(data: &Path, game: &Path, depot: Option<&dyn DepotHashes>) -> Result<Vec<String>> {
    if journal_path(data).exists() {
        return Ok(Vec::new());
    }
    heal_launcher(game, depot)
}

pub(super) fn set_aside(game: &Path, file: &Path) -> Result<PathBuf> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let keep = game
        .join("swofty/offline-recovery")
        .join(format!("{name}.{secs}"));
    if let Some(parent) = keep.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent.to_path_buf())(e))?;
    }
    with_retry(file, || std::fs::rename(file, &keep))?;
    Ok(keep)
}

const EA_SIGNER: &[u8] = b"Electronic Arts";

/// Whether `path` is a program whose certificate table names Electronic Arts
/// as its signer; `None` when it is not a Windows program at all. The
/// signature itself is not verified.
pub(super) fn ea_signed(path: &Path) -> Result<Option<bool>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).map_err(io_err(path))?;
    let mut head = Vec::new();
    (&mut f).take(4096).read_to_end(&mut head).map_err(io_err(path))?;
    let at = |i: usize| {
        head.get(i..i + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    let Some(pe) = at(0x3c).filter(|_| head.starts_with(b"MZ")).map(|v| v as usize) else {
        return Ok(None);
    };
    if head.get(pe..pe + 4) != Some(b"PE\0\0".as_slice()) {
        return Ok(None);
    }
    let dirs = match head.get(pe + 24..pe + 26) {
        Some([0x0b, 0x01]) => pe + 24 + 96,
        Some([0x0b, 0x02]) => pe + 24 + 112,
        _ => return Ok(None),
    };
    let (Some(offset), Some(len)) = (at(dirs + 32), at(dirs + 36)) else {
        return Ok(None);
    };
    if len == 0 {
        return Ok(Some(false));
    }
    let mut table = vec![0; len.min(1 << 20) as usize];
    f.seek(SeekFrom::Start(offset.into())).map_err(io_err(path))?;
    f.read_exact(&mut table).map_err(io_err(path))?;
    Ok(Some(table.windows(EA_SIGNER.len()).any(|w| w == EA_SIGNER)))
}

/// Whether `path` holds offline play's stub rather than EA's launcher: the
/// pinned stub, the one a journal recorded, or any program EA did not sign.
pub(super) fn not_eas(path: &Path, staged: Option<&str>) -> Result<bool> {
    let have = sha256_file(path)?;
    if Some(have.as_str()) == staged || Some(have.as_str()) == pin("f1op_stub.exe") {
        return Ok(true);
    }
    Ok(ea_signed(path)? == Some(false))
}

pub(super) fn heal_launcher(game: &Path, depot: Option<&dyn DepotHashes>) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    let launcher = game.join(EAC_EXE);
    let parked = game.join(EAC_BAK);
    if parked.is_file() && not_eas(&parked, None)? {
        let keep = set_aside(game, &parked)?;
        notes.push(format!(
            "set aside a backup that is offline play's file, not EA's, at {}",
            keep.display()
        ));
    }
    if !parked.exists() {
        if launcher.is_file() && not_eas(&launcher, None)? {
            let keep = set_aside(game, &launcher)?;
            notes.push(format!(
                "{EAC_EXE} was offline play's file, not EA's; set it aside at {}. Repair F1 25 in the EA \
                 app, or verify it in Steam, to put EA's file back",
                keep.display()
            ));
        }
        return Ok(notes);
    }
    let backup = sha256_file(&parked)?;
    if sha256_opt(&launcher)?.as_deref() == Some(backup.as_str()) {
        with_retry(&parked, || std::fs::remove_file(&parked))?;
        notes.push("removed a stale backup identical to the launcher".into());
        return Ok(notes);
    }
    let live_is_stock = match depot {
        Some(d) if launcher.exists() => d.stock(EAC_EXE, &launcher)?,
        _ => None,
    };
    let bak_is_stock = match depot {
        Some(d) => d.stock(EAC_EXE, &parked)?,
        None => None,
    };
    if live_is_stock == Some(true) && bak_is_stock != Some(true) {
        let keep = set_aside(game, &parked)?;
        notes.push(format!(
            "set a stale anti-cheat backup aside at {}",
            keep.display()
        ));
        return Ok(notes);
    }
    if depot.is_some() && bak_is_stock != Some(true) {
        return Ok(notes);
    }
    if launcher.exists() {
        let keep = set_aside(game, &launcher)?;
        notes.push(format!("kept the staged file at {}", keep.display()));
    }
    with_retry(&launcher, || std::fs::rename(&parked, &launcher))?;
    notes.push("restored EAAntiCheat.GameServiceLauncher.exe from its backup".into());
    Ok(notes)
}

/// Refuse a session when the game folder has no anti-cheat launcher to park:
/// the stub would sit in an empty slot with no backup behind it, and the
/// game would start without the launcher it expects.
pub(super) fn require_launcher(game: &Path) -> Result<()> {
    if game.join(EAC_EXE).exists() || game.join(EAC_BAK).exists() {
        return Ok(());
    }
    Err(Error::MissingStock {
        files: vec![EAC_EXE.to_owned()],
        version: crate::game_folder::exe_version::read(&game.join(crate::GAME_EXE)),
    })
}
