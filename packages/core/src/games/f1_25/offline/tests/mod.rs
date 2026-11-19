#[cfg(target_os = "linux")]
use std::process::Command;

use super::antivirus::{STUB, virus_code};
use super::consent::consent_path;
use super::exec::exec_mode_in;
#[cfg(target_os = "linux")]
use super::exec::{command_for, elf_interpreter};
#[cfg(target_os = "linux")]
use super::extension::set_executable;
use super::extension::{BLOCKED, RUNNERS, download, fetch_release, install_files, staging_dir};
use super::heal::heal_launcher;
use super::*;
use crate::game_folder::baseline::{DepotHashes, sha256_file};
use crate::{Error, Result, io_err, write_json};

mod antivirus;
mod consent;
mod exec;
mod fetch;
mod heal;
mod journal;
mod stub;

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-offline-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    std::fs::create_dir_all(&d).unwrap();
    d
}

enum Reply {
    Body(Vec<u8>),
    Status(u16),
    CutOff { claimed: usize, sent: Vec<u8> },
}

fn serve(routes: Vec<(String, Reply)>) -> String {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            reader.read_line(&mut request).ok();
            let path = request.split_whitespace().nth(1).unwrap_or_default().to_owned();
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                    break;
                }
            }
            let (status, length, body) = match routes.iter().find(|(p, _)| *p == path) {
                Some((_, Reply::Body(b))) => (200, b.len(), b.clone()),
                Some((_, Reply::Status(code))) => (*code, 0, Vec::new()),
                Some((_, Reply::CutOff { claimed, sent })) => (200, *claimed, sent.clone()),
                None => (404, 0, Vec::new()),
            };
            let head =
                format!("HTTP/1.1 {status} X\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n");
            stream.write_all(head.as_bytes()).ok();
            stream.write_all(&body).ok();
        }
    });
    base
}

fn fake_release() -> Vec<(&'static str, String)> {
    REQUIRED.iter().map(|n| (*n, format!("fake {n}"))).collect()
}

