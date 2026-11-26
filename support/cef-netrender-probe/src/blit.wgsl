// Full-target triangle sampling one texture 1:1. Used twice, never with a CPU copy:
// - present: the composed frame onto the window surface, blending disabled;
// - compose: the imported CEF texture over the cleared background with
//   premultiplied-alpha blending (CEF OSR output is premultiplied BGRA).
// The sampler uses nearest filtering and the viewport equals the source size, so texels are
// not resampled.

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(source, source_sampler, in.uv);
}
