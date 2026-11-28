use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;

use super::addons::Store;
use super::core_dir::{self, CoreDir};
use super::launch::{self, Host, Os, Place};
use super::manifest::Manifest;
use super::session;
use super::*;
use crate::catalog::requires::Event;
use crate::games::Addons as _;

struct Tree(PathBuf);

impl Tree {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("swofty-oc2-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn addon(&self, rel: &str, id: &str, version: &str, game: &str) -> PathBuf {
        self.file(
            &format!("{rel}/swofty.json"),
            manifest_json(id, version, game).as_bytes(),
        );
        self.file(&format!("{rel}/{}.dll", dll(id)), b"MZ");
        self.0.join(rel)
    }

    fn core(&self) -> CoreDir {
        self.file(
            "dist/core.json",
            br#"{"core":"0.1.0","std":"0.1.0","harmony":"2.3.3","doorstop":"4.3.0"}"#,
        );
        self.file("dist/core/Swofty.Overcooked2.Core.dll", b"MZ");
        self.file("dist/doorstop/linux_x64/libdoorstop.so", b"\x7fELF");
        self.file("dist/doorstop/windows_x64/winhttp.dll", b"MZ doorstop");
        CoreDir(self.0.join("dist"))
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn dll(id: &str) -> String {
    id.rsplit('.').next().unwrap().replace('-', "")
}

fn manifest_json(id: &str, version: &str, game: &str) -> String {
    serde_json::json!({
        "id": id,
        "name": format!("Addon {id}"),
        "version": version,
        "author": "Swofty",
        "description": "Adds a Hello button to the main menu.",
        "compat": { "game": game, "std": ">=0.1.0" },
        "assemblies": [format!("{}.dll", dll(id))],
    })
    .to_string()
}

fn zip_of(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        z.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn linux() -> Host {
    Host {
        os: Os::Linux,
        ld_preload: None,
        wrapper: None,
    }
}

#[test]
fn the_contract_example_manifest_reads_and_lists() {
    let t = Tree::new("manifest");
    t.file(
        "hello/swofty.json",
        br#"{
          "id": "swofty.hello-kitchen",
          "name": "Hello Kitchen",
          "version": "0.1.0",
          "author": "Swofty",
          "description": "Adds a Hello button to the main menu.",
          "compat": { "game": "overcooked_2", "std": ">=0.1.0" },
          "assemblies": ["HelloKitchen.dll"]
        }"#,
    );
    let m = Manifest::load(&t.0.join("hello")).unwrap();
    assert_eq!(m.id, "swofty.hello-kitchen");
    assert_eq!(m.assemblies, ["HelloKitchen.dll"]);
    assert_eq!(m.compat.std, Some(semver::VersionReq::parse(">=0.1.0").unwrap()));
    assert_eq!(m.missing(&t.0.join("hello")), ["HelloKitchen.dll"]);
    let listed = m.listing();
    assert_eq!(listed.id, "swofty.hello-kitchen");
    assert_eq!(listed.name, "Hello Kitchen");
    assert_eq!(listed.version, semver::Version::new(0, 1, 0));
    assert_eq!(listed.compat.game, "overcooked_2");
    assert_eq!(
        listed.description.as_deref(),
        Some("Adds a Hello button to the main menu.")
    );
}

#[test]
fn a_manifest_for_another_game_or_with_paths_outside_its_folder_is_refused() {
    let t = Tree::new("manifest-bad");
    t.addon("f1", "a.f1", "1.0.0", "f1_25");
    let e = Manifest::load(&t.0.join("f1")).unwrap_err().to_string();
    assert!(e.contains("is an addon for F1 25, not Overcooked 2"), "{e}");
    t.addon("odd", "a.odd", "1.0.0", "some_game");
    let e = Manifest::load(&t.0.join("odd")).unwrap_err().to_string();
    assert!(e.contains("compat.game some_game is not overcooked_2"), "{e}");
    for (rel, text) in [
        (
            "up",
            r#"{"id":"a.up","name":"Up","version":"1.0.0","compat":{"game":"overcooked_2"},"assemblies":["../x.dll"]}"#,
        ),
        (
            "abs",
            r#"{"id":"a.abs","name":"Abs","version":"1.0.0","compat":{"game":"overcooked_2"},"assemblies":["/x.dll"]}"#,
        ),
        (
            "id",
            r#"{"id":"../evil","name":"Evil","version":"1.0.0","compat":{"game":"overcooked_2"}}"#,
        ),
        (
            "noname",
            r#"{"id":"a.b","name":" ","version":"1.0.0","compat":{"game":"overcooked_2"}}"#,
        ),
        ("nocompat", r#"{"id":"a.b","name":"B","version":"1.0.0"}"#),
    ] {
        t.file(&format!("{rel}/swofty.json"), text.as_bytes());
        assert!(Manifest::load(&t.0.join(rel)).is_err(), "{rel}");
    }
}

#[test]
fn the_core_is_found_by_env_then_beside_bin_then_up_a_checkout() {
    let t = Tree::new("core-dir");
    assert_eq!(
        core_dir::find_for(Some(t.0.join("anywhere")), Some(&t.0.join("x/swofty"))),
        Some(CoreDir(t.0.join("anywhere")))
    );
    t.file("bundle/bin/swofty-launcher", b"");
    assert_eq!(
        core_dir::find_for(None, Some(&t.0.join("bundle/bin/swofty-launcher"))),
        None
    );
    t.file(
        "bundle/cores/overcooked_2/core/Swofty.Overcooked2.Core.dll",
        b"MZ",
    );
    assert_eq!(
        core_dir::find_for(None, Some(&t.0.join("bundle/bin/swofty-launcher"))),
        Some(CoreDir(t.0.join("bundle/cores/overcooked_2")))
    );
    t.file(
        "repo/games/overcooked_2/dist/core/Swofty.Overcooked2.Core.dll",
        b"MZ",
    );
    assert_eq!(
        core_dir::find_for(None, Some(&t.0.join("repo/target/debug/swofty-launcher"))),
        Some(CoreDir(t.0.join("repo/games/overcooked_2/dist")))
    );
    assert_eq!(core_dir::find_for(Some(PathBuf::new()), None), None);
    let core = t.core();
    assert_eq!(core.versions().core.as_deref(), Some("0.1.0"));
    assert_eq!(core.versions().doorstop.as_deref(), Some("4.3.0"));
}

#[test]
fn installs_from_a_folder_and_a_zip_come_back_on_and_listed_by_name() {
    let t = Tree::new("install");
    let data = t.0.join("data");
    let store = Store::new(&data);
    t.addon("src/hello", "swofty.hello-kitchen", "0.1.0", "overcooked_2");
    let installed = store.install(&t.0.join("src/hello"));
    assert_eq!(installed.len(), 1);
    let hello = installed.into_iter().next().unwrap().unwrap();
    assert_eq!(
        hello.dir,
        data.join("games/overcooked_2/addons/swofty.hello-kitchen")
    );
    assert!(hello.dir.join("hellokitchen.dll").is_file());
    let zip = zip_of(&[
        (
            "pack/a/swofty.json",
            manifest_json("z.alpha", "1.0.0", "overcooked_2").as_bytes(),
        ),
        ("pack/a/alpha.dll", b"MZ"),
        (
            "pack/b/swofty.json",
            manifest_json("z.beta", "2.0.0", "overcooked_2").as_bytes(),
        ),
        ("pack/b/beta.dll", b"MZ"),
    ]);
    let zipped = t.file("pack.zip", &zip);
    let results = Shelf.install(&data, &zipped);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.as_ref().is_ok_and(|a| a.enabled)));
    let listing = Shelf.list(&data);
    let names: Vec<&str> = listing.addons.iter().map(|a| a.manifest.name.as_str()).collect();
    assert_eq!(
        names,
        ["Addon swofty.hello-kitchen", "Addon z.alpha", "Addon z.beta"]
    );
    assert_eq!(store.enabled(), ["swofty.hello-kitchen", "z.alpha", "z.beta"]);
    assert!(listing.unreadable.is_empty());
    assert!(
        !data
            .join("games/overcooked_2/downloads")
            .read_dir()
            .unwrap()
            .any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".unpack"))
    );
}

