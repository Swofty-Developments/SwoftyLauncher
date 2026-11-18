mod cases;
mod vectors;

use serde_json::{Value, json};

use self::cases::cases;
use self::vectors::close;
use super::*;

const SLOTS: usize = 32;

fn instance(set: &[(usize, f32)]) -> ActiveInstance {
    let mut params = vec![0.0f32; SLOTS];
    for &(i, v) in set {
        params[i] = v;
    }
    ActiveInstance {
        effect: "fx".into(),
        label: String::new(),
        params,
    }
}

fn rule(name: &str, priority: i32, order: u32, when: Value, timing: [f64; 5], set: Value) -> Value {
    let [min_ms, hold_ms, for_ms, fade_in_ms, fade_out_ms] = timing;
    json!({
        "name": name, "priority": priority, "order": order,
        "event": when.get(0).is_some_and(|c| c["op"] == "event" || c["event"] == true),
        "when": when, "ramp": null,
        "min_ms": min_ms, "hold_ms": hold_ms, "for_ms": for_ms,
        "fade_in_ms": fade_in_ms, "fade_out_ms": fade_out_ms,
        "set": set,
    })
}

fn event_rule(
    name: &str,
    priority: i32,
    order: u32,
    cond: Value,
    for_ms: f64,
    fades: [f64; 2],
    set: Value,
) -> Value {
    let mut r = rule(
        name,
        priority,
        order,
        json!([cond]),
        [0.0, 0.0, for_ms, fades[0], fades[1]],
        set,
    );
    r["event"] = json!(true);
    r
}

fn blend(instance: usize, slot: usize, values: &[f32]) -> Value {
    json!({ "instance": instance, "slot": slot, "mode": "blend", "values": values })
}

fn bool_on(signal: &str) -> Value {
    json!({ "signal": signal, "op": "true" })
}

fn frame(dt_ms: f64, signals: Value) -> (f64, Value) {
    (dt_ms, signals)
}

#[test]
fn the_engine_resets_and_reports_on() {
    let c = &cases()[0];
    let rules: Vec<Rule> = c
        .rules
        .iter()
        .map(|r| serde_json::from_value(r.clone()).unwrap())
        .collect();
    let mut e = Engine::new(&rules);
    let mut s = Signals::new();
    s.set_bool("view.replay", true);
    for _ in 0..3 {
        e.step(50.0, &s);
    }
    assert!(e.is_on(0));
    assert!(e.weights()[0] > 0.0);
    e.reset();
    assert!(!e.is_on(0));
    assert_eq!(e.weights()[0], 0.0);
}

#[test]
fn rules_round_trip_through_json_with_the_contract_shape() {
    let text = r#"{"name":"kerb tracking","priority":0,"order":0,"event":false,
          "when":[{"signal":"car.surface","op":"is","values":[1],"wheels":2},{"signal":"car.speed","op":"above","value":150}],
          "ramp":null,"min_ms":40,"hold_ms":120,"for_ms":0,"fade_in_ms":60,"fade_out_ms":350,
          "set":[{"instance":0,"slot":3,"mode":"blend","values":[0.12]},{"instance":0,"slot":7,"mode":"switch","values":[1]},{"instance":1,"enabled":false}]}"#;
    let r: Rule = serde_json::from_str(text).unwrap();
    assert_eq!(r.when[0].wheels, Some(2));
    assert_eq!(r.set[2].enabled, Some(false));
    let back: Value = serde_json::to_value(&r).unwrap();
    let want: Value = serde_json::from_str(text).unwrap();
    close(&back, &want, "rule");
}
