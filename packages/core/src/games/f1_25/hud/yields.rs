//! `yield` records: a part of the HUD hidden whole, the game's own part and every addon's records on
//! it, while one leaf is on (`docs/reference/ops/hud.md`, "Yield a part").
//!
//! Each part is one or more spots: a scene that gets a null `<prefix>_yield` around the content of one
//! element, and the template nodes that instance the scene, each of which gets a group
//! `<prefix>_yield_<part>` binding the null's `visible` to NOT the leaf. The nulls go in after every
//! other change to the scene, so they hold whatever the records and other addons put there.

use std::collections::{BTreeMap, BTreeSet};

use swofty_formats::erp::Packer;
use swofty_formats::pkg::{Node, Pkg};

use super::model::Yield;
use super::rewrite::Rewrite;
use super::screens;
use super::stock::Stock;
use super::stock_edit::{self, Take};
use super::template::{self, Usage};
use super::{Built, Input, TemplateBase, composer::first, dials, err};
use crate::Result;
use crate::ui::node::{self, NodeExt};

/// A template node that instances a spot's scene: the yield group goes under `parent`, and its node's
/// layout chain is `via` then the null.
#[derive(Debug, Clone, Copy)]
pub struct Hook {
    pub template: &'static str,
    pub parent: &'static str,
    pub via: &'static [&'static str],
}

/// A scene that gets a null around the children of `inside` that `take` picks.
#[derive(Debug, Clone, Copy)]
pub struct Spot {
    pub scene: &'static str,
    pub inside: &'static str,
    pub take: Take,
    pub hooks: &'static [Hook],
}

#[derive(Debug, Clone, Copy)]
pub struct Part {
    pub key: &'static str,
    pub spots: &'static [Spot],
}

pub const FLAG_SCENE: &str =
    "eaid://ui_package/art/osd/components/message_system/race_condition_flag_message.idf";
pub const NOTIFICATIONS_SCENE: &str =
    "eaid://ui_package/art/osd/components/message_system/message_system.idf";
pub const TIMER_SCENE: &str = "eaid://ui_package/art/osd/components/session_timer/q_session_timer.idf";
pub const MINIMAP_SCENE: &str = "eaid://ui_package/art/osd/components/minimap/minimap_combined.idf";
const MINI_MAP_SCENE: &str = "eaid://ui_package/art/osd/components/minimap/minimap.idf";
const TRACK_MAP_SCENE: &str = "eaid://ui_package/art/osd/components/full_track_map/full_track_map.idf";
const RACE_CONTROL_SCENE: &str =
    "eaid://ui_package/art/osd/components/spectator_mode/spec_race_control_message.idf";
const FLAG_PARENT: &str =
    "hud_left_notification_dynamic.osd_visibility_option.left_notification.left_message";
const FLAG_PARENT_P2: &str =
    "hud_left_notification_dynamic_p2.osd_visibility_option.left_notification.left_message";
const NOTIFICATIONS_PARENT: &str =
    "hud_notifications_dynamic.osd_visibility_option.hud_notifications_dynamic";
const NOTIFICATIONS_PARENT_P2: &str =
    "hud_notifications_dynamic_p2.osd_visibility_option.hud_notifications_dynamic_p2";
const TIMER_PARENT: &str = "hud_session_timer_dynamic.session_timer";
const MINIMAP_PARENT: &str = "hud_dynamic_minimap.osd_visibility_option.map";
const REPLAY_CONTROLS: &str = "replay_interface";
const SPECT_FLAG_VIA: [&str; 2] = [screens::SPECT_PREFIX, "race_condition_flag_message"];
const SPECT_RACE_CONTROL_VIA: [&str; 2] = [screens::SPECT_PREFIX, "race_control_message"];
const SPECT_MINI_MAP_VIA: [&str; 2] = [screens::SPECT_PREFIX, "mini_map"];
const SPECT_TRACK_MAP_VIA: [&str; 2] = [screens::SPECT_PREFIX, "track_map"];
const SPECT_TABLE_VIA: [&str; 2] = [screens::SPECT_PREFIX, screens::SPECT_TABLE_EL];
const SPECT_SCREEN_VIA: [&str; 1] = [screens::SPECT_PREFIX];

