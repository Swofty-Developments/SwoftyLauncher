use std::process::Command;

use clap::Parser;
use swofty_core::manifest::{AddonManifest, MANIFEST_FILE};
use swofty_formats::pe::{exports, has_export};

use super::addon_logs::{matches, needles};
use super::link::{junction_target, mount_point_buffer};
use super::recording::{REGISTER_EXPORT, native_dlls, wine_path};
use super::scaffold::{short_name, title};
use super::toolchain::which;
use super::*;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    dev: DevCommand,
}

fn parse(args: &[&str]) -> DevCommand {
    Cli::try_parse_from(std::iter::once("dev").chain(args.iter().copied()))
        .unwrap()
        .dev
}

#[test]
fn parses_every_form() {
    assert_eq!(
        parse(&["new", "tails.v10", "--registry"]),
        DevCommand::New {
            id: "tails.v10".into(),
            dir: None,
            registry: true
        }
    );
    assert_eq!(
        parse(&["link", "addons/x"]),
        DevCommand::Link {
            folder: Some("addons/x".into())
        }
    );
    assert_eq!(parse(&["run"]), DevCommand::Run { game: None });
    assert_eq!(
        parse(&["logs", "mod.esp", "-f"]),
        DevCommand::Logs {
            addon: Some("mod.esp".into()),
            follow: true,
            session: None
        }
    );
    assert_eq!(parse(&["check"]), DevCommand::Check { folder: None });
    assert_eq!(
        parse(&["test", "."]),
        DevCommand::Test {
            folder: Some(".".into()),
            recording_only: false
        }
    );
    assert_eq!(
        parse(&["pack", "--zip", "--out", "o"]),
        DevCommand::Pack {
            folder: None,
            out: Some("o".into()),
            zip: true
        }
    );
    assert_eq!(parse(&["on"]), DevCommand::On);
}

#[test]
fn a_folder_after_the_end_of_the_options_can_start_with_a_dash() {
    assert_eq!(
        parse(&["link", "--", "-mods/x"]),
        DevCommand::Link {
            folder: Some("-mods/x".into())
        }
    );
    assert_eq!(
        parse(&["check", "--", "-x"]),
        DevCommand::Check {
            folder: Some("-x".into())
        }
    );
    assert_eq!(
        parse(&["test", "--", "-x"]),
        DevCommand::Test {
            folder: Some("-x".into()),
            recording_only: false
        }
    );
    assert_eq!(
        parse(&["pack", "--", "-x"]),
        DevCommand::Pack {
            folder: Some("-x".into()),
            out: None,
            zip: false
        }
    );
    assert_eq!(
        parse(&["logs", "--", "-x"]),
        DevCommand::Logs {
            addon: Some("-x".into()),
            follow: false,
            session: None
        }
    );
}

#[test]
fn run_delegates_to_run_apply_direct() {
    let args = run_args(Some(Path::new("/games/F1 25")));
    assert_eq!(args, ["run", "--apply", "--direct", "--game=/games/F1 25"]);
    assert!(<RunWrap as Parser>::try_parse_from(run_args(Some(Path::new("-games/F1 25")))).is_ok());
    assert!(<RunWrap as Parser>::try_parse_from(&args).is_ok());
    assert!(<RunWrap as Parser>::try_parse_from(run_args(None)).is_ok());
}

#[test]
fn log_lines_match_the_addon_and_its_dlls_only() {
    let m = serde_json::json!({ "operations": { "native": [ { "dll": "native/esp.dll" } ] } });
    let n = needles("mod.esp", &m);
    assert!(matches(
        "[registry] register: mod.esp (esp.dll) 1 engine_sound",
        &n
    ));
    assert!(matches("[registry] engine_sound mod.esp/v10: added", &n));
    assert!(matches("[loader] loaded esp.dll", &n));
    assert!(!matches("[registry] register: mod.esp2 (esp2.dll)", &n));
    assert!(!matches("[loader] loaded myesp.dll", &n));
    assert!(!matches("[registry] register: mod.espx (x.dll)", &n));
}

