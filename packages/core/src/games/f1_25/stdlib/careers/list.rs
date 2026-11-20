//! The rows of the in-game Resume Career list: one per career the save slots back, newest first, with what each
//! row shows and whether pressing it loads the career at once.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::CareerNeeds;
use crate::saves::bag::{Bag, Value as BagValue};
use crate::saves::plain::{Manifest, Plain, SLOT_SUFFIX, Slot, mode_of};
use crate::saves::vault::Location;
use crate::saves::vault::career_names::Names;

pub const READY: &str = "ready";
pub const MISSING: &str = "missing";
pub const VERSION: &str = "version";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ResumeRow {
    pub index: usize,
    /// `driver` or `my_team`: the stock Resume Career screen that loads it.
    pub mode: String,
    /// The save slot key the core finds the slot by.
    pub key: String,
    /// The data group the game's wizard holds for the career's content: `f1_career_1990`, `f1_career_2025`.
    #[serde(default)]
    pub group: String,
    /// The save's container mode the row came from: `driver_career_1990`, `my_team_career_26`.
    #[serde(default)]
    pub game_mode: String,
    pub ready: bool,
    /// False when the career's mode is an addon's that is off: the game has no such mode to load it into, so the
    /// press shows a notice and loads nothing. A career whose team, car or driver row the build seems to lack asks
    /// Load Anyway / Back instead, like one whose addon is at another version.
    #[serde(default = "loadable_default")]
    pub loadable: bool,
    /// `ready`, `missing` (an addon that is off or not installed) or `version` (Mismatched: made with another
    /// addon version, or rows the build seems to lack).
    pub severity: String,
    pub status: String,
    pub badge: String,
    pub driver: String,
    pub team: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub race: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub played: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addons: Vec<RowAddon>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowAddon {
    pub name: String,
    pub state: String,
}

/// Whether the Resume Career list screen is on: always, unless `SWOFTY_RESUME_LIST=0`.
fn loadable_default() -> bool {
    true
}

pub fn wanted() -> bool {
    wanted_from(std::env::var("SWOFTY_RESUME_LIST").ok().as_deref())
}

fn wanted_from(value: Option<&str>) -> bool {
    value != Some("0")
}

fn pill(n: &CareerNeeds) -> (String, &'static str) {
    let mut parts: Vec<String> = Vec::new();
    let mut severity = READY;
    for a in &n.addons {
        if !a.on {
            parts.push(format!("Missing: {}", a.name));
            severity = MISSING;
        } else if !a.at_version {
            parts.push(format!("Mismatched version of {}", a.name));
            if severity == READY {
                severity = VERSION;
            }
        }
    }
    if let Some(name) = &n.missing_mode {
        let text = format!("Missing: {name}");
        if !parts.contains(&text) {
            parts.insert(0, text);
        }
        severity = MISSING;
    }
    if let Some(m) = &n.missing_rows {
        parts.push(m.text());
        if severity == READY {
            severity = VERSION;
        }
    }
    if parts.is_empty() {
        return ("Ready".into(), READY);
    }
    let shown = parts.len().min(2);
    let mut text = parts[..shown].join("; ");
    if parts.len() > shown {
        text.push_str(&format!(" and {} more", parts.len() - shown));
    }
    (text, severity)
}

/// `dd/mm/yyyy` of a unix time in seconds, in UTC.
pub fn date_text(secs: i64) -> String {
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{day:02}/{month:02}/{year}")
}

/// One career the save holds: a slot of a career mode's slot container in the plain store.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveCareer {
    /// The mode's name: `driver_career_26`, `my_team_career_25`, `driver_career_1990`.
    pub mode: String,
    pub slot: Slot,
}

/// The drivers and teams the player made, from the save's entrants container, by stat id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Created {
    pub drivers: BTreeMap<i64, String>,
    pub teams: BTreeMap<i64, String>,
}

const ENTRANTS: &str = "entrants_system";
const HIDDEN: u32 = 0x41;
const NO_ID: u32 = 0x3fff;

fn has_id(id: Option<u32>) -> bool {
    id.is_some_and(|i| i & NO_ID != NO_ID)
}

/// A slot the stock continue screens offer: a played, visible slot with a driver and a team. A slot without them
/// (a custom season's, or one never started as a career) is not a career.
fn is_career(s: &Slot) -> bool {
    s.last_played != 0 && s.flags & HIDDEN == 0 && has_id(s.driver) && has_id(s.team)
}

fn career_mode(mode: &str) -> bool {
    mode.starts_with("driver_career") || mode.starts_with("my_team_career")
}

fn bag_at<'a>(b: &'a Bag, key: &str) -> Option<&'a Bag> {
    match b.get(key)? {
        BagValue::Bag(inner) => Some(inner),
        _ => None,
    }
}

fn text_of(b: &Bag, key: &str) -> Option<String> {
    let s = match b.get(key)? {
        BagValue::S128(t) | BagValue::S256(t) | BagValue::St64(t) | BagValue::St32(t) | BagValue::Sstr(t) => {
            t.as_str()?.trim().to_owned()
        }
        _ => return None,
    };
    (!s.is_empty()).then_some(s)
}

fn id_of(b: &Bag, key: &str) -> Option<i64> {
    match b.get(key)? {
        BagValue::U16(v) => Some(i64::from(*v)),
        BagValue::U32(v) => Some(i64::from(*v)),
        BagValue::I32(v) => Some(i64::from(*v)),
        _ => None,
    }
}

fn entries<'a>(root: &'a Bag, table: &str) -> impl Iterator<Item = &'a Bag> + 'a {
    let list = bag_at(root, "data")
        .and_then(|d| bag_at(d, table))
        .and_then(|t| match t.get("list") {
            Some(BagValue::List(l)) => Some(&l.items),
            _ => None,
        });
    list.into_iter().flatten().filter_map(|v| match v {
        BagValue::Bag(b) => Some(b),
        _ => None,
    })
}

/// The created drivers' and teams' names in an entrants container's bytes. Empty when they cannot be read.
pub fn created_from_bag(bytes: &[u8]) -> Created {
    crate::saves::bag::decode(bytes).map_or_else(|_| Created::default(), |root| created_of(&root))
}

fn created_of(root: &Bag) -> Created {
    let mut out = Created::default();
    for d in entries(root, "drivers") {
        let (Some(id), Some(first)) = (id_of(d, "stat_id"), text_of(d, "first_name")) else {
            continue;
        };
        let name = match text_of(d, "last_name") {
            Some(last) => format!("{first} {last}"),
            None => first,
        };
        out.drivers.insert(id, name);
    }
    for t in entries(root, "teams") {
        if let (Some(id), Some(name)) = (id_of(t, "stat_id"), text_of(t, "display_name")) {
            out.teams.insert(id, name);
        }
    }
    out
}

fn unsigned_of(b: &Bag, key: &str) -> Option<u64> {
    match b.get(key)? {
        BagValue::U8(v) => Some(u64::from(*v)),
        BagValue::U16(v) => Some(u64::from(*v)),
        BagValue::U32(v) => Some(u64::from(*v)),
        BagValue::U64(v) => Some(*v),
        _ => None,
    }
}

fn small_of(b: &Bag, key: &str) -> Option<u32> {
    match b.get(key)? {
        BagValue::I32(v) => u32::try_from(*v).ok(),
        _ => unsigned_of(b, key).and_then(|v| u32::try_from(v).ok()),
    }
}