#[test]
fn an_f1_addon_or_one_missing_its_assembly_is_not_installed() {
    let t = Tree::new("refuse");
    let data = t.0.join("data");
    t.addon("src/f1", "a.f1", "1.0.0", "f1_25");
    let e = Shelf
        .install(&data, &t.0.join("src/f1"))
        .remove(0)
        .unwrap_err()
        .to_string();
    assert!(e.contains("is an addon for F1 25, not Overcooked 2"), "{e}");
    t.addon("src/gone", "a.gone", "1.0.0", "overcooked_2");
    std::fs::remove_file(t.0.join("src/gone/gone.dll")).unwrap();
    let e = Shelf
        .install(&data, &t.0.join("src/gone"))
        .remove(0)
        .unwrap_err()
        .to_string();
    assert!(e.contains("lists gone.dll but the addon doesn't have it"), "{e}");
    let e = Shelf
        .install(&data, &t.0.join("nowhere"))
        .remove(0)
        .unwrap_err()
        .to_string();
    assert!(e.contains("is not there"), "{e}");
    let empty = t.file("empty.zip", &zip_of(&[("readme.txt", b"hi")]));
    let e = Shelf.install(&data, &empty).remove(0).unwrap_err().to_string();
    assert!(e.contains("has no swofty.json"), "{e}");
    assert!(Shelf.list(&data).addons.is_empty());
}

