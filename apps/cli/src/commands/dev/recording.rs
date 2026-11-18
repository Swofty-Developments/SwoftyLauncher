use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use swofty_core::manifest::MANIFEST_FILE;
use swofty_core::store::Store;
use swofty_formats::pe::has_export;

use crate::commands::dev::lock::{LOCK_FILE, Lock};
use crate::commands::dev::native_test::ENGINE_SOURCES;
use crate::commands::dev::toolchain::which;
use crate::commands::dev::{RECORDER_ENV, WINE_ENV, check, data_paths, data_root, lock};

pub const REGISTER_EXPORT: &str = "SwoftyAddonRegister";

pub fn manifest_json(folder: &Path) -> Result<Value> {
    let path = folder.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).with_context(|| format!("{}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("{}", path.display()))
}

pub fn native_dlls(manifest: &Value) -> Vec<String> {
    manifest["operations"]["native"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["dll"].as_str().map(str::to_owned))
        .collect()
}

pub fn register_dlls(folder: &Path, manifest: &Value) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for rel in native_dlls(manifest) {
        let path = folder.join(&rel);
        let image =
            std::fs::read(&path).with_context(|| format!("{rel} is missing; build the addon first"))?;
        if has_export(&image, REGISTER_EXPORT) {
            out.push(path);
        }
    }
    Ok(out)
}

pub struct Recorded {
    pub lock: Lock,
    pub files: Vec<String>,
    pub missing: Vec<String>,
}

pub fn recorded_files(sdk: &Path, folder: &Path, recordings: &[Value]) -> (Vec<String>, Vec<String>) {
    let (mut files, mut missing) = (Vec::new(), Vec::new());
    for rec in recordings {
        for (registry, value) in data_paths::recorded_values(rec) {
            data_paths::file_strings(folder, value, &mut files);
            if let Some(schema) = check::sdk_schema(sdk, registry) {
                let mut named = Vec::new();
                data_paths::schema_paths(&schema, &schema, value, &mut named, 0);
                for p in named {
                    if !data_paths::inside(folder, &p) && !missing.contains(&p) {
                        missing.push(p);
                    } else if data_paths::inside(folder, &p) {
                        let norm = p.replace('\\', "/");
                        if !files.contains(&norm) {
                            files.push(norm);
                        }
                    }
                }
            }
        }
    }
    files.sort();
    missing.sort();
    (files, missing)
}

pub fn write_lock(store: &Store, sdk: &Path, folder: &Path) -> Result<Option<Recorded>> {
    let manifest = manifest_json(folder)?;
    let dlls = register_dlls(folder, &manifest)?;
    let lock_path = folder.join(LOCK_FILE);
    if dlls.is_empty() {
        if lock_path.is_file() {
            std::fs::remove_file(&lock_path)?;
            println!("removed the old {LOCK_FILE}: no DLL has a SWOFTY_REGISTER now");
        }
        return Ok(None);
    }
    let recorder = recorder_dir(sdk)?;
    let scratch = data_root(store).join("cache/recorder");
    std::fs::create_dir_all(&scratch)?;
    let mut recordings = Vec::new();
    for dll in &dlls {
        recordings.push(record(&recorder, &scratch, dll, &folder.join(MANIFEST_FILE))?);
    }
    let mut lock = Lock::from_recordings(&recordings, &lock::generated_by());
    let (files, missing) = recorded_files(sdk, folder, &recordings);
    lock.files = files.clone();
    std::fs::write(&lock_path, lock.to_json()).with_context(|| format!("{}", lock_path.display()))?;
    Ok(Some(Recorded { lock, files, missing }))
}

pub fn recorder_dir(sdk: &Path) -> Result<PathBuf> {
    let ready = |d: &Path| d.join("swofty_record.exe").is_file() && d.join("swofty_core.dll").is_file();
    if let Some(d) = std::env::var_os(RECORDER_ENV).map(PathBuf::from)
        && ready(&d)
    {
        return Ok(d);
    }
    let dist = sdk.join("packages/sdk/recorder/dist");
    if (!ready(&dist) || recorder_stale(sdk, &dist))
        && cfg!(unix)
        && sdk.join("packages/sdk/recorder/build.sh").is_file()
    {
        let status = Command::new("bash")
            .arg(sdk.join("packages/sdk/recorder/build.sh"))
            .stdout(Stdio::null())
            .status();
        if !status.is_ok_and(|s| s.success()) {
            bail!("building the recorder failed; run packages/sdk/recorder/build.sh (it needs zig)");
        }
    }
    if ready(&dist) {
        return Ok(dist);
    }
    bail!(
        "the recorder (swofty_record.exe and swofty_core.dll) isn't in {}; set {RECORDER_ENV}",
        dist.display()
    )
}

