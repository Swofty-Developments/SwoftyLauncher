//! An entry's rows page: its panel, its rows scrolled in view, its title and the view model it binds to.

use swofty_formats::pkg::{Node, Val};

use super::page_rows::{Row, RowKind, number, state_value};
use super::text::{fit, tagged};
use super::{ADJUST_TOOLTIP, Entry, LIGHT_TAG, PAGE_LAYOUT, STATE, Sources, TAG_STYLE, VISIBLE_ROWS};
use crate::Result;
use crate::manifest::OptionType;
use crate::ui::node::*;

pub(in crate::games::f1_25::ui) fn as_group(n: &mut Node) -> Result<()> {
    let keep = [
        "id", "version", "type", "nameId", "name", "animData", "enabled", "visible",
    ];
    let p = n.props_mut()?;
    p.items
        .retain(|(k, _)| k.as_str().is_some_and(|k| keep.contains(&k)));
    p.at_mut("type")?.v_mut()?.set_u32(fourcc("UIGP"));
    n.a_mut()?[BEHAVIOURS] = behaviours(GROUP_BEHAVIOURS);
    Ok(())
}

pub(super) fn adjust_tip(fr: &mut Frame) -> Result<()> {
    let old = fr.tooltips_old.clone();
    let tips = fr.tooltips.slot_mut(CHILDREN)?;
    let mut c = tips
        .iter()
        .find(|c| c.name() == "select")
        .cloned()
        .ok_or_else(|| err("the tooltips have no select"))?;
    rename_refs(&mut c, &format!("{old}.select"), &format!("{old}.adjust"));
    rename(&mut c, "adjust")?;
    set_prop_value(&mut c, "tooltipsName", Val::str(ADJUST_TOOLTIP))?;
    tips.retain(|x| x.name() != "select");
    tips.push(c);
    Ok(())
}

