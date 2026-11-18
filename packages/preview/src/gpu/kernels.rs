pub(super) const KERNELS: &str = "
@group(0) @binding(0) var up_src: texture_2d<f32>;
@group(0) @binding(1) var up_dst: texture_storage_2d<rgba16float, write>;
@group(0) @binding(2) var down_src: texture_2d<f32>;
@group(0) @binding(3) var down_dst: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(8, 8)
fn up(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = textureDimensions(up_src);
    if (id.x >= d.x || id.y >= d.y) { return; }
    textureStore(up_dst, vec2<i32>(id.xy), textureLoad(up_src, vec2<i32>(id.xy), 0));
}

@group(0) @binding(6) var mip_src: texture_2d<f32>;
@group(0) @binding(7) var mip_dst: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn mip(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = textureDimensions(mip_dst);
    if (id.x >= d.x || id.y >= d.y) { return; }
    let m = vec2<i32>(textureDimensions(mip_src)) - vec2<i32>(1);
    let b = vec2<i32>(id.xy) * 2;
    let sum = textureLoad(mip_src, min(b, m), 0)
        + textureLoad(mip_src, min(b + vec2<i32>(1, 0), m), 0)
        + textureLoad(mip_src, min(b + vec2<i32>(0, 1), m), 0)
        + textureLoad(mip_src, min(b + vec2<i32>(1, 1), m), 0);
    textureStore(mip_dst, vec2<i32>(id.xy), sum * 0.25);
}

@group(0) @binding(4) var narrow_src: texture_2d<f32>;
@group(0) @binding(5) var narrow_dst: texture_storage_2d<rgba16float, write>;

fn narrow_in(id: vec3<u32>) -> bool {
    let d = textureDimensions(narrow_src);
    return id.x < d.x && id.y < d.y;
}

fn narrow_at(id: vec3<u32>) -> vec4<f32> {
    return textureLoad(narrow_src, vec2<i32>(id.xy), 0);
}

@compute @workgroup_size(8, 8)
fn as_rgba8(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!narrow_in(id)) { return; }
    let c = clamp(narrow_at(id), vec4<f32>(0.0), vec4<f32>(1.0));
    textureStore(narrow_dst, vec2<i32>(id.xy), round(c * 255.0) / 255.0);
}

@compute @workgroup_size(8, 8)
fn as_rgb10a2(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!narrow_in(id)) { return; }
    let c = clamp(narrow_at(id), vec4<f32>(0.0), vec4<f32>(1.0));
    textureStore(narrow_dst, vec2<i32>(id.xy), vec4<f32>(round(c.rgb * 1023.0) / 1023.0, round(c.a * 3.0) / 3.0));
}

@compute @workgroup_size(8, 8)
fn as_rg(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!narrow_in(id)) { return; }
    let c = narrow_at(id);
    textureStore(narrow_dst, vec2<i32>(id.xy), vec4<f32>(c.r, c.g, 0.0, 1.0));
}

@compute @workgroup_size(8, 8)
fn as_r(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!narrow_in(id)) { return; }
    textureStore(narrow_dst, vec2<i32>(id.xy), vec4<f32>(narrow_at(id).r, 0.0, 0.0, 1.0));
}

@compute @workgroup_size(8, 8)
fn as_r11g11b10f(@builtin(global_invocation_id) id: vec3<u32>) {
    if (!narrow_in(id)) { return; }
    textureStore(narrow_dst, vec2<i32>(id.xy), vec4<f32>(max(narrow_at(id).rgb, vec3<f32>(0.0)), 1.0));
}

@compute @workgroup_size(8, 8)
fn down(@builtin(global_invocation_id) id: vec3<u32>) {
    let d = textureDimensions(down_src);
    if (id.x >= d.x || id.y >= d.y) { return; }
    let c = textureLoad(down_src, vec2<i32>(id.xy), 0);
    textureStore(down_dst, vec2<i32>(id.xy), clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)));
}
";