const fn race(parent: &'static str) -> Hook {
    Hook {
        template: dials::TEMPLATE,
        parent,
        via: &[],
    }
}

/// A node of the spectator screen's template that reaches the scene through `via`.
const fn spectator(via: &'static [&'static str]) -> Hook {
    Hook {
        template: screens::SPECT_UI,
        parent: screens::SPECT_PARENT,
        via,
    }
}

/// A spot around everything in the root of a component scene the spectator screen instances.
const fn spectator_component(scene: &'static str, hooks: &'static [Hook]) -> Spot {
    Spot {
        scene,
        inside: screens::SCENE_ROOT,
        take: Take::All,
        hooks,
    }
}

const fn dial(facts: &'static dials::DialFacts) -> Spot {
    Spot {
        scene: facts.scene,
        inside: dials::CUSTOMISATION,
        take: Take::All,
        hooks: &[],
    }
}

const F126_HOOKS: [Hook; 2] = [
    race(dials::F126.race_parent),
    Hook {
        template: dials::SPECTATOR,
        parent: dials::F126.spectator_parent,
        via: &[],
    },
];
const F125_HOOKS: [Hook; 2] = [
    race(dials::F125.race_parent),
    Hook {
        template: dials::SPECTATOR,
        parent: dials::F125.spectator_parent,
        via: &[],
    },
];
const F2_HOOKS: [Hook; 2] = [
    race(dials::F2.race_parent),
    Hook {
        template: dials::SPECTATOR,
        parent: dials::F2.spectator_parent,
        via: &[],
    },
];

pub const PARTS: &[Part] = &[
    Part {
        key: "f1_26",
        spots: &[Spot {
            hooks: &F126_HOOKS,
            ..dial(&dials::F126)
        }],
    },
    Part {
        key: "f1_25",
        spots: &[Spot {
            hooks: &F125_HOOKS,
            ..dial(&dials::F125)
        }],
    },
    Part {
        key: "f2",
        spots: &[Spot {
            hooks: &F2_HOOKS,
            ..dial(&dials::F2)
        }],
    },
    Part {
        key: "standings",
        spots: &[Spot {
            scene: screens::STACKED,
            inside: screens::SCENE_ROOT,
            take: Take::All,
            hooks: &[race(screens::STANDINGS_PARENT)],
        }],
    },
    Part {
        key: "replay",
        spots: &[Spot {
            scene: screens::REPLAY_SCENE,
            inside: screens::REPLAY_GROUP,
            take: Take::Except(&[REPLAY_CONTROLS]),
            hooks: &[Hook {
                template: screens::REPLAY_UI,
                parent: screens::REPLAY_ROOT,
                via: &[],
            }],
        }],
    },
    Part {
        key: "spectator",
        spots: &[
            spectator_component(screens::SPECT_TABLE, &[spectator(&SPECT_TABLE_VIA)]),
            Spot {
                scene: screens::SPECT_SCREEN,
                inside: screens::SCENE_ROOT,
                take: Take::Only(&[screens::SPECT_HEADER_EL]),
                hooks: &[spectator(&SPECT_SCREEN_VIA)],
            },
        ],
    },
    Part {
        key: "lap_counter",
        spots: &[Spot {
            scene: screens::LAPC_SCENE,
            inside: screens::SCENE_ROOT,
            take: Take::All,
            hooks: &[race(screens::LAPC_PARENT)],
        }],
    },
    Part {
        key: "flag_box",
        spots: &[Spot {
            scene: FLAG_SCENE,
            inside: "boundless",
            take: Take::All,
            hooks: &[
                race(FLAG_PARENT),
                race(FLAG_PARENT_P2),
                spectator(&SPECT_FLAG_VIA),
            ],
        }],
    },
    Part {
        key: "notifications",
        spots: &[
            Spot {
                scene: NOTIFICATIONS_SCENE,
                inside: screens::SCENE_ROOT,
                take: Take::All,
                hooks: &[race(NOTIFICATIONS_PARENT), race(NOTIFICATIONS_PARENT_P2)],
            },
            spectator_component(RACE_CONTROL_SCENE, &[spectator(&SPECT_RACE_CONTROL_VIA)]),
        ],
    },
    Part {
        key: "session_timer",
        spots: &[Spot {
            scene: TIMER_SCENE,
            inside: "boundless",
            take: Take::All,
            hooks: &[race(TIMER_PARENT)],
        }],
    },
    Part {
        key: "minimap",
        spots: &[
            Spot {
                scene: MINIMAP_SCENE,
                inside: screens::SCENE_ROOT,
                take: Take::All,
                hooks: &[race(MINIMAP_PARENT)],
            },
            spectator_component(MINI_MAP_SCENE, &[spectator(&SPECT_MINI_MAP_VIA)]),
            spectator_component(TRACK_MAP_SCENE, &[spectator(&SPECT_TRACK_MAP_VIA)]),
        ],
    },
];