#[test]
fn the_oc2_addon_is_refused_by_the_f1_25_store() {
    let t = Tree::new("f1-refuses");
    let src = t.addon("src/hello", "swofty.hello-kitchen", "0.1.0", "overcooked_2");
    let store = crate::store::Store::new(&t.0.join("data"));
    let e = store.install_addon(&src).unwrap_err().to_string();
    assert!(e.contains("is an addon for Overcooked 2, not F1 25"), "{e}");
    assert!(!store.addons_dir().join("swofty.hello-kitchen").exists());
}

#[test]
fn enable_disable_and_uninstall_keep_the_load_order_and_the_addon_data() {
    let t = Tree::new("toggle");
    let data = t.0.join("data");
    let store = Store::new(&data);
    for id in ["a.one", "a.two", "a.three"] {
        t.addon(&format!("src/{id}"), id, "1.0.0", "overcooked_2");
        store.install(&t.0.join(format!("src/{id}"))).remove(0).unwrap();
    }
    Shelf.set_enabled(&data, "a.one", false).unwrap();
    Shelf.set_enabled(&data, "a.one", true).unwrap();
    assert_eq!(store.enabled(), ["a.two", "a.three", "a.one"]);
    assert!(Shelf.set_enabled(&data, "a.nope", true).is_err());
    Shelf.set_enabled(&data, "a.nope", false).unwrap();
    std::fs::create_dir_all(store.addon_data_dir("a.two")).unwrap();
    std::fs::write(store.addon_data_dir("a.two").join("save.txt"), "kept").unwrap();
    Shelf.uninstall(&data, "a.two").unwrap();
    assert_eq!(store.enabled(), ["a.three", "a.one"]);
    assert!(!store.addons_dir().join("a.two").exists());
    assert!(store.addon_data_dir("a.two").join("save.txt").is_file());
    assert!(Shelf.uninstall(&data, "a.two").is_err());
    assert!(Shelf.uninstall(&data, "../data").is_err());
    let listing = Shelf.list(&data);
    let on: Vec<(&str, bool)> = listing
        .addons
        .iter()
        .map(|a| (a.manifest.id.as_str(), a.enabled))
        .collect();
    assert_eq!(on, [("a.one", true), ("a.three", true)]);
    Shelf.set_enabled(&data, "a.three", false).unwrap();
    assert_eq!(
        store
            .on()
            .iter()
            .map(|a| a.manifest.id.as_str())
            .collect::<Vec<_>>(),
        ["a.one"]
    );
}

#[test]
fn a_broken_folder_is_listed_as_unreadable_and_left_out_of_the_session() {
    let t = Tree::new("unreadable");
    let data = t.0.join("data");
    let store = Store::new(&data);
    t.file("data/games/overcooked_2/addons/a.bad/swofty.json", b"{ not json");
    t.addon(
        "data/games/overcooked_2/addons/a.moved",
        "a.other",
        "1.0.0",
        "overcooked_2",
    );
    std::fs::create_dir_all(data.join("games/overcooked_2/addons/.staging-x")).unwrap();
    let listing = Shelf.list(&data);
    assert!(listing.addons.is_empty());
    let ids: Vec<&str> = listing.unreadable.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["a.bad", "a.moved"]);
    assert!(listing.unreadable[1].reason.contains("names a.other"));
    assert!(store.on().is_empty());
}

