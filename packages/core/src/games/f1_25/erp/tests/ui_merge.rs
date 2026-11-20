use super::*;
use crate::ui::node::find_path;
use crate::ui::node::{NodeExt, arr, boolv, group_node, obj, set_prop, u32v};

const TEMPLATE: &str = "eaid://ui_package/templates/t.ui";

fn tree(edit: &dyn Fn(&mut Vec<Node>)) -> Vec<u8> {
    let mut kids = vec![group_node("a", vec![("visible", boolv(true))], &[], vec![])];
    edit(&mut kids);
    let panel = group_node("panel", vec![("visible", boolv(true))], &[], kids);
    Pkg {
        name: [0; 4],
        root: obj(vec![("Templates", arr(vec![panel]))]),
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

fn set(kids: &mut [Node], x: u32) {
    set_prop(&mut kids[0], "x", u32v(x)).unwrap();
}

fn leaf(name: &str) -> Node {
    group_node(name, vec![], &[], vec![])
}

struct Rig {
    root: PathBuf,
}

impl Rig {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-uimerge-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("game/ui")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(
            root.join("game/ui/a.erp"),
            typed_erp("UI", &[(TEMPLATE, tree(&|_| {}))]),
        )
        .unwrap();
        for (file, bytes) in [
            (
                "one.pkg",
                tree(&|k| {
                    set(k, 1);
                    k.push(leaf("from_one"));
                }),
            ),
            (
                "two.pkg",
                tree(&|k| {
                    set(k, 2);
                    k.push(leaf("from_two"));
                }),
            ),
            ("three.pkg", tree(&|k| k.push(leaf("from_three")))),
        ] {
            std::fs::write(root.join("src").join(file), bytes).unwrap();
        }
        Self { root }
    }

    fn replace(&self, build: &mut ErpBuild, origin: &str, file: &str) {
        let op = ErpOp::ReplaceResource {
            erp: "ui/a.erp".into(),
            resource: TEMPLATE.into(),
            fragment: 0,
            source: Source {
                root: crate::asset::Root::Addon,
                path: file.into(),
            },
        };
        let src = self.root.join("src");
        build
            .apply_for(&op, origin, &|s: &Source| Ok(src.join(&s.path)))
            .unwrap();
    }

    fn template(&self, build: &mut ErpBuild) -> Pkg {
        let bytes = build.fragments("ui/a.erp", TEMPLATE).unwrap().unwrap().remove(0);
        Pkg::parse(&bytes).unwrap()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

#[test]
fn two_addons_replacing_a_template_whole_are_merged_and_the_conflict_names_both() {
    let rig = Rig::new("conflict");
    let game = rig.root.join("game");
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original).with_baseline(&original);
    rig.replace(&mut b, "a.one", "one.pkg");
    assert!(b.take_ui_merges().is_empty(), "nothing to merge with yet");
    rig.replace(&mut b, "b.two (ops.json)", "two.pkg");
    let merges = b.take_ui_merges();
    assert_eq!(merges.len(), 1);
    assert_eq!(merges[0].earlier, ["a.one"]);
    assert_eq!(merges[0].later, "b.two");
    assert_eq!(merges[0].conflicts.len(), 1);
    assert_eq!(merges[0].conflicts[0].path, "panel.a [props/x]");
    assert_eq!(merges[0].conflicts[0].earlier, ["a.one"]);
    let t = rig.template(&mut b);
    for n in ["from_one", "from_two"] {
        assert!(find_path(&t.root, &format!("panel.{n}")).is_ok(), "{n}");
    }
    let at = find_path(&t.root, "panel.a").unwrap();
    let a = crate::ui::node::node_at(&t.root, &at).unwrap();
    assert_eq!(
        a.props().unwrap().get("x").unwrap().as_val().unwrap().as_u32(),
        Some(2)
    );
    rig.replace(&mut b, "c.three", "three.pkg");
    let merges = b.take_ui_merges();
    assert_eq!(merges[0].earlier, ["a.one", "b.two"]);
    assert!(merges[0].conflicts.is_empty());
    let t = rig.template(&mut b);
    for n in ["from_one", "from_two", "from_three"] {
        assert!(find_path(&t.root, &format!("panel.{n}")).is_ok(), "{n}");
    }
}

#[test]
fn one_addon_replacing_twice_and_a_build_without_a_baseline_are_not_merged() {
    let rig = Rig::new("single");
    let game = rig.root.join("game");
    let original = |rel: &str| game.join(rel);
    let mut b = ErpBuild::new(&original).with_baseline(&original);
    rig.replace(&mut b, "a.one", "one.pkg");
    rig.replace(&mut b, "a.one (second)", "two.pkg");
    assert!(b.take_ui_merges().is_empty());
    let t = rig.template(&mut b);
    assert!(
        find_path(&t.root, "panel.from_one").is_err(),
        "the addon's own later file wins"
    );

    let mut b = ErpBuild::new(&original);
    rig.replace(&mut b, "a.one", "one.pkg");
    rig.replace(&mut b, "b.two", "two.pkg");
    assert!(b.take_ui_merges().is_empty());
    let t = rig.template(&mut b);
    assert!(
        find_path(&t.root, "panel.from_one").is_err(),
        "the later replacement wins whole"
    );

    let mut b = ErpBuild::new(&original).with_baseline(&original);
    b.set_merge_ui(false);
    rig.replace(&mut b, "a.one", "one.pkg");
    rig.replace(&mut b, "b.two", "two.pkg");
    assert!(b.take_ui_merges().is_empty());
}
