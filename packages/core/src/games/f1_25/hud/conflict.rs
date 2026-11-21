//! A part of the HUD that the records being composed and another addon's own archive changes both
//! claim. The composer stops with a [`Conflict`] and the build names the addons on both sides
//! (`integrate.rs`): the addons whose records claim the part, and the addons whose changes to
//! `resource` put the other claim there.

use swofty_formats::pkg::{Node, Pkg, Val, Value};
use swofty_formats::uipre::{KEY_LOC, KEY_TEXT, KEY_VISIBLE};

use super::screens;
use crate::ui::node::{self, NodeExt};
use crate::{Error, Result};

/// What one `.uipre` setter makes the HUD editor show for one element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setting {
    Visible(bool),
    Text(String),
    Loc(String),
    Other(String),
}

impl Setting {
    pub fn key(&self) -> u32 {
        match self {
            Setting::Visible(_) => KEY_VISIBLE,
            Setting::Text(_) | Setting::Other(_) => KEY_TEXT,
            Setting::Loc(_) => KEY_LOC,
        }
    }

    pub fn val(&self) -> Val {
        match self {
            Setting::Visible(v) => Val::bool(*v),
            Setting::Text(t) | Setting::Loc(t) | Setting::Other(t) => Val::str(t),
        }
    }

    pub fn read(key: u32, v: &Val) -> Setting {
        match (key, &v.value) {
            (KEY_VISIBLE, Value::Raw(b)) if v.ty == *b"bool" && b.len() == 1 => Setting::Visible(b[0] != 0),
            (KEY_TEXT, Value::Str(t)) => Setting::Text(t.clone()),
            (KEY_LOC, Value::Str(t)) => Setting::Loc(t.clone()),
            (_, value) => Setting::Other(format!("{value:?}")),
        }
    }

    /// The setting of `element` in words, as in "<addon> hides mask".
    pub fn words(&self, element: &str) -> String {
        match self {
            Setting::Visible(true) => format!("shows {element}"),
            Setting::Visible(false) => format!("hides {element}"),
            Setting::Text(t) => format!("fills {element} with \"{t}\""),
            Setting::Loc(id) => format!("labels {element} with the game string {id}"),
            Setting::Other(v) => format!("sets {element} to {v}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A `.uipre` setter the copy the composer starts from already sets to another value.
    Preview {
        element: String,
        have: Setting,
        want: Setting,
    },
    /// A template node under a screen's parent that the game's template has not got already shows
    /// and hides the stock board the screen's records mask: another addon replaces the same board.
    Board {
        screen: String,
        element: String,
        node: String,
    },
    /// A template node the game's template has not got already shows and hides a stock element a
    /// record's `stock_nodes` or `restyle` hides.
    Shown {
        element: String,
        scene: String,
        node: String,
    },
    /// The screen records' board on `screen` takes the screen's UI tree past the slots the game can
    /// index (`walk.rs`): `instances` copies of a `host`-slot host scene, about `slots` in the tree.
    /// No other addon's change is involved; `off` are the addons that bring it inside on their own.
    Walk {
        screen: String,
        host: usize,
        instances: usize,
        slots: usize,
        off: Vec<String>,
    },
    /// The anim string table of `scene`, sorted, ends at `last`, past every name a record can add:
    /// another addon's copy of the scene added anim names of its own.
    Clips { scene: String, last: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The resource whose copy from another addon holds the other claim.
    pub resource: String,
    /// The addons whose records claim the part.
    pub addons: Vec<String>,
    pub kind: Kind,
}

impl Conflict {
    pub fn error(resource: &str, kind: Kind) -> Error {
        Error::HudConflict(Box::new(Conflict {
            resource: resource.to_owned(),
            addons: Vec::new(),
            kind,
        }))
    }

    /// The template node of another addon that holds the other claim.
    pub fn node(&self) -> Option<&str> {
        match &self.kind {
            Kind::Board { node, .. } | Kind::Shown { node, .. } => Some(node),
            Kind::Preview { .. } | Kind::Walk { .. } | Kind::Clips { .. } => None,
        }
    }

    /// The conflict in a sentence, with `ours` and `theirs` as the names of the two sides.
    pub fn text(&self, ours: &str, theirs: &str) -> String {
        match &self.kind {
            Kind::Preview { element, have, want } => format!(
                "{theirs} and {ours} set the HUD editor preview of {} differently: {theirs} {}, {ours} {}",
                shown_on(&self.resource),
                have.words(element),
                want.words(element)
            ),
            Kind::Board { screen, .. } => format!(
                "{theirs} and {ours} both replace {}, and only one of them can switch the game's own list off",
                screens::board_of(screen),
            ),
            Kind::Shown { element, scene, .. } => format!(
                "{theirs} and {ours} both show and hide {element} of {}",
                short(scene)
            ),
            Kind::Walk { screen, slots, .. } => format!(
                "{ours} {} {} too big for the game: about {slots} of the {} slots one screen can hold, and the game can close as soon as it shows it",
                if self.addons.len() > 1 {
                    "together make"
                } else {
                    "makes"
                },
                screens::board_of(screen),
                super::walk::SLOT_LIMIT
            ),
            Kind::Clips { scene, .. } => format!(
                "{theirs} and {ours} both animate {}, and {theirs}'s animation names leave none after them for {ours}",
                match shown_on(&uipre_of(scene)) {
                    part if part.ends_with(".uipre") => short(scene).to_owned(),
                    part => part,
                }
            ),
        }
    }

    /// Where the other side's claim is, for the end of the refusal.
    pub fn detail(&self, theirs: &str) -> String {
        match &self.kind {
            Kind::Preview { .. } => format!("{} from {theirs}", short(&self.resource)),
            Kind::Board { element, node, .. } => {
                format!("{theirs}'s template node {node} already shows and hides {element}")
            }
            Kind::Shown { node, .. } => format!("{theirs}'s template node {node}"),
            Kind::Walk { host, instances, .. } => {
                format!("the board holds the {host}-slot host scene {instances} times")
            }
            Kind::Clips { last, .. } => format!("{theirs}'s anim string table ends at {last}"),
        }
    }
}

impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ours = match self.addons.as_slice() {
            [] => "the composed HUD".to_owned(),
            a => a.join(", "),
        };
        let theirs = "another addon";
        write!(f, "{} ({})", self.text(&ours, theirs), self.detail(theirs))
    }
}

fn short(resource: &str) -> &str {
    resource.rsplit('/').next().unwrap_or(resource)
}

/// The `.uipre` resource that holds the editor preview of `scene`.
pub fn uipre_of(scene: &str) -> String {
    match scene.strip_suffix(".idf") {
        Some(base) => format!("{base}.uipre"),
        None => format!("{scene}.uipre"),
    }
}

/// What the player calls the HUD part a `.uipre` previews.
pub fn shown_on(uipre: &str) -> String {
    if uipre == screens::STACKED_UIPRE {
        return "the Track Position list".into();
    }
    if uipre == super::dials::facts(super::model::Dial::F125).uipre {
        return "the dial".into();
    }
    short(uipre).to_owned()
}

/// Each addon id of `ids` once, in the order they come.
pub fn addons_of<'a>(ids: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ids {
        if !out.iter().any(|o| o == id) {
            out.push(id.to_owned());
        }
    }
    out
}