#[test]
fn the_session_lists_the_addons_on_in_load_order_as_the_contract_says() {
    let t = Tree::new("session");
    let data = t.0.join("data");
    let store = Store::new(&data);
    for id in ["b.second", "a.first", "c.off"] {
        t.addon(&format!("src/{id}"), id, "0.1.0", "overcooked_2");
        store.install(&t.0.join(format!("src/{id}"))).remove(0).unwrap();
    }
    store.set_enabled("c.off", false).unwrap();
    let game = t.0.join("game");
    std::fs::create_dir_all(&game).unwrap();
    let secs = crate::catalog::epoch("2026-11-29T00:15:02Z").unwrap();
    assert_eq!(session::stamp(secs), "20261129-001502");
    let written = session::write(&store, &game, "1.10.63", secs).unwrap();
    let again = session::write(&store, &game, "1.10.63", secs).unwrap();
    assert_eq!(
        written.dir,
        data.join("games/overcooked_2/sessions/20261129-001502")
    );
    assert_eq!(
        again.dir,
        data.join("games/overcooked_2/sessions/20261129-001502-2")
    );
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&written.path).unwrap()).unwrap();
    let addon = |id: &str| {
        serde_json::json!({
            "id": id,
            "name": format!("Addon {id}"),
            "version": "0.1.0",
            "dir": data.join("games/overcooked_2/addons").join(id),
            "assemblies": [format!("{}.dll", dll(id))],
        })
    };
    assert_eq!(
        json,
        serde_json::json!({
            "schema": 1,
            "launcher": "1.10.63",
            "game_dir": game,
            "data_dir": data.join("games/overcooked_2"),
            "log": written.dir.join("core.log"),
            "status": written.dir.join("status.json"),
            "addons": [addon("b.second"), addon("a.first")],
        })
    );
    assert_eq!(session::latest(&store), Some(again.dir.clone()));
    assert_eq!(written.status(), None);
    std::fs::write(
        written.dir.join("status.json"),
        br#"{"core":"0.1.0","std":"0.1.0","loaded":["b.second"],"failed":[{"id":"a.first","error":"boom"}],"main_menu":true}"#,
    )
    .unwrap();
    let status = written.status().unwrap();
    assert_eq!(status.loaded, ["b.second"]);
    assert_eq!(status.failed[0].error, "boom");
    assert!(status.main_menu);
}

#[test]
fn the_linux_launch_preloads_doorstop_with_the_contract_environment() {
    let t = Tree::new("plan-linux");
    let core = t.core();
    let game = t.0.join("game");
    t.file("game/Overcooked2.x86_64", b"\x7fELF");
    let session = t.0.join("data/games/overcooked_2/sessions/s/session.json");
    let plan = launch::plan(&game, &core, &session, &linux()).unwrap();
    assert_eq!(plan.program, game.join("Overcooked2.x86_64"));
    assert_eq!(plan.exe, game.join("Overcooked2.x86_64"));
    assert_eq!(plan.cwd, game);
    assert_eq!(
        plan.args,
        [
            "-logFile".to_owned(),
            session.with_file_name("player.log").display().to_string()
        ]
    );
    let env: std::collections::BTreeMap<&str, &str> =
        plan.env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let preload = core
        .0
        .join("doorstop/linux_x64/libdoorstop.so")
        .display()
        .to_string();
    let target = core
        .0
        .join("core/Swofty.Overcooked2.Core.dll")
        .display()
        .to_string();
    let session_text = session.display().to_string();
    assert_eq!(
        env,
        [
            ("DOORSTOP_ENABLED", "1"),
            ("DOORSTOP_TARGET_ASSEMBLY", target.as_str()),
            ("LD_PRELOAD", preload.as_str()),
            ("SWOFTY_OC2_SESSION", session_text.as_str()),
            ("SteamAppId", "728880"),
            ("SteamGameId", "728880"),
        ]
        .into_iter()
        .collect()
    );
    assert!(plan.place.is_empty());
    let chained = Host {
        ld_preload: Some("/usr/lib/overlay.so".into()),
        ..linux()
    };
    let plan = launch::plan(&game, &core, &session, &chained).unwrap();
    assert!(
        plan.env
            .contains(&("LD_PRELOAD".into(), format!("{preload}:/usr/lib/overlay.so")))
    );
}

