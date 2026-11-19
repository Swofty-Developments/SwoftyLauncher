//! The rows of the in-game Resume Career list: one per career the save slots back, newest first, with what each
//! row shows and whether pressing it loads the career at once.

use serde::{Deserialize, Serialize};

use super::{Career, CareerNeeds};
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
    pub ready: bool,
    /// `ready`, `missing` (an addon or rows the build lacks) or `version` (made with another addon version).
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
}

/// Whether the Resume Career list screen is on: `SWOFTY_RESUME_LIST=1`.
pub fn wanted() -> bool {
    std::env::var("SWOFTY_RESUME_LIST").is_ok_and(|v| v == "1")
}

fn pill(n: &CareerNeeds) -> (String, &'static str) {
    let mut parts: Vec<String> = Vec::new();
    let mut severity = READY;
    for a in &n.addons {
        if !a.on {
            parts.push(format!("Missing: {}", a.name));
            severity = MISSING;
        } else if !a.at_version {
            parts.push(format!("Made with another version of {}", a.name));
            if severity == READY {
                severity = VERSION;
            }
        }
    }
    if let Some(m) = &n.missing_rows {
        parts.push(m.text());
        severity = MISSING;
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

fn number(c: &Career, key: &str) -> Option<i64> {
    c.extra.get(key).and_then(serde_json::Value::as_i64)
}

/// The list rows for `needs` (see `resume_needs`), newest first, named through `names`.
pub fn resume_rows(list: &[Career], needs: &[CareerNeeds], names: &Names) -> Vec<ResumeRow> {
    let mut rows: Vec<ResumeRow> = needs
        .iter()
        .filter_map(|n| {
            let c = list.iter().find(|c| c.field("slot") == Some(n.key.as_str()))?;
            let my_team = c.field("mode") == Some("my_team") || c.career.starts_with("my_team");
            let badge = match (n.mode_label.as_str(), my_team) {
                ("", true) => "My Team".to_owned(),
                ("", false) => "Driver Career".to_owned(),
                (label, _) => label.to_owned(),
            };
            let fallback = c.field("slot_name").unwrap_or(&badge).to_owned();
            let driver = number(c, "driver_raw")
                .and_then(|id| names.driver_named(id))
                .unwrap_or(fallback);
            let team = number(c, "team_raw").and_then(|id| names.team_named(id));
            let (status, severity) = pill(n);
            Some(ResumeRow {
                index: 0,
                mode: if my_team { "my_team" } else { "driver" }.into(),
                key: n.key.clone(),
                ready: severity == READY,
                severity: severity.into(),
                status,
                badge,
                driver,
                colour: team.as_ref().and_then(|t| t.1),
                team: team.map(|t| t.0).unwrap_or_default(),
                season: number(c, "slot_season"),
                race: number(c, "slot_weekend").map(|w| w + 1),
                played: number(c, "last_played"),
            })
        })
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.played));
    for (i, r) in rows.iter_mut().enumerate() {
        r.index = i;
    }
    rows
}

