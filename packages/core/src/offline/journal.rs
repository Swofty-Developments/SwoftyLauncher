use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::game_folder::baseline::{sha256_file, sha256_opt};
use crate::{Error, Result, io_err, with_retry};

use super::{
    Artifacts, EAC_EXE, command, ea_signed, heal_disk, heal_stale_backup, journal_path, loader_hint, not_eas,
    require_launcher, set_aside, start_error,
};

/// Stage the session: park the anti-cheat launcher, drop the stub, and place
/// the hook where the loader picks it up. Everything goes in the journal.
/// Repairs leftovers from interrupted sessions first; returns what it did.
pub fn stage(game: &Path, a: &Artifacts, journal: &Path) -> Result<Vec<String>> {
    // The journal lives under state/offline/, which nothing else creates.
    if let Some(parent) = journal.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io_err(parent.to_path_buf())(e))?;
    }
    let mut notes = heal_stale_backup(game, journal)?;
    require_launcher(game)?;
    let hook_dest = game.join("swofty/mods/core/f1op_hook.dll");
    let out = command(&a.prelaunch)?
        .arg("stage")
        .arg("--game-dir")
        .arg(game)
        .arg("--payload")
        .arg(&a.stub)
        .arg("--hook")
        .arg(&a.hook)
        .arg("--hook-dest")
        .arg(&hook_dest)
        .arg("--journal")
        .arg(journal)
        .output()
        .map_err(|e| start_error(&a.prelaunch, e))?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "offline play staging failed: {}{}",
            String::from_utf8_lossy(&out.stderr).trim(),
            loader_hint(&a.prelaunch, out.status)
        )));
    }
    notes.push("staged offline play for this session".into());
    Ok(notes)
}

#[derive(Debug, Deserialize)]
struct JournalLine {
    op: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    sha256: String,
}

/// The marker a rewrite leaves in a journal that had unreadable lines, so a
/// later restore still knows the record is incomplete.
const DAMAGED: &str = "damaged";

/// The journal as it reads from disk. The extension appends one JSON line per
/// change, and a PC that stops mid-session can leave the file's tail torn or
/// zero-filled, so a line that does not parse is counted, never fatal.
pub(crate) struct Journal {
    pub(crate) lines: Vec<serde_json::Value>,
    /// Unreadable lines after the last record: an append the PC stopped in.
    pub(super) torn: usize,
    /// Unreadable lines with records after them.
    pub(super) lost: usize,
    /// A rewrite of this journal dropped unreadable lines.
    marked: bool,
}

