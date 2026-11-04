//! Packing instance values into the effect's float slots.

use std::collections::BTreeMap;

use crate::lint::bounds;
use crate::rules::{Mode, Op, Override, Rule, RuleCond, RuleRamp};
use crate::signals::signal;
use crate::{ActiveInstance, Catalog, CondOp, Config, Effect, Param, ParamKind, Value};

fn clamp(v: f64, (min, max): (Option<f64>, Option<f64>)) -> f64 {
    let v = min.map_or(v, |m| v.max(m));
    max.map_or(v, |m| v.min(m))
}

fn scalar(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

fn pack_param(p: &Param, value: Option<&Value>) -> Vec<f32> {
    let range = bounds(&p.kind);
    let fits = |v: &Value| match (&p.kind, v) {
        (ParamKind::Float { .. } | ParamKind::Int { .. }, Value::Number(_)) => true,
        (ParamKind::Bool, Value::Bool(_)) => true,
        (ParamKind::Enum { values }, Value::Text(t)) => values.contains(t),
        (k, Value::Vector(v)) => k.width() > 1 && v.len() == k.width(),
        _ => false,
    };
    let value = value.filter(|v| fits(v)).unwrap_or(&p.default);
    match (&p.kind, value) {
        (ParamKind::Enum { values }, Value::Text(t)) => {
            vec![values.iter().position(|v| v == t).unwrap_or(0) as f32]
        }
        (_, Value::Vector(v)) => v.iter().map(|n| clamp(*n, range) as f32).collect(),
        (ParamKind::Bool, v) => vec![scalar(v).unwrap_or(0.0) as f32],
        (ParamKind::Int { .. }, v) => vec![clamp(scalar(v).unwrap_or(0.0), range).round() as f32],
        (_, v) => vec![clamp(scalar(v).unwrap_or(0.0), range) as f32],
    }
}

/// An effect's slot array for the given values: defaults for missing or mistyped ones, numbers
/// clamped to the param's range, bools as 0/1, enums as the value's index.
pub fn pack(effect: &Effect, values: &BTreeMap<String, Value>) -> Vec<f32> {
    let mut slots = vec![0.0f32; effect.slots];
    for p in &effect.params {
        for (i, v) in pack_param(p, values.get(&p.id)).into_iter().enumerate() {
            slots[p.slot + i] = v;
        }
    }
    slots
}

/// The enabled instances with their packed, clamped param slots.
pub fn resolve(config: &Config, catalog: &Catalog) -> Vec<ActiveInstance> {
    config
        .instances
        .iter()
        .filter(|i| i.enabled)
        .filter_map(|i| {
            let effect = catalog.get(&i.effect)?;
            Some(ActiveInstance {
                effect: i.effect.clone(),
                label: i.label.clone().unwrap_or_default(),
                params: pack(effect, &i.values),
            })
        })
        .collect()
}

/// The config's rules resolved against its enabled instances: instance indices follow
/// [`resolve`]'s order, each override is packed like an instance value.
pub fn resolve_rules(config: &Config, catalog: &Catalog) -> Vec<Rule> {
    let mut active_index = Vec::new();
    let mut next = 0;
    for i in &config.instances {
        if i.enabled && catalog.get(&i.effect).is_some() {
            active_index.push(Some(next));
            next += 1;
        } else {
            active_index.push(None);
        }
    }
    let index_of = |id: &str, name: &str| {
        signal(id)
            .and_then(|(_, s)| s.values.iter().position(|v| *v == name))
            .map(|i| i as u32)
    };
    config
        .rules
        .iter()
        .map(|r| {
            let when = r
                .when
                .iter()
                .map(|c| {
                    let mut out = RuleCond {
                        signal: c.signal.clone(),
                        op: Op::True,
                        values: Vec::new(),
                        wheels: None,
                        value: None,
                        lo: None,
                        hi: None,
                    };
                    match &c.op {
                        CondOp::True => {}
                        CondOp::Is { values, wheels } => {
                            out.op = Op::Is;
                            out.values = values.iter().filter_map(|v| index_of(&c.signal, v)).collect();
                            out.wheels = (c.signal == "car.surface").then_some(*wheels);
                        }
                        CondOp::Above(v) => (out.op, out.value) = (Op::Above, Some(*v)),
                        CondOp::Below(v) => (out.op, out.value) = (Op::Below, Some(*v)),
                        CondOp::Between(lo, hi) => {
                            (out.op, out.lo, out.hi) = (Op::Between, Some(*lo), Some(*hi))
                        }
                        CondOp::Event => out.op = Op::Event,
                    }
                    out
                })
                .collect();
            let mut set = Vec::new();
            for s in &r.sets {
                let Some(Some(instance)) = active_index.get(s.instance).copied() else {
                    continue;
                };
                let inst = &config.instances[s.instance];
                if s.off {
                    set.push(Override {
                        instance,
                        slot: None,
                        mode: None,
                        values: Vec::new(),
                        enabled: Some(false),
                    });
                }
                let Some(effect) = catalog.get(&inst.effect) else {
                    continue;
                };
                for p in &effect.params {
                    let Some(v) = s.values.get(&p.id) else { continue };
                    let mode = match p.kind {
                        ParamKind::Int { .. } => Mode::Round,
                        ParamKind::Bool | ParamKind::Enum { .. } => Mode::Switch,
                        _ => Mode::Blend,
                    };
                    set.push(Override {
                        instance,
                        slot: Some(p.slot),
                        mode: Some(mode),
                        values: pack_param(p, Some(v)),
                        enabled: None,
                    });
                }
            }
            Rule {
                name: r.name.clone().unwrap_or_else(|| format!("rule {}", r.order + 1)),
                priority: r.priority,
                order: r.order as u32,
                event: r.event,
                when,
                ramp: r.ramp.as_ref().map(|x| RuleRamp {
                    signal: x.signal.clone(),
                    from: x.from,
                    to: x.to,
                }),
                min_ms: r.min_ms,
                hold_ms: r.hold_ms,
                for_ms: r.for_ms,
                fade_in_ms: r.fade_in_ms,
                fade_out_ms: r.fade_out_ms,
                set,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::tests::{GOOD, catalog};
    use crate::{Instance, lint};

    #[test]
    fn values_pack_in_declaration_order() {
        let c = lint(GOOD, &catalog()).config.unwrap();
        let r = resolve(&c, &catalog());
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].effect, "ambient_light");
        assert_eq!(r[0].label, "");
        assert_eq!(r[0].params, vec![1.5, 1.0, 1.0, 1.0, 1.5, 0.25, 8.0, 0.0, 0.0]);
        assert_eq!(r[1].params, vec![0.2, 2.0]);
    }

    #[test]
    fn missing_values_take_defaults() {
        let c = lint("name = \"a\"\n[ambient_light]\n", &catalog())
            .config
            .unwrap();
        assert_eq!(
            resolve(&c, &catalog())[0].params,
            vec![1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 4.0, 0.0, 0.0]
        );
    }

    #[test]
    fn disabled_and_unknown_instances_are_skipped() {
        let mut c = lint(GOOD, &catalog()).config.unwrap();
        c.instances.push(Instance {
            effect: "gone".into(),
            label: Some("x".into()),
            enabled: true,
            values: BTreeMap::new(),
            span: 0..0,
        });
        c.instances[2].enabled = true;
        let r = resolve(&c, &catalog());
        assert_eq!(r.len(), 3);
        assert_eq!(r[2].label, "heavy");
        assert_eq!(r[2].params, vec![1.0, 4.0]);
    }

    #[test]
    fn packing_clamps_hand_built_values() {
        let cat = catalog();
        let e = cat.get("ambient_light").unwrap();
        let mut v = BTreeMap::new();
        v.insert("intensity".to_owned(), Value::Number(99.0));
        v.insert("taps".to_owned(), Value::Number(2.6));
        v.insert("tint".to_owned(), Value::Vector(vec![-1.0, 5.0, 1.0]));
        v.insert("adapt_black_level".to_owned(), Value::Text("high".into()));
        v.insert("lens".to_owned(), Value::Text("not a bool".into()));
        let p = pack(e, &v);
        assert_eq!(p, vec![4.0, 0.0, 2.0, 0.0, 2.0, 1.0, 3.0, 0.0, 0.0]);
    }

    #[test]
    fn the_slot_array_always_has_the_effects_length() {
        let cat = catalog();
        for e in &cat.effects {
            assert_eq!(pack(e, &BTreeMap::new()).len(), e.slots);
        }
    }
}
