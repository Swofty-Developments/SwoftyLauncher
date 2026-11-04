//! Resolved shader events: the `rules` of `active.json` and the engine that evaluates them each
//! presented frame. The game core runs the same algorithm (`postfx::RuleEngine`) against the same
//! vectors (`tests/rules_vectors.json`).

use serde::{Deserialize, Serialize};

use crate::ActiveInstance;
use crate::signals::{SIGNALS, SignalKind, signal};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    True,
    Is,
    Above,
    Below,
    Between,
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Blend,
    Round,
    Switch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleCond {
    pub signal: String,
    pub op: Op,
    /// `is`: indices into the signal's names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<u32>,
    /// `is` on car.surface: wheels that must match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wheels: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lo: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hi: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleRamp {
    pub signal: String,
    pub from: f64,
    pub to: f64,
}

/// One override: a param's packed values for `blend`, `round` and `switch`, or `enabled: false`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Override {
    pub instance: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub values: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

fn fade_in_default() -> f64 {
    150.0
}

fn fade_out_default() -> f64 {
    300.0
}

fn for_default() -> f64 {
    400.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub order: u32,
    #[serde(default)]
    pub event: bool,
    #[serde(default)]
    pub when: Vec<RuleCond>,
    #[serde(default)]
    pub ramp: Option<RuleRamp>,
    #[serde(default)]
    pub min_ms: f64,
    #[serde(default)]
    pub hold_ms: f64,
    #[serde(default = "for_default")]
    pub for_ms: f64,
    #[serde(default = "fade_in_default")]
    pub fade_in_ms: f64,
    #[serde(default = "fade_out_default")]
    pub fade_out_ms: f64,
    #[serde(default)]
    pub set: Vec<Override>,
}

/// What one signal reads this frame.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Sample {
    /// Not read, or not available right now: conditions on it fail and its events never occur.
    #[default]
    Missing,
    Bool(bool),
    Name(u32),
    /// car.surface: one name index per wheel (FL FR RL RR).
    Surface([u32; 4]),
    Number(f64),
    /// An event's occurrences this frame, each with its magnitude (0 when it has none).
    Events(Vec<f64>),
}

/// The signal values of one frame, indexed like [`SIGNALS`]. Everything starts missing.
#[derive(Debug, Clone, PartialEq)]
pub struct Signals {
    samples: Vec<Sample>,
}

impl Default for Signals {
    fn default() -> Self {
        Self::new()
    }
}

fn name_index(id: &str, v: &serde_json::Value) -> Result<u32, String> {
    let sig = signal(id).ok_or_else(|| format!("unknown signal {id}"))?.1;
    match v {
        serde_json::Value::String(s) => sig
            .values
            .iter()
            .position(|n| n == s)
            .map(|i| i as u32)
            .ok_or_else(|| format!("{id}: no value {s}")),
        serde_json::Value::Number(n) => n
            .as_u64()
            .map(|i| i as u32)
            .ok_or_else(|| format!("{id}: bad index")),
        _ => Err(format!("{id}: expected a name")),
    }
}

impl Signals {
    pub fn new() -> Self {
        Self {
            samples: vec![Sample::Missing; SIGNALS.len()],
        }
    }

    pub fn set(&mut self, id: &str, sample: Sample) {
        if let Some((i, _)) = signal(id) {
            self.samples[i] = sample;
        }
    }

    pub fn set_bool(&mut self, id: &str, on: bool) {
        self.set(id, Sample::Bool(on));
    }

    pub fn set_number(&mut self, id: &str, x: f64) {
        self.set(id, Sample::Number(x));
    }

    /// A name signal by value, such as `("view.camera", "cockpit")`.
    pub fn set_name(&mut self, id: &str, value: &str) {
        if let Some((i, sig)) = signal(id)
            && let Some(v) = sig.values.iter().position(|n| *n == value)
        {
            self.samples[i] = Sample::Name(v as u32);
        }
    }

    /// car.surface, one name per wheel (FL FR RL RR).
    pub fn set_surface(&mut self, wheels: [&str; 4]) {
        let Some((i, sig)) = signal("car.surface") else {
            return;
        };
        let idx = wheels.map(|w| sig.values.iter().position(|n| *n == w).unwrap_or(0) as u32);
        self.samples[i] = Sample::Surface(idx);
    }

    /// Adds one occurrence of an event this frame.
    pub fn fire(&mut self, id: &str, magnitude: f64) {
        if let Some((i, _)) = signal(id) {
            match &mut self.samples[i] {
                Sample::Events(list) => list.push(magnitude),
                other => *other = Sample::Events(vec![magnitude]),
            }
        }
    }

