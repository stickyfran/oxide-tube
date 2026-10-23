struct VideoUniforms {
    ndc_rect: vec4<f32>, // x: left, y: top, z: right, w: bottom
};

@group(0) @binding(0) var t_video: texture_2d<f32>;
@group(0) @binding(1) var s_video: sampler;
@group(0) @binding(2) var<uniform> uniforms: VideoUniforms;

var<private> uvs: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 1.0)
);

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let uv = uvs[vertex_index];
    let left = uniforms.ndc_rect.x;
    let top = uniforms.ndc_rect.y;
    let right = uniforms.ndc_rect.z;
    let bottom = uniforms.ndc_rect.w;

    let x = mix(left, right, uv.x);
    let y = mix(top, bottom, uv.y);

    var out: VertexOutput;
    out.uv = uv;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_video, s_video, in.uv);
}

