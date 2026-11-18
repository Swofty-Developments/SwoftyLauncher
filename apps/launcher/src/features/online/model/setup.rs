//! A lap's car setup, grouped as the setup screen groups it.

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Setup {
    pub values: Vec<(&'static str, f32)>,
    pub compound: Option<String>,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Plain,
    Percent,
    Degrees,
    Psi,
    Kg,
}

pub type SetupRow = (&'static str, &'static str, Unit);

pub const SETUP_GROUPS: [(&str, &[SetupRow]); 7] = [
    (
        "Aerodynamics",
        &[
            ("front_wing", "Front wing", Unit::Plain),
            ("rear_wing", "Rear wing", Unit::Plain),
        ],
    ),
    (
        "Transmission",
        &[
            ("diff_on_throttle", "Differential on throttle", Unit::Percent),
            ("diff_off_throttle", "Differential off throttle", Unit::Percent),
            ("engine_braking", "Engine braking", Unit::Percent),
        ],
    ),
    (
        "Suspension geometry",
        &[
            ("front_camber", "Front camber", Unit::Degrees),
            ("rear_camber", "Rear camber", Unit::Degrees),
            ("front_toe", "Front toe", Unit::Degrees),
            ("rear_toe", "Rear toe", Unit::Degrees),
        ],
    ),
    (
        "Suspension",
        &[
            ("front_suspension", "Front suspension", Unit::Plain),
            ("rear_suspension", "Rear suspension", Unit::Plain),
            ("front_anti_roll_bar", "Front anti-roll bar", Unit::Plain),
            ("rear_anti_roll_bar", "Rear anti-roll bar", Unit::Plain),
            ("front_ride_height", "Front ride height", Unit::Plain),
            ("rear_ride_height", "Rear ride height", Unit::Plain),
        ],
    ),
    (
        "Brakes",
        &[
            ("brake_pressure", "Brake pressure", Unit::Percent),
            ("brake_bias", "Brake bias", Unit::Percent),
        ],
    ),
    (
        "Tyres",
        &[
            ("tyre_pressure_fl", "Front left pressure", Unit::Psi),
            ("tyre_pressure_fr", "Front right pressure", Unit::Psi),
            ("tyre_pressure_rl", "Rear left pressure", Unit::Psi),
            ("tyre_pressure_rr", "Rear right pressure", Unit::Psi),
        ],
    ),
    ("Fuel", &[("fuel_load", "Fuel load", Unit::Kg)]),
];

pub fn setup_value_text(v: f32, unit: Unit) -> String {
    let n = if (v - v.round()).abs() < 0.005 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}").trim_end_matches('0').to_owned()
    };
    match unit {
        Unit::Plain => n,
        Unit::Percent => format!("{n}%"),
        Unit::Degrees => format!("{n}\u{b0}"),
        Unit::Psi => format!("{n}\u{a0}psi"),
        Unit::Kg => format!("{n} kg"),
    }
}

impl Setup {
    pub fn get(&self, key: &str) -> Option<f32> {
        self.values.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.compound.is_none()
    }

    pub fn difference(&self, mine: &Setup, key: &str) -> Option<f32> {
        let (a, b) = (self.get(key)?, mine.get(key)?);
        ((a - b).abs() > 0.004).then_some(a - b)
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for (group, rows) in SETUP_GROUPS {
            let lines: Vec<String> = rows
                .iter()
                .filter_map(|(key, label, unit)| {
                    self.get(key)
                        .map(|v| format!("  {label}: {}", setup_value_text(v, *unit)))
                })
                .collect();
            let compound = (group == "Tyres").then(|| self.compound.clone()).flatten();
            if lines.is_empty() && compound.is_none() {
                continue;
            }
            out.push_str(group);
            out.push('\n');
            if let Some(c) = compound {
                out.push_str(&swofty_i18n::trf!("  Compound: {c}\n", c = c));
            }
            for l in lines {
                out.push_str(&l);
                out.push('\n');
            }
        }
        out
    }
}
