//! The Resume Career list screen: one row per career, newest first, each with its mode, driver, team, place in
//! the season, last played date and a status colour. It is built at Play with literal text from the career
//! index, on the rows page ADDON SETTINGS uses; a press raises `swofty_resume:<row>` to the core and routes
//! to the stock continue screen of the row's mode, where the core loads the career. A row that cannot load
//! (`ResumeRow::loadable` false) only raises the command; the core shows why on the list screen.

use swofty_formats::pkg::{Node, Val};

use super::options::{literal_tooltips, settings_state_matrix};
use super::page::{scroll_rows, vm_page_bind};
use super::text::fit;
use super::tiles::set_component;
use super::{PAGE_LAYOUT, Sources};
use crate::Result;
use crate::stdlib::careers::{ResumeRow, date_text};
use crate::ui::career_row;
use crate::ui::node::*;

pub const RESUME_SCREEN: &str = "swofty_resume_list";

/// The route that opens the list from the Career menu's Resume Career press.
pub const RESUME_ROUTE: &str = "swofty_resume_list";

pub const CONTINUE_DRIVER: &str = "solo_career_continue_driver";
pub const CONTINUE_MY_TEAM: &str = "solo_career_continue_my_team";

/// The route of the New Career row: the Career menu the Career tile opened before the list took its place.
pub const NEW_CAREER_ROUTE: &str = "swofty_new_career";

pub const COMMAND_PREFIX: &str = "swofty_resume:";

const TITLE: &str = "CAREER";
const NEW_ROW: &str = "row_new";
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

fn who(r: &ResumeRow) -> &str {
    &r.driver
}

fn glyph_width(c: char) -> f64 {
    match c {
        'M' | 'W' => 1.7,
        c if c.is_uppercase() => 1.35,
        ' ' => 0.55,
        'i' | 'l' | 'j' | '.' | '\'' | 't' | 'f' | '1' | ',' => 0.5,
        c if c.is_ascii_digit() => 1.1,
        _ => 1.0,
    }
}

fn text_width(s: &str) -> f64 {
    s.chars().map(glyph_width).sum()
}

const DOTS: &str = "...";

/// Scene units per glyph width unit at a 1 pt style: the row font measured in game is 10.3 to 11.5 px per unit at
/// 17 pt and 11.1 at 19 pt, with 100 px to a scene unit; the scale takes the widest, so a cut row never reaches the
/// next column.
const UNITS_PER_GLYPH_PT: f64 = 0.0066;

fn budget(c: &career_row::Column) -> f64 {
    let size: f64 = c
        .style
        .split('_')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(17.0);
    c.width / (UNITS_PER_GLYPH_PT * size)
}

fn clip_col(s: &str, c: &career_row::Column) -> String {
    let room = budget(c) - text_width(DOTS);
    if text_width(s) <= budget(c) {
        return s.to_owned();
    }
    let mut words: Vec<&str> = s.split(' ').collect();
    while words.len() > 1 && text_width(&words.join(" ")) > room {
        words.pop();
    }
    let mut cut = words.join(" ");
    while text_width(&cut) > room && cut.pop().is_some() {}
    format!("{}{DOTS}", cut.trim_end())
}

fn place(r: &ResumeRow) -> String {
    match (r.season, r.race) {
        (Some(s), Some(race)) => format!("Season {s}, Race {race}"),
        (Some(s), None) => format!("Season {s}"),
        _ => String::new(),
    }
}

fn short_place(r: &ResumeRow) -> String {
    let full = place(r);
    match (text_width(&full) > budget(&career_row::PLACE), r.season, r.race) {
        (true, Some(s), Some(race)) => format!("S{s}, R{race}"),
        _ => full,
    }
}

fn short_badge(r: &ResumeRow) -> &str {
    r.badge.strip_suffix(" Career").unwrap_or(&r.badge)
}

fn short_status(severity: &str) -> &'static str {
    match severity {
        crate::stdlib::careers::SEVERITY_READY => "Ready",
        crate::stdlib::careers::SEVERITY_VERSION => "Version",
        _ => "Missing",
    }
}