impl Journal {
    /// `None` when there is no journal.
    pub(crate) fn read(journal: &Path) -> Result<Option<Self>> {
        let bytes = match std::fs::read(journal) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_err(journal)(e)),
        };
        Ok(Some(Self::parse(&bytes)))
    }

    fn parse(bytes: &[u8]) -> Self {
        let pad = |c: char| c.is_whitespace() || c == '\0';
        let mut j = Self {
            lines: Vec::new(),
            torn: 0,
            lost: 0,
            marked: false,
        };
        for raw in bytes.split(|b| *b == b'\n') {
            let raw = String::from_utf8_lossy(raw);
            let head = raw.trim_start_matches(pad);
            let body = head.trim_end_matches(pad);
            // Zero-filled bytes stand where a write never reached the disk.
            let zeros_before = raw[..raw.len() - head.len()].contains('\0');
            let zeros_after = head[body.len()..].contains('\0');
            if body.is_empty() {
                j.torn += usize::from(zeros_before);
                continue;
            }
            // A record is an object that names its operation.
            let line = serde_json::from_str::<serde_json::Value>(body)
                .ok()
                .filter(|l| JournalLine::deserialize(l).is_ok());
            let Some(line) = line else {
                j.torn += 1;
                continue;
            };
            if line.get(DAMAGED).and_then(serde_json::Value::as_bool) == Some(true) {
                j.marked = true;
                continue;
            }
            j.lost += j.torn + usize::from(zeros_before);
            j.torn = usize::from(zeros_after);
            j.lines.push(line);
        }
        j
    }

    /// Whether a change the session made may be missing from the record: a
    /// line did not parse, or the journal holds no record at all.
    pub(crate) fn damaged(&self) -> bool {
        self.torn > 0 || self.lost > 0 || self.marked || self.lines.is_empty()
    }

    fn note(&self) -> String {
        match (self.lost, self.torn) {
            (0, 0) if self.lines.is_empty() => "offline play's record of this session is empty".to_owned(),
            (0, 0) => "offline play's record of this session lost lines in an earlier repair".to_owned(),
            (0, n) => format!(
                "skipped {n} unfinished line(s) at the end of offline play's record; the PC stopped while it was \
                 being written"
            ),
            (n, t) => format!(
                "skipped {} unreadable line(s) in offline play's record, {n} of them before its last entry",
                n + t
            ),
        }
    }

    /// Write the readable lines back in place of the journal, through a
    /// synced temporary file. A journal that had unreadable lines keeps a
    /// marker saying so.
    pub(crate) fn write(&self, journal: &Path) -> Result<()> {
        use std::io::Write;
        let mut text: String = self.lines.iter().map(|l| format!("{l}\n")).collect();
        if self.damaged() {
            text.push_str(&format!("{}\n", serde_json::json!({"op": "meta", DAMAGED: true})));
        }
        let tmp = journal.with_extension("jsonl.tmp");
        std::fs::File::create(&tmp)
            .and_then(|mut f| {
                f.write_all(text.as_bytes())?;
                f.sync_all()
            })
            .map_err(io_err(&tmp))?;
        with_retry(journal, || std::fs::rename(&tmp, journal))?;
        crate::sync_dir(journal.parent());
        Ok(())
    }
}

/// Replay the journal backwards, verifying digests before touching anything.
/// Returns one line per action for logging. Lines that cannot be read are
/// skipped and named in the first line returned.
pub fn restore(journal: &Path, force: bool) -> Result<Vec<String>> {
    let j = Journal::read(journal)
        .and_then(|j| j.ok_or_else(|| io_err(journal)(std::io::ErrorKind::NotFound.into())))?;
    let mut done = replay(&j, force)?;
    if j.torn > 0 || j.lost > 0 {
        done.insert(0, j.note());
    }
    Ok(done)
}

