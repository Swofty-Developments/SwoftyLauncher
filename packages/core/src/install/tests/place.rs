use std::cell::RefCell;
use std::path::Path;

use super::*;
use crate::game_folder::placed::Placed;
use crate::install::checks::refuse_drop_ins;
use crate::install::copy::{Copied, Linker, copy_report};
use crate::platform::disk::Volume;

const GVDB: &str = "game_package/gamedatabases/gamedatabases.2025.bin";
const UI: &str = "2025_asset_groups/ui_package/flow_frontend.erp";
const FIRST: &str = "2025_asset_groups/ui_package/first.erp";
const SECOND: &str = "2025_asset_groups/ui_package/second.erp";
const MAP: &str = "2025_asset_groups/environment_package/tracks/okayama/wep/okayama_common.erp";
const MAP_AI: &str = "2025_asset_groups/environment_package/tracks/okayama/ai/okayama_ai.erp";
const CORE: &str = "swofty/addons/core/swofty_core.dll";
const REG: &str = "WINE REGISTRY Version 2\n\n[Software\\\\Wine] 1700000000\n\"Version\"=\"win10\"\n";

struct Fixture {
    root: PathBuf,
    game: PathBuf,
    data: PathBuf,
    built: PathBuf,
    loose: PathBuf,
    reg: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-place-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let f = Self {
            game: root.join("game"),
            data: root.join("data"),
            built: root.join("data/cache/overlay/MODDED/0123456789abcdef"),
            loose: root.join("loose"),
            reg: root.join("pfx/user.reg"),
            root,
        };
        for (rel, body) in [("F1_25.exe", "exe-v1"), (GVDB, "db-v1"), (UI, "ui-v1")] {
            put(&f.game.join(rel), body);
        }
        put(&f.reg, REG);
        f
    }

    fn game(&self) -> Game {
        Game {
            dir: self.game.clone(),
            user_reg: Some(self.reg.clone()),
            identity: Identity::detect(&self.game, Some(1)).unwrap(),
        }
    }

    fn built(&self, rel: &str, body: &str) -> PathBuf {
        let at = self.built.join(rel);
        put(&at, body);
        at
    }

    fn loose(&self, rel: &str, body: &str) -> PathBuf {
        let at = self.loose.join(rel);
        put(&at, body);
        at
    }

    fn plan(&self) -> Plan {
        Plan {
            profile: "MODDED".into(),
            data: BTreeMap::new(),
            added: BTreeMap::new(),
            loader: BTreeMap::new(),
            registry_override: false,
        }
    }

    fn snapshot(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        walk(&self.game, &self.game, &mut out);
        out.retain(|k, _| !k.starts_with("swofty-cache"));
        out.insert("<user.reg>".into(), std::fs::read_to_string(&self.reg).unwrap());
        out
    }

    fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.game.join(rel)).ok()
    }

    fn rolled_back(&self, game: &Game, original: &BTreeMap<String, String>) {
        Installer::new(&self.data, game).roll_back(false).unwrap();
        assert_eq!(&self.snapshot(), original);
        assert!(Journal::open(&self.data).unwrap().is_empty());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn put(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
        if p.is_dir() {
            out.insert(format!("{rel}/"), String::new());
            walk(base, &p, out);
        } else {
            out.insert(rel, std::fs::read_to_string(&p).unwrap_or_default());
        }
    }
}

fn roomy(_: &Path) -> Option<u64> {
    Some(1 << 40)
}

fn one(_: &Path) -> Option<Volume> {
    Some(Volume {
        id: "one".into(),
        name: "one".into(),
    })
}

fn installer<'a>(f: &'a Fixture, game: &'a Game, out: &'a dyn Fn(&str)) -> Installer<'a> {
    Installer {
        out,
        free_space: roomy,
        volume: one,
        copy_only: false,
        ..Installer::new(&f.data, game)
    }
}

fn mode_line(lines: &[String]) -> &str {
    let modes: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("installing: ") && l.contains(" file(s) linked, "))
        .collect();
    assert_eq!(modes.len(), 1, "{lines:?}");
    modes[0]
}

