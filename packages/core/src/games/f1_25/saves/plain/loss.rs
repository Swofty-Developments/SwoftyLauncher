use super::{Container, Event, Manifest, Plain, Source};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PutBack {
    pub name: String,
    pub careers: u32,
    pub generation: u64,
}

/// The `[saves] put back <name>: <k> career(s) from generation <g>` lines of a game log.
pub fn put_backs(log: &str) -> Vec<PutBack> {
    log.lines()
        .filter_map(|l| {
            let rest = l.split_once("[saves] put back ")?.1;
            let (name, tail) = rest.split_once(": ")?;
            let (k, tail) = tail.split_once(" career")?;
            let g = tail.rsplit_once("generation ")?.1.trim();
            Some(PutBack {
                name: name.trim().to_owned(),
                careers: k.trim().parse().ok()?,
                generation: g.parse().ok()?,
            })
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lost {
    pub container: String,
    pub whole: bool,
    pub careers: Vec<LostCareer>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LostCareer {
    pub guid: String,
    pub name: String,
    pub season: Option<u32>,
    pub round: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loss {
    pub good: u64,
    pub newest: u64,
    pub at: i64,
    pub items: Vec<Lost>,
}

impl Loss {
    pub fn names(&self) -> Vec<String> {
        self.items
            .iter()
            .flat_map(|i| match i.careers.is_empty() {
                true => vec![i.container.clone()],
                false => i
                    .careers
                    .iter()
                    .map(|c| match (c.season, c.round) {
                        (Some(s), Some(r)) => format!("{}, season {s}, round {r}", c.name),
                        _ => c.name.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn containers(&self) -> Vec<String> {
        self.items.iter().map(|i| i.container.clone()).collect()
    }

    pub fn key(&self) -> String {
        format!("g{}:{}", self.good, self.newest)
    }
}

fn lost_in(good: &Container, newest: Option<&Container>) -> Option<Lost> {
    let careers = |only_missing_from: Option<&Container>| -> Vec<LostCareer> {
        good.slots
            .iter()
            .filter(|s| match only_missing_from {
                Some(n) => !n.slots.iter().any(|t| t.guid == s.guid),
                None => true,
            })
            .map(|s| LostCareer {
                guid: s.guid.clone(),
                name: s.name.clone(),
                season: s.season,
                round: s.round,
            })
            .collect()
    };
    match newest {
        None => Some(Lost {
            container: good.name.clone(),
            whole: true,
            careers: careers(None),
        }),
        Some(n) if n.source == Source::Injected => None,
        Some(n) => {
            let missing = careers(Some(n));
            (!missing.is_empty() && !good.slots.is_empty()).then(|| Lost {
                container: good.name.clone(),
                whole: false,
                careers: missing,
            })
        }
    }
}

impl Plain {
    /// What the newest generation lacks of the last good save generation, by identity: a container by name,
    /// then a career by slot guid. The last good save generation is the newest save a core session
    /// wrote. Only a load generation newer than it can show a loss; a career gone from a later save was
    /// deleted in a session with the core, and deletions stick. A container the core put back is in the
    /// newest generation with source `injected`, and one named in its put-back log lines is not lost.
    pub fn loss(&self, put_back: &[PutBack]) -> Option<Loss> {
        let gens = self.generations();
        let newest = gens.first()?;
        if newest.event != Event::Load {
            return None;
        }
        let good = gens
            .iter()
            .find(|m| m.is_session_save() && m.generation < newest.generation)?;
        self.loss_between(good, newest, put_back)
    }

    pub fn loss_between(&self, good: &Manifest, newest: &Manifest, put_back: &[PutBack]) -> Option<Loss> {
        let items: Vec<Lost> = good
            .containers
            .iter()
            .filter(|g| !put_back.iter().any(|p| p.name == g.name))
            .filter_map(|g| lost_in(g, newest.containers.iter().find(|n| n.same(g))))
            .collect();
        (!items.is_empty()).then(|| Loss {
            good: good.generation,
            newest: newest.generation,
            at: good.written_at,
            items,
        })
    }
}
