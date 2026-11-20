//! The careers a profile can't load as they were saved, and the Play lines that say why.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::rows::{added_ids, records_rows};
use super::{Career, CareerAddon, is_guess, is_proven, label_of};
use crate::manifest::AddonManifest;
use crate::stdlib::range::Range;

/// What a career needs that profile `order` doesn't have on: one line per addon, empty when it
/// loads in that profile. An addon recorded only because it was on ([`ROWS_WHY`], [`TRACK_WHY`]) is
/// not a need.
pub fn missing(c: &Career, order: &[String], installed: &BTreeMap<String, AddonManifest>) -> Vec<String> {
    let mut out = Vec::new();
    for a in c.addons.iter().filter(|a| !is_guess(a)) {
        let name = installed
            .get(&a.id)
            .map_or(a.id.clone(), |m| format!("{} ({})", m.name, a.id));
        let range = a
            .need
            .as_deref()
            .and_then(|n| Range::parse(n).ok())
            .unwrap_or_else(Range::any);
        match installed.get(&a.id) {
            None => out.push(format!("needs {name}, which isn't installed")),
            Some(_) if !order.contains(&a.id) => out.push(format!("needs {name}, which is off")),
            Some(m) if !range.matches(&m.version) => {
                out.push(format!("needs {name} {range}, and {} is installed", m.version))
            }
            Some(_) => {}
        }
    }
    out
}

/// The careers of `list` in the save `profile` boots: the ones made under a profile that boots the
/// same save (its own launcher save, or the shared Steam save), and the ones with no profile yet.
/// Finding a profile's save looks for the game's copies, which on Windows runs `reg query` several
/// times, so it is done only for the careers of other profiles.
pub fn in_save_of<'a>(list: &'a [Career], data: &Path, profile: &str) -> Vec<&'a Career> {
    let source = |p: &str| crate::saves::vault::Location::source_of_profile(data, p);
    let mut mine: Option<Option<String>> = None;
    let mut sources: BTreeMap<&str, Option<String>> = BTreeMap::new();
    list.iter()
        .filter(|c| match c.profile.as_deref() {
            None => true,
            Some(p) if p == profile => true,
            Some(p) => {
                let mine = mine.get_or_insert_with(|| source(profile));
                *sources.entry(p).or_insert_with(|| source(p)) == *mine
            }
        })
        .collect()
}

/// The Play stop for careers with ids above the stock rows and no owner [`settle_rows`] could name:
/// no addon the install has ids for provides the row. Names the installed database addons that are
/// off as the ones to try. `None` when every career names its owners.
pub fn unowned_line(
    list: &[Career],
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
    owners: &BTreeSet<String>,
    stock_max: &BTreeMap<String, i64>,
) -> Option<String> {
    let careers: Vec<&str> = list
        .iter()
        .filter(|c| !c.is_gone() && !records_rows(c) && !added_ids(c, stock_max).is_empty())
        .map(|c| {
            if c.label.is_empty() {
                c.career.as_str()
            } else {
                c.label.as_str()
            }
        })
        .collect();
    if careers.is_empty() {
        return None;
    }
    let off: Vec<String> = owners
        .iter()
        .filter(|id| !order.contains(id))
        .filter_map(|id| installed.get(id).map(|m| m.name.clone()))
        .collect();
    let shown = careers.iter().take(3).copied().collect::<Vec<_>>().join("; ");
    let more = careers.len().saturating_sub(3);
    let more = if more > 0 {
        format!("; {more} more")
    } else {
        String::new()
    };
    let try_on = if off.is_empty() {
        String::new()
    } else {
        format!(
            " Addons that are off: {}.",
            off.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
        )
    };
    Some(format!(
        "{} in your save {} a team, car or driver that no installed addon provides ({shown}{more}). \
         Turn on the addons those careers were made with and press Play, or restore a save from before \
         in Profiles > Saves.{try_on}",
        match careers.len() {
            1 => "1 career".to_owned(),
            n => format!("{n} careers"),
        },
        if careers.len() == 1 { "uses" } else { "use" },
    ))
}

