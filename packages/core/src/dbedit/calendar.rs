//! Calendar pages: which season of Track settings dates a track list, and the rounds a lowered event count cuts.

use swofty_formats::gvdb::Gvdb;
use swofty_formats::hash::fnv1a64;

const LISTS: &str = "schedule.track_lists";
const LIST: &str = "schedule.track_lists.track_event_list";

/// The Track settings season whose weekend dates order a track list's rounds:
/// `F1_2026_full` -> `f1_2026`, `F2_2024_8` -> `f2_2024`. Lists without a year in the name have none.
pub fn season_of(list: &str) -> Option<String> {
    let mut parts = list.split('_');
    let series = parts.next()?.to_ascii_lowercase();
    let year = parts.next()?;
    let ok =
        (series == "f1" || series == "f2") && year.len() == 4 && year.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| format!("{series}_{year}"))
}

/// A track list whose Events count is lower than the rounds it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutRounds {
    /// The `_fixed_event_reference_instancecount_` key.
    pub key: u64,
    pub list: String,
    pub count: usize,
    pub filled: usize,
    /// (round number, event) of each round the count drops.
    pub cut: Vec<(usize, String)>,
}

impl CutRounds {
    pub fn text(&self) -> String {
        let rounds = match self.cut.as_slice() {
            [(n, _)] => format!("round {n}"),
            [(a, _), .., (b, _)] => format!("rounds {a} to {b}"),
            [] => "no rounds".to_owned(),
        };
        let names: Vec<&str> = self.cut.iter().map(|(_, e)| e.as_str()).collect();
        format!(
            "{}: Events is {} but the list holds {} rounds, so the game drops {rounds} ({})",
            self.list,
            self.count,
            self.filled,
            names.join(", ")
        )
    }

    pub fn line(&self, by: &str) -> String {
        format!("{by}: {}", self.text())
    }
}

/// The rounds of `list` the count drops, given the event of each round in order.
pub fn cut_of(list: &str, count: usize, events: &[String]) -> Option<CutRounds> {
    (count < events.len()).then(|| CutRounds {
        key: fnv1a64(format!("{list}._fixed_event_reference_instancecount_")),
        list: list.to_owned(),
        count,
        filled: events.len(),
        cut: events
            .iter()
            .enumerate()
            .skip(count)
            .map(|(i, e)| (i + 1, e.clone()))
            .collect(),
    })
}

/// Every track list of `db` whose count drops rounds that have rows.
pub fn cut_rounds(db: &Gvdb) -> Vec<CutRounds> {
    let mut out = Vec::new();
    for i in 0..db.instance_count(LISTS, "track_event_list").unwrap_or(0) {
        let list = format!("{LIST}_{i}");
        let Some(count) = db.instance_count(&list, "fixed_event_reference") else {
            continue;
        };
        let mut events = Vec::new();
        while let Some(e) = db.get_text(&format!(
            "{list}.fixed_event_reference_{}.event_name",
            events.len()
        )) {
            events.push(e);
        }
        let label = db
            .get_text(&format!("{list}._metadata_"))
            .unwrap_or_else(|| list.clone());
        if let Some(mut cut) = cut_of(&list, count, &events) {
            cut.list = label;
            out.push(cut);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use swofty_formats::gvdb::Record;

    #[test]
    fn a_track_list_names_its_track_settings_season() {
        assert_eq!(season_of("F1_2026_full").as_deref(), Some("f1_2026"));
        assert_eq!(season_of("F1_2026_16").as_deref(), Some("f1_2026"));
        assert_eq!(season_of("F2_2024_8").as_deref(), Some("f2_2024"));
        assert_eq!(season_of("F1_career_beta"), None);
        assert_eq!(season_of("F1CG_Test_TrackList"), None);
        assert_eq!(season_of(""), None);
    }

    fn list(label: &str, count: usize, events: &[&str]) -> Gvdb {
        let mut r = vec![
            Record::new("schedule.track_lists._track_event_list_instancecount_", 0, "1"),
            Record::new(&format!("{LIST}_0._metadata_"), 2, label),
            Record::new(
                &format!("{LIST}_0._fixed_event_reference_instancecount_"),
                0,
                count.to_string(),
            ),
        ];
        for (i, e) in events.iter().enumerate() {
            r.push(Record::new(
                &format!("{LIST}_0.fixed_event_reference_{i}.event_name"),
                4,
                *e,
            ));
        }
        Gvdb { records: r }
    }

    #[test]
    fn a_lowered_count_names_the_rounds_it_cuts() {
        let db = list("F1_2026_full", 2, &["rw_a", "rw_b", "rw_c", "rw_d"]);
        let cut = cut_rounds(&db);
        assert_eq!(cut.len(), 1);
        assert_eq!(cut[0].cut, vec![(3, "rw_c".to_owned()), (4, "rw_d".to_owned())]);
        assert_eq!(
            cut[0].line("My calendar"),
            "My calendar: F1_2026_full: Events is 2 but the list holds 4 rounds, so the game drops rounds 3 to 4 (rw_c, rw_d)"
        );
        assert_eq!(
            cut[0].key,
            fnv1a64(format!("{LIST}_0._fixed_event_reference_instancecount_"))
        );
    }

    #[test]
    fn one_cut_round_and_a_full_count_read_right() {
        let one = cut_rounds(&list("L", 2, &["a", "b", "c"]));
        assert!(one[0].text().contains("drops round 3 (c)"), "{}", one[0].text());
        assert!(cut_rounds(&list("L", 3, &["a", "b", "c"])).is_empty());
        assert!(cut_rounds(&list("L", 5, &["a", "b", "c"])).is_empty());
    }
}