    pub fn get(&self, index: usize) -> &Sample {
        &self.samples[index]
    }

    /// The vector files' shape: an object of signal id to value. A bool signal is `true`/`false`, a name
    /// signal its name (or index), a number signal a number, car.surface four names, an event the list
    /// of its occurrences' magnitudes (`[]` none, `[0]` one without magnitude). A missing key or `null`
    /// is a missing signal.
    pub fn from_json(v: &serde_json::Value) -> Result<Self, String> {
        let mut out = Self::new();
        let Some(map) = v.as_object() else {
            return Err("signals must be an object".into());
        };
        for (id, val) in map {
            let (i, sig) = signal(id).ok_or_else(|| format!("unknown signal {id}"))?;
            if val.is_null() {
                continue;
            }
            out.samples[i] = match sig.kind {
                SignalKind::Bool => match val {
                    serde_json::Value::Bool(b) => Sample::Bool(*b),
                    serde_json::Value::Number(n) => Sample::Bool(n.as_f64().unwrap_or(0.0) != 0.0),
                    _ => return Err(format!("{id}: expected true or false")),
                },
                SignalKind::Name if sig.per_wheel() => {
                    let list = val.as_array().filter(|a| a.len() == 4);
                    let list = list.ok_or_else(|| format!("{id}: expected 4 wheel names"))?;
                    let mut w = [0u32; 4];
                    for (slot, item) in w.iter_mut().zip(list) {
                        *slot = name_index(id, item)?;
                    }
                    Sample::Surface(w)
                }
                SignalKind::Name => Sample::Name(name_index(id, val)?),
                SignalKind::Number => {
                    Sample::Number(val.as_f64().ok_or_else(|| format!("{id}: expected a number"))?)
                }
                SignalKind::Event => match val {
                    serde_json::Value::Array(a) => Sample::Events(
                        a.iter()
                            .map(|m| m.as_f64().ok_or_else(|| format!("{id}: expected magnitudes")))
                            .collect::<Result<_, _>>()?,
                    ),
                    serde_json::Value::Number(n) => Sample::Events(vec![n.as_f64().unwrap_or(0.0)]),
                    serde_json::Value::Bool(true) => Sample::Events(vec![0.0]),
                    serde_json::Value::Bool(false) => Sample::Events(Vec::new()),
                    _ => return Err(format!("{id}: expected a list of magnitudes")),
                },
            };
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct State {
    left: f64,
    true_for: f64,
    false_for: f64,
    on: bool,
    w: f64,
}

/// Evaluates the rules once per presented frame (contract section 4).
#[derive(Debug, Clone)]
pub struct Engine {
    rules: Vec<Rule>,
    cond_sig: Vec<Vec<Option<usize>>>,
    ramp_sig: Vec<Option<usize>>,
    sorted: Vec<usize>,
    state: Vec<State>,
    weights: Vec<f64>,
}

fn smooth(w: f64) -> f64 {
    w * w * (3.0 - 2.0 * w)
}

fn magnitude_ok(c: &RuleCond, m: f64) -> bool {
    match c.op {
        Op::Above => m > c.value.unwrap_or(f64::INFINITY),
        Op::Below => m < c.value.unwrap_or(f64::NEG_INFINITY),
        Op::Between => m > c.lo.unwrap_or(f64::INFINITY) && m < c.hi.unwrap_or(f64::NEG_INFINITY),
        _ => true,
    }
}

fn holds(c: &RuleCond, sample: &Sample) -> bool {
    match (c.op, sample) {
        (Op::True, Sample::Bool(b)) => *b,
        (Op::Is, Sample::Name(n)) => c.values.contains(n),
        (Op::Is, Sample::Surface(w)) => {
            let hits = w.iter().filter(|n| c.values.contains(n)).count();
            hits >= usize::from(c.wheels.unwrap_or(1))
        }
        (Op::Above | Op::Below | Op::Between, Sample::Number(x)) => magnitude_ok(c, *x),
        _ => false,
    }
}

impl Engine {
    pub fn new(rules: &[Rule]) -> Self {
        let index = |id: &str| signal(id).map(|(i, _)| i);
        let mut sorted: Vec<usize> = (0..rules.len()).collect();
        sorted.sort_by_key(|&i| (rules[i].priority, rules[i].order));
        Self {
            cond_sig: rules
                .iter()
                .map(|r| r.when.iter().map(|c| index(&c.signal)).collect())
                .collect(),
            ramp_sig: rules
                .iter()
                .map(|r| r.ramp.as_ref().and_then(|ramp| index(&ramp.signal)))
                .collect(),
            sorted,
            state: vec![State::default(); rules.len()],
            weights: vec![0.0; rules.len()],
            rules: rules.to_vec(),
        }
    }

    /// Every rule off, w = 0: what a new `active.json` or a Preview jump does.
    pub fn reset(&mut self) {
        self.state.fill(State::default());
        self.weights.fill(0.0);
    }

    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Advances every rule by one frame of `dt_ms` (clamped to 0-100).
    pub fn step(&mut self, dt_ms: f64, signals: &Signals) {
        let dt = if dt_ms.is_nan() {
            0.0
        } else {
            dt_ms.clamp(0.0, 100.0)
        };
        for (i, r) in self.rules.iter().enumerate() {
            let st = &mut self.state[i];
            let sample = |k: usize| -> &Sample {
                static MISSING: Sample = Sample::Missing;
                match self.cond_sig[i].get(k).copied().flatten() {
                    Some(s) => signals.get(s),
                    None => &MISSING,
                }
            };
            let target;
            if r.event {
                let passes = match (r.when.first(), sample(0)) {
                    (Some(c), Sample::Events(list)) => list.iter().any(|m| magnitude_ok(c, *m)),
                    _ => false,
                };
                st.left = if passes { r.for_ms } else { (st.left - dt).max(0.0) };
                st.on = st.left > 0.0;
                target = if st.on { 1.0 } else { 0.0 };
            } else if let Some(ramp) = &r.ramp {
                let x = match self.ramp_sig[i].map(|s| signals.get(s)) {
                    Some(Sample::Number(x)) => Some(*x),
                    _ => None,
                };
                target = x.map_or(0.0, |x| ((x - ramp.from) / (ramp.to - ramp.from)).clamp(0.0, 1.0));
                st.on = target > 0.0;
            } else {
                let cond = r.when.iter().enumerate().all(|(k, c)| holds(c, sample(k)));
                if cond {
                    st.true_for += dt;
                    st.false_for = 0.0;
                } else {
                    st.true_for = 0.0;
                    st.false_for += dt;
                }
                if !st.on && cond && st.true_for >= r.min_ms {
                    st.on = true;
                }
                if st.on && !cond && st.false_for > r.hold_ms {
                    st.on = false;
                }
                target = if st.on { 1.0 } else { 0.0 };
            }
            if target > st.w {
                st.w = if r.fade_in_ms <= 0.0 {
                    target
                } else {
                    (st.w + dt / r.fade_in_ms).min(target)
                };
            } else if target < st.w {
                st.w = if r.fade_out_ms <= 0.0 {
                    target
                } else {
                    (st.w - dt / r.fade_out_ms).max(target)
                };
            }
            self.weights[i] = st.w;
        }
    }

    /// Each rule's fade state w (0-1), in rule order.
    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    /// Whether each rule is on (what the core logs transitions of), in rule order.
    pub fn is_on(&self, rule: usize) -> bool {
        self.state.get(rule).is_some_and(|s| s.on)
    }

    /// The base instances with every active rule's overrides applied, and which instances are off.
    pub fn apply(&self, base: &[ActiveInstance]) -> (Vec<ActiveInstance>, Vec<bool>) {
        let mut options: Vec<Vec<f64>> = base
            .iter()
            .map(|b| b.params.iter().map(|&p| f64::from(p)).collect())
            .collect();
        let mut off = vec![false; base.len()];
        for &ri in &self.sorted {
            let a = smooth(self.weights[ri]);
            if a <= 0.0 {
                continue;
            }
            for o in &self.rules[ri].set {
                let Some(params) = options.get_mut(o.instance) else {
                    continue;
                };
                if o.enabled == Some(false) {
                    if a >= 0.5 {
                        off[o.instance] = true;
                    }
                    continue;
                }
                let (Some(slot), Some(mode)) = (o.slot, o.mode) else {
                    continue;
                };
                for (k, &t) in o.values.iter().enumerate() {
                    let Some(v) = params.get_mut(slot + k) else {
                        continue;
                    };
                    let t = f64::from(t);
                    match mode {
                        Mode::Blend => *v += (t - *v) * a,
                        Mode::Round => *v = (*v + (t - *v) * a).round(),
                        Mode::Switch => {
                            if a >= 0.5 {
                                *v = t;
                            }
                        }
                    }
                }
            }
        }
        let out = base
            .iter()
            .zip(options)
            .map(|(b, p)| ActiveInstance {
                effect: b.effect.clone(),
                label: b.label.clone(),
                params: p.into_iter().map(|v| v as f32).collect(),
            })
            .collect();
        (out, off)
    }
}
