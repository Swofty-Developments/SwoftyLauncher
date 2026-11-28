use std::path::{Path, PathBuf};
use std::process::Command;

struct Root(PathBuf);

impl Root {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-cli-games-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("data")).unwrap();
        std::fs::create_dir_all(root.join("steam/steamapps")).unwrap();
        Self(root)
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn library(&self) -> PathBuf {
        let lib = self.0.join("lib2");
        self.write(
            "steam/steamapps/libraryfolders.vdf",
            &format!(
                r#""libraryfolders" {{ "0" {{ "path" "{}" }} "1" {{ "path" "{}" }} }}"#,
                vdf_path(&self.0.join("steam")),
                vdf_path(&lib)
            ),
        );
        lib
    }

    fn app(&self, lib: &Path, app_id: u32, installdir: &str, buildid: &str, exe: &str) -> PathBuf {
        let rel = lib.strip_prefix(&self.0).unwrap().display().to_string();
        self.write(
            &format!("{rel}/steamapps/appmanifest_{app_id}.acf"),
            &format!(
                r#""AppState" {{ "appid" "{app_id}" "installdir" "{installdir}" "buildid" "{buildid}" }}"#
            ),
        );
        self.write(&format!("{rel}/steamapps/common/{installdir}/{exe}"), "MZ");
        lib.join("steamapps").join("common").join(installdir)
    }

    fn games(&self, args: &[&str]) -> String {
        let mut all = vec!["games"];
        all.extend_from_slice(args);
        self.ok(&all)
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_swofty"))
            .args(args)
            .env("SWOFTY_DATA_DIR", self.0.join("data"))
            .env("SWOFTY_STEAM_ROOT_ONLY", self.0.join("steam"))
            .env("SWOFTY_OC2_CORE_DIR", self.0.join("dist"))
            .env("SWOFTY_USAGE", "0")
            .env_remove("SWOFTY_STEAM_ROOT")
            .env_remove("LD_PRELOAD")
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?} succeeded");
        String::from_utf8(out.stderr).unwrap()
    }

    fn oc2_addon(&self, rel: &str, id: &str, game: &str) -> PathBuf {
        self.write(
            &format!("{rel}/swofty.json"),
            &serde_json::json!({
                "id": id, "name": "Hello Kitchen", "version": "0.1.0", "author": "Swofty",
                "compat": { "game": game, "std": ">=0.1.0" }, "assemblies": ["HelloKitchen.dll"],
            })
            .to_string(),
        );
        self.write(&format!("{rel}/HelloKitchen.dll"), "MZ");
        self.0.join(rel)
    }

    fn core(&self) {
        self.write("dist/core.json", r#"{"core":"0.1.0","std":"0.1.0"}"#);
        self.write("dist/core/Swofty.Overcooked2.Core.dll", "MZ");
        self.write("dist/doorstop/linux_x64/libdoorstop.so", "ELF");
        self.write("dist/doorstop/windows_x64/winhttp.dll", "MZ");
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn vdf_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

#[test]
fn games_json_lists_f1_25_not_found_and_overcooked_2_found_on_steam() {
    let root = Root::new("json");
    let lib = root.library();
    let oc2 = root.app(&lib, 728_880, "Overcooked! 2", "123", "Overcooked2.exe");
    let json: serde_json::Value = serde_json::from_str(&root.games(&["--json"])).unwrap();
    assert_eq!(
        json,
        serde_json::json!([
            { "id": "f1_25", "name": "F1 25", "support": "full", "found": [] },
            {
                "id": "overcooked_2",
                "name": "Overcooked 2",
                "support": "full",
                "found": [{ "dir": oc2, "store": "steam", "build_id": 123 }]
            }
        ])
    );
}

#[test]
fn games_finds_f1_25_where_swofty_game_list_does() {
    let root = Root::new("f1");
    let lib = root.library();
    let f1 = root.app(&lib, 3_059_520, "F1 25", "25251167", "F1_25.exe");
    root.app(&lib, 728_880, "Overcooked! 2", "123", "F1_25.exe");
    let json: serde_json::Value = serde_json::from_str(&root.games(&["--json"])).unwrap();
    assert_eq!(
        json[0]["found"],
        serde_json::json!([{ "dir": f1, "store": "steam", "build_id": 25_251_167 }])
    );
    assert_eq!(json[1]["found"], serde_json::json!([]));
}

#[test]
fn games_says_both_games_are_supported() {
    let root = Root::new("text");
    let lib = root.library();
    let oc2 = root.app(&lib, 728_880, "Overcooked! 2", "123", "Overcooked2.exe");
    assert_eq!(
        root.games(&[]),
        format!(
            "F1 25: supported\n  not found\nOvercooked 2: supported\n  Steam      {}\n",
            oc2.display()
        )
    );
}

#[test]
fn games_finds_the_overcooked_2_linux_build() {
    let root = Root::new("linux");
    let lib = root.library();
    let oc2 = root.app(&lib, 728_880, "Overcooked! 2", "20236421", "Overcooked2.x86_64");
    let json: serde_json::Value = serde_json::from_str(&root.games(&["--json"])).unwrap();
    assert_eq!(
        json[1]["found"],
        serde_json::json!([{ "dir": oc2, "store": "steam", "build_id": 20_236_421 }])
    );
}

#[test]
fn overcooked_2_addons_install_list_disable_enable_and_remove() {
    let root = Root::new("oc2-addons");
    fn with<'a>(rest: &[&'a str]) -> Vec<&'a str> {
        [&["--game", "overcooked_2"][..], rest].concat()
    }
    assert!(
        root.ok(&with(&["addons", "list"]))
            .starts_with("no Overcooked 2 addons installed in ")
    );
    let src = root.oc2_addon("src/hello", "swofty.hello-kitchen", "overcooked_2");
    let src = src.display().to_string();
    assert_eq!(
        root.ok(&with(&["addons", "install", &src])),
        "installed swofty.hello-kitchen 0.1.0 (on)\n"
    );
    let line = |state: &str| {
        format!(
            "{:32} {:10} {state:9} Hello Kitchen\n",
            "swofty.hello-kitchen", "0.1.0"
        )
    };
    assert_eq!(root.ok(&with(&["addons", "list"])), line("enabled"));
    assert_eq!(
        root.ok(&with(&["addons", "disable", "swofty.hello-kitchen"])),
        "disabled swofty.hello-kitchen\n"
    );
    assert_eq!(root.ok(&with(&["addons", "list"])), line("disabled"));
    assert_eq!(
        root.ok(&with(&["addons", "enable", "swofty.hello-kitchen"])),
        "enabled swofty.hello-kitchen\n"
    );
    let enabled = std::fs::read_to_string(root.0.join("data/games/overcooked_2/enabled.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&enabled).unwrap(),
        serde_json::json!({ "enabled": ["swofty.hello-kitchen"] })
    );
    assert!(
        root.fails(&with(&["addons", "disable", "a.nope"]))
            .contains("no addon named a.nope")
    );
    assert_eq!(
        root.ok(&with(&["addons", "remove", "swofty.hello-kitchen"])),
        "removed swofty.hello-kitchen\n"
    );
    assert!(
        !root
            .0
            .join("data/games/overcooked_2/addons/swofty.hello-kitchen")
            .exists()
    );
    assert!(!root.0.join("data/addons/swofty.hello-kitchen").exists());
}

