use super::*;

const TILES_ERP: &str = "ui/tiles.erp";

fn tiles_archive(names: &[&str]) -> Vec<u8> {
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    use swofty_formats::pkg::{Arr, Node, Obj, Pkg, Val};
    let obj = |pairs: Vec<(&str, Node)>| {
        Node::Obj(Obj::new(
            pairs.into_iter().map(|(k, v)| (Val::str(k), v)).collect(),
        ))
    };
    let tiles = names
        .iter()
        .map(|n| obj(vec![("Name", Node::Val(Val::str(*n)))]))
        .collect();
    let root = obj(vec![(
        "overrides",
        obj(vec![(
            "Contents",
            obj(vec![(
                "home",
                obj(vec![("Contents", Node::Arr(Arr::new(tiles)))]),
            )]),
        )]),
    )]);
    let raw = Pkg {
        name: *b"UIDB",
        root,
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap();
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    let mut res = Resource {
        name: "eaid://ui_package/databases/overrides.uidb".into(),
        kind: swofty_formats::erp::kind("UIDB").unwrap(),
        unk: vec![0; 4],
        fragments: vec![Fragment::new(*b"temp", raw.len() as u64, 0, STORED, raw)],
        hash: [0; 16],
    };
    res.hash = res.content_hash().unwrap();
    Erp {
        header,
        version: 3,
        resources: vec![res],
    }
    .to_bytes()
    .unwrap()
}

fn tile_names(path: &Path) -> Vec<String> {
    let erp = swofty_formats::erp::Erp::parse(&std::fs::read(path).unwrap()).unwrap();
    let res = erp.find("eaid://ui_package/databases/overrides.uidb").unwrap();
    let pkg = swofty_formats::pkg::Pkg::parse(&res.fragments[0].data().unwrap()).unwrap();
    crate::content::pkg_edit::value_text(&pkg.root, "overrides.Contents.home.Contents[0].Name").unwrap();
    let mut names = Vec::new();
    for i in 0.. {
        match crate::content::pkg_edit::value_text(
            &pkg.root,
            &format!("overrides.Contents.home.Contents[{i}].Name"),
        ) {
            Ok(n) => names.push(n),
            Err(_) => break,
        }
    }
    names
}

struct Seasons {
    root: PathBuf,
    store: Store,
    game: Game,
}

impl Seasons {
    fn new(name: &str, in_game: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-stale-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let dir = root.join("game");
        std::fs::create_dir_all(dir.join("game_package/gamedatabases")).unwrap();
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::write(dir.join(crate::GAME_EXE), "exe v2").unwrap();
        std::fs::write(
            dir.join(GVDB),
            Gvdb {
                records: Vec::new().into(),
            }
            .to_bytes(),
        )
        .unwrap();
        std::fs::write(dir.join(TILES_ERP), tiles_archive(in_game)).unwrap();
        let game = Game {
            identity: Identity::detect(&dir, Some(2)).unwrap(),
            dir,
            user_reg: None,
        };
        let store = Store::new(root.join("data"));
        Self { root, store, game }
    }

    fn add(&self, id: &str, name: &str, tile: &str) {
        let src = self.root.join("src").join(id);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("swofty.json"),
            format!(
                r#"{{ "schema_version": 1, "id": "{id}", "name": "{name}", "version": "1.0.0", "author": "t",
                     "compat": {{ "game": "f1_25" }}, "operations": {{ "erp": ["erp.json"] }} }}"#
            ),
        )
        .unwrap();
        std::fs::write(
            src.join("erp.json"),
            format!(
                r#"{{ "schema_version": 1, "ops": [{{ "op": "patch-pkg", "erp": "{TILES_ERP}",
                     "resource": "eaid://ui_package/databases/overrides.uidb", "edits": [
                     {{ "op": "clone-entry", "at": "overrides.Contents.home.Contents", "src": "[Name=stock]",
                        "as": "{tile}" }}] }}] }}"#
            ),
        )
        .unwrap();
        self.store.install_addon(&src).unwrap();
        self.store
            .update_profile("default", |p| p.set_enabled(id, true))
            .unwrap();
    }

    fn back_up_for_build_1(&self, names: &[&str]) {
        let clean = self.root.join("clean.erp");
        std::fs::write(&clean, tiles_archive(names)).unwrap();
        let objects = Objects {
            root: self.store.root.join("baseline/objects"),
        };
        let sha256 = objects.put_file(&clean).unwrap();
        let old = Identity {
            build_id: "1".into(),
            exe_sha256: "a".repeat(64),
        };
        let mut b = Baseline::new(&old);
        b.files.insert(
            TILES_ERP.into(),
            crate::game_folder::baseline::FileRecord {
                size: std::fs::metadata(&clean).unwrap().len(),
                sha256,
                source: crate::game_folder::baseline::Source::FirstSight,
            },
        );
        b.save(&self.store.root).unwrap();
    }
}

