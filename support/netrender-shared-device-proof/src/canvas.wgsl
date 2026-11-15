// The page's canvas frame: top half opaque green, bottom half 50% blue
// (premultiplied, as a `premultiplied` canvas), drawn on the GPU. Shared by the
// raw wgpu-core producer (src/main.rs) and the V8 guest (src/guest.js).
@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    if (pos.y < 8.0) {
        return vec4<f32>(0.0, 1.0, 0.0, 1.0);
    }
    return vec4<f32>(0.0, 0.0, 0.5, 0.5);
}
