//! Seven optional Native Chart appearance additions, with a legacy fallback.
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use swofty_addon::helper::{VIEW_CHART, VIEW_CHART_OPTIONS};
use swofty_addon::view::*;

fn main() {
    let mut charts = false;
    let mut options = false;
    let mut alternate = false;
    let mut out = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let Ok(call) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = call.get("id") else {
            continue;
        };
        let result = match call["method"].as_str() {
            Some("hello") => {
                let has = |name: &str| {
                    call["params"]["capabilities"]
                        .as_array()
                        .is_some_and(|c| c.iter().any(|v| v.as_str() == Some(name)))
                };
                charts = has(VIEW_CHART);
                options = has(VIEW_CHART_OPTIONS);
                json!({"api":1})
            }
            Some("view") => {
                if charts {
                    let mut data: Vec<Chart> = serde_json::from_str(include_str!(
                        "../../../tools/native-charts/fixtures/options.json"
                    ))
                    .unwrap();
                    for chart in &mut data {
                        if !options {
                            chart.options = None;
                            chart.images.clear();
                            for s in &mut chart.series {
                                s.line_width = None;
                                s.blocks = None;
                                if let ChartData::Ranges { cells } | ChartData::Heatmap { cells } =
                                    &mut s.data
                                {
                                    for cell in cells {
                                        cell.style = None;
                                    }
                                }
                            }
                        } else if alternate && let Some(o) = &mut chart.options {
                            o.text.title = Some(28.);
                        }
                    }
                    let blocks: Vec<_> = data.into_iter().map(Block::Chart).collect();
                    json!({"subtitle":"Seven Native Chart options","actions":[{"id":"change-style","label":"Change sample style"}],"body":{"blocks":blocks}})
                } else {
                    json!({"subtitle":"Native Chart options","body":{"blocks":[{"text":{"text":"This launcher does not advertise native chart support."}}]}})
                }
            }
            Some("event") => {
                alternate = !alternate;
                json!("reload")
            }
            _ => Value::Null,
        };
        if writeln!(out, "{}", json!({"id":id,"result":result}))
            .and_then(|()| out.flush())
            .is_err()
        {
            break;
        }
    }
}