/// The needs and rows of the list for the profile `order` loads, read from the career index in the data folder
/// and named from the database the build wrote in `build`, with its id `aliases`. `None` when that database
/// cannot be read.
pub fn resume_list_for(
    store: &crate::store::Store,
    game: &crate::install::Game,
    build: &std::path::Path,
    order: &[&crate::manifest::AddonManifest],
    aliases: crate::row_ids::IdAliases,
) -> Option<(Vec<CareerNeeds>, Vec<ResumeRow>)> {
    let names = Names::of_build(store, game, build, aliases)?;
    let installed = store.addons().ok()?;
    let on: Vec<String> = order.iter().map(|m| m.id.clone()).collect();
    let careers = super::read(&super::data_path(&store.root));
    let needs = super::resume_needs(&careers, &on, &installed, Some(names.rows()));
    let rows = resume_rows(&careers, &needs, &names);
    Some((needs, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stdlib::careers::{MissingRows, ResumeNeed, parse};
    use swofty_formats::gvdb::{Gvdb, Record};

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
        }
    }

    fn addon(name: &str, on: bool, at_version: bool) -> ResumeNeed {
        ResumeNeed {
            id: name.into(),
            name: name.into(),
            version: "1".into(),
            on,
            at_version,
        }
    }

    #[test]
    fn rows_come_newest_first_with_their_status_and_names() {
        let list = parse(
            r#"[{"career":"driver_career:a","slot":"a","mode":"driver_career","team_raw":7,"driver_raw":61,"slot_season":2,"slot_weekend":3,"last_played":1700000000},
                {"career":"my_team:b","slot":"b","mode":"my_team","team_raw":16384,"driver_raw":16386,"slot_name":"My Team Save 1","last_played":1800000000},
                {"career":"driver_career:c","slot":"c","mode":"driver_career","team_raw":503,"driver_raw":61,"last_played":1750000000},
                {"career":"driver_career:d","slot":"d","mode":"driver_career","team_raw":7,"driver_raw":61,"last_played":1600000000}]"#,
        );
        let needs = vec![
            need("a", vec![], None),
            need(
                "b",
                vec![addon("Alpha", false, false), addon("Beta", true, false)],
                None,
            ),
            need(
                "c",
                vec![],
                Some(MissingRows {
                    addon: "F1 1990".into(),
                    tables: vec!["team".into()],
                }),
            ),
            need("d", vec![addon("Beta", true, false)], None),
        ];
        let names = Names::from_db(db(), Some(&english()));
        let rows = resume_rows(&list, &needs, &names);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["b", "c", "a", "d"]);
        assert_eq!(rows.iter().map(|r| r.index).collect::<Vec<_>>(), [0, 1, 2, 3]);
        assert_eq!(rows[0].mode, "my_team");
        assert_eq!(rows[0].badge, "My Team");
        assert_eq!(
            rows[0].status,
            "Missing: Alpha; Made with another version of Beta"
        );
        assert_eq!((rows[0].ready, rows[0].severity.as_str()), (false, MISSING));
        assert_eq!(rows[0].driver, "My Team Save 1");
        assert_eq!(rows[1].status, "Missing rows from F1 1990");
        assert_eq!(rows[1].team, "", "a team the build lacks has no name");
        assert!(rows[2].ready && rows[2].status == "Ready");
        assert_eq!(rows[2].driver, "Nigel Mansell");
        assert_eq!((rows[2].season, rows[2].race), (Some(2), Some(4)));
        assert_eq!(rows[2].colour, Some(0xd2be));
        assert_eq!((rows[3].ready, rows[3].severity.as_str()), (false, VERSION));
        assert_eq!(rows[3].status, "Made with another version of Beta");
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
        let list = parse(
            r#"[{"career":"driver_career:a","slot":"a","mode":"driver_career","team_raw":503,"driver_raw":61,"last_played":2},
                {"career":"driver_career:b","slot":"b","mode":"driver_career","team_raw":513,"driver_raw":61,"last_played":1}]"#,
        );
        let mut names = Names::from_db(db(), Some(&english()));
        let needs = vec![need("a", vec![], None), need("b", vec![], None)];
        assert_eq!(resume_rows(&list, &needs, &names)[0].team, "");
        let mut aliases = crate::row_ids::IdAliases::default();
        aliases.tables.insert("teams".into(), [(503, 646)].into());
        names = names.with_aliases(aliases);
        let rows = resume_rows(&list, &needs, &names);
        assert_eq!(rows[0].team, "Team");
        assert_eq!(rows[0].colour, Some(0x0000_00ff));
        assert_eq!(rows[1].team, "");
    }

    #[test]
    fn dates_are_day_month_year() {
        assert_eq!(date_text(0), "01/01/1970");
        assert_eq!(date_text(1_795_093_120), "19/11/2026");
        assert_eq!(date_text(951_782_400), "29/02/2000");
    }
}