/// A slot of a save file's `<mode>_SaveSlotGameModeComponent` with the fields the core records for it in a plain
/// generation.
fn slot_of(b: &Bag) -> Option<Slot> {
    Some(Slot {
        slot: small_of(b, "slot_id")?,
        guid: format!("0x{:016x}", unsigned_of(b, "slot_guid")?),
        name: text_of(b, "name").unwrap_or_default(),
        season: small_of(b, "info_season"),
        round: small_of(b, "info_weekend"),
        team: small_of(b, "info_team"),
        driver: small_of(b, "info_driver"),
        last_played: unsigned_of(b, "info_date")
            .and_then(|v| i64::try_from(v).ok())
            .unwrap_or(0),
        flags: small_of(b, "type_flag").unwrap_or(0),
    })
}

fn slots_of(component: &Bag) -> Vec<Slot> {
    let Some(BagValue::List(list)) = bag_at(component, "data").and_then(|d| d.get("slots")) else {
        return Vec::new();
    };
    list.items
        .iter()
        .filter_map(|v| match v {
            BagValue::Bag(b) => slot_of(b),
            _ => None,
        })
        .collect()
}

/// The careers of a profile save file, the player-made names its entrants container holds and the career modes it
/// has a slot container of, with or without careers.
struct FileCareers {
    careers: Vec<SaveCareer>,
    created: Created,
    modes: BTreeSet<String>,
}

fn read_save_file(path: &std::path::Path) -> Option<FileCareers> {
    let file = std::fs::read(path).ok()?;
    let data = crate::saves::codec::decode_data(&file).ok()?;
    let root = crate::saves::bag::decode(&data.stream).ok()?;
    let mut out = FileCareers {
        careers: Vec::new(),
        created: Created::default(),
        modes: BTreeSet::new(),
    };
    for (name, c) in crate::saves::remap::save_containers(&root) {
        if name.starts_with(ENTRANTS) {
            out.created = created_of(c);
        } else if name.ends_with(SLOT_SUFFIX) && career_mode(&mode_of(&name)) {
            let mode = mode_of(&name);
            out.careers
                .extend(slots_of(c).into_iter().filter(is_career).map(|slot| SaveCareer {
                    mode: mode.clone(),
                    slot,
                }));
            out.modes.insert(mode);
        }
    }
    Some(out)
}

/// The careers in a profile save file, with the player-made names its entrants container holds. `None` when the
/// file cannot be read or decoded.
pub fn save_file_careers(path: &std::path::Path) -> Option<(Vec<SaveCareer>, Created)> {
    read_save_file(path).map(|f| (f.careers, f.created))
}

/// The slots of a slot container the game did not register (its addon's mode is not in the build), read from the
/// container the plain store kept.
fn kept_slots(plain: &Plain, generation: u64, c: &crate::saves::plain::Container) -> Vec<Slot> {
    std::fs::read(plain.bag_path(generation, c))
        .ok()
        .and_then(|b| crate::saves::bag::decode(&b).ok())
        .map(|bag| slots_of(&bag))
        .unwrap_or_default()
}

/// The careers of generation `m` of `plain`.
fn generation_careers(plain: &Plain, m: &Manifest) -> Vec<SaveCareer> {
    m.containers
        .iter()
        .filter(|c| career_mode(&c.mode()))
        .flat_map(|c| {
            let slots = match c.registered || !c.name.ends_with(SLOT_SUFFIX) {
                true => c.slots.clone(),
                false => kept_slots(plain, m.generation, c),
            };
            slots
                .into_iter()
                .filter(is_career)
                .map(|slot| SaveCareer { mode: c.mode(), slot })
        })
        .collect()
}

/// The careers of the newest generation in `plain`, with the player-made names its entrants container holds.
pub fn save_careers(plain: &Plain) -> (Vec<SaveCareer>, Created) {
    let Some(m) = plain.newest() else {
        return (Vec::new(), Created::default());
    };
    let created = m
        .containers
        .iter()
        .find(|c| c.name.starts_with(ENTRANTS))
        .and_then(|c| std::fs::read(plain.bag_path(m.generation, c)).ok())
        .map(|b| created_from_bag(&b))
        .unwrap_or_default();
    (generation_careers(plain, &m), created)
}

fn key_of(s: &Slot) -> String {
    match u64::from_str_radix(s.guid.trim_start_matches("0x"), 16) {
        Ok(g) if g != 0 => format!("{g:016x}"),
        _ => s.slot.to_string(),
    }
}

/// The year a career mode's name ends in when that is all that follows `driver_career_` or `my_team_career_`
/// (`driver_career_26`, `driver_career_1990`). An addon's own mode (`driver_career_f2_26`) has none, whatever its
/// name ends in.
fn year_of(mode: &str) -> Option<u32> {
    super::mode_suffix(mode)
        .filter(|s| s.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|y| y.parse::<u32>().ok())
        .filter(|y| (y.to_string().len() == 2 && *y < 100) || (1900..2100).contains(y))
}

/// The game's data group for a career mode with a year in its name; empty for a mode without one (an addon's own
/// content, `driver_career_f2_26` too), whose group the core takes from the content that registers the mode or
/// from the careers index.
pub fn group_of(mode: &str) -> String {
    match year_of(mode) {
        Some(y) if y < 100 => format!("f1_career_20{y:02}"),
        Some(y) => format!("f1_career_{y}"),
        None => String::new(),
    }
}

/// The content a career mode belongs to: the game's own name for its year, else the addon's label, else the year.
fn content_of(mode: &str, label: &str, names: &Names) -> String {
    let year = year_of(mode);
    if let Some(name) = year.and_then(|y| names.content_name(y)) {
        return name.to_owned();
    }
    if !label.is_empty() {
        return label.to_owned();
    }
    match year {
        Some(y) if y < 100 => format!("20{y:02}"),
        Some(y) => y.to_string(),
        None => String::new(),
    }
}

/// The list rows for the careers the save holds, newest first, named through `names` and `created`. `careers` is
/// the launcher's career index and `needs` what each of its careers needs; a save career the index has never seen
/// needs nothing.
pub fn resume_rows(
    saved: &[SaveCareer],
    created: &Created,
    needs: &[CareerNeeds],
    names: &Names,
) -> Vec<ResumeRow> {
    let mut rows: Vec<ResumeRow> = saved
        .iter()
        .map(|c| {
            let key = key_of(&c.slot);
            let n = needs.iter().find(|n| n.key == key);
            let my_team = c.mode.starts_with("my_team");
            let word = if my_team { "My Team" } else { "Driver Career" };
            let content = content_of(&c.mode, n.map_or("", |n| n.mode_label.as_str()), names);
            let badge = if content.is_empty() {
                word.to_owned()
            } else {
                format!("{content} {word}")
            };
            let driver = c.slot.driver.map(i64::from);
            let team = c.slot.team.map(i64::from);
            let driver = driver
                .and_then(|id| {
                    created
                        .drivers
                        .get(&id)
                        .cloned()
                        .or_else(|| names.driver_named(id))
                })
                .unwrap_or_default();
            let (team_name, colour) = match team {
                Some(id) if created.teams.contains_key(&id) => (created.teams[&id].clone(), None),
                Some(id) => names.team_named(id).map_or((String::new(), None), |t| (t.0, t.1)),
                None => (String::new(), None),
            };
            let (status, severity) = n.map_or(("Ready".to_owned(), READY), pill);
            let addons = n
                .map(|n| {
                    n.addons
                        .iter()
                        .map(|a| RowAddon {
                            name: a.name.clone(),
                            state: if !a.on {
                                "Off".to_owned()
                            } else if !a.at_version {
                                format!("Version {}", a.version)
                            } else {
                                "On".to_owned()
                            },
                        })
                        .collect()
                })
                .unwrap_or_default();
            ResumeRow {
                index: 0,
                mode: if my_team { "my_team" } else { "driver" }.into(),
                key,
                group: group_of(&c.mode),
                game_mode: c.mode.clone(),
                ready: severity == READY,
                loadable: n.is_none_or(|n| n.missing_mode.is_none()),
                severity: severity.into(),
                status,
                badge,
                driver,
                colour,
                team: team_name,
                season: c.slot.season.map(|s| i64::from(s) + 1),
                race: c.slot.round.map(|r| i64::from(r) + 1),
                played: Some(c.slot.last_played),
                addons,
            }
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.played));
    for (i, r) in rows.iter_mut().enumerate() {
        r.index = i;
    }
    rows
}

