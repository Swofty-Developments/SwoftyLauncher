//! The Resume Career list screen: one row per career, newest first, each with its mode, driver, team, place in
//! the season, last played date and a status colour. It is built at Play with literal text from the career
//! index, on the rows page ADDON SETTINGS uses; a press raises `swofty_resume:<row>` to the core and routes
//! to the stock continue screen of the row's mode, where the core loads the career.

use swofty_formats::pkg::{Node, Val};

use super::options::{literal_tooltips, settings_state_matrix};
use super::page::{scroll_rows, vm_page_bind};
use super::text::{fit, fit_to, tagged};
use super::{PAGE_LAYOUT, Sources, TAG_STYLE};
use crate::Result;
use crate::stdlib::careers::{ResumeRow, date_text};
use crate::ui::node::*;

pub const RESUME_SCREEN: &str = "swofty_resume_list";

/// The route that opens the list from the Career menu's Resume Career press.
pub const RESUME_ROUTE: &str = "swofty_resume_list";

pub const CONTINUE_DRIVER: &str = "solo_career_continue_driver";
pub const CONTINUE_MY_TEAM: &str = "solo_career_continue_my_team";

pub const COMMAND_PREFIX: &str = "swofty_resume:";

const TITLE: &str = "RESUME CAREER";
const READY: [u8; 4] = [0x2e, 0xcc, 0x71, 0xff];
const MISSING: [u8; 4] = [0xe7, 0x4c, 0x3c, 0xff];
const VERSION: [u8; 4] = [0xf5, 0xa6, 0x23, 0xff];

pub fn row_command(index: usize) -> String {
    format!("{COMMAND_PREFIX}{index}")
}

pub fn row_name(index: usize) -> String {
    format!("row_{index}")
}

pub fn status_colour(severity: &str) -> [u8; 4] {
    match severity {
        crate::stdlib::careers::SEVERITY_READY => READY,
        crate::stdlib::careers::SEVERITY_VERSION => VERSION,
        _ => MISSING,
    }
}

fn continue_route(r: &ResumeRow) -> &'static str {
    match r.mode.as_str() {
        "my_team" => CONTINUE_MY_TEAM,
        _ => CONTINUE_DRIVER,
    }
}

fn label(r: &ResumeRow) -> String {
    let who = match r.team.is_empty() {
        true => r.driver.clone(),
        false => format!("{}, {}", r.driver, r.team),
    };
    tagged(&fit_to(who, 150), &r.badge.to_uppercase(), TAG_STYLE)
}

fn place(r: &ResumeRow) -> String {
    match (r.season, r.race) {
        (Some(s), Some(race)) => format!("Season {s} · Race {race}"),
        (Some(s), None) => format!("Season {s}"),
        _ => String::new(),
    }
}

fn details(r: &ResumeRow) -> String {
    let mut lines = vec![r.badge.clone()];
    if !r.team.is_empty() {
        lines.push(r.team.clone());
    }
    let place = place(r);
    if !place.is_empty() {
        lines.push(place);
    }
    if let Some(at) = r.played {
        lines.push(format!("Last played {}", date_text(at)));
    }
    lines.push(r.status.clone());
    fit(lines.join("\n"))
}

fn tint(n: &mut Node, colour: [u8; 4]) -> Result<()> {
    set_bool(n, "visible", true)?;
    set_prop(
        n,
        "colourOverride",
        prop(Node::Val(Val::raw(*b"rgba", colour)), None),
    )
}

fn rgb(c: u32) -> [u8; 4] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 0xff]
}

fn row_node(src: &Sources, r: &ResumeRow) -> Result<Node> {
    let sname = RESUME_SCREEN;
    let mut t = instance(
        src.t("settings_edit_button_text")?,
        sname,
        &row_name(r.index),
        &[("label", &label(r)), ("value", &fit(r.status.clone()))],
        false,
    )?;
    t.slot_mut(CHILDREN)?
        .retain(|c| !matches!(c.name(), "button_pressed" | "reset"));
    set_bool(t.child_mut("indicator")?, "visible", false)?;
    if let Some(c) = r.colour {
        tint(t.child_mut("indicator")?, rgb(c))?;
    }
    tint(t.child_mut("button_icon")?, status_colour(&r.severity))?;
    let cmd = row_command(r.index);
    add_events(
        &mut t,
        vec![
            event(
                "focused",
                vec![
                    sig_with(
                        &format!("{sname}.description.set.title"),
                        "setText",
                        "focused",
                        &[("text", fit(r.driver.to_uppercase()).as_str().into())],
                    ),
                    sig_with(
                        &format!("{sname}.description.set.body"),
                        "setText",
                        "focused",
                        &[("text", details(r).as_str().into())],
                    ),
                ],
            ),
            event(
                "pressed",
                vec![
                    sig(&format!("{sname}.audio_theme_trigger"), "confirm", "pressed"),
                    raise_cmd(sname, "pressed", &cmd, &cmd),
                    sig_with(
                        &format!("{sname}.Action"),
                        "route",
                        "pressed",
                        &[("key", continue_route(r).into())],
                    ),
                ],
            ),
        ],
    )?;
    Ok(t)
}

/// The list screen for `rows`, which must be in index order and not empty.
pub fn build_resume_page(src: &Sources, rows: &[ResumeRow]) -> Result<(String, Vec<u8>)> {
    let sname = RESUME_SCREEN;
    let first = rows.first().ok_or_else(|| err("the resume list has no rows"))?;
    let scr = src.screen(sname, PAGE_LAYOUT)?;
    let fr = src.frame(sname)?;
    let mut kids = vec![
        src.title(sname, "settings_edit_screen", TITLE)?,
        hidden("hide_filter", &["filter"]),
        hidden("hide_scroll_bar", &["scroll_bar"]),
    ];
    let body = format!("{} career{}", rows.len(), if rows.len() == 1 { "" } else { "s" });
    let mut about = instance(
        src.t("settings_edit_description_category")?,
        sname,
        "about",
        &[("title", TITLE), ("body", &body)],
        true,
    )?;
    for c in about.slot_mut(CHILDREN)? {
        if c.name() == "icon" {
            set_bool(c, "visible", false)?;
        }
    }
    let mut panel = instance(
        src.t("settings_edit_description_panel")?,
        sname,
        "description",
        &[],
        true,
    )?;
    panel.slot_mut(CHILDREN)?.push(instance(
        src.t("settings_edit_description_set")?,
        &format!("{sname}.description"),
        "set",
        &[
            ("title", &fit(first.driver.to_uppercase())),
            ("body", &details(first)),
        ],
        true,
    )?);
    kids.extend([about, panel]);
    let mut rows_at = Vec::new();
    for r in rows {
        rows_at.push(kids.len());
        kids.push(row_node(src, r)?);
    }
    chain_focus(sname, &mut kids, &rows_at)?;
    scroll_rows(sname, &mut kids, &rows_at)?;
    let mut done = finish(scr, fr, kids, &row_name(first.index))?;
    vm_page_bind(&mut done)?;
    settings_state_matrix(src, &mut done)?;
    literal_tooltips(&mut done)?;
    Ok((sname.to_owned(), ui_bytes(vec![done])?))
}
