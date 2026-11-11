//! The HUD composer inside a profile build: two addons' `operations.hud` records, the ADDONS MFD page and
//! another addon's whole-resource replacement of the template must all end up in one template. Needs
//! `F1_25_DIR`; the stock files are read through the launcher's baseline, never the Halo-modified folder.

use std::path::{Path, PathBuf};

use swofty_addon::asset::{AssetOp, Root};
use swofty_addon::ops::ErpOp;
use swofty_core::baseline::Identity;
use swofty_core::build::{GVDB, build};
use swofty_core::hud::{self, GameStock, dials};
use swofty_core::install::Game;
use swofty_core::store::Store;
use swofty_core::ui::node::{NodeExt, ObjExt, clone_node, edit, find_path, root_obj};
use swofty_formats::erp::{Erp, ErpFile};
use swofty_formats::gvdb::Gvdb;
use swofty_formats::pkg::Pkg;

const FLOW_RACE: &str = "2025_asset_groups/ui_package/flow_race.erp";
const MARKER: &str = "planked_brake_panel";

fn write(path: &Path, body: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn png(path: &Path, w: u32, h: u32) {
    write(path, png_bytes(w, h));
}

fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = [255u8; 4]
            .iter()
            .copied()
            .cycle()
            .take((w * h * 4) as usize)
            .collect();
        enc.write_header().unwrap().write_image_data(&data).unwrap();
    }
    out
}

fn record() -> serde_json::Value {
    serde_json::json!({
        "dial": "f1_26",
        "sprites": {
            "base": { "png": "art/base.png", "bounds": [-60, -20, 60, 20] },
            "lit": { "png": "art/lit.png", "bounds": [-50, -10, -34, -2] }
        },
        "elements": [
            { "kind": "sprite", "id": "base", "sprite": "base", "layer": 20 },
            { "kind": "text", "id": "speed", "style": "OSD_24_bold", "align": "centre", "box": 40,
              "cap_height": 9.5, "at": [0, -6], "value": { "game": "speed_str" } },
            { "kind": "sprite", "id": "boost", "sprite": "lit",
              "visible": { "view_model": "view_model_data.hud_t_boost" } }
        ]
    })
}

struct Fixture {
    root: PathBuf,
    store: Store,
    game: Game,
    _pin: swofty_core::builtin::LoaderPin,
}

