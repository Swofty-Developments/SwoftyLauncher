use serde_json::{Value, json};

use super::*;

pub(super) struct Case {
    name: &'static str,
    note: &'static str,
    pub(super) rules: Vec<Value>,
    base: Vec<ActiveInstance>,
    frames: Vec<(f64, Value)>,
}

pub(super) fn cases() -> Vec<Case> {
    let kerb2 = json!({ "signal": "car.surface", "op": "is", "values": [1], "wheels": 2 });
    let speed150 = json!({ "signal": "car.speed", "op": "above", "value": 150 });
    vec![
        Case {
            name: "min_ms_and_fades",
            note: "FORMAT: each frame lists the complete signals of that frame (no state carries over). Keys are signal ids; bool: true/false; name: the name (or its index); number: a number; car.surface: 4 names (FL FR RL RR); event: the list of this frame's occurrence magnitudes ([] none, [0] one without magnitude). A missing key or null is a missing signal. expect.weights = each rule's fade state w after the frame; expect.options = every instance's 32 params after apply(); expect.off = per instance. Rule JSON is the active.json rule. Compare floats with 1e-5.",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([bool_on("view.replay")]),
                [100.0, 0.0, 0.0, 100.0, 200.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(50.0, json!({"view.replay": true})),
                frame(50.0, json!({"view.replay": true})),
                frame(50.0, json!({"view.replay": true})),
                frame(50.0, json!({"view.replay": true})),
                frame(50.0, json!({"view.replay": false})),
                frame(100.0, json!({"view.replay": false})),
                frame(100.0, json!({"view.replay": false})),
            ],
        },
        Case {
            name: "brief_touch_under_min_ms",
            note: "A condition that clears before min_ms never turns the rule on.",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([bool_on("view.paused")]),
                [100.0, 0.0, 0.0, 100.0, 100.0],
                json!([blend(0, 3, &[2.0])]),
            )],
            base: vec![instance(&[(3, 0.5)])],
            frames: vec![
                frame(40.0, json!({"view.paused": true})),
                frame(40.0, json!({"view.paused": true})),
                frame(40.0, json!({"view.paused": false})),
                frame(40.0, json!({"view.paused": true})),
                frame(40.0, json!({})),
            ],
        },
        Case {
            name: "hold_ms",
            note: "false_for must exceed hold_ms (strictly) before the rule goes off.",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([bool_on("view.photo")]),
                [0.0, 100.0, 0.0, 0.0, 0.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(10.0, json!({"view.photo": true})),
                frame(50.0, json!({"view.photo": false})),
                frame(50.0, json!({"view.photo": false})),
                frame(10.0, json!({"view.photo": false})),
                frame(10.0, json!({"view.photo": false})),
            ],
        },
        Case {
            name: "ramp_follows_the_signal",
            note: "Ramp rules have an empty `when`; zero fades make w jump to the target.",
            rules: vec![{
                let mut x = rule(
                    "flat out",
                    0,
                    0,
                    json!([]),
                    [0.0, 0.0, 0.0, 0.0, 0.0],
                    json!([blend(0, 1, &[0.5, 1.0])]),
                );
                x["ramp"] = json!({ "signal": "car.speed", "from": 250, "to": 320 });
                x
            }],
            base: vec![instance(&[(1, 0.0), (2, 0.25)])],
            frames: vec![
                frame(16.0, json!({"car.speed": 200})),
                frame(16.0, json!({"car.speed": 267.5})),
                frame(16.0, json!({"car.speed": 285})),
                frame(16.0, json!({"car.speed": 400})),
                frame(16.0, json!({"car.speed": 100})),
            ],
        },
        Case {
            name: "event_for_ms_and_magnitude",
            note: "car.impact above 3 fires a 300 ms window; a 2.0 hit is ignored.",
            rules: vec![event_rule(
                "impact",
                0,
                0,
                json!({ "signal": "car.impact", "op": "above", "value": 3.0 }),
                300.0,
                [0.0, 200.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(100.0, json!({"car.impact": [2.0]})),
                frame(100.0, json!({"car.impact": [2.0, 3.5]})),
                frame(100.0, json!({"car.impact": []})),
                frame(100.0, json!({})),
                frame(100.0, json!({})),
                frame(100.0, json!({})),
                frame(100.0, json!({})),
            ],
        },
        Case {
            name: "event_without_magnitude",
            note: "A plain event rule fires on any occurrence, each occurrence restarts the window.",
            rules: vec![event_rule(
                "lap",
                0,
                0,
                json!({ "signal": "session.lap", "op": "event" }),
                150.0,
                [0.0, 0.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(100.0, json!({"session.lap": [0]})),
                frame(100.0, json!({})),
                frame(100.0, json!({"session.lap": [0]})),
                frame(100.0, json!({})),
                frame(100.0, json!({})),
            ],
        },
        Case {
            name: "all_conditions",
            note: "Every entry must hold; a missing car.speed fails its entry.",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([kerb2.clone(), speed150.clone()]),
                [0.0, 0.0, 0.0, 0.0, 0.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(
                    16.0,
                    json!({"car.surface": ["kerb","kerb","tarmac","tarmac"], "car.speed": 100}),
                ),
                frame(
                    16.0,
                    json!({"car.surface": ["kerb","kerb","tarmac","tarmac"], "car.speed": 200}),
                ),
                frame(
                    16.0,
                    json!({"car.surface": ["kerb","tarmac","tarmac","tarmac"], "car.speed": 200}),
                ),
                frame(16.0, json!({"car.surface": ["kerb","kerb","tarmac","tarmac"]})),
                frame(
                    16.0,
                    json!({"car.surface": ["tarmac","tarmac","kerb","kerb"], "car.speed": 151}),
                ),
            ],
        },
        Case {
            name: "surface_wheels",
            note: "wheels = 3 over [kerb, rumble]: at least 3 wheels on either surface.",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([{ "signal": "car.surface", "op": "is", "values": [1, 2], "wheels": 3 }]),
                [0.0, 0.0, 0.0, 0.0, 0.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(16.0, json!({"car.surface": ["kerb","rumble","grass","grass"]})),
                frame(16.0, json!({"car.surface": ["kerb","rumble","kerb","grass"]})),
                frame(16.0, json!({"car.surface": [1, 2, 2, 1]})),
                frame(16.0, json!({})),
            ],
        },
        Case {
            name: "camera_name_list",
            note: "A name signal given as a name; `is` holds when it is any listed index (cockpit 0, tcam 1).",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([{ "signal": "view.camera", "op": "is", "values": [0, 1] }]),
                [0.0, 0.0, 0.0, 0.0, 0.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(16.0, json!({"view.camera": "cockpit"})),
                frame(16.0, json!({"view.camera": "chase_far"})),
                frame(16.0, json!({"view.camera": 1})),
            ],
        },
        Case {
            name: "priority_orders_a_shared_slot",
            note: "Applied by (priority, order) ascending, so the highest priority ends on top; equal priority: later in the file wins.",
            rules: vec![
                rule(
                    "hi",
                    10,
                    0,
                    json!([bool_on("view.replay")]),
                    [0.0, 0.0, 0.0, 0.0, 0.0],
                    json!([blend(0, 0, &[0.9]), blend(0, 4, &[0.1])]),
                ),
                rule(
                    "lo",
                    0,
                    1,
                    json!([bool_on("view.replay")]),
                    [0.0, 0.0, 0.0, 100.0, 0.0],
                    json!([blend(0, 0, &[0.2])]),
                ),
                rule(
                    "lo2",
                    0,
                    2,
                    json!([bool_on("view.replay")]),
                    [0.0, 0.0, 0.0, 0.0, 0.0],
                    json!([blend(0, 4, &[0.7])]),
                ),
            ],
            base: vec![instance(&[(0, 0.5), (4, 0.5)])],
            frames: vec![
                frame(50.0, json!({"view.replay": true})),
                frame(50.0, json!({"view.replay": true})),
                frame(50.0, json!({"view.replay": false})),
            ],
        },
        Case {
            name: "switch_at_half_and_enabled",
            note: "switch values and enabled=false apply once the smoothed strength a is at least 0.5 (w = 0.5).",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([bool_on("view.menu")]),
                [0.0, 0.0, 0.0, 100.0, 100.0],
                json!([
                    { "instance": 0, "slot": 5, "mode": "switch", "values": [1.0] },
                    { "instance": 1, "enabled": false },
                ]),
            )],
            base: vec![instance(&[]), instance(&[])],
            frames: vec![
                frame(25.0, json!({"view.menu": true})),
                frame(25.0, json!({"view.menu": true})),
                frame(25.0, json!({"view.menu": true})),
                frame(25.0, json!({"view.menu": true})),
                frame(25.0, json!({"view.menu": false})),
                frame(25.0, json!({"view.menu": false})),
                frame(25.0, json!({"view.menu": false})),
            ],
        },
        Case {
            name: "round_mode",
            note: "round: blend then round to a whole number; base 4 to 8 at a = 0.15625 is 4.625, rounds to 5.",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([bool_on("view.cutscene")]),
                [0.0, 0.0, 0.0, 100.0, 100.0],
                json!([{ "instance": 0, "slot": 6, "mode": "round", "values": [8.0] }]),
            )],
            base: vec![instance(&[(6, 4.0)])],
            frames: vec![
                frame(25.0, json!({"view.cutscene": true})),
                frame(25.0, json!({"view.cutscene": true})),
                frame(25.0, json!({"view.cutscene": true})),
                frame(25.0, json!({"view.cutscene": true})),
            ],
        },
        Case {
            name: "dt_clamp",
            note: "dt is clamped to 0-100 ms: a 1000 ms frame moves a 1000 ms fade by 0.1, a negative dt moves nothing.",
            rules: vec![rule(
                "a",
                0,
                0,
                json!([bool_on("view.replay")]),
                [0.0, 0.0, 0.0, 1000.0, 1000.0],
                json!([blend(0, 0, &[1.0])]),
            )],
            base: vec![instance(&[])],
            frames: vec![
                frame(1000.0, json!({"view.replay": true})),
                frame(-50.0, json!({"view.replay": true})),
                frame(100.0, json!({"view.replay": true})),
                frame(5000.0, json!({"view.replay": false})),
            ],
        },
        Case {
            name: "missing_signals",
            note: "Conditions on a missing signal are false; a ramp on a missing signal targets 0; events never occur.",
            rules: vec![
                rule(
                    "a",
                    0,
                    0,
                    json!([speed150]),
                    [0.0, 0.0, 0.0, 0.0, 0.0],
                    json!([blend(0, 0, &[1.0])]),
                ),
                {
                    let mut x = rule(
                        "b",
                        0,
                        1,
                        json!([]),
                        [0.0, 0.0, 0.0, 0.0, 0.0],
                        json!([blend(0, 1, &[1.0])]),
                    );
                    x["ramp"] = json!({ "signal": "car.rpm", "from": 0.5, "to": 1.0 });
                    x
                },
                event_rule(
                    "c",
                    0,
                    2,
                    json!({ "signal": "car.damage", "op": "event" }),
                    200.0,
                    [0.0, 0.0],
                    json!([blend(0, 2, &[1.0])]),
                ),
            ],
            base: vec![instance(&[])],
            frames: vec![
                frame(
                    16.0,
                    json!({"car.speed": 200, "car.rpm": 1.0, "car.damage": [0.2]}),
                ),
                frame(16.0, json!({"car.speed": null})),
                frame(16.0, json!({})),
            ],
        },
    ]
}