/// The careers the list shows: the careers of the save a session of `profile` loads, as the core leaves them after
/// the load.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListSave {
    pub profile: String,
    pub careers: Vec<SaveCareer>,
    pub created: Created,
    /// Where the careers came from, for the build's log.
    pub source: String,
}

impl ListSave {
    /// What the list shows of the save, as text a build hash takes in: each career slot and the names the player
    /// made.
    pub fn state(&self) -> String {
        format!("{:?}{:?}", self.careers, self.created)
    }
}

/// The careers a session of `profile` on `game` loads. `launcher_saves` is whether the profile plays on its launcher
/// saves.
pub fn list_save(
    data: &std::path::Path,
    profile: &str,
    launcher_saves: bool,
    game: &crate::install::Game,
) -> ListSave {
    list_save_with(data, profile, launcher_saves, game, || Location::for_game(game))
}

/// [`list_save`] with the game copy's own save from `own`. A session on launcher saves loads the data file of the
/// launcher store for `game`, or the copy's own while that store is empty, since the session start copies the copy's
/// own save into an empty store. Any other session loads the copy's own save. The plain store is the one the
/// session's settings name.
fn list_save_with(
    data: &std::path::Path,
    profile: &str,
    launcher_saves: bool,
    game: &crate::install::Game,
    own: impl FnOnce() -> Option<Location>,
) -> ListSave {
    let redirect = crate::saves::launcher::redirected(data, profile, launcher_saves);
    let plain = crate::saves::plain::root_of_session(data, profile, redirect).map(Plain::at);
    let store = redirect
        .then(|| crate::saves::launcher::store_of_game(data, profile, game))
        .filter(crate::saves::launcher::has_files);
    let file = store.or_else(own).map(|l| l.data_file());
    ListSave {
        profile: profile.to_owned(),
        ..session_careers(file.as_deref(), plain.as_ref())
    }
}

/// The careers of save file `file` as the core leaves them at load from the session's plain store `plain`: a career
/// mode the file has no slot container of comes from the plain store's newest session save generation, which the
/// core puts it back from, and a mode a pending restore request names comes from that request's generation when the
/// request replaces it or the file lacks it. Without a readable file, the careers of the plain store's newest
/// generation.
fn session_careers(file: Option<&std::path::Path>, plain: Option<&Plain>) -> ListSave {
    let Some(mut read) = file.and_then(read_save_file) else {
        let missing = match file {
            Some(f) => format!("no readable save file at {}", f.display()),
            None => "no save file found".to_owned(),
        };
        return match plain.and_then(|p| Some((p, p.newest()?))) {
            Some((p, m)) => {
                let (careers, created) = save_careers(p);
                ListSave {
                    careers,
                    created,
                    source: format!("generation {} of {} ({missing})", m.generation, p.root.display()),
                    ..ListSave::default()
                }
            }
            None => ListSave {
                source: format!("nothing: {missing} and no plain generation"),
                ..ListSave::default()
            },
        };
    };
    let mut source = file.map(|f| f.display().to_string()).unwrap_or_default();
    if let Some(p) = plain {
        let mut from: BTreeMap<String, u64> = BTreeMap::new();
        if let Some(m) = p.newest_session_save() {
            for mode in m.modes() {
                if career_mode(&mode) && !read.modes.contains(&mode) {
                    from.insert(mode, m.generation);
                }
            }
        }
        for r in p.pending() {
            let mode = mode_of(&r.container);
            if career_mode(&mode) && (r.replace || !read.modes.contains(&mode)) {
                from.insert(mode, r.generation);
            }
        }
        for (mode, generation) in from {
            let Some(m) = p.manifest(generation) else {
                continue;
            };
            read.careers.retain(|c| c.mode != mode);
            read.careers
                .extend(generation_careers(p, &m).into_iter().filter(|c| c.mode == mode));
            source.push_str(&format!(
                "; {mode} from generation {generation} of {}",
                p.root.display()
            ));
        }
    }
    ListSave {
        careers: read.careers,
        created: read.created,
        source,
        ..ListSave::default()
    }
}