#[test]
fn names_and_wine_paths() {
    assert_eq!(short_name("tails.whatif-history"), "whatif_history");
    assert_eq!(title("tails.whatif-history"), "Whatif History");
    if cfg!(unix) {
        assert_eq!(wine_path(Path::new("/a/b c/x.dll")), "Z:\\a\\b c\\x.dll");
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("swofty-dev-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&d).ok();
    d
}

#[test]
fn new_makes_a_registry_addon_from_the_template() {
    let sdk = sdk_root().unwrap();
    let dest = scratch("new");
    new_addon(&sdk, "tails.v10-sound", &dest, true).unwrap();
    let m = manifest_json(&dest).unwrap();
    assert_eq!(m["id"], "tails.v10-sound");
    assert_eq!(m["name"], "V10 Sound");
    assert_eq!(native_dlls(&m), ["native/v10_sound.dll"]);
    assert_eq!(m["compat"]["launcher"], ">=1.6.0");
    assert!(dest.join("native/src/v10_sound.cpp").is_file());
    assert!(dest.join("native/include/swofty/fake_core.hpp").is_file());
    assert!(dest.join("test/register_test.cpp").is_file());
    assert!(!dest.join("native/src/sample_plugin.cpp").exists());
    assert!(
        std::fs::read_to_string(dest.join("native/build.sh"))
            .unwrap()
            .contains("v10_sound.dll")
    );
    assert!(AddonManifest::load(&dest).is_ok());
    assert!(new_addon(&sdk, "tails.v10-sound", &dest, true).is_err());
    assert!(new_addon(&sdk, "bad id", &scratch("bad"), false).is_err());
    let plain = scratch("plain");
    new_addon(&sdk, "a.b", &plain, false).unwrap();
    assert!(plain.join("native/src/sample_plugin.cpp").is_file());
    assert_eq!(manifest_json(&plain).unwrap()["id"], "a.b");
    if which("zig").is_some() {
        let store = Store::new(scratch("new-store"));
        assert_eq!(test(&sdk, &dest, &store, true).unwrap(), ExitCode::SUCCESS);
        assert_eq!(test(&sdk, &dest, &store, false).unwrap(), ExitCode::SUCCESS);
    } else {
        eprintln!("skipped dev test of the new addon: zig isn't installed");
    }
    for d in [dest, plain] {
        std::fs::remove_dir_all(d).ok();
    }
}

#[test]
fn junction_reparse_data_is_a_mount_point() {
    assert_eq!(
        junction_target(r"\\?\C:\Users\RUNNER~1\work/mod.linked").unwrap(),
        r"C:\Users\RUNNER~1\work\mod.linked"
    );
    assert_eq!(junction_target(r"D:\a").unwrap(), r"D:\a");
    assert!(junction_target(r"\\?\UNC\server\share\a").is_err());
    assert!(junction_target(r"\\server\share\a").is_err());
    assert!(junction_target("relative").is_err());
    let buf = mount_point_buffer(r"C:\ab").unwrap();
    let u16_at = |at: usize| u16::from_le_bytes([buf[at], buf[at + 1]]);
    assert_eq!(u32::from_le_bytes(buf[0..4].try_into().unwrap()), 0xA000_0003);
    let substitute = r"\??\C:\ab".len() * 2;
    let print = r"C:\ab".len() * 2;
    assert_eq!(u16_at(4) as usize, 8 + substitute + 2 + print + 2);
    assert_eq!(buf.len(), 8 + u16_at(4) as usize);
    assert_eq!((u16_at(8), u16_at(10)), (0, substitute as u16));
    assert_eq!((u16_at(12), u16_at(14)), (substitute as u16 + 2, print as u16));
    let text: Vec<u16> = buf[16..]
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    assert_eq!(String::from_utf16(&text).unwrap(), "\\??\\C:\\ab\0C:\\ab\0");
    assert!(mount_point_buffer(&"a".repeat(9000)).is_err());
}

#[test]
fn link_points_the_installed_addon_at_the_folder() {
    let root = scratch("link");
    let store = Store::new(root.join("data"));
    let folder = root.join("work/mod.linked");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join(MANIFEST_FILE),
        r#"{"schema_version":1,"id":"mod.linked","name":"Linked","version":"0.1.0","author":"t","kind":["native"],"compat":{"game":"f1_25"}}"#,
    )
    .unwrap();
    let profile = store.active_profile();
    assert_eq!(link(&store, &profile, &folder).unwrap(), "mod.linked");
    assert!(store.addons().unwrap().contains_key("mod.linked"));
    assert!(
        store
            .profile(&profile)
            .unwrap()
            .enabled()
            .any(|id| id == "mod.linked")
    );
    std::fs::write(folder.join("README.md"), "changed").unwrap();
    assert!(store.addons_dir().join("mod.linked/README.md").is_file());
    assert_eq!(link(&store, &profile, &folder).unwrap(), "mod.linked");
    std::fs::remove_dir_all(&root).ok();
}

fn plain_addon(dir: &Path) {
    std::fs::create_dir_all(dir.join("db")).unwrap();
    std::fs::write(
        dir.join(MANIFEST_FILE),
        r#"{"schema_version":1,"id":"mod.plain","name":"Plain","version":"0.1.0","operations":{"database":["db/a.json"]}}"#,
    )
    .unwrap();
    std::fs::write(dir.join("db/a.json"), r#"{"ops":[]}"#).unwrap();
    std::fs::write(dir.join("README.md"), "readme").unwrap();
    std::fs::write(dir.join("notes.txt"), "not packed").unwrap();
}

#[test]
fn packs_an_addon_without_registry_code_like_pack_addon_py() {
    let Ok(py) = python() else {
        eprintln!("skipped: python3 isn't installed");
        return;
    };
    let sdk = sdk_root().unwrap();
    let root = scratch("pack");
    let folder = root.join("mod.plain");
    plain_addon(&folder);
    std::fs::write(folder.join(LOCK_FILE), "{}").unwrap();
    let store = Store::new(root.join("data"));
    assert!(write_lock(&store, &sdk, &folder).unwrap().is_none());
    assert!(!folder.join(LOCK_FILE).exists());
    pack(&sdk, &folder, &root.join("ours"), false, &[]).unwrap();
    let direct = Command::new(py)
        .arg(sdk.join("tools/pack_addon.py"))
        .arg(&folder)
        .arg(root.join("theirs"))
        .status()
        .unwrap();
    assert!(direct.success());
    let ours = std::fs::read(root.join("ours/mod.plain.swoftyaddon")).unwrap();
    let theirs = std::fs::read(root.join("theirs/mod.plain.swoftyaddon")).unwrap();
    assert_eq!(ours, theirs);
    assert!(!String::from_utf8_lossy(&ours).contains("notes.txt"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn packs_hud_files_and_the_files_they_name() {
    if python().is_err() {
        eprintln!("skipped: python3 isn't installed");
        return;
    }
    let sdk = sdk_root().unwrap();
    let root = scratch("pack-hud");
    let folder = root.join("mod.hud");
    std::fs::create_dir_all(folder.join("hud")).unwrap();
    std::fs::create_dir_all(folder.join("art")).unwrap();
    std::fs::write(
        folder.join(MANIFEST_FILE),
        r#"{"schema_version":1,"id":"mod.hud","name":"H","version":"1.0.0","operations":{"hud":["hud/h.json"]}}"#,
    )
    .unwrap();
    std::fs::write(
        folder.join("hud/h.json"),
        r#"{"hud.slot":{"radio":{"name":"Radio","rect":[0.1,0.1,0.2,0.1]}},"hud.element":{"e":{"sprites":{"row":{"png":"art/row.png"}}}}}"#,
    )
    .unwrap();
    std::fs::write(folder.join("art/row.png"), "png").unwrap();
    std::fs::write(folder.join("art/unused.png"), "png").unwrap();
    pack(&sdk, &folder, &root.join("out"), false, &[]).unwrap();
    let packed =
        String::from_utf8_lossy(&std::fs::read(root.join("out/mod.hud.swoftyaddon")).unwrap()).into_owned();
    assert!(packed.contains("hud/h.json"));
    assert!(packed.contains("art/row.png"));
    assert!(!packed.contains("art/unused.png"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn pack_records_a_registry_dll_when_wine_is_there() {
    let sdk = sdk_root().unwrap();
    let built = sdk.join("native/build/recorder-test");
    let has_wine = cfg!(windows) || which("wine").is_some() || std::env::var_os(WINE_ENV).is_some();
    if !built.join("t_register.dll").is_file() || !has_wine || python().is_err() {
        eprintln!(
            "skipped: needs Wine, python3 and the DLLs packages/sdk/recorder/test/run_recorder_test.sh builds"
        );
        return;
    }
    let root = scratch("pack-reg");
    let folder = root.join("t.recorder");
    std::fs::create_dir_all(folder.join("native")).unwrap();
    std::fs::copy(built.join("t_register.dll"), folder.join("native/t_register.dll")).unwrap();
    std::fs::write(
        folder.join(MANIFEST_FILE),
        r#"{"schema_version":1,"id":"t.recorder","name":"T","version":"1.0.0","operations":{"native":[{"dll":"native/t_register.dll"}]}}"#,
    )
    .unwrap();
    let store = Store::new(root.join("data"));
    std::fs::create_dir_all(folder.join("sounds")).unwrap();
    std::fs::write(folder.join("sounds/v10.bdl"), "bdl").unwrap();
    let Recorded { lock, files, missing } = write_lock(&store, &sdk, &folder).unwrap().unwrap();
    assert_eq!(files, ["sounds/v10.bdl"]);
    assert!(missing.is_empty(), "{missing:?}");
    assert_eq!(
        lock.provides["engine_sound"],
        [lock::Provided::Id("t.recorder/v10".into())]
    );
    let lock_text = std::fs::read_to_string(folder.join(LOCK_FILE)).unwrap();
    assert!(lock_text.contains("livery:mod.liveries/red"));
    pack(&sdk, &folder, &root.join("out"), false, &files).unwrap();
    let pkg = std::fs::read(root.join("out/t.recorder.swoftyaddon")).unwrap();
    assert!(String::from_utf8_lossy(&pkg).contains(LOCK_FILE));
    assert!(String::from_utf8_lossy(&pkg).contains("sounds/v10.bdl"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn reads_the_recorder_tests_dlls_when_built() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native/build/recorder-test");
    let (Ok(with), Ok(without)) = (
        std::fs::read(dir.join("t_register.dll")),
        std::fs::read(dir.join("t_plain.dll")),
    ) else {
        eprintln!("skipped: run packages/sdk/recorder/test/run_recorder_test.sh to build the test DLLs");
        return;
    };
    assert!(has_export(&with, REGISTER_EXPORT));
    assert!(!has_export(&without, REGISTER_EXPORT));
    assert!(exports(&without).unwrap().iter().any(|n| n == "SwoftyAddonInit"));
}