#[test]
fn without_the_system_elf_loader_the_game_runs_through_steam_run_with_doorstop_inside() {
    let t = Tree::new("plan-wrapped");
    let core = t.core();
    let game = t.0.join("game");
    t.file("game/Overcooked2.x86_64", b"\x7fELF");
    let session = t.0.join("s/session.json");
    let host = Host {
        wrapper: Some(PathBuf::from("/run/current-system/sw/bin/steam-run")),
        ..linux()
    };
    let plan = launch::plan(&game, &core, &session, &host).unwrap();
    assert_eq!(
        plan.program,
        PathBuf::from("/run/current-system/sw/bin/steam-run")
    );
    let preload = core
        .0
        .join("doorstop/linux_x64/libdoorstop.so")
        .display()
        .to_string();
    assert_eq!(plan.args[0], "env");
    assert_eq!(plan.args[1], format!("LD_PRELOAD={preload}"));
    assert_eq!(
        plan.args[2],
        game.join("Overcooked2.x86_64").display().to_string()
    );
    assert_eq!(plan.args[3], "-logFile");
    assert!(!plan.env.iter().any(|(k, _)| k == "LD_PRELOAD"));
    assert!(plan.env.iter().any(|(k, v)| k == "DOORSTOP_ENABLED" && v == "1"));
    assert_eq!(plan.exe, game.join("Overcooked2.x86_64"));
}

#[test]
fn the_windows_launch_places_doorstop_beside_the_exe_and_takes_it_out_after() {
    let t = Tree::new("plan-windows");
    let core = t.core();
    let game = t.0.join("game");
    t.file("game/Overcooked2.exe", b"MZ");
    let session = t.0.join("s/session.json");
    let host = Host {
        os: Os::Windows,
        ..linux()
    };
    let plan = launch::plan(&game, &core, &session, &host).unwrap();
    assert_eq!(plan.program, game.join("Overcooked2.exe"));
    assert!(
        !plan
            .env
            .iter()
            .any(|(k, _)| k.starts_with("DOORSTOP") || k == "LD_PRELOAD")
    );
    assert!(
        plan.env
            .iter()
            .any(|(k, v)| k == "SWOFTY_OC2_SESSION" && *v == session.display().to_string())
    );
    let ini = format!(
        "[General]\r\nenabled = true\r\ntarget_assembly = {}\r\n",
        core.target_assembly().display()
    );
    assert_eq!(
        plan.place,
        [
            Place::Copy {
                from: core.0.join("doorstop/windows_x64/winhttp.dll"),
                to: game.join("winhttp.dll"),
            },
            Place::Write {
                to: game.join("doorstop_config.ini"),
                text: ini.clone(),
            },
        ]
    );
    let placed = plan.place().unwrap();
    assert_eq!(std::fs::read(game.join("winhttp.dll")).unwrap(), b"MZ doorstop");
    assert_eq!(
        std::fs::read_to_string(game.join("doorstop_config.ini")).unwrap(),
        ini
    );
    assert_eq!(plan.place().unwrap(), placed);
    let started = Started {
        child: std::process::Command::new(std::env::current_exe().unwrap())
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
        session: t.0.join("s"),
        exe: game.join("Overcooked2.exe"),
        placed,
    };
    let mut child = started.child;
    child.wait().unwrap();
    let started = Started { child, ..started };
    started.finish();
    assert!(!game.join("winhttp.dll").exists());
    assert!(!game.join("doorstop_config.ini").exists());
    std::fs::write(game.join("winhttp.dll"), b"someone else's loader").unwrap();
    let e = plan.place().unwrap_err().to_string();
    assert!(e.contains("is already in the game folder"), "{e}");
    assert!(!game.join("doorstop_config.ini").exists());
}

