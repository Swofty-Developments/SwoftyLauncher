use super::*;

const TILES_ERP: &str = "ui/tiles.erp";
const DATABASES_ERP: &str = "2025_asset_groups/ui_package/databases.erp";

fn obj(pairs: Vec<(&str, swofty_formats::pkg::Node)>) -> swofty_formats::pkg::Node {
    use swofty_formats::pkg::{Node, Obj, Val};
    Node::Obj(Obj::new(
        pairs.into_iter().map(|(k, v)| (Val::str(k), v)).collect(),
    ))
}

fn uidb_archive(name: &str, root: swofty_formats::pkg::Node) -> Vec<u8> {
    use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED};
    use swofty_formats::pkg::Pkg;
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
        name: name.into(),
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

fn tiles_archive(names: &[&str]) -> Vec<u8> {
    use swofty_formats::pkg::{Arr, Node, Val};
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
    uidb_archive("eaid://ui_package/databases/overrides.uidb", root)
}

fn databases_archive(tooltips: &[&str]) -> Vec<u8> {
    use swofty_formats::pkg::{Node, Val};
    let dict = tooltips
        .iter()
        .map(|n| (*n, obj(vec![("Name", Node::Val(Val::str(*n)))])))
        .collect();
    let root = obj(vec![(
        "tool_tips",
        obj(vec![(
            "Contents",
            obj(vec![("tooltips_dictionary", obj(vec![("Contents", obj(dict))]))]),
        )]),
    )]);
    uidb_archive("eaid://ui_package/databases/tool_tips.uidb", root)
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
        self.back_up_for_build_1_as(TILES_ERP, &tiles_archive(names));
    }

    fn back_up_for_build_1_as(&self, rel: &str, data: &[u8]) {
        let clean = self.root.join("clean.erp");
        std::fs::write(&clean, data).unwrap();
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
            rel.into(),
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

impl Seasons {
    fn databases_in_game(&self, tooltips: &[&str]) -> PathBuf {
        let path = self.game.dir.join(DATABASES_ERP);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, databases_archive(tooltips)).unwrap();
        path
    }
}

#[test]
fn a_game_databases_erp_holding_the_launchers_own_tooltip_gets_its_original_back() {
    let s = Seasons::new("menus-healed", &["stock"]);
    let path = s.databases_in_game(&["settings_fe_home", "mod_menu", "swofty_adjust"]);
    s.back_up_for_build_1_as(DATABASES_ERP, &databases_archive(&["settings_fe_home"]));
    let original = |rel: &str| original(&s.store.root, &s.game, rel);
    let Err(Error::StaleInput { rel, detail }) = check_menu_originals(&original) else {
        panic!("the launcher's own tooltips in the game's databases.erp are a stale input");
    };
    assert_eq!(rel, DATABASES_ERP);
    assert_eq!(
        detail,
        "menus: the game's databases.erp already holds the tooltip mod_menu, the tooltip swofty_adjust"
    );
    let note = heal_stale(&s.store.root, &s.game, &rel).unwrap().unwrap();
    assert_eq!(
        note,
        format!(
            "{DATABASES_ERP} in the game folder still held the launcher's changes from an earlier session; put back \
             its original backed up for game build 1"
        )
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        databases_archive(&["settings_fe_home"])
    );
    check_menu_originals(&original).unwrap();
}

#[test]
fn a_game_databases_erp_with_only_the_games_own_tooltips_is_no_stale_input() {
    let s = Seasons::new("menus-clean", &["stock"]);
    s.databases_in_game(&["settings_fe_home", "delete", "change_driver"]);
    check_menu_originals(&|rel: &str| original(&s.store.root, &s.game, rel)).unwrap();
}

#[test]
fn a_recorded_original_holding_the_launchers_own_tooltip_is_a_stale_input_too() {
    let s = Seasons::new("menus-recorded", &["stock"]);
    let stale = databases_archive(&["settings_fe_home", "mod_menu"]);
    s.databases_in_game(&["settings_fe_home"]);
    let objects = Objects {
        root: s.store.root.join("baseline/objects"),
    };
    let staged = s.root.join("stale.erp");
    std::fs::write(&staged, &stale).unwrap();
    let mut b = Baseline::new(&s.game.identity);
    b.files.insert(
        DATABASES_ERP.into(),
        crate::game_folder::baseline::FileRecord {
            size: stale.len() as u64,
            sha256: objects.put_file(&staged).unwrap(),
            source: crate::game_folder::baseline::Source::FirstSight,
        },
    );
    b.save(&s.store.root).unwrap();
    let original = |rel: &str| original(&s.store.root, &s.game, rel);
    let Err(Error::StaleInput { rel, .. }) = check_menu_originals(&original) else {
        panic!("a recorded original holding the launcher's tooltip is a stale input");
    };
    let note = heal_stale(&s.store.root, &s.game, &rel).unwrap().unwrap();
    assert!(note.starts_with(&format!(
        "the recorded original of {DATABASES_ERP} held the launcher's changes"
    )));
    check_menu_originals(&original).unwrap();
}

#[test]
fn a_stale_databases_erp_with_no_clean_copy_on_record_says_to_repair_the_game() {
    let s = Seasons::new("menus-unhealed", &["stock"]);
    s.databases_in_game(&["settings_fe_home", "mod_menu"]);
    let original = |rel: &str| original(&s.store.root, &s.game, rel);
    let Err(Error::StaleInput { rel, detail }) = check_menu_originals(&original) else {
        panic!("the launcher's own tooltip in the game's databases.erp is a stale input");
    };
    assert_eq!(heal_stale(&s.store.root, &s.game, &rel).unwrap(), None);
    let text = stale_text(&detail, &rel, false);
    assert!(
        text.starts_with("menus: the game's databases.erp already holds the tooltip mod_menu. ")
            && text.contains("the launcher has no clean copy of it on record")
            && text.contains("repair F1 25 in the EA app"),
        "{text}"
    );
}