fn replay(j: &Journal, force: bool) -> Result<Vec<String>> {
    let lines: Vec<JournalLine> = j
        .lines
        .iter()
        .filter_map(|l| JournalLine::deserialize(l).ok())
        .collect();
    // Best effort: every entry is attempted so one locked or changed file
    // never strands the rest half-modded. The all-important launcher rename
    // still happens even if a stub or hook delete is blocked. Transient locks
    // are retried; anything left is reported together at the end so `swofty
    // restore` can be re-run once the locker (usually an antivirus) lets go.
    let mut done = Vec::new();
    let mut failures = Vec::new();
    let renamed: Vec<&str> = lines
        .iter()
        .filter(|e| e.op == "rename")
        .map(|e| e.from.as_str())
        .collect();
    let staged = |slot: &str| {
        lines
            .iter()
            .find(|e| e.op == "copy" && e.to == slot)
            .map(|e| e.sha256.as_str())
    };
    for e in lines.iter().rev() {
        match e.op.as_str() {
            "meta" => {}
            "copy" => {
                let to = Path::new(&e.to);
                if !to.exists() {
                    continue;
                }
                if !force {
                    match sha256_file(to) {
                        // A game repair or update put its own file back over
                        // the staged one; the rename that parked the original
                        // decides what happens to it.
                        Ok(have) if have != e.sha256 && renamed.contains(&e.to.as_str()) => continue,
                        Ok(have) if have != e.sha256 => {
                            failures.push(format!(
                                "{}: changed since staging (digest mismatch); left in place",
                                e.to
                            ));
                            continue;
                        }
                        Ok(_) => {}
                        Err(err) => {
                            failures.push(err.to_string());
                            continue;
                        }
                    }
                }
                match with_retry(to, || std::fs::remove_file(to)) {
                    Ok(()) => done.push(format!("removed {}", e.to)),
                    Err(err) => failures.push(err.to_string()),
                }
            }
            "rename" => {
                let (from, to) = (Path::new(&e.from), Path::new(&e.to));
                if !to.exists() {
                    if from.exists() {
                        done.push(format!(
                            "{} was put back by a game repair or update; nothing to restore",
                            e.from
                        ));
                    } else {
                        failures.push(format!("staged backup is missing: {}", e.to));
                    }
                    continue;
                }
                let dir = to.parent().unwrap_or(Path::new("."));
                let launcher = e.from.ends_with(EAC_EXE);
                if launcher && !force {
                    match not_eas(to, staged(&e.from)) {
                        Ok(true) => {
                            match set_aside(dir, to) {
                                Ok(keep) => done.push(format!(
                                    "{}: its backup is offline play's file, not EA's; set it aside at {}. Repair \
                                     F1 25 in the EA app, or verify it in Steam, to put EA's file back",
                                    e.from,
                                    keep.display()
                                )),
                                Err(err) => failures.push(err.to_string()),
                            }
                            continue;
                        }
                        Ok(false) => {}
                        Err(err) => {
                            failures.push(err.to_string());
                            continue;
                        }
                    }
                }
                if from.exists() && !force {
                    match (sha256_file(from), sha256_file(to)) {
                        (Ok(have), Ok(parked)) if have == parked => {
                            match with_retry(to, || std::fs::remove_file(to)) {
                                Ok(()) => done.push(format!(
                                    "{} was put back by a game repair or update; removed its identical backup {}",
                                    e.from, e.to
                                )),
                                Err(err) => failures.push(err.to_string()),
                            }
                        }
                        (Err(err), _) | (_, Err(err)) => failures.push(err.to_string()),
                        _ if launcher && matches!(ea_signed(from), Ok(Some(true))) => match set_aside(dir, to) {
                            Ok(keep) => done.push(format!(
                                "{} was replaced by the EA app; kept EA's file and set the older backup aside at {}",
                                e.from,
                                keep.display()
                            )),
                            Err(err) => failures.push(err.to_string()),
                        },
                        _ => failures.push(format!("{}: already exists; left as is", e.from)),
                    }
                    continue;
                }
                match with_retry(from, || std::fs::rename(to, from)) {
                    Ok(()) => done.push(format!("restored {}", e.from)),
                    Err(err) => failures.push(err.to_string()),
                }
            }
            other => {
                failures.push(format!("unknown staging journal op: {other} (update swofty)"));
            }
        }
    }
    if !failures.is_empty() {
        return Err(Error::Other(format!(
            "offline-play restore could not finish cleanly; re-run `swofty restore` once \
             anything locking the files is closed:\n  {}",
            failures.join("\n  ")
        )));
    }
    Ok(done)
}

/// Restore any pending staging in `game` (crash recovery, `swofty restore`).
/// Returns `None` when nothing was staged, else what there is to say about
/// it, which is nothing for a journal that read cleanly.
///
/// A journal with unreadable lines is what a PC that stopped mid-session
/// leaves. Every line that does read is replayed, and because the change an
/// unreadable line recorded is unknown, the launcher's slot is then put back
/// from the game folder itself, as [`heal_stale_backup`] does.
pub fn restore_pending(data: &Path, game: &Path) -> Result<Option<Vec<String>>> {
    let journal = journal_path(data);
    let Some(j) = Journal::read(&journal)? else {
        return Ok(None);
    };
    let mut notes = Vec::new();
    if j.damaged() {
        notes.push(j.note());
        if let Err(e) = replay(&j, false) {
            notes.push(format!(
                "what is left of the record could not be replayed ({e}); recovered from the backup instead"
            ));
        }
        notes.extend(heal_disk(game)?);
    } else {
        replay(&j, false)?;
    }
    std::fs::remove_file(&journal).map_err(io_err(&journal))?;
    Ok(Some(notes))
}