#[test]
fn the_windows_build_on_linux_is_not_supported_yet_and_a_folder_without_the_game_is_refused() {
    let t = Tree::new("plan-proton");
    let core = t.core();
    t.file("proton/Overcooked2.exe", b"MZ");
    let e = launch::plan(&t.0.join("proton"), &core, &t.0.join("s.json"), &linux())
        .unwrap_err()
        .to_string();
    assert!(e.contains("under Proton is not supported yet"), "{e}");
    std::fs::create_dir_all(t.0.join("empty")).unwrap();
    let e = launch::plan(&t.0.join("empty"), &core, &t.0.join("s.json"), &linux())
        .unwrap_err()
        .to_string();
    assert!(e.ends_with("has no Overcooked2.x86_64"), "{e}");
}

#[cfg(unix)]
#[test]
fn play_writes_the_session_and_starts_the_game_with_its_environment() {
    use std::os::unix::fs::PermissionsExt;
    let t = Tree::new("start");
    let core = t.core();
    let data = t.0.join("data");
    t.addon("src/hello", "swofty.hello-kitchen", "0.1.0", "overcooked_2");
    Store::new(&data)
        .install(&t.0.join("src/hello"))
        .remove(0)
        .unwrap();
    let exe = t.file(
        "game/Overcooked2.x86_64",
        b"#!/bin/sh\nenv | grep -E '^(DOORSTOP|SWOFTY_OC2|SteamAppId|SteamGameId|LD_PRELOAD)' | sort\necho \"cwd=$(pwd)\"\necho \"args=$*\"\n",
    );
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let game = t.0.join("game");
    // SAFETY: this test is the only one that sets the core folder variable.
    unsafe { std::env::set_var(core_dir::ENV, &core.0) };
    let mut started = start(&data, &game, &linux(), "1.10.63").unwrap();
    unsafe { std::env::remove_var(core_dir::ENV) };
    assert!(started.child.wait().unwrap().success());
    started.finish();
    assert_eq!(started.game_pid(), started.pid());
    let out = std::fs::read_to_string(started.session.join("output.log")).unwrap();
    let session = started.session.join("session.json");
    assert!(
        out.contains(&format!("SWOFTY_OC2_SESSION={}", session.display())),
        "{out}"
    );
    assert!(out.contains(&format!(
        "DOORSTOP_TARGET_ASSEMBLY={}",
        core.target_assembly().display()
    )));
    assert!(out.contains(&format!("LD_PRELOAD={}", core.doorstop_linux().display())));
    assert!(out.contains("DOORSTOP_ENABLED=1"));
    assert!(out.contains("SteamAppId=728880"));
    assert!(out.contains(&format!("cwd={}", game.display())));
    assert!(out.contains(&format!(
        "args=-logFile {}",
        started.session.join("player.log").display()
    )));
    let json: session::Session = serde_json::from_slice(&std::fs::read(&session).unwrap()).unwrap();
    assert_eq!(json.addons.len(), 1);
    assert_eq!(json.addons[0].id, "swofty.hello-kitchen");
    assert!(
        started
            .session
            .starts_with(data.join("games/overcooked_2/sessions"))
    );
}

#[test]
fn the_feed_lists_overcooked_2_addons_and_saves_them_in_the_game_folder() {
    let t = Tree::new("feed");
    let feed = Shelf.feed(&t.0);
    assert!(feed.url.ends_with("/v1/addons?game=overcooked_2"), "{}", feed.url);
    assert_eq!(feed.cache, t.0.join("games/overcooked_2/catalog.json"));
    assert_ne!(feed.cache, crate::catalog::Feed::f1_25(&t.0).cache);
    assert_eq!(feed.cached(), None);
    t.file(
        "games/overcooked_2/catalog.json",
        br#"{"addons":[{"id":"swofty.hello-kitchen","name":"Hello Kitchen"}]}"#,
    );
    assert_eq!(feed.cached().unwrap()[0].id, "swofty.hello-kitchen");
    assert!(crate::catalog::cached(&t.0).is_none());
}

