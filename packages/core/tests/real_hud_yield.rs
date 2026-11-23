//! A yield record built beside SWOFTY 2011's season HUD, and beside Project Halo, through the whole
//! profile build: the yield's null holds the stock part, 2011's records and Halo's whole-file dial and
//! tower in either load order (2011 and Halo both replace the tower, so they are built apart). Needs `F1_25_DIR` (read through the launcher's baseline); 2011 and Halo come from the
//! launcher's addon store (`SWOFTY_2011_DIR`, `PROJECT_HALO_DIR` override), and each is left out when
//! it is not there.

use std::path::{Path, PathBuf};

use swofty_core::build::{GVDB, build};
use swofty_core::game_folder::baseline::Identity;
use swofty_core::hud::{GameStock, dials, stock_edit, yields};
use swofty_core::install::Game;
use swofty_core::store::Store;
use swofty_core::ui::idf;
use swofty_core::ui::node::{NodeExt, find_path, node_at};
use swofty_formats::erp::ErpFile;
use swofty_formats::gvdb::Gvdb;
use swofty_formats::pkg::Pkg;

const UI: &str = "2025_asset_groups/ui_package";
const ID: &str = "test.yielder";
const LEAF: &str = "view_model_data.hud_yt_on";
const PARTS: [&str; 6] = [
    "f1_25",
    "f1_26",
    "standings",
    "lap_counter",
    "flag_box",
    "notifications",
];

fn write(path: &Path, body: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn copy_dir(from: &Path, to: &Path) {
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t);
        } else {
            write(&t, std::fs::read(&p).unwrap());
        }
    }
}

struct Fixture {
    root: PathBuf,
    store: Store,
    game: Game,
    _pin: swofty_core::addons::builtin::LoaderPin,
}