fn details(r: &ResumeRow) -> String {
    let mut lines = vec![r.badge.clone()];
    if !r.team.is_empty() && r.team != who(r) {
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
    for a in &r.addons {
        lines.push(format!("{}: {}", a.name, a.state));
    }
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

/// A signed ARGB colour from the game database as the opaque RGB a node tints with.
pub fn colour_rgb(c: u32) -> [u8; 4] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 0xff]
}

const STOCK_PARTS: [&str; 7] = [
    "anim_open_and_pulse",
    "anim",
    "content_holder",
    "value",
    "label",
    "indicator",
    "button_icon",
];

fn through_base(n: &mut Node) -> Result<()> {
    let steps: Vec<String> = n
        .slot(LAYOUT)?
        .first()
        .and_then(Node::as_arr)
        .map(|g| {
            g.items
                .iter()
                .filter_map(|b| Some(b.as_obj()?.get("debugName")?.as_val()?.as_str()?.to_owned()))
                .collect()
        })
        .unwrap_or_default();
    if steps.is_empty() {
        return Ok(());
    }
    let mut path = vec![career_row::BASE];
    path.extend(steps.iter().map(String::as_str));
    n.a_mut()?[LAYOUT] = layout_path(&path);
    Ok(())
}

fn column(row: &mut Node, parent: &str, c: &career_row::Column, text: &str) -> Result<Node> {
    let mut n = clone_node(row.child("label")?, parent, c.name)?;
    n.a_mut()?[LAYOUT] = layout_path(&[c.name]);
    literal_label(&mut n, text)?;
    Ok(n)
}

fn base_row(src: &Sources, name: &str, texts: [&str; 5], status: Option<[u8; 4]>) -> Result<Node> {
    let mut t = instance(
        src.t("settings_edit_button_text")?,
        RESUME_SCREEN,
        name,
        &[("label", ""), ("value", "")],
        false,
    )?;
    set_component(&mut t, career_row::SCENE)?;
    t.slot_mut(CHILDREN)?
        .retain(|c| !matches!(c.name(), "button_pressed" | "reset"));
    for part in STOCK_PARTS {
        through_base(t.child_mut(part)?)?;
    }
    set_bool(t.child_mut("indicator")?, "visible", false)?;
    set_bool(t.child_mut("button_icon")?, "visible", false)?;
    let parent = format!("{RESUME_SCREEN}.{name}");
    let mut added = Vec::new();
    for (c, text) in career_row::COLUMNS.iter().zip(texts) {
        added.push(column(&mut t, &parent, c, text)?);
    }
    if let Some(colour) = status {
        tint(&mut added[4], colour)?;
    }
    t.slot_mut(CHILDREN)?.extend(added);
    Ok(t)
}

fn focused(title: &str, body: &str) -> Node {
    event(
        "focused",
        vec![
            sig_with(
                &format!("{RESUME_SCREEN}.description.set.title"),
                "setText",
                "focused",
                &[("text", fit(title.to_uppercase()).as_str().into())],
            ),
            sig_with(
                &format!("{RESUME_SCREEN}.description.set.body"),
                "setText",
                "focused",
                &[("text", body.into())],
            ),
        ],
    )
}

fn new_career_row(src: &Sources) -> Result<Node> {
    let mut t = base_row(src, NEW_ROW, ["NEW CAREER", "", "", "", ""], None)?;
    add_events(
        &mut t,
        vec![
            focused("New career", "Start a new Driver Career or My Team."),
            event(
                "pressed",
                vec![
                    sig(
                        &format!("{RESUME_SCREEN}.audio_theme_trigger"),
                        "confirm",
                        "pressed",
                    ),
                    sig_with(
                        &format!("{RESUME_SCREEN}.Action"),
                        "route",
                        "pressed",
                        &[("key", NEW_CAREER_ROUTE.into())],
                    ),
                ],
            ),
        ],
    )?;
    Ok(t)
}