fn serve(files: Vec<(String, Vec<u8>)>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let files = Arc::new(files);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let files = files.clone();
            std::thread::spawn(move || {
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
                let body = files.iter().find(|(p, _)| *p == path).map(|(_, b)| b.clone());
                let (status, body) = body.map_or((404, Vec::new()), |b| (200, b));
                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).ok();
                stream.write_all(&body).ok();
                stream.shutdown(std::net::Shutdown::Write).ok();
                std::io::copy(&mut reader, &mut std::io::sink()).ok();
            });
        }
    });
    base
}

fn listed(
    base: &str,
    zip: &[u8],
    id: &str,
    version: &str,
    game: &str,
) -> (crate::catalog::Addon, crate::catalog::Version) {
    let v: crate::catalog::Version = serde_json::from_value(serde_json::json!({
        "version": version,
        "game": game,
        "size": zip.len(),
        "sha256": crate::game_folder::baseline::sha256_bytes(zip),
        "url": format!("{base}/{id}.zip"),
    }))
    .unwrap();
    let addon: crate::catalog::Addon =
        serde_json::from_value(serde_json::json!({ "id": id, "name": "Hello Kitchen" })).unwrap();
    (addon, v)
}

#[test]
fn the_addon_browser_install_downloads_checks_and_installs_into_the_oc2_store() {
    let t = Tree::new("listed");
    let data = t.0.join("data");
    let good = zip_of(&[
        (
            "hello/swofty.json",
            manifest_json("swofty.hello-kitchen", "0.1.0", "overcooked_2").as_bytes(),
        ),
        ("hello/hellokitchen.dll", b"MZ"),
    ]);
    let wrong = zip_of(&[
        (
            "hello/swofty.json",
            manifest_json("swofty.other", "0.1.0", "overcooked_2").as_bytes(),
        ),
        ("hello/other.dll", b"MZ"),
    ]);
    let base = serve(vec![
        ("/swofty.hello-kitchen.zip".into(), good.clone()),
        ("/swofty.other.zip".into(), wrong.clone()),
    ]);
    let (addon, v) = listed(&base, &good, "swofty.hello-kitchen", "0.1.0", "overcooked_2");
    let events = std::cell::RefCell::new(Vec::new());
    let done = Shelf
        .install_listed(&data, &addon, &v, &|e| {
            events.borrow_mut().push(match e {
                Event::Addon(a, _) => format!("addon {}", a.id),
                Event::Stage(s) => format!("{s:?}").split('(').next().unwrap().to_owned(),
                Event::Installed(m) => format!("installed {}", m.id),
                Event::Planned(_) => "planned".into(),
            })
        })
        .unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].id, "swofty.hello-kitchen");
    let mut seen = events.into_inner();
    seen.dedup();
    assert_eq!(
        seen,
        [
            "addon swofty.hello-kitchen",
            "Downloading",
            "Verifying",
            "Installing",
            "installed swofty.hello-kitchen"
        ]
    );
    let listing = Shelf.list(&data);
    assert_eq!(listing.addons.len(), 1);
    assert!(listing.addons[0].enabled);
    assert!(
        !data
            .join("games/overcooked_2/downloads/swofty.hello-kitchen-0.1.0.zip")
            .exists()
    );
    let (other, v) = listed(&base, &wrong, "swofty.other", "0.1.0", "overcooked_2");
    let (_, mut lying) = listed(&base, &wrong, "swofty.other", "0.1.0", "overcooked_2");
    lying.sha256 = "00".repeat(32);
    assert!(Shelf.install_listed(&data, &other, &lying, &|_| {}).is_err());
    let renamed: crate::catalog::Addon =
        serde_json::from_value(serde_json::json!({ "id": "swofty.renamed", "name": "R" })).unwrap();
    let e = Shelf
        .install_listed(&data, &renamed, &v, &|_| {})
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("the registry lists swofty.renamed 0.1.0, the download holds swofty.other 0.1.0"),
        "{e}"
    );
    let (_, f1) = listed(&base, &good, "swofty.hello-kitchen", "0.1.0", "f1_25");
    let e = Shelf
        .install_listed(&data, &addon, &f1, &|_| {})
        .unwrap_err()
        .to_string();
    assert!(e.contains("for f1_25, not Overcooked 2"), "{e}");
    assert_eq!(Shelf.list(&data).addons.len(), 1);
}
