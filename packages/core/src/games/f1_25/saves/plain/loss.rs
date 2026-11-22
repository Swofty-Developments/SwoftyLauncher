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
    pub mode: String,
    /// The mode's containers in the good generation, which a restore puts back together.
    pub containers: Vec<String>,
    /// No container of the mode is left in the newest generation.
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
                true => vec![crate::saves::vault::careers::mode_label(Some(&i.mode))],
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

    pub fn modes(&self) -> Vec<String> {
        self.items.iter().map(|i| i.mode.clone()).collect()
    }

    pub fn containers(&self) -> Vec<String> {
        self.items.iter().flat_map(|i| i.containers.clone()).collect()
    }

    pub fn key(&self) -> String {
        format!("g{}:{}", self.good, self.newest)
    }
}

fn halves<'a>(m: &'a Manifest, mode: &'a str) -> Vec<&'a Container> {
    m.halves(mode).collect()
}

/// A mode with careers: a container of it lists career slots, or it is a mode the core keeps for an addon no
/// loaded game mode claims, whose slots the game does not read. The game's own records (options, trophies,
/// journal entries) and modes with no career are not careers.
fn holds_careers(was: &[&Container]) -> bool {
    was.iter()
        .any(|c| !c.slots.is_empty() || (!c.registered && c.mode() != c.name))
}

/// Whether a generation lists the containers a loaded game mode read. A core on a build whose save layout is
/// not proven (the EA app copy) leaves them out and lists only the containers it keeps or puts back.
fn lists_registered(m: &Manifest) -> bool {
    m.containers.iter().any(|c| c.registered)
}

fn lost_mode(good: &Manifest, newest: &Manifest, mode: &str) -> Option<Lost> {
    let was = halves(good, mode);
    let now = halves(newest, mode);
    if !holds_careers(&was) {
        return None;
    }
    if now.iter().any(|c| c.source == Source::Injected) && now.len() >= was.len() {
        return None;
    }
    let listed = lists_registered(newest);
    let judged: Vec<&Container> = was.iter().copied().filter(|c| listed || !c.registered).collect();
    let missing_half = judged.iter().any(|g| !now.iter().any(|n| n.same(g)));
    let careers: Vec<LostCareer> = judged
        .iter()
        .filter(|g| !now.iter().any(|n| n.same(g) && !n.registered))
        .flat_map(|c| c.slots.iter())
        .filter(|s| !now.iter().any(|n| n.slots.iter().any(|t| t.guid == s.guid)))
        .fold(Vec::new(), |mut out: Vec<LostCareer>, s| {
            if !out.iter().any(|c| c.guid == s.guid) {
                out.push(LostCareer {
                    guid: s.guid.clone(),
                    name: s.name.clone(),
                    season: s.season,
                    round: s.round,
                });
            }
            out
        });
    (missing_half || !careers.is_empty()).then(|| Lost {
        mode: mode.to_owned(),
        containers: was.iter().map(|c| c.name.clone()).collect(),
        whole: now.is_empty(),
        careers,
    })
}

impl Plain {
    /// What the newest generation lacks of the last good save generation, by identity: a container by name,
    /// then a career by slot guid. The last good save generation is the newest save a core session
    /// wrote. Only a load generation newer than it can show a loss; a career gone from a later save was
    /// deleted in a session with the core, and deletions stick. A container the core put back is in the
    /// newest generation with source `injected`, and one named in its put-back log lines is not lost.
    /// Only modes with careers count. A load generation that lists no registered container judges only the
    /// kept ones, and a container the load kept unclaimed (its addon off) still holds its careers.
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
            .modes()
            .into_iter()
            .filter(|mode| {
                !good
                    .halves(mode)
                    .all(|c| put_back.iter().any(|p| p.name == c.name))
            })
            .filter_map(|mode| lost_mode(good, newest, &mode))
            .collect();
        (!items.is_empty()).then_some(Loss {
            good: good.generation,
            newest: newest.generation,
            at: good.written_at,
            items,
        })
    }
}
