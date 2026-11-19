use std::path::Path;

use swofty_core::saves::plain::{Event, Plain, Source, inject};
use swofty_i18n::{tr, trf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Older {
    pub(super) generation: u64,
    pub(super) at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlainCareer {
    pub(super) container: String,
    pub(super) guid: String,
    pub(super) name: String,
    pub(super) place: Option<String>,
    pub(super) last_played: i64,
    pub(super) source: Source,
    pub(super) waiting: bool,
    pub(super) older: Vec<Older>,
}

impl PlainCareer {
    pub(super) fn lives(&self) -> String {
        match self.source {
            Source::Game => trf!("In the game's save · {container}", container = self.container),
            Source::Kept => trf!("Kept by the launcher · {container}", container = self.container),
            Source::Injected => trf!(
                "Put back by the launcher · {container}",
                container = self.container
            ),
            Source::Other => self.container.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlainView {
    pub(super) source: String,
    pub(super) generation: u64,
    pub(super) written_at: i64,
    pub(super) event: Event,
    pub(super) careers: Vec<PlainCareer>,
    pub(super) answers: Vec<inject::Answer>,
}

impl PlainView {
    pub(super) fn answer_line(&self) -> Option<String> {
        let a = self.answers.last()?;
        Some(match a.ok {
            true => trf!("The game applied the last restore: {detail}", detail = a.detail),
            false => trf!(
                "The game could not apply the last restore: {detail}",
                detail = a.detail
            ),
        })
    }

    pub(super) fn waiting_line(&self) -> Option<&'static str> {
        self.careers
            .iter()
            .any(|c| c.waiting)
            .then(|| tr("A restore is waiting. The game applies it the next time you press Play."))
    }
}

fn place(season: Option<u32>, round: Option<u32>) -> Option<String> {
    match (season, round) {
        (Some(s), Some(r)) => Some(trf!("Season {season} · Round {round}", season = s, round = r)),
        (Some(s), None) => Some(trf!("Season {season}", season = s)),
        _ => None,
    }
}

const OLDER: usize = 6;

pub(super) fn build_plain(root: &Path, source: &str) -> Option<PlainView> {
    let plain = Plain::of_source(root, source);
    let gens = plain.generations();
    let newest = gens.first()?;
    let pending = plain.pending();
    let careers = plain
        .careers()
        .into_iter()
        .map(|row| {
            let sha_now = newest.container(&row.container).map(|c| c.sha256.clone());
            let mut seen = vec![sha_now];
            let mut older = Vec::new();
            for m in gens.iter().skip(1) {
                if older.len() >= OLDER {
                    break;
                }
                let Some(c) = m.container(&row.container) else {
                    continue;
                };
                let sha = Some(c.sha256.clone());
                if seen.contains(&sha) {
                    continue;
                }
                seen.push(sha);
                older.push(Older {
                    generation: m.generation,
                    at: m.written_at,
                });
            }
            PlainCareer {
                waiting: pending.iter().any(|r| r.container == row.container),
                place: place(row.slot.season, row.slot.round),
                last_played: row.slot.last_played,
                name: row.slot.name,
                guid: row.slot.guid,
                container: row.container,
                source: row.source,
                older,
            }
        })
        .collect();
    Some(PlainView {
        source: source.to_owned(),
        generation: newest.generation,
        written_at: newest.written_at,
        event: newest.event,
        careers,
        answers: plain.answers(),
    })
}
