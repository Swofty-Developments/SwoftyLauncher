use std::collections::HashMap;

use swofty_formats::pkg::{Node, Pkg};

use super::node::*;
use crate::Result;
use crate::addons::settings::{SessionMap, SessionOption};
use crate::manifest::{CareerSetup, OptionType};

mod home;
mod listing;
mod options;
mod page;
mod page_rows;
mod resume;
mod setup;
mod text;
mod tiles;

use home::build_home;
pub use home::{HomePage, home_of, home_plan, home_screen};
pub use listing::entries;
pub use options::build_options_page;
use options::{literal_tooltips, settings_state_matrix};
pub(super) use page::as_group;
use page::{build_page, retitle, vm_page};
use page_rows::{addon_rows, map_rows, mod_rows, number, state_value};
pub use resume::{
    COMMAND_PREFIX, CONTINUE_DRIVER, CONTINUE_MY_TEAM, NEW_CAREER_ROUTE, RESUME_ROUTE, RESUME_SCREEN,
    build_resume_page, colour_rgb, row_command, row_name, status_colour,
};
pub use setup::{
    SETUP_CONFIRM, SETUP_OPTIONS, build_setup_home, build_setup_page, setup_rows_screen, setup_title,
};

pub const HOME: &str = "swofty_settings";
pub const STATE: &str = "#ui_menu_data.swofty";
pub const HOME_LAYOUT: &str = "eaid://ui_package/art/screens/options/options_screen";
pub const PAGE_LAYOUT: &str = "eaid://ui_package/art/screens/options/options_edit_screen";
pub const ADJUST_TOOLTIP: &str = "swofty_adjust";
const TAG_STYLE: &str = "18_regular_upper_part_neg";
const LIGHT_TAG: &str = "17_regular_uber";
const SETTINGS_ICON: &str = "ui_package/art/textures/options/o_category/";
/// Columns of tiles ADDON SETTINGS keeps on screen; past them the row
/// scrolls by hiding the column at the other edge, which the stacker closes up.
const VISIBLE_COLUMNS: usize = 3;
/// Rows ADDON SETTINGS shows whole, each under its label, with the next one in part under them; past
/// them the rows scroll by hiding the row and label at the other edge, which the block's stacker closes
/// up.
const HOME_ROWS: usize = 2;
/// The row the page tiles go in.
const PAGES_ROW: &str = "More";
/// The home's one block, which stacks its rows.
const HOME_BLOCK: &str = "col_0";
/// The screen title's element, which the title's parts name first.
const TITLE: &str = "screen_title";
/// Rows an addon's page keeps on screen: what fits the settings page's clipping window. Past
/// them the page scrolls by hiding the row at the other edge, which the list's stacker closes up.
const VISIBLE_ROWS: usize = 18;
/// Tiles one ADDON SETTINGS home screen holds; more go on further home screens, which a NEXT PAGE
/// tile opens. No addon page has opened from a home of 60 tiles or more.
pub const HOME_PAGE_TILES: usize = 24;
pub const NEXT_PAGE: &str = "page_next";
pub const PREV_PAGE: &str = "page_prev";
const PAGE_FIRST: &str = "page_first";

pub struct TileStyle {
    pub rows: usize,
    pub slot: fn(usize) -> &'static str,
    /// A tile's icon, from its kind, its id and whether it is a track.
    pub icon: fn(Kind, &str, bool) -> Icon,
    pub attach: &'static [&'static str],
    pub hide_divider: bool,
    pub by_gap: bool,
    pub wrap: Option<(usize, usize)>,
}

pub enum Icon {
    Texture(&'static str),
    Ref(&'static str),
    Path(&'static str),
}

pub const TILES: TileStyle = TileStyle {
    rows: 3,
    slot: |_| "1a",
    icon: |k, id, track| Icon::Ref(super::icons::of(k, id, track)),
    attach: &["title_1", "stacker_title"],
    hide_divider: true,
    by_gap: true,
    wrap: Some((24, 32)),
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Addon,
    Map,
    Mod,
}

impl Kind {
    pub fn key(self) -> &'static str {
        match self {
            Kind::Addon => "addon",
            Kind::Map => "map",
            Kind::Mod => "mod",
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Kind::Addon => "ADDON",
            Kind::Map | Kind::Mod => "MOD",
        }
    }
}

pub struct Entry<'a> {
    pub kind: Kind,
    pub index: usize,
    pub screen: String,
    pub name: &'a str,
    pub author: &'a str,
    pub version: &'a str,
    pub description: &'a str,
    pub id: &'a str,
    /// A track addon or a track package's page.
    pub track: bool,
    /// The track packages an addon brings, shown on its page.
    pub maps: Vec<&'a SessionMap>,
    /// The row it is listed in: its launcher folder, or Addons, Tracks or Mods.
    pub folder: &'a str,
    pub options: Vec<Opt<'a>>,
    /// Every option of the addon, hidden ones too, for its career setup page.
    pub all: Vec<Opt<'a>>,
    pub setup: Option<&'a CareerSetup>,
}

pub struct Opt<'a> {
    pub k: usize,
    pub local: usize,
    pub o: &'a SessionOption,
}

