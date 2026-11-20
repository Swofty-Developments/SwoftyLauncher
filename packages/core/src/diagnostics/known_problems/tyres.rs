use std::collections::BTreeSet;

use super::Diagnosis;
use super::evidence::Evidence;

/// The log line the core writes for a car whose fitted tyre models were never
/// placed on its wheels.
pub(super) const TYRES_UNPLACED: &str = "fitted tyres were never placed on the wheels";

/// The car numbers, humans first, the core's tyre check found without placed
/// tyres: from `[tyres] car 24 (team 7, AI): fitted tyres were never placed ...`.
pub(super) fn unplaced_cars(loader: &str) -> (BTreeSet<u32>, BTreeSet<u32>) {
    let (mut human, mut ai) = (BTreeSet::new(), BTreeSet::new());
    for line in loader.lines().filter(|l| l.contains(TYRES_UNPLACED)) {
        let Some(rest) = line.split_once("[tyres] car ").map(|(_, r)| r) else {
            continue;
        };
        let Some((number, rest)) = rest.split_once(' ') else {
            continue;
        };
        let Ok(number) = number.parse::<u32>() else {
            continue;
        };
        if rest
            .split_once("):")
            .is_some_and(|(who, _)| who.ends_with(", human"))
        {
            human.insert(number);
        } else {
            ai.insert(number);
        }
    }
    (human, ai)
}

pub(super) fn car_numbers(cars: &BTreeSet<u32>) -> String {
    cars.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
}

/// A session whose cars drove without tyres: the core's tyre check logged the
/// cars whose fitted tyre models were never placed on their wheels.
pub fn unplaced_tyres(e: &Evidence) -> Option<Diagnosis> {
    let (human, ai) = unplaced_cars(&e.loader);
    if human.is_empty() && ai.is_empty() {
        return None;
    }
    let who = match (human.is_empty(), ai.is_empty()) {
        (false, true) => format!("your car (car {})", car_numbers(&human)),
        (false, false) => format!(
            "your car (car {}) and {} other car(s)",
            car_numbers(&human),
            ai.len()
        ),
        _ => format!("{} car(s) (cars {})", ai.len(), car_numbers(&ai)),
    };
    Some(Diagnosis {
        id: "tyres_not_placed",
        title: "Cars drove without any tyres".to_owned(),
        what: format!(
            "In this session the tyres of {who} were never put on the wheels, so they were invisible. The \
             car is not damaged and drives normally. This happens when an addon's car or handling data moves \
             the wheels away from where the car's tyre models sit, or when a grid has more cars than F1 25 \
             sets tyres up for."
        ),
        steps: vec![
            "If you installed or updated a car or handling addon just before this, turn it off in your \
             profile, then press Play again."
                .to_owned(),
            "If it keeps happening with every addon off, press Send Logs so the tyre lines in this \
             session's log reach support."
                .to_owned(),
        ],
    })
}
