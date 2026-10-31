use crate::game::{GameData, List};
use crate::model::{
    Driver, FIRST_DRIVER_ID, FIRST_PRESET_ID, LAST_DRIVER_ID, MAX_DRIVERS, Roster, STATS, TRAITS,
    driver_id_usable,
};

pub const NAME_MAX: usize = 24;
pub const SEASONS_MAX: u32 = 30;
pub const BIRTH_YEARS: (u32, u32) = (1950, 2012);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub level: Level,
    pub text: String,
}

fn error(out: &mut Vec<Issue>, text: impl Into<String>) {
    out.push(Issue {
        level: Level::Error,
        text: text.into(),
    });
}

fn warn(out: &mut Vec<Issue>, text: impl Into<String>) {
    out.push(Issue {
        level: Level::Warn,
        text: text.into(),
    });
}

pub fn errors(issues: &[Issue]) -> usize {
    issues.iter().filter(|i| i.level == Level::Error).count()
}

pub fn is_valid(issues: &[Issue]) -> bool {
    errors(issues) == 0
}

fn name(out: &mut Vec<Issue>, what: &str, value: &str) {
    let v = value.trim();
    if v.is_empty() {
        error(out, format!("{what} is empty."));
    } else if v.chars().count() > NAME_MAX {
        error(out, format!("{what} is longer than {NAME_MAX} characters."));
    } else if v.chars().any(char::is_control) {
        error(out, format!("{what} holds a control character."));
    }
}

pub fn check(d: &Driver, roster: &Roster, game: Option<&GameData>) -> Vec<Issue> {
    let mut out = Vec::new();
    name(&mut out, "First name", &d.first);
    name(&mut out, "Last name", &d.last);

    let others = roster
        .drivers
        .iter()
        .filter(|o| o.stat_id != d.stat_id || d.is_new());
    let others: Vec<&Driver> = others.collect();

    if d.code.len() != 3 || !d.code.bytes().all(|b| b.is_ascii_uppercase()) {
        error(&mut out, "Code must be 3 letters, A to Z.");
    } else {
        let earlier = others
            .iter()
            .find(|o| o.code == d.code && (d.is_new() || o.stat_id < d.stat_id));
        if let Some(o) = earlier {
            error(
                &mut out,
                format!("Code {} is already used by {}.", d.code, o.display_name()),
            );
        }
        if let Some(s) = game.and_then(|g| {
            g.stock
                .iter()
                .find(|s| s.code == d.code && s.label != "default_driver")
        }) {
            warn(&mut out, format!("Code {} is also used by {}.", d.code, s.name));
        }
    }

    if !(1..=99).contains(&d.number) {
        error(&mut out, "Number must be from 1 to 99.");
    } else {
        let mut clash: Vec<String> = others
            .iter()
            .filter(|o| o.number == d.number)
            .map(|o| o.display_name())
            .collect();
        if let Some(g) = game {
            clash.extend(
                g.stock
                    .iter()
                    .filter(|s| s.number == d.number && s.label != "default_driver")
                    .map(|s| s.name.clone()),
            );
        }
        if !clash.is_empty() {
            let shown = clash.len().min(3);
            let more = if clash.len() > shown {
                format!(" and {} more", clash.len() - shown)
            } else {
                String::new()
            };
            warn(
                &mut out,
                format!(
                    "Number {} is also used by {}{more}.",
                    d.number,
                    clash[..shown].join(", ")
                ),
            );
        }
    }

    match game {
        Some(g) => {
            let picks = [
                (List::Nationality, "Nationality", &d.nationality),
                (List::Head, "Head", &d.head),
                (List::Helmet, "Helmet", &d.helmet),
                (List::Suit, "Suit", &d.suit),
                (List::Gloves, "Gloves", &d.gloves),
                (List::Pose, "Pose", &d.pose),
                (List::Emote, "Podium emote", &d.emote),
                (List::Voice, "Commentary name", &d.voice),
            ];
            for (list, what, value) in picks {
                if !g.has(list, value) {
                    error(&mut out, format!("{what} {value:?} isn't in the game."));
                } else if !g.installed(list, value) {
                    error(
                        &mut out,
                        format!(
                            "{what} {} isn't installed in this copy of the game.",
                            g.label(list, value)
                        ),
                    );
                }
            }
            if !d.is_new() && g.driver_ids.contains(&d.stat_id) {
                error(
                    &mut out,
                    format!("Driver id {} is now used by the game.", d.stat_id),
                );
            }
            if !d.is_new() && g.preset_ids.contains(&d.preset_id) {
                error(
                    &mut out,
                    format!("Look id {} is now used by the game.", d.preset_id),
                );
            }
        }
        None => error(
            &mut out,
            "The game database isn't available, so the choices can't be checked.",
        ),
    }

    for ((_, what), r) in STATS.iter().zip(d.ratings.iter()) {
        if r.start > 100 || r.peak > 100 {
            error(&mut out, format!("{what} must be from 0 to 100."));
        } else if r.peak < r.start {
            error(&mut out, format!("{what} peak is below its start."));
        }
    }

    let b = d.birth;
    if !(BIRTH_YEARS.0..=BIRTH_YEARS.1).contains(&b.year) {
        error(
            &mut out,
            format!("Birth year must be from {} to {}.", BIRTH_YEARS.0, BIRTH_YEARS.1),
        );
    }
    if !(1..=12).contains(&b.month) {
        error(&mut out, "Birth month must be January to December.");
    } else if !(1..=b.days_in_month()).contains(&b.day) {
        error(
            &mut out,
            format!("{} has no day {} in {}.", b.month_name(), b.day, b.year),
        );
    }
    for ((_, what), v) in TRAITS.iter().zip(d.traits.iter()) {
        if !(0.0..=1.0).contains(v) {
            error(&mut out, format!("{what} must be from 0 to 1."));
        }
    }

    if d.seasons > SEASONS_MAX {
        error(
            &mut out,
            format!("Seasons completed must be from 0 to {SEASONS_MAX}."),
        );
    }
    if d.garage > 1 {
        error(&mut out, "Garage slot must be 1 or 2.");
    }

    if !d.is_new() {
        if !driver_id_usable(d.stat_id) {
            error(
                &mut out,
                format!(
                    "Driver id {} is outside {FIRST_DRIVER_ID} to {LAST_DRIVER_ID}, or is 254 or an id ending in 255 in hex.",
                    d.stat_id
                ),
            );
        }
        if d.preset_id < FIRST_PRESET_ID {
            error(
                &mut out,
                format!("Look id {} is below {FIRST_PRESET_ID}.", d.preset_id),
            );
        }
        if others
            .iter()
            .any(|o| o.stat_id == d.stat_id || o.preset_id == d.preset_id)
        {
            error(&mut out, "Another driver has the same ids.");
        }
    } else if roster.drivers.len() >= MAX_DRIVERS {
        error(
            &mut out,
            format!("The list already has {MAX_DRIVERS} drivers, the most there are ids for."),
        );
    }
    out
}