/// The needs and rows of the list for the profile `order` loads: the careers of `saved`, named from the database the
/// build wrote in `build` with its id `aliases`, and what each needs from the career index in the data folder.
/// `None` when that database cannot be read.
pub fn resume_list_for(
    store: &crate::store::Store,
    game: &crate::install::Game,
    saved: &ListSave,
    build: &std::path::Path,
    order: &[&crate::manifest::AddonManifest],
    aliases: crate::row_ids::IdAliases,
    pending: Option<&swofty_formats::lng::Lng>,
) -> Option<(Vec<CareerNeeds>, Vec<ResumeRow>)> {
    let on: Vec<String> = order.iter().map(|m| m.id.clone()).collect();
    let registry = crate::row_ids::registry_rows(&store.root, Some(&game.dir), &on);
    let names = Names::of_build(store, game, build, aliases, pending)?.with_rows(&registry);
    let installed = store.addons().ok()?;
    let index = super::read(&super::data_path(&store.root));
    let careers = super::of_profile(&index, &saved.profile);
    let mut needs = super::resume_needs(&careers, &on, &installed, Some(names.rows()));
    let keyed: Vec<(String, String)> = saved
        .careers
        .iter()
        .map(|c| (key_of(&c.slot), c.mode.clone()))
        .collect();
    super::mark_missing_modes(&mut needs, &keyed, &super::mode_owners(&index), &on, &installed);
    let rows = resume_rows(&saved.careers, &saved.created, &needs, &names);
    Some((needs, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stdlib::careers::{MissingRows, ResumeNeed};
    use swofty_formats::gvdb::{Gvdb, Record};

    #[test]
    fn the_list_screen_is_on_unless_the_variable_is_zero() {
        assert!(wanted_from(None));
        assert!(wanted_from(Some("1")));
        assert!(wanted_from(Some("")));
        assert!(!wanted_from(Some("0")));
    }

    fn db() -> Gvdb {
        let mut r = vec![
            Record::new("entrants.teams._team_instancecount_", 0, "2"),
            Record::new("entrants.drivers._driver_instancecount_", 0, "1"),
            Record::new("entrants.vehicles._vehicle_instancecount_", 0, "1"),
            Record::new("entrants.vehicles.vehicle_0.stat_id", 0, "695"),
            Record::new("entrants.drivers.driver_0._metadata_", 2, "nigel"),
            Record::new("entrants.drivers.driver_0.stat_id", 0, "61"),
            Record::new("entrants.drivers.driver_0.first_name", 2, "lng_nigel"),
            Record::new("entrants.drivers.driver_0.last_name", 2, "lng_mansell"),
        ];
        for (i, (label, id, colour)) in [("haas", "7", "-16723266"), ("williams_1990", "646", "-16776961")]
            .iter()
            .enumerate()
        {
            r.push(Record::new(
                &format!("entrants.teams.team_{i}._metadata_"),
                2,
                *label,
            ));
            r.push(Record::new(&format!("entrants.teams.team_{i}.stat_id"), 0, *id));
            r.push(Record::new(
                &format!("entrants.teams.team_{i}.display_name"),
                2,
                "lng_team",
            ));
            r.push(Record::new(
                &format!("entrants.teams.team_{i}.ui_team_colour"),
                11,
                *colour,
            ));
        }
        Gvdb::new(r)
    }

    fn english() -> Vec<u8> {
        let e = |k: &str, v: &str| swofty_formats::lng::Entry {
            key: k.into(),
            value: v.into(),
        };
        swofty_formats::lng::Lng {
            entries: vec![
                e("lng_nigel", "Nigel"),
                e("lng_mansell", "Mansell"),
                e("lng_team", "Team"),
            ],
        }
        .to_bytes()
    }

    fn need(key: &str, addons: Vec<ResumeNeed>, rows: Option<MissingRows>) -> CareerNeeds {
        CareerNeeds {
            key: key.into(),
            career: key.into(),
            mode_label: String::new(),
            addons,
            missing_rows: rows,
            missing_mode: None,
        }
    }

    fn addon(name: &str, on: bool, at_version: bool) -> ResumeNeed {
        ResumeNeed {
            id: name.into(),
            name: name.into(),
            version: "1".into(),
            on,
            at_version,
            ..ResumeNeed::default()
        }
    }

    fn slot(id: u32, guid: &str, team: u32, driver: u32, season: u32, round: u32, played: i64) -> Slot {
        Slot {
            slot: id,
            guid: guid.into(),
            name: "Career Save 1".into(),
            season: Some(season),
            round: Some(round),
            team: Some(team),
            driver: Some(driver),
            last_played: played,
            flags: 0,
        }
    }

    fn saved(mode: &str, slot: Slot) -> SaveCareer {
        SaveCareer {
            mode: mode.into(),
            slot,
        }
    }

    fn created_bag() -> Vec<u8> {
        use crate::saves::bag::{Bag, List, Text, Value};
        let text = |s: &str| Value::S128(Text::new(s));
        let entry = |id: u16, first: &str, last: &str| {
            let mut b = Bag::new();
            b.push("stat_id", Value::U16(id));
            b.push("first_name", text(first));
            b.push("last_name", text(last));
            Value::Bag(b)
        };
        let team = |id: u16, name: &str| {
            let mut b = Bag::new();
            b.push("stat_id", Value::U16(id));
            b.push("display_name", text(name));
            Value::Bag(b)
        };
        let list = |items: Vec<Value>| {
            let mut l = List::new();
            l.items = items;
            let mut b = Bag::new();
            b.push("list", Value::List(l));
            Value::Bag(b)
        };
        let mut data = Bag::new();
        data.push(
            "drivers",
            list(vec![entry(16386, "John ", "BENNETT"), entry(16387, "Solo", "")]),
        );
        data.push("teams", list(vec![team(16384, "Bennett Racing")]));
        let mut root = Bag::new();
        root.push("data", Value::Bag(data));
        crate::saves::bag::encode(&root).unwrap()
    }

    #[test]
    fn the_players_own_drivers_and_teams_are_named_from_the_entrants_container() {
        let c = created_from_bag(&created_bag());
        assert_eq!(c.drivers[&16386], "John BENNETT");
        assert_eq!(c.drivers[&16387], "Solo");
        assert_eq!(c.teams[&16384], "Bennett Racing");
        assert_eq!(created_from_bag(b"not a bag"), Created::default());
    }

    #[test]
    fn rows_come_newest_first_with_their_status_and_names() {
        let list = vec![
            saved(
                "driver_career_26",
                slot(1, "0x00000000000000aa", 7, 61, 1, 3, 1_700_000_000),
            ),
            saved(
                "my_team_career_26",
                slot(2, "0x00000000000000bb", 16384, 16386, 0, 0, 1_800_000_000),
            ),
            saved(
                "driver_career_1990",
                slot(3, "0x00000000000000cc", 503, 61, 0, 0, 1_750_000_000),
            ),
            saved(
                "driver_career_25",
                slot(4, "0x00000000000000dd", 7, 16387, 0, 0, 1_600_000_000),
            ),
        ];
        let needs = vec![
            need(
                "00000000000000bb",
                vec![addon("Alpha", false, false), addon("Beta", true, false)],
                None,
            ),
            CareerNeeds {
                mode_label: "F1 1990".into(),
                ..need(
                    "00000000000000cc",
                    vec![],
                    Some(MissingRows {
                        addon: "F1 1990".into(),
                        tables: vec!["team".into()],
                    }),
                )
            },
            need("00000000000000dd", vec![addon("Beta", true, false)], None),
        ];
        let names = Names::from_db(db(), Some(&english()));
        let created = created_from_bag(&created_bag());
        let rows = resume_rows(&list, &created, &needs, &names);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "00000000000000bb",
                "00000000000000cc",
                "00000000000000aa",
                "00000000000000dd"
            ]
        );
        assert_eq!(rows.iter().map(|r| r.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(
            (rows[0].mode.as_str(), rows[0].badge.as_str()),
            ("my_team", "2026 My Team")
        );
        assert_eq!(rows[0].driver, "John BENNETT");
        assert_eq!(rows[0].team, "Bennett Racing");
        assert_eq!(rows[0].colour, None, "a player-made team has no database colour");
        assert_eq!(rows[0].status, "Missing: Alpha; Mismatched version of Beta");
        assert_eq!((rows[0].ready, rows[0].severity.as_str()), (false, MISSING));
        assert_eq!(rows[0].addons.len(), 2);
        assert_eq!(
            (rows[0].addons[0].name.as_str(), rows[0].addons[0].state.as_str()),
            ("Alpha", "Off")
        );
        assert_eq!(rows[0].addons[1].state, "Version 1");
        assert_eq!(rows[1].badge, "F1 1990 Driver Career");
        assert_eq!(rows[1].status, "Mismatched rows from F1 1990");
        assert_eq!((rows[1].ready, rows[1].severity.as_str()), (false, VERSION));
        assert!(
            rows.iter().all(|r| r.loadable),
            "rows the build seems to lack, an addon that is off or at another version all offer Load Anyway"
        );
        assert_eq!(rows[1].team, "", "a team the build lacks has no name");
        assert!(rows[2].ready && rows[2].status == "Ready" && rows[2].addons.is_empty());
        assert_eq!(rows[2].badge, "2026 Driver Career");
        assert_eq!(rows[2].driver, "Nigel Mansell");
        assert_eq!((rows[2].season, rows[2].race), (Some(2), Some(4)));
        assert_eq!(rows[2].colour, Some(0xd2be));
        assert_eq!(rows[3].badge, "2025 Driver Career");
        assert_eq!(rows[3].driver, "Solo");
        assert_eq!((rows[3].ready, rows[3].severity.as_str()), (false, VERSION));
    }

    #[test]
    fn a_driver_the_save_and_database_cannot_name_is_left_out_not_replaced_by_the_slot_name() {
        let list = vec![saved(
            "driver_career_26",
            slot(1, "0x00000000000000aa", 7, 16399, 0, 0, 5),
        )];
        let names = Names::from_db(db(), Some(&english()));
        let rows = resume_rows(&list, &Created::default(), &[], &names);
        assert_eq!(rows[0].driver, "");
        assert_eq!(rows[0].team, "Team");
    }

    #[test]
    fn only_career_modes_with_occupied_visible_slots_are_listed_from_a_plain_store() {
        use crate::saves::plain::fixture::{Gen, current, pair, write};
        let dir = std::env::temp_dir().join(format!("swofty-resume-save-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let p = Plain::at(&dir);
        let mut containers = pair(
            "driver_career_26",
            "game",
            vec![("0x00000000000000aa", "Career Save 1")],
        );
        containers.extend(pair(
            "online_custom",
            "game",
            vec![("0x00000000000000bb", "F1 25 Save 1")],
        ));
        containers.extend(pair(
            "grand_prix_23",
            "game",
            vec![("0x00000000000000cc", "F1 25 Save 1")],
        ));
        containers.extend(pair(
            "my_team_career_25",
            "game",
            vec![("0x00000000000000dd", "My Team Save 1")],
        ));
        write(
            &p,
            &Gen {
                n: 1,
                event: "load",
                session: Some("s"),
                containers,
            },
        );
        current(&p, 1);
        let (saved, created) = save_careers(&p);
        let modes: Vec<&str> = saved.iter().map(|c| c.mode.as_str()).collect();
        assert_eq!(modes, ["driver_career_26", "my_team_career_25"]);
        assert_eq!(created, Created::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_long_status_is_cut_to_two_causes_and_a_count() {
        let n = need(
            "a",
            vec![
                addon("A", false, false),
                addon("B", false, false),
                addon("C", false, false),
            ],
            None,
        );
        assert_eq!(pill(&n).0, "Missing: A; Missing: B and 1 more");
    }

    #[test]
    fn an_aliased_team_shows_its_name_and_a_dead_id_shows_none() {
        let list = vec![
            saved(
                "driver_career_1990",
                slot(1, "0x00000000000000aa", 503, 61, 0, 0, 2),
            ),
            saved(
                "driver_career_1990",
                slot(2, "0x00000000000000bb", 513, 61, 0, 0, 1),
            ),
        ];
        let mut names = Names::from_db(db(), Some(&english()));
        let c = Created::default();
        assert_eq!(resume_rows(&list, &c, &[], &names)[0].team, "");
        let mut aliases = crate::row_ids::IdAliases::default();
        aliases.tables.insert("teams".into(), [(503, 646)].into());
        names = names.with_aliases(aliases);
        let rows = resume_rows(&list, &c, &[], &names);
        assert_eq!(rows[0].team, "Team");
        assert_eq!(rows[0].colour, Some(0x0000_00ff));
        assert_eq!(rows[1].team, "");
    }

    #[test]
    fn every_row_names_its_container_mode_and_content_group() {
        assert_eq!(group_of("driver_career_1990"), "f1_career_1990");
        assert_eq!(group_of("driver_career_25"), "f1_career_2025");
        assert_eq!(group_of("my_team_career_26"), "f1_career_2026");
        assert_eq!(group_of("driver_career_progressive"), "");
        assert_eq!(
            group_of("driver_career_f2_26"),
            "",
            "an addon's mode ending in a number is not the stock 2026 group"
        );
        assert_eq!(group_of("my_team_career_f2_2026"), "");
        let list = vec![
            saved(
                "driver_career_1990",
                slot(1, "0x00000000000000aa", 645, 61, 0, 0, 2),
            ),
            saved(
                "my_team_career_26",
                slot(2, "0x00000000000000bb", 646, 61, 0, 0, 1),
            ),
            saved(
                "driver_career_progressive",
                slot(3, "0x00000000000000cc", 646, 61, 0, 0, 3),
            ),
        ];
        let names = Names::from_db(db(), Some(&english()));
        let rows = resume_rows(&list, &Created::default(), &[], &names);
        assert!(rows.iter().all(|r| !r.game_mode.is_empty()));
        let by = |mode: &str| rows.iter().find(|r| r.game_mode == mode).unwrap();
        assert_eq!(by("driver_career_1990").group, "f1_career_1990");
        assert_eq!(by("my_team_career_26").group, "f1_career_2026");
        assert_eq!(by("driver_career_progressive").group, "");
        let f2 = resume_rows(
            &[saved(
                "driver_career_f2_26",
                slot(4, "0x00000000000000dd", 646, 61, 0, 0, 4),
            )],
            &Created::default(),
            &[],
            &names,
        );
        assert_eq!(f2[0].group, "");
        assert_eq!(
            f2[0].badge, "Driver Career",
            "an addon's mode ending in _26 is not named as the game's 2026 content"
        );
        let json = serde_json::to_value(by("driver_career_1990")).unwrap();
        assert_eq!(json["group"], "f1_career_1990");
        assert_eq!(json["game_mode"], "driver_career_1990");
    }

    #[test]
    fn dates_are_day_month_year() {
        assert_eq!(date_text(0), "01/01/1970");
        assert_eq!(date_text(1_795_093_120), "19/11/2026");
        assert_eq!(date_text(951_782_400), "29/02/2000");
    }

    fn english_with_content() -> Vec<u8> {
        let e = |k: &str, v: &str| swofty_formats::lng::Entry {
            key: k.into(),
            value: v.into(),
        };
        swofty_formats::lng::Lng {
            entries: vec![
                e("lng_nigel", "Nigel"),
                e("lng_mansell", "Mansell"),
                e("lng_team", "Team"),
                e("lng_f1_25", "F1\u{ae} 2025"),
                e("lng_f1_2026", "F1\u{ae} 2026"),
                e("lng_f1_1990", "F1 1990"),
            ],
        }
        .to_bytes()
    }

    #[test]
    fn a_badge_is_the_games_own_name_of_the_content_and_the_mode() {
        let list = vec![
            saved("driver_career_25", slot(1, "0x00000000000000a1", 7, 61, 0, 0, 4)),
            saved("my_team_career_26", slot(2, "0x00000000000000a2", 7, 61, 0, 0, 3)),
            saved(
                "driver_career_1990",
                slot(3, "0x00000000000000a3", 7, 61, 0, 0, 2),
            ),
            saved(
                "driver_career_progressive",
                slot(4, "0x00000000000000a4", 7, 61, 0, 0, 1),
            ),
        ];
        let names = Names::from_db(db(), Some(&english_with_content()));
        let rows = resume_rows(&list, &Created::default(), &[], &names);
        let badges: Vec<&str> = rows.iter().map(|r| r.badge.as_str()).collect();
        assert_eq!(
            badges,
            [
                "F1\u{ae} 2025 Driver Career",
                "F1\u{ae} 2026 My Team",
                "F1 1990 Driver Career",
                "Driver Career"
            ]
        );
    }

    #[test]
    fn the_names_an_addon_adds_to_the_language_table_resolve_in_a_build() {
        let dir = std::env::temp_dir().join(format!("swofty-names-lang-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let lang = dir.join("swofty/assets/lang");
        std::fs::create_dir_all(&lang).unwrap();
        let e = |k: &str, v: &str| swofty_formats::lng::Entry {
            key: k.into(),
            value: v.into(),
        };
        std::fs::write(
            lang.join("all.lng"),
            swofty_formats::lng::Lng {
                entries: vec![e("lng_team", "Addon Team")],
            }
            .to_bytes(),
        )
        .unwrap();
        let merged = crate::saves::vault::career_names::with_build_lang(Some(english()), &dir, None).unwrap();
        let names = Names::from_db(db(), Some(&merged));
        assert_eq!(names.team_named(646).unwrap().0, "Addon Team");
        assert_eq!(
            names.driver_named(61).unwrap(),
            "Nigel Mansell",
            "the game's own text stays"
        );
        let same =
            crate::saves::vault::career_names::with_build_lang(Some(english()), &dir.join("none"), None)
                .unwrap();
        assert_eq!(same, english());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn text_an_addon_has_queued_but_not_yet_written_names_its_rows() {
        let queued = swofty_formats::lng::Lng {
            entries: vec![swofty_formats::lng::Entry {
                key: "lng_team".into(),
                value: "Queued Team".into(),
            }],
        };
        let none = std::env::temp_dir().join("swofty-resume-queued-none");
        let merged =
            crate::saves::vault::career_names::with_build_lang(Some(english()), &none, Some(&queued))
                .unwrap();
        let names = Names::from_db(db(), Some(&merged));
        assert_eq!(names.team_named(646).unwrap().0, "Queued Team");
        assert_eq!(names.driver_named(61).unwrap(), "Nigel Mansell");
    }

    fn slot_bag(
        (id, guid): (u64, u64),
        (team, driver): (u16, u16),
        (season, round): (i32, i32),
        date: u64,
        flag: u8,
    ) -> crate::saves::bag::Value {
        use crate::saves::bag::{Bag, Text, Value};
        let mut b = Bag::new();
        b.push("slot_id", Value::U64(id));
        b.push("slot_guid", Value::U64(guid));
        b.push("name", Value::S128(Text::new("Career Save 2")));
        b.push("info_season", Value::I32(season));
        b.push("info_weekend", Value::I32(round));
        b.push("info_team", Value::U16(team));
        b.push("info_date", Value::U64(date));
        b.push("type_flag", Value::U8(flag));
        b.push("info_driver", Value::U16(driver));
        Value::Bag(b)
    }

    fn save_file(dir: &std::path::Path) -> std::path::PathBuf {
        save_file_named(
            dir,
            crate::saves::plain::rewrite::SAVE_FILE,
            crate::saves::codec::STREAM_HEADER,
        )
    }

    fn save_file_named(dir: &std::path::Path, name: &str, header: [u8; 16]) -> std::path::PathBuf {
        save_with(
            dir,
            name,
            header,
            vec![
                (
                    "driver_career_1990",
                    vec![
                        slot_bag((8, 0x604a2c4a9c422bfc), (646, 105), (0, 1), 1_794_798_122, 0),
                        slot_bag((9, 0xaa), (16383, 16383), (-1, -1), 0, 0),
                    ],
                ),
                (
                    "grand_prix_23",
                    vec![slot_bag(
                        (9, 0x5ed07dc40d4f6b41),
                        (646, 105),
                        (0, 0),
                        1_794_797_255,
                        0,
                    )],
                ),
                (
                    "my_team_career_26",
                    vec![slot_bag(
                        (2, 0x635981a5f8d30254),
                        (16384, 197),
                        (7, 1),
                        1_792_460_159,
                        0x41,
                    )],
                ),
            ],
        )
    }

    fn save_with(
        dir: &std::path::Path,
        name: &str,
        header: [u8; 16],
        modes: Vec<(&str, Vec<crate::saves::bag::Value>)>,
    ) -> std::path::PathBuf {
        use crate::saves::bag::{Bag, List, Value};
        let component = |slots: Vec<Value>| {
            let mut l = List::new();
            l.items = slots;
            let mut data = Bag::new();
            data.push("slots", Value::List(l));
            let mut c = Bag::new();
            c.push("data", Value::Bag(data));
            Value::Bag(c)
        };
        let mut containers = Bag::new();
        for (mode, slots) in modes {
            containers.push(&format!("{mode}{SLOT_SUFFIX}"), component(slots));
        }
        containers.push(
            "entrants_system_entrants_system",
            Value::Bag(crate::saves::bag::decode(&created_bag()).unwrap()),
        );
        let mut save = Bag::new();
        save.push("uid", Value::U64(1));
        save.push("containers", Value::Bag(containers));
        let mut root = Bag::new();
        root.push("save_game", Value::Bag(save));
        let stream = crate::saves::bag::encode(&root).unwrap();
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        let sealed = crate::saves::codec::DecodedData {
            iv: [3u8; 16],
            header,
            frame: Vec::new(),
            stream: Vec::new(),
        };
        std::fs::write(&path, sealed.reseal(&stream, 3).unwrap()).unwrap();
        path
    }

    fn game_copy(root: &std::path::Path, ea: bool) -> crate::install::Game {
        let dir = root.join(if ea { "ea/F1 25" } else { "steam/F1 25" });
        let marker = if ea {
            "__Installer/installerdata.xml"
        } else {
            "EAJavelinInstaller_installscript.vdf"
        };
        for rel in ["F1_25.exe", marker] {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
        crate::install::Game {
            dir,
            user_reg: None,
            identity: crate::game_folder::baseline::Identity {
                build_id: "1".into(),
                exe_sha256: "0".repeat(64),
            },
        }
    }

    #[test]
    fn a_career_of_an_addon_mode_that_is_off_shows_the_addon_missing_and_does_not_load() {
        use crate::stdlib::careers::{Career, CareerAddon, mark_missing_modes, mode_owners};
        let career = |id: &str, why: &str, addon: &str| Career {
            career: id.into(),
            profile: Some("other".into()),
            label: String::new(),
            key: None,
            addons: vec![CareerAddon {
                id: addon.into(),
                version: None,
                need: None,
                why: Some(why.into()),
            }],
            extra: Default::default(),
        };
        let index = vec![
            career(
                "driver_career_progressive:5eb854e628b2449a",
                "career content f1_career_progressive",
                "swofty.progressive",
            ),
            career(
                "driver_career_1990:68af7d0c5b7b3262",
                "career content f1_career_1990",
                "swofty.f1-1990",
            ),
            career("driver_career_own:00000000000000ff", "its career mode", "a.own"),
            career(
                "driver_career_26:0000000000000001",
                "career content f1_career_2026",
                "swofty.core",
            ),
        ];
        let owners = mode_owners(&index);
        assert_eq!(
            owners.get("progressive").map(String::as_str),
            Some("swofty.progressive")
        );
        assert_eq!(owners.get("1990").map(String::as_str), Some("swofty.f1-1990"));
        assert_eq!(owners.get("own").map(String::as_str), Some("a.own"));
        assert!(!owners.contains_key("2026"));
        let list = vec![
            saved(
                "driver_career_progressive",
                slot(5, "0x6507954a314cde85", 478, 9, 0, 0, 1_793_205_412),
            ),
            saved(
                "driver_career_1990",
                slot(7, "0x68af7d0c5b7b3262", 645, 61, 0, 0, 1_793_688_849),
            ),
        ];
        let keyed: Vec<(String, String)> = list.iter().map(|c| (key_of(&c.slot), c.mode.clone())).collect();
        let mut needs = Vec::new();
        mark_missing_modes(
            &mut needs,
            &keyed,
            &owners,
            &["swofty.f1-1990".into()],
            &BTreeMap::new(),
        );
        assert_eq!(needs.len(), 1);
        let names = Names::from_db(db(), Some(&english()));
        let rows = resume_rows(&list, &Created::default(), &needs, &names);
        let progressive = rows.iter().find(|r| r.key == "6507954a314cde85").unwrap();
        assert_eq!(progressive.status, "Missing: swofty.progressive");
        assert_eq!(progressive.severity, MISSING);
        assert!(!progressive.ready && !progressive.loadable);
        let mansell = rows.iter().find(|r| r.key == "68af7d0c5b7b3262").unwrap();
        assert!(mansell.ready && mansell.loadable);
        let mut on = Vec::new();
        mark_missing_modes(
            &mut on,
            &keyed,
            &owners,
            &["swofty.progressive".into(), "swofty.f1-1990".into()],
            &BTreeMap::new(),
        );
        assert!(on.is_empty());
    }

    #[test]
    fn a_plain_store_lists_the_careers_of_a_slot_container_the_game_kept_unregistered() {
        use crate::saves::plain::fixture::{Gen, current, fnv, write};
        let root = std::env::temp_dir().join(format!("swofty-resume-kept-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let p = Plain::at(crate::saves::plain::root_of_profile(&root, "p"));
        let slot_name = "driver_career_progressive_SaveSlotGameModeComponent";
        write(
            &p,
            &Gen {
                n: 1,
                event: "load",
                session: Some("s"),
                containers: vec![
                    ("gm_driver_career_progressive", "kept", vec![]),
                    (slot_name, "kept", vec![]),
                ],
            },
        );
        current(&p, 1);
        assert!(save_careers(&p).0.is_empty());
        use crate::saves::bag::{List, Value};
        let mut l = List::new();
        l.items = vec![slot_bag(
            (5, 0x6507954a314cde85),
            (478, 9),
            (0, 0),
            1_793_205_412,
            0,
        )];
        let mut data = Bag::new();
        data.push("slots", Value::List(l));
        let mut component = Bag::new();
        component.push("data", Value::Bag(data));
        let encoded = crate::saves::bag::encode(&component).unwrap();
        std::fs::write(
            p.generation_dir(1)
                .join("containers")
                .join(format!("{}.bag", fnv(slot_name))),
            &encoded,
        )
        .unwrap();
        let manifest_path = p.generation_dir(1).join("manifest.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
        for c in manifest["containers"].as_array_mut().unwrap() {
            if c["name"] == slot_name {
                c["bytes"] = encoded.len().into();
            }
        }
        std::fs::write(&manifest_path, manifest.to_string()).unwrap();
        let (careers, _) = save_careers(&p);
        assert_eq!(careers.len(), 1);
        assert_eq!(careers[0].mode, "driver_career_progressive");
        assert_eq!(key_of(&careers[0].slot), "6507954a314cde85");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_save_files_careers_are_read_as_the_core_records_them() {
        let dir = std::env::temp_dir().join(format!("swofty-resume-file-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let (careers, created) = save_file_careers(&save_file(&dir)).unwrap();
        assert_eq!(careers.len(), 1);
        assert_eq!(careers[0].mode, "driver_career_1990");
        assert_eq!(
            careers[0].slot,
            Slot {
                slot: 8,
                guid: "0x604a2c4a9c422bfc".into(),
                name: "Career Save 2".into(),
                season: Some(0),
                round: Some(1),
                team: Some(646),
                driver: Some(105),
                last_played: 1_794_798_122,
                flags: 0,
            }
        );
        assert_eq!(key_of(&careers[0].slot), "604a2c4a9c422bfc");
        assert_eq!(created.drivers[&16386], "John BENNETT");
        assert!(save_file_careers(&dir.join("missing")).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn ea_header() -> [u8; 16] {
        let mut header = crate::saves::codec::STREAM_HEADER;
        header[13..].copy_from_slice(&[0xb4, 0xeb, 0xc9]);
        header
    }

    fn keys(saved: &ListSave) -> Vec<String> {
        let mut out: Vec<String> = saved.careers.iter().map(|c| key_of(&c.slot)).collect();
        out.sort();
        out
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("swofty-resume-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        root
    }

    fn imported(root: &std::path::Path) {
        let marker = crate::saves::launcher::marker_path(root);
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(marker, "{}").unwrap();
    }

    fn generation(
        p: &Plain,
        n: u64,
        event: &str,
        containers: Vec<crate::saves::plain::fixture::FixtureContainer>,
    ) {
        use crate::saves::plain::fixture::{Gen, current, write};
        write(
            p,
            &Gen {
                n,
                event,
                session: Some("s"),
                containers,
            },
        );
        current(p, n);
    }

    fn niggo_layout(root: &std::path::Path) -> (crate::install::Game, Location) {
        let career = |id: u64, guid: u64, team: u16, driver: u16, season: i32, date: u64| {
            slot_bag((id, guid), (team, driver), (season, 23), date, 0)
        };
        let stock = || {
            (
                "driver_career_26",
                vec![career(2, 0x6090e65341392af1, 478, 16388, 1, 1_795_107_833)],
            )
        };
        let f2 = || {
            (
                "driver_career_f2_26",
                vec![
                    career(1, 0x6820c8793a7ff7ff, 480, 16386, 0, 1_785_682_480),
                    career(2, 0x64adcb609301dde8, 477, 16387, 9, 1_794_148_590),
                ],
            )
        };
        let older = || {
            (
                "driver_career_25",
                vec![
                    career(1, 0x6820435d59bd9ccb, 505, 7, 0, 1_794_140_370),
                    slot_bag((3, 0x6cc4fd7ed2d2f62f), (505, 7), (0, 0), 1_793_876_614, 0x01),
                ],
            )
        };
        let ea = game_copy(root, true);
        let own = Location::ea(&root.join("Nico/AppData/Local/F1 25/savegame/ea/profile"), "Nico");
        save_with(&own.dir, "NXDSMWW.BWW", ea_header(), vec![stock(), older(), f2()]);
        save_with(
            &crate::saves::launcher::ea_dir(root, "default"),
            "NXDSMWW.BWW",
            ea_header(),
            vec![
                (
                    "driver_career_26",
                    vec![slot_bag((2, 0x73abbb879a0faa02), (478, 16388), (0, 0), 0, 0)],
                ),
                older(),
                f2(),
            ],
        );
        crate::saves::plain::remember_copy_source(root, &own.source);
        generation(&Plain::of_source(root, &own.source), 2, "load", Vec::new());
        generation(
            &Plain::at(crate::saves::plain::root_of_profile(root, "default")),
            1,
            "load",
            crate::saves::plain::fixture::pair(
                "driver_career_26",
                "game",
                vec![
                    ("0x6820435d59bd9ccb", "F1 25 Save 1"),
                    ("0x64adcb609301dde8", "Career Save 2"),
                ],
            ),
        );
        imported(root);
        (ea, own)
    }

    #[test]
    fn launcher_saves_off_lists_the_ea_copys_own_save_and_not_a_stale_plain_store() {
        let root = scratch("niggo-off");
        let (ea, own) = niggo_layout(&root);
        let stale = Plain::at(crate::saves::plain::root_of_profile(&root, "default"));
        assert_eq!(save_careers(&stale).0.len(), 2);
        assert!(save_careers(&Plain::of_source(&root, &own.source)).0.is_empty());
        let saved = list_save_with(&root, "default", false, &ea, || Some(own.clone()));
        assert_eq!(
            keys(&saved),
            [
                "6090e65341392af1",
                "64adcb609301dde8",
                "6820435d59bd9ccb",
                "6820c8793a7ff7ff"
            ]
        );
        assert_eq!(saved.source, own.data_file().display().to_string());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn launcher_saves_on_lists_the_launcher_store_the_game_loads() {
        let root = scratch("niggo-on");
        let (ea, own) = niggo_layout(&root);
        let saved = list_save_with(&root, "default", true, &ea, || Some(own.clone()));
        assert_eq!(
            keys(&saved),
            ["64adcb609301dde8", "6820435d59bd9ccb", "6820c8793a7ff7ff"]
        );
        let store = crate::saves::launcher::location_ea(&root, "default");
        assert_eq!(saved.source, store.data_file().display().to_string());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_ea_plain_generation_with_no_containers_hides_no_career() {
        let root = scratch("ea-empty-plain");
        let (ea, own) = niggo_layout(&root);
        let launcher = Plain::at(crate::saves::plain::root_of_profile(&root, "default"));
        generation(&launcher, 2, "load", Vec::new());
        generation(&launcher, 3, "save", Vec::new());
        generation(&Plain::of_source(&root, &own.source), 3, "save", Vec::new());
        let on = list_save_with(&root, "default", true, &ea, || Some(own.clone()));
        let off = list_save_with(&root, "default", false, &ea, || Some(own.clone()));
        assert_eq!(on.careers.len(), 3);
        assert_eq!(off.careers.len(), 4);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_empty_launcher_store_lists_the_save_the_session_start_copies_into_it() {
        let root = scratch("seed");
        let (ea, own) = niggo_layout(&root);
        std::fs::remove_dir_all(crate::saves::launcher::ea_dir(&root, "default")).unwrap();
        let saved = list_save_with(&root, "default", true, &ea, || Some(own.clone()));
        assert_eq!(saved.careers.len(), 4);
        assert_eq!(saved.source, own.data_file().display().to_string());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn without_a_readable_save_the_list_falls_back_to_the_sessions_plain_store() {
        let root = scratch("fallback");
        let (ea, own) = niggo_layout(&root);
        let copy = Plain::of_source(&root, &own.source);
        generation(
            &copy,
            3,
            "load",
            crate::saves::plain::fixture::pair(
                "driver_career_26",
                "game",
                vec![("0x00000000000000aa", "Career Save 1")],
            ),
        );
        let saved = list_save_with(&root, "default", false, &ea, || None);
        assert_eq!(keys(&saved), ["00000000000000aa"]);
        assert!(saved.source.starts_with("generation 3 of "), "{}", saved.source);
        std::fs::write(own.data_file(), b"torn").unwrap();
        let torn = list_save_with(&root, "default", false, &ea, || Some(own.clone()));
        assert_eq!(keys(&torn), ["00000000000000aa"]);
        assert!(torn.source.contains("no readable save file"), "{}", torn.source);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_vault_restore_of_the_save_changes_the_list_at_the_next_play() {
        let root = scratch("restore");
        let (ea, own) = niggo_layout(&root);
        let before = list_save_with(&root, "default", false, &ea, || Some(own.clone()));
        save_with(
            &own.dir,
            "NXDSMWW.BWW",
            ea_header(),
            vec![(
                "driver_career_26",
                vec![slot_bag((1, 0xbb), (478, 16388), (0, 3), 1_793_000_000, 0)],
            )],
        );
        let after = list_save_with(&root, "default", false, &ea, || Some(own.clone()));
        assert_eq!(keys(&after), ["00000000000000bb"]);
        assert_ne!(before.state(), after.state());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_mode_the_save_lacks_comes_from_the_generation_the_core_puts_it_back_from() {
        use crate::saves::plain::fixture::pair;
        let root = scratch("put-back");
        let steam = game_copy(&root, false);
        save_file(&crate::saves::launcher::remote_dir(&root, "p"));
        let p = Plain::at(crate::saves::plain::root_of_profile(&root, "p"));
        let mut containers = pair(
            "driver_career_progressive",
            "kept",
            vec![("0x00000000000000cc", "Career Save 1")],
        );
        containers.extend(pair(
            "driver_career_1990",
            "game",
            vec![("0x00000000000000dd", "Career Save 2")],
        ));
        generation(&p, 3, "save", containers);
        generation(
            &p,
            4,
            "load",
            pair("driver_career_progressive", "kept", Vec::new()),
        );
        let saved = list_save_with(&root, "p", true, &steam, || None);
        assert_eq!(keys(&saved), ["00000000000000cc", "604a2c4a9c422bfc"]);
        assert!(
            saved
                .source
                .contains("driver_career_progressive from generation 3"),
            "{}",
            saved.source
        );
        p.restore_mode(3, "driver_career_1990", 1).unwrap();
        let restored = list_save_with(&root, "p", true, &steam, || None);
        assert_eq!(keys(&restored), ["00000000000000cc", "00000000000000dd"]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_steam_session_lists_its_launcher_store_or_the_steam_save() {
        let root = scratch("steam");
        let steam = game_copy(&root, false);
        let remote = root.join("Steam/userdata/843502855/3059520/remote");
        let own = Location::steam(&remote).unwrap();
        save_with(
            &remote,
            crate::saves::plain::rewrite::SAVE_FILE,
            crate::saves::codec::STREAM_HEADER,
            vec![(
                "driver_career_26",
                vec![slot_bag((1, 0xee), (7, 61), (0, 0), 1_793_000_000, 0)],
            )],
        );
        save_file(&crate::saves::launcher::remote_dir(&root, "p"));
        let on = list_save_with(&root, "p", true, &steam, || Some(own.clone()));
        let off = list_save_with(&root, "p", false, &steam, || Some(own.clone()));
        assert_eq!(keys(&on), ["604a2c4a9c422bfc"]);
        assert_eq!(keys(&off), ["00000000000000ee"]);
        assert_ne!(on.state(), off.state());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_slot_without_a_driver_and_team_is_not_a_career_the_stock_screen_would_list() {
        let mut none = slot(5, "0x00000000000000e5", 16383, 16383, 0, 0, 9);
        let mut no_team = slot(6, "0x00000000000000e6", 7, 61, 0, 0, 9);
        no_team.team = None;
        let mut no_driver = slot(7, "0x00000000000000e7", 7, 61, 0, 0, 9);
        no_driver.driver = None;
        let real = slot(8, "0x00000000000000e8", 7, 61, 0, 0, 9);
        none.name = "Career Save 1".into();
        assert!(!is_career(&none) && !is_career(&no_team) && !is_career(&no_driver));
        assert!(is_career(&real));
        let mut idle = real.clone();
        idle.last_played = 0;
        assert!(!is_career(&idle));
    }
}