pub const KEYS: [&str; 11] = [
    "f1_26",
    "f1_25",
    "f2",
    "standings",
    "replay",
    "spectator",
    "lap_counter",
    "flag_box",
    "notifications",
    "session_timer",
    "minimap",
];

pub fn part(key: &str) -> Option<&'static Part> {
    PARTS.iter().find(|p| p.key == key)
}

pub fn null_name(pfx: &str) -> String {
    format!("{pfx}_yield")
}

pub fn group_name(pfx: &str, part: &str) -> String {
    format!("{pfx}_yield_{part}")
}

/// The name of the node in a part's group for the part's spot `i`.
pub fn node_name(i: usize) -> String {
    match i {
        0 => "v_yield".to_owned(),
        i => format!("v_yield_{i}"),
    }
}

/// The archive a resource is read from when no other step of the composer wrote it.
pub fn stock_erp(resource: &str) -> &'static str {
    match resource {
        screens::REPLAY_UI | screens::REPLAY_SCENE => screens::FLOW_REPLAY,
        _ => dials::STOCK_ERP,
    }
}

/// The parts a record yields, each once.
fn parts_of(y: &Yield) -> Vec<&'static Part> {
    let mut seen = BTreeSet::new();
    y.parts
        .iter()
        .filter(|p| seen.insert(p.as_str()))
        .filter_map(|p| part(p))
        .collect()
}

/// The scenes and templates the yield records among `inputs` write.
pub fn written(inputs: &[Input]) -> Vec<&'static str> {
    let mut out = BTreeSet::new();
    for y in inputs.iter().filter_map(|i| i.record.yields.as_ref()) {
        for p in parts_of(y) {
            for s in p.spots {
                out.insert(s.scene);
                out.extend(s.hooks.iter().map(|h| h.template));
            }
        }
    }
    out.into_iter().collect()
}

enum Edit<'a> {
    Wrap {
        spot: &'a Spot,
        null: String,
    },
    Hook {
        hook: &'a Hook,
        scene: &'static str,
        group: String,
        node: String,
        null: String,
        db: String,
        negate: bool,
    },
}

