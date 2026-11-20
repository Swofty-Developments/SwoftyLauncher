use std::collections::HashMap;
use std::path::Path;

use swofty_formats::gvdb::Gvdb;

use super::careers::Listed;
use crate::dbedit::Catalog;
use crate::install::Game;
use crate::row_ids::IdAliases;
use crate::stdlib::careers::RowSet;
use crate::store::Store;

const TEAMS: (&str, &str) = ("entrants.teams", "team");
const DRIVERS: (&str, &str) = ("entrants.drivers", "driver");
const VEHICLES: (&str, &str) = ("entrants.vehicles", "vehicle");
const ID_BITS: i64 = 0x3fff;
const CONTENT_KEYS: [(u32, &str); 3] = [(25, "lng_f1_25"), (26, "lng_f1_2026"), (1990, "lng_f1_1990")];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Names {
    teams: HashMap<i64, String>,
    drivers: HashMap<i64, String>,
    colours: HashMap<i64, u32>,
    content: HashMap<u32, String>,
    rows: RowSet,
}

fn stat_ids(db: &Gvdb, (container, row): (&str, &str)) -> std::collections::BTreeSet<i64> {
    (0..db.instance_count(container, row).unwrap_or(0))
        .filter_map(|i| {
            db.get_text(&format!("{container}.{row}_{i}.stat_id"))
                .and_then(|t| t.trim().parse::<i64>().ok())
        })
        .collect()
}

fn team_colours(db: &Gvdb) -> HashMap<i64, u32> {
    let mut out = HashMap::new();
    for i in 0..db.instance_count(TEAMS.0, TEAMS.1).unwrap_or(0) {
        let path = format!("{}.{}_{i}", TEAMS.0, TEAMS.1);
        let id = db
            .get_text(&format!("{path}.stat_id"))
            .and_then(|t| t.trim().parse::<i64>().ok());
        let argb = db
            .get_text(&format!("{path}.ui_team_colour"))
            .and_then(|t| t.trim().parse::<i32>().ok());
        if let (Some(id), Some(argb)) = (id, argb) {
            out.entry(id).or_insert(argb as u32 & 0x00ff_ffff);
        }
    }
    out
}

fn table(cat: &Catalog, db: &Gvdb, (container, row): (&str, &str)) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    for i in 0..db.instance_count(container, row).unwrap_or(0) {
        let path = format!("{container}.{row}_{i}");
        let Some(id) = db
            .get_text(&format!("{path}.stat_id"))
            .and_then(|t| t.trim().parse::<i64>().ok())
        else {
            continue;
        };
        let name = cat
            .find(&path)
            .and_then(|n| cat.node(n).display.clone())
            .filter(|n| !n.is_empty());
        if let Some(name) = name {
            out.entry(id).or_insert(name);
        }
    }
    out
}

fn lookup(map: &HashMap<i64, String>, raw: i64) -> Option<&String> {
    match raw >> 14 {
        0 => map.get(&raw),
        _ => map.get(&raw).or_else(|| map.get(&(raw & ID_BITS))),
    }
}

#[derive(serde::Deserialize)]
struct Assets {
    #[serde(default)]
    db_edits: Vec<Edit>,
}

#[derive(serde::Deserialize)]
struct Edit {
    hash: String,
    #[serde(rename = "type", default = "text_tag")]
    tag: u8,
    value: String,
}

fn text_tag() -> u8 {
    2
}

/// Put the runtime database edits of an overlay's `assets.json` over `db`, the way the game loads it:
/// a record the edit names takes its value, and one the database lacks is added.
pub fn apply_edits(db: &mut Gvdb, assets_json: &[u8]) {
    let Ok(assets) = serde_json::from_slice::<Assets>(assets_json) else {
        return;
    };
    // Last match: the game loads a key's last record over its first (Gdb_HashMap_Insert updates in place).
    let mut at: HashMap<u64, usize> = db.records.iter().enumerate().map(|(i, r)| (r.key, i)).collect();
    for e in assets.db_edits {
        let Ok(key) = u64::from_str_radix(&e.hash, 16) else {
            continue;
        };
        match at.get(&key) {
            Some(&i) => db.records[i].value = e.value.into_bytes(),
            None => {
                at.insert(key, db.records.len());
                db.records.push(swofty_formats::gvdb::Record {
                    key,
                    tag: e.tag,
                    value: e.value.into_bytes(),
                });
            }
        }
    }
}

fn is_none(raw: i64) -> bool {
    raw & ID_BITS == ID_BITS
}