#[test]
fn the_journal_lists_the_replacements_then_each_new_folder_and_file_then_the_override() {
    let f = Fixture::new("order");
    let game = f.game();
    let original = f.snapshot();
    let mut plan = f.plan();
    plan.data.insert(UI.into(), f.built(UI, "ui-modded"));
    plan.data.insert(GVDB.into(), f.built(GVDB, "db-modded"));
    plan.added.insert(MAP_AI.into(), f.built(MAP_AI, "ai"));
    plan.added.insert(MAP.into(), f.built(MAP, "map"));
    plan.loader.insert(PROXY.into(), f.loose("proxy", "proxy"));
    plan.loader.insert(CORE.into(), f.loose("core", "core"));
    plan.registry_override = true;
    let inst = Installer {
        stop_after: Some(0),
        ..installer(&f, &game, &quiet)
    };
    let err = inst.apply(&plan).unwrap_err();
    assert_eq!(
        err.to_string(),
        Error::Other("stopped after 0 step(s)".into()).to_string()
    );
    assert_eq!(f.snapshot(), original);
    let open = Journal::open(&f.data).unwrap();
    assert_eq!(open.len(), 1);
    let j = &open[0];
    assert_eq!(j.game.as_deref(), Some(f.game.as_path()));
    let steps: Vec<(Op, &str)> = j.entries.iter().map(|e| (e.op, e.path.as_str())).collect();
    let reg = f.reg.display().to_string();
    assert_eq!(
        steps,
        [
            (Op::ReplaceFile, UI),
            (Op::ReplaceFile, GVDB),
            (Op::CreateDir, "2025_asset_groups/environment_package"),
            (Op::CreateDir, "2025_asset_groups/environment_package/tracks"),
            (
                Op::CreateDir,
                "2025_asset_groups/environment_package/tracks/okayama"
            ),
            (
                Op::CreateDir,
                "2025_asset_groups/environment_package/tracks/okayama/ai"
            ),
            (Op::CreateFile, MAP_AI),
            (
                Op::CreateDir,
                "2025_asset_groups/environment_package/tracks/okayama/wep"
            ),
            (Op::CreateFile, MAP),
            (Op::CreateDir, "swofty"),
            (Op::CreateDir, "swofty/addons"),
            (Op::CreateDir, "swofty/addons/core"),
            (Op::CreateFile, CORE),
            (Op::CreateFile, crate::stdlib::state::FILE),
            (Op::CreateFile, PROXY),
            (Op::RegistryOverride, reg.as_str()),
        ]
    );
    for e in &j.entries {
        assert_eq!(e.state, crate::game_folder::journal::State::Pending, "{}", e.path);
        match e.op {
            Op::ReplaceFile => {
                let stock = if e.path == UI { "ui-v1" } else { "db-v1" };
                assert_eq!(e.before.as_deref(), Some(sha256_bytes(stock.as_bytes()).as_str()));
                assert_eq!(
                    e.after.as_deref(),
                    Some(sha256_bytes(std::fs::read(&plan.data[&e.path]).unwrap().as_slice()).as_str())
                );
            }
            Op::CreateDir => assert_eq!((e.before.as_deref(), e.after.as_deref()), (None, None)),
            Op::CreateFile => assert!(e.before.is_none() && e.after.is_some(), "{}", e.path),
            Op::RegistryOverride => {
                assert_eq!(e.before.as_deref(), Some(sha256_bytes(REG.as_bytes()).as_str()));
                let added = add_override(REG).unwrap();
                assert_eq!(e.after.as_deref(), Some(sha256_bytes(added.as_bytes()).as_str()));
            }
        }
    }
    let placed = Placed::load(&f.data).unwrap();
    assert!(placed.get(UI).unwrap().replaced && placed.get(GVDB).unwrap().replaced);
    for rel in [MAP, MAP_AI, CORE, PROXY, crate::stdlib::state::FILE] {
        assert!(!placed.get(rel).unwrap().replaced, "{rel}");
    }
    assert_eq!(placed.registry.as_deref(), Some(reg.as_str()));
    assert!(f.data.join("state/core-state.json").is_file());
    f.rolled_back(&game, &original);
}

#[test]
fn a_new_file_that_cannot_be_copied_stops_the_install_and_the_rollback_clears_the_rest() {
    let f = Fixture::new("create-fails");
    let game = f.game();
    let original = f.snapshot();
    let mut plan = f.plan();
    plan.data.insert(GVDB.into(), f.built(GVDB, "db-modded"));
    plan.added.insert(FIRST.into(), f.built(FIRST, "first"));
    plan.added.insert(SECOND.into(), f.built(SECOND, "second"));
    let second = plan.added[SECOND].clone();
    let progress = |done: usize, _: usize| {
        if done == 2 {
            std::fs::remove_file(&second).unwrap();
        }
    };
    let inst = Installer {
        progress: &progress,
        copy_only: true,
        ..installer(&f, &game, &quiet)
    };
    let err = inst.apply(&plan).unwrap_err();
    assert!(
        matches!(&err, Error::Io { path, .. } if *path == f.game.join(SECOND)),
        "{err}"
    );
    assert_eq!(f.read(GVDB).as_deref(), Some("db-modded"));
    assert_eq!(f.read(FIRST).as_deref(), Some("first"));
    assert_eq!(f.read(SECOND), None);
    let open = Journal::open(&f.data).unwrap();
    let states: Vec<(&str, crate::game_folder::journal::State)> = open[0]
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.state))
        .collect();
    use crate::game_folder::journal::State::Pending;
    assert_eq!(states, [(GVDB, Pending), (FIRST, Pending), (SECOND, Pending)]);
    f.rolled_back(&game, &original);
}

