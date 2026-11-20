use std::collections::BTreeMap;
use std::path::PathBuf;

use super::files::*;
use super::wine_override::*;
use super::*;
use crate::Error;
use crate::game_folder::baseline::{Identity, Objects, sha256_bytes};
use crate::game_folder::journal::{Journal, Op};
use crate::profile::VANILLA;

#[test]
fn tells_steam_and_ea_app_folders_apart() {
    let root = std::env::temp_dir().join(format!("swofty-source-{}", std::process::id()));
    let touch = |rel: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    };
    touch("ea/F1 25/F1_25.exe");
    touch("ea/F1 25/__Installer/installerdata.xml");
    touch("steam/F1 25/F1_25.exe");
    touch("steam/F1 25/EAJavelinInstaller_installscript.vdf");
    touch("lib/steamapps/appmanifest_3059520.acf");
    touch("lib/steamapps/common/F1 25/F1_25.exe");
    touch("lib/steamapps/common/F1 25/__Installer/installerdata.xml");
    touch("bare/F1 25/F1_25.exe");
    assert_eq!(Source::detect(&root.join("ea/F1 25")), Source::EaApp);
    assert_eq!(Source::detect(&root.join("steam/F1 25")), Source::Steam);
    assert_eq!(
        Source::detect(&root.join("lib/steamapps/common/F1 25")),
        Source::Steam
    );
    assert_eq!(Source::detect(&root.join("bare/F1 25")), Source::Unknown);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_link_cache_copy_reports_its_progress() {
    let root = std::env::temp_dir().join(format!("swofty-link-progress-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let built = root.join("built");
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let game = Game {
        dir: root.join("game"),
        user_reg: None,
        identity: Identity {
            build_id: "1".into(),
            exe_sha256: String::new(),
        },
    };
    std::fs::create_dir_all(&game.dir).unwrap();
    let mut entries = Vec::new();
    let mut files = Vec::new();
    for n in 0..3u32 {
        let src = outside.join(format!("f{n}.bin"));
        std::fs::write(&src, [n as u8; 8]).unwrap();
        entries.push(crate::game_folder::journal::Entry {
            seq: n,
            op: Op::CreateFile,
            path: format!("mods/f{n}.bin"),
            before: None,
            after: None,
            state: crate::game_folder::journal::State::Pending,
        });
        files.push(src);
    }
    let sources: BTreeMap<&str, &PathBuf> = entries
        .iter()
        .map(|e| e.path.as_str())
        .zip(files.iter())
        .collect();
    let seen = std::cell::RefCell::new(Vec::new());
    let progress = |done: usize, total: usize| seen.borrow_mut().push((done, total));
    let inst = Installer {
        progress: &progress,
        free_space: |_| None,
        ..Installer::new(&root, &game)
    };
    let cache = inst.link_cache(&built, &entries, &sources);
    assert_eq!(cache.synced, Some((3, 0)), "{:?}", cache.failure);
    assert_eq!(seen.into_inner(), [(1, 3), (2, 3), (3, 3)]);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_full_volume_is_named_with_what_the_install_needed_there() {
    let root = std::env::temp_dir().join(format!("swofty-full-{}", std::process::id()));
    let game = Game {
        dir: root.join("game"),
        user_reg: None,
        identity: Identity {
            build_id: "1".into(),
            exe_sha256: String::new(),
        },
    };
    let inst = Installer {
        free_space: |_| Some(5 << 20),
        ..Installer::new(&root, &game)
    };
    let space = crate::platform::disk::volumes([(game.dir.as_path(), 3 << 30)]);
    let v = space[0].1.clone();
    let full = |source: std::io::Error| Error::Io {
        path: root.join("state/journal/x.json.tmp"),
        source,
    };
    match inst.full(full(std::io::ErrorKind::StorageFull.into()), &space) {
        Error::NoSpace { drive, need, free } => {
            assert_eq!((drive, need, free), (v.name, 3 << 30, 5 << 20))
        }
        e => panic!("{e}"),
    }
    assert!(matches!(
        inst.full(full(std::io::ErrorKind::PermissionDenied.into()), &space),
        Error::Io { .. }
    ));
    assert!(matches!(
        inst.full(full(std::io::ErrorKind::StorageFull.into()), &[]),
        Error::Io { .. }
    ));
    let shown = Error::NoSpace {
        drive: "C:".into(),
        need: 3 << 30,
        free: 1 << 29,
    };
    assert_eq!(
        shown.to_string(),
        "not enough free space on C: this profile needs about 3.0 GB, 0.5 GB free"
    );
}

#[test]
fn removes_appended_section_exactly() {
    let orig = "WINE REGISTRY Version 2\n\n[Software\\\\Wine] 1\n\"a\"=\"b\"\n";
    let with = format!("{orig}\n{OVERRIDE_HEADER}\n{OVERRIDE_VALUE}\n");
    assert_eq!(remove_section(&with, OVERRIDE_HEADER).unwrap(), orig);
    let middle = format!("{orig}\n{OVERRIDE_HEADER}\n{OVERRIDE_VALUE}\n\n[Other] 2\n\"x\"=\"y\"\n");
    assert_eq!(
        remove_section(&middle, OVERRIDE_HEADER).unwrap(),
        format!("{orig}\n[Other] 2\n\"x\"=\"y\"\n")
    );
}

#[test]
fn an_empty_section_gets_the_override_and_loses_it_again() {
    let orig = format!(
        "WINE REGISTRY Version 2\n\n{OVERRIDE_HEADER} 1792606023\n#time=1dd6186fa06f04c\n\n[Other] 2\n\"x\"=\"y\"\n"
    );
    assert!(!has_override(&orig));
    let with = add_override(&orig).unwrap();
    assert_eq!(
        with,
        format!(
            "WINE REGISTRY Version 2\n\n{OVERRIDE_HEADER} 1792606023\n#time=1dd6186fa06f04c\n{OVERRIDE_VALUE}\n\n[Other] 2\n\"x\"=\"y\"\n"
        )
    );
    assert!(has_override(&with));
    assert_eq!(add_override(&with), None);
    assert_eq!(
        remove_override(&with).unwrap(),
        "WINE REGISTRY Version 2\n\n[Other] 2\n\"x\"=\"y\"\n"
    );
    assert_eq!(
        remove_override(&orig).unwrap(),
        "WINE REGISTRY Version 2\n\n[Other] 2\n\"x\"=\"y\"\n"
    );
}

#[test]
fn the_override_joins_and_leaves_a_section_with_other_dlls() {
    let orig = format!("WINE REGISTRY Version 2\n\n{OVERRIDE_HEADER} 5\n\"d3d11\"=\"native\"\n");
    let with = add_override(&orig).unwrap();
    assert_eq!(
        with,
        format!("WINE REGISTRY Version 2\n\n{OVERRIDE_HEADER} 5\n{OVERRIDE_VALUE}\n\"d3d11\"=\"native\"\n")
    );
    assert!(has_override(&with));
    assert_eq!(remove_override(&with).unwrap(), orig);
    assert_eq!(remove_override(&orig), None);
    let header_only = format!("WINE REGISTRY Version 2\n\n{OVERRIDE_HEADER}");
    assert_eq!(
        add_override(&header_only).unwrap(),
        format!("{header_only}\n{OVERRIDE_VALUE}\n")
    );
}

#[test]
fn a_version_value_of_the_prefix_is_left_alone() {
    for (value, native) in [
        ("\"version\"=\"native,builtin\"", true),
        ("\"*version\"=\"n,b\"", true),
        ("\"Version\"=\"builtin\"", false),
        ("\"version\"=\"\"", false),
    ] {
        let text = format!("WINE REGISTRY Version 2\n\n{OVERRIDE_HEADER} 5\n{value}\n");
        assert_eq!(has_override(&text), native, "{value}");
        assert_eq!(add_override(&text), None, "{value}");
    }
    let fresh = add_override("WINE REGISTRY Version 2\n").unwrap();
    assert_eq!(
        fresh,
        format!("WINE REGISTRY Version 2\n\n{OVERRIDE_HEADER}\n{OVERRIDE_VALUE}\n")
    );
    assert_eq!(remove_override(&fresh).unwrap(), "WINE REGISTRY Version 2\n");
}

struct Stranded {
    root: PathBuf,
    data: PathBuf,
    game: Game,
}

impl Stranded {
    /// A journal for `old/F1 25`, which is gone, whose files are now in
    /// `moved/F1 25`: `a` and `c` hold what it wrote, `b` and `d` hold
    /// something else, `e` was still pending, and `f` already holds its
    /// original.
    fn new(name: &str, build_id: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-stranded-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let data = root.join("data");
        let dir = root.join("moved/F1 25");
        let objects = Objects {
            root: data.join("baseline/objects"),
        };
        let mut shas = BTreeMap::new();
        for (key, body) in [
            ("orig-a", "orig-a"),
            ("orig-b", "orig-b"),
            ("orig-e", "orig-e"),
            ("orig-f", "orig-f"),
        ] {
            let src = root.join("src").join(key);
            std::fs::create_dir_all(src.parent().unwrap()).unwrap();
            std::fs::write(&src, body).unwrap();
            shas.insert(key, objects.put_file(&src).unwrap());
        }
        for key in ["mod-a", "mod-b", "mod-c", "mod-d", "mod-e"] {
            shas.insert(key, sha256_bytes(key.as_bytes()));
        }
        for (rel, body) in [
            (crate::GAME_EXE, "exe"),
            ("a.erp", "mod-a"),
            ("b.erp", "from-elsewhere"),
            ("swofty/c.dll", "mod-c"),
            ("swofty/d.dll", "another-d"),
            ("e.erp", "partial-e"),
            ("f.erp", "orig-f"),
        ] {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        let mut j = Journal::create(&data, "MODDED", "1", true).unwrap();
        j.game = Some(root.join("old/F1 25"));
        let sha = |k: &str| Some(shas[k].clone());
        j.begin_all([
            (Op::ReplaceFile, "a.erp".to_owned(), sha("orig-a"), sha("mod-a")),
            (Op::ReplaceFile, "b.erp".to_owned(), sha("orig-b"), sha("mod-b")),
            (Op::CreateFile, "swofty/c.dll".to_owned(), None, sha("mod-c")),
            (Op::CreateFile, "swofty/d.dll".to_owned(), None, sha("mod-d")),
            (Op::ReplaceFile, "e.erp".to_owned(), sha("orig-e"), sha("mod-e")),
            (Op::ReplaceFile, "f.erp".to_owned(), sha("orig-f"), sha("mod-e")),
        ])
        .unwrap();
        for i in [0, 1, 2, 3, 5] {
            j.done(i).unwrap();
        }
        j.commit().unwrap();
        let game = Game {
            dir,
            user_reg: None,
            identity: Identity {
                build_id: build_id.into(),
                exe_sha256: String::new(),
            },
        };
        Self { root, data, game }
    }

    fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.game.dir.join(rel)).ok()
    }
}

impl Drop for Stranded {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[test]
fn a_journal_for_a_folder_that_is_gone_rolls_back_only_what_is_provably_ours() {
    let f = Stranded::new("moved", "1");
    let rolled = Installer::new(&f.data, &f.game).roll_back(true).unwrap();
    assert_eq!(rolled.len(), 1);
    let mut drift = rolled[0].drift.clone();
    drift.sort_by_key(|d| d.to_string());
    assert_eq!(
        drift,
        [
            Drift::Stranded("b.erp".into()),
            Drift::Stranded("e.erp".into()),
            Drift::Stranded("swofty/d.dll".into()),
        ]
    );
    assert_eq!(f.read("a.erp").as_deref(), Some("orig-a"));
    assert_eq!(f.read("b.erp").as_deref(), Some("from-elsewhere"));
    assert_eq!(f.read("swofty/c.dll"), None);
    assert_eq!(f.read("swofty/d.dll").as_deref(), Some("another-d"));
    assert_eq!(f.read("e.erp").as_deref(), Some("partial-e"));
    assert_eq!(f.read("f.erp").as_deref(), Some("orig-f"));
    assert!(Journal::open(&f.data).unwrap().is_empty());
    assert_eq!(
        Drift::Stranded("b.erp".into()).to_string(),
        "b.erp is not what the earlier session wrote in the old game folder nor its original; left in place"
    );
}

#[test]
fn a_journal_for_a_folder_that_is_gone_leaves_a_newer_build_alone_and_closes() {
    let f = Stranded::new("rebuilt", "2");
    let rolled = Installer::new(&f.data, &f.game).roll_back(true).unwrap();
    let mut drift = rolled[0].drift.clone();
    drift.sort_by_key(|d| d.to_string());
    assert_eq!(
        drift,
        [
            Drift::Stranded("a.erp".into()),
            Drift::Stranded("b.erp".into()),
            Drift::Stranded("e.erp".into()),
            Drift::Stranded("swofty/d.dll".into()),
        ]
    );
    assert_eq!(f.read("a.erp").as_deref(), Some("mod-a"));
    assert_eq!(
        f.read("swofty/c.dll"),
        None,
        "an added file it wrote is removed on any build"
    );
    assert!(Journal::open(&f.data).unwrap().is_empty());
}

#[test]
fn a_journal_whose_folder_still_has_the_game_treats_pending_files_as_its_own() {
    let f = Stranded::new("still-there", "1");
    let old = f.root.join("old/F1 25");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join(crate::GAME_EXE), "exe").unwrap();
    let rolled = Installer::new(&f.data, &f.game).roll_back(true).unwrap();
    let mut drift = rolled[0].drift.clone();
    drift.sort_by_key(|d| d.to_string());
    assert_eq!(
        drift,
        [
            Drift::LeftInPlace("b.erp".into()),
            Drift::LeftInPlace("swofty/d.dll".into()),
        ]
    );
    assert_eq!(f.read("e.erp").as_deref(), Some("orig-e"));
    assert!(Journal::open(&f.data).unwrap().is_empty());
}

#[test]
fn dir_prefixes() {
    assert_eq!(
        rel_parts("swofty/addons/core/x.dll"),
        ["swofty", "swofty/addons", "swofty/addons/core"]
    );
    assert!(rel_parts("version.dll").is_empty());
}

#[test]
fn each_drift_and_conflict_reads_as_the_player_sees_it() {
    let p = || "a/b.erp".to_owned();
    let drift = [
        (Drift::Changed(p()), "a/b.erp differs from the baseline"),
        (Drift::Missing(p()), "a/b.erp is missing"),
        (Drift::Addition(p()), "a/b.erp is not part of the game"),
        (Drift::RegistryOverride, "the prefix has the version.dll override"),
        (Drift::OpenJournal("j1".into()), "journal j1 is open"),
        (
            Drift::NewBuild {
                baseline: "1.0".into(),
                current: "1.1".into(),
            },
            "game build 1.1 differs from baseline 1.0",
        ),
        (
            Drift::LeftInPlace(p()),
            "a/b.erp changed since it was written (game update?); left in place",
        ),
        (
            Drift::Stranded(p()),
            "a/b.erp is not what the earlier session wrote in the old game folder nor its original; left in \
             place",
        ),
        (
            Drift::NoOriginal(p()),
            "a/b.erp still holds a file swofty wrote and its original is not on record; verify the game \
             files in Steam",
        ),
    ];
    for (d, text) in drift {
        assert_eq!(d.to_string(), text);
    }
    let conflict = |replaced, owned| Conflict {
        path: p(),
        sha256: None,
        owned,
        replaced,
    };
    assert_eq!(
        conflict(true, true).to_string(),
        "a/b.erp still holds what an unfinished session wrote"
    );
    assert_eq!(
        conflict(false, true).to_string(),
        "a/b.erp was left behind by an unfinished session"
    );
    assert_eq!(
        conflict(true, false).to_string(),
        "a/b.erp is not the original, and swofty did not write it"
    );
    assert_eq!(
        conflict(false, false).to_string(),
        "a/b.erp is already in the game folder, and swofty did not put it there"
    );
}

#[test]
fn a_plan_is_vanilla_only_with_nothing_to_place_and_no_override() {
    assert!(Plan::vanilla().is_vanilla());
    assert_eq!(Plan::vanilla().profile, VANILLA);
    let one = || BTreeMap::from([("x".to_owned(), PathBuf::from("y"))]);
    for plan in [
        Plan {
            data: one(),
            ..Plan::vanilla()
        },
        Plan {
            added: one(),
            ..Plan::vanilla()
        },
        Plan {
            loader: one(),
            ..Plan::vanilla()
        },
        Plan {
            registry_override: true,
            ..Plan::vanilla()
        },
    ] {
        assert!(!plan.is_vanilla(), "{plan:?}");
    }
}

mod place;