type FileKey = Option<(std::path::PathBuf, Option<std::time::SystemTime>, u64)>;

static LOADED: std::sync::Mutex<Option<(Vec<FileKey>, Names)>> = std::sync::Mutex::new(None);

fn file_key(p: &Path) -> (std::path::PathBuf, Option<std::time::SystemTime>, u64) {
    let meta = std::fs::metadata(p).ok();
    (
        p.to_path_buf(),
        meta.as_ref().and_then(|m| m.modified().ok()),
        meta.map_or(0, |m| m.len()),
    )
}

const BUILD_LANG: [&str; 2] = ["swofty/assets/lang/all.lng", "swofty/assets/lang/eng.lng"];

/// `english` with the text the addons of `build` add to the game's English table (the all-languages table, then
/// the English one), so the names of rows an addon adds resolve.
pub(crate) fn with_build_lang(
    english: Option<Vec<u8>>,
    build: &Path,
    pending: Option<&swofty_formats::lng::Lng>,
) -> Option<Vec<u8>> {
    let extra: Vec<swofty_formats::lng::Lng> = BUILD_LANG
        .iter()
        .filter_map(|rel| std::fs::read(build.join(rel)).ok())
        .filter_map(|b| swofty_formats::lng::Lng::parse(&b).ok())
        .chain(pending.cloned())
        .collect();
    if extra.is_empty() {
        return english;
    }
    let mut lng = swofty_formats::lng::Lng::parse(&english?).ok()?;
    for e in extra {
        lng.entries.extend(e.entries);
    }
    Some(lng.to_bytes())
}

impl Names {
    pub fn from_db(db: Gvdb, english: Option<&[u8]>) -> Self {
        let mut cat = Catalog::from_gvdb(db.clone(), String::new());
        if let Some(english) = english {
            let _ = cat.set_lang(english);
        }
        let rows = RowSet {
            teams: stat_ids(&db, TEAMS),
            vehicles: stat_ids(&db, VEHICLES),
            drivers: stat_ids(&db, DRIVERS),
            aliases: IdAliases::default(),
        };
        let content = english
            .and_then(|b| swofty_formats::lng::Lng::parse(b).ok())
            .map(|lng| {
                CONTENT_KEYS
                    .iter()
                    .filter_map(|(year, key)| Some((*year, lng.get(key)?.to_owned())))
                    .collect()
            })
            .unwrap_or_default();
        Self {
            teams: table(&cat, &db, TEAMS),
            drivers: table(&cat, &db, DRIVERS),
            colours: team_colours(&db),
            content,
            rows,
        }
    }

    /// The game's own name of the content a career mode's year (`25`, `26`, `1990`) belongs to.
    pub fn content_name(&self, year: u32) -> Option<&str> {
        self.content.get(&year).map(String::as_str)
    }

    /// The names of the database a build wrote in `build` (its `game_package` database and runtime edits, else
    /// the game's own database), with the build's id `aliases`.
    pub fn of_build(
        store: &Store,
        game: &Game,
        build: &Path,
        aliases: IdAliases,
        pending: Option<&swofty_formats::lng::Lng>,
    ) -> Option<Self> {
        let db_path = build.join(crate::build::GVDB);
        let path = match db_path.is_file() {
            true => db_path,
            false => crate::dbedit::stock_database(store, game).ok()?,
        };
        let mut db = Gvdb::parse(&std::fs::read(&path).ok()?).ok()?;
        if let Ok(edits) = std::fs::read(build.join(crate::content::runtime_assets::ASSETS_FILE)) {
            apply_edits(&mut db, &edits);
        }
        let english =
            crate::game_folder::baseline::original_lang(&store.root, &game.dir, &game.identity, "eng").ok();
        let english = english.and_then(|p| std::fs::read(p).ok());
        let english = with_build_lang(english, build, pending);
        let mut names = Self::from_db(db, english.as_deref());
        names.rows.aliases = aliases;
        Some(names)
    }

    pub fn with_aliases(mut self, aliases: IdAliases) -> Self {
        self.rows.aliases = aliases;
        self
    }

    /// The names with the rows of `added` (by table) the game holds once it has loaded besides the database file's.
    pub fn with_rows(
        mut self,
        added: &std::collections::BTreeMap<String, std::collections::BTreeSet<i64>>,
    ) -> Self {
        self.rows.add(added);
        self
    }

    pub fn rows(&self) -> &RowSet {
        &self.rows
    }

