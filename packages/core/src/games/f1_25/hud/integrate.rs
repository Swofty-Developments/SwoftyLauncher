//! How the composer sits in a profile build: it runs last among the archive changes, starts from the
//! resources as the other addons and the launcher leave them, and its overlay replaces those
//! resources with the same content plus its nodes.

use std::collections::BTreeSet;
use std::path::PathBuf;

use swofty_addon::asset::{AssetOp, Op, Origin};
use swofty_addon::ops::ErpOp;
use swofty_formats::erp::{ErpFile, Packer};
use swofty_formats::pkg::Pkg;

use super::conflict::{self, Conflict, Kind};
use super::{Input, Layered, Output, Stock, TemplateBase, compose_with, dials, screen_build, screens, walk};
use crate::erp::ErpBuild;
use crate::store::Store;
use crate::{Error, Result};

/// The display line of the composer's origin in conflict reports and the runtime layering.
pub const ORIGIN: &str = "HUD composer";

pub struct Context<'a> {
    pub store: &'a Store,
    pub stock: &'a dyn Stock,
    pub base: &'a dyn Fn(&str) -> PathBuf,
    pub baseline: &'a dyn Fn(&str) -> PathBuf,
    pub puts: &'a [String],
    pub name: &'a dyn Fn(&str) -> String,
    /// The build's packer, which reuses the compressed frames of earlier builds.
    pub packer: &'a Packer,
}

pub struct Composed {
    pub output: Output,
    pub ops: Vec<Op>,
    pub lines: Vec<String>,
    pub warnings: Vec<String>,
    /// The warning that HUD customisation closes the game, also in `warnings`.
    pub editor: Option<String>,
}

/// The archives screen records write beyond the dials' (`hud/screens.rs`).
pub const SCREEN_ERPS: [&str; 4] = [
    screens::FLOW_REPLAY,
    screens::FLOW_FRONTEND,
    screens::REPLAY_SCREEN_ERP,
    "2025_asset_groups/ui_package/flow_options.erp",
];