impl Entry<'_> {
    fn details(&self) -> String {
        let author = if self.author.is_empty() {
            "unknown"
        } else {
            self.author
        };
        let version = if self.version.is_empty() {
            "?"
        } else {
            self.version
        };
        format!("by {author}, v{version}")
    }

    fn upper(&self) -> String {
        self.name.to_uppercase()
    }
}

pub struct Sources {
    pub templates: HashMap<String, Node>,
    pub career: Node,
    pub home: Node,
    pub container: Node,
}

impl Sources {
    pub fn parse(syst: &[u8], career: &[u8], home: &[u8], container: &[u8]) -> Result<Self> {
        let mut templates = HashMap::new();
        let p = Pkg::parse(syst)?;
        for t in p.root.o()?.at("Templates")?.a()? {
            templates.insert(t.name().to_owned(), t.clone());
        }
        Ok(Self {
            templates,
            career: first_screen_of(career)?,
            home: first_screen_of(home)?,
            container: first_screen_of(container)?,
        })
    }

    fn t(&self, name: &str) -> Result<&Node> {
        self.templates
            .get(name)
            .ok_or_else(|| err(format!("the settings templates have no {name}")))
    }

    fn frame(&self, sname: &str) -> Result<Frame> {
        frame(
            sname,
            &FrameSources {
                action: self.career.child("Action")?,
                audio: self.home.child("audio_theme_trigger")?,
                back: self.home.child("back_listener")?,
                tooltips: self.career.child("tooltips")?,
            },
            &["select", "back"],
        )
    }

    fn screen(&self, name: &str, layout: &str) -> Result<Node> {
        screen(name, layout, self.career.props()?)
    }

    fn title(&self, sname: &str, panel: &str, text: &str) -> Result<Node> {
        let mut t = self.t(panel)?.child("screen_title")?.clone();
        rename_refs(
            &mut t,
            &format!("{panel}.screen_title"),
            &format!("{sname}.screen_title"),
        );
        strip_data(&mut t)?;
        literal_label(t.child_mut("title")?, text)?;
        literal_label(t.child_mut("hidden_settings_text")?, "")?;
        for c in t.slot_mut(CHILDREN)? {
            if matches!(c.name(), "hidden_settings_bg" | "hidden_settings_text") {
                set_bool(c, "visible", false)?;
            }
        }
        Ok(t)
    }
}

pub struct Screens {
    pub order: Vec<(String, Vec<u8>)>,
    pub view_models: Vec<String>,
}

pub fn build_all(src: &Sources, style: &TileStyle, flow: &[Entry]) -> Result<Screens> {
    let mut nodes = Vec::new();
    for page in home_plan(flow.len()) {
        nodes.push((page.screen.clone(), build_home(src, style, flow, &page)?));
    }
    let mut view_models = Vec::new();
    for e in flow.iter().filter(|e| e.kind == Kind::Addon) {
        let rows = addon_rows(e);
        let mut scr = build_page(src, e, &rows, !e.options.is_empty())?;
        retitle(&mut scr, e)?;
        if !e.options.is_empty() {
            vm_page(&mut scr, e)?;
            view_models.push(e.screen.clone());
        }
        nodes.push((e.screen.clone(), scr));
    }
    for e in flow.iter().filter(|e| e.kind == Kind::Map) {
        let mut scr = build_page(src, e, &map_rows(e), false)?;
        retitle(&mut scr, e)?;
        nodes.push((e.screen.clone(), scr));
    }
    for e in flow.iter().filter(|e| e.kind == Kind::Mod) {
        let mut scr = build_page(src, e, &mod_rows(e), false)?;
        retitle(&mut scr, e)?;
        nodes.push((e.screen.clone(), scr));
    }
    let mut order = Vec::new();
    for (name, mut scr) in nodes {
        settings_state_matrix(src, &mut scr)?;
        literal_tooltips(&mut scr)?;
        order.push((name, ui_bytes(vec![scr])?));
    }
    Ok(Screens { order, view_models })
}

pub fn state_entries(flow: &[Entry]) -> Result<Vec<(String, Node)>> {
    let mut out = vec![
        ("last".to_owned(), uidb_var("last", "uistring", s(""))),
        (
            "online_popup_seen".to_owned(),
            uidb_var("online_popup_seen", "bool", boolv(false)),
        ),
    ];
    for e in flow.iter().filter(|e| e.kind == Kind::Addon) {
        for opt in &e.options {
            let k = opt.k;
            let o = opt.o;
            let name = format!("o{k}");
            out.push((
                name.clone(),
                uidb_var(&name, "uistring", s(&o.current.to_string())),
            ));
            if o.kind == OptionType::Slider {
                let name = format!("s{k}");
                let v = number(&state_value(o)?.value)?;
                out.push((name.clone(), uidb_var(&name, "float", f32v(v))));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