fn round(x: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (x * k).round() / k
}

fn run_case(c: &Case) -> Vec<Value> {
    let rules: Vec<Rule> = c
        .rules
        .iter()
        .map(|r| serde_json::from_value(r.clone()).unwrap())
        .collect();
    let mut engine = Engine::new(&rules);
    c.frames
        .iter()
        .map(|(dt, signals)| {
            engine.step(*dt, &Signals::from_json(signals).unwrap());
            let (out, off) = engine.apply(&c.base);
            json!({
                "weights": engine.weights().iter().map(|w| round(*w, 9)).collect::<Vec<_>>(),
                "options": out.iter().map(|i| i.params.iter().map(|p| round(f64::from(*p), 6)).collect::<Vec<_>>()).collect::<Vec<_>>(),
                "off": off,
            })
        })
        .collect()
}

fn line(v: &Value) -> String {
    serde_json::to_string(v).unwrap()
}

pub(super) fn render(cases: &[Case]) -> String {
    let mut out = String::from(
        "{\n  \"format\": \"Shader rule vectors. Per case: build an engine from `rules`, then for each frame step(dt_ms, signals) and apply(base); compare weights, options (32 floats per instance) and off. See the first case's note for the signals shape.\",\n  \"cases\": [\n",
    );
    for (n, c) in cases.iter().enumerate() {
        let expects = run_case(c);
        out.push_str("    {\n");
        out.push_str(&format!("      \"name\": {},\n", line(&json!(c.name))));
        out.push_str(&format!("      \"note\": {},\n", line(&json!(c.note))));
        out.push_str("      \"rules\": [\n");
        let rules: Vec<String> = c.rules.iter().map(|r| format!("        {}", line(r))).collect();
        out.push_str(&rules.join(",\n"));
        out.push_str("\n      ],\n");
        let base: Vec<Value> = c.base.iter().map(|b| json!(b.params)).collect();
        out.push_str(&format!("      \"base\": {},\n", line(&json!(base))));
        out.push_str("      \"frames\": [\n");
        let frames: Vec<String> = c
            .frames
            .iter()
            .zip(&expects)
            .map(|((dt, signals), expect)| {
                format!(
                    "        {}",
                    line(&json!({ "dt_ms": dt, "signals": signals, "expect": expect }))
                )
            })
            .collect();
        out.push_str(&frames.join(",\n"));
        out.push_str("\n      ]\n    }");
        out.push_str(if n + 1 < cases.len() { ",\n" } else { "\n" });
    }
    out.push_str("  ]\n}\n");
    out
}
