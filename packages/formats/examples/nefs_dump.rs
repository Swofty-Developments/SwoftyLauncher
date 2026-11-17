use std::fs::File;
use std::io::{BufReader, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use swofty_formats::nefs::{Archive, Entry};

pub const USAGE: &str = "usage: nefs_dump <F1_25.exe> <game.dat> [--out DIR] [FILTER...]

Lists the files of the NeFS archive whose header F1_25.exe carries (path and size, one per line).
FILTER keeps the paths that contain any of the given texts, ignoring case. With --out DIR the kept
files are also written under DIR at their archive paths. The archive and the exe are only read.";

pub struct Args {
    pub exe: PathBuf,
    pub data: PathBuf,
    pub out: Option<PathBuf>,
    pub filters: Vec<String>,
}

pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut plain = Vec::new();
    let mut out = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => return Err(USAGE.to_owned()),
            "--out" => out = Some(PathBuf::from(it.next().ok_or("--out needs a folder")?)),
            _ => plain.push(a.clone()),
        }
    }
    if plain.len() < 2 {
        return Err(USAGE.to_owned());
    }
    let filters = plain.split_off(2);
    Ok(Args {
        exe: PathBuf::from(&plain[0]),
        data: PathBuf::from(&plain[1]),
        out,
        filters,
    })
}

pub fn is_folder(archive: &Archive, entry: &Entry) -> bool {
    entry.size == 0
        && archive.entries.iter().any(|e| {
            e.path.len() > entry.path.len() + 1
                && e.path.starts_with(&entry.path)
                && e.path.as_bytes()[entry.path.len()] == b'/'
        })
}

pub fn selected<'a>(archive: &'a Archive, filters: &[String]) -> Vec<&'a Entry> {
    let wanted: Vec<String> = filters.iter().map(|f| f.to_ascii_lowercase()).collect();
    archive
        .entries
        .iter()
        .filter(|e| !is_folder(archive, e))
        .filter(|e| wanted.is_empty() || wanted.iter().any(|f| e.path.to_ascii_lowercase().contains(f)))
        .collect()
}

pub fn target(out: &Path, path: &str) -> Option<PathBuf> {
    let rel = Path::new(path);
    if path.is_empty() || !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(out.join(rel))
}

pub fn dump(
    archive: &Archive,
    data: &mut (impl Read + Seek),
    out: &Path,
    entries: &[&Entry],
) -> Result<Vec<PathBuf>, String> {
    let targets = entries
        .iter()
        .map(|e| {
            target(out, &e.path).ok_or_else(|| format!("{}: not a path inside the output folder", e.path))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut written = Vec::with_capacity(entries.len());
    for (e, to) in entries.iter().zip(targets) {
        let bytes = archive
            .read(e, data)
            .map_err(|err| format!("{}: {err}", e.path))?;
        if let Some(dir) = to.parent() {
            std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
        }
        std::fs::write(&to, bytes).map_err(|err| format!("{}: {err}", to.display()))?;
        written.push(to);
    }
    Ok(written)
}

pub fn run(args: &[String], w: &mut impl Write) -> Result<(), String> {
    let a = parse(args)?;
    let open = |p: &Path| {
        File::open(p)
            .map(BufReader::new)
            .map_err(|e| format!("{}: {e}", p.display()))
    };
    let archive = Archive::open_exe(&mut open(&a.exe)?).map_err(|e| format!("{}: {e}", a.exe.display()))?;
    let entries = selected(&archive, &a.filters);
    let io = |e: std::io::Error| e.to_string();
    writeln!(
        w,
        "# volume {} block {} entries {} kept {}",
        archive.volume,
        archive.block_size,
        archive.entries.len(),
        entries.len()
    )
    .map_err(io)?;
    for e in &entries {
        writeln!(w, "{}\t{}", e.path, e.size).map_err(io)?;
    }
    if let Some(out) = &a.out {
        let written = dump(&archive, &mut open(&a.data)?, out, &entries)?;
        writeln!(w, "# wrote {} file(s) under {}", written.len(), out.display()).map_err(io)?;
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args, &mut std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("nefs_dump: {e}");
            ExitCode::from(2)
        }
    }
}
