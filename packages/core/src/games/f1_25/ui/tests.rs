#[test]
fn a_second_menu_build_takes_its_frames_from_the_cache() {
    use crate::erp::pack::frame_cache;
    use swofty_formats::erp::{Erp, HEADER_LEN, MAGIC};
    let dir = std::env::temp_dir().join(format!("swofty-ui-frames-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let screen: Vec<u8> = (0..24u32 << 10)
        .map(|i| (i % 251) as u8 ^ (i / 1021) as u8)
        .collect();
    let added: Vec<u8> = screen.iter().rev().copied().collect();
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    let src = dir.join("src.erp");
    let ui = "eaid://ui_package/screens/a.ui";
    let archive = Erp {
        header,
        version: 3,
        resources: vec![frame_cache::zstd_resource(ui, "UIDF", &screen)],
    }
    .to_bytes()
    .unwrap();
    std::fs::write(&src, archive).unwrap();
    let packer = Packer::cached(dir.join("zstd"));
    let write = |dst: &Path| {
        let patch: Patch = Box::new(|d: &[u8]| Ok([d, b"edited"].concat()));
        let new = (
            "eaid://ui_package/screens/b.ui".to_owned(),
            "UIDF",
            vec![added.clone()],
        );
        write_archive(
            &src,
            dst,
            vec![(ui, 0, patch)],
            vec![new],
            &ArchiveWriter::new(&packer),
        )
        .unwrap();
        std::fs::read(dst).unwrap()
    };
    let first = write(&dir.join("first/a.erp"));
    assert_eq!(frame_cache::marked_in(&first), 0);
    assert_eq!(frame_cache::mark(&dir.join("zstd")), 2);
    let second = write(&dir.join("second/a.erp"));
    assert_eq!(frame_cache::marked_in(&second), 2);
    let data = |bytes: &[u8]| -> Vec<Vec<u8>> {
        let erp = Erp::parse(bytes).unwrap();
        erp.resources
            .iter()
            .map(|r| r.fragments[0].data().unwrap())
            .collect()
    };
    assert_eq!(data(&second), data(&first));
    assert_eq!(data(&first), [[&screen[..], b"edited"].concat(), added]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_GAME, a stock F1 25 folder"]
fn the_car_category_screen_gets_its_addon_boxes_within_the_loader_rules() {
    let stock = std::path::PathBuf::from(std::env::var_os("SWOFTY_STOCK_GAME").expect("set"));
    let out = std::env::temp_dir().join(format!("swofty-car-category-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let input = |rel: &str| stock.join(rel);
    let (mut lines, mut warnings) = (Vec::new(), Vec::new());
    let written = car_category_boxes(
        &input,
        &|_, _| None,
        &out,
        &mut lines,
        &mut warnings,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert_eq!(written, car_category::ARCHIVES, "{warnings:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
    for rel in car_category::ARCHIVES {
        let dst = out.join(ui_file(rel));
        let ui = read_frag(&dst, car_category::UI, car_category::UI_FRAGMENT).unwrap();
        let scene = read_frag(&dst, car_category::SCENE, 0).unwrap();
        assert!(car_category::taken(&ui, &scene).unwrap(), "{rel}");
        for k in 0..car_category::SLOTS {
            let text = String::from_utf8_lossy(&ui).into_owned();
            for want in [
                car_category::slot(k),
                car_category::entry(k),
                car_category::command(k),
            ] {
                assert!(text.contains(&want), "{rel}: {want}");
            }
        }
    }
    let again = car_category_boxes(
        &|rel: &str| out.join(rel),
        &|_, _| None,
        &out,
        &mut Vec::new(),
        &mut Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(again.is_empty(), "boxes are added once");
    std::fs::remove_dir_all(&out).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_GAME, a stock F1 25 folder"]
fn the_era_rows_and_car_category_boxes_write_each_archive_once() {
    let stock = std::path::PathBuf::from(std::env::var_os("SWOFTY_STOCK_GAME").expect("set"));
    let out = std::env::temp_dir().join(format!("swofty-picker-rows-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let input = |rel: &str| stock.join(rel);
    let (mut lines, mut warnings) = (Vec::new(), Vec::new());
    WRITTEN.with(|w| w.borrow_mut().clear());
    let written = picker_rows(
        true,
        &input,
        &|_, _| None,
        &out,
        &mut lines,
        &mut warnings,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    let wrote = WRITTEN.with(|w| w.take());
    let all = [era::ARCHIVES[0], era::ARCHIVES[1], car_category::ARCHIVES[1]];
    assert_eq!(written, all, "{warnings:?}");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        lines,
        [
            "ui: the Grand Prix and Time Trial era picker gets Built In and Addons rows in 2 archive(s)"
                .to_owned(),
            format!(
                "ui: the Car Category screen gets {} addon box slot(s) in 2 archive(s)",
                car_category::SLOTS
            ),
        ]
    );
    let world = input(&ui_file(era::ARCHIVES[0]));
    let title = read_frag(&world, era::TITLE_SCENE, 0).unwrap();
    for rel in all {
        let before = Erp::parse(&std::fs::read(input(&ui_file(rel))).unwrap()).unwrap();
        let after = Erp::parse(&std::fs::read(out.join(ui_file(rel))).unwrap()).unwrap();
        let mut edited: BTreeMap<&str, (usize, Vec<u8>)> = BTreeMap::new();
        let frag = |name: &str, i: usize| before.find(name).unwrap().fragments[i].data().unwrap();
        if era::ARCHIVES.contains(&rel) {
            let ui = era::build_ui(&frag(era::UI, era::UI_FRAGMENT)).unwrap();
            let scene = era::build_scene(&frag(era::SCENE, 0), &title).unwrap();
            edited.insert(era::UI, (era::UI_FRAGMENT, ui));
            edited.insert(era::SCENE, (0, scene));
        }
        if car_category::ARCHIVES.contains(&rel) {
            let ui = car_category::build_ui(&frag(car_category::UI, car_category::UI_FRAGMENT)).unwrap();
            let scene = car_category::build_scene(&frag(car_category::SCENE, 0)).unwrap();
            edited.insert(car_category::UI, (car_category::UI_FRAGMENT, ui));
            edited.insert(car_category::SCENE, (0, scene));
        }
        let added = if era::ROW_FADE && era::ARCHIVES.contains(&rel) {
            2
        } else {
            0
        };
        assert_eq!(after.resources.len(), before.resources.len() + added, "{rel}");
        for (old, new) in before.resources.iter().zip(&after.resources) {
            assert_eq!(new.name, old.name, "{rel}");
            assert_eq!(new.fragments.len(), old.fragments.len(), "{rel}: {}", old.name);
            for (i, (a, b)) in old.fragments.iter().zip(&new.fragments).enumerate() {
                match edited.get(old.name.as_str()) {
                    Some((at, data)) if *at == i => {
                        assert!(b.data().unwrap() == *data, "{rel}: {} {i}", old.name)
                    }
                    Some(_) => assert!(b.data().unwrap() == a.data().unwrap(), "{rel}: {} {i}", old.name),
                    None => assert!(
                        b.raw().unwrap() == a.raw().unwrap(),
                        "{rel}: {} {i} is not copied as it is",
                        old.name
                    ),
                }
            }
        }
        if added > 0 {
            let fade = era::fade(
                &read_frag(&world, era::FADE_SOURCE, 0).unwrap(),
                &read_frag(&world, &format!("{}.image", era::FADE_SOURCE), 0).unwrap(),
                era::FADE,
            )
            .unwrap();
            let tail = &after.resources[before.resources.len()..];
            assert_eq!(tail[0].name, era::FADE);
            assert_eq!(tail[1].name, format!("{}.image", era::FADE));
            assert!(tail[0].fragments[0].data().unwrap() == fade[0]);
            let image: Vec<Vec<u8>> = tail[1].fragments.iter().map(|f| f.data().unwrap()).collect();
            assert!(image == fade[1..]);
        }
    }
    let mut wrote = wrote;
    wrote.sort();
    let mut want: Vec<PathBuf> = all.iter().map(|rel| out.join(ui_file(rel))).collect();
    want.sort();
    assert_eq!(
        wrote, want,
        "each archive is written once, straight to where it goes"
    );
    let mut fade = Vec::new();
    if era::ROW_FADE {
        let made = era::fade(
            &read_frag(&world, era::FADE_SOURCE, 0).unwrap(),
            &read_frag(&world, &format!("{}.image", era::FADE_SOURCE), 0).unwrap(),
            era::FADE,
        )
        .unwrap();
        fade.push((era::FADE.to_owned(), era::SRV_KIND, vec![made[0].clone()]));
        fade.push((
            format!("{}.image", era::FADE),
            era::SURFACE_KIND,
            made[1..].to_vec(),
        ));
    }
    for rel in all {
        let stock_at = input(&ui_file(rel));
        let frag = |name: &str, i: usize| read_frag(&stock_at, name, i).unwrap();
        let mut at = stock_at.clone();
        if era::ARCHIVES.contains(&rel) {
            let ui = era::build_ui(&frag(era::UI, era::UI_FRAGMENT)).unwrap();
            let scene = era::build_scene(&frag(era::SCENE, 0), &title).unwrap();
            let next = out.join("two-passes/era").join(ui_file(rel));
            let patches: Vec<(&str, usize, Patch)> = vec![
                (era::UI, era::UI_FRAGMENT, Box::new(move |_| Ok(ui))),
                (era::SCENE, 0, Box::new(move |_| Ok(scene))),
            ];
            write_archive(
                &at,
                &next,
                patches,
                fade.clone(),
                &ArchiveWriter::new(&Packer::default()),
            )
            .unwrap();
            at = next;
        }
        if car_category::ARCHIVES.contains(&rel) {
            let ui = car_category::build_ui(&frag(car_category::UI, car_category::UI_FRAGMENT)).unwrap();
            let scene = car_category::build_scene(&frag(car_category::SCENE, 0)).unwrap();
            let next = out.join("two-passes/cars").join(ui_file(rel));
            let patches: Vec<(&str, usize, Patch)> = vec![
                (
                    car_category::UI,
                    car_category::UI_FRAGMENT,
                    Box::new(move |_| Ok(ui)),
                ),
                (car_category::SCENE, 0, Box::new(move |_| Ok(scene))),
            ];
            write_archive(
                &at,
                &next,
                patches,
                Vec::new(),
                &ArchiveWriter::new(&Packer::default()),
            )
            .unwrap();
            at = next;
        }
        assert!(
            std::fs::read(&at).unwrap() == std::fs::read(out.join(ui_file(rel))).unwrap(),
            "{rel} is not byte for byte what the era picker's and then the Car Category's own pass write"
        );
    }
    std::fs::remove_dir_all(&out).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_FLOW_FRONTEND, a stock flow_frontend.erp"]
fn the_career_content_select_gets_its_rows_within_the_loader_rules() {
    let src = std::path::PathBuf::from(std::env::var_os("SWOFTY_STOCK_FLOW_FRONTEND").expect("set"));
    let mut lines = Vec::new();
    let mut warnings = Vec::new();
    let (ui, type_ui, scene, templates) = career_row(&src, &|_, _| None, &mut lines, &mut warnings)
        .unwrap()
        .expect("a stock archive takes the rows");
    let out = std::env::temp_dir().join(format!("swofty-career-row-{}", std::process::id()));
    let dst = out.join("flow_frontend.erp");
    write_archive(
        &src,
        &dst,
        vec![
            (career_select::UI, 0, Box::new(move |_| Ok(ui))),
            (career_select::TYPE_UI, 0, Box::new(move |_| Ok(type_ui))),
            (career_select::SCENE, 0, Box::new(move |_| Ok(scene))),
            (
                career_select::TEMPLATES,
                career_select::TEMPLATES_FRAGMENT,
                Box::new(move |_| Ok(templates)),
            ),
        ],
        Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    let name = ui_file(career_select::ARCHIVE);
    let report = check::archive_files(&[(name.as_str(), src.as_path(), dst.as_path())]).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    assert_eq!(
        lines,
        ["ui: the career content select gets Built In and Addons rows"]
    );
    std::fs::remove_dir_all(&out).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_FLOW_FRONTEND, a stock flow_frontend.erp"]
fn the_main_menu_gets_its_rows_within_the_loader_rules() {
    let src = std::path::PathBuf::from(std::env::var_os("SWOFTY_STOCK_FLOW_FRONTEND").expect("set"));
    let mut lines = Vec::new();
    let mut warnings = Vec::new();
    let (scene, templates) = main_menu_rows(&src, None, &mut lines, &mut warnings)
        .unwrap()
        .expect("a stock archive takes the rows");
    assert!(warnings.is_empty(), "{warnings:?}");
    let out = std::env::temp_dir().join(format!("swofty-home-rows-{}", std::process::id()));
    let dst = out.join("flow_frontend.erp");
    write_archive(
        &src,
        &dst,
        vec![
            (home_rows::SCENE, 0, Box::new(move |_| Ok(scene))),
            (
                home_rows::TEMPLATES,
                home_rows::TEMPLATES_FRAGMENT,
                Box::new(move |_| Ok(templates)),
            ),
        ],
        Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    let name = ui_file(home_rows::ARCHIVE);
    let report = check::archive_files(&[(name.as_str(), src.as_path(), dst.as_path())]).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    assert_eq!(lines, ["ui: the main menu gets rows of cards"]);
    std::fs::remove_dir_all(&out).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_COMMON_FRONT_END, a stock common_front_end.erp"]
fn the_player_card_strip_binds_its_visibility_within_the_loader_rules() {
    let src = std::path::PathBuf::from(std::env::var_os("SWOFTY_STOCK_COMMON_FRONT_END").expect("set"));
    let ui = read_frag(&src, home_rows::PLAYERCARD, 0).unwrap();
    assert!(!home_rows::has_strip_binding(&ui).unwrap());
    let out = home_rows::hide_strip(&ui).unwrap();
    assert!(home_rows::has_strip_binding(&out).unwrap());
    let dir = std::env::temp_dir().join(format!("swofty-strip-{}", std::process::id()));
    let dst = dir.join("common_front_end.erp");
    write_archive(
        &src,
        &dst,
        vec![(home_rows::PLAYERCARD, 0, Box::new(move |_| Ok(out)))],
        Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    let name = ui_file(home_rows::PLAYERCARD_ARCHIVE);
    let report = check::archive_files(&[(name.as_str(), src.as_path(), dst.as_path())]).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_PERSISTENT, a stock persistent.erp"]
fn the_main_menu_navigation_routes_and_entries_are_added_within_the_loader_rules() {
    let src = std::path::PathBuf::from(std::env::var_os("SWOFTY_STOCK_PERSISTENT").expect("set"));
    let routes = read_frag(&src, F1_WORLD_ROUTES, 0).unwrap();
    let out = patch::f1_world_routes(&routes).unwrap();
    let home_routes = read_frag(&src, ROUTES, 0).unwrap();
    let home_out = patch::routes(&home_routes, Vec::new(), Vec::new(), &routes).unwrap();
    let customisation = read_frag(&src, CUSTOMISATION_ROUTES, 0).unwrap();
    let customisation_out = patch::customisation_routes(&customisation, &routes).unwrap();
    customisation_built(&home_out, &customisation_out, None).unwrap();
    let flows = read_frag(&src, FLOWS, 0).unwrap();
    let flows_out = patch::f1_world_entries(&flows).unwrap();
    let dir = std::env::temp_dir().join(format!("swofty-f1routes-{}", std::process::id()));
    let dst = dir.join("persistent.erp");
    write_archive(
        &src,
        &dst,
        vec![
            (F1_WORLD_ROUTES, 0, Box::new(move |_| Ok(out))),
            (ROUTES, 0, Box::new(move |_| Ok(home_out))),
            (CUSTOMISATION_ROUTES, 0, Box::new(move |_| Ok(customisation_out))),
            (FLOWS, 0, Box::new(move |_| Ok(flows_out))),
        ],
        Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    let name = ui_file("persistent.erp");
    let report = check::archive_files(&[(name.as_str(), src.as_path(), dst.as_path())]).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    let written = read_frag(&dst, F1_WORLD_ROUTES, 0).unwrap();
    let p = swofty_formats::pkg::Pkg::parse(&written).unwrap();
    let text = format!("{:?}", p.root);
    assert!(text.contains(patch::BACK_HOME_FROM_CONTAINER) && text.contains(patch::HOME_BACK_ROUTE));
    assert!(patch::has_route(
        &read_frag(&dst, ROUTES, 0).unwrap(),
        "fe_home",
        patch::CUSTOMISATION_ROUTE
    ));
    assert!(patch::has_route(
        &read_frag(&dst, CUSTOMISATION_ROUTES, 0).unwrap(),
        patch::CUSTOMISATION_GROUP,
        patch::BACK_HOME_FROM_CONTAINER
    ));
    let flows_written = read_frag(&dst, FLOWS, 0).unwrap();
    let text = format!(
        "{:?}",
        swofty_formats::pkg::Pkg::parse(&flows_written).unwrap().root
    );
    for entry in [
        patch::TIME_TRIAL_ENTRY,
        patch::GRAND_PRIX_ENTRY,
        patch::RETURN_ENTRY,
    ] {
        assert!(text.contains(entry), "{entry}");
    }
    assert!(text.contains("time_trial") && text.contains("grand_prix_new"));
    use node::{NodeExt, ObjExt};
    let route_text = |data: &[u8], group: &str, id: &str| {
        let p = swofty_formats::pkg::Pkg::parse(data).unwrap();
        let r = p
            .root
            .o()
            .unwrap()
            .at("RouteGroupings")
            .unwrap()
            .a()
            .unwrap()
            .iter()
            .filter(|g| g.o().unwrap().str_at("Id").ok() == Some(group))
            .flat_map(|g| g.o().unwrap().at("Routes").unwrap().a().unwrap().iter())
            .find(|r| r.o().unwrap().str_at("Id").ok() == Some(id))
            .unwrap()
            .clone();
        let cmds = r.o().unwrap().at("Commands").unwrap().a().unwrap();
        cmds.iter()
            .map(|c| {
                let c = c.o().unwrap();
                let attrs: Vec<String> = c
                    .at("Attributes")
                    .unwrap()
                    .a()
                    .unwrap()
                    .iter()
                    .map(|a| {
                        let a = a.o().unwrap();
                        format!("{}={}", a.str_at("Id").unwrap(), a.str_at("Value").unwrap())
                    })
                    .collect();
                format!(
                    "{} [{}] after {}",
                    c.str_at("Type").unwrap(),
                    attrs.join(", "),
                    c.str_at("Dependencies").unwrap_or("-")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let ours = route_text(
        &read_frag(&dst, ROUTES, 0).unwrap(),
        "fe_home",
        patch::SHOWROOM_ROUTE,
    );
    let hub = route_text(&routes, "fe_f1_world", "showroom").replace("=fe_f1_world", "=fe_home");
    assert_eq!(
        ours, hub,
        "the main menu's Showroom route is the hub's own showroom route from the home screen"
    );
    assert!(!ours.contains("slice_bg_content_underlay") && !ours.contains("theatre_container"));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_FLOW_FRONTEND, a stock flow_frontend.erp"]
fn the_session_end_hand_over_lands_on_the_activity_container_within_the_loader_rules() {
    let src = std::path::PathBuf::from(std::env::var_os("SWOFTY_STOCK_FLOW_FRONTEND").expect("set"));
    let ui = read_frag(&src, FRONTEND_TRANSITION, 0).unwrap();
    let out = patch::frontend_transition(&ui).unwrap();
    assert_ne!(ui, out);
    assert!(patch::frontend_transition(&out).is_err(), "patched once only");
    let dir = std::env::temp_dir().join(format!("swofty-transition-{}", std::process::id()));
    let dst = dir.join("flow_frontend.erp");
    write_archive(
        &src,
        &dst,
        vec![(FRONTEND_TRANSITION, 0, Box::new(move |_| Ok(out)))],
        Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    let name = ui_file("flow_frontend.erp");
    let report = check::archive_files(&[(name.as_str(), src.as_path(), dst.as_path())]).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    let written = read_frag(&dst, FRONTEND_TRANSITION, 0).unwrap();
    let text = format!("{:?}", swofty_formats::pkg::Pkg::parse(&written).unwrap().root);
    assert!(text.contains(patch::RETURN_ENTRY));
    std::fs::remove_dir_all(&dir).ok();
}

const SETUP_ROUTE: &str = "career_wizard_driver_career_progressive";

#[test]
#[ignore = "needs SWOFTY_STOCK_DATABASES, a stock databases.erp"]
fn a_databases_erp_an_earlier_build_wrote_is_written_again_the_same() {
    let stock = PathBuf::from(std::env::var_os("SWOFTY_STOCK_DATABASES").expect("set"));
    assert!(
        marks::launcher_entries("databases.erp", &stock)
            .unwrap()
            .is_empty()
    );
    let dir = std::env::temp_dir().join(format!("swofty-databases-again-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    let packer = Packer::default();
    let archives = ArchiveWriter::new(&packer);
    let state = || vec![("page".to_owned(), node::s("home"))];
    let once = dir.join("once/databases.erp");
    write_databases(&(stock.clone(), once.clone()), state(), &archives).unwrap();
    assert_eq!(
        marks::launcher_entries("databases.erp", &once).unwrap(),
        [
            "the tooltip mod_menu",
            "the tooltip mod_menu_offline",
            "the tooltip swofty_adjust",
            "the ui data swofty",
            "the icon list settings.swofty_addons",
        ]
    );
    let twice = dir.join("twice/databases.erp");
    write_databases(&(once.clone(), twice.clone()), state(), &archives).unwrap();
    assert_eq!(std::fs::read(&twice).unwrap(), std::fs::read(&once).unwrap());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_PERSISTENT, a stock persistent.erp"]
fn the_f1_world_entries_go_in_once() {
    let src = PathBuf::from(std::env::var_os("SWOFTY_STOCK_PERSISTENT").expect("set"));
    assert!(
        marks::launcher_entries("persistent.erp", &src)
            .unwrap()
            .is_empty()
    );
    let once = patch::f1_world_entries(&read_frag(&src, FLOWS, 0).unwrap()).unwrap();
    assert_eq!(patch::f1_world_entries(&once).unwrap(), once);
}

fn choice(id: &str, addon: &str, values: &[&str]) -> serde_json::Value {
    let states: Vec<serde_json::Value> = values
        .iter()
        .enumerate()
        .map(|(i, v)| serde_json::json!({ "value": v, "text": v, "command": format!("swofty_set:{addon}:{id}:{i}") }))
        .collect();
    serde_json::json!({ "id": id, "type": "choice", "binds": "native", "label": id.replace('_', " "),
                        "description": format!("About {id}."), "current": 0, "states": states })
}

fn core_with(id: &str, on: bool) -> SessionSettings {
    let states: Vec<serde_json::Value> = [true, false]
        .iter()
        .enumerate()
        .map(|(i, v)| serde_json::json!({ "value": v, "text": v.to_string(), "command": format!("swofty_set:{CORE_ID}:{id}:{i}") }))
        .collect();
    serde_json::from_value(serde_json::json!({
        "schema": 1, "profile": "t", "restart_command": "swofty_restart",
        "addons": [{
            "id": CORE_ID, "name": "SwoftyMod", "version": "0.2.13", "enabled": true, "locked": true,
            "enable_commands": ["", ""],
            "options": [{ "id": id, "type": "toggle", "binds": "native", "label": id, "current": usize::from(!on),
                          "states": states }]
        }]
    }))
    .unwrap()
}

#[test]
fn the_season_picker_splits_unless_swoftymod_turns_it_off() {
    let none: SessionSettings = serde_json::from_value(serde_json::json!({
        "schema": 1, "profile": "t", "restart_command": "swofty_restart", "addons": []
    }))
    .unwrap();
    assert!(
        season_picker_split(&none),
        "a settings.json without the option keeps the split"
    );
    assert!(season_picker_split(&core_with("split_season_picker", true)));
    assert!(!season_picker_split(&core_with("split_season_picker", false)));
    let panel_off = core_with("show_launcher_panel", false);
    assert!(
        season_picker_split(&panel_off),
        "another SwoftyMod toggle leaves it alone"
    );
    assert!(!panel_shown(&panel_off));
}

#[test]
fn swoftymod_declares_the_season_picker_toggle_on() {
    let core = crate::addons::builtin::core_manifest();
    let o = core
        .options
        .iter()
        .find(|o| o.id == "split_season_picker")
        .expect("declared");
    assert_eq!(o.default, serde_json::Value::Bool(true));
}

fn setup_settings() -> SessionSettings {
    let addon = "t.progressive";
    let years: Vec<String> = (2016..=2026).map(|y| y.to_string()).collect();
    let years: Vec<&str> = years.iter().map(String::as_str).collect();
    let mut dev = choice("dev_probe", addon, &["off", "on"]);
    dev["hidden"] = true.into();
    let mut start = choice("start_season", addon, &years);
    start["hidden"] = true.into();
    serde_json::from_value(serde_json::json!({
        "schema": 1, "profile": "t", "restart_command": "swofty_restart",
        "addons": [{
            "id": addon, "name": "Progressive", "version": "0.1.0", "author": "t",
            "description": "Every season takes that year's grid.", "enabled": true, "locked": false,
            "enable_commands": [format!("swofty_enable:{addon}:0"), format!("swofty_enable:{addon}:1")],
            "options": [
                choice("show_hud", addon, &["on", "off"]),
                start,
                choice("rnd_mode", addon, &["real_life", "base", "hybrid"]),
                choice("team_moves", addon, &["follow_driver", "choose"]),
                dev,
            ],
            "career_setup": { "route": SETUP_ROUTE, "title": "Progressive Career",
                              "options": ["start_season", "rnd_mode", "team_moves"] }
        }]
    }))
    .unwrap()
}

fn stock_input() -> impl Fn(&str) -> PathBuf {
    let data = PathBuf::from(std::env::var_os("SWOFTY_STOCK_DATA").expect("set"));
    let baseline = std::fs::read_dir(data.join("baseline"))
        .unwrap()
        .flatten()
        .map(|e| e.path().join("baseline.json"))
        .find(|p| p.is_file())
        .unwrap();
    let b: crate::game_folder::baseline::Baseline =
        serde_json::from_slice(&std::fs::read(baseline).unwrap()).unwrap();
    let objects = crate::game_folder::baseline::Objects {
        root: data.join("baseline/objects"),
    };
    let game = std::env::var_os("SWOFTY_STOCK_GAME").map(PathBuf::from);
    move |rel: &str| match (b.files.get(rel), &game) {
        (Some(f), _) => objects.path(&f.sha256),
        (None, Some(game)) => game.join(rel),
        (None, None) => panic!("no stock {rel} (SWOFTY_STOCK_GAME reads the files the baseline lacks)"),
    }
}

fn groupings(path: &Path, res: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let p = swofty_formats::pkg::Pkg::parse(&read_frag(path, res, 0).unwrap()).unwrap();
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for g in p.root.o().unwrap().at("RouteGroupings").unwrap().a().unwrap() {
        let go = g.o().unwrap();
        let gid = go.str_at("Id").unwrap().to_owned();
        for r in go.at("Routes").unwrap().a().unwrap() {
            let rid = r.o().unwrap().str_at("Id").unwrap().to_owned();
            out.entry(gid.clone())
                .or_default()
                .insert(rid, node::canonical(r));
        }
    }
    out
}

#[test]
#[ignore = "needs SWOFTY_STOCK_DATA, the launcher data folder that holds the game's baseline"]
fn a_career_setup_page_builds_within_the_loader_rules() {
    let input = stock_input();
    let settings = setup_settings();
    let out = std::env::temp_dir().join(format!("swofty-career-setup-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let built = build(
        &settings,
        &input,
        &|_, _| None,
        &out,
        false,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(
        built
            .screens
            .contains_key(&format!("{UI_DIR}/{}", home_rows::CUSTOMISATION_FLAG)),
        "{:?}",
        built.warnings
    );
    let persistent = ui_file("persistent.erp");
    let report = check::archive_files(&[(
        persistent.as_str(),
        input(&persistent).as_path(),
        out.join(&persistent).as_path(),
    )])
    .unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    let home = format!("swofty_setup_{SETUP_ROUTE}");
    let rows = format!("swofty_addon_{SETUP_ROUTE}_0");
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.contains(&format!("career setup page {home}"))),
        "{:?}",
        built.lines
    );
    let session: BTreeMap<&str, bool> = built
        .session
        .iter()
        .map(|s| (s.name.as_str(), s.view_model))
        .collect();
    assert_eq!(session.get(home.as_str()), Some(&false));
    assert_eq!(session.get(rows.as_str()), Some(&true));

    let options = input(&ui_file("flow_options.erp"));
    let stock = check::ArchiveUi::read("flow_options.erp", &options).unwrap();
    let mut edited = check::ArchiveUi::read("flow_options.erp", &options).unwrap();
    for name in [&home, &rows] {
        let data = std::fs::read(&built.screens[&format!("{UI_DIR}/{name}.ui")]).unwrap();
        edited.resources.insert(
            format!("eaid://ui_package/screens/{name}.ui"),
            ("UIDF".to_owned(), vec![data]),
        );
    }
    let report = check::archives(&[edited], &[stock], catalog::stock(), catalog::engine()).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");

    let rows_ui = std::fs::read(&built.screens[&format!("{UI_DIR}/{rows}.ui")]).unwrap();
    let rows_ui = String::from_utf8_lossy(&rows_ui);
    for id in ["start_season", "rnd_mode", "team_moves"] {
        assert!(
            rows_ui.contains(&format!("swofty_next:t.progressive:{id}")),
            "{id}"
        );
    }
    assert!(!rows_ui.contains("t.progressive:show_hud"));
    let page = std::fs::read(&built.screens[&format!("{UI_DIR}/swofty_addon_0.ui")]).unwrap();
    let page = String::from_utf8_lossy(&page);
    assert!(page.contains("swofty_next:t.progressive:team_moves"));
    for hidden in ["dev_probe", "start_season"] {
        assert!(
            !page.contains(&format!("t.progressive:{hidden}")),
            "a hidden option stays off ADDON SETTINGS: {hidden}"
        );
    }

    let (src_p, dst_p) = (
        input(&ui_file("persistent.erp")),
        out.join(ui_file("persistent.erp")),
    );
    for res in [ROUTES, GAME_ROUTES, WIZARD_ROUTES] {
        let (before, after) = (groupings(&src_p, res), groupings(&dst_p, res));
        for (gid, routes) in &before {
            for (rid, r) in routes {
                assert_eq!(after[gid].get(rid), Some(r), "{res}: {gid}.{rid} changed");
            }
        }
    }
    let after = groupings(&dst_p, ROUTES);
    let entry = &after["fe_career_dlc_select"][SETUP_ROUTE];
    for want in [home.as_str(), "options_flow"] {
        assert!(entry.contains(want), "{entry}");
    }
    assert!(!entry.contains("career_wizard_season_length_select"), "{entry}");
    assert!(entry.contains("Customisation"), "{entry}");
    let ids: Vec<&str> = after[&home].keys().map(String::as_str).collect();
    assert_eq!(
        ids,
        [
            "back",
            "close",
            "confirm",
            "open",
            "open_no_input",
            "open_options"
        ]
    );
    assert!(after[&home]["back"].contains("frontend_flow"));
    assert!(after[&home]["confirm"].contains("career_season_setup_flow"));
    let wizard = groupings(&dst_p, WIZARD_ROUTES);
    let back = &wizard["career_wizard_season_length_select"][&format!("back_{SETUP_ROUTE}")];
    for want in [
        home.as_str(),
        "options_flow",
        "career_wizard_summary",
        "slice_gen_screen_title",
        "Customisation",
        "AutoFocus",
    ] {
        assert!(back.contains(want), "{want}: {back}");
    }
    assert!(after[&home]["back"].contains("AutoFocus"));

    let stock_built = build(
        &setup_settings_without(),
        &input,
        &|_, _| None,
        &out,
        false,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(!stock_built.lines.iter().any(|l| l.contains("career setup")));
    assert_eq!(
        read_frag(&src_p, WIZARD_ROUTES, 0).unwrap(),
        read_frag(&out.join(ui_file("persistent.erp")), WIZARD_ROUTES, 0).unwrap()
    );
    std::fs::remove_dir_all(&out).ok();
}

#[test]
#[ignore = "needs SWOFTY_STOCK_DATA, the launcher data folder that holds the game's baseline"]
fn a_hub_tab_page_builds_within_the_loader_rules() {
    let input = stock_input();
    let mut settings = setup_settings();
    for o in settings.addons[0]
        .options
        .iter_mut()
        .filter(|o| o.id == "rnd_mode")
    {
        o.hidden = true;
    }
    settings.addons[0].hub_tab = Some(crate::manifest::HubTab {
        name: "progressive".into(),
        title: "Progressive Settings".into(),
        options: vec!["rnd_mode".into(), "team_moves".into()],
        data_group: "f1_career_progressive".into(),
    });
    let out = std::env::temp_dir().join(format!("swofty-hub-tab-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let (mut lines, mut warnings) = (Vec::new(), Vec::new());
    let written = hub_tabs(
        &settings,
        &input,
        &|_, _| None,
        &out,
        &mut lines,
        &mut warnings,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert_eq!(written, hub::ARCHIVES);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[0].starts_with("ui: hub tabs copy "), "{lines:?}");
    assert_eq!(
        lines[2],
        "ui: t.progressive has a hub tab template swofty_hub_progressive for f1_career_progressive careers in 2 \
         archive(s); its rows read view_model_data.hud_swofty_hub_progressive"
    );
    let templates = |path: &Path| -> BTreeMap<String, String> {
        let p =
            swofty_formats::pkg::Pkg::parse(&read_frag(path, hub::UI, hub::UI_FRAGMENT).unwrap()).unwrap();
        p.root
            .o()
            .unwrap()
            .at("Templates")
            .unwrap()
            .a()
            .unwrap()
            .iter()
            .map(|t| (t.name().to_owned(), node::canonical(t)))
            .collect()
    };
    let options = input(&ui_file(hub::OPTIONS));
    let options_file = ErpFile::open(&options).unwrap();
    let names =
        |f: &ErpFile| -> BTreeSet<String> { f.erp.resources.iter().map(|r| r.name.clone()).collect() };
    let mut shared = BTreeSet::new();
    for rel in hub::SHARED {
        shared.extend(names(&ErpFile::open(&input(&ui_file(rel))).unwrap()));
    }
    for rel in hub::ARCHIVES {
        let (before, after) = (
            templates(&input(&ui_file(rel))),
            templates(&out.join(ui_file(rel))),
        );
        for (name, t) in &before {
            assert_eq!(after.get(name), Some(t), "{rel}: {name} changed");
        }
        assert_eq!(after.len(), before.len() + 1, "{rel}");
        let page = &after["swofty_hub_progressive"];
        for want in [
            "#view_model_data.hud_swofty_hub_progressive.title",
            "#view_model_data.hud_swofty_hub_progressive.o0.label",
            "#view_model_data.hud_swofty_hub_progressive.o0.value",
            "#view_model_data.hud_swofty_hub_progressive.o1.label",
            "#view_model_data.hud_swofty_hub_progressive.o1.value",
            "swofty_next:t.progressive:rnd_mode",
            "swofty_prev:t.progressive:rnd_mode",
            "swofty_next:t.progressive:team_moves",
            "About rnd_mode.",
            "swofty_adjust",
            "options/options_edit_screen",
        ] {
            assert!(page.contains(want), "{rel}: {want}");
        }
        for unwanted in [
            "o2.label",
            "dataContext",
            "tab_content_emails_panel",
            "messages_panel",
            "email_list",
        ] {
            assert!(!page.contains(unwanted), "{rel}: {unwanted}");
        }

        let (src, dst) = (
            ErpFile::open(&input(&ui_file(rel))).unwrap(),
            ErpFile::open(&out.join(ui_file(rel))).unwrap(),
        );
        let (stock, built) = (names(&src), names(&dst));
        assert!(stock.is_subset(&built), "{rel}");
        let added: BTreeSet<String> = built.difference(&stock).cloned().collect();
        assert!(!added.is_empty(), "{rel}");
        for r in &src.erp.resources {
            if r.name == hub::UI {
                continue;
            }
            let got = dst.erp.resources.iter().find(|d| d.name == r.name).unwrap();
            assert_eq!(got.hash, r.hash, "{rel}: {} changed", r.name);
        }
        let mut available = shared.clone();
        available.extend(built.iter().cloned());
        for name in &added {
            let copy = dst.load(dst.erp.find(name).unwrap()).unwrap();
            let original = options_file.load(options_file.erp.find(name).unwrap()).unwrap();
            assert_eq!(copy.kind, original.kind, "{rel}: {name}");
            assert_eq!(copy.fragments.len(), original.fragments.len(), "{rel}: {name}");
            for (a, b) in copy.fragments.iter().zip(&original.fragments) {
                let data = a.data().unwrap();
                assert_eq!(data, b.data().unwrap(), "{rel}: {name}");
                for r in hub::references(&data) {
                    if !r.starts_with("eaid://ui_package/") || hub::resolves(&available, &r) {
                        continue;
                    }
                    assert!(
                        !hub::resolves(&names(&options_file), &r),
                        "{rel}: {name} uses {r}, which the archive lacks"
                    );
                }
            }
        }
        for scene in hub::SCENES {
            assert!(available.contains(scene), "{rel}: {scene}");
        }

        let built_ui =
            Pkg::parse(&read_frag(&out.join(ui_file(rel)), hub::UI, hub::UI_FRAGMENT).unwrap()).unwrap();
        let tab = built_ui
            .root
            .o()
            .unwrap()
            .at("Templates")
            .unwrap()
            .a()
            .unwrap()
            .iter()
            .find(|t| t.name() == "swofty_hub_progressive")
            .unwrap();
        let on = node::canonical(&node::prop(node::boolv(true), None));
        let shown = |n: &Node| {
            n.props()
                .unwrap()
                .get("visible")
                .is_none_or(|v| node::canonical(v) == on)
        };
        let kids: Vec<&str> = tab
            .slot(node::CHILDREN)
            .unwrap()
            .iter()
            .filter(|c| c.is_node())
            .map(|c| c.name())
            .collect();
        assert_eq!(
            kids,
            [
                "fade_on",
                "tool_tip_interface",
                "hide_title",
                "hide_filter",
                "hide_scroll_bar",
                "about",
                "description",
                "row_0",
                "row_1"
            ],
            "{rel}"
        );
        for r in ["row_0", "row_1"] {
            let toggle = tab.child(r).unwrap().child("toggle").unwrap();
            assert_eq!(toggle.type_code(), "UIGP", "{rel}");
            assert!(!shown(toggle.child("indicator").unwrap()), "{rel}");
        }
        let scene = read_frag(&out.join(ui_file(rel)), hub::PAGE_SCENE, 0).unwrap();
        let scene = Pkg::parse(&scene).unwrap();
        assert!(idf::xref_mismatches(&scene).unwrap().is_empty(), "{rel}");
        assert!(idf::id_problems(&scene).unwrap().is_empty(), "{rel}");
        let scene = idf::scenes(&scene).unwrap()[0].o().unwrap().at("scene").unwrap();
        let parts: Vec<(&str, bool, &Node)> = tab
            .slot(node::CHILDREN)
            .unwrap()
            .iter()
            .filter(|c| c.is_node())
            .filter_map(|c| {
                let step = c
                    .slot(node::COMPONENT)
                    .ok()?
                    .first()?
                    .a()
                    .ok()?
                    .last()?
                    .o()
                    .ok()?;
                let at = idf::find(scene, step.str_at("debugName").ok()?)?;
                Some((c.name(), shown(c), at))
            })
            .collect();
        assert!(parts.iter().any(|p| p.0 == "row_0" && p.1), "{rel}");
        assert!(parts.iter().any(|p| p.0 == "description" && p.1), "{rel}");
        for (shown, _, at) in parts.iter().filter(|p| p.1) {
            for (hidden, _, over) in parts.iter().filter(|p| !p.1) {
                assert!(
                    idf::find(over, idf::elem_name(at)).is_none(),
                    "{rel}: hidden {hidden} covers {shown}"
                );
            }
        }
    }
    settings.addons[0].hub_tab = None;
    let none = hub_tabs(
        &settings,
        &input,
        &|_, _| None,
        &out,
        &mut Vec::new(),
        &mut Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(none.is_empty());
    std::fs::remove_dir_all(&out).ok();
}

fn resume_rows() -> Vec<crate::stdlib::careers::ResumeRow> {
    use crate::stdlib::careers::ResumeRow;
    let row = |index: usize, mode: &str, driver: &str, severity: &str, status: &str| ResumeRow {
        index,
        mode: mode.into(),
        key: format!("slot{index}"),
        group: "f1_career_2026".into(),
        game_mode: "driver_career_26".into(),
        ready: severity == "ready",
        loadable: true,
        severity: severity.into(),
        status: status.into(),
        badge: if mode == "my_team" {
            "My Team"
        } else {
            "Driver Career"
        }
        .into(),
        driver: driver.into(),
        team: "Williams Renault".into(),
        colour: Some(0x00ff_8800),
        season: Some(2),
        race: Some(4),
        played: Some(1_795_093_120),
        addons: Vec::new(),
    };
    vec![
        row(0, "driver", "Nigel Mansell", "ready", "Ready"),
        row(1, "my_team", "Team Save", "missing", "Missing: F1 1990"),
        row(
            2,
            "driver",
            "Ayrton Senna",
            "version",
            "Mismatched version of F1 1990",
        ),
    ]
}

#[test]
fn the_resume_list_screen_has_a_row_per_career_that_raises_its_index() {
    let rows = resume_rows();
    let (screen, bytes) = {
        let Some(dir) = std::env::var_os("SWOFTY_STOCK_DATA") else {
            eprintln!("SWOFTY_STOCK_DATA is not set; skipping");
            return;
        };
        let _ = dir;
        let input = stock_input();
        let flow_options = input(&ui_file("flow_options.erp"));
        let flow_frontend = input(&ui_file("flow_frontend.erp"));
        let src = Sources::parse(
            &read_frag(&flow_options, CONTAINER, 1).unwrap(),
            &read_frag(&flow_frontend, FE_CAREER, 0).unwrap(),
            &read_frag(&flow_frontend, FE_HOME, 0).unwrap(),
            &read_frag(&flow_options, CONTAINER, 0).unwrap(),
        )
        .unwrap();
        pages::build_resume_page(&src, &rows).unwrap()
    };
    assert_eq!(screen, pages::RESUME_SCREEN);
    check_resume_nodes(&bytes, &rows);
    check_row_nodes(&bytes);
}

#[test]
fn a_row_that_cannot_load_raises_its_index_but_routes_nowhere() {
    let Some(_) = std::env::var_os("SWOFTY_STOCK_DATA") else {
        eprintln!("SWOFTY_STOCK_DATA is not set; skipping");
        return;
    };
    let mut rows = resume_rows();
    rows.truncate(1);
    rows[0].loadable = false;
    let input = stock_input();
    let flow_options = input(&ui_file("flow_options.erp"));
    let flow_frontend = input(&ui_file("flow_frontend.erp"));
    let src = Sources::parse(
        &read_frag(&flow_options, CONTAINER, 1).unwrap(),
        &read_frag(&flow_frontend, FE_CAREER, 0).unwrap(),
        &read_frag(&flow_frontend, FE_HOME, 0).unwrap(),
        &read_frag(&flow_options, CONTAINER, 0).unwrap(),
    )
    .unwrap();
    let (_, bytes) = pages::build_resume_page(&src, &rows).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains(&pages::row_command(0)));
    assert!(
        !text.contains(pages::CONTINUE_DRIVER),
        "no route to the continue screen"
    );
}

fn tint_of(n: &Node) -> Option<Vec<u8>> {
    use swofty_formats::pkg::Value;
    match &n
        .props()
        .ok()?
        .get("colourOverride")?
        .as_obj()?
        .get("value")?
        .as_val()?
        .value
    {
        Value::Raw(b) => Some(b.clone()),
        Value::Str(_) => None,
    }
}

fn visible_of(n: &Node) -> Option<bool> {
    n.props()
        .ok()?
        .get("visible")?
        .as_obj()?
        .get("value")?
        .as_val()
        .and_then(|v| match &v.value {
            swofty_formats::pkg::Value::Raw(b) => b.first().map(|b| *b == 1),
            swofty_formats::pkg::Value::Str(_) => None,
        })
}

fn check_row_nodes(bytes: &[u8]) {
    let p = Pkg::parse(bytes).unwrap();
    let scr = p.root.o().unwrap().at("Screens").unwrap().a().unwrap()[0].clone();
    let row = |name: &str| scr.child(name).unwrap();
    for name in ["row_0", "row_1", "row_2", "row_new"] {
        let r = row(name);
        for c in career_row::COLUMNS {
            assert!(r.child(c.name).is_ok(), "{name} has no {} column", c.name);
        }
    }
    for name in ["row_0", "row_1", "row_2", "row_new"] {
        assert_eq!(
            visible_of(row(name).child("indicator").unwrap()),
            Some(false),
            "{name} draws no stock strip"
        );
    }
    assert_eq!(
        tint_of(row("row_0").child("status").unwrap()),
        Some(pages::status_colour("ready").to_vec())
    );
    assert_eq!(
        tint_of(row("row_1").child("status").unwrap()),
        Some(pages::status_colour("missing").to_vec())
    );
    assert_eq!(
        tint_of(row("row_2").child("status").unwrap()),
        Some(pages::status_colour("version").to_vec())
    );
    assert_eq!(tint_of(row("row_new").child("status").unwrap()), None);
}

#[test]
fn a_team_colour_from_the_database_is_the_rgb_a_node_tints_with() {
    assert_eq!(pages::colour_rgb(0x00ff_8800), [0xff, 0x88, 0x00, 0xff]);
    assert_eq!(pages::colour_rgb(16746240), [0xff, 0x87, 0x00, 0xff]);
    let argb: i32 = -16723266;
    assert_eq!(
        pages::colour_rgb(argb as u32 & 0x00ff_ffff),
        [0x00, 0xd2, 0xbe, 0xff]
    );
}

fn check_resume_nodes(bytes: &[u8], rows: &[crate::stdlib::careers::ResumeRow]) {
    let p = Pkg::parse(bytes).unwrap();
    let scr = p.root.o().unwrap().at("Screens").unwrap().a().unwrap()[0].clone();
    assert_eq!(scr.name(), pages::RESUME_SCREEN);
    let text = String::from_utf8_lossy(bytes).into_owned();
    assert!(text.contains("CAREER") && !text.contains("RESUME CAREER"));
    assert!(kids_named(&scr, "row_new"), "the New Career row");
    assert!(text.contains(pages::NEW_CAREER_ROUTE) && text.contains("NEW CAREER"));
    for r in rows {
        let kids: Vec<&str> = scr
            .slot(node::CHILDREN)
            .unwrap()
            .iter()
            .filter(|c| c.is_node())
            .map(|c| c.name())
            .collect();
        assert!(kids.contains(&pages::row_name(r.index).as_str()), "{kids:?}");
        assert!(text.contains(&pages::row_command(r.index)), "row {}", r.index);
        assert!(text.contains(&r.driver), "{}", r.driver);
        assert!(text.contains(&r.team), "{}", r.team);
    }
    assert!(text.contains(pages::CONTINUE_DRIVER) && text.contains(pages::CONTINUE_MY_TEAM));
    assert!(text.contains("Last played 19/11/2026") && text.contains("Season 2, Race 4"));
    assert!(!text.contains('\u{b7}'), "the font has no middle dot");
}

fn kids_named(scr: &Node, name: &str) -> bool {
    scr.slot(node::CHILDREN)
        .unwrap()
        .iter()
        .any(|c| c.is_node() && c.name() == name)
}

fn assert_resolved_references(data: &[u8], input: &dyn Fn(&str) -> PathBuf, built: &[PathBuf]) {
    let mut have = BTreeSet::new();
    for path in built {
        let f = ErpFile::open(path).unwrap();
        have.extend(f.erp.resources.iter().map(|r| r.name.clone()));
    }
    for rel in ["flow_options.erp", "persistent.erp", "flow_persistent.erp"] {
        let f = ErpFile::open(&input(&ui_file(rel))).unwrap();
        have.extend(f.erp.resources.iter().map(|r| r.name.clone()));
    }
    let unresolved: Vec<String> = hub::references(data)
        .into_iter()
        .filter(|r| r.starts_with("eaid://ui_package/") && !hub::resolves(&have, r))
        .collect();
    assert!(
        unresolved.is_empty(),
        "the page references resources flow_options lacks: {unresolved:?}"
    );
}

#[test]
#[ignore = "needs SWOFTY_STOCK_DATA, the launcher data folder that holds the game's baseline"]
fn the_resume_list_builds_within_the_loader_rules_and_routes_in_and_out() {
    let input = stock_input();
    let mut settings = setup_settings_without();
    settings.resume_list = resume_rows();
    settings.resume_list_on = true;
    let out = std::env::temp_dir().join(format!("swofty-resume-list-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let built = build(
        &settings,
        &input,
        &|_, _| None,
        &out,
        false,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(
        built
            .lines
            .iter()
            .any(|l| l.contains("Career list screen swofty_resume_list has 3 career row(s)")),
        "{:?}",
        built.lines
    );
    let persistent = ui_file("persistent.erp");
    let report = check::archive_files(&[(
        persistent.as_str(),
        input(&persistent).as_path(),
        out.join(&persistent).as_path(),
    )])
    .unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    let name = pages::RESUME_SCREEN;
    let data = std::fs::read(&built.screens[&format!("{UI_DIR}/{name}.ui")]).unwrap();
    check_resume_nodes(&data, &settings.resume_list);
    assert_resolved_references(&data, &input, &[out.join(ui_file("persistent.erp"))]);
    let session: BTreeMap<&str, bool> = built
        .session
        .iter()
        .map(|s| (s.name.as_str(), s.view_model))
        .collect();
    assert_eq!(session.get(name), Some(&true));

    let options = input(&ui_file("flow_options.erp"));
    let stock = check::ArchiveUi::read("flow_options.erp", &options).unwrap();
    let mut edited = check::ArchiveUi::read("flow_options.erp", &options).unwrap();
    edited.resources.insert(
        format!("eaid://ui_package/screens/{name}.ui"),
        ("UIDF".to_owned(), vec![data]),
    );
    let report = check::archives(&[edited], &[stock], catalog::stock(), catalog::engine()).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");

    let (src_p, dst_p) = (input(&persistent), out.join(&persistent));
    let (before, after) = (groupings(&src_p, ROUTES), groupings(&dst_p, ROUTES));
    for (gid, routes) in &before {
        for (rid, r) in routes {
            if (gid.as_str(), rid.as_str()) == ("fe_home", "career") {
                continue;
            }
            assert_eq!(after[gid].get(rid), Some(r), "{gid}.{rid} changed");
        }
    }
    {
        let changed = after["fe_home"]["career"].clone();
        assert_ne!(
            before["fe_home"].get("career"),
            Some(&changed),
            "fe_home.career still opens the Career menu"
        );
        assert!(
            changed.contains(name) && changed.contains("options_flow"),
            "{changed}"
        );
        assert!(!changed.contains("fe_career"), "{changed}");
    }
    let career = before["fe_home"]["career"].clone();
    for gid in ["fe_career", "fe_career_dlc_select"] {
        let entry = &after[gid][pages::RESUME_ROUTE];
        for want in [name, "options_flow", "Migrate", "Customisation"] {
            assert!(entry.contains(want), "{gid}: {want}: {entry}");
        }
        assert!(!entry.contains("career_continue_driver"), "{entry}");
    }
    let ids: Vec<&str> = after[name].keys().map(String::as_str).collect();
    assert_eq!(
        ids,
        [
            "back",
            "close",
            "open",
            "open_no_input",
            "solo_career_continue_driver",
            "solo_career_continue_my_team",
            "swofty_new_career"
        ]
    );
    assert!(after[name]["solo_career_continue_driver"].contains("career_continue_driver"));
    assert!(after[name]["solo_career_continue_my_team"].contains("career_continue_my_team"));
    assert!(after[name]["solo_career_continue_driver"].contains("frontend_flow"));
    assert!(after[name]["back"].contains("fe_home") && !after[name]["back"].contains("fe_career"));
    assert!(after[name][pages::NEW_CAREER_ROUTE].contains("fe_career") && career.contains("fe_career"));
    for (id, stock) in [
        ("solo_career_continue_driver", "solo_career_continue_driver"),
        ("solo_career_continue_my_team", "solo_career_continue_my_team"),
    ] {
        let want = &before["fe_career"][stock];
        let got = &after[name][id];
        for step in [
            "Wait",
            "StartStandardWipe",
            "InputState",
            "TriggerSubRoute",
            "Migrate",
        ] {
            let in_stock = want.matches(&format!("Str(\"{step}\")")).count();
            let here = got.matches(&format!("Str(\"{step}\")")).count();
            assert!(here >= in_stock && here > 0, "{id}: {step}: {got}");
        }
        for kept in [
            "tran_pre_wipe_dly_gen",
            "ui_wipes/ui_wipe_standard_in.bk2",
            "tran_wipe_stndrd_in_dly_c",
        ] {
            assert!(got.contains(kept), "{id}: {kept}");
        }
    }

    let none = build(
        &setup_settings_without(),
        &input,
        &|_, _| None,
        &out,
        false,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(!none.lines.iter().any(|l| l.contains("Resume Career list")));
    assert!(!none.screens.keys().any(|k| k.contains(name)));
    assert!(!groupings(&out.join(&persistent), ROUTES).contains_key(name));
    std::fs::remove_dir_all(&out).ok();
}

#[test]
fn the_row_columns_sit_inside_the_row_without_touching() {
    use career_row::{Align, COLUMNS};
    let mut spans: Vec<(f64, f64, &str)> = COLUMNS
        .iter()
        .map(|c| match c.align {
            Align::Start => (c.x, c.x + c.width, c.name),
            Align::End => (c.x - c.width, c.x, c.name),
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(spans[0].0 >= 0.2 && spans.last().unwrap().1 <= 11.0, "{spans:?}");
    for w in spans.windows(2) {
        assert!(w[0].1 <= w[1].0, "{} runs into {}: {spans:?}", w[0].2, w[1].2);
    }
    assert_eq!(
        spans.last().unwrap().2,
        "status",
        "the status is the far right column"
    );
    assert!(COLUMNS.iter().all(|c| c.chars >= 8));
}

#[test]
#[ignore = "needs SWOFTY_STOCK_DATA, the launcher data folder that holds the game's baseline"]
fn the_career_row_scene_is_the_stock_row_with_a_text_per_column() {
    let input = stock_input();
    let flow_options = input(&ui_file("flow_options.erp"));
    let bytes = career_row::row_scene(&read_frag(&flow_options, career_row::STOCK, 0).unwrap()).unwrap();
    let p = Pkg::parse(&bytes).unwrap();
    let root = idf::scenes(&p).unwrap()[0].o().unwrap().at("scene").unwrap();
    let all = idf::elements(root);
    let names: Vec<&str> = all.iter().skip(1).map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        ["tl", "br", "base", "driver", "team", "badge", "place", "status"]
    );
    let base = idf::find(root, "base").unwrap();
    assert_eq!(idf::param_str(base, "fileRef"), Some(career_row::STOCK));
    assert_eq!(idf::xref(base), Some(idf::scene_id(career_row::STOCK)));
    for c in career_row::COLUMNS {
        let e = idf::find(root, c.name).unwrap();
        assert_eq!(idf::param_str(e, "style"), Some(c.style));
        assert_eq!(
            idf::param_str(e, "wrapping"),
            Some("none"),
            "{}: no shrinking",
            c.name
        );
        assert!(
            e.as_obj()
                .unwrap()
                .get("anims")
                .and_then(Node::as_arr)
                .is_none_or(|a| a.items.is_empty())
        );
    }
    assert_eq!(
        idf::param_str(idf::find(root, "status").unwrap(), "halign"),
        Some("right")
    );
    assert_eq!(
        idf::param_str(idf::find(root, "driver").unwrap(), "halign"),
        Some("start")
    );
    let stock = read_frag(&flow_options, career_row::STOCK, 0).unwrap();
    let flow_frontend = input(&ui_file("flow_frontend.erp"));
    let src = Sources::parse(
        &read_frag(&flow_options, CONTAINER, 1).unwrap(),
        &read_frag(&flow_frontend, FE_CAREER, 0).unwrap(),
        &read_frag(&flow_frontend, FE_HOME, 0).unwrap(),
        &read_frag(&flow_options, CONTAINER, 0).unwrap(),
    )
    .unwrap();
    let (_, page) = pages::build_resume_page(&src, &resume_rows()).unwrap();
    assert_eq!(
        career_row::layout_problems(&page, &bytes, &stock).unwrap(),
        Vec::<String>::new()
    );
    let unbound = career_row::layout_problems(&page, &stock, &stock).unwrap();
    assert!(
        unbound.iter().any(|p| p.starts_with("row_0.driver "))
            && unbound.iter().any(|p| p.starts_with("row_new.status ")),
        "{unbound:?}"
    );
    let report = check::scene_edit(career_row::SCENE, &bytes, catalog::stock()).unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
}

#[test]
#[ignore = "needs SWOFTY_STOCK_DATA, the launcher data folder that holds the game's baseline"]
fn with_no_careers_the_list_holds_the_new_career_row_alone() {
    let input = stock_input();
    let mut settings = setup_settings_without();
    settings.resume_list_on = true;
    let out = std::env::temp_dir().join(format!("swofty-resume-empty-{}", std::process::id()));
    std::fs::remove_dir_all(&out).ok();
    let built = build(
        &settings,
        &input,
        &|_, _| None,
        &out,
        false,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    let name = pages::RESUME_SCREEN;
    let data = std::fs::read(&built.screens[&format!("{UI_DIR}/{name}.ui")]).unwrap();
    let p = Pkg::parse(&data).unwrap();
    let scr = p.root.o().unwrap().at("Screens").unwrap().a().unwrap()[0].clone();
    let rows: Vec<&str> = scr
        .slot(node::CHILDREN)
        .unwrap()
        .iter()
        .filter(|c| c.is_node() && c.name().starts_with("row_"))
        .map(|c| c.name())
        .collect();
    assert_eq!(rows, ["row_new"]);
    let text = String::from_utf8_lossy(&data).into_owned();
    assert!(text.contains("No careers yet") && text.contains(pages::NEW_CAREER_ROUTE));
    assert_resolved_references(&data, &input, &[out.join(ui_file("persistent.erp"))]);
    let persistent = ui_file("persistent.erp");
    let report = check::archive_files(&[(
        persistent.as_str(),
        input(&persistent).as_path(),
        out.join(&persistent).as_path(),
    )])
    .unwrap();
    let hard: Vec<String> = report.hard.iter().map(ToString::to_string).collect();
    assert!(hard.is_empty(), "{hard:?}");
    let after = groupings(&out.join(&persistent), ROUTES);
    assert!(after.contains_key(name) && after["fe_home"]["career"].contains(name));
    let off = build(
        &setup_settings_without(),
        &input,
        &|_, _| None,
        &out,
        false,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(
        !off.screens.keys().any(|k| k.contains(name)),
        "the list is off without the flag"
    );
    std::fs::remove_dir_all(&out).ok();
}

fn setup_settings_without() -> SessionSettings {
    let mut s = setup_settings();
    s.addons[0].career_setup = None;
    s
}

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use swofty_formats::erp::{Erp, Fragment, HEADER_LEN, MAGIC, Resource, STORED, kind};
use swofty_formats::erp::{ErpFile, Packer};
use swofty_formats::pkg::{Node, Pkg};

use super::archives::*;
use super::menu_rows::*;
use super::picker_rows::*;
use super::toggles::*;
use super::*;
use crate::addons::builtin::CORE_ID;
const CACHED_SCREEN: &str = "eaid://ui_package/screens/a.ui";

fn one_screen_archive(path: &Path, screen: &[u8]) {
    use crate::erp::pack::frame_cache;
    use swofty_formats::erp::{Erp, HEADER_LEN, MAGIC};
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&3u32.to_le_bytes());
    let archive = Erp {
        header,
        version: 3,
        resources: vec![frame_cache::zstd_resource(CACHED_SCREEN, "UIDF", screen)],
    }
    .to_bytes()
    .unwrap();
    std::fs::write(path, archive).unwrap();
}

fn screen_bytes(seed: u8, len: u32) -> Vec<u8> {
    (0..len)
        .map(|i| ((i % 251) as u8 ^ (i / 1021) as u8).wrapping_add(seed))
        .collect()
}

fn cached_write(src: &Path, dst: &Path, tail: &'static [u8], archives: &ArchiveWriter) -> Vec<u8> {
    let patch: Patch = Box::new(move |d: &[u8]| Ok([d, tail].concat()));
    write_archive(src, dst, vec![(CACHED_SCREEN, 0, patch)], Vec::new(), archives).unwrap();
    std::fs::read(dst).unwrap()
}

fn real_writes() -> Vec<PathBuf> {
    archives::WRITTEN.with(|w| w.borrow().clone())
}

#[test]
fn a_second_ui_archive_write_the_same_links_the_first_from_the_archive_cache() {
    use crate::cache::archive_entry::kept_archives;
    let dir = std::env::temp_dir().join(format!("swofty-ui-archive-cache-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src.erp");
    one_screen_archive(&src, &screen_bytes(1, 24 << 10));
    let (packer, cache) = (Packer::default(), dir.join("archives"));
    let archives = ArchiveWriter::new(&packer).cached(&cache);
    let first = cached_write(&src, &dir.join("first/a.erp"), b"edited", &archives);
    let second = cached_write(&src, &dir.join("second/a.erp"), b"edited", &archives);
    assert_eq!(second, first);
    assert_eq!(
        real_writes(),
        [dir.join("first/a.erp")],
        "the second write is a link"
    );
    let kept = kept_archives(&cache);
    assert_eq!(kept.len(), 1);
    assert_eq!(std::fs::read(&kept[0]).unwrap(), first);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let inode = |p: &Path| std::fs::metadata(p).unwrap().ino();
        assert_eq!(inode(&dir.join("second/a.erp")), inode(&kept[0]));
    }
    let uncached = cached_write(
        &src,
        &dir.join("uncached/a.erp"),
        b"edited",
        &ArchiveWriter::new(&packer),
    );
    assert_eq!(uncached, first, "the cache changes no byte");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_changed_patch_or_source_makes_a_new_ui_archive_cache_entry() {
    use crate::cache::archive_entry::kept_archives;
    let dir = std::env::temp_dir().join(format!("swofty-ui-archive-cache-new-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src.erp");
    one_screen_archive(&src, &screen_bytes(1, 24 << 10));
    let (packer, cache) = (Packer::default(), dir.join("archives"));
    let archives = ArchiveWriter::new(&packer).cached(&cache);
    let first = cached_write(&src, &dir.join("one/a.erp"), b"edited", &archives);
    let other_patch = cached_write(&src, &dir.join("two/a.erp"), b"edited again", &archives);
    assert_ne!(other_patch, first);
    assert_eq!(real_writes().len(), 2, "another patch is written");
    assert_eq!(kept_archives(&cache).len(), 2);
    one_screen_archive(&src, &screen_bytes(2, 25 << 10));
    let other_source = cached_write(&src, &dir.join("three/a.erp"), b"edited", &archives);
    assert_ne!(other_source, first);
    assert_eq!(real_writes().len(), 3, "another source is written");
    assert_eq!(kept_archives(&cache).len(), 3);
    let again = cached_write(&src, &dir.join("four/a.erp"), b"edited", &archives);
    assert_eq!(again, other_source);
    assert_eq!(real_writes().len(), 3, "and then linked");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_ui_archive_cache_entry_whose_bytes_changed_is_never_linked() {
    use crate::cache::archive_entry::{change_a_byte_in_place, kept_archives};
    let dir = std::env::temp_dir().join(format!("swofty-ui-archive-cache-changed-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src.erp");
    one_screen_archive(&src, &screen_bytes(1, 24 << 10));
    let (packer, cache) = (Packer::default(), dir.join("archives"));
    let archives = ArchiveWriter::new(&packer).cached(&cache);
    let first = cached_write(&src, &dir.join("one/a.erp"), b"edited", &archives);
    let kept = kept_archives(&cache);
    assert_eq!(kept.len(), 1);
    change_a_byte_in_place(&kept[0]);
    let second = cached_write(&src, &dir.join("two/a.erp"), b"edited", &archives);
    assert_eq!(real_writes().len(), 2, "the changed archive is not linked");
    assert_eq!(second, first);
    let third = cached_write(&src, &dir.join("three/a.erp"), b"edited", &archives);
    assert_eq!(real_writes().len(), 2, "the archive written again is");
    assert_eq!(third, first);
    std::fs::remove_dir_all(&dir).ok();
}

use crate::addons::settings::{SessionSettings, UI_DIR};
use node::{NodeExt, ObjExt, arr, node, obj, s};

const MOD: &str = "F1 2012 Season";

fn pkg(root: Node) -> Vec<u8> {
    Pkg {
        name: *b"TEST",
        root,
        vca: Vec::new(),
        layout: None,
    }
    .to_bytes()
    .unwrap()
}

fn ui() -> Vec<u8> {
    pkg(obj(vec![(
        "Templates",
        arr(vec![node("dynamic_era_select", "UIGR", vec![], vec![], &[], &[])]),
    )]))
}

fn scene() -> Vec<u8> {
    pkg(obj(vec![(
        "scenelist",
        arr(vec![obj(vec![(
            "scene",
            obj(vec![("name", s("root")), ("children", arr(vec![]))]),
        )])]),
    )]))
}

fn resource(name: &str, ty: &str, frags: &[&[u8]]) -> Resource {
    let mut res = Resource {
        name: name.into(),
        kind: kind(ty).unwrap(),
        unk: vec![0; 6],
        fragments: frags
            .iter()
            .map(|d| Fragment::new(*b"temp", d.len() as u64, 0, STORED, d.to_vec()))
            .collect(),
        hash: [0; 16],
    };
    res.hash = res.content_hash().unwrap();
    res
}

fn archive(root: &Path, rel: &str, ui: &[u8]) -> PathBuf {
    let resources = vec![
        resource(era::UI, "UIDF", &[b"head", ui]),
        resource(era::SCENE, "UILayout", &[&scene()]),
        resource(era::TITLE_SCENE, "UILayout", &[&scene()]),
    ];
    write(root, rel, resources)
}

fn write(root: &Path, rel: &str, resources: Vec<Resource>) -> PathBuf {
    let mut header = [0u8; HEADER_LEN];
    header[..4].copy_from_slice(MAGIC);
    header[4..8].copy_from_slice(&4u32.to_le_bytes());
    let mut erp = Erp {
        header,
        version: 4,
        resources,
    };
    let path = root.join(ui_file(rel));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, erp.to_bytes().unwrap()).unwrap();
    path
}

#[test]
fn an_addon_era_picker_that_cannot_be_read_is_left_as_it_is() {
    let root = std::env::temp_dir().join(format!("swofty-era-skip-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let full = ui();
    let odd = full[..full.len() / 2].to_vec();
    let why = Pkg::parse(&odd).unwrap_err().to_string();
    let game = root.join("game");
    for rel in era::ARCHIVES {
        archive(&game, rel, &odd);
    }
    let input = |rel: &str| game.join(rel);
    let out = root.join("out");
    let from_mod = |_: &str, res: &str| (res == era::UI).then(|| MOD.to_owned());
    let (mut lines, mut warnings) = (Vec::new(), Vec::new());
    let written = era_rows(
        &input,
        &from_mod,
        &out,
        &mut lines,
        &mut warnings,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(written.is_empty());
    assert!(!out.exists(), "nothing is written");
    assert!(lines.is_empty(), "{lines:?}");
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    for (w, rel) in warnings.iter().zip(era::ARCHIVES) {
        assert!(
            w.starts_with(&format!("ui: {MOD} has its own copy of the era picker in {rel},")),
            "{w}"
        );
        assert!(
            w.contains("the era picker boxes there are off while it's on"),
            "{w}"
        );
        assert!(w.contains(&why), "{w}");
    }

    let stock = |_: &str, _: &str| None;
    let e = era_rows(
        &input,
        &stock,
        &out,
        &mut Vec::new(),
        &mut Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap_err();
    assert!(e.to_string().contains(&why), "{e}");

    let world_only =
        |erp: &str, res: &str| (erp.ends_with(era::ARCHIVES[0]) && res == era::UI).then(|| MOD.to_owned());
    let mut warnings = Vec::new();
    let e = era_rows(
        &input,
        &world_only,
        &out,
        &mut Vec::new(),
        &mut warnings,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap_err();
    assert!(e.to_string().contains(&why), "{e}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains(era::ARCHIVES[0]), "{warnings:?}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_addon_era_picker_without_the_stock_templates_is_left_as_it_is() {
    let root = std::env::temp_dir().join(format!("swofty-era-templates-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("game");
    for rel in era::ARCHIVES {
        archive(&game, rel, &ui());
    }
    let input = |rel: &str| game.join(rel);
    let out = root.join("out");
    let from_mod = |_: &str, _: &str| Some(MOD.to_owned());
    let mut warnings = Vec::new();
    let written = era_rows(
        &input,
        &from_mod,
        &out,
        &mut Vec::new(),
        &mut warnings,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap();
    assert!(written.is_empty());
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings.iter().all(|w| w.contains("time_trial_select not found")),
        "{warnings:?}"
    );
    let stock = |_: &str, _: &str| None;
    let e = era_rows(
        &input,
        &stock,
        &out,
        &mut Vec::new(),
        &mut Vec::new(),
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap_err();
    assert!(e.to_string().contains("time_trial_select not found"), "{e}");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_missing_era_picker_resource_stays_an_error() {
    let root = std::env::temp_dir().join(format!("swofty-era-missing-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let game = root.join("game");
    let mut odd = ui();
    odd.extend_from_slice(&[0; 4]);
    archive(&game, era::ARCHIVES[0], &odd);
    write(
        &game,
        era::ARCHIVES[1],
        vec![resource(era::UI, "UIDF", &[b"head", &odd])],
    );
    let input = |rel: &str| game.join(rel);
    let from_mod = |_: &str, _: &str| Some(MOD.to_owned());
    let mut warnings = Vec::new();
    let e = era_rows(
        &input,
        &from_mod,
        &root.join("out"),
        &mut Vec::new(),
        &mut warnings,
        &ArchiveWriter::new(&Packer::default()),
    )
    .unwrap_err();
    assert!(e.to_string().contains(era::SCENE), "{e}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    std::fs::remove_dir_all(&root).ok();
}