impl Fixture {
    fn new(name: &str, real: &Path) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-hud-yield-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let dir = root.join("game");
        write(&dir.join("F1_25.exe"), "exe");
        std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
        std::fs::write(dir.join(GVDB), Gvdb::new(Vec::new()).to_bytes()).unwrap();
        let originals = GameStock::with_baseline(real, &Store::open_default().root).unwrap();
        for sub in ["", "/screens"] {
            for e in std::fs::read_dir(real.join(format!("{UI}{sub}")))
                .unwrap()
                .flatten()
            {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.ends_with(".erp") {
                    continue;
                }
                let rel = format!("{UI}{sub}/{name}");
                let to = dir.join(&rel);
                std::fs::create_dir_all(to.parent().unwrap()).unwrap();
                #[cfg(unix)]
                std::os::unix::fs::symlink(originals.original(&rel), &to).unwrap();
                #[cfg(not(unix))]
                std::fs::copy(originals.original(&rel), &to).unwrap();
            }
        }
        let loader = root.join("loader");
        write(&loader.join(swofty_core::install::PROXY), "proxy");
        write(&loader.join("swofty/addons/core/swofty_core.dll"), "core");
        let game = Game {
            identity: Identity::detect(&dir, Some(1)).unwrap(),
            dir,
            user_reg: None,
        };
        Self {
            store: Store::new(root.join("data")),
            root,
            game,
            _pin: swofty_core::addons::builtin::LoaderPin::to(loader),
        }
    }

    fn install(&self, src: &Path) {
        let m: serde_json::Value =
            serde_json::from_slice(&std::fs::read(src.join("swofty.json")).unwrap()).unwrap();
        let id = m["id"].as_str().unwrap().to_owned();
        self.store.install_addon(src).unwrap();
        self.store
            .update_profile("default", |p| p.set_enabled(&id, true))
            .unwrap();
    }

    fn yielder(&self) -> PathBuf {
        let src = self.root.join("src/yielder");
        write(
            &src.join("swofty.json"),
            serde_json::json!({
                "schema_version": 1, "id": ID, "name": "Yielder", "version": "0.1.0", "author": "test", "kind": ["ui"],
                "compat": { "game": "f1_25" }, "operations": { "hud": ["hud/yield.json"] }
            })
            .to_string(),
        );
        write(
            &src.join("hud/yield.json"),
            serde_json::json!({ "hud.element": { "all": {
                "yield": { "parts": PARTS, "while": { "view_model": LEAF } } } } })
            .to_string(),
        );
        src
    }

    fn season(&self, from: &Path) -> PathBuf {
        let src = self.root.join("src/s11");
        copy_dir(&from.join("art"), &src.join("art"));
        let mut hud: serde_json::Value =
            serde_json::from_slice(&std::fs::read(from.join("hud/season.json")).unwrap()).unwrap();
        hud.as_object_mut().unwrap().remove("when");
        write(&src.join("hud/season.json"), hud.to_string());
        write(
            &src.join("swofty.json"),
            serde_json::json!({
                "schema_version": 1, "id": "swofty.swofty-seasons-2011", "name": "SWOFTY 2011", "version": "1.0.6", "author": "Swofty",
                "kind": ["ui"], "compat": { "game": "f1_25" }, "operations": { "hud": ["hud/season.json"] }
            })
            .to_string(),
        );
        src
    }

    fn halo(&self, from: &Path) -> PathBuf {
        let src = self.root.join("src/halo");
        copy_dir(from, &src);
        let manifest = src.join("swofty.json");
        let mut m: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
        m.as_object_mut().unwrap().remove("requires");
        m["kind"] = serde_json::json!(["erp"]);
        m["operations"].as_object_mut().unwrap().remove("native");
        let _ = std::fs::remove_dir_all(src.join("native"));
        std::fs::write(&manifest, m.to_string()).unwrap();
        src
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn store_addon(env: &str, id: &str, marker: &str) -> Option<PathBuf> {
    let dir = std::env::var_os(env)
        .map(PathBuf::from)
        .unwrap_or_else(|| Store::open_default().addons_dir().join(id));
    match dir.join(marker).is_file() {
        true => Some(dir),
        false => {
            eprintln!("{id} is not installed; building without it");
            None
        }
    }
}

fn resource_of(archive: &Path, name: &str) -> Option<Vec<u8>> {
    let file = ErpFile::open(archive).ok()?;
    let res = file.erp.find(name).ok()?;
    file.load(res).ok()?.fragments[0].data().ok()
}

fn paths(bytes: &[u8]) -> Vec<String> {
    let p = Pkg::parse(bytes).unwrap();
    assert!(idf::id_problems(&p).unwrap().is_empty());
    idf::elements(stock_edit::scene_root(&p).unwrap())
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

/// The children of `inside` in a scene, by name.
fn kids_of(all: &[String], inside: &str) -> Vec<String> {
    let base = match inside {
        "Scene Root" => String::new(),
        _ => {
            let at = all
                .iter()
                .find(|p| p.rsplit('.').next() == Some(inside))
                .unwrap_or_else(|| panic!("no {inside}"));
            format!("{at}.")
        }
    };
    all.iter()
        .filter(|p| p.as_str() != "Scene Root")
        .filter_map(|p| p.strip_prefix(&base))
        .filter(|rest| !rest.contains('.'))
        .map(str::to_owned)
        .collect()
}

fn built_with(order: &[&str]) -> Option<()> {
    let game = PathBuf::from(std::env::var_os("F1_25_DIR")?);
    let s11 = store_addon("SWOFTY_2011_DIR", "swofty.swofty-seasons-2011", "hud/season.json");
    let halo = store_addon("PROJECT_HALO_DIR", "nick.f1-2026-halo", "erp/halo_scene.erp");
    let f = Fixture::new(&order.join("-"), &game);
    for who in order {
        match *who {
            "yield" => f.install(&f.yielder()),
            "s11" => {
                if let Some(d) = &s11 {
                    f.install(&f.season(d));
                }
            }
            "halo" => {
                if let Some(d) = &halo {
                    f.install(&f.halo(d));
                }
            }
            _ => unreachable!(),
        }
    }
    let built = build(&f.store, "default", &f.game).unwrap();
    let pfx = swofty_core::hud::prefix_for(ID, "all", None).unwrap();
    let null = yields::null_name(&pfx);
    assert!(
        built.lines.iter().any(|l| l.contains(&format!("{pfx}: yields "))),
        "{:#?}",
        built.lines
    );
    let race = &built.plan.data[&format!("{UI}/flow_race.erp")];
    for part in PARTS {
        for spot in yields::part(part).unwrap().spots {
            let scene =
                resource_of(race, spot.scene).unwrap_or_else(|| panic!("{} is not built", spot.scene));
            let all = paths(&scene);
            assert_eq!(
                kids_of(&all, spot.inside),
                std::slice::from_ref(&null),
                "{part}: {}",
                spot.scene
            );
            let held = all.iter().filter(|p| p.contains(&format!("{null}."))).count();
            assert!(held > 0, "{part}: {null} holds nothing");
            for p in all.iter().filter(|p| p.contains("s11_") || p.contains("halo")) {
                let inside_null = p.split('.').any(|s| s == null);
                let around = all
                    .iter()
                    .any(|q| q.starts_with(&format!("{p}.")) && q.ends_with(&format!(".{null}")));
                assert!(inside_null || around, "{part}: {p} is outside {null}");
            }
        }
    }
    let dial = paths(&resource_of(race, dials::F125.scene).unwrap());
    if s11.is_some() && order.contains(&"s11") {
        assert!(
            dial.iter().any(|p| p.contains(&format!("{null}.s11_onboard"))),
            "{dial:#?}"
        );
        let lapc = paths(&resource_of(race, swofty_core::hud::screens::LAPC_SCENE).unwrap());
        assert!(
            lapc.iter().any(|p| p.contains(&format!("{null}.s11_tower_w_"))),
            "{lapc:#?}"
        );
    }
    if halo.is_some() && order.contains(&"halo") {
        let held = kids_of(&dial, &null);
        assert!(
            held.iter().any(|k| !dials::STOCK_KIDS.contains(&k.as_str())),
            "Halo's dial is not inside {null}: {held:?}"
        );
        let board = paths(&resource_of(race, swofty_core::hud::screens::STACKED).unwrap());
        assert!(board.contains(&format!("{null}.tt_host")), "{board:#?}");
    }
    let t = Pkg::parse(&resource_of(race, dials::TEMPLATE).unwrap()).unwrap();
    for part in PARTS {
        for hook in yields::part(part).unwrap().spots[0]
            .hooks
            .iter()
            .filter(|h| h.template == dials::TEMPLATE)
        {
            let path = format!("{}.{}.v_yield", hook.parent, yields::group_name(&pfx, part));
            let at = find_path(&t.root, &path).unwrap_or_else(|_| panic!("{path}"));
            let vis = node_at(&t.root, &at)
                .unwrap()
                .props()
                .unwrap()
                .get("visible")
                .unwrap()
                .clone();
            let o = vis.as_obj().unwrap();
            assert_eq!(
                o.get("dbentry").unwrap().as_val().unwrap().as_str(),
                Some(&*format!("#{LEAF}"))
            );
        }
    }
    assert!(
        !built.warnings.iter().any(|w| w.contains("cannot be applied")),
        "{:#?}",
        built.warnings
    );
    Some(())
}

#[test]
fn a_yield_holds_the_season_hud_with_the_yield_installed_first() {
    built_with(&["yield", "s11"]);
}

#[test]
fn a_yield_holds_the_season_hud_with_the_yield_installed_last() {
    built_with(&["s11", "yield"]);
}

#[test]
fn a_yield_holds_halos_whole_files_with_the_yield_installed_first() {
    built_with(&["yield", "halo"]);
}

#[test]
fn a_yield_holds_halos_whole_files_with_the_yield_installed_last() {
    built_with(&["halo", "yield"]);
}
