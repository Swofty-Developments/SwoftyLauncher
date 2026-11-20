use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result, bail};
use swofty_core::store::Store;

use crate::commands::dev::recording::recorder_dir;
use crate::commands::dev::toolchain::{compiler, which};
use crate::commands::dev::{data_root, plural};

pub const ENGINE_SOURCES: &[&str] = &[
    "store.cpp",
    "table.cpp",
    "validate.cpp",
    "schema.cpp",
    "engine.cpp",
];

fn zig_cache(cmd: &mut Command, store: &Store) {
    if which("zig").is_some() && std::env::var_os("ZIG_GLOBAL_CACHE_DIR").is_none() {
        let cache = data_root(store).join("cache/zig");
        cmd.env("ZIG_GLOBAL_CACHE_DIR", &cache)
            .env("ZIG_LOCAL_CACHE_DIR", &cache);
    }
}

fn engine_objects(sdk: &Path, cxx: &Path, pre: &[String], out: &Path, store: &Store) -> Result<Vec<PathBuf>> {
    let src = sdk.join("native/src/core/registry");
    let dir = out.join("engine");
    std::fs::create_dir_all(&dir)?;
    let mut objects = Vec::new();
    let mut jobs = Vec::new();
    for name in ENGINE_SOURCES {
        let source = src.join(name);
        if !source.is_file() {
            bail!(
                "the core's registry engine isn't in the SDK ({} is missing); run with --recording-only to test without it",
                source.display()
            );
        }
        let obj = dir.join(name.replace(".cpp", ".o"));
        let fresh = |p: &Path| p.metadata().and_then(|m| m.modified()).ok();
        if fresh(&obj).zip(fresh(&source)).is_none_or(|(o, s)| o < s) {
            let mut cmd = Command::new(cxx);
            cmd.args(pre)
                .args(test_flags(cxx))
                .arg(format!("-I{}", sdk.join("packages/sdk/include").display()))
                .arg("-c")
                .arg(&source)
                .arg("-o")
                .arg(&obj)
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            zig_cache(&mut cmd, store);
            jobs.push((name, cmd.spawn().context("the C++ compiler didn't start")?));
        }
        objects.push(obj);
    }
    for (name, job) in jobs {
        let o = job.wait_with_output()?;
        if !o.status.success() {
            bail!(
                "the registry engine's {name} doesn't build: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            );
        }
    }
    Ok(objects)
}

pub fn test(sdk: &Path, folder: &Path, store: &Store, engine: bool) -> Result<ExitCode> {
    let dir = folder.join("test");
    let mut tests: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "cpp"))
                .collect()
        })
        .unwrap_or_default();
    tests.sort();
    if tests.is_empty() {
        println!("no tests: put unit tests in {}/*.cpp", dir.display());
        return Ok(ExitCode::SUCCESS);
    }
    let (cxx, pre) = compiler()?;
    let out = folder.join("build/test");
    std::fs::create_dir_all(&out)?;
    let objects = if engine {
        engine_objects(sdk, &cxx, &pre, &out, store)?
    } else {
        Vec::new()
    };
    if cfg!(windows)
        && let Ok(rec) = recorder_dir(sdk)
    {
        std::fs::copy(rec.join("swofty_core.dll"), out.join("swofty_core.dll")).ok();
    }
    let mut failed = 0;
    for t in &tests {
        let name = t.file_stem().unwrap_or_default().to_string_lossy().into_owned();
        let exe = out.join(if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.clone()
        });
        let mut cmd = Command::new(&cxx);
        cmd.args(&pre)
            .args(test_flags(&cxx))
            .arg(format!("-I{}", folder.join("native/include").display()))
            .arg(format!("-I{}", sdk.join("packages/sdk/include").display()))
            .arg(format!("-I{}", folder.join("native/src").display()))
            .args(engine.then_some("-DSWOFTY_FAKE_CORE_ENGINE=swofty_registry_engine"))
            .arg("-o")
            .arg(&exe)
            .arg(t)
            .args(&objects);
        zig_cache(&mut cmd, store);
        let built = cmd.output().context("the C++ compiler didn't start")?;
        if !built.status.success() {
            println!("FAIL  {name}: it doesn't build");
            print!("{}", String::from_utf8_lossy(&built.stderr));
            failed += 1;
            continue;
        }
        let ran = Command::new(&exe).current_dir(folder).output()?;
        print!("{}", String::from_utf8_lossy(&ran.stdout));
        if ran.status.success() {
            println!("ok    {name}");
        } else {
            print!("{}", String::from_utf8_lossy(&ran.stderr));
            println!("FAIL  {name}");
            failed += 1;
        }
    }
    println!("{} test{}, {failed} failed", tests.len(), plural(tests.len()));
    Ok(if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

pub fn test_flags(cxx: &Path) -> Vec<&'static str> {
    let gcc = cxx
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains("g++") || n == "gcc" || n == "c++");
    let mut flags = vec!["-std=c++23", "-O1", "-Wno-ignored-attributes"];
    if !gcc {
        flags.extend([
            "-fdeclspec",
            "-Wno-nullability-completeness",
            "-Wno-unknown-warning-option",
            "-Wno-cast-function-type-mismatch",
        ]);
    }
    flags
}
