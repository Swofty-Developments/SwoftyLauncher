use std::collections::HashMap;
use std::path::Path;

use swofty_formats::gvdb::Gvdb;

use super::careers::Listed;
use crate::dbedit::Catalog;
use crate::install::Game;
use crate::store::Store;

const TEAMS: (&str, &str) = ("entrants.teams", "team");
const DRIVERS: (&str, &str) = ("entrants.drivers", "driver");
const ID_BITS: i64 = 0x3fff;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Names {
    teams: HashMap<i64, String>,
    drivers: HashMap<i64, String>,
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

impl Names {
    pub fn from_db(db: Gvdb, english: Option<&[u8]>) -> Self {
        let mut cat = Catalog::from_gvdb(db.clone(), String::new());
        if let Some(english) = english {
            let _ = cat.set_lang(english);
        }
        Self {
            teams: table(&cat, &db, TEAMS),
            drivers: table(&cat, &db, DRIVERS),
        }
    }

    pub fn load(store: &Store, game: &Game, profile: Option<&str>) -> Option<Self> {
        let built = profile.and_then(|p| {
            crate::row_ids::last_build(&store.overlays_dir(p), "", crate::build::GVDB).and_then(|b| b.db)
        });
        let path = match built {
            Some(p) => p,
            None => crate::dbedit::stock_database(store, game).ok()?,
        };
        let db = Gvdb::parse(&std::fs::read(&path).ok()?).ok()?;
        let english = crate::baseline::original_lang(&store.root, &game.dir, &game.identity, "eng")
            .ok()
            .and_then(|p| std::fs::read(p).ok());
        Some(Self::from_db(db, english.as_deref()))
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
                && let Some(id) = c.driver_id
            {
                c.driver = Some(self.driver(id));
            }
            if c.team.is_none()
                && let Some(id) = c.team_id
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
        Gvdb { records: r }
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
                {"career":"a","driver_raw":5,"team_raw":7,"last_played":3},
                {"career":"b","driver_raw":16,"team_raw":16387,"last_played":2},
                {"career":"c","driver_raw":99,"team_raw":99,"last_played":1},
                {"career":"d","driver":"Kept Name","team":"Kept Team","driver_raw":5,"team_raw":7},
                {"career":"e","last_played":0}
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
}
