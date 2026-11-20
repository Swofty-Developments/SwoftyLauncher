use serde_json::{Value, json};

use super::cases::{cases, render};
use super::*;

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/rules_vectors.json");

#[test]
fn write_vectors_when_asked() {
    if std::env::var_os("SWOFTY_WRITE_VECTORS").is_some() {
        std::fs::write(FILE, render(&cases())).unwrap();
    }
}

pub(super) fn close(a: &Value, b: &Value, what: &str) {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            assert_eq!(x.len(), y.len(), "{what}");
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                close(p, q, &format!("{what}[{i}]"));
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            assert_eq!(x.len(), y.len(), "{what}");
            for (k, p) in x {
                close(p, &y[k], &format!("{what}.{k}"));
            }
        }
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            assert!((x - y).abs() < 1e-5, "{what}: {x} vs {y}");
        }
        _ => assert_eq!(a, b, "{what}"),
    }
}

fn load() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FILE).expect("rules_vectors.json")).unwrap()
}

#[test]
fn the_vector_file_matches_the_engine() {
    let file = load();
    let cases = file["cases"].as_array().unwrap();
    assert!(cases.len() >= 10);
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let rules: Vec<Rule> = serde_json::from_value(c["rules"].clone()).unwrap();
        let base: Vec<ActiveInstance> = c["base"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| ActiveInstance {
                effect: "fx".into(),
                label: String::new(),
                params: serde_json::from_value(b.clone()).unwrap(),
            })
            .collect();
        let mut engine = Engine::new(&rules);
        for (n, f) in c["frames"].as_array().unwrap().iter().enumerate() {
            let signals = Signals::from_json(&f["signals"]).unwrap();
            engine.step(f["dt_ms"].as_f64().unwrap(), &signals);
            let (out, off) = engine.apply(&base);
            let got = json!({
                "weights": engine.weights(),
                "options": out.iter().map(|i| i.params.clone()).collect::<Vec<_>>(),
                "off": off,
            });
            close(&got, &f["expect"], &format!("{name} frame {n}"));
        }
    }
}

fn frame_of<'a>(file: &'a Value, case: &str, n: usize) -> &'a Value {
    let c = file["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == case)
        .unwrap();
    &c["frames"][n]["expect"]
}

#[test]
fn hand_checked_fade_and_min_ms() {
    let f = load();
    let w = |n: usize| {
        frame_of(&f, "min_ms_and_fades", n)["weights"][0]
            .as_f64()
            .unwrap()
    };
    let o = |n: usize| {
        frame_of(&f, "min_ms_and_fades", n)["options"][0][0]
            .as_f64()
            .unwrap()
    };
    assert_eq!([w(0), w(1)], [0.0, 0.5]);
    assert_eq!(w(2), 1.0);
    assert_eq!((o(0), o(1), o(2)), (0.0, 0.5, 1.0));
    assert_eq!(w(4), 0.75);
    assert!((o(4) - 0.84375).abs() < 1e-6);
    assert_eq!(w(6), 0.0);
}

#[test]
fn hand_checked_ramp_quarter_is_smoothstep() {
    let f = load();
    let e = frame_of(&f, "ramp_follows_the_signal", 1);
    assert_eq!(e["weights"][0], 0.25);
    assert!((e["options"][0][1].as_f64().unwrap() - 0.5 * 0.15625).abs() < 1e-6);
    assert!((e["options"][0][2].as_f64().unwrap() - (0.25 + (1.0 - 0.25) * 0.15625)).abs() < 1e-6);
    assert_eq!(frame_of(&f, "ramp_follows_the_signal", 3)["weights"][0], 1.0);
}

#[test]
fn hand_checked_switch_round_and_dt_clamp() {
    let f = load();
    let sw = |n: usize| frame_of(&f, "switch_at_half_and_enabled", n);
    assert_eq!(sw(0)["options"][0][5], 0.0);
    assert_eq!(sw(0)["off"], json!([false, false]));
    assert_eq!(sw(1)["options"][0][5], 1.0);
    assert_eq!(sw(1)["off"], json!([false, true]));
    assert_eq!(sw(3)["weights"][0], 1.0);
    assert_eq!(sw(4)["weights"][0], 0.75);
    assert_eq!(sw(5)["off"], json!([false, true]));
    assert_eq!(sw(6)["off"], json!([false, false]));
    let r = frame_of(&f, "round_mode", 0);
    assert_eq!(r["options"][0][6], 5.0);
    let d = |n: usize| frame_of(&f, "dt_clamp", n)["weights"][0].as_f64().unwrap();
    assert!((d(0) - 0.1).abs() < 1e-9 && (d(1) - 0.1).abs() < 1e-9 && (d(2) - 0.2).abs() < 1e-9);
    assert!((d(3) - 0.1).abs() < 1e-9);
}