pub(super) fn panel(
    src: &Sources,
    sname: &str,
    e: &Entry,
    first_title: &str,
    first_body: &str,
) -> Result<[Node; 2]> {
    let body = fit([e.details(), e.description.to_owned()]
        .into_iter()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join("\n"));
    let upper = fit(e.upper());
    let mut cat = instance(
        src.t("settings_edit_description_category")?,
        sname,
        "about",
        &[("title", &upper), ("body", &body)],
        true,
    )?;
    for c in cat.slot_mut(CHILDREN)? {
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
    let set = instance(
        src.t("settings_edit_description_set")?,
        &format!("{sname}.description"),
        "set",
        &[("title", first_title), ("body", first_body)],
        true,
    )?;
    panel.slot_mut(CHILDREN)?.push(set);
    Ok([cat, panel])
}

pub(super) fn build_page(src: &Sources, e: &Entry, rows: &[Row], adjustable: bool) -> Result<Node> {
    let sname = e.screen.as_str();
    let scr = src.screen(sname, PAGE_LAYOUT)?;
    let mut fr = src.frame(sname)?;
    if adjustable {
        adjust_tip(&mut fr)?;
    } else {
        fr.tooltips.slot_mut(CHILDREN)?.retain(|x| x.name() != "select");
    }
    let mut kids = vec![
        src.title(sname, "settings_edit_screen", &fit(e.upper()))?,
        hidden("hide_filter", &["filter"]),
        hidden("hide_scroll_bar", &["scroll_bar"]),
    ];
    let first = rows.first().ok_or_else(|| err(format!("{sname} has no rows")))?;
    kids.extend(panel(src, sname, e, &first.title, &first.body)?);
    let mut rows_at = Vec::new();
    for r in rows {
        let key = r.key.as_str();
        let mut t = match &r.kind {
            RowKind::Info(value) => {
                let mut t = instance(
                    src.t("settings_edit_toggle")?,
                    sname,
                    key,
                    &[("toggle.label", &r.label), ("toggle.value", &fit(value.clone()))],
                    true,
                )?;
                let tg = t.child_mut("toggle")?;
                as_group(tg)?;
                for c in tg.slot_mut(CHILDREN)? {
                    if matches!(c.name(), "indicator" | "left_arrow" | "right_arrow") {
                        set_bool(c, "visible", false)?;
                    }
                }
                t
            }
            RowKind::Slider(opt) => {
                let o = opt.o;
                let cur = state_value(o)?;
                let mut t = instance(
                    src.t("settings_edit_slider")?,
                    sname,
                    key,
                    &[
                        ("slider.label", &r.label),
                        ("slider.value", &fit(cur.text.clone())),
                    ],
                    true,
                )?;
                let vals = o
                    .states
                    .iter()
                    .map(|s| number(&s.value))
                    .collect::<Result<Vec<_>>>()?;
                let sl = t.child_mut("slider")?;
                let p = sl.props_mut()?;
                p.drop_key("dataContext");
                p.set(
                    "value",
                    prop(f32v(number(&cur.value)?), Some(&format!("{STATE}.s{}", opt.k))),
                );
                p.set("min", prop(f32v(vals[0]), None));
                p.set(
                    "max",
                    prop(
                        f32v(
                            *vals
                                .last()
                                .ok_or_else(|| err(format!("option {} has no states", o.id)))?,
                        ),
                        None,
                    ),
                );
                p.set("step", prop(f32v(1.0), None));
                let c = o.current;
                set_bool(sl.child_mut("left_toggle")?, "enabled", c > 0)?;
                set_bool(sl.child_mut("right_toggle")?, "enabled", c + 1 < vals.len())?;
                set_bool(sl.child_mut("indicator")?, "visible", false)?;
                t
            }
            RowKind::Toggle(opt) => {
                let o = opt.o;
                let c = o.current;
                let cur = state_value(o)?;
                let mut t = instance(
                    src.t("settings_edit_toggle")?,
                    sname,
                    key,
                    &[
                        ("toggle.label", &r.label),
                        ("toggle.value", &fit(cur.text.clone())),
                    ],
                    true,
                )?;
                let tg = t.child_mut("toggle")?;
                as_group(tg)?;
                set_bool(tg.child_mut("indicator")?, "visible", false)?;
                set_bool(tg.child_mut("left_arrow")?, "enabled", c > 0)?;
                set_bool(tg.child_mut("right_arrow")?, "enabled", c + 1 < o.states.len())?;
                t
            }
        };
        let mut evs = vec![event(
            "focused",
            vec![
                sig_with(
                    &format!("{sname}.description.set.title"),
                    "setText",
                    "focused",
                    &[("text", r.title.as_str().into())],
                ),
                sig_with(
                    &format!("{sname}.description.set.body"),
                    "setText",
                    "focused",
                    &[("text", r.body.as_str().into())],
                ),
            ],
        )];
        if let RowKind::Slider(opt) | RowKind::Toggle(opt) = &r.kind {
            let k = opt.k;
            evs.push(event(
                "navRight",
                vec![sig(&format!("{sname}.o{k}_next"), "query", "navRight")],
            ));
            evs.push(event(
                "navLeft",
                vec![sig(&format!("{sname}.o{k}_prev"), "query", "navLeft")],
            ));
        }
        add_events(&mut t, evs)?;
        rows_at.push(kids.len());
        kids.push(t);
    }
    chain_focus(sname, &mut kids, &rows_at)?;
    scroll_rows(sname, &mut kids, &rows_at)?;
    let first = kids[rows_at[0]].name().to_owned();
    finish(scr, fr, kids, &first)
}

pub(super) fn scroll_rows(sname: &str, kids: &mut [Node], rows_at: &[usize]) -> Result<()> {
    if rows_at.len() <= VISIBLE_ROWS {
        return Ok(());
    }
    let paths: Vec<String> = rows_at
        .iter()
        .map(|&k| format!("{sname}.{}", kids[k].name()))
        .collect();
    for (i, &k) in rows_at.iter().enumerate() {
        let mut evs = Vec::new();
        if i + 1 < paths.len() && i + 1 >= VISIBLE_ROWS {
            evs.push(event(
                "navDown",
                vec![sig(&paths[i + 1 - VISIBLE_ROWS], "hide", "navDown")],
            ));
        }
        if i > 0 {
            evs.push(event("navUp", vec![sig(&paths[i - 1], "show", "navUp")]));
        }
        add_events(&mut kids[k], evs)?;
    }
    Ok(())
}

pub(super) fn retitle(scr: &mut Node, e: &Entry) -> Result<()> {
    let upper = e.upper();
    let tag = e.kind.tag();
    set_text_value(
        scr.child_mut("screen_title")?.child_mut("title")?,
        &tagged(&upper, tag, LIGHT_TAG),
    )?;
    set_text_value(
        scr.child_mut("about")?.child_mut("title")?,
        &tagged(&upper, tag, TAG_STYLE),
    )
}

pub(super) fn bind_label(n: &mut Node, path: &str) -> Result<()> {
    let p = n.props_mut()?;
    p.set("text", obj(vec![("value", s("")), ("dbentry", s(path))]));
    p.set("localise", prop(boolv(false), None));
    Ok(())
}

pub(super) fn vm_page_bind(scr: &mut Node) -> Result<()> {
    set_prop(scr, "allowVMBinding", prop(boolv(true), None))
}

pub(super) fn vm_page(scr: &mut Node, e: &Entry) -> Result<()> {
    let sname = scr.name().to_owned();
    set_prop(scr, "allowVMBinding", prop(boolv(true), None))?;
    for opt in &e.options {
        let o = opt.o;
        let row = scr.child_mut(&format!("row_{}", opt.k))?;
        let loc = format!("o{}", opt.local);
        let cmd = format!("{}:{}", e.id, o.id);
        let slider = o.kind == OptionType::Slider;
        let sound = if slider { "slider" } else { "toggle" };
        let (ctl, arrows) = if slider {
            let ctl = row.child_mut("slider")?;
            let p = ctl.props_mut()?;
            p.set("dataContext", obj(vec![("value", s("")), ("dbentry", s(&loc))]));
            p.set("value", prop(i32v(0), Some(&format!("{loc}.value"))));
            p.set("min", prop(i32v(0), Some(&format!("{loc}.min"))));
            p.set("max", prop(i32v(1), Some(&format!("{loc}.max"))));
            p.set("step", prop(i32v(1), None));
            (ctl, ["left_toggle", "right_toggle"])
        } else {
            (row.child_mut("toggle")?, ["left_arrow", "right_arrow"])
        };
        let (label, text) = if slider {
            ("label".to_owned(), "text".to_owned())
        } else {
            (format!("{loc}.label"), format!("{loc}.text"))
        };
        bind_label(ctl.child_mut("label")?, &label)?;
        bind_label(ctl.child_mut("value")?, &text)?;
        for a in arrows {
            set_bool(ctl.child_mut(a)?, "enabled", true)?;
        }
        for ev in scr.child_mut(&format!("row_{}", opt.k))?.slot_mut(EVENTS)? {
            let o = ev.o_mut()?;
            let nm = o.str_at("eventName")?.to_owned();
            let verb = match nm.as_str() {
                "navRight" => "next",
                "navLeft" => "prev",
                _ => continue,
            };
            let c = format!("swofty_{verb}:{cmd}");
            *o.at_mut("signals")?.a_mut()? = vec![
                sig(&format!("{sname}.audio_theme_trigger"), sound, &nm),
                raise_cmd(&sname, &nm, &c, &c),
            ];
        }
    }
    fix_ids(scr, None);
    Ok(())
}
