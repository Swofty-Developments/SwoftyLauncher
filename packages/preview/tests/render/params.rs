use swofty_preview::{ActiveInstance, CompiledEffect, Preview};

use crate::{compiled, dxc, effect, near, pass, preview, solid, spirv};

#[test]
fn param_slots_reach_the_shader() {
    let Some(dxc) = dxc() else {
        eprintln!("skipping: no dxc");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let Some(mut preview) = preview(vec![compiled(&dxc, dir.path())], 0.0) else {
        return;
    };
    let a = [51, 102, 153];
    let plain = preview.render(&solid(0, 32, 32, a)).unwrap();
    preview.set_instances(vec![ActiveInstance {
        effect: "temporal".into(),
        label: String::new(),
        params: vec![0.05],
    }]);
    let biased = preview.render(&solid(0, 32, 32, a)).unwrap();
    near(&biased, [0.5 * 0.8 + 0.1 + 0.05, 0.3 + 0.05, 0.2 + 0.05]);
    assert_ne!(plain.rgba, biased.rgba);
    preview.set_instances(Vec::new());
    let none = preview.render(&solid(0, 32, 32, a)).unwrap();
    assert_eq!(none.rgba, solid(0, 32, 32, a).rgba);
}

const SLOTS: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel) { return float4(option(16), option(19), option(31), 1); }"#;

#[test]
fn all_32_option_slots_reach_the_shader() {
    let Some(dxc) = dxc() else {
        eprintln!("skipping: no dxc");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let compiled = CompiledEffect {
        effect: effect("slots", Vec::new(), vec![pass("slots", &[], None)]),
        blobs: vec![spirv(&dxc, dir.path(), "slots", SLOTS)],
        images: Vec::new(),
    };
    let mut preview = match Preview::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping: {e}");
            return;
        }
    };
    preview.set_effects(vec![compiled]);
    preview.set_instances(vec![ActiveInstance {
        effect: "slots".into(),
        label: String::new(),
        params: (0..32).map(|i| i as f32 / 40.0).collect(),
    }]);
    let out = preview.render(&solid(0, 16, 16, [0, 0, 0])).unwrap();
    near(&out, [16.0 / 40.0, 19.0 / 40.0, 31.0 / 40.0]);
}