/// `e` with the addons whose records claim the part, when it is a [`Conflict`].
pub fn by(e: Error, addons: &[String]) -> Error {
    match e {
        Error::HudConflict(mut c) => {
            c.addons = addons.to_vec();
            Error::HudConflict(c)
        }
        e => e,
    }
}

fn chain_names(chain: &Node) -> Vec<String> {
    chain
        .as_arr()
        .map(|a| {
            a.items
                .iter()
                .filter_map(|l| {
                    l.as_obj()?
                        .get("debugName")?
                        .as_val()?
                        .as_str()
                        .map(str::to_owned)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The nodes under `parent` in `template` that the game's template `original` has not got and that
/// set `visible` on one of `masks`, each a layout chain from the parent's scene (a node's chain
/// continues its parent's): the node's full template path and the mask it sets, links joined by
/// ` > `.
pub fn board_fights(
    template: &Node,
    original: &Node,
    parent: &str,
    masks: &[Vec<String>],
) -> Result<Vec<(String, String)>> {
    fn walk(
        n: &Node,
        path: &str,
        prefix: &[String],
        masks: &[Vec<String>],
        out: &mut Vec<(String, String)>,
    ) -> Result<()> {
        let chains: Vec<Vec<String>> = n
            .slot(node::LAYOUT)
            .map(|l| l.iter().map(chain_names).collect())
            .unwrap_or_default();
        if n.props().ok().is_some_and(|p| p.get("visible").is_some()) {
            for c in &chains {
                let full: Vec<String> = prefix.iter().chain(c).cloned().collect();
                if masks.contains(&full) {
                    out.push((path.to_owned(), full.join(" > ")));
                }
            }
        }
        let next: Vec<String> = prefix
            .iter()
            .chain(chains.first().into_iter().flatten())
            .cloned()
            .collect();
        for c in n.slot(node::CHILDREN)? {
            if c.is_node() {
                walk(c, &format!("{path}.{}", c.name()), &next, masks, out)?;
            }
        }
        Ok(())
    }
    let at = node::find_path(template, parent)?;
    let top = node::node_at(template, &at)?;
    let mut out = Vec::new();
    for c in top.slot(node::CHILDREN)? {
        if c.is_node() {
            walk(c, &format!("{parent}.{}", c.name()), &[], masks, &mut out)?;
        }
    }
    out.retain(|(p, _)| node::find_path(original, p).is_err());
    Ok(out)
}

/// The full template path of a node path that [`super::stock_edit::visible_fights`] gives relative
/// to the node at `under` (starting with that node's own name).
pub fn full_path(under: &str, rel: &str) -> String {
    match under.rsplit_once('.') {
        Some((base, _)) => format!("{base}.{rel}"),
        None => rel.to_owned(),
    }
}

/// Whether the template `t` has the node at the full path `path`.
pub fn has_node(t: &Pkg, path: &str) -> bool {
    node::find_path(&t.root, path).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARENT: &str = "hud_track_position.osd_visibility_option.track_position";

    fn template(extra: Vec<Node>) -> Node {
        let mut kids = vec![node::hidden("bottom_cap", &["driver_position_bottom_cap"])];
        kids.extend(extra);
        let mut t = node::group_node(
            "hud_track_position",
            vec![],
            &[],
            vec![node::group_node(
                "osd_visibility_option",
                vec![],
                &[],
                vec![node::group_node("track_position", vec![], &[], kids)],
            )],
        );
        node::fix_ids(&mut t, None);
        node::arr(vec![t])
    }

    fn masks() -> Vec<Vec<String>> {
        vec![vec![screens::STACKED_MASK.to_owned()]]
    }

    #[test]
    fn another_addons_node_on_the_board_mask_is_a_fight() {
        let tt = node::group_node(
            "tt_ctl",
            vec![],
            &[],
            vec![
                node::hidden("tt_mask", &["mask"]),
                node::group_node(
                    "tt_host_n",
                    vec![],
                    &["tt_host"],
                    vec![node::hidden("v_tt_root", &["tt_root"])],
                ),
            ],
        );
        let live = template(vec![tt]);
        let fights = board_fights(&live, &template(vec![]), PARENT, &masks()).unwrap();
        assert_eq!(fights, [(format!("{PARENT}.tt_ctl.tt_mask"), "mask".to_owned())]);
    }

    #[test]
    fn the_games_own_nodes_and_other_elements_are_no_fight() {
        let own = vec![node::hidden("stock_mask", &["mask"])];
        let live = template(own.clone());
        assert!(
            board_fights(&live, &template(own), PARENT, &masks())
                .unwrap()
                .is_empty()
        );
        let deeper = node::group_node(
            "x_ctl",
            vec![],
            &["tt_host"],
            vec![node::hidden("x_mask", &["mask"])],
        );
        let live = template(vec![deeper, node::hidden("x_other", &["lap_counter"])]);
        assert!(
            board_fights(&live, &template(vec![]), PARENT, &masks())
                .unwrap()
                .is_empty()
        );
        let chained = vec![vec!["driver_table".to_owned(), "Scene Root".to_owned()]];
        let live = template(vec![node::hidden("r_mask", &["driver_table", "Scene Root"])]);
        assert_eq!(
            board_fights(&live, &template(vec![]), PARENT, &chained).unwrap(),
            [(format!("{PARENT}.r_mask"), "driver_table > Scene Root".to_owned())]
        );
    }

    #[test]
    fn a_relative_node_path_gets_its_template_path() {
        assert_eq!(
            full_path(
                "hud_lap_counter.osd_visibility_option",
                "osd_visibility_option.tt_lapc.v_tt_lapc_car_category_logo"
            ),
            "hud_lap_counter.osd_visibility_option.tt_lapc.v_tt_lapc_car_category_logo"
        );
        assert_eq!(
            full_path("hud_lap_counter", "hud_lap_counter.x"),
            "hud_lap_counter.x"
        );
    }

    #[test]
    fn a_conflict_says_both_sides_and_what_they_claim() {
        let board = Conflict {
            resource: super::super::dials::TEMPLATE.to_owned(),
            addons: vec!["pablo.tv-timing-tower".to_owned()],
            kind: Kind::Board {
                screen: "standings".into(),
                element: "mask".into(),
                node: format!("{PARENT}.tt_ctl.tt_mask"),
            },
        };
        assert_eq!(board.node(), Some(format!("{PARENT}.tt_ctl.tt_mask").as_str()));
        assert_eq!(
            board.text("TV Timing Tower", "Project Halo"),
            "Project Halo and TV Timing Tower both replace the Track Position list, and only one of them can switch the game's own list off"
        );
        assert_eq!(
            board.detail("Project Halo"),
            format!("Project Halo's template node {PARENT}.tt_ctl.tt_mask already shows and hides mask")
        );
        let shown = Conflict {
            resource: super::super::dials::TEMPLATE.to_owned(),
            addons: vec![],
            kind: Kind::Shown {
                element: "car_category_logo".into(),
                scene: "eaid://ui_package/art/osd/components/lap_counter/lap_counter.idf".into(),
                node: "hud_lap_counter.osd_visibility_option.tt_lapc.v".into(),
            },
        };
        assert_eq!(
            shown.to_string(),
            "another addon and the composed HUD both show and hide car_category_logo of lap_counter.idf (another addon's template node hud_lap_counter.osd_visibility_option.tt_lapc.v)"
        );
        assert_eq!(addons_of(["a", "b", "a"]), ["a", "b"]);
    }
}
