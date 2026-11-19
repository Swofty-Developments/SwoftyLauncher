use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// An archive the archive cache keeps under its key, `<key[..2]>/<key>.erp`, with a record beside it of
/// the size and CRC-32 it was kept with. The archive shares its inode with every file linked to it, so a
/// write through any of those links changes the kept bytes too: it is linked out only while it still has
/// that size and checksum.
pub struct ArchiveEntry {
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Sum {
    len: u64,
    crc: u32,
}

impl ArchiveEntry {
    pub fn at(dir: &Path, key: &str) -> Self {
        Self {
            path: dir.join(&key[..2]).join(format!("{key}.erp")),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn record(&self) -> PathBuf {
        self.path.with_extension("sum")
    }

    /// Links `out` to the kept archive and marks it used, when it still has the size and checksum it was
    /// kept with; false when there is none, its bytes changed or it cannot link.
    pub fn link_to(&self, out: &Path) -> bool {
        let kept: Option<Sum> = std::fs::read(self.record())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let Some(kept) = kept else {
            return false;
        };
        let intact =
            std::fs::metadata(&self.path).is_ok_and(|m| m.len() == kept.len) && sum(&self.path) == Some(kept);
        if !intact || std::fs::hard_link(&self.path, out).is_err() {
            return false;
        }
        super::mark_used(&self.path);
        true
    }

    /// Keeps `out`, an archive just written, in place of whatever was kept here, with its size and
    /// checksum; false when it cannot.
    pub fn keep(&self, out: &Path) -> bool {
        let Some(s) = sum(out) else {
            return false;
        };
        let record = self.record();
        std::fs::remove_file(&record).ok();
        std::fs::remove_file(&self.path).ok();
        self.path
            .parent()
            .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
            && std::fs::hard_link(out, &self.path).is_ok()
            && std::fs::write(&record, serde_json::to_vec(&s).expect("json")).is_ok()
    }
}

fn sum(path: &Path) -> Option<Sum> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut crc = flate2::Crc::new();
    let mut buf = vec![0; 1 << 20];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        crc.update(&buf[..n]);
        read += n as u64;
    }
    (read == len).then_some(Sum { len, crc: crc.sum() })
}

/// Changes the byte in the middle of the file at `path` in place, keeping its size and write time.
#[cfg(test)]
pub(crate) fn change_a_byte_in_place(path: &Path) {
    use std::io::{Seek, SeekFrom, Write};
    let meta = std::fs::metadata(path).unwrap();
    let at = meta.len() / 2;
    let byte = std::fs::read(path).unwrap()[at as usize];
    let mut f = std::fs::File::options().write(true).open(path).unwrap();
    f.seek(SeekFrom::Start(at)).unwrap();
    f.write_all(&[byte ^ 0xff]).unwrap();
    f.set_modified(meta.modified().unwrap()).unwrap();
}

/// The archives kept in the archive cache at `dir`.
#[cfg(test)]
pub(crate) fn kept_archives(dir: &Path) -> Vec<PathBuf> {
    let mut kept: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|sub| std::fs::read_dir(sub.path()).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "erp"))
        .collect();
    kept.sort();
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_archive_links_out_only_while_it_has_the_size_and_checksum_it_was_kept_with() {
        let dir = std::env::temp_dir().join(format!("swofty-archive-entry-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let bytes: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let written = dir.join("written.erp");
        std::fs::write(&written, &bytes).unwrap();
        let entry = ArchiveEntry::at(&dir.join("cache"), "ab12cd");
        assert!(entry.keep(&written));
        assert!(entry.link_to(&dir.join("one.erp")));
        assert_eq!(std::fs::read(dir.join("one.erp")).unwrap(), bytes);

        change_a_byte_in_place(entry.path());
        assert_eq!(std::fs::metadata(entry.path()).unwrap().len(), bytes.len() as u64);
        assert_eq!(kept_archives(&dir.join("cache")), [entry.path()]);
        assert!(
            !entry.link_to(&dir.join("two.erp")),
            "a byte changed in place, the size and the write time kept"
        );
        assert!(!dir.join("two.erp").exists());

        std::fs::write(&written, &bytes).unwrap();
        assert!(entry.keep(&written));
        std::fs::File::options()
            .write(true)
            .open(entry.path())
            .unwrap()
            .set_len(1000)
            .unwrap();
        assert!(!entry.link_to(&dir.join("three.erp")), "a cut short archive");

        let fresh = dir.join("fresh.erp");
        std::fs::write(&fresh, &bytes).unwrap();
        assert!(entry.keep(&fresh));
        assert!(
            entry.link_to(&dir.join("four.erp")),
            "kept again from a new write"
        );
        assert_eq!(std::fs::read(dir.join("four.erp")).unwrap(), bytes);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_archive_kept_without_its_record_is_never_linked_out() {
        let dir = std::env::temp_dir().join(format!("swofty-archive-entry-bare-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let entry = ArchiveEntry::at(&dir, "ef34");
        std::fs::create_dir_all(entry.path().parent().unwrap()).unwrap();
        std::fs::write(entry.path(), b"an archive an older build kept").unwrap();
        assert!(!entry.link_to(&dir.join("out.erp")));
        std::fs::remove_dir_all(&dir).ok();
    }
}
