use swofty_preview::{ActiveInstance, CompiledEffect, Preview};

use crate::{dxc, effect, near, pass, solid, spirv, texture};

const SHOW: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel) { return input_at(0, int2(uv * float2(input_size(0)))); }"#;

#[test]
fn image_textures_reach_the_shader_the_right_way_up() {
    let Some(dxc) = dxc() else {
        eprintln!("skipping: no dxc");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut pattern = texture("pattern", false);
    pattern.format = None;
    pattern.file = Some("pattern.png".into());
    let fx = effect("image", vec![pattern], vec![pass("show", &["pattern"], None)]);
    let rgba = [
        [255, 0, 0, 255],
        [0, 255, 0, 255],
        [0, 0, 255, 255],
        [255, 255, 255, 255],
    ]
    .concat();
    let compiled = CompiledEffect {
        effect: fx,
        blobs: vec![spirv(&dxc, dir.path(), "show", SHOW)],
        images: vec![("pattern".into(), 2, 2, rgba)],
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
        effect: "image".into(),
        label: String::new(),
        params: Vec::new(),
    }]);
    let out = preview.render(&solid(0, 64, 48, [128, 128, 128])).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * out.width + x) * 4) as usize;
        [out.rgba[i], out.rgba[i + 1], out.rgba[i + 2]]
    };
    assert_eq!(at(1, 1), [255, 0, 0], "top left");
    assert_eq!(at(62, 1), [0, 255, 0], "top right");
    assert_eq!(at(1, 46), [0, 0, 255], "bottom left");
    assert_eq!(at(62, 46), [255, 255, 255], "bottom right");
}

const FILL_R: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel) { return float4(0.25, 0.5, 0.75, 0.5); }"#;

const FILL_8: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel) { return float4(1.5, -0.5, 0.5, 1); }"#;

const SHOW_BOTH: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel)
{
    float4 r = input(0, uv), b = input(1, uv);
    return float4(r.r, r.g + r.b, b.r * 0.5, 1);
}"#;

#[test]
fn declared_formats_hold_what_the_games_textures_hold() {
    let Some(dxc) = dxc() else {
        eprintln!("skipping: no dxc");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut single = texture("single", false);
    single.format = Some("r16f".into());
    let mut bytes = texture("bytes", false);
    bytes.format = Some("rgba8".into());
    let fx = effect(
        "formats",
        vec![single, bytes],
        vec![
            pass("fill_r", &[], Some("single")),
            pass("fill_8", &[], Some("bytes")),
            pass("show_both", &["single", "bytes"], None),
        ],
    );
    let compiled = CompiledEffect {
        effect: fx,
        blobs: vec![
            spirv(&dxc, dir.path(), "fill_r", FILL_R),
            spirv(&dxc, dir.path(), "fill_8", FILL_8),
            spirv(&dxc, dir.path(), "show_both", SHOW_BOTH),
        ],
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
        effect: "formats".into(),
        label: String::new(),
        params: Vec::new(),
    }]);
    let out = preview.render(&solid(0, 32, 32, [128, 128, 128])).unwrap();
    near(&out, [0.25, 0.0, 0.5]);
}

const FILL_PATTERN: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel) { return float4(float(pixel.x + 4 * pixel.y) / 16.0, 0.5, 0, 1); }"#;

const FILL_ODD: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel) { return float4(0.5, 0.5, 0.5, 1); }"#;

const SHOW_LODS: &str = r#"#include "swofty/postfx.hlsli"
float4 effect(float2 uv, uint2 pixel)
{
    float v = uv.x < 0.5
        ? input_lod(0, uv, 2).r
        : input_lod_point(0, float2(uv.x * 2 - 1, uv.y), 1).r;
    float level0 = input(0, float2(0.125, 0.125)).r;
    float g = float(input_size_lod(0, 1).x) / 4 + float(input_size_lod(0, 2).x) / 16;
    float b = float(input_size_lod(1, 1).x) / 4 + input_lod(1, uv, 2).r * 0.25;
    return float4(v, g, b, 1) + float4(0, 0, 0, level0 * 0);
}"#;

#[test]
fn mip_chains_hold_the_box_averages() {
    let Some(dxc) = dxc() else {
        eprintln!("skipping: no dxc");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let mut lens = texture("lens", false);
    lens.width = Some(4);
    lens.height = Some(4);
    lens.mips = Some(3);
    let mut odd = texture("odd", false);
    odd.width = Some(5);
    odd.height = Some(3);
    odd.mips = Some(9);
    let fx = effect(
        "mips",
        vec![lens, odd],
        vec![
            pass("fill_pattern", &[], Some("lens")),
            pass("fill_odd", &[], Some("odd")),
            pass("show_lods", &["lens", "odd"], None),
        ],
    );
    let compiled = CompiledEffect {
        effect: fx,
        blobs: vec![
            spirv(&dxc, dir.path(), "fill_pattern", FILL_PATTERN),
            spirv(&dxc, dir.path(), "fill_odd", FILL_ODD),
            spirv(&dxc, dir.path(), "show_lods", SHOW_LODS),
        ],
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
        effect: "mips".into(),
        label: String::new(),
        params: Vec::new(),
    }]);
    let out = preview.render(&solid(0, 64, 48, [128, 128, 128])).unwrap();
    let at = |x: u32, y: u32| {
        let i = ((y * out.width + x) * 4) as usize;
        [out.rgba[i], out.rgba[i + 1], out.rgba[i + 2]].map(|v| f32::from(v) / 255.0)
    };
    let close = |got: f32, want: f32, what: &str| {
        assert!((got - want).abs() < 2.0 / 255.0, "{what}: got {got}, want {want}");
    };
    close(at(10, 10)[0], 7.5 / 16.0, "level 2 is the mean of all 16");
    close(at(40, 12)[0], 2.5 / 16.0, "level 1 texel 0,0");
    close(at(56, 12)[0], 4.5 / 16.0, "level 1 texel 1,0");
    close(at(40, 36)[0], 10.5 / 16.0, "level 1 texel 0,1");
    close(at(56, 36)[0], 12.5 / 16.0, "level 1 texel 1,1");
    let [_, g, b] = at(10, 10);
    close(g, 0.5 + 1.0 / 16.0, "level sizes of the 4x4");
    close(b, 0.5 + 0.5 / 4.0, "the 5x3 rounds down to 2 wide and holds 0.5");
}
