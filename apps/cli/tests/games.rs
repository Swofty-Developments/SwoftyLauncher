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
        let out = Command::new(env!("CARGO_BIN_EXE_swofty"))
            .arg("games")
            .args(args)
            .env("SWOFTY_DATA_DIR", self.0.join("data"))
            .env("SWOFTY_STEAM_ROOT_ONLY", self.0.join("steam"))
            .env_remove("SWOFTY_STEAM_ROOT")
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
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
                "support": "not_yet",
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
fn games_says_which_game_is_not_supported_yet() {
    let root = Root::new("text");
    let lib = root.library();
    let oc2 = root.app(&lib, 728_880, "Overcooked! 2", "123", "Overcooked2.exe");
    assert_eq!(
        root.games(&[]),
        format!(
            "F1 25: supported\n  not found\nOvercooked 2: not supported yet\n  Steam      {}\n",
            oc2.display()
        )
    );
}