impl Fixture {
    fn new(name: &str, real: &Path) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-hud-build-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let dir = root.join("game");
        write(&dir.join("F1_25.exe"), "exe");
        std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
        std::fs::write(dir.join(GVDB), Gvdb { records: Vec::new() }.to_bytes()).unwrap();
        let originals = GameStock::with_baseline(real, &Store::open_default().root).unwrap();
        let ui = |rel: &str| format!("2025_asset_groups/ui_package/{rel}");
        let mut rels: Vec<String> = swofty_core::ui::ERPS
            .iter()
            .chain(swofty_core::ui::READS)
            .chain(["flow_race.erp"].iter())
            .map(|r| ui(r))
            .collect();
        rels.extend(dials::SCENE_ERPS.iter().map(|s| (*s).to_owned()));
        rels.sort();
        rels.dedup();
        for rel in rels {
            let to = dir.join(&rel);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(originals.original(&rel), &to).unwrap();
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
            _pin: swofty_core::builtin::LoaderPin::to(loader),
        }
    }

    fn install(&self, id: &str, extra: serde_json::Value, files: Vec<(&str, Vec<u8>)>) {
        let src = self.root.join("src").join(id);
        let mut manifest = serde_json::json!({
            "schema_version": 1, "id": id, "name": id, "version": "1.0.0", "author": "t",
            "compat": { "game": "f1_25" }
        });
        for (k, v) in extra.as_object().unwrap() {
            manifest[k] = v.clone();
        }
        write(&src.join("swofty.json"), manifest.to_string());
        for (rel, data) in files {
            write(&src.join(rel), data);
        }
        self.store.install_addon(&src).unwrap();
        self.store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    }

    fn hud_addon(&self, id: &str) {
        let hud = serde_json::json!({ "hud.element": { "hud": record() } });
        self.install(
            id,
            serde_json::json!({ "operations": { "hud": ["hud.json"] } }),
            vec![
                ("hud.json", hud.to_string().into_bytes()),
                ("art/base.png", png_bytes(64, 28)),
                ("art/lit.png", png_bytes(16, 8)),
            ],
        );
    }

    fn mfd_addon(&self) {
        self.install(
            "c.mfd",
            serde_json::json!({
                "options": [{ "id": "livery", "type": "toggle", "label": "Livery", "default": false, "binds": "native" }],
                "hud": { "mfd": { "title": "LIVERY", "rows": ["livery"] } }
            }),
            vec![],
        );
    }

    fn planked(&self, real: &Path) {
        let originals = GameStock::with_baseline(real, &Store::open_default().root).unwrap();
        let stock = ErpFile::open(&originals.original(FLOW_RACE)).unwrap();
        let res = stock.erp.find(dials::TEMPLATE).unwrap();
        let bytes = stock.load(res).unwrap().fragments[0].data().unwrap();
        let replaced = edit(&bytes, |p| {
            let list = root_obj(p)?.at_mut("Templates")?.a_mut()?;
            let first = list.iter().find(|t| t.is_node()).unwrap().clone();
            list.push(clone_node(&first, "", MARKER)?);
            Ok(())
        })
        .unwrap();
        let ops = serde_json::json!({ "schema_version": 1, "ops": [
            { "op": "erp.replace-resource", "erp": FLOW_RACE, "resource": dials::TEMPLATE, "source": "template.ui" }
        ] });
        self.install(
            "d.planked",
            serde_json::json!({ "operations": { "assets": ["ops.json"] } }),
            vec![
                ("ops.json", ops.to_string().into_bytes()),
                ("template.ui", replaced),
            ],
        );
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn real() -> Option<PathBuf> {
    match std::env::var_os("F1_25_DIR") {
        Some(d) => Some(PathBuf::from(d)),
        None => {
            eprintln!("F1_25_DIR is not set; skipping");
            None
        }
    }
}

fn template_of(archive: &Path) -> Pkg {
    let file = ErpFile::open(archive).unwrap();
    let res = file.erp.find(dials::TEMPLATE).unwrap();
    Pkg::parse(&file.load(res).unwrap().fragments[0].data().unwrap()).unwrap()
}

fn has(t: &Pkg, path: &str) -> bool {
    find_path(&t.root, path).is_ok()
}

const PANEL: &str = "hud_mfd_dynamic.mfd.attacher.swofty_addons_panel";

#[test]
fn two_hud_addons_the_mfd_page_and_a_replaced_template_compose_into_one() {
    let Some(game) = real() else { return };
    let f = Fixture::new("all", &game);
    f.hud_addon("one.hud");
    f.hud_addon("two.hud");
    f.mfd_addon();
    f.planked(&game);
    let built = build(&f.store, "default", &f.game).unwrap();
    let parent = dials::F126.race_parent;

    let cached = f.store.addon_cache_dir("one.hud").join("hud/hud_template.erp");
    for (what, t) in [
        ("the composed overlay archive", template_of(&cached)),
        (
            "the built flow_race.erp",
            template_of(&built.plan.data[FLOW_RACE]),
        ),
    ] {
        assert!(has(&t, &format!("{parent}.one_hud_ctl")), "{what}: one.hud");
        assert!(has(&t, &format!("{parent}.two_hud_ctl")), "{what}: two.hud");
        assert!(has(&t, PANEL), "{what}: the MFD page");
        assert!(
            t.root.as_obj().is_some() && find_path(&t.root, MARKER).is_ok(),
            "{what}: the replaced template's own node"
        );
    }
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.contains("d.planked") && l.contains("on top of that change")),
        "{:#?}",
        built.lines
    );

    let scene =
        Erp::parse(&std::fs::read(f.store.addon_cache_dir("one.hud").join("hud/hud_scene.erp")).unwrap())
            .unwrap();
    assert!(scene.resources.iter().any(|r| r.name.contains("one_hud_base")));
    assert!(scene.resources.iter().any(|r| r.name.contains("two_hud_base")));
}

#[test]
fn the_overlay_ops_point_at_the_composed_archives() {
    let Some(game) = real() else { return };
    let stock = GameStock::with_baseline(&game, &Store::open_default().root).unwrap();
    let dir = std::env::temp_dir().join(format!("swofty-hud-ops-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    png(&dir.join("art/base.png"), 64, 28);
    png(&dir.join("art/lit.png"), 16, 8);
    let input = hud::Input {
        addon: "one.hud".into(),
        key: "hud".into(),
        dir: dir.clone(),
        record: hud::model::HudElement::parse(&record()).unwrap(),
    };
    let out = hud::compose(&stock, &[input]).unwrap();
    let ops = out.ops("hud");
    assert_eq!(ops.len(), out.archives.len());
    for (op, archive) in ops.iter().zip(&out.archives) {
        let ErpOp::Overlay { erps, source } = op else {
            panic!("{op:?}")
        };
        assert_eq!(source.root, Root::Cache);
        assert_eq!(source.path, format!("hud/{}", archive.file));
        assert_eq!(erps, &archive.erps);
    }
    let _ = AssetOp::Erp(ops[0].clone());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn a_compose_error_stops_the_build_with_the_reason() {
    let Some(game) = real() else { return };
    let f = Fixture::new("error", &game);
    let mut bad = record();
    bad["elements"][1]["value"] = serde_json::json!({ "game": "no_such_value" });
    let hud = serde_json::json!({ "hud.element": { "hud": bad } });
    f.install(
        "bad.hud",
        serde_json::json!({ "operations": { "hud": ["hud.json"] } }),
        vec![
            ("hud.json", hud.to_string().into_bytes()),
            ("art/base.png", png_bytes(64, 28)),
            ("art/lit.png", png_bytes(16, 8)),
        ],
    );
    let err = build(&f.store, "default", &f.game).unwrap_err().to_string();
    assert!(err.contains("the HUD elements do not compose"), "{err}");
    assert!(err.contains("no_such_value"), "{err}");
}