#[test]
fn every_file_linked_straight_from_the_build_says_hard_links() {
    let f = Fixture::new("hard-links");
    let game = f.game();
    let mut plan = f.plan();
    plan.data.insert(GVDB.into(), f.built(GVDB, "db-modded"));
    plan.added.insert(FIRST.into(), f.built(FIRST, "first"));
    let lines = RefCell::new(Vec::new());
    let out = |l: &str| lines.borrow_mut().push(l.to_owned());
    installer(&f, &game, &out).apply(&plan).unwrap();
    assert_eq!(
        mode_line(&lines.borrow()),
        "installing: hard links; 2 file(s) linked, 0 copied"
    );
    assert_eq!(f.read(FIRST).as_deref(), Some("first"));
}

#[test]
fn after_a_link_fails_the_rest_are_copied_and_the_mode_says_why() {
    for cached in [false, true] {
        let f = Fixture::new(if cached { "cache-link-fails" } else { "link-fails" });
        let game = f.game();
        let mut plan = f.plan();
        for (rel, body) in [(FIRST, "first"), (SECOND, "second")] {
            let src = if cached {
                f.loose(rel, body)
            } else {
                f.built(rel, body)
            };
            plan.added.insert(rel.into(), src);
        }
        let in_the_way = f.game.join(SECOND);
        let progress = |done: usize, _: usize| {
            if done == 1 {
                put(&in_the_way, "in the way");
            }
        };
        let lines = RefCell::new(Vec::new());
        let out = |l: &str| lines.borrow_mut().push(l.to_owned());
        let inst = Installer {
            progress: &progress,
            ..installer(&f, &game, &out)
        };
        inst.apply(&plan).unwrap();
        assert_eq!(f.read(FIRST).as_deref(), Some("first"));
        assert_eq!(f.read(SECOND).as_deref(), Some("second"));
        let lines = lines.borrow();
        let line = mode_line(&lines);
        let why = if cached {
            "link cache link failed"
        } else {
            "hard link failed"
        };
        assert!(line.starts_with(&format!("installing: copy ({why}: ")), "{line}");
        assert!(line.ends_with("); 1 file(s) linked, 1 copied"), "{line}");
    }
}

#[test]
fn loose_files_without_room_for_the_cache_are_copied_and_the_mode_says_why() {
    let f = Fixture::new("no-cache");
    let game = f.game();
    let mut plan = f.plan();
    plan.added.insert(FIRST.into(), f.loose(FIRST, "first"));
    let lines = RefCell::new(Vec::new());
    let out = |l: &str| lines.borrow_mut().push(l.to_owned());
    let inst = Installer {
        free_space: |_| Some(1 << 20),
        ..installer(&f, &game, &out)
    };
    inst.apply(&plan).unwrap();
    assert_eq!(f.read(FIRST).as_deref(), Some("first"));
    let lines = lines.borrow();
    let line = mode_line(&lines);
    assert!(
        line.starts_with("installing: copy (link cache unavailable: "),
        "{line}"
    );
    assert!(line.ends_with("); 0 file(s) linked, 1 copied"), "{line}");
}

#[test]
fn an_override_alone_has_no_build_output_to_link() {
    let f = Fixture::new("override-only");
    let game = f.game();
    let mut plan = f.plan();
    plan.registry_override = true;
    let lines = RefCell::new(Vec::new());
    let out = |l: &str| lines.borrow_mut().push(l.to_owned());
    installer(&f, &game, &out).apply(&plan).unwrap();
    assert_eq!(
        mode_line(&lines.borrow()),
        "installing: copy (no build output to link); 0 file(s) linked, 0 copied"
    );
    assert!(has_override(&std::fs::read_to_string(&f.reg).unwrap()));
}