/// The Play stop for careers whose own rows come from an addon that is off or not installed in
/// profile `order`: the career's team, car or driver ([`OWN_WHY`]) or its My Team car ([`CAR_WHY`]).
/// An addon recorded only because it was on, a career gone from the save and a record no save slot
/// backs raise nothing. `None` when no such addon is missing.
pub fn stop_line(
    list: &[Career],
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
) -> Option<String> {
    let mut addons: BTreeMap<&str, String> = BTreeMap::new();
    let mut careers: Vec<&str> = Vec::new();
    for c in list.iter().filter(|c| !c.is_gone() && c.in_slots()) {
        let gone: Vec<&CareerAddon> = c
            .addons
            .iter()
            .filter(|a| is_proven(a))
            .filter(|a| !order.contains(&a.id) || !installed.contains_key(&a.id))
            .collect();
        if gone.is_empty() {
            continue;
        }
        for a in gone {
            let name = installed.get(&a.id).map_or(a.id.clone(), |m| m.name.clone());
            let state = if installed.contains_key(&a.id) {
                "off"
            } else {
                "not installed"
            };
            addons.insert(&a.id, format!("{name} ({state})"));
        }
        careers.push(label_of(c));
    }
    if addons.is_empty() {
        return None;
    }
    let names = addons.values().cloned().collect::<Vec<_>>().join(", ");
    let shown = careers.iter().take(3).copied().collect::<Vec<_>>().join("; ");
    let more = careers.len().saturating_sub(3);
    let more = if more > 0 {
        format!("; {more} more")
    } else {
        String::new()
    };
    let it = if addons.len() == 1 { "it" } else { "them" };
    Some(format!(
        "{} in your save {} a team, car or driver from {names} ({shown}{more}). Without {it}, {} may not \
         load, or may load with a stock car in its place. Turn {it} back on and press Play, or restore a \
         save from before in Profiles > Saves.",
        match careers.len() {
            1 => "1 career".to_owned(),
            n => format!("{n} careers"),
        },
        if careers.len() == 1 { "uses" } else { "use" },
        if careers.len() == 1 {
            "that career"
        } else {
            "those careers"
        },
    ))
}

/// The careers in `list` that need `addon` and are still in the save: the careers turning `addon` off
/// would stop from loading as they were saved. An addon recorded only because it was on is not needed.
pub fn needing<'a>(list: &'a [Career], addon: &str) -> Vec<&'a Career> {
    list.iter()
        .filter(|c| !c.is_gone() && c.in_slots())
        .filter(|c| c.addons.iter().any(|a| a.id == addon && !is_guess(a)))
        .collect()
}

/// The one Play line for the careers that won't load in profile `order`: how many, and the first
/// few with the addon each needs. `None` when every career loads.
pub fn play_line(
    list: &[Career],
    order: &[String],
    installed: &BTreeMap<String, AddonManifest>,
) -> Option<String> {
    let short: Vec<(&Career, Vec<String>)> = list
        .iter()
        .filter(|c| !c.is_gone() && c.in_slots())
        .map(|c| (c, missing(c, order, installed)))
        .filter(|(_, m)| !m.is_empty())
        .collect();
    if short.is_empty() {
        return None;
    }
    let shown = short
        .iter()
        .take(3)
        .map(|(c, m)| {
            let name = if c.label.is_empty() {
                c.career.as_str()
            } else {
                c.label.as_str()
            };
            format!("{name} {}", m[0])
        })
        .collect::<Vec<_>>()
        .join("; ");
    let more = short.len().saturating_sub(3);
    let more = if more > 0 {
        format!("; {more} more")
    } else {
        String::new()
    };
    Some(format!(
        "{} won't load in this profile: {shown}{more}",
        match short.len() {
            1 => "1 career".to_owned(),
            n => format!("{n} careers"),
        }
    ))
}