#[test]
fn each_game_refuses_the_others_addons() {
    let root = Root::new("cross");
    let f1 = root.oc2_addon("src/f1", "a.f1", "f1_25").display().to_string();
    let oc2 = root
        .oc2_addon("src/oc2", "a.oc2", "overcooked_2")
        .display()
        .to_string();
    let e = root.fails(&["--game", "overcooked_2", "addons", "install", &f1]);
    assert!(e.contains("is an addon for F1 25, not Overcooked 2"), "{e}");
    let e = root.fails(&["addons", "install", &oc2]);
    assert!(e.contains("is an addon for Overcooked 2, not F1 25"), "{e}");
}

#[test]
fn f1_25_commands_are_refused_for_overcooked_2_and_play_is_refused_for_f1_25() {
    let root = Root::new("scope");
    let e = root.fails(&["--game", "overcooked_2", "build"]);
    assert!(
        e.contains("this command is for F1 25; for Overcooked 2 use addons, play or games"),
        "{e}"
    );
    let e = root.fails(&["play", "--dry-run"]);
    assert!(e.contains("F1 25 plays through Steam or the EA app"), "{e}");
    let e = root.fails(&["--game", "minecraft", "games"]);
    assert!(e.contains("minecraft is not a game the launcher knows"), "{e}");
}

#[test]
fn play_dry_run_prints_the_session_and_the_contract_environment() {
    let root = Root::new("play");
    root.core();
    let lib = root.library();
    let game = root.app(&lib, 728_880, "Overcooked! 2", "20236421", "Overcooked2.x86_64");
    let src = root.oc2_addon("src/hello", "swofty.hello-kitchen", "overcooked_2");
    root.ok(&[
        "--game",
        "overcooked_2",
        "addons",
        "install",
        &src.display().to_string(),
    ]);
    let out = root.ok(&["--game", "overcooked_2", "play", "--dry-run"]);
    let dist = root.0.join("dist");
    for want in [
        format!("cwd {}", game.display()),
        "env SteamAppId=728880".to_owned(),
        "env SteamGameId=728880".to_owned(),
        "env DOORSTOP_ENABLED=1".to_owned(),
        format!(
            "env DOORSTOP_TARGET_ASSEMBLY={}",
            dist.join("core/Swofty.Overcooked2.Core.dll").display()
        ),
        "env SWOFTY_OC2_SESSION=".to_owned(),
        "\"id\": \"swofty.hello-kitchen\"".to_owned(),
        "\"schema\": 1".to_owned(),
    ] {
        assert!(out.contains(&want), "{want} not in\n{out}");
    }
    assert!(!root.0.join("data/games/overcooked_2/sessions").exists());
}
