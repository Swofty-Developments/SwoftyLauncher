use super::*;

const SELECT: &str = "eaid://ui_package/screens/driver_career_driver_select.ui";
const TEAM: &str = "eaid://ui_package/screens/driver_career_team_select.ui";
const ARCHIVES: [&str; 2] = [
    "2025_asset_groups/ui_package/flow_career_season_setup.erp",
    "2025_asset_groups/ui_package/screens/driver_career_driver_select.erp",
];

fn ui(name: &str, fragments: &[(&[u8; 4], &[u8])]) -> Resource {
    let mut res = Resource {
        name: name.into(),
        kind: erp::kind("UIDF").unwrap(),
        unk: 9u32.to_le_bytes().to_vec(),
        fragments: fragments
            .iter()
            .map(|(n, body)| Fragment::new(**n, body.len() as u64, 0x10, ZSTD, compress(body).unwrap()))
            .collect(),
        hash: [0; 16],
    };
    res.hash = res.content_hash().unwrap();
    res
}

fn names(res: &Resource) -> Vec<[u8; 4]> {
    res.fragments.iter().map(|f| f.name).collect()
}

struct Game {
    root: PathBuf,
}

impl Game {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("swofty-templates-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        for rel in ARCHIVES {
            let path = root.join("game").join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(
                path,
                archive(vec![
                    ui(
                        SELECT,
                        &[(b"temp", b"STOCK-SCREENS"), (b"syst", b"STOCK-TEMPLATES")],
                    ),
                    ui(TEAM, &[(b"temp", b"STOCK-TEAM")]),
                ]),
            )
            .unwrap();
        }
        let addon = root.join("addon/styles");
        std::fs::create_dir_all(&addon).unwrap();
        for (file, resources) in [
            (
                "select.erpk",
                vec![
                    ui(SELECT, &[(b"temp", b"ADDON-SCREENS")]),
                    ui(TEAM, &[(b"temp", b"ADDON-TEAM")]),
                ],
            ),
            (
                "own.erpk",
                vec![ui(
                    SELECT,
                    &[(b"temp", b"OWN-SCREENS"), (b"syst", b"OWN-TEMPLATES")],
                )],
            ),
        ] {
            std::fs::write(addon.join(file), archive(resources)).unwrap();
        }
        Self { root }
    }

    fn put(&self, source: &str, resource: &str) -> ErpOp {
        erp_ops(&format!(
            r#"{{"schema_version":1,"ops":[{{"op":"put-resource","erp":"{}","resource":"{resource}","source":"{source}"}}]}}"#,
            ARCHIVES[1]
        ))
        .remove(0)
    }

    fn build(&self, name: &str, ops: &[(&ErpOp, &str)]) -> (Vec<String>, Vec<Erp>) {
        let game = self.root.join("game");
        let addon = self.root.join("addon");
        let original = |rel: &str| game.join(rel);
        let resolve = |s: &Source| Ok(s.resolve(&addon)?);
        let mut b = ErpBuild::new(&original);
        for (op, origin) in ops {
            b.apply_for(op, origin, &resolve).unwrap();
        }
        let notes = b.take_notes();
        let out = self.root.join(name);
        b.write(&out).unwrap();
        let written = ARCHIVES
            .iter()
            .filter_map(|rel| std::fs::read(out.join(rel)).ok())
            .map(|bytes| Erp::parse(&bytes).unwrap())
            .collect();
        (notes, written)
    }

    fn stock(&self) -> Erp {
        Erp::parse(&std::fs::read(self.root.join("game").join(ARCHIVES[1])).unwrap()).unwrap()
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).ok();
    }
}

fn found<'e>(erp: &'e Erp, name: &str) -> &'e Resource {
    erp.resources.iter().find(|r| r.name == name).unwrap()
}

#[test]
fn a_put_ui_without_templates_keeps_the_templates_of_the_copy_it_replaces() {
    let g = Game::new("put");
    let put = g.put("styles/select.erpk", SELECT);
    let (notes, written) = g.build("out", &[(&put, "be.addon")]);
    assert_eq!(
        notes,
        [format!(
            "{}: {SELECT} from be.addon has no templates fragment; the templates of the copy it replaces are kept",
            ARCHIVES[1]
        )]
    );
    let [erp] = written.as_slice() else {
        panic!("one archive written, got {}", written.len());
    };
    let res = found(erp, SELECT);
    assert_eq!(names(res), [*b"temp", *b"syst"]);
    assert_eq!(
        data(res),
        [b"ADDON-SCREENS".to_vec(), b"STOCK-TEMPLATES".to_vec()]
    );
    assert_eq!(res.hash, res.content_hash().unwrap());
    let stock = g.stock();
    let syst = &found(&stock, SELECT).fragments[1];
    assert_eq!(
        (res.fragments[1].size, res.fragments[1].flags),
        (syst.size, syst.flags)
    );
}

#[test]
fn a_put_ui_with_templates_or_over_a_copy_without_any_is_kept_as_it_is() {
    let g = Game::new("own");
    let own = g.put("styles/own.erpk", SELECT);
    let team = g.put("styles/select.erpk", TEAM);
    let (notes, written) = g.build("out", &[(&own, "own.addon"), (&team, "be.addon")]);
    assert!(notes.is_empty(), "{notes:?}");
    let erp = &written[0];
    assert_eq!(
        data(found(erp, SELECT)),
        [b"OWN-SCREENS".to_vec(), b"OWN-TEMPLATES".to_vec()]
    );
    let res = found(erp, TEAM);
    assert_eq!(names(res), [*b"temp"]);
    assert_eq!(data(res), [b"ADDON-TEAM".to_vec()]);
}

#[test]
fn a_later_put_without_templates_keeps_an_earlier_addons_templates() {
    let g = Game::new("later");
    let own = g.put("styles/own.erpk", SELECT);
    let put = g.put("styles/select.erpk", SELECT);
    let (notes, written) = g.build("out", &[(&own, "own.addon"), (&put, "be.addon")]);
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(
        data(found(&written[0], SELECT)),
        [b"ADDON-SCREENS".to_vec(), b"OWN-TEMPLATES".to_vec()]
    );
}

#[test]
fn an_overlay_into_every_archive_keeps_the_templates_in_each() {
    let g = Game::new("overlay");
    let overlay = erp_ops(&format!(
        r#"{{"schema_version":1,"ops":[{{"op":"overlay","erps":["{}","{}"],"source":"styles/select.erpk"}}]}}"#,
        ARCHIVES[0], ARCHIVES[1]
    ))
    .remove(0);
    let (notes, written) = g.build("out", &[(&overlay, "be.addon")]);
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert_eq!(written.len(), 2);
    for erp in &written {
        assert_eq!(
            data(found(erp, SELECT)),
            [b"ADDON-SCREENS".to_vec(), b"STOCK-TEMPLATES".to_vec()]
        );
        assert_eq!(names(found(erp, TEAM)), [*b"temp"]);
    }
}
