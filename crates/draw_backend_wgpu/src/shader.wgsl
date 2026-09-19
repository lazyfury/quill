// Backend-neutral `DrawList` -> GPU shader.
//
// Geometry is tessellated on the CPU into triangles whose positions are already
// in normalized device coordinates (NDC). Transform, opacity and clip are
// therefore resolved before the GPU sees the vertices, so this shader stays a
// trivial textured-quad pass. `uv` addresses a 1x1 white texture for solid
// fills, a registered image, or the built-in bitmap-font atlas.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.uv = input.uv;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let texel = textureSample(u_texture, u_sampler, input.uv);
    return texel * input.color;
}
