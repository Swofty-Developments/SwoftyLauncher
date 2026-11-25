//! The launcher's own entries in the menu archives it writes. The game's copy of these archives holds none of
//! them, so a copy that does is a menu build's output an earlier session left in place of the original.

use std::path::Path;

use swofty_formats::erp::ErpFile;
use swofty_formats::pkg::{Node, Pkg};

use super::pages::ADJUST_TOOLTIP;
use super::patch::{MOD_MENU, MOD_MENU_OFFLINE, hands_over_to_return};
use super::{
    FRONTEND_TRANSITION, HUD_MESSAGES, MESSAGES, OVERRIDES, TOOLTIPS, UI_DATA, career_row, icons, rows,
};
use crate::Result;

/// The menu archives whose game copy a build checks for the launcher's own entries.
pub const MARKED: &[&str] = &[
    "databases.erp",
    "persistent.erp",
    "flow_persistent.erp",
    "flow_frontend.erp",
];

const PREFIX: &str = "swofty_";

fn data(file: &ErpFile, name: &str) -> Result<Option<Vec<u8>>> {
    let Ok(res) = file.erp.find(name) else {
        return Ok(None);
    };
    let res = file.load(res)?;
    match res.fragments.first() {
        Some(f) => Ok(Some(f.data()?)),
        None => Ok(None),
    }
}

fn at<'n>(root: &'n Node, path: &[&str]) -> Option<&'n Node> {
    path.iter().try_fold(root, |n, k| n.as_obj()?.get(k))
}

fn keys(file: &ErpFile, resource: &str, path: &[&str], names: &[&str]) -> Result<Vec<String>> {
    let Some(bytes) = data(file, resource)? else {
        return Ok(Vec::new());
    };
    let pkg = Pkg::parse(&bytes)?;
    let Some(dict) = at(&pkg.root, path).and_then(Node::as_obj) else {
        return Ok(Vec::new());
    };
    Ok(names
        .iter()
        .filter(|n| dict.get(n).is_some())
        .map(|n| (*n).to_owned())
        .collect())
}

fn message_ids(file: &ErpFile, resource: &str) -> Result<Vec<String>> {
    let Some(bytes) = data(file, resource)? else {
        return Ok(Vec::new());
    };
    let pkg = Pkg::parse(&bytes)?;
    Ok(at(&pkg.root, &["messages"])
        .and_then(Node::as_arr)
        .into_iter()
        .flat_map(|a| &a.items)
        .filter_map(|m| m.as_obj()?.get("id")?.as_val()?.as_str())
        .filter(|id| id.starts_with(PREFIX))
        .map(str::to_owned)
        .collect())
}

/// The launcher's own entries the copy at `path` of the menu archive `archive` (one of [`MARKED`]) holds, each
/// named for a message.
pub fn launcher_entries(archive: &str, path: &Path) -> Result<Vec<String>> {
    let file = ErpFile::open(path)?;
    let mut found = Vec::new();
    match archive {
        "databases.erp" => {
            let tips = ["tool_tips", "Contents", "tooltips_dictionary", "Contents"];
            for name in keys(
                &file,
                TOOLTIPS,
                &tips,
                &[MOD_MENU, MOD_MENU_OFFLINE, ADJUST_TOOLTIP],
            )? {
                found.push(format!("the tooltip {name}"));
            }
            for name in keys(&file, UI_DATA, &["ui_menu_data", "Contents"], &["swofty"])? {
                found.push(format!("the ui data {name}"));
            }
            let settings = ["overrides", "Contents", "settings", "Contents"];
            for name in keys(&file, OVERRIDES, &settings, &[icons::LIST])? {
                found.push(format!("the icon list settings.{name}"));
            }
        }
        "persistent.erp" => {
            for name in [rows::O_ROW, career_row::SCENE] {
                if file.erp.find(name).is_ok() {
                    found.push(format!("the resource {name}"));
                }
            }
        }
        "flow_persistent.erp" => {
            for resource in [MESSAGES, HUD_MESSAGES] {
                for id in message_ids(&file, resource)? {
                    found.push(format!("the message {id}"));
                }
            }
        }
        "flow_frontend.erp" => {
            if let Some(bytes) = data(&file, FRONTEND_TRANSITION)?
                && hands_over_to_return(&bytes)?
            {
                found.push(format!("the hand-over {}", super::patch::RETURN_ENTRY));
            }
        }
        _ => {}
    }
    Ok(found)
}
