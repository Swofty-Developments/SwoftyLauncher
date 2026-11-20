use std::time::Instant;
use swofty_core::saves::bag::{self, Value};
use swofty_core::saves::codec;

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Bag(_) => "pbag",
        Value::List(_) => "plst",
        _ => "value",
    }
}

fn main() {
    let mut failed = 0;
    for path in std::env::args().skip(1) {
        let file = std::fs::read(&path).unwrap();
        let t = Instant::now();
        let data = match codec::decode_data(&file) {
            Ok(d) => d,
            Err(e) => {
                println!("{path}: codec FAILED {e:?}");
                failed += 1;
                continue;
            }
        };
        let tree = match bag::decode(&data.stream) {
            Ok(b) => b,
            Err(e) => {
                println!("{path}: bag decode FAILED {e:?}");
                failed += 1;
                continue;
            }
        };
        let decoded_ms = t.elapsed().as_millis();
        let again = bag::encode(&tree).unwrap();
        let same = again == data.stream;
        let json = bag::bag_to_json(&data.stream).unwrap();
        let back = bag::bag_from_json(&json).unwrap();
        let json_same = back == data.stream;
        if !same || !json_same {
            failed += 1;
        }
        println!(
            "{path}: file {} B, stream {} B, decode {decoded_ms} ms, re-encode identical {same}, json round trip {json_same} (json {} B)",
            file.len(),
            data.stream.len(),
            json.len()
        );
        for e in &tree.entries {
            println!(
                "  root {} ({})",
                String::from_utf8_lossy(&e.key.0),
                kind(&e.value)
            );
            if let Value::Bag(b) = &e.value {
                for c in &b.entries {
                    let n = match &c.value {
                        Value::Bag(x) => x.entries.len(),
                        Value::List(x) => x.items.len(),
                        _ => 0,
                    };
                    println!(
                        "    {} ({}, {n} children)",
                        String::from_utf8_lossy(&c.key.0),
                        kind(&c.value)
                    );
                    if std::env::var_os("SAVE_LIST").is_some()
                        && let Value::Bag(x) = &c.value
                    {
                        let names: Vec<String> = x
                            .entries
                            .iter()
                            .map(|g| String::from_utf8_lossy(&g.key.0).into_owned())
                            .collect();
                        println!("      {}", names.join(", "));
                    }
                }
            }
        }
    }
    std::process::exit(failed);
}