impl Drop for Seasons {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[test]
fn a_game_file_holding_an_earlier_sessions_entry_gets_its_original_back_and_the_build_succeeds() {
    if loader_dir().is_some() {
        return;
    }
    let s = Seasons::new("healed", &["stock", "sw_f1_2016"]);
    s.back_up_for_build_1(&["stock"]);
    s.add("t.season-2016", "Season 2016", "sw_f1_2016");
    let built = build(&s.store, "default", &s.game).unwrap();
    assert!(
        built.lines.iter().any(|l| l.starts_with(&format!(
            "stale: {TILES_ERP} in the game folder still held the launcher's changes from an earlier session"
        ))),
        "{:?}",
        built.lines
    );
    assert_eq!(tile_names(&s.game.dir.join(TILES_ERP)), ["stock"]);
    let out = built
        .plan
        .data
        .get(TILES_ERP)
        .expect("the tiles archive is built");
    assert_eq!(tile_names(out), ["stock", "sw_f1_2016"]);
    let again = build(&s.store, "default", &s.game).unwrap();
    assert!(!again.lines.iter().any(|l| l.starts_with("stale: ")));
}

#[test]
fn a_stale_game_file_with_no_clean_copy_on_record_says_to_repair_the_game() {
    if loader_dir().is_some() {
        return;
    }
    let s = Seasons::new("unhealed", &["stock", "sw_f1_2016"]);
    s.add("t.season-2016", "Season 2016", "sw_f1_2016");
    let text = build(&s.store, "default", &s.game).unwrap_err().to_string();
    assert!(
        text.starts_with("applying t.season-2016: eaid://ui_package/databases/overrides.uidb: ")
            && text.contains("an entry with Name=sw_f1_2016 already exists")
            && text.contains(&format!(
                "{TILES_ERP} in the game folder still holds the launcher's changes from an earlier session"
            ))
            && text.contains("repair F1 25 in the EA app"),
        "{text}"
    );
    assert_eq!(tile_names(&s.game.dir.join(TILES_ERP)), ["stock", "sw_f1_2016"]);
    let checked = crate::profile::conflicts::check(&s.store, "default", &s.game)
        .unwrap_err()
        .to_string();
    assert!(
        checked.contains("press Play, which puts its backed-up original back"),
        "{checked}"
    );
}

#[test]
fn two_addons_adding_the_same_entry_refuse_and_name_both() {
    if loader_dir().is_some() {
        return;
    }
    let s = Seasons::new("twice", &["stock"]);
    s.back_up_for_build_1(&["stock"]);
    s.add("t.season-a", "Season A", "sw_f1_2016");
    s.add("t.season-b", "Season B", "sw_f1_2016");
    let text = build(&s.store, "default", &s.game).unwrap_err().to_string();
    let (later, earlier) = match text.starts_with("Season B") {
        true => ("Season B", "Season A"),
        false => ("Season A", "Season B"),
    };
    assert!(
        text.starts_with(&format!(
            "{later} and {earlier} both add the entry Name=sw_f1_2016"
        )),
        "{text}"
    );
    assert!(text.contains("turn one of them off"), "{text}");
    assert_eq!(tile_names(&s.game.dir.join(TILES_ERP)), ["stock"]);
}