/// Every edit the yield records make, by resource, in the order of their prefixes.
fn edits(yields: &[(String, &Input)]) -> Result<BTreeMap<&'static str, Vec<Edit<'static>>>> {
    let mut sorted: Vec<&(String, &Input)> = yields.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: BTreeMap<&'static str, Vec<Edit>> = BTreeMap::new();
    for (pfx, input) in sorted {
        let Some(y) = &input.record.yields else { continue };
        let r = template::resolve(&y.while_, Usage::Visible, "yield while")
            .map_err(|e| err(format!("{}/{}: {e}", input.addon, input.key)))?;
        for p in parts_of(y) {
            for (i, spot) in p.spots.iter().enumerate() {
                out.entry(spot.scene).or_default().push(Edit::Wrap {
                    spot,
                    null: null_name(pfx),
                });
                for hook in spot.hooks {
                    out.entry(hook.template).or_default().push(Edit::Hook {
                        hook,
                        scene: spot.scene,
                        group: group_name(pfx, p.key),
                        node: node_name(i),
                        null: null_name(pfx),
                        db: r.db.clone(),
                        negate: r.negate,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// The nulls each record's wrap made, by the record's null name and the scene.
type Nulls = BTreeMap<(String, &'static str), Vec<String>>;

fn wrap_scene(bytes: Vec<u8>, list: &[Edit], donor: &Node, nulls: &mut Nulls) -> Result<Vec<u8>> {
    let mut out = bytes;
    for e in list {
        if let Edit::Wrap { spot, null } = e {
            let (b, names) =
                stock_edit::wrap_children(&out, spot.scene, spot.inside, spot.take, null, Some(donor))?;
            out = b;
            nulls.insert((null.clone(), spot.scene), names);
        }
    }
    Ok(out)
}

fn hook_template(bytes: Vec<u8>, list: &[Edit], nulls: &Nulls) -> Result<Vec<u8>> {
    let mut pkg = Pkg::parse(&bytes)?;
    for e in list {
        let Edit::Hook {
            hook,
            scene,
            group,
            node: name,
            null,
            db,
            negate,
        } = e
        else {
            continue;
        };
        for (k, made) in nulls[&(null.clone(), *scene)].iter().enumerate() {
            let name = match k {
                0 => name.clone(),
                k => format!("{name}_{}", k + 1),
            };
            add_hook(&mut pkg, hook, group, &name, made, db, *negate)?;
        }
    }
    Ok(pkg.to_bytes()?)
}

fn add_hook(
    pkg: &mut Pkg,
    hook: &Hook,
    group: &str,
    name: &str,
    null: &str,
    db: &str,
    negate: bool,
) -> Result<()> {
    let mut chain: Vec<&str> = hook.via.to_vec();
    chain.push(null);
    let v = node::group_node(
        name,
        vec![("visible", node::bool_prop(true, Some(db), !negate))],
        &chain,
        vec![],
    );
    {
        let at = node::find_path(&pkg.root, hook.parent)?;
        let target = node::node_at_mut(&mut pkg.root, &at)?;
        let kids = target.slot_mut(node::CHILDREN)?;
        let mut grp = match kids.iter().position(|c| c.is_node() && c.name() == group) {
            Some(i) => kids.remove(i),
            None => node::group_node(group, vec![], &[], vec![]),
        };
        if grp
            .slot(node::CHILDREN)?
            .iter()
            .any(|c| c.is_node() && c.name() == name)
        {
            return Err(err(format!("{}.{group} already has a {name}", hook.parent)));
        }
        grp.slot_mut(node::CHILDREN)?.push(v);
        node::fix_ids(&mut grp, Some(hook.parent));
        kids.push(grp);
    }
    Ok(())
}

/// Adds the yield records' nulls and template groups to `archives`: in place where the composer
/// already writes the resource (every copy alike), else in new archives `hud_yield.erp`,
/// `hud_yield_2.erp` … over every stock archive that carries it, read from `stock` with `base`
/// applied to the HUD template.
pub fn apply(
    stock: &dyn Stock,
    yields: &[(String, &Input)],
    base: Option<TemplateBase>,
    archives: &mut Vec<Built>,
    lines: &mut Vec<String>,
    packer: &Packer,
) -> Result<()> {
    let edits = edits(yields)?;
    if edits.is_empty() {
        return Ok(());
    }
    let donor = stock_edit::plain_null(&first(
        stock.read(dials::STOCK_ERP, screens::LAPC_SCENE)?,
        screens::LAPC_SCENE,
    )?)?;
    let mut rw = Rewrite::new(stock, archives)?;
    let mut nulls = Nulls::new();
    let scenes_first = edits
        .iter()
        .filter(|(r, _)| r.ends_with(".idf"))
        .chain(edits.iter().filter(|(r, _)| !r.ends_with(".idf")));
    for (resource, list) in scenes_first {
        let base = base.filter(|_| *resource == dials::TEMPLATE);
        let bytes = rw.read(resource, stock_erp(resource), base)?.swap_remove(0);
        let new = match resource.ends_with(".idf") {
            true => wrap_scene(bytes, list, &donor, &mut nulls)?,
            false => hook_template(bytes, list, &nulls)?,
        };
        rw.write(resource, new)?;
    }
    let added = rw.finish(archives, "hud_yield", "the yield records", lines, packer)?;
    archives.extend(added);
    for (pfx, input) in yields {
        if let Some(y) = &input.record.yields
            && !y.parts.is_empty()
        {
            lines.push(format!(
                "{pfx}: yields {} while {} is on",
                parts_of(y).iter().map(|p| p.key).collect::<Vec<_>>().join(", "),
                y.while_
                    .view_model
                    .as_deref()
                    .or(y.while_.path.as_deref())
                    .unwrap_or_default()
            ));
        }
    }
    Ok(())
}