    /// The team a save's id names in the build, through its aliases, with its UI colour (RGB).
    pub fn team_named(&self, raw: i64) -> Option<(String, Option<u32>)> {
        let id = self.rows.resolve("team_raw", raw)?;
        let name = self.teams.get(&id)?.clone();
        Some((name, self.colours.get(&id).copied()))
    }

    /// The driver a save's id names in the build, through its aliases.
    pub fn driver_named(&self, raw: i64) -> Option<String> {
        let id = self.rows.resolve("driver_raw", raw)?;
        self.drivers.get(&id).cloned()
    }

    pub fn load(store: &Store, game: &Game, profile: Option<&str>) -> Option<Self> {
        let built =
            profile.and_then(|p| crate::row_ids::last_build(&store.overlays_dir(p), "", crate::build::GVDB));
        let path = match built.as_ref().and_then(|b| b.db.clone()) {
            Some(p) => p,
            None => crate::dbedit::stock_database(store, game).ok()?,
        };
        let edits = built.and_then(|b| b.edits);
        let english =
            crate::game_folder::baseline::original_lang(&store.root, &game.dir, &game.identity, "eng").ok();
        let key: Vec<FileKey> = [Some(&path), edits.as_ref(), english.as_ref()]
            .into_iter()
            .map(|p| p.map(|p| file_key(p)))
            .collect();
        let mut cache = LOADED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, names)) = cache.as_ref()
            && *k == key
        {
            return Some(names.clone());
        }
        let mut db = Gvdb::parse(&std::fs::read(&path).ok()?).ok()?;
        if let Some(edits) = edits.and_then(|p| std::fs::read(p).ok()) {
            apply_edits(&mut db, &edits);
        }
        let english = english.and_then(|p| std::fs::read(p).ok());
        let names = Self::from_db(db, english.as_deref());
        *cache = Some((key, names.clone()));
        Some(names)
    }

    pub fn team(&self, raw: i64) -> String {
        match lookup(&self.teams, raw) {
            Some(n) => n.clone(),
            None => format!("Team #{}", raw & ID_BITS),
        }
    }

    pub fn driver(&self, raw: i64) -> String {
        match lookup(&self.drivers, raw) {
            Some(n) => n.clone(),
            None => format!("Driver #{}", raw & ID_BITS),
        }
    }

    pub fn name(&self, list: &mut [Listed]) {
        for c in list {
            if c.driver.is_none()
                && let Some(id) = c.driver_id.filter(|i| !is_none(*i))
            {
                c.driver = Some(self.driver(id));
            }
            if c.team.is_none()
                && let Some(id) = c.team_id.filter(|i| !is_none(*i))
            {
                c.team = Some(self.team(id));
            }
        }
    }
}

