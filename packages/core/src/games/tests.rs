use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use swofty_addon::game::{F1_25, GameId, OVERCOOKED_2, StoreId, Support};
use swofty_addon::manifest::Kind;

use super::detect::{ByStores, Detect, Found, Probe, epic_installs};
use super::*;
use crate::platform::steam::{find_app_in, find_game_in};
use crate::session::Config;

const KINDS: [Kind; 7] = [
    Kind::Database,
    Kind::Erp,
    Kind::Text,
    Kind::Ui,
    Kind::Files,
    Kind::Native,
    Kind::Simplified,
];

struct Tree(PathBuf);

impl Tree {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("swofty-games-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn steam_root(&self, libs: &[&Path]) -> PathBuf {
        let root = self.0.join("steam");
        let listed: Vec<String> = std::iter::once(root.as_path())
            .chain(libs.iter().copied())
            .enumerate()
            .map(|(i, l)| format!(r#""{i}" {{ "path" "{}" }}"#, vdf_path(l)))
            .collect();
        self.file(
            "steam/steamapps/libraryfolders.vdf",
            &format!(r#""libraryfolders" {{ {} }}"#, listed.join(" ")),
        );
        root
    }

    fn app(&self, lib: &str, app_id: u32, installdir: &str, buildid: &str, exes: &[&str]) -> PathBuf {
        self.file(
            &format!("{lib}/steamapps/appmanifest_{app_id}.acf"),
            &format!(
                r#""AppState" {{ "appid" "{app_id}" "installdir" "{installdir}" "buildid" "{buildid}" }}"#
            ),
        );
        let dir = self.0.join(lib).join("steamapps/common").join(installdir);
        std::fs::create_dir_all(&dir).unwrap();
        for exe in exes {
            std::fs::write(dir.join(exe), b"MZ").unwrap();
        }
        dir
    }

    fn snapshot(&self) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                if path.is_dir() {
                    out.insert(rel, None);
                    walk(&path, root, out);
                } else {
                    out.insert(rel, Some(std::fs::read(&path).unwrap()));
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(&self.0, &self.0, &mut out);
        out
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn vdf_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

fn probe<'a>(config: &'a Config, steam_roots: &[PathBuf], epic: &[PathBuf]) -> Probe<'a> {
    Probe {
        config,
        steam_roots: steam_roots.to_vec(),
        epic_manifests: epic.to_vec(),
    }
}

const STEAM_F1: StoreId = StoreId::Steam { app_id: 3_059_520 };
const STEAM_OC2: StoreId = StoreId::Steam { app_id: 728_880 };

#[test]
fn f1_25_identity_matches_the_released_constants() {
    assert_eq!(crate::STEAM_APP_ID, 3_059_520);
    assert_eq!(Some(crate::STEAM_APP_ID), F1_25.steam_app_id());
    assert_eq!(crate::GAME_EXE, "F1_25.exe");
    assert_eq!(crate::GAME_ID, "f1_25");
    assert_eq!(swofty_addon::GAME_ID, F1_25.id);
    assert_eq!(F1_25.name, "F1 25");
    assert_eq!(
        crate::platform::ea_app::INSTALL_KEYS,
        [
            r"HKLM\SOFTWARE\Codemasters\F1_25",
            r"HKLM\SOFTWARE\WOW6432Node\Codemasters\F1_25",
            r"HKLM\SOFTWARE\EA Games\F1 25",
            r"HKLM\SOFTWARE\WOW6432Node\EA Games\F1 25",
            r"HKLM\SOFTWARE\EA Sports\F1 25",
            r"HKLM\SOFTWARE\WOW6432Node\EA Sports\F1 25",
        ]
    );
}

#[test]
fn every_game_id_has_one_def_and_round_trips() {
    for game in GameId::ALL {
        let d = def(game);
        assert_eq!(d.info.game, game);
        assert!(std::ptr::eq(by_id(game.id()).unwrap(), d));
        assert_eq!(ALL.iter().filter(|d| d.info.game == game).count(), 1);
    }
    assert_eq!(ALL.len(), GameId::ALL.len());
    assert!(by_id("F1 25").is_none());
    assert_eq!(def(GameId::F1_25).info.id, F1_25.id);
    assert_eq!(def(GameId::Overcooked2).info.id, OVERCOOKED_2.id);
}

#[test]
fn f1_25_has_every_capability_it_had() {
    let d = def(GameId::F1_25);
    assert_eq!(
        d.caps(),
        Capabilities {
            play: true,
            build: true,
            native_core: true,
            online: true,
            time_trial: true,
            addons: true,
        }
    );
    for kind in KINDS {
        assert!(d.accepts_kind(kind), "{kind:?}");
    }
}

#[test]
fn overcooked_2_has_no_capabilities() {
    let d = def(GameId::Overcooked2);
    assert_eq!(d.info.support, Support::NotYet);
    assert_eq!(
        d.caps(),
        Capabilities {
            play: false,
            build: false,
            native_core: false,
            online: false,
            time_trial: false,
            addons: false,
        }
    );
    for kind in KINDS {
        assert!(!d.accepts_kind(kind), "{kind:?}");
    }
}

#[test]
fn find_app_in_finds_what_find_game_in_found() {
    let t = Tree::new("find-app");
    let lib = t.0.join("lib2");
    let root = t.steam_root(&[&lib]);
    std::fs::create_dir_all(lib.join("steamapps/compatdata/3059520/pfx")).unwrap();
    let dir = t.app("lib2", 3_059_520, "F1 25", "25251167", &["F1_25.exe"]);
    let g = find_game_in(&root).unwrap();
    assert_eq!(g.dir, dir);
    assert_eq!(g.library, lib);
    assert_eq!(g.steam_root, root);
    assert_eq!(g.build_id, Some(25_251_167));
    assert_eq!(g.compatdata, Some(lib.join("steamapps/compatdata/3059520")));
    assert_eq!(find_app_in(&root, 3_059_520), Some(g));
    assert_eq!(find_app_in(&root, 728_880), None);
}

#[test]
fn f1_25_is_found_through_its_def_as_the_copies_list_finds_it() {
    let t = Tree::new("f1-def");
    let lib = t.0.join("lib2");
    let root = t.steam_root(&[&lib]);
    let steam_dir = t.app("lib2", 3_059_520, "F1 25", "25251167", &["F1_25.exe"]);
    let added = t.0.join("Games/F1 25 copy");
    std::fs::create_dir_all(&added).unwrap();
    std::fs::write(added.join("F1_25.exe"), b"MZ").unwrap();
    let empty = t.0.join("Games/empty");
    std::fs::create_dir_all(&empty).unwrap();
    let mut config = Config::default();
    config.games.add(added.clone());
    config.games.add(empty);
    config.games.add(steam_dir.clone());
    let found = def(GameId::F1_25).find(&probe(&config, std::slice::from_ref(&root), &[]));
    assert_eq!(
        found,
        vec![
            Found {
                game: GameId::F1_25,
                dir: steam_dir,
                store: Some(STEAM_F1),
                build_id: Some(25_251_167),
            },
            Found {
                game: GameId::F1_25,
                dir: added,
                store: None,
                build_id: None,
            },
        ]
    );
    let copies = f1_25::copies_in(&config, &[root]);
    let dirs: Vec<&PathBuf> = copies.iter().map(|c| &c.dir).collect();
    assert_eq!(dirs, found.iter().map(|f| &f.dir).collect::<Vec<_>>());
}

#[test]
fn f1_25_without_its_exe_is_not_a_copy() {
    let t = Tree::new("f1-no-exe");
    let root = t.steam_root(&[]);
    t.app("steam", 3_059_520, "F1 25", "1", &[]);
    let config = Config::default();
    assert!(def(GameId::F1_25).find(&probe(&config, &[root], &[])).is_empty());
}

#[test]
fn overcooked_2_is_found_in_a_secondary_steam_library() {
    let t = Tree::new("oc2-lib");
    let lib = t.0.join("lib2");
    let root = t.steam_root(&[&lib]);
    let dir = t.app("lib2", 728_880, "Overcooked! 2", "123", &["Overcooked2.exe"]);
    let config = Config::default();
    assert_eq!(
        def(GameId::Overcooked2).find(&probe(&config, &[root], &[])),
        vec![Found {
            game: GameId::Overcooked2,
            dir,
            store: Some(STEAM_OC2),
            build_id: Some(123),
        }]
    );
}

#[test]
fn an_acf_without_the_exe_is_not_a_copy() {
    let t = Tree::new("oc2-no-exe");
    let lib = t.0.join("lib2");
    let root = t.steam_root(&[&lib]);
    t.app("lib2", 728_880, "Overcooked! 2", "123", &[]);
    let config = Config::default();
    assert!(
        ByStores
            .find(&OVERCOOKED_2, &probe(&config, &[root], &[]))
            .is_empty()
    );
}

#[test]
fn f1_and_overcooked_in_one_library_are_told_apart() {
    let t = Tree::new("both");
    let root = t.steam_root(&[]);
    let f1 = t.app("steam", 3_059_520, "F1 25", "25251167", &["F1_25.exe"]);
    let oc2 = t.app("steam", 728_880, "Overcooked! 2", "123", &["Overcooked2.exe"]);
    let mut config = Config::default();
    config.games.add(oc2.clone());
    let roots = [root.clone()];
    let f1_found = def(GameId::F1_25).find(&probe(&config, &roots, &[]));
    assert_eq!(f1_found.iter().map(|f| &f.dir).collect::<Vec<_>>(), [&f1]);
    let oc2_found = def(GameId::Overcooked2).find(&probe(&config, &roots, &[]));
    assert_eq!(oc2_found.iter().map(|f| &f.dir).collect::<Vec<_>>(), [&oc2]);
    assert_eq!(find_game_in(&root).unwrap().dir, f1);
    let gathered = crate::game_folder::copies::gather(None, Vec::new(), std::slice::from_ref(&oc2));
    assert!(gathered.is_empty());
}

#[test]
fn the_wrong_exe_under_the_right_app_id_is_not_a_copy() {
    let t = Tree::new("wrong-exe");
    let root = t.steam_root(&[]);
    t.app("steam", 728_880, "Overcooked! 2", "123", &["F1_25.exe"]);
    t.app("steam", 3_059_520, "F1 25", "1", &["Overcooked2.exe"]);
    let config = Config::default();
    for d in ALL {
        assert!(
            d.find(&probe(&config, std::slice::from_ref(&root), &[]))
                .is_empty(),
            "{}",
            d.info.id
        );
    }
}

#[test]
fn epic_items_find_overcooked_2_by_its_exe() {
    let t = Tree::new("epic");
    let oc2 = t.0.join("Epic/Overcooked2");
    t.file("Epic/Overcooked2/Overcooked2.exe", "MZ");
    t.file("Epic/Other/Other.exe", "MZ");
    let item = |location: &Path, exe: &str| {
        serde_json::json!({ "InstallLocation": location, "LaunchExecutable": exe, "AppName": "x" })
            .to_string()
    };
    t.file("Manifests/A.item", &item(&oc2, "Overcooked2.exe"));
    t.file("Manifests/B.item", &item(&t.0.join("Epic/Other"), "Other.exe"));
    t.file(
        "Manifests/C.item",
        &item(&t.0.join("Epic/Gone"), "Overcooked2.exe"),
    );
    t.file("Manifests/D.txt", &item(&oc2, "Overcooked2.exe"));
    t.file("Manifests/E.item", "not json");
    let manifests = [t.0.join("Manifests")];
    assert_eq!(
        epic_installs(&manifests, "Overcooked2.exe"),
        std::slice::from_ref(&oc2)
    );
    let config = Config::default();
    assert_eq!(
        def(GameId::Overcooked2).find(&probe(&config, &[], &manifests)),
        vec![Found {
            game: GameId::Overcooked2,
            dir: oc2,
            store: Some(StoreId::Epic {
                exe: "Overcooked2.exe"
            }),
            build_id: None,
        }]
    );
}

#[test]
fn the_stub_writes_nothing() {
    let t = Tree::new("writes");
    let lib = t.0.join("lib2");
    let root = t.steam_root(&[&lib]);
    t.app("lib2", 728_880, "Overcooked! 2", "123", &["Overcooked2.exe"]);
    t.file("Manifests/A.item", "{}");
    std::fs::create_dir_all(t.0.join("data")).unwrap();
    let before = t.snapshot();
    let config = Config::default();
    let found = def(GameId::Overcooked2).find(&probe(&config, &[root], &[t.0.join("Manifests")]));
    assert_eq!(found.len(), 1);
    assert_eq!(t.snapshot(), before);
    assert_eq!(std::fs::read_dir(t.0.join("data")).unwrap().count(), 0);
}

#[test]
fn the_stub_starts_no_process_and_opens_no_connection() {
    let sources = [
        ("detect.rs", include_str!("detect.rs")),
        ("overcooked_2.rs", include_str!("overcooked_2.rs")),
    ];
    let banned = [
        "Command",
        "process::",
        "spawn",
        "TcpStream",
        "UdpSocket",
        "http",
        "ureq",
        "reqwest",
        "fs::write",
        "File::create",
        "OpenOptions",
        "create_dir",
        "fs::remove",
        "fs::rename",
        "fs::copy",
    ];
    for (file, text) in sources {
        for word in banned {
            assert!(!text.contains(word), "{file} uses {word}");
        }
    }
}