#[test]
fn a_refused_plan_opens_no_journal_and_changes_nothing() {
    for case in ["drop-in", "missing", "leftover"] {
        let f = Fixture::new(case);
        let game = f.game();
        let mut plan = f.plan();
        match case {
            "drop-in" => {
                let rel = crate::game_folder::stock::UPSCALER_DROP_INS[0];
                plan.added.insert(rel.into(), f.built(rel, "dll"));
            }
            "missing" => {
                std::fs::remove_file(f.game.join(UI)).unwrap();
                plan.data.insert(UI.into(), f.built(UI, "ui-modded"));
            }
            _ => {
                put(&f.game.join(FIRST), "the player's");
                plan.added.insert(FIRST.into(), f.built(FIRST, "first"));
            }
        }
        plan.registry_override = true;
        let original = f.snapshot();
        let err = installer(&f, &game, &quiet).apply(&plan).unwrap_err();
        let refused = match case {
            "drop-in" => matches!(err, Error::Resolve(_)),
            "missing" => matches!(&err, Error::MissingStock { files, .. } if files == &[UI.to_owned()]),
            _ => matches!(&err, Error::Leftovers(c) if c.len() == 1 && !c[0].owned && !c[0].replaced),
        };
        assert!(refused, "{case}: {err}");
        assert_eq!(f.snapshot(), original, "{case}");
        assert!(Journal::open(&f.data).unwrap().is_empty(), "{case}");
        assert!(Placed::load(&f.data).unwrap().get(FIRST).is_none(), "{case}");
    }
}

#[test]
fn the_copy_report_names_how_the_files_were_placed() {
    let c = |linked, placed, link_failure: Option<&str>, cached, cache_failure: Option<&str>| Copied {
        linked,
        from_cache: linked.min(1),
        placed,
        link_failure: link_failure.map(str::to_owned),
        cached,
        cache_failure: cache_failure.map(str::to_owned),
    };
    for (copied, copy_only, line) in [
        (
            c(0, 3, None, None, None),
            true,
            "installing: copy (SWOFTY_INSTALL_COPY=1); 0 file(s) linked, 3 copied",
        ),
        (
            c(1, 3, Some("denied"), Some((1, 0)), None),
            false,
            "installing: copy (link cache link failed: denied); 1 file(s) linked, 2 copied",
        ),
        (
            c(2, 3, None, Some((1, 4)), None),
            false,
            "installing: 2 linked (1 from the link cache: 1 copied into it, 4 reused), 1 copied",
        ),
        (
            c(0, 3, None, None, Some("no room")),
            false,
            "installing: copy (link cache unavailable: no room); 0 file(s) linked, 3 copied",
        ),
        (
            c(2, 3, Some("denied"), None, Some("no room")),
            false,
            "installing: copy (hard link failed: denied); 2 file(s) linked, 1 copied",
        ),
        (
            c(0, 0, None, None, None),
            false,
            "installing: copy (no build output to link); 0 file(s) linked, 0 copied",
        ),
        (
            c(3, 3, None, None, Some("no room")),
            false,
            "installing: hard links; 3 file(s) linked, 0 copied",
        ),
    ] {
        assert_eq!(copy_report(&copied, copy_only), line, "{copied:?}");
    }
}

#[test]
fn the_linker_links_build_output_and_cached_copies_until_a_link_fails() {
    let f = Fixture::new("linker");
    let (outside, cached) = (f.loose("outside", "x"), f.loose("cached", "c"));
    let mut linker = Linker {
        via: BTreeMap::from([("in/cache.erp".to_owned(), cached)]),
        built: f.built.clone(),
        on: true,
        from_cache: 0,
        failure: None,
    };
    std::fs::create_dir_all(&f.game).unwrap();
    let at = |name: &str| f.game.join(name);
    assert!(!linker.link("outside.erp", &outside, &at("outside.erp"), false));
    assert!(!at("outside.erp").exists() && linker.on);
    assert!(linker.link("built.erp", &f.built("b", "b"), &at("built.erp"), false));
    assert!(linker.link("in/cache.erp", &outside, &at("cache.erp"), false));
    assert_eq!(std::fs::read_to_string(at("cache.erp")).unwrap(), "c");
    assert_eq!(linker.from_cache, 1);
    assert!(!linker.link("again.erp", &f.built("b", "b"), &at("built.erp"), false));
    assert!(!linker.on && linker.failure.is_some());
    assert!(!linker.link("later.erp", &f.built("l", "l"), &at("later.erp"), false));
    assert!(!at("later.erp").exists());
}

#[test]
fn any_upscaler_drop_in_in_a_plan_refuses_it() {
    let f = Fixture::new("drop-ins");
    let rel = crate::game_folder::stock::UPSCALER_DROP_INS[0];
    for at in 0..3 {
        let mut plan = f.plan();
        plan.data.insert(GVDB.into(), f.built(GVDB, "db"));
        let into = [&mut plan.data, &mut plan.added, &mut plan.loader];
        into.into_iter()
            .nth(at)
            .unwrap()
            .insert(rel.to_ascii_uppercase(), f.built(rel, "dll"));
        let err = refuse_drop_ins(&plan).unwrap_err();
        assert!(err.to_string().contains("upscaler drop-in"), "{err}");
    }
    let mut plan = f.plan();
    plan.added.insert(format!("bin/{rel}"), f.built(rel, "dll"));
    refuse_drop_ins(&plan).unwrap();
}