fn recorder_stale(sdk: &Path, dist: &Path) -> bool {
    let time = |p: &Path| p.metadata().and_then(|m| m.modified()).ok();
    let Some(built) = time(&dist.join("swofty_record.exe")) else {
        return true;
    };
    let reg = sdk.join("native/src/core/registry");
    let mut sources = vec![
        sdk.join("packages/sdk/recorder/recorder.cpp"),
        sdk.join("packages/sdk/recorder/core_stub.c"),
        sdk.join("packages/sdk/include/swofty/fake_core.hpp"),
    ];
    sources.extend(ENGINE_SOURCES.iter().map(|s| reg.join(s)));
    sources.iter().filter_map(|s| time(s)).any(|t| t > built)
}

pub fn wine_path(p: &Path) -> String {
    if cfg!(windows) {
        return p.display().to_string();
    }
    format!("Z:{}", p.display().to_string().replace('/', "\\"))
}

pub fn recorder_command(
    recorder: &Path,
    prefix: &Path,
    dll: &Path,
    manifest: &Path,
    out: &Path,
) -> Result<Command> {
    let exe = recorder.join("swofty_record.exe");
    let args = [
        wine_path(dll),
        wine_path(out),
        "--manifest".into(),
        wine_path(manifest),
    ];
    if cfg!(windows) {
        let mut c = Command::new(exe);
        c.args(args);
        return Ok(c);
    }
    let wine = std::env::var_os(WINE_ENV)
        .map(PathBuf::from)
        .or_else(|| which("wine"))
        .or_else(|| which("wine64"))
        .context("the recorder runs under Wine on Linux: install wine or set SWOFTY_WINE")?;
    let mut c = Command::new(wine);
    c.arg(exe).args(args).env("WINEDEBUG", "-all").env(
        "WINEDLLOVERRIDES",
        "mscoree,mshtml=;winemenubuilder.exe,winedbg.exe=d",
    );
    c.env("WINEPREFIX", prefix);
    Ok(c)
}

pub fn record(recorder: &Path, scratch: &Path, dll: &Path, manifest: &Path) -> Result<Value> {
    let name = dll.file_name().unwrap_or_default().to_string_lossy().into_owned();
    static RUN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let out = scratch.join(format!("{name}.{}-{run}.recording.json", std::process::id()));
    std::fs::remove_file(&out).ok();
    let mut child = recorder_command(recorder, &scratch.join("prefix"), dll, manifest, &out)?
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("the recorder didn't start")?;
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > Duration::from_secs(180) {
            child.kill().ok();
            bail!("{name}: the recorder didn't finish in 180 s");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        e.read_to_string(&mut err).ok();
    }
    let text = std::fs::read_to_string(&out).with_context(|| {
        format!(
            "{name}: the recorder wrote no recording (exit {}): {}",
            status.code().unwrap_or(-1),
            err.trim()
        )
    })?;
    let rec: Value = serde_json::from_str(&text).with_context(|| format!("{}", out.display()))?;
    match rec["status"].as_str() {
        Some("ok") | Some("no_register") => Ok(rec),
        other => bail!(
            "{name}: {}: {}",
            describe_status(other.unwrap_or("unknown")),
            rec["error"]
        ),
    }
}

pub fn describe_status(status: &str) -> &'static str {
    match status {
        "crashed" => "SWOFTY_REGISTER crashed",
        "exception" => "SWOFTY_REGISTER threw a C++ exception",
        "timeout" => "SWOFTY_REGISTER didn't return",
        "unexpected" => "SWOFTY_REGISTER called outside the registry, hooks and services",
        "load_failed" => "the DLL didn't load",
        _ => "the recorder failed",
    }
}