fn row_node(src: &Sources, r: &ResumeRow) -> Result<Node> {
    let sname = RESUME_SCREEN;
    let first = clip_col(who(r), &career_row::DRIVER);
    let team = match r.team.as_str() {
        t if t == first || t == who(r) => String::new(),
        t => clip_col(t, &career_row::TEAM),
    };
    let badge = match short_badge(r) {
        b if b == first || b == who(r) => String::new(),
        b => clip_col(b, &career_row::BADGE),
    };
    let texts = [
        first.as_str(),
        team.as_str(),
        badge.as_str(),
        &clip_col(&short_place(r), &career_row::PLACE),
        short_status(&r.severity),
    ];
    let mut t = base_row(src, &row_name(r.index), texts, Some(status_colour(&r.severity)))?;
    let cmd = row_command(r.index);
    let mut press = vec![
        sig(&format!("{sname}.audio_theme_trigger"), "confirm", "pressed"),
        raise_cmd(sname, "pressed", &cmd, &cmd),
    ];
    if r.loadable {
        press.push(sig_with(
            &format!("{sname}.Action"),
            "route",
            "pressed",
            &[("key", continue_route(r).into())],
        ));
    }
    add_events(
        &mut t,
        vec![focused(who(r), &details(r)), event("pressed", press)],
    )?;
    Ok(t)
}

/// The Career list screen: the New Career row, then `rows`, which must be in index order.
pub fn build_resume_page(src: &Sources, rows: &[ResumeRow]) -> Result<(String, Vec<u8>)> {
    let sname = RESUME_SCREEN;
    let scr = src.screen(sname, PAGE_LAYOUT)?;
    let fr = src.frame(sname)?;
    let mut kids = vec![
        src.title(sname, "settings_edit_screen", TITLE)?,
        hidden("hide_filter", &["filter"]),
        hidden("hide_scroll_bar", &["scroll_bar"]),
    ];
    let body = match rows.len() {
        0 => "No careers yet".to_owned(),
        1 => "1 career".to_owned(),
        n => format!("{n} careers"),
    };
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
            ("title", "NEW CAREER"),
            ("body", "Start a new Driver Career or My Team."),
        ],
        true,
    )?);
    kids.extend([about, panel]);
    let mut rows_at = vec![kids.len()];
    kids.push(new_career_row(src)?);
    for r in rows {
        rows_at.push(kids.len());
        kids.push(row_node(src, r)?);
    }
    chain_focus(sname, &mut kids, &rows_at)?;
    scroll_rows(sname, &mut kids, &rows_at)?;
    let mut done = finish(scr, fr, kids, NEW_ROW)?;
    vm_page_bind(&mut done)?;
    settings_state_matrix(src, &mut done)?;
    literal_tooltips(&mut done)?;
    Ok((sname.to_owned(), ui_bytes(vec![done])?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_team_names_are_cut_at_a_word_by_their_drawn_width() {
        let t = &career_row::TEAM;
        assert_eq!(
            clip_col("Mercedes-AMG PETRONAS Formula One Team", t),
            "Mercedes-AMG PETRONAS..."
        );
        assert_eq!(
            clip_col("McLaren Mastercard Formula 1 Team", t),
            "McLaren Mastercard..."
        );
        assert_eq!(clip_col("McLaren Formula 1 Team", t), "McLaren Formula 1 Team");
        assert_eq!(clip_col("Williams Renault", t), "Williams Renault");
    }

    #[test]
    fn driver_names_and_places_seen_in_game_stay_whole() {
        for name in [
            "Riccardo PATRESE",
            "Andrea DE CESARIS",
            "Jacob NARDELLA",
            "George RUSSELL",
        ] {
            assert_eq!(clip_col(name, &career_row::DRIVER), name);
        }
        for place in ["Season 8, Race 12", "Season 10, Race 24"] {
            assert_eq!(clip_col(place, &career_row::PLACE), place);
        }
    }

    #[test]
    fn one_unbroken_word_is_cut_inside_the_column() {
        let cut = clip_col("Wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww", &career_row::TEAM);
        assert!(cut.ends_with(DOTS) && text_width(&cut) <= budget(&career_row::TEAM));
    }
}