pub fn read_named_listing(data: &Path, store: &Store, game: &Game, profile: Option<&str>) -> Vec<Listed> {
    let mut list = super::careers::read_listing(data, profile);
    if list.iter().any(|c| c.driver_id.is_some() || c.team_id.is_some()) {
        match Names::load(store, game, profile) {
            Some(names) => names.name(&mut list),
            None => Names::default().name(&mut list),
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stdlib::careers::parse;
    use swofty_formats::gvdb::Record;
    use swofty_formats::lng::{Entry, Lng};

    fn db() -> Gvdb {
        let mut r = vec![
            Record::new("entrants.teams._team_instancecount_", 0, "2"),
            Record::new("entrants.drivers._driver_instancecount_", 0, "2"),
        ];
        for (i, (label, id, title)) in [
            ("mclaren_26", "7", "lng_team_mcl"),
            ("ferrari_26", "3", "lng_team_fer"),
        ]
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
                *title,
            ));
        }
        for (i, (label, id, first, last)) in [
            ("oscar_piastri", "5", "lng_oscar", "lng_piastri"),
            ("charles_leclerc", "16", "lng_charles", "lng_leclerc"),
        ]
        .iter()
        .enumerate()
        {
            r.push(Record::new(
                &format!("entrants.drivers.driver_{i}._metadata_"),
                2,
                *label,
            ));
            r.push(Record::new(
                &format!("entrants.drivers.driver_{i}.stat_id"),
                0,
                *id,
            ));
            r.push(Record::new(
                &format!("entrants.drivers.driver_{i}.first_name"),
                2,
                *first,
            ));
            r.push(Record::new(
                &format!("entrants.drivers.driver_{i}.last_name"),
                2,
                *last,
            ));
        }
        Gvdb::new(r)
    }

    fn english() -> Vec<u8> {
        let e = |k: &str, v: &str| Entry {
            key: k.into(),
            value: v.into(),
        };
        Lng {
            entries: vec![
                e("lng_team_mcl", "McLaren"),
                e("lng_team_fer", "Ferrari"),
                e("lng_oscar", "Oscar"),
                e("lng_piastri", "Piastri"),
                e("lng_charles", "Charles"),
                e("lng_leclerc", "Leclerc"),
            ],
        }
        .to_bytes()
    }

    fn index() -> Vec<Listed> {
        super::super::careers::listing(
            &parse(
                r#"{"careers":[
                {"career":"a","slot":"a","driver_raw":5,"team_raw":7,"last_played":3},
                {"career":"b","slot":"b","driver_raw":16,"team_raw":16387,"last_played":2},
                {"career":"c","slot":"c","driver_raw":99,"team_raw":99,"last_played":1},
                {"career":"d","slot":"d","driver":"Kept Name","team":"Kept Team","driver_raw":5,"team_raw":7},
                {"career":"e","slot":"e","last_played":0}
                ]}"#,
            ),
            None,
        )
    }

    #[test]
    fn resolves_ids_to_names_from_the_database() {
        let names = Names::from_db(db(), Some(&english()));
        let mut l = index();
        names.name(&mut l);
        let by = |id: &str| l.iter().find(|c| c.career == id).unwrap().clone();
        assert_eq!(by("a").title(), "Oscar Piastri, McLaren");
        assert_eq!(by("b").title(), "Charles Leclerc, Ferrari");
        assert_eq!(by("d").title(), "Kept Name, Kept Team");
        assert_eq!(by("e").driver, None);
    }

    #[test]
    fn unknown_ids_fall_back_to_numbers() {
        let mut l = index();
        Names::from_db(db(), Some(&english())).name(&mut l);
        let c = l.iter().find(|c| c.career == "c").unwrap();
        assert_eq!(c.title(), "Driver #99, Team #99");
        let mut l = index();
        Names::default().name(&mut l);
        let a = l.iter().find(|c| c.career == "a").unwrap();
        assert_eq!(a.title(), "Driver #5, Team #7");
    }

    #[test]
    fn a_database_with_no_english_text_falls_back_too() {
        let mut l = index();
        Names::from_db(db(), None).name(&mut l);
        let a = l.iter().find(|c| c.career == "a").unwrap();
        assert_eq!(a.title(), "Driver #5, Team #7");
    }

    #[test]
    fn runtime_edits_add_the_rows_a_season_addon_brings() {
        use swofty_formats::hash::fnv1a64;
        let mut base = db();
        let edit = |path: &str, tag: u8, value: &str| serde_json::json!({ "hash": format!("{:016x}", fnv1a64(path)), "type": tag, "value": value });
        let assets = serde_json::json!({ "db_edits": [
            edit("entrants.teams._team_instancecount_", 0, "3"),
            edit("entrants.teams.team_2._metadata_", 2, "haas_26"),
            edit("entrants.teams.team_2.stat_id", 0, "504"),
            edit("entrants.teams.team_2.display_name", 2, "lng_team_haas"),
            edit("entrants.drivers._driver_instancecount_", 0, "3"),
            edit("entrants.drivers.driver_2._metadata_", 2, "added_driver"),
            edit("entrants.drivers.driver_2.stat_id", 0, "104"),
            edit("entrants.drivers.driver_2.first_name", 2, "lng_added"),
            edit("entrants.drivers.driver_2.last_name", 2, "lng_driver"),
        ]});
        apply_edits(&mut base, assets.to_string().as_bytes());
        let mut lng = Lng::parse(&english()).unwrap();
        lng.entries.push(Entry {
            key: "lng_team_haas".into(),
            value: "Haas".into(),
        });
        lng.entries.push(Entry {
            key: "lng_added".into(),
            value: "Added".into(),
        });
        lng.entries.push(Entry {
            key: "lng_driver".into(),
            value: "Driver".into(),
        });
        let names = Names::from_db(base, Some(&lng.to_bytes()));
        assert_eq!(names.driver(104), "Added Driver");
        assert_eq!(names.team(504 & 0x3fff), "Haas");
    }

    #[test]
    fn the_none_id_names_nothing() {
        let l = super::super::careers::listing(
            &parse(
                r#"{"careers":[{"career":"driver_career:n1","slot":"n1","driver_raw":16383,"team_raw":16383}]}"#,
            ),
            None,
        );
        let mut l = l;
        Names::from_db(db(), Some(&english())).name(&mut l);
        assert_eq!(l[0].driver, None);
        assert_eq!(l[0].title(), "New career");
    }
}
