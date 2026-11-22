//! The addon rows of the race MFD's pit page: NO/YES clones of the front wing repair row.

use swofty_formats::pkg::{Node, Val};

use super::addons_page::PitRow;
use super::mfd::template_mut;
use super::node::*;
use crate::Result;

const TEMPLATE: &str = "hud_mfd_dynamic";
const LIST: &str = "mfd.attacher.pit_settings_panel.settings_list";
const STOCK_ROW: &str = "repair";
const MAX_FLAGS: &[&str] = &["is_slider_max", "is_slider_max_unhighlighted"];
const LAST_OPTION: &str = "data[1]";

pub fn row_name(k: usize) -> String {
    format!("swofty_pit_{k}")
}

fn row(stock: &Node, parent: &str, r: &PitRow) -> Result<Node> {
    let mut n = clone_node(stock, parent, &r.name)?;
    let context = format!("[{STOCK_ROW}]");
    replace_strings(&mut n, &|v| {
        v.strip_prefix(context.as_str())
            .filter(|rest| rest.is_empty() || rest.starts_with('.'))
            .map(|rest| format!("[{}]{rest}", r.name))
    });
    n.props_mut()?.set("visible", prop(boolv(true), None));
    for c in n.slot_mut(COMPONENT)? {
        set_str(c.o_mut()?, "name", &r.name)?;
    }
    for flag in MAX_FLAGS {
        set_prop_value(n.child_mut(flag)?, "SecondDBEntry", Val::str(LAST_OPTION))?;
    }
    literal_label(n.child_mut("name")?, &r.label)?;
    Ok(n)
}