/// The resources the composer writes that `op` changes; empty when it changes none of them.
fn touched(op: &ErpOp, written: &BTreeSet<&str>, source: &dyn Fn() -> Option<PathBuf>) -> Vec<String> {
    let ours = |erps: &[&str]| {
        erps.iter()
            .any(|e| dials::SCENE_ERPS.contains(e) || SCREEN_ERPS.contains(e))
    };
    let one = |erp: &str, name: &str| match ours(&[erp]) && written.contains(name) {
        true => vec![name.to_owned()],
        false => Vec::new(),
    };
    match op {
        ErpOp::PutResource {
            erp,
            resource,
            rename,
            ..
        }
        | ErpOp::CopyResource {
            erp,
            resource,
            rename,
            ..
        } => one(erp, rename.as_deref().unwrap_or(resource)),
        ErpOp::ReplaceResource { erp, resource, .. }
        | ErpOp::RemoveResource { erp, resource }
        | ErpOp::AddResource { erp, resource, .. }
        | ErpOp::PatchPkg { erp, resource, .. } => one(erp, resource),
        ErpOp::Overlay { erps, .. } => {
            let erps: Vec<&str> = erps.iter().map(String::as_str).collect();
            if !ours(&erps) {
                return Vec::new();
            }
            source()
                .and_then(|p| crate::erp::overlay_names(&p).ok())
                .map(|names| {
                    names
                        .into_iter()
                        .filter(|n| written.contains(n.as_str()))
                        .collect()
                })
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn or_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

/// The warning for a standings board that races fit and HUD customisation's tree does not: every
/// addon on the board, and the ones whose removal alone brings the tree inside the limit.
pub fn editor_text(name: &dyn Fn(&str) -> String, e: &walk::EditorOver) -> String {
    let ours: Vec<String> = e.addons.iter().map(|a| name(a)).collect();
    let off: Vec<String> = e.off.iter().map(|a| name(a)).collect();
    let what = match off.is_empty() {
        true => format!("some of {}", and_list(&ours)),
        false => or_list(&off),
    };
    format!(
        "{} {} {} too big for HUD customisation (OSD Customization in the pause menu): about {} of the {} slots one screen can hold, and the game closes when it opens. Races show the list as before. Turn {what} off in this profile before you open HUD customisation (it holds the {}-slot host scene {} times)",
        and_list(&ours),
        match ours.len() > 1 {
            true => "together make",
            false => "makes",
        },
        screens::board_of("standings"),
        e.board.editor(),
        walk::SLOT_LIMIT,
        e.board.host,
        walk::EDITOR_BOARDS * e.board.instances()
    )
}

/// One archive change of another addon that the composer builds on.
struct Change<'a> {
    resource: String,
    who: String,
    op: &'a ErpOp,
    addon: &'a str,
}

/// The copy of `resource` an archive change carries, when it carries it whole.
fn copy_of(cx: &Context, c: &Change, resource: &str) -> Option<Pkg> {
    let resolve = |s: &swofty_addon::asset::Source| {
        s.resolve(&crate::build::source_dir(cx.store, c.addon, s.root))
            .ok()
    };
    let from_archive = |path: PathBuf| -> Option<Vec<u8>> {
        let file = ErpFile::open(&path).ok()?;
        let res = file.erp.find(resource).ok()?;
        file.load(res).ok()?.fragments.first()?.data().ok()
    };
    let bytes = match c.op {
        ErpOp::Overlay { source, .. } | ErpOp::PutResource { source, .. } => from_archive(resolve(source)?)?,
        ErpOp::ReplaceResource {
            source, fragment: 0, ..
        } => std::fs::read(resolve(source)?).ok()?,
        _ => return None,
    };
    Pkg::parse(&bytes).ok()
}

/// The names of the addons whose archive changes put the other claim of `c` there: the ones whose
/// copy of the resource holds the conflict's template node, or else every addon that changed it.
fn theirs(cx: &Context, c: &Conflict, changes: &[Change], ours: &[String]) -> Vec<String> {
    let on: Vec<&Change> = changes
        .iter()
        .filter(|ch| ch.resource == c.resource && !ours.contains(&ch.who))
        .collect();
    let holding: Vec<&Change> = match c.node() {
        Some(path) => on
            .iter()
            .copied()
            .filter(|ch| copy_of(cx, ch, &c.resource).is_some_and(|p| conflict::has_node(&p, path)))
            .collect(),
        None => Vec::new(),
    };
    let pick = if holding.is_empty() { on } else { holding };
    conflict::addons_of(pick.iter().map(|ch| ch.who.as_str()))
}

/// The refusal of a [`Conflict`], naming the addons whose records claim the part and the addons whose
/// archive changes put the other claim there.
fn conflict_text(cx: &Context, c: &Conflict, changes: &[Change]) -> String {
    let ours: Vec<String> = c.addons.iter().map(|a| (cx.name)(a)).collect();
    let ours = match ours.is_empty() {
        true => vec!["the composed HUD".to_owned()],
        false => ours,
    };
    if let Kind::Walk { off, .. } = &c.kind {
        let off: Vec<String> = off.iter().map(|a| (cx.name)(a)).collect();
        let what = match off.is_empty() {
            true => format!("some of {}", and_list(&ours)),
            false => or_list(&off),
        };
        return format!(
            "the HUD elements do not compose: {}; turn {what} off in this profile ({})",
            c.text(&and_list(&ours), ""),
            c.detail("")
        );
    }
    let theirs = theirs(cx, c, changes, &ours);
    if theirs.is_empty() {
        return format!(
            "the HUD elements do not compose: {}; turn {} off in this profile and use Send Logs ({})",
            c.text(&and_list(&ours), "another addon"),
            or_list(&ours),
            c.detail("another addon")
        );
    }
    let both: Vec<String> = theirs.iter().chain(&ours).cloned().collect();
    format!(
        "the HUD elements do not compose: {}; turn {} off in this profile ({})",
        c.text(&and_list(&ours), &and_list(&theirs)),
        or_list(&both),
        c.detail(&and_list(&theirs))
    )
}

/// Composes `inputs` for a build: the archive changes in `erp_ops` that touch what the composer writes
/// are applied first to a scratch build, and the composer reads its resources from that. `template`
/// edits the template the composer starts from.
pub fn compose_in_build(
    cx: &Context,
    erp_ops: &[Op],
    inputs: &[Input],
    template: Option<TemplateBase>,
) -> Result<Composed> {
    let shared = screen_build::shared_scenes(cx.stock, inputs)?;
    let mut written: BTreeSet<&str> = dials::written().into_iter().collect();
    written.extend(inputs.iter().filter_map(|i| i.record.scene.as_deref()));
    written.extend(
        inputs
            .iter()
            .flat_map(|i| i.record.stock_nodes.iter())
            .filter(|s| !s.wrap.is_empty())
            .filter_map(|s| s.scene.as_deref()),
    );
    written.extend(shared.iter().map(String::as_str));
    written.extend(super::yields::written(inputs));
    if inputs.iter().any(|i| i.record.screen.is_some()) {
        written.extend(screen_build::written());
    }
    let mut scratch = ErpBuild::new(cx.base)
        .with_baseline(cx.baseline)
        .with_packer(cx.packer.clone());
    let mut lines = Vec::new();
    let mut warnings = Vec::new();
    let huds: Vec<String> = {
        let mut v: Vec<String> = inputs.iter().map(|i| (cx.name)(&i.addon)).collect();
        v.dedup();
        v.sort();
        v.dedup();
        v
    };
    for rel in cx
        .puts
        .iter()
        .filter(|r| dials::SCENE_ERPS.contains(&r.as_str()) || SCREEN_ERPS.contains(&r.as_str()))
    {
        scratch.load(rel)?;
        lines.push(format!(
            "{rel} is replaced whole by an addon; the composed overlay starts from that archive's resources"
        ));
    }
    let mut changes: Vec<Change> = Vec::new();
    for op in erp_ops {
        let AssetOp::Erp(e) = &op.op else { continue };
        let origin = op.origin.to_string();
        let resolve = |s: &swofty_addon::asset::Source| {
            Ok(s.resolve(&crate::build::source_dir(cx.store, &op.origin.addon, s.root))?)
        };
        let path = || match e {
            ErpOp::Overlay { source, .. } => resolve(source).ok(),
            _ => None,
        };
        let names = touched(e, &written, &path);
        if names.is_empty() {
            continue;
        }
        match scratch.apply_for(e, &origin, &resolve) {
            Ok(()) => {
                for resource in names {
                    changes.push(Change {
                        resource,
                        who: (cx.name)(&op.origin.addon),
                        op: e,
                        addon: &op.origin.addon,
                    });
                }
                lines.push(format!(
                    "{} changes a resource the composer writes; the composed overlay is built on top of that change",
                    (cx.name)(&origin)
                ))
            }
            Err(err) => warnings.push(format!(
                "{}: its change to a HUD resource cannot be applied under {}'s composed HUD ({err}); the composed overlay replaces that resource with a copy of the game's own",
                (cx.name)(&origin),
                huds.join(", ")
            )),
        }
    }
    let layered = Layered {
        inner: cx.stock,
        build: &scratch,
    };
    let output = compose_with(&layered, inputs, template, cx.packer).map_err(|e| match e {
        Error::HudConflict(c) => Error::Other(conflict_text(cx, &c, &changes)),
        e => Error::Other(format!("the HUD elements do not compose: {e}")),
    })?;
    let left = super::check::uncovered(&layered, &output.archives)?;
    if !left.is_empty() {
        return Err(Error::Other(format!(
            "the composed HUD of {} would leave the game's own copy of a resource beside the composed one, and the game crashes loading the HUD when it mixes them: {}. Turn {} off in this profile and use Send Logs.",
            huds.join(", "),
            left.join("; "),
            huds.join(", ")
        )));
    }
    let rotations = super::check::constant_rotations(&output.archives)?;
    if !rotations.is_empty() {
        return Err(Error::Other(format!(
            "the composed HUD of {} has clips with a one-key rotation the game can read misaligned and crash on: {}",
            huds.join(", "),
            rotations.join("; ")
        )));
    }
    let unpadded = super::check::unpadded_clips(&output.archives)?;
    if !unpadded.is_empty() {
        return Err(Error::Other(format!(
            "the composed HUD of {} has clips whose key data the game can place misaligned and crash on: {}",
            huds.join(", "),
            unpadded.join("; ")
        )));
    }
    let owner = inputs.first().map(|i| i.addon.clone()).unwrap_or_default();
    let ops = output
        .ops("hud")
        .into_iter()
        .map(|op| Op {
            op: AssetOp::Erp(op),
            origin: Origin {
                addon: owner.clone(),
                item: Some(ORIGIN.into()),
            },
        })
        .collect();
    lines.extend(output.lines.iter().cloned());
    let editor = output.editor.as_ref().map(|e| editor_text(cx.name, e));
    warnings.extend(editor.clone());
    Ok(Composed {
        output,
        ops,
        lines,
        warnings,
        editor,
    })
}

#[cfg(test)]
mod tests {
    use swofty_addon::asset::{Root, Source};
    use swofty_formats::pkg::{Node, Obj, Val};

    use super::*;
    use crate::hud::GameStock;
    use crate::hud::conflict::{Kind, Setting};
    use crate::ui::node;

    const HALO: &str = "nick.f1-2026-halo";
    const PLANKED: &str = "d.planked";
    const TOWER: &str = "pablo.tv-timing-tower";
    const S10: &str = "swofty.swofty-seasons-2010";
    const S11: &str = "swofty.swofty-seasons-2011";
    const PARENT: &str = "hud_track_position.osd_visibility_option.track_position";

    fn name(id: &str) -> String {
        match id {
            HALO => "Project Halo".into(),
            PLANKED => "Planked".into(),
            TOWER => "TV Timing Tower".into(),
            S10 => "SWOFTY 2010".into(),
            S11 => "SWOFTY 2011".into(),
            other => other.into(),
        }
    }

    fn template(extra: Vec<Node>) -> Vec<u8> {
        let t = node::group_node(
            "hud_track_position",
            vec![],
            &[],
            vec![node::group_node(
                "osd_visibility_option",
                vec![],
                &[],
                vec![node::group_node("track_position", vec![], &[], extra)],
            )],
        );
        Pkg {
            name: *b"UIDF",
            root: Node::Obj(Obj::new(vec![(Val::str("Templates"), node::arr(vec![t]))])),
            vca: vec![],
            layout: None,
        }
        .to_bytes()
        .unwrap()
    }

    fn replace(path: &str) -> ErpOp {
        ErpOp::ReplaceResource {
            erp: screens::FLOW_RACE.into(),
            resource: dials::TEMPLATE.into(),
            fragment: 0,
            source: Source {
                root: Root::Addon,
                path: path.into(),
            },
        }
    }

    fn text(store: &Store, c: &Conflict, changes: &[Change]) -> String {
        let stock = GameStock::new(store.root.join("game"));
        let base = |rel: &str| store.root.join(rel);
        let packer = Packer::default();
        let cx = Context {
            store,
            packer: &packer,
            stock: &stock,
            base: &base,
            baseline: &base,
            puts: &[],
            name: &name,
        };
        conflict_text(&cx, c, changes)
    }

    fn store(tag: &str) -> Store {
        let root = std::env::temp_dir().join(format!("swofty-hud-conflict-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        Store::new(root)
    }

    #[test]
    fn a_preview_conflict_names_both_addons_and_what_to_turn_off() {
        let s = store("preview");
        let op = replace("uipre.bin");
        let changes = [Change {
            resource: screens::STACKED_UIPRE.into(),
            who: name(HALO),
            op: &op,
            addon: HALO,
        }];
        let c = Conflict {
            resource: screens::STACKED_UIPRE.into(),
            addons: vec![TOWER.into()],
            kind: Kind::Preview {
                element: "mask".into(),
                have: Setting::Visible(true),
                want: Setting::Visible(false),
            },
        };
        assert_eq!(
            text(&s, &c, &changes),
            "the HUD elements do not compose: Project Halo and TV Timing Tower set the HUD editor preview of the Track Position list differently: Project Halo shows mask, TV Timing Tower hides mask; turn Project Halo or TV Timing Tower off in this profile (driver_position_table_stacked.uipre from Project Halo)"
        );
        std::fs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn a_board_conflict_names_the_addon_whose_copy_holds_the_node() {
        let s = store("board");
        let halo = s.addons_dir().join(HALO).join("t.ui");
        let planked = s.addons_dir().join(PLANKED).join("t.ui");
        for (p, extra) in [
            (
                &halo,
                vec![node::group_node(
                    "tt_ctl",
                    vec![],
                    &[],
                    vec![node::hidden("tt_mask", &["mask"])],
                )],
            ),
            (&planked, vec![]),
        ] {
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, template(extra)).unwrap();
        }
        let op = replace("t.ui");
        let changes = [
            Change {
                resource: dials::TEMPLATE.into(),
                who: name(PLANKED),
                op: &op,
                addon: PLANKED,
            },
            Change {
                resource: dials::TEMPLATE.into(),
                who: name(HALO),
                op: &op,
                addon: HALO,
            },
        ];
        let c = Conflict {
            resource: dials::TEMPLATE.into(),
            addons: vec![TOWER.into()],
            kind: Kind::Board {
                screen: "standings".into(),
                element: "mask".into(),
                node: format!("{PARENT}.tt_ctl.tt_mask"),
            },
        };
        assert_eq!(
            text(&s, &c, &changes),
            format!(
                "the HUD elements do not compose: Project Halo and TV Timing Tower both replace the Track Position list, and only one of them can switch the game's own list off; turn Project Halo or TV Timing Tower off in this profile (Project Halo's template node {PARENT}.tt_ctl.tt_mask already shows and hides mask)"
            )
        );
        std::fs::remove_dir_all(&s.root).ok();
    }

    #[test]
    fn a_conflict_with_no_known_other_side_asks_for_logs() {
        let s = store("unknown");
        let c = Conflict {
            resource: screens::STACKED_UIPRE.into(),
            addons: vec![TOWER.into()],
            kind: Kind::Preview {
                element: "mask".into(),
                have: Setting::Visible(true),
                want: Setting::Visible(false),
            },
        };
        assert_eq!(
            text(&s, &c, &[]),
            "the HUD elements do not compose: another addon and TV Timing Tower set the HUD editor preview of the Track Position list differently: another addon shows mask, TV Timing Tower hides mask; turn TV Timing Tower off in this profile and use Send Logs (driver_position_table_stacked.uipre from another addon)"
        );
    }

    #[test]
    fn a_board_too_big_for_the_ui_tree_names_every_addon_on_it_and_which_to_turn_off() {
        let s = store("walk");
        let walk = |off: Vec<String>| Conflict {
            resource: screens::CAP.into(),
            addons: vec![S10.into(), S11.into(), TOWER.into()],
            kind: Kind::Walk {
                screen: "standings".into(),
                host: 14_101,
                instances: 4,
                slots: 73_904,
                off,
            },
        };
        let all = walk(vec![S10.into(), S11.into(), TOWER.into()]);
        assert_eq!(
            text(&s, &all, &[]),
            "the HUD elements do not compose: SWOFTY 2010, SWOFTY 2011 and TV Timing Tower together make the Track Position list too big for the game: about 73904 of the 65536 slots one screen can hold, and the game can close as soon as it shows it; turn SWOFTY 2010, SWOFTY 2011 or TV Timing Tower off in this profile (the board holds the 14101-slot host scene 4 times)"
        );
        let none = walk(vec![]);
        assert!(
            text(&s, &none, &[])
                .contains("; turn some of SWOFTY 2010, SWOFTY 2011 and TV Timing Tower off in this profile"),
            "{}",
            text(&s, &none, &[])
        );
    }

    #[test]
    fn a_board_too_big_for_hud_customisation_warns_with_every_addon_on_it_and_which_to_turn_off() {
        let editor = |addons: Vec<&str>, off: Vec<&str>, host, records| walk::EditorOver {
            addons: addons.into_iter().map(String::from).collect(),
            off: off.into_iter().map(String::from).collect(),
            board: walk::Board { host, records },
        };
        let three = editor(vec![S10, S11, TOWER], vec![S10, S11, TOWER], 7_299, 3);
        assert_eq!(
            editor_text(&name, &three),
            "SWOFTY 2010, SWOFTY 2011 and TV Timing Tower together make the Track Position list too big for HUD customisation (OSD Customization in the pause menu): about 88404 of the 65536 slots one screen can hold, and the game closes when it opens. Races show the list as before. Turn SWOFTY 2010, SWOFTY 2011 or TV Timing Tower off in this profile before you open HUD customisation (it holds the 7299-slot host scene 12 times)"
        );
        let one = editor(vec![TOWER], vec![TOWER], 10_915, 1);
        assert!(
            editor_text(&name, &one).starts_with(
                "TV Timing Tower makes the Track Position list too big for HUD customisation (OSD Customization in the pause menu): about 66306 of"
            ),
            "{}",
            editor_text(&name, &one)
        );
        let none = editor(vec![S10, TOWER], vec![], 20_000, 2);
        assert!(
            editor_text(&name, &none)
                .contains("Turn some of SWOFTY 2010 and TV Timing Tower off in this profile"),
            "{}",
            editor_text(&name, &none)
        );
    }

    #[test]
    fn a_full_anim_string_table_names_the_addon_whose_copy_filled_it() {
        let s = store("clips");
        let op = replace("dial.bin");
        let changes = [Change {
            resource: dials::F125.strtab.into(),
            who: name(HALO),
            op: &op,
            addon: HALO,
        }];
        let c = Conflict {
            resource: dials::F125.strtab.into(),
            addons: vec![S10.into()],
            kind: Kind::Clips {
                scene: dials::F125.scene.into(),
                last: "zz_~".into(),
            },
        };
        assert_eq!(
            text(&s, &c, &changes),
            "the HUD elements do not compose: Project Halo and SWOFTY 2010 both animate the dial, and Project Halo's animation names leave none after them for SWOFTY 2010; turn Project Halo or SWOFTY 2010 off in this profile (Project Halo's anim string table ends at zz_~)"
        );
    }

    #[test]
    fn lists_read_as_english() {
        let v = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(and_list(&v(&["A"])), "A");
        assert_eq!(and_list(&v(&["A", "B", "C"])), "A, B and C");
        assert_eq!(or_list(&v(&["A", "B"])), "A or B");
    }
}