fn digests(release: &[(&'static str, String)]) -> Vec<(&'static str, String)> {
    release
        .iter()
        .map(|(n, b)| (*n, crate::game_folder::baseline::sha256_bytes(b.as_bytes())))
        .collect()
}

fn fetch_fake(d: &Path, base: &str, release: &[(&'static str, String)]) -> Result<()> {
    let pins = digests(release);
    let pins: Vec<(&str, &str)> = pins.iter().map(|(n, h)| (*n, h.as_str())).collect();
    fetch_release(d, base, "9.9.9", &pins)
}

fn routes(
    release: &[(&'static str, String)],
    mut tweak: impl FnMut(&str) -> Option<Reply>,
) -> Vec<(String, Reply)> {
    release
        .iter()
        .map(|(n, b)| {
            let reply = tweak(n).unwrap_or_else(|| Reply::Body(b.clone().into_bytes()));
            (format!("/v9.9.9/{n}"), reply)
        })
        .collect()
}

fn previous_install(d: &Path) -> PathBuf {
    let marker = extension_dir(d).join("previous.txt");
    std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
    std::fs::write(&marker, "previous install").unwrap();
    marker
}

fn assert_untouched(d: &Path, marker: &Path) {
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "previous install");
    assert!(!extension_dir(d).join(REQUIRED[0]).exists());
    assert!(!staging_dir(d).exists(), "the partial download is cleaned up");
}

/// A copy of this test binary whose ELF interpreter points at a loader
/// that does not exist: what a Linux build linked on NixOS looks like on a
/// Steam Deck. Returns the copy and the real loader.
#[cfg(target_os = "linux")]
fn binary_with_a_missing_loader(d: &Path) -> Option<(PathBuf, PathBuf)> {
    let exe = std::env::current_exe().unwrap();
    let real = elf_interpreter(&exe).unwrap()?;
    let name = real.file_name()?.to_str()?.to_owned();
    let dir_len = real.to_str()?.len() - name.len() - 1;
    let fake = format!("/{}/{name}", "z".repeat(dir_len.checked_sub(1)?));
    let mut data = std::fs::read(&exe).unwrap();
    let at = data
        .windows(real.as_os_str().len())
        .position(|w| w == real.as_os_str().as_encoded_bytes())?;
    data[at..at + fake.len()].copy_from_slice(fake.as_bytes());
    let copy = d.join("runner");
    std::fs::write(&copy, data).unwrap();
    set_executable(&copy).unwrap();
    Some((copy, real))
}

/// A game folder staged the way the extension stages it: the anti-cheat
/// launcher parked as `.bak`, the stub in its place and the hook dropped,
/// with the journal that records it.
fn staged(d: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let game = d.join("game");
    std::fs::create_dir_all(game.join("swofty/mods/core")).unwrap();
    let launcher = game.join(EAC_EXE);
    let parked = game.join(EAC_BAK);
    let hook = game.join("swofty/mods/core/f1op_hook.dll");
    std::fs::write(&launcher, "genuine").unwrap();
    std::fs::rename(&launcher, &parked).unwrap();
    std::fs::write(&launcher, "stub").unwrap();
    std::fs::write(&hook, "hook").unwrap();
    let journal = d.join("state/offline/staging.jsonl");
    std::fs::create_dir_all(journal.parent().unwrap()).unwrap();
    let lines = [
        serde_json::json!({"op": "meta", "game_dir": "x", "created_utc": "", "schema_version": "1"}),
        serde_json::json!({"op": "rename", "from": launcher.display().to_string(), "to": parked.display().to_string(), "source": "", "sha256": ""}),
        serde_json::json!({"op": "copy", "from": "", "to": launcher.display().to_string(), "source": "stub", "sha256": sha256_file(&launcher).unwrap()}),
        serde_json::json!({"op": "copy", "from": "", "to": hook.display().to_string(), "source": "hook", "sha256": sha256_file(&hook).unwrap()}),
    ];
    let text: Vec<String> = lines.iter().map(serde_json::Value::to_string).collect();
    std::fs::write(&journal, text.join("\n") + "\n").unwrap();
    (journal, launcher, parked, hook)
}

fn put_back(d: &Path, text: &[u8]) -> Vec<String> {
    let (journal, launcher, parked, hook) = staged(d);
    std::fs::write(&journal, text).unwrap();
    let notes = restore_pending(d, &d.join("game")).unwrap().unwrap();
    assert_eq!(
        std::fs::read_to_string(&launcher).unwrap(),
        "genuine",
        "{notes:?}"
    );
    assert!(!parked.exists(), "{notes:?}");
    assert!(!hook.exists(), "{notes:?}");
    assert!(!journal.exists(), "{notes:?}");
    assert_eq!(restore_pending(d, &d.join("game")).unwrap(), None);
    notes
}

fn journal_text(d: &Path) -> Vec<String> {
    let (journal, ..) = staged(d);
    std::fs::read_to_string(journal)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

struct FakeDepot {
    rel: String,
    bytes: Vec<u8>,
}

impl DepotHashes for FakeDepot {
    fn stock(&self, rel: &str, path: &Path) -> Result<Option<bool>> {
        if rel != self.rel {
            return Ok(None);
        }
        match std::fs::read(path) {
            Ok(data) => Ok(Some(data == self.bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Some(false)),
            Err(e) => Err(io_err(path)(e)),
        }
    }
}

fn program(signer: Option<&str>, body: &str) -> Vec<u8> {
    let mut pe = vec![0u8; 0x200];
    pe[..2].copy_from_slice(b"MZ");
    pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
    pe[0x80..0x84].copy_from_slice(b"PE\0\0");
    pe[0x98..0x9a].copy_from_slice(&0x20bu16.to_le_bytes());
    pe.extend_from_slice(body.as_bytes());
    if let Some(signer) = signer {
        let table = format!("\0\0\0\0certificate O={signer}, Inc.");
        let dir = 0x98 + 112 + 32;
        let at = pe.len() as u32;
        pe[dir..dir + 4].copy_from_slice(&at.to_le_bytes());
        pe[dir + 4..dir + 8].copy_from_slice(&(table.len() as u32).to_le_bytes());
        pe.extend_from_slice(table.as_bytes());
    }
    pe
}

fn stub_artifacts(d: &Path) -> Artifacts {
    let dir = d.join("extension");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("f1op_stub.exe"), "stub").unwrap();
    Artifacts {
        hook: dir.join("f1op_hook.dll"),
        stub: dir.join("f1op_stub.exe"),
        prelaunch: dir.join("prelaunch"),
        probe: dir.join("probe"),
        dir,
    }
}

fn recovered(game: &Path) -> Vec<Vec<u8>> {
    std::fs::read_dir(game.join("swofty/offline-recovery"))
        .map(|r| r.flatten().map(|e| std::fs::read(e.path()).unwrap()).collect())
        .unwrap_or_default()
}

fn missing_line(game: &Path, at: &str) -> String {
    format!(
        "17\t[{at}]\tERROR   \t(eax::services::process::ProcessWin::createProcess)\t[PROCESS] Create Process: \
         The system cannot find the file specified. | exe=[{}] | OS ErrorCode:  0x2\n",
        game.join(EAC_EXE).display()
    )
}