pub fn templates(data: &[u8], rows: &[PitRow]) -> Result<Vec<u8>> {
    edit(data, |p| {
        let list = template_mut(root_obj(p)?.at_mut("Templates")?.a_mut()?, TEMPLATE)?.find_mut(LIST)?;
        let parent = format!("{TEMPLATE}.{LIST}");
        let at = |list: &Node, name: &str| {
            list.slot(CHILDREN)
                .ok()
                .and_then(|kids| kids.iter().position(|c| c.is_node() && c.name() == name))
        };
        let mut prev = at(list, STOCK_ROW).ok_or_else(|| err(format!("{parent} has no {STOCK_ROW} row")))?;
        let stock = list.slot(CHILDREN)?[prev].clone();
        let mut added = 0;
        for r in rows {
            if let Some(i) = at(list, &r.name) {
                prev = i;
                continue;
            }
            let n = row(&stock, &parent, r)?;
            prev += 1;
            list.slot_mut(CHILDREN)?.insert(prev, n);
            added += 1;
        }
        if added > 0 {
            let height = list
                .props_mut()?
                .at_mut("viewHeight")?
                .o_mut()?
                .at_mut("value")?
                .v_mut()?;
            let h = height
                .as_u32()
                .ok_or_else(|| err(format!("{parent}.viewHeight is not a number")))?;
            height.set_u32(h + added);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use crate::addons::settings::SessionSettings;
    use crate::ui::pit_rows;

    #[test]
    fn the_rows_of_enabled_addons_are_numbered_in_order_up_to_two() {
        let addon = |id: &str, enabled: bool, rows: &[&str]| {
            serde_json::json!({
                "id": id, "name": id, "version": "1.0.0", "enabled": enabled, "locked": false,
                "enable_commands": ["", ""],
                "pit_rows": rows.iter().map(|r| serde_json::json!({ "id": r, "label": r.to_uppercase() })).collect::<Vec<_>>()
            })
        };
        let settings = |addons: Vec<serde_json::Value>| -> SessionSettings {
            serde_json::from_value(serde_json::json!({
                "schema": 1, "profile": "p", "restart_command": "", "addons": addons
            }))
            .unwrap()
        };
        let named = |s: &SessionSettings| {
            pit_rows(s)
                .into_iter()
                .map(|r| (r.addon, r.id, r.label, r.name))
                .collect::<Vec<_>>()
        };
        let row =
            |a: &str, id: &str, k: usize| (a.into(), id.into(), id.to_uppercase(), format!("swofty_pit_{k}"));
        let s = settings(vec![
            addon("a.off", false, &["off"]),
            addon("b.one", true, &["serve"]),
            addon("c.none", true, &[]),
            addon("d.two", true, &["x", "y"]),
        ]);
        assert_eq!(named(&s), [row("b.one", "serve", 0), row("d.two", "x", 1)]);
        let s = settings(vec![
            addon("d.two", true, &["x", "y"]),
            addon("b.one", true, &["serve"]),
        ]);
        assert_eq!(named(&s), [row("d.two", "x", 0), row("d.two", "y", 1)]);
        assert!(named(&settings(vec![addon("a.off", false, &["off"])])).is_empty());
    }
}

#[cfg(test)]
mod real {
    use swofty_formats::pkg::{Node, Pkg};

    use super::*;
    use crate::ui::{catalog, check, mfd, mfd_templates};

    const PANEL: &str = "hud_mfd_dynamic.mfd.attacher.pit_settings_panel";

    fn rows() -> Vec<PitRow> {
        [("serve_penalty", "SERVE PENALTY"), ("other", "Other row")]
            .iter()
            .enumerate()
            .map(|(k, (id, label))| PitRow {
                addon: "a.pit".into(),
                id: (*id).into(),
                label: (*label).into(),
                name: row_name(k),
            })
            .collect()
    }

    fn node<'a>(root: &'a Node, path: &str) -> &'a Node {
        node_at(root, &find_path(root, path).unwrap()).unwrap()
    }

    fn strings(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Val(v) => out.extend(v.as_str().map(str::to_owned)),
            Node::Obj(o) => o.items.iter().for_each(|(_, v)| strings(v, out)),
            Node::Arr(a) => a.items.iter().for_each(|v| strings(v, out)),
            Node::Iar(_) => {}
        }
    }

    fn prop_str<'a>(n: &'a Node, key: &str, field: &str) -> Option<&'a str> {
        n.props().ok()?.get(key)?.as_obj()?.get(field)?.as_val()?.as_str()
    }

    fn passes_the_ui_checks(stock: &[u8], out: &[u8]) {
        let input = check::UiInput {
            resource: mfd::TEMPLATES,
            frag: "syst",
            data: out,
            stock: Some(stock),
        };
        let report = check::ui_edit(
            &input,
            &catalog::Scenes::default(),
            catalog::stock(),
            catalog::engine(),
        )
        .unwrap();
        let hard: Vec<String> = report.hard.iter().map(|h| h.to_string()).collect();
        assert!(hard.is_empty(), "{hard:#?}");
    }

    #[test]
    fn the_pit_rows_follow_the_repair_row_on_the_real_templates() {
        let Ok(path) = std::env::var("SWOFTY_HUD_TEMPLATES") else {
            return;
        };
        let stock = std::fs::read(path).unwrap();
        let rows = rows();
        let out = templates(&stock, &rows).unwrap();
        assert!(templates(&out, &rows).unwrap() == out, "not idempotent");
        let t = Pkg::parse(&out).unwrap();
        let list = node(&t.root, &format!("{PANEL}.settings_list"));
        let names: Vec<&str> = list
            .slot(CHILDREN)
            .unwrap()
            .iter()
            .filter(|c| c.is_node())
            .map(|c| c.name())
            .collect();
        assert_eq!(
            names,
            ["downforce", "repair", "swofty_pit_0", "swofty_pit_1", "tyres"]
        );
        let height = |n: &Node| {
            n.props()
                .unwrap()
                .get("viewHeight")
                .unwrap()
                .as_obj()
                .unwrap()
                .get("value")
                .unwrap()
                .as_val()
                .unwrap()
                .as_u32()
        };
        assert_eq!(height(list), Some(5));
        let repair_path = format!("{PANEL}.settings_list.repair");
        let stock_t = Pkg::parse(&stock).unwrap();
        let repair = node(&stock_t.root, &repair_path);
        assert_eq!(canonical(node(&t.root, &repair_path)), canonical(repair));
        for r in &rows {
            let path = format!("{PANEL}.settings_list.{}", r.name);
            let n = node(&t.root, &path);
            let ctx = format!("[{}]", r.name);
            assert_eq!(prop_str(n, "dataContext", "dbentry"), Some(ctx.as_str()));
            assert_eq!(
                prop_str(n, "enabled", "dbentry"),
                Some(format!("{ctx}.enabled").as_str())
            );
            let visible = n.props().unwrap().get("visible").unwrap().as_obj().unwrap();
            assert!(visible.get("dbentry").is_none());
            assert_eq!(
                visible.get("value").unwrap().as_val().unwrap().value,
                Val::bool(true).value
            );
            let mut all = Vec::new();
            strings(n, &mut all);
            assert!(
                !all.iter().any(|s| s.contains("settings_list.repair")
                    || s.contains("[repair]")
                    || s == "repair_damage"),
                "{:?}",
                all.iter().filter(|s| s.contains("repair")).collect::<Vec<_>>()
            );
            for kept in [
                "data",
                "current_repair",
                "current_repair.display_name",
                "action_player_index",
            ] {
                assert!(all.iter().any(|s| s == kept), "{kept}");
            }
            assert!(all.iter().any(|s| s.starts_with(&format!("{path}.input"))));
            let component = &n.slot(COMPONENT).unwrap()[0];
            assert_eq!(component.as_obj().unwrap().str_at("name").unwrap(), r.name);
            let mut stock_component = repair.slot(COMPONENT).unwrap()[0].clone();
            set_str(stock_component.o_mut().unwrap(), "name", &r.name).unwrap();
            assert_eq!(canonical(component), canonical(&stock_component));
            for (flag, entry) in [
                ("is_slider_min", "data[0]"),
                ("is_slider_min_unhighlighted", "data[0]"),
                ("is_slider_max", "data[1]"),
                ("is_slider_max_unhighlighted", "data[1]"),
            ] {
                let f = node(&t.root, &format!("{path}.{flag}"));
                assert_eq!(prop_str(f, "SecondDBEntry", "value"), Some(entry), "{flag}");
                assert_eq!(
                    prop_str(f, "FirstDBEntry", "value"),
                    Some("current_repair"),
                    "{flag}"
                );
            }
            let name = node(&t.root, &format!("{path}.name"));
            let text = name.props().unwrap().get("text").unwrap().as_obj().unwrap();
            assert_eq!(text.str_at("value").unwrap(), r.label);
            assert!(text.get("dbentry").is_none() && text.get("isloctag").is_none());
            let localise = name.props().unwrap().get("localise").unwrap().as_obj().unwrap();
            assert_eq!(
                localise.get("value").unwrap().as_val().unwrap().value,
                Val::bool(false).value
            );
            assert_eq!(
                prop_str(node(&t.root, &format!("{path}.row_icon")), "texture", "dbentry"),
                prop_str(
                    node(&stock_t.root, &format!("{repair_path}.row_icon")),
                    "texture",
                    "dbentry"
                ),
            );
        }
        passes_the_ui_checks(&stock, &out);

        let one = templates(&stock, &rows[..1]).unwrap();
        let t = Pkg::parse(&one).unwrap();
        assert_eq!(height(node(&t.root, &format!("{PANEL}.settings_list"))), Some(4));
        assert!(find_path(&t.root, &format!("{PANEL}.settings_list.swofty_pit_1")).is_err());

        let page = [mfd::Row {
            label: "PIT LANE".into(),
            value: None,
        }];
        let same = |a: &[u8], b: &[u8]| {
            canonical(&Pkg::parse(a).unwrap().root) == canonical(&Pkg::parse(b).unwrap().root)
        };
        let both = mfd_templates(&stock, &page, &rows).unwrap();
        assert!(
            mfd_templates(&both, &page, &rows).unwrap() == both,
            "not idempotent"
        );
        assert!(
            same(&both, &mfd::templates(&out, &page).unwrap()),
            "the order of the edits matters"
        );
        let t = Pkg::parse(&both).unwrap();
        assert!(find_path(&t.root, &format!("hud_mfd_dynamic.mfd.attacher.{}", mfd::PANEL)).is_ok());
        assert!(find_path(&t.root, &format!("{PANEL}.settings_list.swofty_pit_1")).is_ok());
        passes_the_ui_checks(&stock, &both);
        assert!(mfd_templates(&stock, &page, &[]).unwrap() == mfd::templates(&stock, &page).unwrap());
        assert!(mfd_templates(&stock, &[], &rows).unwrap() == out);
    }
}