/// What the anti-cheat launcher's slot holds while a session is staged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// Offline play's file, as staged.
    Staged,
    /// A file EA signed, which an EA app repair or update put there.
    Replaced,
    /// No file.
    Missing,
    /// A file that is neither offline play's nor EA's.
    Unknown,
}

struct Staging {
    journal: Journal,
    copy: usize,
    rename: Option<usize>,
    slot: PathBuf,
    parked: PathBuf,
    slot_state: Slot,
}

fn staging(journal: &Path) -> Result<Option<Staging>> {
    let Some(journal) = Journal::read(journal)? else {
        return Ok(None);
    };
    let lines = &journal.lines;
    let field =
        |l: &serde_json::Value, k: &str| l.get(k).and_then(serde_json::Value::as_str).map(str::to_owned);
    let Some(copy) = lines.iter().position(|l| {
        field(l, "op").as_deref() == Some("copy") && field(l, "to").is_some_and(|t| t.ends_with(EAC_EXE))
    }) else {
        return Ok(None);
    };
    let to = field(&lines[copy], "to").unwrap_or_default();
    let rename = lines.iter().position(|l| {
        field(l, "op").as_deref() == Some("rename") && field(l, "from").as_deref() == Some(&to)
    });
    let slot = PathBuf::from(&to);
    let parked = rename
        .and_then(|i| field(&lines[i], "to"))
        .map_or_else(|| PathBuf::from(format!("{to}.bak")), PathBuf::from);
    let slot_state = match sha256_opt(&slot)? {
        None => Slot::Missing,
        Some(have) if field(&lines[copy], "sha256").as_ref() == Some(&have) => Slot::Staged,
        Some(_) if ea_signed(&slot)? == Some(true) => Slot::Replaced,
        Some(_) => Slot::Unknown,
    };
    Ok(Some(Staging {
        journal,
        copy,
        rename,
        slot,
        parked,
        slot_state,
    }))
}

/// What the slot holds now, for a session staged under `journal`.
pub fn slot(journal: &Path) -> Result<Option<Slot>> {
    Ok(staging(journal)?.map(|s| s.slot_state))
}

/// Put offline play's file back when an EA app repair took it out of the
/// slot during a session. EA's file becomes the backup the restore puts
/// back, and the older backup is set aside. Returns what the slot held.
pub fn restage(a: &Artifacts, journal: &Path) -> Result<Option<Slot>> {
    let Some(mut s) = staging(journal)? else {
        return Ok(None);
    };
    match s.slot_state {
        Slot::Staged | Slot::Unknown => return Ok(Some(s.slot_state)),
        Slot::Missing => {}
        Slot::Replaced => {
            let have = sha256_file(&s.slot)?;
            if s.parked.exists() && sha256_file(&s.parked)? != have {
                let dir = s.parked.parent().unwrap_or(Path::new("."));
                set_aside(dir, &s.parked)?;
            }
            with_retry(&s.slot, || std::fs::rename(&s.slot, &s.parked))?;
            let entry = serde_json::json!({
                "op": "rename",
                "from": s.slot.display().to_string(),
                "to": s.parked.display().to_string(),
                "source": "",
                "sha256": have,
            });
            match s.rename {
                Some(i) => s.journal.lines[i]["sha256"] = have.into(),
                None => {
                    s.journal.lines.insert(s.copy, entry);
                    s.copy += 1;
                }
            }
        }
    }
    with_retry(&s.slot, || std::fs::copy(&a.stub, &s.slot))?;
    s.journal.lines[s.copy]["sha256"] = sha256_file(&s.slot)?.into();
    s.journal.write(journal)?;
    Ok(Some(s.slot_state))
}
